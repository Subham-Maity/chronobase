//! End-to-end integration tests for the encrypted VPS Pull Bridge.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::net::TcpListener;
use tokio::sync::watch;

use chronobase_core::backup::{CommandRunner, TokioCommandRunner};
use chronobase_core::bridge::agent::run_agent_server_with_deps;
use chronobase_core::bridge::client::pull_remote_dump;
use chronobase_core::bridge::{AgentConfig, BridgeError, generate_pairing_key};
use chronobase_core::connection::DbKind;
use chronobase_core::secret::Secret;
use chronobase_core::tools::{ToolDiscovery, ToolStatus};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("chronobase_bridge_{label}_{nanos}"));
    fs::create_dir_all(&dir).expect("create temp");
    dir
}

fn create_stub_pg_dump(dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let script = dir.join("stub_pg_dump.cmd");
        let content = "@echo off\r\n:loop\r\nif \"%~1\"==\"\" goto done\r\nif \"%~1\"==\"-f\" (\r\n  echo -- vps encrypted dump > \"%~2\"\r\n  exit /b 0\r\n)\r\nshift\r\ngoto loop\r\n:done\r\nexit /b 1\r\n";
        fs::write(&script, content).expect("write stub");
        script
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("stub_pg_dump.sh");
        let content = "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"-f\" ]; then\n    echo \"-- vps encrypted dump\" > \"$2\"\n    exit 0\n  fi\n  shift\ndone\nexit 1\n";
        fs::write(&script, content).expect("write stub");
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
async fn vps_pull_authenticates_encrypts_verifies_sha_and_deletes_remote_staging() {
    let vps_dir = temp_dir("vps");
    let local_dir = temp_dir("local");
    let local_backups = local_dir.join("backups");
    fs::create_dir_all(&local_backups).expect("backups dir");

    let stub_pg = create_stub_pg_dump(&vps_dir);
    let key = generate_pairing_key().expect("key");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();

    let (tx, rx) = watch::channel(false);
    let cfg = AgentConfig {
        pairing_key: key.clone(),
        bind_addr: addr.clone(),
        allow_ips: vec![],
    };

    let tools: Arc<dyn ToolDiscovery> = Arc::new(StubTools(ToolStatus {
        pg_dump: Some(stub_pg),
        ..Default::default()
    }));
    let runner: Arc<dyn CommandRunner> = Arc::new(TokioCommandRunner::new());

    let vps_dir_clone = vps_dir.clone();
    let server_handle = tokio::spawn(async move {
        let _ = run_agent_server_with_deps(vps_dir_clone, cfg, listener, tools, runner, rx).await;
    });

    // 1. Wrong pairing key must be rejected
    let wrong_key = generate_pairing_key().expect("wrong key");
    let err = pull_remote_dump(
        &local_backups,
        "ProdVPS",
        &addr,
        DbKind::Postgres,
        &Secret::new("postgresql://u:pw@localhost:5432/prod"),
        &wrong_key,
    )
    .await
    .expect_err("wrong key must fail");
    assert!(matches!(err, BridgeError::AuthFailed));

    // 2. Valid key pulls encrypted dump, verifies SHA-256, and deletes VPS staging dir
    let ok = pull_remote_dump(
        &local_backups,
        "ProdVPS",
        &addr,
        DbKind::Postgres,
        &Secret::new("postgresql://u:pw@localhost:5432/prod"),
        &key,
    )
    .await
    .expect("valid pull succeeds");

    assert!(ok.remote_deleted);
    assert!(ok.full_path.exists());
    let contents = fs::read_to_string(&ok.full_path).expect("read dump");
    assert!(contents.contains("vps encrypted dump"));

    // Verify zero staging directories remain on VPS
    let leftover_staging: Vec<_> = fs::read_dir(&vps_dir)
        .expect("read vps dir")
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".cb_vps_stage_")
        })
        .collect();
    assert!(
        leftover_staging.is_empty(),
        "staging files must be deleted from VPS after transfer"
    );

    let _ = tx.send(true);
    let _ = server_handle.await;
    let _ = fs::remove_dir_all(&vps_dir);
    let _ = fs::remove_dir_all(&local_dir);
}
