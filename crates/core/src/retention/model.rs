//! Domain models for backup retention policies and cleanup plans.

use serde::{Deserialize, Serialize};

use crate::backup::BackupEntry;

fn default_failed_max_age_days() -> Option<u32> {
    Some(7)
}

/// How often scheduled automatic retention cleanup runs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "cron", rename_all = "snake_case")]
pub enum CleanupFrequency {
    /// Run once daily at 03:00 local time (`0 3 * * *`).
    #[default]
    Daily,
    /// Run weekly on Sunday at 03:00 local time (`0 3 * * 0`).
    Weekly,
    /// Run monthly on the 1st at 03:00 local time (`0 3 1 * *`).
    Monthly,
    /// Custom 5-field (or 6/7-field) cron expression.
    Custom(String),
}

impl CleanupFrequency {
    /// Returns the cron expression for this frequency.
    pub fn cron_expr(&self) -> &str {
        match self {
            Self::Daily => "0 3 * * *",
            Self::Weekly => "0 3 * * 0",
            Self::Monthly => "0 3 1 * *",
            Self::Custom(expr) => expr.as_str(),
        }
    }

    /// Returns the short UI identifier (`"daily"`, `"weekly"`, `"monthly"`, `"custom"`).
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Custom(_) => "custom",
        }
    }
}

/// Retention policy governing how many snapshots or days of history are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Whether automatic retention is active. Disabled by default for upgrade safety.
    #[serde(default)]
    pub enabled: bool,
    /// Keep at most the newest `N` successful snapshots per `(connection_id, source_kind)`.
    #[serde(default)]
    pub keep_last: Option<u32>,
    /// Delete successful snapshots older than `N` days (newest good snapshot is still kept).
    #[serde(default)]
    pub max_age_days: Option<u32>,
    /// Delete failed backup history entries older than `N` days (defaults to 7).
    #[serde(default = "default_failed_max_age_days")]
    pub failed_max_age_days: Option<u32>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            keep_last: None,
            max_age_days: None,
            failed_max_age_days: default_failed_max_age_days(),
        }
    }
}

/// Why a backup entry was selected for retention cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionReason {
    /// Exceeded `keep_last` count in its `(connection_id, source_kind)` group.
    ExceedsKeepLast,
    /// Older than `max_age_days`.
    ExceedsMaxAge,
    /// Failed entry older than `failed_max_age_days`.
    FailedExpired,
    /// Artifact is missing on disk; metadata entry is dropped.
    MissingOnDisk,
}

impl RetentionReason {
    /// Returns a human-readable reason tag for logs and CLI output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExceedsKeepLast => "exceeds_keep_last",
            Self::ExceedsMaxAge => "exceeds_max_age",
            Self::FailedExpired => "failed_expired",
            Self::MissingOnDisk => "missing_on_disk",
        }
    }
}

/// A single snapshot scheduled for deletion by [`crate::retention::plan_retention`].
#[derive(Debug, Clone)]
pub struct RetentionCandidate {
    /// The backup entry to delete.
    pub entry: BackupEntry,
    /// Reason the entry matched the retention policy.
    pub reason: RetentionReason,
}

/// Pure output of [`crate::retention::plan_retention`].
#[derive(Debug, Clone, Default)]
pub struct RetentionPlan {
    /// Entries selected for deletion.
    pub to_delete: Vec<BackupEntry>,
    /// Detailed candidates with deletion reasons.
    pub candidates: Vec<RetentionCandidate>,
    /// Total on-disk bytes that will be reclaimed by deleting `to_delete`.
    pub bytes_freed: u64,
}

/// Result summary returned after executing retention cleanup.
#[derive(Debug, Clone, Default)]
pub struct RetentionRunReport {
    /// Number of entries successfully deleted.
    pub deleted_count: usize,
    /// Total bytes reclaimed on disk.
    pub bytes_freed: u64,
    /// Per-item deletion errors (one failure never aborts the remaining deletions).
    pub errors: Vec<String>,
}
