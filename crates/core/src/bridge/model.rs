//! Domain models for VPS Pull peers, agent config, and DB-to-DB migration.

use serde::{Deserialize, Serialize};

use crate::backup::BackupEntry;
use crate::connection::DbKind;
use crate::constant::DEFAULT_GROUP;
use crate::secret::Secret;

fn default_vps_group() -> String {
    DEFAULT_GROUP.to_string()
}

/// Either a direct database connection or a configured VPS Pull peer used as migration endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateEndpoint {
    /// Direct database connection ID.
    Connection(u64),
    /// Remote VPS Pull peer ID.
    VpsPeer(u64),
}

/// Destination for a snapshot pulled from a remote VPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "connection_id", rename_all = "snake_case")]
pub enum PullTargetMode {
    /// Save the pulled dump only in local `backups/` storage.
    StorageOnly,
    /// Save locally AND restore/migrate directly into a local ChronoBase connection.
    LocalConnection(u64),
}

/// Saved VPS peer configuration stored in `chronobase-data.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpsPeer {
    /// Auto-incremented unique identifier.
    pub id: u64,
    /// Friendly name (e.g., `"Prod Hetzner VPS"`).
    pub name: String,
    /// Group name used for Connections organization, backups folder, and cron schedules.
    #[serde(default = "default_vps_group")]
    pub group_name: String,
    /// Agent TCP endpoint (`host:port`, e.g., `"127.0.0.1:9443"` or `"203.0.113.10:9443"`).
    pub endpoint: String,
    /// Remote database engine.
    pub db_type: DbKind,
    /// Remote database URL (sent encrypted inside the authenticated pull request).
    pub remote_url: Secret,
    /// 256-bit shared pairing key (redacted in logs/Debug/Display).
    pub pairing_key: Secret,
    /// Whether to keep in local storage only or push straight into a local DB.
    pub target_mode: PullTargetMode,
    /// Timestamp of the last completed pull (`YYYY-MM-DD HH:MM:SS`).
    #[serde(default)]
    pub last_pull: Option<String>,
    /// Creation timestamp (`YYYY-MM-DD HH:MM:SS`).
    pub created_at: String,
}

/// Input payload for creating a VPS peer.
#[derive(Debug, Clone)]
pub struct CreateVpsPeerInput {
    /// Friendly name.
    pub name: String,
    /// Group name (defaults to `"Default"` if blank).
    pub group_name: String,
    /// Agent endpoint (`host:port`).
    pub endpoint: String,
    /// Remote DB engine.
    pub db_type: DbKind,
    /// Remote DB URL on the VPS (usually `localhost` inside the VPS).
    pub remote_url: Secret,
    /// Shared pairing secret.
    pub pairing_key: Secret,
    /// Local target destination.
    pub target_mode: PullTargetMode,
}

/// Input payload for updating a VPS peer.
#[derive(Debug, Clone)]
pub struct UpdateVpsPeerInput {
    /// Updated name.
    pub name: String,
    /// Updated group name (defaults to `"Default"` if blank).
    pub group_name: String,
    /// Updated endpoint (`host:port`).
    pub endpoint: String,
    /// Updated engine.
    pub db_type: DbKind,
    /// Updated remote DB URL.
    pub remote_url: Secret,
    /// Updated pairing key.
    pub pairing_key: Secret,
    /// Updated target mode.
    pub target_mode: PullTargetMode,
}

/// Outcome of pulling a backup from a VPS peer.
#[derive(Debug, Clone)]
pub struct PullResult {
    /// Peer ID.
    pub peer_id: u64,
    /// Peer name.
    pub peer_name: String,
    /// Recorded local backup entry.
    pub entry: BackupEntry,
    /// Whether the remote temporary staging file on the VPS was confirmed deleted.
    pub remote_deleted: bool,
    /// Whether the snapshot was also restored into a local DB target.
    pub restored_to_local_db: bool,
}

/// Outcome of a direct one-click DB-to-DB migration.
#[derive(Debug, Clone)]
pub struct MigrateResult {
    /// Source connection name.
    pub source_name: String,
    /// Destination connection name.
    pub dest_name: String,
    /// Snapshot created during migration.
    pub entry: BackupEntry,
}

/// Configuration file (`chronobase-agent.json`, `0600`) used by `chronobase --agent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Shared 256-bit pairing key (stored with `0600` file permissions, never passed in argv).
    pub pairing_key: Secret,
    /// Bind address (default `"127.0.0.1:9443"`).
    pub bind_addr: String,
    /// Optional IP allowlist; empty means allow any IP that passes HMAC authentication.
    #[serde(default)]
    pub allow_ips: Vec<String>,
}
