//! Single-task background cron scheduler.
//!
//! Sleeps until the earliest next due time across all enabled schedules in local
//! time (no per-schedule polling), prevents duplicate concurrent runs of the same
//! schedule ID, and updates `last_run` upon completion.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::bridge::BridgeService;
use crate::schedule::{Schedule, ScheduleRepository, parse_cron_expr};

/// Coordinates the single background scheduler task.
pub struct ScheduleRunner {
    repo: Arc<dyn ScheduleRepository>,
    bridge_service: Arc<dyn BridgeService>,
    reload_notify: Arc<Notify>,
    in_flight: Arc<Mutex<HashSet<u64>>>,
}

impl ScheduleRunner {
    /// Creates a new [`ScheduleRunner`].
    pub fn new(
        repo: Arc<dyn ScheduleRepository>,
        bridge_service: Arc<dyn BridgeService>,
        reload_notify: Arc<Notify>,
    ) -> Self {
        Self {
            repo,
            bridge_service,
            reload_notify,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Spawns the single scheduler loop and returns its [`JoinHandle`].
    pub fn spawn(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut next_runs: HashMap<u64, DateTime<Local>> = HashMap::new();
            let mut active_tasks: Vec<JoinHandle<()>> = Vec::new();

            loop {
                // Reap finished task handles.
                active_tasks.retain(|h| !h.is_finished());

                let now = Local::now();
                let schedules = self.repo.list().unwrap_or_default();

                // Remove disabled or deleted schedules from tracking.
                let enabled_ids: HashSet<u64> = schedules
                    .iter()
                    .filter(|s| s.enabled)
                    .map(|s| s.id)
                    .collect();
                next_runs.retain(|id, _| enabled_ids.contains(id));

                // Compute next due time for newly enabled schedules.
                for sched in schedules.iter().filter(|s| s.enabled) {
                    if !next_runs.contains_key(&sched.id)
                        && let Some(next_dt) = compute_next_due(sched, now)
                    {
                        next_runs.insert(sched.id, next_dt);
                    }
                }

                // Trigger any schedule whose due time <= now.
                let due: Vec<Schedule> = schedules
                    .into_iter()
                    .filter(|s| s.enabled)
                    .filter(|s| next_runs.get(&s.id).is_some_and(|dt| *dt <= now))
                    .collect();

                for sched in due {
                    if let Some(next_dt) = compute_next_due(&sched, now) {
                        next_runs.insert(sched.id, next_dt);
                    } else {
                        next_runs.remove(&sched.id);
                    }

                    if self.try_mark_in_flight(sched.id) {
                        let runner = Arc::clone(&self);
                        let handle = tokio::spawn(async move {
                            runner.execute_schedule(sched).await;
                        });
                        active_tasks.push(handle);
                    } else {
                        tracing::debug!(
                            schedule_id = sched.id,
                            "skipping schedule trigger because previous run is still active"
                        );
                    }
                }

                // Sleep until the earliest next_run (or wait for reload/shutdown if none).
                let wait_duration = next_runs.values().min().map(|earliest| {
                    let diff = *earliest - Local::now();
                    diff.to_std().unwrap_or(Duration::from_millis(50))
                });

                tokio::select! {
                    _ = self.reload_notify.notified() => {
                        // Schedule list changed; recompute next_runs immediately.
                        next_runs.clear();
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            break;
                        }
                    }
                    _ = sleep_optional(wait_duration) => {}
                }
            }

            // Await any in-flight scheduled backups on shutdown.
            for handle in active_tasks {
                // Intentionally ignore: task errors are already logged inside execute_schedule.
                let _ = handle.await;
            }
        })
    }

    fn try_mark_in_flight(&self, schedule_id: u64) -> bool {
        let mut guard = match self.in_flight.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.insert(schedule_id)
    }

    fn clear_in_flight(&self, schedule_id: u64) {
        let mut guard = match self.in_flight.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.remove(&schedule_id);
    }

    async fn execute_schedule(&self, sched: Schedule) {
        tracing::info!(
            schedule_id = sched.id,
            group = %sched.group_name,
            "schedule triggered"
        );

        match self
            .bridge_service
            .backup_all_in_group(&sched.group_name)
            .await
        {
            Ok(results) => {
                let ok_count = results
                    .iter()
                    .filter(|r| r.entry.status == crate::backup::BackupStatus::Success)
                    .count();
                tracing::info!(
                    schedule_id = sched.id,
                    succeeded = ok_count,
                    total = results.len(),
                    "scheduled backup batch finished"
                );
            }
            Err(err) => {
                tracing::error!(
                    schedule_id = sched.id,
                    error = %err,
                    "scheduled backup batch failed"
                );
            }
        }

        if let Err(err) = self.repo.mark_last_run(sched.id) {
            tracing::error!(
                schedule_id = sched.id,
                error = %err,
                "failed to update schedule last_run"
            );
        }

        self.clear_in_flight(sched.id);
    }
}

fn compute_next_due(sched: &Schedule, after: DateTime<Local>) -> Option<DateTime<Local>> {
    match parse_cron_expr(&sched.cron_expr) {
        Ok(parsed) => parsed.after(&after).next(),
        Err(err) => {
            tracing::warn!(
                schedule_id = sched.id,
                error = %err,
                "ignoring schedule with invalid cron expression"
            );
            None
        }
    }
}

async fn sleep_optional(duration: Option<Duration>) {
    match duration {
        Some(d) => tokio::time::sleep(d).await,
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::engine::MockCommandRunner;
    use crate::backup::{BackupRepository, DefaultBackupService, StoreBackupRepository};
    use crate::bridge::{DefaultBridgeService, StoreVpsPeerRepository};
    use crate::connection::{ConnectionRepository, StoreConnectionRepository};
    use crate::schedule::StoreScheduleRepository;
    use crate::store::JsonStore;
    use crate::tools::ToolDiscovery;
    use crate::tools::ToolStatus;
    use std::fs;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct StubTools;
    impl ToolDiscovery for StubTools {
        fn status(&self) -> Pin<Box<dyn std::future::Future<Output = ToolStatus> + Send + '_>> {
            Box::pin(async { ToolStatus::default() })
        }
        fn refresh(&self) -> Pin<Box<dyn std::future::Future<Output = ToolStatus> + Send + '_>> {
            self.status()
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chronobase_sched_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp");
        dir
    }

    #[test]
    fn in_flight_guard_prevents_concurrent_duplicate_runs() {
        let dir = temp_dir("inflight");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let conn_repo: Arc<dyn ConnectionRepository> =
            Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
        let bk_repo: Arc<dyn BackupRepository> =
            Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
        let peer_repo: Arc<dyn crate::bridge::VpsPeerRepository> =
            Arc::new(StoreVpsPeerRepository::new(Arc::clone(&store)));
        let sched_repo = Arc::new(StoreScheduleRepository::new(Arc::clone(&store)));
        let tools: Arc<dyn ToolDiscovery> = Arc::new(StubTools);
        let bk_svc: Arc<dyn crate::backup::BackupService> = Arc::new(DefaultBackupService::new(
            Arc::clone(&store),
            Arc::clone(&conn_repo),
            Arc::clone(&bk_repo),
            Arc::clone(&tools),
            Arc::new(MockCommandRunner::new()),
        ));
        let bridge_svc = Arc::new(DefaultBridgeService::new(
            Arc::clone(&store),
            peer_repo,
            conn_repo,
            bk_repo,
            bk_svc,
            tools,
        ));
        let runner = ScheduleRunner::new(sched_repo, bridge_svc, Arc::new(Notify::new()));

        assert!(runner.try_mark_in_flight(42));
        assert!(!runner.try_mark_in_flight(42));
        runner.clear_in_flight(42);
        assert!(runner.try_mark_in_flight(42));

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
