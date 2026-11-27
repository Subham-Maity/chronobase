//! Application dependency injection container (`AppState`).

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Notify;

use chronobase_core::backup::{
    BackupRepository, BackupService, DefaultBackupService, StoreBackupRepository,
    TokioCommandRunner,
};
use chronobase_core::bridge::{
    BridgeService, DefaultBridgeService, StoreVpsPeerRepository, VpsPeerRepository,
};
use chronobase_core::connection::{
    ConnectionRepository, ConnectionService, DefaultConnectionService, StoreConnectionRepository,
};
use chronobase_core::error::CoreError;
use chronobase_core::retention::{DefaultRetentionService, RetentionService};
use chronobase_core::schedule::{
    DefaultScheduleService, ScheduleRepository, ScheduleRunner, ScheduleService,
    StoreScheduleRepository,
};
use chronobase_core::store::JsonStore;
use chronobase_core::system::{
    DangerService, DefaultDangerService, DefaultSettingsService, SettingsService,
};
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
    /// Backup retention preview & auto-cleanup service.
    pub retention_service: Arc<dyn RetentionService>,
    /// Schedule management service.
    pub schedule_service: Arc<dyn ScheduleService>,
    /// VPS Pull Bridge and DB-to-DB migration service.
    pub bridge_service: Arc<dyn BridgeService>,
    /// Danger Zone destructive operations service.
    pub danger_service: Arc<dyn DangerService>,
    /// Desktop background tray and OS startup settings service.
    pub settings_service: Arc<dyn SettingsService>,
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
        let peer_repo: Arc<dyn VpsPeerRepository> =
            Arc::new(StoreVpsPeerRepository::new(Arc::clone(&store)));

        let tool_discovery: Arc<dyn ToolDiscovery> = Arc::new(DefaultToolDiscovery::new());
        let command_runner = Arc::new(TokioCommandRunner::new());
        let schedule_reload = Arc::new(Notify::new());
        let snapshot_op_lock = Arc::new(tokio::sync::Mutex::new(()));

        let connection_service: Arc<dyn ConnectionService> =
            Arc::new(DefaultConnectionService::new(Arc::clone(&conn_repo)));
        let backup_service: Arc<dyn BackupService> = Arc::new(
            DefaultBackupService::new(
                Arc::clone(&store),
                Arc::clone(&conn_repo),
                Arc::clone(&backup_repo),
                Arc::clone(&tool_discovery),
                command_runner,
            )
            .with_snapshot_lock(Arc::clone(&snapshot_op_lock)),
        );
        let retention_service: Arc<dyn RetentionService> = Arc::new(DefaultRetentionService::new(
            Arc::clone(&store),
            Arc::clone(&conn_repo),
            Arc::clone(&backup_service),
            snapshot_op_lock,
        ));
        let schedule_service: Arc<dyn ScheduleService> = Arc::new(DefaultScheduleService::new(
            Arc::clone(&schedule_repo),
            Arc::clone(&schedule_reload),
        ));
        let bridge_service: Arc<dyn BridgeService> = Arc::new(DefaultBridgeService::new(
            Arc::clone(&store),
            peer_repo,
            conn_repo,
            backup_repo,
            Arc::clone(&backup_service),
            Arc::clone(&tool_discovery),
        ));
        let danger_service: Arc<dyn DangerService> =
            Arc::new(DefaultDangerService::new(Arc::clone(&store)));
        let settings_service: Arc<dyn SettingsService> = Arc::new(
            DefaultSettingsService::new(Arc::clone(&store))
                .with_schedule_reload(Arc::clone(&schedule_reload)),
        );
        let schedule_runner = Arc::new(
            ScheduleRunner::new(schedule_repo, Arc::clone(&bridge_service), schedule_reload)
                .with_retention(Arc::clone(&store), Arc::clone(&retention_service)),
        );

        Ok(Self {
            store,
            connection_service,
            backup_service,
            retention_service,
            schedule_service,
            bridge_service,
            danger_service,
            settings_service,
            tool_discovery,
            schedule_runner,
        })
    }
}
