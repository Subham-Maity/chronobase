//! Repository trait and JSON-store implementation for backup schedules.

use std::sync::Arc;

use crate::schedule::{CreateScheduleInput, Schedule, ScheduleError};
use crate::store::{JsonStore, local_timestamp};

/// Persistence boundary for backup schedules.
pub trait ScheduleRepository: Send + Sync {
    /// Lists all schedules (newest first).
    fn list(&self) -> Result<Vec<Schedule>, ScheduleError>;

    /// Creates and persists a new schedule.
    fn create(&self, input: CreateScheduleInput) -> Result<Schedule, ScheduleError>;

    /// Toggles `enabled` on a schedule and returns the updated schedule.
    fn toggle(&self, id: u64) -> Result<Schedule, ScheduleError>;

    /// Updates `last_run` to the current local timestamp.
    fn mark_last_run(&self, id: u64) -> Result<(), ScheduleError>;

    /// Deletes a schedule by ID.
    fn delete(&self, id: u64) -> Result<(), ScheduleError>;
}

/// JSON-store-backed [`ScheduleRepository`].
pub struct StoreScheduleRepository {
    store: Arc<JsonStore>,
}

impl StoreScheduleRepository {
    /// Wraps an open [`JsonStore`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self { store }
    }
}

impl ScheduleRepository for StoreScheduleRepository {
    fn list(&self) -> Result<Vec<Schedule>, ScheduleError> {
        Ok(self.store.read_with(|d| d.schedules.clone()))
    }

    fn create(&self, input: CreateScheduleInput) -> Result<Schedule, ScheduleError> {
        let sched = self.store.mutate_with(|d| {
            let id = d._seq.s;
            d._seq.s = d._seq.s.saturating_add(1);
            let sched = Schedule {
                id,
                name: input.name.trim().to_string(),
                group_name: if input.group_name.trim().is_empty() {
                    crate::constant::ALL_GROUPS.to_string()
                } else {
                    input.group_name.trim().to_string()
                },
                cron_expr: input.cron_expr.trim().to_string(),
                enabled: true,
                last_run: None,
                created_at: local_timestamp(),
            };
            d.schedules.insert(0, sched.clone());
            sched
        });
        tracing::info!(schedule_id = sched.id, "schedule created");
        Ok(sched)
    }

    fn toggle(&self, id: u64) -> Result<Schedule, ScheduleError> {
        let updated = self.store.mutate_with(|d| {
            let s = d.schedules.iter_mut().find(|s| s.id == id)?;
            s.enabled = !s.enabled;
            Some(s.clone())
        });
        updated.ok_or(ScheduleError::NotFound(id))
    }

    fn mark_last_run(&self, id: u64) -> Result<(), ScheduleError> {
        self.store.mutate_with(|d| {
            if let Some(s) = d.schedules.iter_mut().find(|s| s.id == id) {
                s.last_run = Some(local_timestamp());
            }
        });
        Ok(())
    }

    fn delete(&self, id: u64) -> Result<(), ScheduleError> {
        let removed = self.store.mutate_with(|d| {
            let before = d.schedules.len();
            d.schedules.retain(|s| s.id != id);
            d.schedules.len() < before
        });
        if !removed {
            return Err(ScheduleError::NotFound(id));
        }
        tracing::info!(schedule_id = id, "schedule deleted");
        Ok(())
    }
}
