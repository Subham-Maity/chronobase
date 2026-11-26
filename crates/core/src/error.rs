//! Top-level core error type used only when an operation crosses feature boundaries.

use thiserror::Error;

use crate::backup::BackupError;
use crate::connection::ConnectionError;
use crate::schedule::ScheduleError;
use crate::store::StoreError;
use crate::system::SystemError;

/// Unified error returned by cross-cutting operations (such as CLI commands or bootstrap).
#[derive(Debug, Error)]
pub enum CoreError {
    /// Store persistence or locking error.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Connection management error.
    #[error(transparent)]
    Connection(#[from] ConnectionError),

    /// Backup, restore, or wipe engine error.
    #[error(transparent)]
    Backup(#[from] BackupError),

    /// Schedule management or cron parsing error.
    #[error(transparent)]
    Schedule(#[from] ScheduleError),

    /// System or danger-zone error.
    #[error(transparent)]
    System(#[from] SystemError),
}
