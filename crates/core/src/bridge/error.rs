//! Typed error enum for the secure VPS Pull Bridge and DB-to-DB migration.

use thiserror::Error;

use crate::backup::BackupError;
use crate::connection::ConnectionError;
use crate::store::StoreError;

/// Errors arising from VPS peer management, agent auth, encrypted transfer, or DB migration.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// Requested VPS peer ID or name does not exist.
    #[error("VPS peer {0} not found")]
    NotFound(String),

    /// Input validation failed.
    #[error("{0}")]
    Validation(String),

    /// Mutual HMAC authentication failed or pairing key rejected.
    #[error("Authentication failed: invalid pairing key or expired timestamp")]
    AuthFailed,

    /// Client IP is not allowed or temporarily rate-limited after failed attempts.
    #[error("Connection rejected by VPS agent rate limiter or IP allowlist")]
    AccessDenied,

    /// Malformed frame, unexpected protocol message, or SHA-256 mismatch.
    #[error("Transfer protocol error: {0}")]
    Protocol(String),

    /// Streamed archive entry attempted path traversal outside the staging directory.
    #[error("Blocked unsafe path in remote backup stream: {0}")]
    PathTraversal(String),

    /// Network or remote dump operation timed out.
    #[error("VPS transfer timed out after {0}s")]
    TimedOut(u64),

    /// Remote VPS agent reported an execution error.
    #[error("VPS agent error: {0}")]
    RemoteAgent(String),

    /// Underlying connection error.
    #[error(transparent)]
    Connection(#[from] ConnectionError),

    /// Underlying backup or restore error.
    #[error(transparent)]
    Backup(#[from] BackupError),

    /// Underlying persistence error.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Network or filesystem I/O error.
    #[error("Bridge I/O error: {0}")]
    Io(#[from] std::io::Error),
}
