//! Framed `CBV1` binary protocol and path-safe archive helpers for the VPS bridge.

use std::path::{Component, Path, PathBuf};

use chacha20poly1305::ChaCha20Poly1305;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::bridge::BridgeError;
use crate::bridge::crypto::{decrypt_payload, encrypt_payload};
use crate::connection::DbKind;

/// Protocol magic banner (`"CBV1"`).
pub const PROTOCOL_MAGIC: &[u8; 4] = b"CBV1";

/// Direction identifier for client-to-server encrypted frames.
pub const DIR_C2S: u8 = 1;

/// Direction identifier for server-to-client encrypted frames.
pub const DIR_S2C: u8 = 2;

/// Hard upper bound on a single wire frame (1 MiB) to prevent memory exhaustion DoS.
pub const MAX_FRAME_LEN: usize = 1024 * 1024;

/// Maximum plaintext chunk size per stream message (256 KiB).
pub const STREAM_CHUNK_SIZE: usize = 256 * 1024;

/// Unencrypted server hello carrying fresh 32-byte CSPRNG nonce + server unix timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    /// Fresh server random challenge nonce.
    pub server_nonce: [u8; 32],
    /// Server unix timestamp in seconds.
    pub server_ts: u64,
}

/// Client authentication frame sent immediately after `ServerHello`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientAuth {
    /// Fresh client random nonce.
    pub client_nonce: [u8; 32],
    /// Client unix timestamp in seconds.
    pub client_ts: u64,
    /// `HMAC-SHA256(key, "CBV1-CLIENT-AUTH" || server_nonce || client_nonce || client_ts)`.
    pub client_mac: [u8; 32],
}

/// Mutual authentication proof returned by the VPS agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAuth {
    /// `HMAC-SHA256(key, "CBV1-SERVER-AUTH" || client_nonce || server_nonce)`.
    pub server_mac: [u8; 32],
}

/// Encrypted client request sent inside the AEAD session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncryptedRequest {
    /// Execute a dump on the VPS and stream it back, then delete the staging artifact.
    PullDump {
        /// Target database engine on the VPS.
        db_type: DbKind,
        /// Target database connection string on the VPS.
        remote_url: String,
    },
    /// Confirm receipt and SHA-256 verification so the VPS deletes the staging artifact.
    AckDelete,
}

/// Encrypted server response frames sent inside the AEAD session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncryptedResponse {
    /// PostgreSQL `.sql` file stream begins.
    PostgresStart,
    /// A chunk of the active PostgreSQL `.sql` file.
    FileChunk(Vec<u8>),
    /// MongoDB BSON directory file entry begins (relative POSIX path).
    MongoFileStart {
        /// Relative path inside the MongoDB dump directory.
        rel_path: String,
    },
    /// Current file entry finished.
    MongoFileEnd,
    /// Full dump stream completed cleanly with SHA-256 over all plaintext bytes.
    StreamDone {
        /// Total uncompressed bytes transferred.
        total_bytes: u64,
        /// Lowercase hex SHA-256 digest of the transferred byte stream.
        sha256_hex: String,
    },
    /// VPS staging files were deleted cleanly.
    DeletedAck,
    /// Operation failed on the VPS agent.
    Error(String),
}

/// Writes an unencrypted length-prefixed JSON handshake frame.
pub async fn write_plain_frame<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), BridgeError> {
    let bytes = serde_json::to_vec(msg).map_err(|e| BridgeError::Protocol(e.to_string()))?;
    if bytes.len() > MAX_FRAME_LEN {
        return Err(BridgeError::Protocol("Handshake frame too large".into()));
    }
    let len = bytes.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads an unencrypted length-prefixed JSON handshake frame.
pub async fn read_plain_frame<R: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, BridgeError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME_LEN {
        return Err(BridgeError::Protocol(format!(
            "Invalid frame length: {len}"
        )));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf).map_err(|e| BridgeError::Protocol(e.to_string()))
}

/// Encrypts and writes an AEAD-protected JSON frame, incrementing `counter`.
pub async fn write_encrypted_frame<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    cipher: &ChaCha20Poly1305,
    direction: u8,
    counter: &mut u64,
    msg: &T,
) -> Result<(), BridgeError> {
    let plaintext = serde_json::to_vec(msg).map_err(|e| BridgeError::Protocol(e.to_string()))?;
    let ct = encrypt_payload(cipher, direction, *counter, &plaintext)?;
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| BridgeError::Protocol("frame counter overflow".into()))?;
    if ct.len() > MAX_FRAME_LEN {
        return Err(BridgeError::Protocol("Encrypted frame too large".into()));
    }
    let len = ct.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&ct).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads and decrypts an AEAD-protected JSON frame, incrementing `counter`.
pub async fn read_encrypted_frame<R: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
    cipher: &ChaCha20Poly1305,
    direction: u8,
    counter: &mut u64,
) -> Result<T, BridgeError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME_LEN {
        return Err(BridgeError::Protocol(format!(
            "Invalid encrypted frame length: {len}"
        )));
    }
    let mut ct = vec![0u8; len];
    reader.read_exact(&mut ct).await?;
    let plaintext = decrypt_payload(cipher, direction, *counter, &ct)?;
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| BridgeError::Protocol("frame counter overflow".into()))?;
    serde_json::from_slice(&plaintext).map_err(|e| BridgeError::Protocol(e.to_string()))
}

/// Validates a relative file path received from a remote MongoDB stream.
///
/// Security: a compromised VPS agent could attempt path traversal by sending
/// `../../.ssh/authorized_keys` or `C:\...` inside `MongoFileStart`. We reject
/// any absolute path, drive prefix, backslash, null byte, or `..`/`.` component.
pub fn validate_safe_rel_path(raw: &str) -> Result<PathBuf, BridgeError> {
    if raw.is_empty() || raw.contains('\0') || raw.contains('\\') || raw.contains(':') {
        return Err(BridgeError::PathTraversal(raw.to_string()));
    }
    let p = Path::new(raw);
    if p.is_absolute() {
        return Err(BridgeError::PathTraversal(raw.to_string()));
    }
    let mut clean = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(BridgeError::PathTraversal(raw.to_string()));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(BridgeError::PathTraversal(raw.to_string()));
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_rel_path_blocks_traversal_and_absolute_paths() {
        assert_eq!(
            validate_safe_rel_path("mydb/users.bson").expect("valid"),
            PathBuf::from("mydb").join("users.bson")
        );
        for bad in [
            "",
            "/etc/passwd",
            "../../.ssh/authorized_keys",
            "a/../b",
            "./secret",
            "a\\b",
            "C:foo",
            "foo\0bar",
        ] {
            assert!(
                matches!(
                    validate_safe_rel_path(bad),
                    Err(BridgeError::PathTraversal(_))
                ),
                "must reject {bad:?}"
            );
        }
    }
}
