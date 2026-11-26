//! Typed error enum for backup, restore, and wipe operations.

use thiserror::Error;

use crate::store::StoreError;

/// Errors arising from backup execution, restore, wipe, or snapshot deletion.
#[derive(Debug, Error)]
pub enum BackupError {
    /// Backup entry ID does not exist in history.
    #[error("Backup {0} not found")]
    NotFound(u64),

    /// Connection ID does not exist.
    #[error("Connection {0} not found")]
    ConnectionNotFound(u64),

    /// Required CLI tool (`pg_dump`, `psql`, `mongodump`, `mongorestore`, `mongosh`) is missing.
    #[error("{tool} not found. See the Setup tab for installation instructions.")]
    ToolMissing {
        /// Executable base name that could not be located.
        tool: &'static str,
    },

    /// Backup file or folder is missing on disk.
    #[error("Backup file not found on disk. It may have been moved or deleted.")]
    FileMissingOnDisk,

    /// Invalid restore target URL or missing URL.
    #[error("{0}")]
    Validation(String),

    /// Subprocess failed to spawn or exited with a non-zero status.
    #[error("{0}")]
    ProcessFailed(String),

    /// Subprocess exceeded its configured timeout.
    #[error("Operation timed out after {0}s")]
    TimedOut(u64),

    /// Underlying persistence or path-containment error.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Filesystem I/O error.
    #[error("Filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
