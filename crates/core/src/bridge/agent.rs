//! Secure VPS agent server (`chronobase --agent` and `chronobase agent-init`).
//!
//! Runs on a remote VPS, authenticates incoming pull clients via mutual
//! HMAC-SHA256 + `ChaCha20-Poly1305` AEAD encryption, dumps the requested local
//! database into an isolated temporary staging folder, streams it with a
//! SHA-256 checksum, and unconditionally deletes the temporary staging files
//! immediately after transfer (and via RAII `Drop` on any failure/disconnect).

use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::backup::engine::{
    CommandRunner, TempSecretFile, TokioCommandRunner, build_mongodump_spec, build_pg_dump_spec,
    is_config_flag_unsupported,
};
use crate::bridge::crypto::{
    AuthRateLimiter, ReplayGuard, compute_client_auth_mac, compute_server_auth_mac,
    derive_master_key, derive_session_cipher, generate_pairing_key, hex_encode, key_fingerprint,
    random_nonce32, unix_timestamp_secs, verify_mac_ct,
};
use crate::bridge::protocol::{
    ClientAuth, DIR_C2S, DIR_S2C, EncryptedRequest, EncryptedResponse, PROTOCOL_MAGIC,
    STREAM_CHUNK_SIZE, ServerAuth, ServerHello, read_encrypted_frame, read_plain_frame,
    write_encrypted_frame, write_plain_frame,
};
use crate::bridge::{AgentConfig, BridgeError};
use crate::connection::DbKind;
use crate::constant::BACKUP_TIMEOUT;
use crate::platform;
use crate::secret::Secret;
use crate::tools::{DefaultToolDiscovery, ToolDiscovery};

/// Filename for the `0600` agent configuration stored inside the VPS data dir.
pub const AGENT_CONFIG_FILE: &str = "chronobase-agent.json";

/// Default agent listen address (loopback by default; override with `--bind` or config).
pub const DEFAULT_AGENT_BIND: &str = "127.0.0.1:9443";

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(900);

/// RAII temporary staging directory that guarantees deletion of remote dump
/// artifacts on drop (whether transfer succeeded, errored, or disconnected).
pub struct TempStagingDir {
    path: PathBuf,
}

impl TempStagingDir {
    /// Creates a unique isolated staging directory under `parent`.
    pub fn create(parent: &Path) -> Result<Self, BridgeError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let path = parent.join(format!(".cb_vps_stage_{pid}_{nanos}"));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Returns the staging directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Explicitly removes the staging directory and all files inside.
    pub fn cleanup(self) -> Result<(), BridgeError> {
        let res = if self.path.exists() {
            fs::remove_dir_all(&self.path)
        } else {
            Ok(())
        };
        std::mem::forget(self);
        res?;
        Ok(())
    }
}

impl Drop for TempStagingDir {
    fn drop(&mut self) {
        if self.path.exists() {
            // Intentionally ignore: best-effort Drop cleanup if explicit cleanup didn't run.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Generates a new `chronobase-agent.json` with `0600` permissions in `data_dir`.
/// Returns `(config_path, generated_pairing_key, sha256_fingerprint)`.
pub fn init_agent_config(
    data_dir: &Path,
    bind_addr: Option<&str>,
) -> Result<(PathBuf, Secret, String), BridgeError> {
    fs::create_dir_all(data_dir)?;
    let key = generate_pairing_key()?;
    let fingerprint = key_fingerprint(&key);
    let cfg = AgentConfig {
        pairing_key: key.clone(),
        bind_addr: bind_addr.unwrap_or(DEFAULT_AGENT_BIND).to_string(),
        allow_ips: Vec::new(),
    };
    let path = data_dir.join(AGENT_CONFIG_FILE);
    let json =
        serde_json::to_string_pretty(&cfg).map_err(|e| BridgeError::Protocol(e.to_string()))?;
    let mut f = platform::create_private_file(&path)?;
    f.write_all(json.as_bytes())?;
    f.sync_all()?;
    Ok((path, key, fingerprint))
}

/// Loads [`AgentConfig`] from `data_dir/chronobase-agent.json`, allowing
/// `CHRONOBASE_AGENT_KEY` env override (never read from argv).
pub fn load_agent_config(
    data_dir: &Path,
    bind_override: Option<&str>,
    allow_ips: Vec<String>,
) -> Result<AgentConfig, BridgeError> {
    let path = data_dir.join(AGENT_CONFIG_FILE);
    let mut cfg = if path.exists() {
        let raw = fs::read_to_string(&path)?;
        serde_json::from_str::<AgentConfig>(&raw)
            .map_err(|e| BridgeError::Protocol(format!("Invalid {AGENT_CONFIG_FILE}: {e}")))?
    } else if let Ok(env_key) = std::env::var("CHRONOBASE_AGENT_KEY") {
        AgentConfig {
            pairing_key: Secret::new(env_key),
            bind_addr: DEFAULT_AGENT_BIND.to_string(),
            allow_ips: Vec::new(),
        }
    } else {
        return Err(BridgeError::Validation(format!(
            "No agent config found at {} and CHRONOBASE_AGENT_KEY is not set. Run `chronobase agent-init` first.",
            path.display()
        )));
    };

    if let Some(b) = bind_override {
        cfg.bind_addr = b.to_string();
    }
    if !allow_ips.is_empty() {
        cfg.allow_ips = allow_ips;
    }
    // Validate key strength immediately on startup.
    let _ = derive_master_key(&cfg.pairing_key)?;
    Ok(cfg)
}

/// Runs the authenticated, encrypted VPS Pull Agent TCP server until `shutdown` triggers.
pub async fn run_agent_server(
    data_dir: PathBuf,
    config: AgentConfig,
    shutdown: watch::Receiver<bool>,
) -> Result<(), BridgeError> {
    let listener = TcpListener::bind(&config.bind_addr).await?;
    let tools: Arc<dyn ToolDiscovery> = Arc::new(DefaultToolDiscovery::new());
    let runner: Arc<dyn CommandRunner> = Arc::new(TokioCommandRunner::new());
    run_agent_server_with_deps(data_dir, config, listener, tools, runner, shutdown).await
}

/// Runs the VPS Pull Agent using an existing [`TcpListener`] and injected tool/command deps.
pub async fn run_agent_server_with_deps(
    data_dir: PathBuf,
    config: AgentConfig,
    listener: TcpListener,
    tools: Arc<dyn ToolDiscovery>,
    runner: Arc<dyn CommandRunner>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), BridgeError> {
    let master_key = derive_master_key(&config.pairing_key)?;
    let fingerprint = key_fingerprint(&config.pairing_key);
    let allow_ips: Vec<IpAddr> = config
        .allow_ips
        .iter()
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .collect();

    let local_addr = listener.local_addr()?;
    tracing::info!(
        bind = %local_addr,
        key_fingerprint = %fingerprint,
        "ChronoBase VPS agent listening"
    );

    let replay_guard = Arc::new(ReplayGuard::new());
    let rate_limiter = Arc::new(AuthRateLimiter::new());

    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((stream, peer_addr)) => {
                        let data_dir = data_dir.clone();
                        let replay = Arc::clone(&replay_guard);
                        let limiter = Arc::clone(&rate_limiter);
                        let tools = Arc::clone(&tools);
                        let runner = Arc::clone(&runner);
                        let allowed = allow_ips.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_client(
                                stream,
                                peer_addr,
                                master_key,
                                &allowed,
                                &replay,
                                &limiter,
                                &data_dir,
                                &tools,
                                &runner,
                            ).await {
                                tracing::warn!(peer = %peer_addr.ip(), error = %err, "vps agent session rejected or failed");
                            }
                        });
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "vps agent accept failed");
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    tracing::info!("vps agent shutting down");
                    break;
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_client(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    master_key: [u8; 32],
    allow_ips: &[IpAddr],
    replay: &ReplayGuard,
    limiter: &AuthRateLimiter,
    data_dir: &Path,
    tools: &Arc<dyn ToolDiscovery>,
    runner: &Arc<dyn CommandRunner>,
) -> Result<(), BridgeError> {
    let ip = peer_addr.ip();

    // Security: optional IP allowlist + per-IP brute-force lockout before any crypto.
    if !allow_ips.is_empty() && !allow_ips.contains(&ip) {
        return Err(BridgeError::AccessDenied);
    }
    limiter.check(ip)?;

    let (cipher, mut c2s_seq, mut s2c_seq) = match tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let mut magic = [0u8; 4];
        stream.read_exact(&mut magic).await?;
        if &magic != PROTOCOL_MAGIC {
            return Err(BridgeError::Protocol("Invalid protocol magic".into()));
        }

        let server_nonce = random_nonce32()?;
        let server_ts = unix_timestamp_secs();
        write_plain_frame(
            &mut stream,
            &ServerHello {
                server_nonce,
                server_ts,
            },
        )
        .await?;

        let auth: ClientAuth = read_plain_frame(&mut stream).await?;
        replay.check_and_record(auth.client_nonce, auth.client_ts, unix_timestamp_secs())?;

        let expected_mac = compute_client_auth_mac(
            &master_key,
            &server_nonce,
            &auth.client_nonce,
            auth.client_ts,
        )?;
        if !verify_mac_ct(&expected_mac, &auth.client_mac) {
            return Err(BridgeError::AuthFailed);
        }

        let server_mac = compute_server_auth_mac(&master_key, &auth.client_nonce, &server_nonce)?;
        write_plain_frame(&mut stream, &ServerAuth { server_mac }).await?;

        let cipher = derive_session_cipher(&master_key, &server_nonce, &auth.client_nonce)?;
        Ok((cipher, 0u64, 0u64))
    })
    .await
    {
        Ok(Ok(ok)) => {
            limiter.record_success(ip);
            ok
        }
        Ok(Err(err)) => {
            if matches!(err, BridgeError::AuthFailed) {
                limiter.record_failure(ip);
            }
            return Err(err);
        }
        Err(_) => return Err(BridgeError::TimedOut(HANDSHAKE_TIMEOUT.as_secs())),
    };

    let req: EncryptedRequest = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        read_encrypted_frame(&mut stream, &cipher, DIR_C2S, &mut c2s_seq),
    )
    .await
    .map_err(|_| BridgeError::TimedOut(HANDSHAKE_TIMEOUT.as_secs()))??;

    let (db_type, remote_url) = match req {
        EncryptedRequest::PullDump {
            db_type,
            remote_url,
        } => (db_type, remote_url),
        EncryptedRequest::AckDelete => {
            return Err(BridgeError::Protocol(
                "Unexpected AckDelete before dump".into(),
            ));
        }
    };

    // Execute dump inside RAII TempStagingDir so it is ALWAYS deleted on return/drop.
    let staging = TempStagingDir::create(data_dir)?;
    let tool_status = tools.status().await;

    let transfer_res = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        match db_type {
            DbKind::Postgres => {
                let Some(bin) = tool_status.pg_dump else {
                    let msg = "pg_dump not found on VPS".to_string();
                    write_encrypted_frame(
                        &mut stream,
                        &cipher,
                        DIR_S2C,
                        &mut s2c_seq,
                        &EncryptedResponse::Error(msg.clone()),
                    )
                    .await?;
                    return Err(BridgeError::RemoteAgent(msg));
                };
                let dump_path = staging.path().join("dump.sql");
                let spec = build_pg_dump_spec(bin, &remote_url, &dump_path, BACKUP_TIMEOUT)?;
                let out = runner.run(spec).await?;
                if !out.success {
                    let msg = if out.stderr_tail.is_empty() {
                        "pg_dump failed on VPS".to_string()
                    } else {
                        out.stderr_tail
                    };
                    write_encrypted_frame(
                        &mut stream,
                        &cipher,
                        DIR_S2C,
                        &mut s2c_seq,
                        &EncryptedResponse::Error(msg.clone()),
                    )
                    .await?;
                    return Err(BridgeError::RemoteAgent(msg));
                }

                write_encrypted_frame(
                    &mut stream,
                    &cipher,
                    DIR_S2C,
                    &mut s2c_seq,
                    &EncryptedResponse::PostgresStart,
                )
                .await?;

                let mut hasher = Sha256::new();
                let mut total_bytes = 0u64;
                stream_file_chunks(
                    &dump_path,
                    &mut stream,
                    &cipher,
                    &mut s2c_seq,
                    &mut hasher,
                    &mut total_bytes,
                )
                .await?;

                let sha256_hex = hex_encode(&hasher.finalize());
                write_encrypted_frame(
                    &mut stream,
                    &cipher,
                    DIR_S2C,
                    &mut s2c_seq,
                    &EncryptedResponse::StreamDone {
                        total_bytes,
                        sha256_hex,
                    },
                )
                .await?;
            }
            DbKind::Mongodb => {
                let Some(bin) = tool_status.mongodump else {
                    let msg = "mongodump not found on VPS".to_string();
                    write_encrypted_frame(
                        &mut stream,
                        &cipher,
                        DIR_S2C,
                        &mut s2c_seq,
                        &EncryptedResponse::Error(msg.clone()),
                    )
                    .await?;
                    return Err(BridgeError::RemoteAgent(msg));
                };
                let dump_dir = staging.path().join("bson");
                let yaml_cfg = format!(
                    "uri: \"{}\"\n",
                    remote_url.replace('\\', "\\\\").replace('"', "\\\"")
                );
                let cfg_file =
                    TempSecretFile::create(staging.path(), "vps_mongo_cfg", "yaml", &yaml_cfg)?;
                let spec = build_mongodump_spec(
                    bin.clone(),
                    Some(cfg_file.path()),
                    &remote_url,
                    &dump_dir,
                    BACKUP_TIMEOUT,
                );
                let mut out = runner.run(spec).await?;
                if !out.success && is_config_flag_unsupported(&out.stderr_tail) {
                    let fallback =
                        build_mongodump_spec(bin, None, &remote_url, &dump_dir, BACKUP_TIMEOUT);
                    out = runner.run(fallback).await?;
                }
                drop(cfg_file);

                if !out.success {
                    let msg = if out.stderr_tail.is_empty() {
                        "mongodump failed on VPS".to_string()
                    } else {
                        out.stderr_tail
                    };
                    write_encrypted_frame(
                        &mut stream,
                        &cipher,
                        DIR_S2C,
                        &mut s2c_seq,
                        &EncryptedResponse::Error(msg.clone()),
                    )
                    .await?;
                    return Err(BridgeError::RemoteAgent(msg));
                }

                let mut files = Vec::new();
                collect_files_rel(&dump_dir, &dump_dir, &mut files)?;
                let mut hasher = Sha256::new();
                let mut total_bytes = 0u64;

                for (rel, full_path) in files {
                    hasher.update(rel.as_bytes());
                    hasher.update(b"\0");
                    write_encrypted_frame(
                        &mut stream,
                        &cipher,
                        DIR_S2C,
                        &mut s2c_seq,
                        &EncryptedResponse::MongoFileStart { rel_path: rel },
                    )
                    .await?;

                    stream_file_chunks(
                        &full_path,
                        &mut stream,
                        &cipher,
                        &mut s2c_seq,
                        &mut hasher,
                        &mut total_bytes,
                    )
                    .await?;

                    write_encrypted_frame(
                        &mut stream,
                        &cipher,
                        DIR_S2C,
                        &mut s2c_seq,
                        &EncryptedResponse::MongoFileEnd,
                    )
                    .await?;
                }

                let sha256_hex = hex_encode(&hasher.finalize());
                write_encrypted_frame(
                    &mut stream,
                    &cipher,
                    DIR_S2C,
                    &mut s2c_seq,
                    &EncryptedResponse::StreamDone {
                        total_bytes,
                        sha256_hex,
                    },
                )
                .await?;
            }
        }

        // Wait for client AckDelete, then delete staging dir immediately and confirm.
        let ack: EncryptedRequest =
            read_encrypted_frame(&mut stream, &cipher, DIR_C2S, &mut c2s_seq).await?;
        if !matches!(ack, EncryptedRequest::AckDelete) {
            return Err(BridgeError::Protocol("Expected AckDelete".into()));
        }
        Ok(())
    })
    .await;

    // Staging files are ALWAYS deleted right here even if the transfer timed out or failed.
    staging.cleanup()?;
    match transfer_res {
        Ok(inner) => inner?,
        Err(_) => return Err(BridgeError::TimedOut(TRANSFER_TIMEOUT.as_secs())),
    }

    write_encrypted_frame(
        &mut stream,
        &cipher,
        DIR_S2C,
        &mut s2c_seq,
        &EncryptedResponse::DeletedAck,
    )
    .await?;

    tracing::info!(peer = %ip, "vps pull completed and staging files deleted");
    Ok(())
}

async fn stream_file_chunks(
    path: &Path,
    stream: &mut TcpStream,
    cipher: &chacha20poly1305::ChaCha20Poly1305,
    s2c_seq: &mut u64,
    hasher: &mut Sha256,
    total_bytes: &mut u64,
) -> Result<(), BridgeError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let slice = &buf[..n];
        hasher.update(slice);
        *total_bytes = total_bytes.saturating_add(n as u64);
        write_encrypted_frame(
            stream,
            cipher,
            DIR_S2C,
            s2c_seq,
            &EncryptedResponse::FileChunk(slice.to_vec()),
        )
        .await?;
    }
    Ok(())
}

fn collect_files_rel(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), BridgeError> {
    let mut entries: Vec<_> = fs::read_dir(current)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_files_rel(root, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| BridgeError::Protocol(e.to_string()))?;
            let rel_posix = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel_posix, path));
        }
    }
    Ok(())
}
