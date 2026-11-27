//! Encrypted, mutually-authenticated pull client communicating with a remote VPS agent.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Local;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::bridge::BridgeError;
use crate::bridge::crypto::{
    compute_client_auth_mac, compute_server_auth_mac, derive_master_key, derive_session_cipher,
    hex_encode, random_nonce32, unix_timestamp_secs, verify_mac_ct,
};
use crate::bridge::protocol::{
    ClientAuth, DIR_C2S, DIR_S2C, EncryptedRequest, EncryptedResponse, PROTOCOL_MAGIC, ServerAuth,
    ServerHello, read_encrypted_frame, read_plain_frame, validate_safe_rel_path,
    write_encrypted_frame, write_plain_frame,
};
use crate::connection::DbKind;
use crate::secret::Secret;
use crate::slug::slugify;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(900);

/// Downloaded backup payload metadata returned after SHA-256 verification and `AckDelete`.
#[derive(Debug, Clone)]
pub struct DownloadedDump {
    /// Relative path under `backups_dir` (e.g., `VPS_Prod/Prod_20261127_061000.sql`).
    pub rel_path: String,
    /// Full path on local disk.
    pub full_path: PathBuf,
    /// Total bytes saved on local disk.
    pub file_size: u64,
    /// Whether the VPS agent confirmed deletion of its temporary staging files.
    pub remote_deleted: bool,
}

/// Connects to a ChronoBase VPS agent, performs mutual HMAC-SHA256 authentication,
/// pulls the encrypted dump into `backups_dir`, verifies SHA-256, and instructs
/// the VPS agent to delete its temporary staging files.
pub async fn pull_remote_dump(
    backups_dir: &Path,
    group_name: &str,
    peer_name: &str,
    endpoint: &str,
    db_type: DbKind,
    remote_url: &Secret,
    pairing_key: &Secret,
) -> Result<DownloadedDump, BridgeError> {
    let master_key = derive_master_key(pairing_key)?;

    let mut stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(endpoint))
        .await
        .map_err(|_| BridgeError::TimedOut(CONNECT_TIMEOUT.as_secs()))??;

    // Mutual challenge-response handshake
    let (cipher, mut c2s_seq, mut s2c_seq) = tokio::time::timeout(CONNECT_TIMEOUT, async {
        stream.write_all(PROTOCOL_MAGIC).await?;
        stream.flush().await?;

        let hello: ServerHello = read_plain_frame(&mut stream).await?;
        let client_nonce = random_nonce32()?;
        let client_ts = unix_timestamp_secs();
        let client_mac =
            compute_client_auth_mac(&master_key, &hello.server_nonce, &client_nonce, client_ts)?;

        write_plain_frame(
            &mut stream,
            &ClientAuth {
                client_nonce,
                client_ts,
                client_mac,
            },
        )
        .await?;

        let server_auth: ServerAuth = match read_plain_frame(&mut stream).await {
            Ok(frame) => frame,
            Err(BridgeError::Io(ref err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                return Err(BridgeError::AuthFailed);
            }
            Err(err) => return Err(err),
        };
        let expected_server_mac =
            compute_server_auth_mac(&master_key, &client_nonce, &hello.server_nonce)?;
        if !verify_mac_ct(&expected_server_mac, &server_auth.server_mac) {
            return Err(BridgeError::AuthFailed);
        }

        let cipher = derive_session_cipher(&master_key, &hello.server_nonce, &client_nonce)?;
        Ok::<_, BridgeError>((cipher, 0u64, 0u64))
    })
    .await
    .map_err(|_| BridgeError::TimedOut(CONNECT_TIMEOUT.as_secs()))??;

    // Prepare local destination under backups/<GroupSlug>/<YYYY-MM-DD>/
    let group_slug = slugify(group_name);
    let name_slug = slugify(peer_name);
    let now = Local::now();
    let date_folder = now.format("%Y-%m-%d").to_string();
    let stamp = now.format("%Y%m%d_%H%M%S").to_string();
    let base_name = format!("{name_slug}_{stamp}");
    let target_dir = backups_dir.join(&group_slug).join(&date_folder);
    fs::create_dir_all(&target_dir)?;

    let (rel_path, full_path) = match db_type {
        DbKind::Postgres => {
            let file_name = format!("{base_name}.sql");
            (
                format!("{group_slug}/{date_folder}/{file_name}"),
                target_dir.join(file_name),
            )
        }
        DbKind::Mongodb => (
            format!("{group_slug}/{date_folder}/{base_name}"),
            target_dir.join(base_name),
        ),
    };

    let result = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        write_encrypted_frame(
            &mut stream,
            &cipher,
            DIR_C2S,
            &mut c2s_seq,
            &EncryptedRequest::PullDump {
                db_type,
                remote_url: remote_url.expose().to_string(),
            },
        )
        .await?;

        let mut hasher = Sha256::new();
        let mut active_file: Option<File> = None;
        let mut canon_target_dir: Option<PathBuf> = None;

        if db_type == DbKind::Mongodb {
            fs::create_dir_all(&full_path)?;
            canon_target_dir = Some(fs::canonicalize(&full_path)?);
        }

        loop {
            let resp: EncryptedResponse =
                read_encrypted_frame(&mut stream, &cipher, DIR_S2C, &mut s2c_seq).await?;
            match resp {
                EncryptedResponse::PostgresStart => {
                    active_file = Some(File::create(&full_path)?);
                }
                EncryptedResponse::MongoFileStart { rel_path } => {
                    let safe_rel = validate_safe_rel_path(&rel_path)?;
                    let dest = full_path.join(&safe_rel);
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                        // Security: canonicalize parent directory and verify containment inside target folder.
                        if let Some(ref root_canon) = canon_target_dir {
                            let parent_canon = fs::canonicalize(parent)?;
                            if !parent_canon.starts_with(root_canon) {
                                return Err(BridgeError::PathTraversal(rel_path));
                            }
                        }
                    }
                    hasher.update(rel_path.as_bytes());
                    hasher.update(b"\0");
                    active_file = Some(File::create(&dest)?);
                }
                EncryptedResponse::FileChunk(chunk) => {
                    let Some(ref mut f) = active_file else {
                        return Err(BridgeError::Protocol(
                            "Received FileChunk before file start".into(),
                        ));
                    };
                    hasher.update(&chunk);
                    f.write_all(&chunk)?;
                }
                EncryptedResponse::MongoFileEnd => {
                    if let Some(f) = active_file.take() {
                        f.sync_all()?;
                    }
                }
                EncryptedResponse::StreamDone {
                    total_bytes: _,
                    sha256_hex,
                } => {
                    if let Some(f) = active_file.take() {
                        f.sync_all()?;
                    }
                    let computed = hex_encode(&hasher.finalize());
                    if computed != sha256_hex {
                        return Err(BridgeError::Protocol(
                            "SHA-256 checksum verification failed on received backup stream".into(),
                        ));
                    }
                    break;
                }
                EncryptedResponse::Error(msg) => {
                    return Err(BridgeError::RemoteAgent(msg));
                }
                EncryptedResponse::DeletedAck => {
                    return Err(BridgeError::Protocol("Unexpected DeletedAck".into()));
                }
            }
        }

        // Confirm receipt so the VPS deletes its temporary dump artifacts.
        write_encrypted_frame(
            &mut stream,
            &cipher,
            DIR_C2S,
            &mut c2s_seq,
            &EncryptedRequest::AckDelete,
        )
        .await?;

        let ack_resp: EncryptedResponse =
            read_encrypted_frame(&mut stream, &cipher, DIR_S2C, &mut s2c_seq).await?;
        let remote_deleted = matches!(ack_resp, EncryptedResponse::DeletedAck);
        let file_size = dir_or_file_size(&full_path);

        Ok::<_, BridgeError>(DownloadedDump {
            rel_path,
            full_path: full_path.clone(),
            file_size,
            remote_deleted,
        })
    })
    .await;

    match result {
        Ok(Ok(done)) => Ok(done),
        Ok(Err(err)) => {
            cleanup_partial(&full_path);
            Err(err)
        }
        Err(_) => {
            cleanup_partial(&full_path);
            Err(BridgeError::TimedOut(TRANSFER_TIMEOUT.as_secs()))
        }
    }
}

fn cleanup_partial(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn dir_or_file_size(path: &Path) -> u64 {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|e| dir_or_file_size(&e.path())).sum()
}
