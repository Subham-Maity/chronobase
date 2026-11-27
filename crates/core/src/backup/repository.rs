//! Repository trait and JSON-store implementation for backup history entries.

use std::sync::Arc;

use crate::backup::{BackupEntry, BackupError, BackupSourceKind, BackupStatus};
use crate::connection::DbKind;
use crate::constant::BACKUP_HISTORY_CAP;
use crate::store::{JsonStore, local_timestamp};

/// Input used to record a new backup snapshot in history.
#[derive(Debug, Clone)]
pub struct RecordBackupInput {
    /// Source connection ID.
    pub connection_id: u64,
    /// Source connection name.
    pub connection_name: String,
    /// Source group name.
    pub group_name: String,
    /// Database engine kind.
    pub db_type: DbKind,
    /// Origin kind (`Direct` or `VpsPull`).
    pub source_kind: BackupSourceKind,
    /// Relative path under `backups/` (empty on failure).
    pub file_path: String,
    /// Size in bytes (0 on failure).
    pub file_size: u64,
    /// Backup outcome.
    pub status: BackupStatus,
    /// Error message when `status == BackupStatus::Failed`.
    pub error_msg: Option<String>,
}

/// Persistence boundary for backup history entries.
pub trait BackupRepository: Send + Sync {
    /// Lists backup entries (newest first).
    fn list(&self) -> Result<Vec<BackupEntry>, BackupError>;

    /// Finds a backup entry by ID.
    fn find_by_id(&self, id: u64) -> Result<Option<BackupEntry>, BackupError>;

    /// Inserts a new backup entry at the front and enforces the history cap
    /// by evicting the oldest `Failed` entries first (never dropping `Success` entries).
    fn record(&self, input: RecordBackupInput) -> Result<BackupEntry, BackupError>;

    /// Removes a backup entry by ID from history.
    fn delete(&self, id: u64) -> Result<(), BackupError>;
}

/// JSON-store-backed [`BackupRepository`].
pub struct StoreBackupRepository {
    store: Arc<JsonStore>,
}

impl StoreBackupRepository {
    /// Wraps an open [`JsonStore`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self { store }
    }
}

impl BackupRepository for StoreBackupRepository {
    fn list(&self) -> Result<Vec<BackupEntry>, BackupError> {
        Ok(self.store.read_with(|d| d.backups.clone()))
    }

    fn find_by_id(&self, id: u64) -> Result<Option<BackupEntry>, BackupError> {
        Ok(self
            .store
            .read_with(|d| d.backups.iter().find(|b| b.id == id).cloned()))
    }

    fn record(&self, input: RecordBackupInput) -> Result<BackupEntry, BackupError> {
        let entry = self.store.mutate_with(|d| {
            let id = d._seq.b;
            d._seq.b = d._seq.b.saturating_add(1);
            let entry = BackupEntry {
                id,
                connection_id: input.connection_id,
                connection_name: input.connection_name,
                group_name: input.group_name,
                db_type: input.db_type,
                source_kind: input.source_kind,
                file_path: input.file_path,
                file_size: input.file_size,
                status: input.status,
                error_msg: input.error_msg,
                created_at: local_timestamp(),
                missing_on_disk: false,
            };
            d.backups.insert(0, entry.clone());
            // Evict the oldest Failed entries first when exceeding BACKUP_HISTORY_CAP.
            // Never silently drop Success entries, as doing so would orphan their
            // backup files/directories on disk.
            if d.backups.len() > BACKUP_HISTORY_CAP {
                let mut excess = d.backups.len() - BACKUP_HISTORY_CAP;
                let mut idx = d.backups.len();
                while idx > 0 && excess > 0 {
                    idx -= 1;
                    if d.backups[idx].status == BackupStatus::Failed {
                        d.backups.remove(idx);
                        excess -= 1;
                    }
                }
                if d.backups.len() > BACKUP_HISTORY_CAP {
                    tracing::warn!(
                        count = d.backups.len(),
                        cap = BACKUP_HISTORY_CAP,
                        "successful backup history exceeds cap; keeping all Success entries to avoid orphaning files on disk"
                    );
                }
            }
            entry
        });
        Ok(entry)
    }

    fn delete(&self, id: u64) -> Result<(), BackupError> {
        let removed = self.store.mutate_with(|d| {
            let before = d.backups.len();
            d.backups.retain(|b| b.id != id);
            d.backups.len() < before
        });
        if !removed {
            return Err(BackupError::NotFound(id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chronobase_bk_repo_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp");
        dir
    }

    #[test]
    fn history_is_capped_at_500_entries() {
        let dir = temp_dir("cap");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let repo = StoreBackupRepository::new(Arc::clone(&store));

        for i in 0..505 {
            repo.record(RecordBackupInput {
                connection_id: 1,
                connection_name: format!("DB {i}"),
                group_name: "Default".into(),
                db_type: DbKind::Postgres,
                source_kind: BackupSourceKind::Direct,
                file_path: String::new(),
                file_size: 0,
                status: BackupStatus::Failed,
                error_msg: Some("boom".into()),
            })
            .expect("record");
        }

        let list = repo.list().expect("list");
        assert_eq!(list.len(), BACKUP_HISTORY_CAP);
        assert_eq!(list[0].connection_name, "DB 504");
        assert_eq!(list[BACKUP_HISTORY_CAP - 1].connection_name, "DB 5");

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_entries_evicted_before_success_entries() {
        let dir = temp_dir("evict_failed_first");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let repo = StoreBackupRepository::new(Arc::clone(&store));

        // Record 10 Failed entries first (oldest), then 495 Success entries.
        for i in 0..10 {
            repo.record(RecordBackupInput {
                connection_id: 1,
                connection_name: format!("Fail {i}"),
                group_name: "Default".into(),
                db_type: DbKind::Postgres,
                source_kind: BackupSourceKind::Direct,
                file_path: String::new(),
                file_size: 0,
                status: BackupStatus::Failed,
                error_msg: Some("err".into()),
            })
            .expect("record");
        }
        for i in 0..495 {
            repo.record(RecordBackupInput {
                connection_id: 1,
                connection_name: format!("Ok {i}"),
                group_name: "Default".into(),
                db_type: DbKind::Postgres,
                source_kind: BackupSourceKind::Direct,
                file_path: format!("Default/Ok_{i}.sql"),
                file_size: 100,
                status: BackupStatus::Success,
                error_msg: None,
            })
            .expect("record");
        }

        let list = repo.list().expect("list");
        assert_eq!(list.len(), BACKUP_HISTORY_CAP);
        let success_count = list
            .iter()
            .filter(|b| b.status == BackupStatus::Success)
            .count();
        let failed_count = list
            .iter()
            .filter(|b| b.status == BackupStatus::Failed)
            .count();
        assert_eq!(success_count, 495);
        assert_eq!(failed_count, 5);
        // Oldest failed entries (Fail 0..5) were evicted; Fail 5..10 remain.
        assert!(list.iter().any(|b| b.connection_name == "Fail 9"));
        assert!(!list.iter().any(|b| b.connection_name == "Fail 0"));

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn success_entries_never_evicted_when_over_cap() {
        let dir = temp_dir("never_evict_success");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let repo = StoreBackupRepository::new(Arc::clone(&store));

        for i in 0..505 {
            repo.record(RecordBackupInput {
                connection_id: 1,
                connection_name: format!("DB {i}"),
                group_name: "Default".into(),
                db_type: DbKind::Postgres,
                source_kind: BackupSourceKind::Direct,
                file_path: format!("Default/DB_{i}.sql"),
                file_size: 10,
                status: BackupStatus::Success,
                error_msg: None,
            })
            .expect("record");
        }

        let list = repo.list().expect("list");
        assert_eq!(list.len(), 505);
        assert_eq!(list[0].connection_name, "DB 504");
        assert_eq!(list[504].connection_name, "DB 0");

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
