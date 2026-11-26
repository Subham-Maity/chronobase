//! Business logic and cron validation for schedules.

use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::Notify;

use crate::schedule::{CreateScheduleInput, Schedule, ScheduleError, ScheduleRepository};

/// Service interface for managing backup schedules.
pub trait ScheduleService: Send + Sync {
    /// Lists all schedules.
    fn list(&self) -> Result<Vec<Schedule>, ScheduleError>;

    /// Validates and creates a new schedule, waking the scheduler runner.
    fn create(&self, input: CreateScheduleInput) -> Result<Schedule, ScheduleError>;

    /// Toggles `enabled` on a schedule and wakes the scheduler runner.
    fn toggle(&self, id: u64) -> Result<Schedule, ScheduleError>;

    /// Deletes a schedule and wakes the scheduler runner.
    fn delete(&self, id: u64) -> Result<(), ScheduleError>;
}

/// Converts a 5-field (v1 `node-cron`) or 6/7-field cron expression into a parsed
/// [`cron::Schedule`].
///
/// When given 5 whitespace-separated fields (`min hour dom month dow`), `"0 "`
/// is prepended so the `cron` crate evaluates it at second `0`.
pub fn parse_cron_expr(raw: &str) -> Result<cron::Schedule, ScheduleError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ScheduleError::Validation(
            "Cron expression is required".to_string(),
        ));
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let normalized = if parts.len() == 5 {
        // v1 (node-cron) uses 0 for Sunday in the day-of-week field; the `cron`
        // crate uses 1-7 (1 = Sunday) or SUN-SAT. Map standalone `0` -> `SUN`.
        let dow = normalize_dow_sunday(parts[4]);
        format!(
            "0 {} {} {} {} {}",
            parts[0], parts[1], parts[2], parts[3], dow
        )
    } else if parts.len() == 6 || parts.len() == 7 {
        trimmed.to_string()
    } else {
        return Err(ScheduleError::Validation(format!(
            "Invalid cron expression '{trimmed}': expected 5 fields (e.g. '0 2 * * *')"
        )));
    };

    cron::Schedule::from_str(&normalized)
        .map_err(|e| ScheduleError::Validation(format!("Invalid cron expression '{trimmed}': {e}")))
}

fn normalize_dow_sunday(dow: &str) -> String {
    dow.split(',')
        .map(|part| if part == "0" { "SUN" } else { part })
        .collect::<Vec<_>>()
        .join(",")
}

/// Default [`ScheduleService`] backed by [`ScheduleRepository`].
pub struct DefaultScheduleService {
    repo: Arc<dyn ScheduleRepository>,
    reload_notify: Arc<Notify>,
}

impl DefaultScheduleService {
    /// Creates a new [`DefaultScheduleService`].
    pub fn new(repo: Arc<dyn ScheduleRepository>, reload_notify: Arc<Notify>) -> Self {
        Self {
            repo,
            reload_notify,
        }
    }
}

impl ScheduleService for DefaultScheduleService {
    fn list(&self) -> Result<Vec<Schedule>, ScheduleError> {
        self.repo.list()
    }

    fn create(&self, input: CreateScheduleInput) -> Result<Schedule, ScheduleError> {
        if input.name.trim().is_empty() {
            return Err(ScheduleError::Validation(
                "Schedule name is required".to_string(),
            ));
        }
        parse_cron_expr(&input.cron_expr)?;
        let sched = self.repo.create(input)?;
        self.reload_notify.notify_one();
        Ok(sched)
    }

    fn toggle(&self, id: u64) -> Result<Schedule, ScheduleError> {
        let sched = self.repo.toggle(id)?;
        self.reload_notify.notify_one();
        Ok(sched)
    }

    fn delete(&self, id: u64) -> Result<(), ScheduleError> {
        self.repo.delete(id)?;
        self.reload_notify.notify_one();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_field_v1_cron_presets_parse_cleanly() {
        for expr in [
            "0 2 * * *",
            "0 */6 * * *",
            "0 * * * *",
            "0 2 * * 0",
            "0 2 1 * *",
        ] {
            assert!(parse_cron_expr(expr).is_ok(), "failed to parse {expr}");
        }
    }

    #[test]
    fn invalid_cron_returns_clear_validation_error() {
        assert!(matches!(
            parse_cron_expr("not a cron"),
            Err(ScheduleError::Validation(_))
        ));
        assert!(matches!(
            parse_cron_expr("* * *"),
            Err(ScheduleError::Validation(_))
        ));
    }
}
