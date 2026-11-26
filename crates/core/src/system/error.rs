//! Typed error enum for system and Danger Zone operations.

use thiserror::Error;

use crate::store::StoreError;

/// Errors arising from Danger Zone resets and system operations.
#[derive(Debug, Error)]
pub enum SystemError {
    /// Confirmation phrase did not match the required literal string.
    #[error("Confirmation phrase does not match")]
    ConfirmPhraseMismatch,

    /// Underlying persistence or filesystem error.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Filesystem I/O error.
    #[error("Filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
