//! Application dependency injection container (`AppState`).

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Notify;

use chronobase_core::backup::{
    BackupRepository, BackupService, DefaultBackupService, StoreBackupRepository,
    TokioCommandRunner,
};
use chronobase_core::connection::{
    ConnectionRepository, ConnectionService, DefaultConnectionService, StoreConnectionRepository,
};
use chronobase_core::error::CoreError;
use chronobase_core::schedule::{
    DefaultScheduleService, ScheduleRepository, ScheduleRunner, ScheduleService,
    StoreScheduleRepository,
};
use chronobase_core::store::JsonStore;
use chronobase_core::system::{DangerService, DefaultDangerService};
use chronobase_core::tools::{DefaultToolDiscovery, ToolDiscovery};

/// Shared application state wired once at startup and passed via `Arc`.
#[derive(Clone)]
pub struct AppState {
    /// Atomic JSON store.
    pub store: Arc<JsonStore>,
    /// Connection management service.
    pub connection_service: Arc<dyn ConnectionService>,
    /// Backup, restore, and wipe service.
    pub backup_service: Arc<dyn BackupService>,
    /// Schedule management service.
    pub schedule_service: Arc<dyn ScheduleService>,
    /// Danger Zone destructive operations service.
    pub danger_service: Arc<dyn DangerService>,
    /// Cached external CLI tool discovery.
    pub tool_discovery: Arc<dyn ToolDiscovery>,
    /// Background cron scheduler runner.
    pub schedule_runner: Arc<ScheduleRunner>,
}

impl AppState {
    /// Opens the store in `data_dir` and wires all repositories and services.
    pub fn build(data_dir: &Path) -> Result<Self, CoreError> {
        let store = Arc::new(JsonStore::open(data_dir.to_path_buf())?);
        let conn_repo: Arc<dyn ConnectionRepository> =
            Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
        let backup_repo: Arc<dyn BackupRepository> =
            Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
        let schedule_repo: Arc<dyn ScheduleRepository> =
            Arc::new(StoreScheduleRepository::new(Arc::clone(&store)));

        let tool_discovery: Arc<dyn ToolDiscovery> = Arc::new(DefaultToolDiscovery::new());
        let command_runner = Arc::new(TokioCommandRunner::new());
        let schedule_reload = Arc::new(Notify::new());

        let connection_service: Arc<dyn ConnectionService> =
            Arc::new(DefaultConnectionService::new(Arc::clone(&conn_repo)));
        let backup_service: Arc<dyn BackupService> = Arc::new(DefaultBackupService::new(
            Arc::clone(&store),
            Arc::clone(&conn_repo),
            Arc::clone(&backup_repo),
            Arc::clone(&tool_discovery),
            command_runner,
        ));
        let schedule_service: Arc<dyn ScheduleService> = Arc::new(DefaultScheduleService::new(
            Arc::clone(&schedule_repo),
            Arc::clone(&schedule_reload),
        ));
        let danger_service: Arc<dyn DangerService> =
            Arc::new(DefaultDangerService::new(Arc::clone(&store)));
        let schedule_runner = Arc::new(ScheduleRunner::new(
            schedule_repo,
            Arc::clone(&backup_service),
            schedule_reload,
        ));

        Ok(Self {
            store,
            connection_service,
            backup_service,
            schedule_service,
            danger_service,
            tool_discovery,
            schedule_runner,
        })
    }
}
