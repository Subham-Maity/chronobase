//! Retention policy preview, safe deletion execution, and serialization lock.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Local;
use tokio::sync::Mutex;

use crate::backup::BackupService;
use crate::connection::ConnectionRepository;
use crate::retention::error::RetentionError;
use crate::retention::model::{RetentionPlan, RetentionPolicy, RetentionRunReport};
use crate::retention::planner::plan_retention;
use crate::store::{JsonStore, local_timestamp};

/// Service interface for previewing and executing backup retention cleanup.
pub trait RetentionService: Send + Sync {
    /// Computes which snapshots would be deleted without touching disk or history.
    fn preview(&self) -> Result<RetentionPlan, RetentionError>;

    /// Executes retention across all connections (serialized against restore/migrate/pull).
    fn run(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<RetentionRunReport, RetentionError>> + Send + '_>>;

    /// Executes retention for a single connection ID (e.g. immediately after a backup).
    fn run_for_connection(
        &self,
        connection_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<RetentionRunReport, RetentionError>> + Send + '_>>;
}

/// Default [`RetentionService`] deleting exclusively via [`BackupService::delete_backup`].
pub struct DefaultRetentionService {
    store: Arc<JsonStore>,
    conn_repo: Arc<dyn ConnectionRepository>,
    backup_service: Arc<dyn BackupService>,
    snapshot_op_lock: Arc<Mutex<()>>,
}

impl DefaultRetentionService {
    /// Creates a new [`DefaultRetentionService`].
    pub fn new(
        store: Arc<JsonStore>,
        conn_repo: Arc<dyn ConnectionRepository>,
        backup_service: Arc<dyn BackupService>,
        snapshot_op_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            store,
            conn_repo,
            backup_service,
            snapshot_op_lock,
        }
    }

    fn build_plan(&self, only_conn_id: Option<u64>) -> Result<RetentionPlan, RetentionError> {
        let global_policy = self.store.read_with(|d| d.settings.retention.clone());
        let conns = self.conn_repo.list()?;
        let entries = self.backup_service.list_filtered(None, None, None)?;

        let policy_for_conn = |conn_id: u64| -> Option<RetentionPolicy> {
            if let Some(target) = only_conn_id
                && conn_id != target
            {
                return None;
            }
            if let Some(conn) = conns.iter().find(|c| c.id == conn_id)
                && let Some(ref custom) = conn.retention
            {
                return Some(custom.clone());
            }
            Some(global_policy.clone())
        };

        Ok(plan_retention(&entries, policy_for_conn, Local::now()))
    }

    async fn execute_plan(
        &self,
        only_conn_id: Option<u64>,
    ) -> Result<RetentionRunReport, RetentionError> {
        let _guard = self.snapshot_op_lock.lock().await;
        let plan = self.build_plan(only_conn_id)?;

        let mut report = RetentionRunReport::default();
        for cand in plan.candidates {
            let entry = &cand.entry;
            match self.backup_service.delete_backup(entry.id) {
                Ok(()) => {
                    tracing::info!(
                        backup_id = entry.id,
                        path = %entry.file_path,
                        size = entry.file_size,
                        reason = cand.reason.as_str(),
                        "retention deleted snapshot"
                    );
                    report.deleted_count += 1;
                    if !entry.missing_on_disk {
                        report.bytes_freed = report.bytes_freed.saturating_add(entry.file_size);
                    }
                }
                Err(err) => {
                    let msg = format!(
                        "Failed to delete backup #{} ({}): {err}",
                        entry.id, entry.file_path
                    );
                    tracing::error!(
                        backup_id = entry.id,
                        path = %entry.file_path,
                        error = %err,
                        "retention deletion failed"
                    );
                    report.errors.push(msg);
                }
            }
        }

        if only_conn_id.is_none() {
            self.store.mutate_with(|d| {
                d.settings.last_cleanup = Some(local_timestamp());
            });
        }

        Ok(report)
    }
}

impl RetentionService for DefaultRetentionService {
    fn preview(&self) -> Result<RetentionPlan, RetentionError> {
        self.build_plan(None)
    }

    fn run(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<RetentionRunReport, RetentionError>> + Send + '_>> {
        Box::pin(async move { self.execute_plan(None).await })
    }

    fn run_for_connection(
        &self,
        connection_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<RetentionRunReport, RetentionError>> + Send + '_>> {
        Box::pin(async move { self.execute_plan(Some(connection_id)).await })
    }
}
