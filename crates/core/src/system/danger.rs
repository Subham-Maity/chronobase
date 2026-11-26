//! Danger Zone destructive operations with core-level phrase re-validation.

use std::fs;
use std::sync::Arc;

use crate::constant::{PHRASE_DELETE_ALL, PHRASE_DELETE_BACKUPS};
use crate::store::{JsonStore, StoreData};
use crate::system::SystemError;

/// Service interface for irreversible Danger Zone actions.
pub trait DangerService: Send + Sync {
    /// Deletes all backup files on disk and clears backup history.
    /// Requires `confirm == "DELETE BACKUPS"`.
    fn delete_all_backups(&self, confirm: &str) -> Result<(), SystemError>;

    /// Deletes all backup files and resets connections, schedules, and history.
    /// Requires `confirm == "DELETE EVERYTHING"`.
    fn reset_everything(&self, confirm: &str) -> Result<(), SystemError>;
}

/// Default [`DangerService`] backed by [`JsonStore`].
pub struct DefaultDangerService {
    store: Arc<JsonStore>,
}

impl DefaultDangerService {
    /// Creates a new [`DangerService`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self { store }
    }

    fn recreate_backups_dir(&self) -> Result<(), SystemError> {
        let dir = self.store.backups_dir();
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        fs::create_dir_all(dir)?;
        Ok(())
    }
}

impl DangerService for DefaultDangerService {
    fn delete_all_backups(&self, confirm: &str) -> Result<(), SystemError> {
        // Security: always re-validate the confirmation phrase in core so UI
        // or IPC bugs can never trigger mass deletion accidentally.
        if confirm != PHRASE_DELETE_BACKUPS {
            return Err(SystemError::ConfirmPhraseMismatch);
        }
        self.recreate_backups_dir()?;
        self.store.mutate_with(|d| {
            d.backups.clear();
        });
        self.store.flush_now()?;
        tracing::info!("danger zone: deleted all backup files and history");
        Ok(())
    }

    fn reset_everything(&self, confirm: &str) -> Result<(), SystemError> {
        // Security: always re-validate the confirmation phrase in core.
        if confirm != PHRASE_DELETE_ALL {
            return Err(SystemError::ConfirmPhraseMismatch);
        }
        self.recreate_backups_dir()?;
        self.store.mutate_with(|d| {
            *d = StoreData::default();
        });
        self.store.flush_now()?;
        tracing::info!("danger zone: reset all ChronoBase data");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chronobase_danger_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp");
        dir
    }

    #[test]
    fn confirm_phrase_is_enforced_in_core() {
        let dir = temp_dir("phrase");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let svc = DefaultDangerService::new(Arc::clone(&store));

        assert!(matches!(
            svc.delete_all_backups("delete backups"),
            Err(SystemError::ConfirmPhraseMismatch)
        ));
        assert!(matches!(
            svc.reset_everything("DELETE BACKUPS"),
            Err(SystemError::ConfirmPhraseMismatch)
        ));

        svc.delete_all_backups(PHRASE_DELETE_BACKUPS)
            .expect("exact phrase succeeds");
        svc.reset_everything(PHRASE_DELETE_ALL)
            .expect("exact reset phrase succeeds");

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
