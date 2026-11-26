//! Typed error enum for schedule management and cron validation.

use thiserror::Error;

use crate::store::StoreError;

/// Errors arising from schedule CRUD and cron expression validation.
#[derive(Debug, Error)]
pub enum ScheduleError {
    /// Requested schedule ID does not exist.
    #[error("Schedule {0} not found")]
    NotFound(u64),

    /// Input validation failed (missing name or invalid cron expression).
    #[error("{0}")]
    Validation(String),

    /// Underlying persistence error.
    #[error(transparent)]
    Store(#[from] StoreError),
}
