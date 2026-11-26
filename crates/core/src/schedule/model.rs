//! Domain models for automated backup schedules.

use serde::{Deserialize, Serialize};

/// A cron-based backup schedule (matches ChronoBase v1 JSON schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Auto-incremented unique identifier.
    pub id: u64,
    /// Display name of the schedule.
    pub name: String,
    /// Target group name or `"All"` for all connections.
    pub group_name: String,
    /// 5-field (or 6/7-field) cron expression.
    pub cron_expr: String,
    /// Whether the schedule is active.
    pub enabled: bool,
    /// Timestamp of the last completed run (`YYYY-MM-DD HH:MM:SS`), if any.
    #[serde(default)]
    pub last_run: Option<String>,
    /// Creation timestamp (`YYYY-MM-DD HH:MM:SS` local time, matching v1).
    pub created_at: String,
}

/// Input payload for creating a new schedule.
#[derive(Debug, Clone)]
pub struct CreateScheduleInput {
    /// Display name (required, non-empty).
    pub name: String,
    /// Target group name or `"All"`.
    pub group_name: String,
    /// 5-field cron expression (e.g. `"0 2 * * *"`).
    pub cron_expr: String,
}
