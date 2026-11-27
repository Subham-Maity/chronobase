//! Integration tests for end-to-end backup -> list -> delete flow.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chronobase_core::backup::{
    BackupRepository, BackupService, BackupSourceKind, BackupStatus, DefaultBackupService,
    RecordBackupInput, StoreBackupRepository, TokioCommandRunner,
};
use chronobase_core::connection::{
    ConnectionRepository, ConnectionService, CreateConnectionInput, DbKind,
    DefaultConnectionService, StoreConnectionRepository,
};
use chronobase_core::retention::{
    CleanupFrequency, DefaultRetentionService, RetentionPolicy, RetentionService,
};
use chronobase_core::secret::Secret;
use chronobase_core::store::JsonStore;
use chronobase_core::system::{DefaultSettingsService, SettingsService};
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
            retention: None,
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
    assert!(run.entry.file_path.starts_with("Prod/"));
    assert!(run.entry.file_path.contains("/IntegrationDB_"));

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
            retention: None,
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
            retention: None,
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
async fn retention_deletes_old_files_keeps_newest_and_blocks_path_traversal() {
    let dir = temp_dir("retention_flow");
    let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
    let conn_repo = Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
    let bk_repo = Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
    let tools = Arc::new(StubTools(ToolStatus::default()));
    let op_lock = Arc::new(tokio::sync::Mutex::new(()));

    let conn = conn_repo
        .create(CreateConnectionInput {
            name: "RetDB".into(),
            group_name: "Prod".into(),
            db_type: DbKind::Postgres,
            url: Secret::new("postgresql://u:p@localhost:5432/ret"),
            notes: "".into(),
            retention: None,
        })
        .expect("conn");

    // Create 3 real backup files on disk with >1h old timestamps.
    let bk_dir = store.backups_dir().join("Prod").join("2026-11-01");
    fs::create_dir_all(&bk_dir).expect("mkdir");
    let f_new = bk_dir.join("RetDB_newest.sql");
    let f_old = bk_dir.join("RetDB_oldest.sql");
    fs::write(&f_new, "-- newest").expect("write new");
    fs::write(&f_old, "-- oldest").expect("write old");

    // Also create a secret file outside backups/ to test path traversal refusal.
    let outside_file = dir.join("outside_secret.txt");
    fs::write(&outside_file, "do not delete").expect("write outside");

    // Record newest, oldest, and traversal entry, then backdate created_at (> 1h ago).
    let e_new = bk_repo
        .record(RecordBackupInput {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            group_name: "Prod".into(),
            db_type: DbKind::Postgres,
            source_kind: BackupSourceKind::Direct,
            file_path: "Prod/2026-11-01/RetDB_newest.sql".into(),
            file_size: 9,
            status: BackupStatus::Success,
            error_msg: None,
        })
        .expect("rec new");
    let e_old = bk_repo
        .record(RecordBackupInput {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            group_name: "Prod".into(),
            db_type: DbKind::Postgres,
            source_kind: BackupSourceKind::Direct,
            file_path: "Prod/2026-11-01/RetDB_oldest.sql".into(),
            file_size: 9,
            status: BackupStatus::Success,
            error_msg: None,
        })
        .expect("rec old");
    let e_evil = bk_repo
        .record(RecordBackupInput {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            group_name: "Prod".into(),
            db_type: DbKind::Postgres,
            source_kind: BackupSourceKind::Direct,
            file_path: "../outside_secret.txt".into(),
            file_size: 13,
            status: BackupStatus::Success,
            error_msg: None,
        })
        .expect("rec evil");

    store.mutate_with(|d| {
        for b in &mut d.backups {
            if b.id == e_new.id {
                b.created_at = "2026-11-26 10:00:00".into();
            } else if b.id == e_old.id {
                b.created_at = "2026-11-20 10:00:00".into();
            } else if b.id == e_evil.id {
                b.created_at = "2026-11-19 10:00:00".into();
            }
        }
    });

    let settings_svc = DefaultSettingsService::new(Arc::clone(&store));
    settings_svc
        .update_retention(
            RetentionPolicy {
                enabled: true,
                keep_last: Some(1),
                max_age_days: None,
                failed_max_age_days: Some(7),
            },
            CleanupFrequency::Daily,
        )
        .expect("enable retention");

    let bk_svc: Arc<dyn BackupService> = Arc::new(
        DefaultBackupService::new(
            Arc::clone(&store),
            Arc::clone(&conn_repo) as _,
            Arc::clone(&bk_repo) as _,
            tools,
            Arc::new(TokioCommandRunner::new()),
        )
        .with_snapshot_lock(Arc::clone(&op_lock)),
    );
    let ret_svc =
        DefaultRetentionService::new(Arc::clone(&store), conn_repo, Arc::clone(&bk_svc), op_lock);

    let report = ret_svc.run().await.expect("run retention");
    assert_eq!(report.deleted_count, 1);
    assert_eq!(report.errors.len(), 1); // path-traversal entry refused without aborting
    assert!(f_new.exists(), "newest snapshot must be kept");
    assert!(!f_old.exists(), "old snapshot must be deleted");
    assert!(outside_file.exists(), "outside file must never be deleted");

    let remaining = bk_svc.list_filtered(None, None, None).expect("list");
    assert!(remaining.iter().any(|b| b.id == e_new.id));
    assert!(!remaining.iter().any(|b| b.id == e_old.id));

    drop(ret_svc);
    drop(bk_svc);
    drop(store);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v3_schema_loads_with_retention_disabled_and_deletes_nothing() {
    let dir = temp_dir("v3_schema");
    let bk_dir = dir.join("backups").join("Prod").join("2026-10-01");
    fs::create_dir_all(&bk_dir).expect("mkdir");
    let old_file = bk_dir.join("Legacy_old.sql");
    fs::write(&old_file, "-- legacy").expect("write");

    let v3_json = r#"{
        "schema_version": 3,
        "connections": [
            {
                "id": 1,
                "name": "Legacy",
                "group_name": "Prod",
                "type": "postgres",
                "url": "postgresql://u:p@localhost:5432/db",
                "created_at": "2026-10-01 10:00:00"
            }
        ],
        "backups": [
            {
                "id": 1,
                "connection_id": 1,
                "connection_name": "Legacy",
                "group_name": "Prod",
                "type": "postgres",
                "file_path": "Prod/2026-10-01/Legacy_old.sql",
                "file_size": 9,
                "status": "success",
                "created_at": "2026-10-01 10:00:00"
            }
        ],
        "schedules": [],
        "vps_peers": [],
        "settings": { "run_in_background": false, "run_on_startup": false },
        "_seq": { "c": 2, "b": 2, "s": 1, "v": 1 }
    }"#;
    fs::write(dir.join("chronobase-data.json"), v3_json).expect("write v3 json");

    let store = Arc::new(JsonStore::open(dir.clone()).expect("open v3"));
    let settings = store.read_with(|d| d.settings.clone());
    assert!(!settings.retention.enabled);

    let conn_repo = Arc::new(StoreConnectionRepository::new(Arc::clone(&store)));
    let bk_repo = Arc::new(StoreBackupRepository::new(Arc::clone(&store)));
    let bk_svc: Arc<dyn BackupService> = Arc::new(DefaultBackupService::new(
        Arc::clone(&store),
        Arc::clone(&conn_repo) as _,
        bk_repo,
        Arc::new(StubTools(ToolStatus::default())),
        Arc::new(TokioCommandRunner::new()),
    ));
    let ret_svc = DefaultRetentionService::new(
        Arc::clone(&store),
        conn_repo,
        Arc::clone(&bk_svc),
        Arc::new(tokio::sync::Mutex::new(())),
    );
    let report = ret_svc.run().await.expect("run");
    assert_eq!(report.deleted_count, 0);
    assert!(old_file.exists());

    drop(ret_svc);
    drop(bk_svc);
    drop(store);
    let _ = fs::remove_dir_all(&dir);
}
