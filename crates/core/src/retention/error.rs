//! Error types for backup retention configuration and execution.

use thiserror::Error;

/// Errors returned by [`crate::retention::RetentionService`].
#[derive(Debug, Error)]
pub enum RetentionError {
    /// Invalid retention policy or cleanup cron expression.
    #[error("{0}")]
    Validation(String),

    /// Failed to query or delete backup entries.
    #[error("Backup operation failed: {0}")]
    Backup(#[from] crate::backup::BackupError),

    /// Failed to read connections for policy resolution.
    #[error("Connection lookup failed: {0}")]
    Connection(#[from] crate::connection::ConnectionError),
}
