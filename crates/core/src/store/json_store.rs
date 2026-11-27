//! Atomic JSON store with advisory file locking, debounced flushing,
//! v1 schema compatibility, stale path re-basing, and corrupt-file recovery.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use chrono::Local;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::backup::{BackupEntry, BackupStatus};
use crate::bridge::VpsPeer;
use crate::connection::Connection;
use crate::constant::{
    BACKUPS_DIR_NAME, CURRENT_SCHEMA_VERSION, DATA_FILE_NAME, LOCK_FILE_NAME, STORE_FLUSH_DEBOUNCE,
};
use crate::platform;
use crate::schedule::Schedule;

/// Errors returned by the JSON persistence layer.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Another ChronoBase instance already holds the advisory lock on this data directory.
    #[error("Another ChronoBase instance is already using this data directory")]
    LockHeld,

    /// Stored backup path is empty or invalid.
    #[error("Backup path is empty or invalid")]
    InvalidBackupPath,

    /// Stored backup path resolved outside the `backups/` directory.
    #[error("Refusing to delete path outside the backups directory")]
    PathOutsideBackupsDir,

    /// JSON serialization error.
    #[error("Failed to serialize store data: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Filesystem I/O error.
    #[error("Store I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Auto-increment sequence counters (matches ChronoBase v1 `_seq`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seq {
    /// Next connection ID.
    #[serde(default = "default_seq_one")]
    pub c: u64,
    /// Next backup ID.
    #[serde(default = "default_seq_one")]
    pub b: u64,
    /// Next schedule ID.
    #[serde(default = "default_seq_one")]
    pub s: u64,
    /// Next VPS peer ID.
    #[serde(default = "default_seq_one")]
    pub v: u64,
}

impl Default for Seq {
    fn default() -> Self {
        Self {
            c: 1,
            b: 1,
            s: 1,
            v: 1,
        }
    }
}

fn default_seq_one() -> u64 {
    1
}

fn default_v1_schema_version() -> u32 {
    1
}

/// User-configurable desktop background & startup preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    /// When `true`, closing the main window hides it to the system tray so cron schedules keep running.
    #[serde(default)]
    pub run_in_background: bool,
    /// When `true`, ChronoBase is registered to launch minimized (`--tray`) on OS login.
    #[serde(default)]
    pub run_on_startup: bool,
}

/// Root persisted state matching ChronoBase v1's `chronobase-data.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreData {
    /// Schema version (`1` for v1 files without this field, `3` for current).
    #[serde(default = "default_v1_schema_version")]
    pub schema_version: u32,
    /// Saved database connections.
    #[serde(default)]
    pub connections: Vec<Connection>,
    /// Recorded backup snapshots (newest first, capped at 500).
    #[serde(default)]
    pub backups: Vec<BackupEntry>,
    /// Configured cron schedules.
    #[serde(default)]
    pub schedules: Vec<Schedule>,
    /// Configured remote VPS pull peers.
    #[serde(default)]
    pub vps_peers: Vec<VpsPeer>,
    /// Desktop background tray & startup settings.
    #[serde(default)]
    pub settings: AppSettings,
    /// Auto-increment sequence counters.
    #[serde(default)]
    pub _seq: Seq,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            connections: Vec::new(),
            backups: Vec::new(),
            schedules: Vec::new(),
            vps_peers: Vec::new(),
            settings: AppSettings::default(),
            _seq: Seq::default(),
        }
    }
}

/// Returns the current local timestamp formatted as `YYYY-MM-DD HH:MM:SS` (matching v1).
pub fn local_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Thread-safe, lock-protected, atomically persisted JSON store.
pub struct JsonStore {
    data_dir: PathBuf,
    backups_dir: PathBuf,
    data_file: PathBuf,
    // Held open for the lifetime of JsonStore to maintain the advisory fs4 lock.
    _lock_file: File,
    state: RwLock<StoreData>,
    dirty: AtomicBool,
    flush_notify: Arc<Notify>,
}

impl JsonStore {
    /// Opens (or initializes) the JSON store in `data_dir`, acquiring an exclusive
    /// `fs4` advisory lock so two ChronoBase instances cannot corrupt the file.
    pub fn open(data_dir: PathBuf) -> Result<Self, StoreError> {
        fs::create_dir_all(&data_dir)?;
        let backups_dir = data_dir.join(BACKUPS_DIR_NAME);
        fs::create_dir_all(&backups_dir)?;

        let lock_path = data_dir.join(LOCK_FILE_NAME);
        let lock_file = platform::open_lock_file(&lock_path)?;
        FileExt::try_lock(&lock_file).map_err(|err| match err {
            fs4::TryLockError::WouldBlock => StoreError::LockHeld,
            fs4::TryLockError::Error(io_err) => StoreError::Io(io_err),
        })?;

        let data_file = data_dir.join(DATA_FILE_NAME);
        let mut data = load_or_recover(&data_file)?;
        migrate_and_check_backups(&mut data, &backups_dir);

        let store = Self {
            data_dir,
            backups_dir,
            data_file,
            _lock_file: lock_file,
            state: RwLock::new(data),
            dirty: AtomicBool::new(false),
            flush_notify: Arc::new(Notify::new()),
        };

        // Ensure the data file exists on first run (matching v1 behavior).
        if !store.data_file.exists() {
            store.flush_now()?;
        }

        Ok(store)
    }

    /// Returns the active data directory.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Returns the `backups/` directory path.
    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }

    /// Executes a read-only closure against the in-memory store state.
    pub fn read_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&StoreData) -> R,
    {
        let guard = match self.state.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        f(&guard)
    }

    /// Executes a mutating closure against the in-memory store state and schedules
    /// a debounced atomic flush to disk.
    pub fn mutate_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut StoreData) -> R,
    {
        let result = {
            let mut guard = match self.state.write() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.schema_version = CURRENT_SCHEMA_VERSION;
            f(&mut guard)
        };
        self.dirty.store(true, Ordering::Release);
        self.flush_notify.notify_one();
        result
    }

    /// Atomically writes the current in-memory state to disk (temp file + `sync_all` + rename).
    pub fn flush_now(&self) -> Result<(), StoreError> {
        let json = {
            let mut guard = match self.state.write() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.schema_version = CURRENT_SCHEMA_VERSION;
            serde_json::to_string_pretty(&*guard)?
        };

        let tmp_path = self.data_dir.join(format!("{DATA_FILE_NAME}.tmp"));
        {
            let mut tmp_file = platform::create_private_file(&tmp_path)?;
            tmp_file.write_all(json.as_bytes())?;
            tmp_file.sync_all()?;
        }
        fs::rename(&tmp_path, &self.data_file)?;
        self.dirty.store(false, Ordering::Release);
        Ok(())
    }

    /// Resolves a stored backup path (relative or legacy absolute) to a path on disk.
    pub fn resolve_backup_path(&self, stored_path: &str) -> Result<PathBuf, StoreError> {
        if stored_path.trim().is_empty() {
            return Err(StoreError::InvalidBackupPath);
        }
        let candidate = Path::new(stored_path);
        if candidate.is_relative() {
            Ok(self.backups_dir.join(candidate))
        } else if candidate.exists() {
            Ok(candidate.to_path_buf())
        } else if let Some(rel) = extract_group_file_tail(stored_path) {
            Ok(self.backups_dir.join(rel))
        } else {
            Ok(candidate.to_path_buf())
        }
    }

    /// Resolves and validates a stored backup path against `backups_dir`
    /// without requiring existence (returns canonicalized path if present).
    ///
    /// Security: canonicalizes both `backups_dir` and the target path and asserts
    /// `target.starts_with(&canon_backups)` so a tampered `chronobase-data.json`
    /// (e.g., `file_path: "../../etc/passwd"`) can never be deleted or opened.
    pub fn resolve_backup_path_checked(&self, stored_path: &str) -> Result<PathBuf, StoreError> {
        let resolved = self.resolve_backup_path(stored_path)?;
        if !resolved.exists() {
            return Ok(resolved);
        }

        let canon_backups = fs::canonicalize(&self.backups_dir)?;
        let canon_target = fs::canonicalize(&resolved)?;

        if !canon_target.starts_with(&canon_backups) || canon_target == canon_backups {
            tracing::warn!("blocked attempt to access path outside backups directory");
            return Err(StoreError::PathOutsideBackupsDir);
        }

        Ok(canon_target)
    }

    /// Resolves and validates a stored backup path before deletion.
    pub fn resolve_backup_path_for_delete(&self, stored_path: &str) -> Result<PathBuf, StoreError> {
        self.resolve_backup_path_checked(stored_path)
    }

    /// Spawns a background task that debounces store writes and performs a final
    /// flush when `shutdown` becomes `true`.
    pub fn spawn_flush_loop(
        store: Arc<Self>,
        mut shutdown: watch::Receiver<bool>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = store.flush_notify.notified() => {
                        tokio::time::sleep(STORE_FLUSH_DEBOUNCE).await;
                        if store.dirty.load(Ordering::Acquire)
                            && let Err(err) = store.flush_now()
                        {
                            tracing::error!(error = %err, "failed to flush store");
                        }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            if store.dirty.load(Ordering::Acquire)
                                && let Err(err) = store.flush_now()
                            {
                                tracing::error!(error = %err, "failed final store flush on shutdown");
                            }
                            break;
                        }
                    }
                }
            }
        })
    }
}

impl Drop for JsonStore {
    fn drop(&mut self) {
        // Explicitly release the advisory lock before closing the file descriptor.
        // Intentionally ignore: OS also releases locks when the descriptor closes.
        let _ = FileExt::unlock(&self._lock_file);
    }
}

fn load_or_recover(data_file: &Path) -> Result<StoreData, StoreError> {
    if !data_file.exists() {
        return Ok(StoreData::default());
    }

    let raw = fs::read_to_string(data_file)?;
    match serde_json::from_str::<StoreData>(&raw) {
        Ok(data) => Ok(data),
        Err(err) => {
            let stamp = Local::now().format("%Y%m%d_%H%M%S");
            let corrupt_name = format!("{DATA_FILE_NAME}.corrupt-{stamp}");
            let corrupt_path = data_file
                .parent()
                .map(|p| p.join(&corrupt_name))
                .unwrap_or_else(|| PathBuf::from(&corrupt_name));
            tracing::warn!(
                error = %err,
                backup = %corrupt_path.display(),
                "corrupt store file detected; preserving backup and starting fresh"
            );
            fs::rename(data_file, &corrupt_path)?;
            Ok(StoreData::default())
        }
    }
}

/// Re-bases stale absolute `file_path` entries from v1 exports onto `backups_dir`
/// using their `<GroupSlug>/<entry_name>` tail, and marks missing files.
fn migrate_and_check_backups(data: &mut StoreData, backups_dir: &Path) {
    let canon_backups = fs::canonicalize(backups_dir).ok();

    for entry in &mut data.backups {
        if entry.file_path.is_empty() {
            if entry.status == BackupStatus::Success {
                entry.missing_on_disk = true;
            }
            continue;
        }

        let stored = Path::new(&entry.file_path);
        let is_abs = is_cross_platform_absolute(&entry.file_path);
        let is_inside_current = if is_abs && stored.is_absolute() {
            match (&canon_backups, fs::canonicalize(stored)) {
                (Some(cb), Ok(ct)) => ct.starts_with(cb),
                _ => false,
            }
        } else {
            !is_abs
        };

        if !is_inside_current && let Some(rel) = extract_group_file_tail(&entry.file_path) {
            entry.file_path = rel;
        }

        let check_path = if Path::new(&entry.file_path).is_relative() {
            backups_dir.join(&entry.file_path)
        } else {
            PathBuf::from(&entry.file_path)
        };

        if entry.status == BackupStatus::Success && !check_path.exists() {
            entry.missing_on_disk = true;
        }
    }
}

fn is_cross_platform_absolute(raw: &str) -> bool {
    let b = raw.as_bytes();
    raw.starts_with('/')
        || raw.starts_with('\\')
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'/' || b[2] == b'\\'))
}

/// Extracts `<GroupSlug>/[<YYYY-MM-DD>/]<filename>` from a Windows or Unix path string.
fn extract_group_file_tail(raw_path: &str) -> Option<String> {
    let parts: Vec<&str> = raw_path
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(pos) = parts
        .iter()
        .rposition(|p| p.eq_ignore_ascii_case("backups"))
    {
        let tail = &parts[pos + 1..];
        if !tail.is_empty() && tail.len() <= 3 && tail.iter().all(|s| *s != "." && *s != "..") {
            return Some(tail.join("/"));
        }
    }
    if parts.len() >= 2 {
        let group = parts[parts.len() - 2];
        let name = parts[parts.len() - 1];
        if group != "." && group != ".." && name != "." && name != ".." {
            return Some(format!("{group}/{name}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chronobase_test_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn round_trip_and_exclusive_lock() {
        let dir = temp_dir("roundtrip");
        {
            let store = JsonStore::open(dir.clone()).expect("open store");
            // Second open on same directory must fail due to fs4 lock.
            assert!(matches!(
                JsonStore::open(dir.clone()),
                Err(StoreError::LockHeld)
            ));

            store.mutate_with(|d| {
                d._seq.c = 2;
            });
            store.flush_now().expect("flush");
        }

        let reopened = JsonStore::open(dir.clone()).expect("reopen after drop");
        let seq_c = reopened.read_with(|d| d._seq.c);
        assert_eq!(seq_c, 2);
        drop(reopened);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn v1_import_rebases_stale_absolute_paths_and_marks_missing() {
        let dir = temp_dir("v1_import");
        let backups_dir = dir.join("backups").join("MySaas");
        fs::create_dir_all(&backups_dir).expect("create group dir");
        let existing_sql = backups_dir.join("App_20260814_014526.sql");
        fs::write(&existing_sql, "-- sql").expect("write sql");

        let v1_json = r#"{
          "connections": [],
          "backups": [
            {
              "id": 1,
              "connection_id": 1,
              "connection_name": "App",
              "group_name": "MySaas",
              "type": "postgres",
              "file_path": "C:\\OldMachine\\chronobase\\backups\\MySaas\\App_20260814_014526.sql",
              "file_size": 6,
              "status": "success",
              "error_msg": null,
              "created_at": "2026-08-14 01:45:26"
            },
            {
              "id": 2,
              "connection_id": 1,
              "connection_name": "App",
              "group_name": "MySaas",
              "type": "postgres",
              "file_path": "/old/machine/backups/MySaas/Missing_20260814_010000.sql",
              "file_size": 100,
              "status": "success",
              "error_msg": null,
              "created_at": "2026-08-14 01:00:00"
            }
          ],
          "schedules": [],
          "_seq": { "c": 1, "b": 3, "s": 1 }
        }"#;
        fs::write(dir.join(DATA_FILE_NAME), v1_json).expect("write v1 json");

        let store = JsonStore::open(dir.clone()).expect("open v1 store");
        store.read_with(|d| {
            assert_eq!(d.schema_version, 1);
            assert_eq!(d.backups.len(), 2);
            assert_eq!(d.backups[0].file_path, "MySaas/App_20260814_014526.sql");
            assert!(!d.backups[0].missing_on_disk);
            assert_eq!(d.backups[1].file_path, "MySaas/Missing_20260814_010000.sql");
            assert!(d.backups[1].missing_on_disk);
        });

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_renamed_instead_of_silently_wiped() {
        let dir = temp_dir("corrupt");
        fs::write(dir.join(DATA_FILE_NAME), "{ not valid json !!!").expect("write corrupt");

        let store = JsonStore::open(dir.clone()).expect("recover from corrupt");
        let count = store.read_with(|d| d.connections.len());
        assert_eq!(count, 0);

        let corrupt_files: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert_eq!(corrupt_files.len(), 1);

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_containment_blocks_escape_outside_backups_dir() {
        let dir = temp_dir("path_safety");
        let outside_file = dir.join("secret.txt");
        fs::write(&outside_file, "do not delete").expect("write outside");

        let store = JsonStore::open(dir.clone()).expect("open");
        let err = store
            .resolve_backup_path_for_delete("../secret.txt")
            .expect_err("must reject traversal");
        assert!(matches!(err, StoreError::PathOutsideBackupsDir));
        assert!(outside_file.exists());

        // Verify 3-segment Group/Date/File path extraction works cleanly.
        assert_eq!(
            extract_group_file_tail(
                r"C:\Old\chronobase\backups\FXam\2026-11-27\Fxam_20261127_020001.sql"
            )
            .as_deref(),
            Some("FXam/2026-11-27/Fxam_20261127_020001.sql")
        );

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
