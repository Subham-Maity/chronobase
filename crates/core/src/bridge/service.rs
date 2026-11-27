//! Business logic for VPS Pull Bridge peers and one-click DB-to-DB migration.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use url::Url;

use crate::backup::{
    BackupRepository, BackupService, BackupStatus, RecordBackupInput, RestoreTarget,
};
use crate::bridge::client::pull_remote_dump;
use crate::bridge::crypto::derive_master_key;
use crate::bridge::{
    BridgeError, CreateVpsPeerInput, MigrateResult, PullResult, PullTargetMode, UpdateVpsPeerInput,
    VpsPeer, VpsPeerRepository,
};
use crate::connection::{ConnectionRepository, DbKind};
use crate::slug::slugify;
use crate::store::JsonStore;

/// Service interface for VPS peer management, remote pulls, and DB-to-DB migration.
pub trait BridgeService: Send + Sync {
    /// Lists all configured VPS peers.
    fn list_peers(&self) -> Result<Vec<VpsPeer>, BridgeError>;

    /// Gets a VPS peer by ID.
    fn get_peer(&self, id: u64) -> Result<VpsPeer, BridgeError>;

    /// Validates and creates a new VPS peer.
    fn create_peer(&self, input: CreateVpsPeerInput) -> Result<VpsPeer, BridgeError>;

    /// Validates and updates an existing VPS peer.
    fn update_peer(&self, id: u64, input: UpdateVpsPeerInput) -> Result<VpsPeer, BridgeError>;

    /// Deletes a VPS peer.
    fn delete_peer(&self, id: u64) -> Result<(), BridgeError>;

    /// Pulls a fresh backup from the specified VPS peer (by ID), verifies integrity,
    /// triggers VPS cleanup, and optionally migrates into the configured local DB.
    fn pull_from_vps(
        &self,
        peer_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<PullResult, BridgeError>> + Send + '_>>;

    /// One-click migration between two ChronoBase connections (dumps source to `backups/`
    /// and restores straight into destination DB).
    fn migrate_db_to_db(
        &self,
        source_conn_id: u64,
        dest_conn_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<MigrateResult, BridgeError>> + Send + '_>>;
}

/// Default [`BridgeService`].
pub struct DefaultBridgeService {
    store: Arc<JsonStore>,
    peer_repo: Arc<dyn VpsPeerRepository>,
    conn_repo: Arc<dyn ConnectionRepository>,
    backup_repo: Arc<dyn BackupRepository>,
    backup_service: Arc<dyn BackupService>,
}

impl DefaultBridgeService {
    /// Creates a new [`DefaultBridgeService`].
    pub fn new(
        store: Arc<JsonStore>,
        peer_repo: Arc<dyn VpsPeerRepository>,
        conn_repo: Arc<dyn ConnectionRepository>,
        backup_repo: Arc<dyn BackupRepository>,
        backup_service: Arc<dyn BackupService>,
    ) -> Self {
        Self {
            store,
            peer_repo,
            conn_repo,
            backup_repo,
            backup_service,
        }
    }

    fn validate_peer_fields(
        &self,
        name: &str,
        endpoint: &str,
        db_type: DbKind,
        remote_url: &str,
        pairing_key: &crate::secret::Secret,
        target_mode: PullTargetMode,
    ) -> Result<(), BridgeError> {
        if name.trim().is_empty() {
            return Err(BridgeError::Validation("VPS name is required".into()));
        }
        let ep = endpoint.trim();
        if ep.is_empty() || !ep.contains(':') {
            return Err(BridgeError::Validation(
                "VPS endpoint must be host:port (e.g. 127.0.0.1:9443)".into(),
            ));
        }

        let url_trimmed = remote_url.trim();
        let parsed = Url::parse(url_trimmed).map_err(|_| {
            BridgeError::Validation("Remote database URL is not a valid URL".into())
        })?;
        let scheme = parsed.scheme();
        match db_type {
            DbKind::Postgres if scheme != "postgres" && scheme != "postgresql" => {
                return Err(BridgeError::Validation(
                    "PostgreSQL URL must start with postgres:// or postgresql://".into(),
                ));
            }
            DbKind::Mongodb if scheme != "mongodb" && scheme != "mongodb+srv" => {
                return Err(BridgeError::Validation(
                    "MongoDB URL must start with mongodb:// or mongodb+srv://".into(),
                ));
            }
            _ => {}
        }

        let _ = derive_master_key(pairing_key)?;

        if let PullTargetMode::LocalConnection(conn_id) = target_mode {
            let conn = self.conn_repo.find_by_id(conn_id)?.ok_or_else(|| {
                BridgeError::Validation("Selected local target DB not found".into())
            })?;
            if conn.db_type != db_type {
                return Err(BridgeError::Validation(format!(
                    "Local target DB '{}' must match VPS engine ({})",
                    conn.name,
                    db_type.display_name()
                )));
            }
        }

        Ok(())
    }
}

impl BridgeService for DefaultBridgeService {
    fn list_peers(&self) -> Result<Vec<VpsPeer>, BridgeError> {
        self.peer_repo.list()
    }

    fn get_peer(&self, id: u64) -> Result<VpsPeer, BridgeError> {
        self.peer_repo
            .find_by_id(id)?
            .ok_or_else(|| BridgeError::NotFound(id.to_string()))
    }

    fn create_peer(&self, input: CreateVpsPeerInput) -> Result<VpsPeer, BridgeError> {
        self.validate_peer_fields(
            &input.name,
            &input.endpoint,
            input.db_type,
            input.remote_url.expose(),
            &input.pairing_key,
            input.target_mode,
        )?;
        self.peer_repo.create(input)
    }

    fn update_peer(&self, id: u64, input: UpdateVpsPeerInput) -> Result<VpsPeer, BridgeError> {
        self.validate_peer_fields(
            &input.name,
            &input.endpoint,
            input.db_type,
            input.remote_url.expose(),
            &input.pairing_key,
            input.target_mode,
        )?;
        self.peer_repo.update(id, input)
    }

    fn delete_peer(&self, id: u64) -> Result<(), BridgeError> {
        self.peer_repo.delete(id)
    }

    fn pull_from_vps(
        &self,
        peer_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<PullResult, BridgeError>> + Send + '_>> {
        Box::pin(async move {
            let peer = self.get_peer(peer_id)?;
            let group_name = format!("VPS_{}", slugify(&peer.name));

            let downloaded = match pull_remote_dump(
                self.store.backups_dir(),
                &peer.name,
                &peer.endpoint,
                peer.db_type,
                &peer.remote_url,
                &peer.pairing_key,
            )
            .await
            {
                Ok(d) => d,
                Err(err) => {
                    let _ = self.backup_repo.record(RecordBackupInput {
                        connection_id: 0,
                        connection_name: format!("VPS: {}", peer.name),
                        group_name,
                        db_type: peer.db_type,
                        file_path: String::new(),
                        file_size: 0,
                        status: BackupStatus::Failed,
                        error_msg: Some(err.to_string()),
                    });
                    return Err(err);
                }
            };

            let entry = self.backup_repo.record(RecordBackupInput {
                connection_id: 0,
                connection_name: format!("VPS: {}", peer.name),
                group_name,
                db_type: peer.db_type,
                file_path: downloaded.rel_path,
                file_size: downloaded.file_size,
                status: BackupStatus::Success,
                error_msg: None,
            })?;

            self.peer_repo.mark_last_pull(peer.id)?;

            let mut restored_to_local_db = false;
            if let PullTargetMode::LocalConnection(target_conn_id) = peer.target_mode {
                let target_conn = self.conn_repo.find_by_id(target_conn_id)?.ok_or_else(|| {
                    BridgeError::Validation("Local target database no longer exists".into())
                })?;
                if target_conn.db_type != peer.db_type {
                    return Err(BridgeError::Validation(
                        "Local target database engine does not match VPS engine".into(),
                    ));
                }
                self.backup_service
                    .restore(entry.id, RestoreTarget::MigrateDatabase(target_conn.url))
                    .await?;
                restored_to_local_db = true;
            }

            tracing::info!(
                vps_peer_id = peer.id,
                remote_deleted = downloaded.remote_deleted,
                restored_to_local_db,
                "vps pull completed"
            );

            Ok(PullResult {
                peer_id: peer.id,
                peer_name: peer.name,
                entry,
                remote_deleted: downloaded.remote_deleted,
                restored_to_local_db,
            })
        })
    }

    fn migrate_db_to_db(
        &self,
        source_conn_id: u64,
        dest_conn_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<MigrateResult, BridgeError>> + Send + '_>> {
        Box::pin(async move {
            if source_conn_id == dest_conn_id {
                return Err(BridgeError::Validation(
                    "Source and destination connections must be different".into(),
                ));
            }

            let src = self
                .conn_repo
                .find_by_id(source_conn_id)?
                .ok_or_else(|| BridgeError::Validation("Source connection not found".into()))?;
            let dst = self.conn_repo.find_by_id(dest_conn_id)?.ok_or_else(|| {
                BridgeError::Validation("Destination connection not found".into())
            })?;

            if src.db_type != dst.db_type {
                return Err(BridgeError::Validation(format!(
                    "Cannot migrate {} '{}' into {} '{}' — engines must match",
                    src.db_type.display_name(),
                    src.name,
                    dst.db_type.display_name(),
                    dst.name
                )));
            }

            let run = self.backup_service.backup_connection(src.id).await?;
            if run.entry.status != BackupStatus::Success {
                return Err(BridgeError::Backup(
                    crate::backup::BackupError::ProcessFailed(
                        run.entry
                            .error_msg
                            .unwrap_or_else(|| "Source database backup failed".into()),
                    ),
                ));
            }

            self.backup_service
                .restore(run.entry.id, RestoreTarget::MigrateDatabase(dst.url))
                .await?;

            tracing::info!(
                source_id = src.id,
                dest_id = dst.id,
                backup_id = run.entry.id,
                "db-to-db migration completed"
            );

            Ok(MigrateResult {
                source_name: src.name,
                dest_name: dst.name,
                entry: run.entry,
            })
        })
    }
}
