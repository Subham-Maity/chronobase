//! Integration tests for end-to-end backup -> list -> delete flow.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chronobase_core::backup::{
    BackupService, BackupStatus, DefaultBackupService, StoreBackupRepository, TokioCommandRunner,
};
use chronobase_core::connection::{
    ConnectionRepository, ConnectionService, CreateConnectionInput, DbKind,
    DefaultConnectionService, StoreConnectionRepository,
};
use chronobase_core::secret::Secret;
use chronobase_core::store::JsonStore;
use chronobase_core::tools::{ToolDiscovery, ToolStatus};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("chronobase_int_{label}_{nanos}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn create_stub_pg_dump(dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let script = dir.join("stub_pg_dump.cmd");
        // Finds "-f" argument and writes a dummy SQL file at the next argument.
        let content = "@echo off\r\n:loop\r\nif \"%~1\"==\"\" goto done\r\nif \"%~1\"==\"-f\" (\r\n  echo -- stub sql dump > \"%~2\"\r\n  exit /b 0\r\n)\r\nshift\r\ngoto loop\r\n:done\r\nexit /b 1\r\n";
        fs::write(&script, content).expect("write stub cmd");
        script
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("stub_pg_dump.sh");
        let content = "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"-f\" ]; then\n    echo \"-- stub sql dump\" > \"$2\"\n    exit 0\n  fi\n  shift\ndone\nexit 1\n";
        fs::write(&script, content).expect("write stub sh");
        let mut perms = fs::metadata(&script).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
        script
    }
}

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

#[tokio::test]
async fn full_backup_list_and_delete_flow_with_stub_executable() {
    let dir = temp_dir("flow");
    let stub_bin = create_stub_pg_dump(&dir);

    let store = Arc::new(JsonStore::open(dir.clone()).expect("open store"));
    let conn_repo = Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
    let bk_repo = Arc::new(StoreBackupRepository::new(Arc::clone(&store)));

    let conn_svc = DefaultConnectionService::new(Arc::clone(&conn_repo) as _);
    let conn = conn_svc
        .create(CreateConnectionInput {
            name: "IntegrationDB".into(),
            group_name: "Prod".into(),
            db_type: DbKind::Postgres,
            url: Secret::new("postgresql://user:pass123@localhost:5432/mydb"),
            notes: "".into(),
        })
        .expect("create connection");

    let tools = Arc::new(StubTools(ToolStatus {
        pg_dump: Some(stub_bin),
        ..Default::default()
    }));
    let bk_svc = DefaultBackupService::new(
        Arc::clone(&store),
        conn_repo,
        bk_repo,
        tools,
        Arc::new(TokioCommandRunner::new()),
    );

    let run = bk_svc
        .backup_connection(conn.id)
        .await
        .expect("backup connection");
    assert_eq!(run.entry.status, BackupStatus::Success);
    assert!(run.entry.file_size > 0);
    assert!(run.entry.file_path.starts_with("Prod/IntegrationDB_"));

    let listed = bk_svc.list_filtered(None, None, None).expect("list");
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].missing_on_disk);

    let disk_path = store
        .resolve_backup_path(&listed[0].file_path)
        .expect("resolve");
    assert!(disk_path.exists());

    bk_svc.delete_backup(listed[0].id).expect("delete");
    assert!(!disk_path.exists());
    assert!(
        bk_svc
            .list_filtered(None, None, None)
            .expect("list")
            .is_empty()
    );

    drop(bk_svc);
    drop(store);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn opt_in_real_postgres_backup_if_env_set() {
    let Ok(url) = std::env::var("CHRONOBASE_TEST_PG_URL") else {
        return;
    };
    let dir = temp_dir("real_pg");
    let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
    let conn_repo = Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
    let bk_repo = Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
    let tools = Arc::new(chronobase_core::tools::DefaultToolDiscovery::new());
    let conn = conn_repo
        .create(CreateConnectionInput {
            name: "RealPG".into(),
            group_name: "Test".into(),
            db_type: DbKind::Postgres,
            url: Secret::new(url),
            notes: "".into(),
        })
        .expect("conn");
    let bk_svc = DefaultBackupService::new(
        Arc::clone(&store),
        conn_repo,
        bk_repo,
        tools,
        Arc::new(TokioCommandRunner::new()),
    );
    let res = bk_svc.backup_connection(conn.id).await.expect("backup");
    assert_eq!(res.entry.status, BackupStatus::Success);
    drop(bk_svc);
    drop(store);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn opt_in_real_mongo_backup_if_env_set() {
    let Ok(url) = std::env::var("CHRONOBASE_TEST_MONGO_URL") else {
        return;
    };
    let dir = temp_dir("real_mongo");
    let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
    let conn_repo = Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
    let bk_repo = Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
    let tools = Arc::new(chronobase_core::tools::DefaultToolDiscovery::new());
    let conn = conn_repo
        .create(CreateConnectionInput {
            name: "RealMongo".into(),
            group_name: "Test".into(),
            db_type: DbKind::Mongodb,
            url: Secret::new(url),
            notes: "".into(),
        })
        .expect("conn");
    let bk_svc = DefaultBackupService::new(
        Arc::clone(&store),
        conn_repo,
        bk_repo,
        tools,
        Arc::new(TokioCommandRunner::new()),
    );
    let res = bk_svc.backup_connection(conn.id).await.expect("backup");
    assert_eq!(res.entry.status, BackupStatus::Success);
    drop(bk_svc);
    drop(store);
    let _ = fs::remove_dir_all(&dir);
}
