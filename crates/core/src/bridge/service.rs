//! Business logic for VPS Pull Bridge peers and one-click DB-to-DB migration.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;

use tokio::process::Command;
use url::Url;

use crate::backup::{
    BackupRepository, BackupRunResult, BackupService, BackupSourceKind, BackupStatus,
    RecordBackupInput, RestoreTarget,
};
use crate::bridge::client::pull_remote_dump;
use crate::bridge::crypto::derive_master_key;
use crate::bridge::{
    BridgeError, CreateVpsPeerInput, MigrateEndpoint, MigrateResult, PullResult, PullTargetMode,
    UpdateVpsPeerInput, VpsPeer, VpsPeerRepository,
};
use crate::connection::{ConnectionRepository, DbKind};
use crate::constant::{ALL_GROUPS, DEFAULT_GROUP, PROBE_TIMEOUT};
use crate::platform;
use crate::secret::split_postgres_url;
use crate::store::{JsonStore, local_timestamp};
use crate::tools::{ToolDiscovery, parse_major_version};

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

    /// Runs backups for all direct connections AND pulls all VPS peers matching `group_name` (or `"All"`).
    fn backup_all_in_group(
        &self,
        group_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<BackupRunResult>, BridgeError>> + Send + '_>>;

    /// One-click migration between two endpoints (`Connection -> Connection` or `VpsPeer -> Connection`)
    /// with engine and major-version preflight checks.
    fn migrate_endpoints(
        &self,
        source: MigrateEndpoint,
        dest: MigrateEndpoint,
    ) -> Pin<Box<dyn Future<Output = Result<MigrateResult, BridgeError>> + Send + '_>>;

    /// One-click migration between two direct ChronoBase connections (dumps source to `backups/`
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
    tools: Arc<dyn ToolDiscovery>,
}

impl DefaultBridgeService {
    /// Creates a new [`DefaultBridgeService`].
    pub fn new(
        store: Arc<JsonStore>,
        peer_repo: Arc<dyn VpsPeerRepository>,
        conn_repo: Arc<dyn ConnectionRepository>,
        backup_repo: Arc<dyn BackupRepository>,
        backup_service: Arc<dyn BackupService>,
        tools: Arc<dyn ToolDiscovery>,
    ) -> Self {
        Self {
            store,
            peer_repo,
            conn_repo,
            backup_repo,
            backup_service,
            tools,
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
            let group_name = if peer.group_name.trim().is_empty() {
                DEFAULT_GROUP.to_string()
            } else {
                peer.group_name.trim().to_string()
            };

            let downloaded = match pull_remote_dump(
                self.store.backups_dir(),
                &group_name,
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
                        connection_id: peer.id,
                        connection_name: format!("VPS: {}", peer.name),
                        group_name,
                        db_type: peer.db_type,
                        source_kind: BackupSourceKind::VpsPull,
                        file_path: String::new(),
                        file_size: 0,
                        status: BackupStatus::Failed,
                        error_msg: Some(err.to_string()),
                    });
                    return Err(err);
                }
            };

            let entry = self.backup_repo.record(RecordBackupInput {
                connection_id: peer.id,
                connection_name: format!("VPS: {}", peer.name),
                group_name,
                db_type: peer.db_type,
                source_kind: BackupSourceKind::VpsPull,
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

    fn backup_all_in_group(
        &self,
        group_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<BackupRunResult>, BridgeError>> + Send + '_>> {
        let group_owned = group_name.to_string();
        Box::pin(async move {
            let mut results = self.backup_service.backup_group(&group_owned).await?;
            let peers = self.peer_repo.list()?;
            let all = group_owned.eq_ignore_ascii_case(ALL_GROUPS);

            for peer in peers {
                if !all && !peer.group_name.eq_ignore_ascii_case(&group_owned) {
                    continue;
                }
                let conn_name = format!("VPS: {}", peer.name);
                match self.pull_from_vps(peer.id).await {
                    Ok(res) => results.push(BackupRunResult {
                        connection_id: peer.id,
                        connection_name: conn_name,
                        entry: res.entry,
                    }),
                    Err(err) => {
                        results.push(BackupRunResult {
                            connection_id: peer.id,
                            connection_name: conn_name.clone(),
                            entry: crate::backup::BackupEntry {
                                id: 0,
                                connection_id: peer.id,
                                connection_name: conn_name,
                                group_name: peer.group_name,
                                db_type: peer.db_type,
                                source_kind: BackupSourceKind::VpsPull,
                                file_path: String::new(),
                                file_size: 0,
                                status: BackupStatus::Failed,
                                error_msg: Some(err.to_string()),
                                created_at: local_timestamp(),
                                missing_on_disk: false,
                            },
                        });
                    }
                }
            }

            Ok(results)
        })
    }

    fn migrate_endpoints(
        &self,
        source: MigrateEndpoint,
        dest: MigrateEndpoint,
    ) -> Pin<Box<dyn Future<Output = Result<MigrateResult, BridgeError>> + Send + '_>> {
        Box::pin(async move {
            if source == dest {
                return Err(BridgeError::Validation(
                    "Source and destination must be different".into(),
                ));
            }
            let dest_conn_id = match dest {
                MigrateEndpoint::Connection(id) => id,
                MigrateEndpoint::VpsPeer(_) => {
                    return Err(BridgeError::Validation(
                        "Migration destination must be a local/direct database connection".into(),
                    ));
                }
            };
            let dst = self.conn_repo.find_by_id(dest_conn_id)?.ok_or_else(|| {
                BridgeError::Validation("Destination connection not found".into())
            })?;

            match source {
                MigrateEndpoint::Connection(src_id) => {
                    let src = self.conn_repo.find_by_id(src_id)?.ok_or_else(|| {
                        BridgeError::Validation("Source connection not found".into())
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

                    if src.db_type == DbKind::Postgres {
                        self.preflight_pg_versions(Some(src.url.expose()), dst.url.expose())
                            .await?;
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
                }
                MigrateEndpoint::VpsPeer(peer_id) => {
                    let peer = self.get_peer(peer_id)?;
                    if peer.db_type != dst.db_type {
                        return Err(BridgeError::Validation(format!(
                            "Cannot migrate {} VPS '{}' into {} '{}' — engines must match",
                            peer.db_type.display_name(),
                            peer.name,
                            dst.db_type.display_name(),
                            dst.name
                        )));
                    }

                    let group_name = if peer.group_name.trim().is_empty() {
                        DEFAULT_GROUP.to_string()
                    } else {
                        peer.group_name.trim().to_string()
                    };

                    let downloaded = pull_remote_dump(
                        self.store.backups_dir(),
                        &group_name,
                        &peer.name,
                        &peer.endpoint,
                        peer.db_type,
                        &peer.remote_url,
                        &peer.pairing_key,
                    )
                    .await?;

                    let entry = self.backup_repo.record(RecordBackupInput {
                        connection_id: peer.id,
                        connection_name: format!("VPS: {}", peer.name),
                        group_name,
                        db_type: peer.db_type,
                        source_kind: BackupSourceKind::VpsPull,
                        file_path: downloaded.rel_path,
                        file_size: downloaded.file_size,
                        status: BackupStatus::Success,
                        error_msg: None,
                    })?;

                    self.peer_repo.mark_last_pull(peer.id)?;
                    self.backup_service
                        .restore(entry.id, RestoreTarget::MigrateDatabase(dst.url))
                        .await?;

                    tracing::info!(
                        vps_peer_id = peer.id,
                        dest_id = dst.id,
                        backup_id = entry.id,
                        "vps-to-db migration completed"
                    );

                    Ok(MigrateResult {
                        source_name: format!("VPS: {}", peer.name),
                        dest_name: dst.name,
                        entry,
                    })
                }
            }
        })
    }

    fn migrate_db_to_db(
        &self,
        source_conn_id: u64,
        dest_conn_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<MigrateResult, BridgeError>> + Send + '_>> {
        self.migrate_endpoints(
            MigrateEndpoint::Connection(source_conn_id),
            MigrateEndpoint::Connection(dest_conn_id),
        )
    }
}

impl DefaultBridgeService {
    async fn preflight_pg_versions(
        &self,
        source_url: Option<&str>,
        dest_url: &str,
    ) -> Result<(), BridgeError> {
        let tools = self.tools.status().await;
        let Some(psql_bin) = tools.psql.as_deref() else {
            return Ok(());
        };
        let pg_dump_major = tools
            .versions
            .pg_dump
            .as_deref()
            .and_then(parse_major_version);

        let src_major = match source_url {
            Some(url) => probe_pg_server_major(psql_bin, url).await,
            None => None,
        };
        let dst_major = probe_pg_server_major(psql_bin, dest_url).await;

        if let (Some(dump_m), Some(src_m)) = (pg_dump_major, src_major)
            && dump_m < src_m
        {
            return Err(BridgeError::Validation(format!(
                "Version mismatch: local pg_dump v{dump_m} is older than source PostgreSQL v{src_m}. Upgrade PostgreSQL client tools."
            )));
        }

        if let (Some(src_m), Some(dst_m)) = (src_major, dst_major)
            && src_m > dst_m
        {
            return Err(BridgeError::Validation(format!(
                "Version mismatch: source PostgreSQL v{src_m} is newer than destination PostgreSQL v{dst_m}. Destination major version must be >= source."
            )));
        }

        Ok(())
    }
}

async fn probe_pg_server_major(psql_bin: &Path, raw_url: &str) -> Option<u32> {
    let (clean_url, password) = split_postgres_url(raw_url).ok()?;
    let mut cmd = Command::new(psql_bin);
    cmd.args([clean_url.as_str(), "-Atc", "SHOW server_version_num;"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(pass) = password {
        cmd.env("PGPASSWORD", pass);
    }
    platform::configure_background_command(&mut cmd);

    let out = tokio::time::timeout(PROBE_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let num = text.trim().parse::<u32>().ok()?;
    Some(num / 10_000)
}
