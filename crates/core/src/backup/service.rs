//! Business logic for backup execution, restore, database wipe, and snapshot deletion.

use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Local;

use crate::backup::engine::{
    CommandRunner, CommandSpec, TempSecretFile, build_mongodump_spec, build_mongorestore_spec,
    build_pg_dump_spec, build_psql_restore_spec, build_psql_wipe_spec, is_config_flag_unsupported,
};
use crate::backup::{
    BackupEntry, BackupError, BackupRepository, BackupRunResult, BackupStatus, RecordBackupInput,
    RestoreTarget,
};
use crate::connection::{Connection, ConnectionRepository, DbKind};
use crate::constant::{BACKUP_TIMEOUT, WIPE_TIMEOUT};
use crate::slug::slugify;
use crate::store::JsonStore;
use crate::tools::ToolDiscovery;

/// Service interface for backups, restores, wipes, and history queries.
pub trait BackupService: Send + Sync {
    /// Lists backup history filtered by optional group, type, and status.
    fn list_filtered(
        &self,
        group: Option<&str>,
        db_type: Option<DbKind>,
        status: Option<BackupStatus>,
    ) -> Result<Vec<BackupEntry>, BackupError>;

    /// Runs a backup for a single connection ID, always recording success or failure.
    fn backup_connection(
        &self,
        connection_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<BackupRunResult, BackupError>> + Send + '_>>;

    /// Runs backups sequentially for all connections in `group_name` (or `"All"`).
    fn backup_group(
        &self,
        group_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<BackupRunResult>, BackupError>> + Send + '_>>;

    /// Restores a snapshot to the same database or a different target URL.
    fn restore(
        &self,
        backup_id: u64,
        target: RestoreTarget,
    ) -> Pin<Box<dyn Future<Output = Result<(), BackupError>> + Send + '_>>;

    /// Wipes all tables/collections inside the target connection's database.
    fn wipe_database(
        &self,
        connection_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<(), BackupError>> + Send + '_>>;

    /// Deletes a backup record and its artifact on disk (after path-containment check).
    fn delete_backup(&self, backup_id: u64) -> Result<(), BackupError>;
}

/// Default [`BackupService`] wiring repositories, [`ToolDiscovery`], and [`CommandRunner`].
pub struct DefaultBackupService {
    store: Arc<JsonStore>,
    conn_repo: Arc<dyn ConnectionRepository>,
    backup_repo: Arc<dyn BackupRepository>,
    tools: Arc<dyn ToolDiscovery>,
    runner: Arc<dyn CommandRunner>,
}

impl DefaultBackupService {
    /// Creates a new [`DefaultBackupService`].
    pub fn new(
        store: Arc<JsonStore>,
        conn_repo: Arc<dyn ConnectionRepository>,
        backup_repo: Arc<dyn BackupRepository>,
        tools: Arc<dyn ToolDiscovery>,
        runner: Arc<dyn CommandRunner>,
    ) -> Self {
        Self {
            store,
            conn_repo,
            backup_repo,
            tools,
            runner,
        }
    }

    async fn run_backup_for_conn(&self, conn: &Connection) -> BackupRunResult {
        match self.execute_backup(conn).await {
            Ok((rel_path, size)) => {
                let entry = self
                    .backup_repo
                    .record(RecordBackupInput {
                        connection_id: conn.id,
                        connection_name: conn.name.clone(),
                        group_name: conn.group_name.clone(),
                        db_type: conn.db_type,
                        file_path: rel_path,
                        file_size: size,
                        status: BackupStatus::Success,
                        error_msg: None,
                    })
                    .unwrap_or_else(|e| failed_placeholder(conn, e.to_string()));
                tracing::info!(
                    connection_id = conn.id,
                    backup_id = entry.id,
                    size = entry.file_size,
                    "backup completed"
                );
                BackupRunResult {
                    connection_id: conn.id,
                    connection_name: conn.name.clone(),
                    entry,
                }
            }
            Err(err) => {
                let msg = err.to_string();
                tracing::error!(connection_id = conn.id, error = %msg, "backup failed");
                let entry = self
                    .backup_repo
                    .record(RecordBackupInput {
                        connection_id: conn.id,
                        connection_name: conn.name.clone(),
                        group_name: conn.group_name.clone(),
                        db_type: conn.db_type,
                        file_path: String::new(),
                        file_size: 0,
                        status: BackupStatus::Failed,
                        error_msg: Some(msg),
                    })
                    .unwrap_or_else(|e| failed_placeholder(conn, e.to_string()));
                BackupRunResult {
                    connection_id: conn.id,
                    connection_name: conn.name.clone(),
                    entry,
                }
            }
        }
    }

    async fn execute_backup(&self, conn: &Connection) -> Result<(String, u64), BackupError> {
        let group_slug = slugify(&conn.group_name);
        let name_slug = slugify(&conn.name);
        let stamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let base_name = format!("{name_slug}_{stamp}");

        let group_dir = self.store.backups_dir().join(&group_slug);
        fs::create_dir_all(&group_dir)?;

        let tool_status = self.tools.status().await;

        match conn.db_type {
            DbKind::Postgres => {
                let bin = tool_status
                    .pg_dump
                    .ok_or(BackupError::ToolMissing { tool: "pg_dump" })?;
                let file_name = format!("{base_name}.sql");
                let full_path = group_dir.join(&file_name);
                let spec = build_pg_dump_spec(bin, conn.url.expose(), &full_path, BACKUP_TIMEOUT)?;
                let out = self.runner.run(spec).await?;
                if !out.success {
                    // Clean up partial dump file on failure.
                    let _ = fs::remove_file(&full_path);
                    return Err(BackupError::ProcessFailed(non_empty_err(
                        out.stderr_tail,
                        "pg_dump failed",
                    )));
                }
                let size = path_size(&full_path);
                let rel_path = format!("{group_slug}/{file_name}");
                Ok((rel_path, size))
            }
            DbKind::Mongodb => {
                let bin = tool_status
                    .mongodump
                    .ok_or(BackupError::ToolMissing { tool: "mongodump" })?;
                let full_dir = group_dir.join(&base_name);
                let yaml_cfg = format!("uri: \"{}\"\n", escape_yaml(conn.url.expose()));
                let cfg_file = TempSecretFile::create(
                    self.store.data_dir(),
                    "mongodump_cfg",
                    "yaml",
                    &yaml_cfg,
                )?;

                let spec = build_mongodump_spec(
                    bin.clone(),
                    Some(cfg_file.path()),
                    conn.url.expose(),
                    &full_dir,
                    BACKUP_TIMEOUT,
                );
                let mut out = self.runner.run(spec).await?;
                if !out.success && is_config_flag_unsupported(&out.stderr_tail) {
                    let fallback = build_mongodump_spec(
                        bin,
                        None,
                        conn.url.expose(),
                        &full_dir,
                        BACKUP_TIMEOUT,
                    );
                    out = self.runner.run(fallback).await?;
                }
                drop(cfg_file);

                if !out.success {
                    let _ = fs::remove_dir_all(&full_dir);
                    return Err(BackupError::ProcessFailed(non_empty_err(
                        out.stderr_tail,
                        "mongodump failed",
                    )));
                }
                let size = path_size(&full_dir);
                let rel_path = format!("{group_slug}/{base_name}");
                Ok((rel_path, size))
            }
        }
    }
}

impl BackupService for DefaultBackupService {
    fn list_filtered(
        &self,
        group: Option<&str>,
        db_type: Option<DbKind>,
        status: Option<BackupStatus>,
    ) -> Result<Vec<BackupEntry>, BackupError> {
        let mut entries = self.backup_repo.list()?;
        for entry in &mut entries {
            if entry.status == BackupStatus::Success {
                let exists = self
                    .store
                    .resolve_backup_path(&entry.file_path)
                    .map(|p| p.exists())
                    .unwrap_or(false);
                entry.missing_on_disk = !exists;
            }
        }
        Ok(entries
            .into_iter()
            .filter(|b| {
                group.is_none_or(|g| g.is_empty() || b.group_name == g)
                    && db_type.is_none_or(|t| b.db_type == t)
                    && status.is_none_or(|s| b.status == s)
            })
            .collect())
    }

    fn backup_connection(
        &self,
        connection_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<BackupRunResult, BackupError>> + Send + '_>> {
        Box::pin(async move {
            let conn = self
                .conn_repo
                .find_by_id(connection_id)
                .map_err(|e| BackupError::Validation(e.to_string()))?
                .ok_or(BackupError::ConnectionNotFound(connection_id))?;
            Ok(self.run_backup_for_conn(&conn).await)
        })
    }

    fn backup_group(
        &self,
        group_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<BackupRunResult>, BackupError>> + Send + '_>> {
        let group = group_name.to_string();
        Box::pin(async move {
            let conns = self
                .conn_repo
                .list_by_group(&group)
                .map_err(|e| BackupError::Validation(e.to_string()))?;
            let mut results = Vec::with_capacity(conns.len());
            for conn in &conns {
                results.push(self.run_backup_for_conn(conn).await);
            }
            Ok(results)
        })
    }

    fn restore(
        &self,
        backup_id: u64,
        target: RestoreTarget,
    ) -> Pin<Box<dyn Future<Output = Result<(), BackupError>> + Send + '_>> {
        Box::pin(async move {
            let entry = self
                .backup_repo
                .find_by_id(backup_id)?
                .ok_or(BackupError::NotFound(backup_id))?;

            let disk_path = self.store.resolve_backup_path(&entry.file_path)?;
            if !disk_path.exists() {
                return Err(BackupError::FileMissingOnDisk);
            }

            let (target_url, is_same_db) = match target {
                RestoreTarget::SameDatabase => {
                    let conn = self
                        .conn_repo
                        .find_by_id(entry.connection_id)
                        .map_err(|e| BackupError::Validation(e.to_string()))?
                        .ok_or_else(|| {
                            BackupError::Validation(
                                "Original connection not found — use Different Database instead"
                                    .to_string(),
                            )
                        })?;
                    (conn.url, true)
                }
                RestoreTarget::DifferentDatabase(url) => {
                    if url.is_blank() {
                        return Err(BackupError::Validation(
                            "Target database URL is required".to_string(),
                        ));
                    }
                    (url, false)
                }
            };

            let tool_status = self.tools.status().await;

            match entry.db_type {
                DbKind::Postgres => {
                    let bin = tool_status
                        .psql
                        .ok_or(BackupError::ToolMissing { tool: "psql" })?;
                    let spec = build_psql_restore_spec(
                        bin,
                        target_url.expose(),
                        &disk_path,
                        BACKUP_TIMEOUT,
                    )?;
                    let out = self.runner.run(spec).await?;
                    if !out.success {
                        return Err(BackupError::ProcessFailed(non_empty_err(
                            out.stderr_tail,
                            "psql restore failed",
                        )));
                    }
                }
                DbKind::Mongodb => {
                    let bin = tool_status.mongorestore.ok_or(BackupError::ToolMissing {
                        tool: "mongorestore",
                    })?;
                    let yaml_cfg = format!("uri: \"{}\"\n", escape_yaml(target_url.expose()));
                    let cfg_file = TempSecretFile::create(
                        self.store.data_dir(),
                        "mongorestore_cfg",
                        "yaml",
                        &yaml_cfg,
                    )?;

                    let spec = build_mongorestore_spec(
                        bin.clone(),
                        Some(cfg_file.path()),
                        target_url.expose(),
                        &disk_path,
                        is_same_db,
                        BACKUP_TIMEOUT,
                    );
                    let mut out = self.runner.run(spec).await?;
                    if !out.success && is_config_flag_unsupported(&out.stderr_tail) {
                        let fallback = build_mongorestore_spec(
                            bin,
                            None,
                            target_url.expose(),
                            &disk_path,
                            is_same_db,
                            BACKUP_TIMEOUT,
                        );
                        out = self.runner.run(fallback).await?;
                    }
                    drop(cfg_file);

                    if !out.success {
                        return Err(BackupError::ProcessFailed(non_empty_err(
                            out.stderr_tail,
                            "mongorestore failed",
                        )));
                    }
                }
            }

            tracing::info!(backup_id, is_same_db, "restore completed");
            Ok(())
        })
    }

    fn wipe_database(
        &self,
        connection_id: u64,
    ) -> Pin<Box<dyn Future<Output = Result<(), BackupError>> + Send + '_>> {
        Box::pin(async move {
            let conn = self
                .conn_repo
                .find_by_id(connection_id)
                .map_err(|e| BackupError::Validation(e.to_string()))?
                .ok_or(BackupError::ConnectionNotFound(connection_id))?;

            let tool_status = self.tools.status().await;

            match conn.db_type {
                DbKind::Postgres => {
                    let bin = tool_status
                        .psql
                        .ok_or(BackupError::ToolMissing { tool: "psql" })?;
                    let spec = build_psql_wipe_spec(bin, conn.url.expose(), WIPE_TIMEOUT)?;
                    let out = self.runner.run(spec).await?;
                    if !out.success {
                        return Err(BackupError::ProcessFailed(non_empty_err(
                            out.stderr_tail,
                            "psql wipe failed",
                        )));
                    }
                }
                DbKind::Mongodb => {
                    let bin = tool_status
                        .mongosh
                        .ok_or(BackupError::ToolMissing { tool: "mongosh" })?;
                    // Security: write a 0600 temporary JS script so the MongoDB URI
                    // is never exposed in argv.
                    let script = format!(
                        "const db = connect({}); db.getCollectionNames().forEach(c => db.getCollection(c).drop());\n",
                        serde_json::to_string(conn.url.expose()).unwrap_or_default()
                    );
                    let js_file = TempSecretFile::create(
                        self.store.data_dir(),
                        "mongosh_wipe",
                        "js",
                        &script,
                    )?;
                    let spec = CommandSpec {
                        program: bin,
                        args: vec![
                            "--nodb".to_string(),
                            "--quiet".to_string(),
                            "--file".to_string(),
                            js_file.path().to_string_lossy().into_owned(),
                        ],
                        env: Vec::new(),
                        timeout: WIPE_TIMEOUT,
                    };
                    let out = self.runner.run(spec).await?;
                    drop(js_file);
                    if !out.success {
                        return Err(BackupError::ProcessFailed(non_empty_err(
                            out.stderr_tail,
                            "mongosh wipe failed",
                        )));
                    }
                }
            }

            tracing::info!(connection_id, "database wiped");
            Ok(())
        })
    }

    fn delete_backup(&self, backup_id: u64) -> Result<(), BackupError> {
        let entry = self
            .backup_repo
            .find_by_id(backup_id)?
            .ok_or(BackupError::NotFound(backup_id))?;

        if !entry.file_path.trim().is_empty() {
            let safe_path = self
                .store
                .resolve_backup_path_for_delete(&entry.file_path)?;
            if safe_path.exists() {
                if safe_path.is_dir() {
                    fs::remove_dir_all(&safe_path)?;
                } else {
                    fs::remove_file(&safe_path)?;
                }
            }
        }

        self.backup_repo.delete(backup_id)?;
        tracing::info!(backup_id, "backup deleted");
        Ok(())
    }
}

fn failed_placeholder(conn: &Connection, msg: String) -> BackupEntry {
    BackupEntry {
        id: 0,
        connection_id: conn.id,
        connection_name: conn.name.clone(),
        group_name: conn.group_name.clone(),
        db_type: conn.db_type,
        file_path: String::new(),
        file_size: 0,
        status: BackupStatus::Failed,
        error_msg: Some(msg),
        created_at: crate::store::local_timestamp(),
        missing_on_disk: false,
    }
}

fn non_empty_err(stderr: String, fallback: &str) -> String {
    if stderr.trim().is_empty() {
        fallback.to_string()
    } else {
        stderr
    }
}

fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn path_size(path: &Path) -> u64 {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|e| path_size(&e.path())).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::CommandOutput;
    use crate::backup::StoreBackupRepository;
    use crate::backup::engine::MockCommandRunner;
    use crate::connection::{CreateConnectionInput, StoreConnectionRepository};
    use crate::secret::Secret;
    use crate::tools::ToolStatus;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct StubTools(ToolStatus);
    impl ToolDiscovery for StubTools {
        fn status(&self) -> Pin<Box<dyn Future<Output = ToolStatus> + Send + '_>> {
            let s = self.0.clone();
            Box::pin(async move { s })
        }
        fn refresh(&self) -> Pin<Box<dyn Future<Output = ToolStatus> + Send + '_>> {
            self.status()
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chronobase_bk_svc_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp");
        dir
    }

    #[tokio::test]
    async fn failed_backup_records_error_msg() {
        let dir = temp_dir("fail_msg");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let conn_repo = Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
        let bk_repo = Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
        let conn = conn_repo
            .create(CreateConnectionInput {
                name: "Prod".into(),
                group_name: "MySaas".into(),
                db_type: DbKind::Postgres,
                url: Secret::new("postgresql://u:secretpw@localhost:5432/db"),
                notes: "".into(),
            })
            .expect("conn");

        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(|spec| {
            assert!(
                spec.args.iter().all(|a| !a.contains("secretpw")),
                "password must not be in argv"
            );
            Box::pin(async {
                Ok(CommandOutput {
                    success: false,
                    exit_code: Some(1),
                    stderr_tail: "connection refused".into(),
                })
            })
        });

        let tools = Arc::new(StubTools(ToolStatus {
            pg_dump: Some(PathBuf::from("pg_dump")),
            ..Default::default()
        }));

        let svc = DefaultBackupService::new(
            Arc::clone(&store),
            conn_repo,
            bk_repo,
            tools,
            Arc::new(mock),
        );

        let res = svc.backup_connection(conn.id).await.expect("run");
        assert_eq!(res.entry.status, BackupStatus::Failed);
        assert_eq!(res.entry.error_msg.as_deref(), Some("connection refused"));

        drop(svc);
        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
