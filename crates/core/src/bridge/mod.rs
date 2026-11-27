//! Secure VPS-to-Local Pull Bridge and one-click DB-to-DB migration.

pub mod agent;
pub mod client;
pub mod crypto;
pub mod error;
pub mod model;
pub mod protocol;
pub mod repository;
pub mod service;

pub use agent::{
    AGENT_CONFIG_FILE, DEFAULT_AGENT_BIND, TempStagingDir, init_agent_config, load_agent_config,
    run_agent_server,
};
pub use crypto::{generate_pairing_key, key_fingerprint};
pub use error::BridgeError;
pub use model::{
    AgentConfig, CreateVpsPeerInput, MigrateEndpoint, MigrateResult, PullResult, PullTargetMode,
    UpdateVpsPeerInput, VpsPeer,
};
pub use repository::{StoreVpsPeerRepository, VpsPeerRepository};
pub use service::{BridgeService, DefaultBridgeService};
