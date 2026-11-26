//! Typed error enum for connection management.

use thiserror::Error;

use crate::store::StoreError;

/// Errors arising from connection CRUD and validation.
#[derive(Debug, Error)]
pub enum ConnectionError {
    /// Requested connection ID does not exist.
    #[error("Connection {0} not found")]
    NotFound(u64),

    /// Input validation failed (missing name, blank URL, or invalid URL scheme).
    #[error("{0}")]
    Validation(String),

    /// Underlying persistence error.
    #[error(transparent)]
    Store(#[from] StoreError),
}
