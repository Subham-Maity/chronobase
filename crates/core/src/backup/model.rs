//! Domain models for backup snapshots and restore operations.

use serde::{Deserialize, Serialize};

use crate::connection::DbKind;
use crate::secret::Secret;

/// Outcome status of a backup attempt (matches ChronoBase v1 JSON schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    /// Backup completed and artifact was written to disk.
    Success,
    /// Backup failed; `error_msg` holds the reason.
    Failed,
}

impl BackupStatus {
    /// Returns `"success"` or `"failed"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}

/// A recorded backup snapshot in history (matches ChronoBase v1 JSON schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    /// Auto-incremented unique identifier.
    pub id: u64,
    /// ID of the source connection.
    pub connection_id: u64,
    /// Snapshot of connection name at backup time.
    pub connection_name: String,
    /// Snapshot of group name at backup time.
    pub group_name: String,
    /// Database engine kind.
    #[serde(rename = "type")]
    pub db_type: DbKind,
    /// Relative path under `backups/` (new) or legacy absolute path (v1).
    pub file_path: String,
    /// Total size in bytes of the `.sql` file or BSON directory.
    pub file_size: u64,
    /// Outcome status.
    pub status: BackupStatus,
    /// Error message when `status == BackupStatus::Failed`.
    #[serde(default)]
    pub error_msg: Option<String>,
    /// Creation timestamp (`YYYY-MM-DD HH:MM:SS` local time, matching v1).
    pub created_at: String,
    /// Runtime-only flag set when a `Success` entry's file cannot be found on disk.
    #[serde(skip)]
    pub missing_on_disk: bool,
}

/// Result of running a single connection backup with error capture.
#[derive(Debug, Clone)]
pub struct BackupRunResult {
    /// Connection ID that was backed up.
    pub connection_id: u64,
    /// Connection name.
    pub connection_name: String,
    /// Recorded backup entry (whether success or failed).
    pub entry: BackupEntry,
}

/// Target destination mode for restoring a snapshot.
#[derive(Debug, Clone)]
pub enum RestoreTarget {
    /// Overwrite the original connection's database.
    SameDatabase,
    /// Push the snapshot to a user-supplied target database URL.
    DifferentDatabase(Secret),
}
