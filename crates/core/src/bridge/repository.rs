//! Repository trait and JSON-store implementation for VPS Pull peers.

use std::sync::Arc;

use crate::bridge::{BridgeError, CreateVpsPeerInput, UpdateVpsPeerInput, VpsPeer};
use crate::store::{JsonStore, local_timestamp};

/// Persistence boundary for configured VPS Pull peers.
pub trait VpsPeerRepository: Send + Sync {
    /// Lists all configured VPS peers.
    fn list(&self) -> Result<Vec<VpsPeer>, BridgeError>;

    /// Finds a VPS peer by ID.
    fn find_by_id(&self, id: u64) -> Result<Option<VpsPeer>, BridgeError>;

    /// Creates and persists a new VPS peer.
    fn create(&self, input: CreateVpsPeerInput) -> Result<VpsPeer, BridgeError>;

    /// Updates an existing VPS peer.
    fn update(&self, id: u64, input: UpdateVpsPeerInput) -> Result<VpsPeer, BridgeError>;

    /// Updates `last_pull` timestamp after a successful pull.
    fn mark_last_pull(&self, id: u64) -> Result<(), BridgeError>;

    /// Deletes a VPS peer by ID.
    fn delete(&self, id: u64) -> Result<(), BridgeError>;
}

/// JSON-store-backed [`VpsPeerRepository`].
pub struct StoreVpsPeerRepository {
    store: Arc<JsonStore>,
}

impl StoreVpsPeerRepository {
    /// Wraps an open [`JsonStore`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self { store }
    }
}

impl VpsPeerRepository for StoreVpsPeerRepository {
    fn list(&self) -> Result<Vec<VpsPeer>, BridgeError> {
        Ok(self.store.read_with(|d| d.vps_peers.clone()))
    }

    fn find_by_id(&self, id: u64) -> Result<Option<VpsPeer>, BridgeError> {
        Ok(self
            .store
            .read_with(|d| d.vps_peers.iter().find(|p| p.id == id).cloned()))
    }

    fn create(&self, input: CreateVpsPeerInput) -> Result<VpsPeer, BridgeError> {
        let peer = self.store.mutate_with(|d| {
            let id = d._seq.v;
            d._seq.v = d._seq.v.saturating_add(1);
            let peer = VpsPeer {
                id,
                name: input.name.trim().to_string(),
                endpoint: input.endpoint.trim().to_string(),
                db_type: input.db_type,
                remote_url: input.remote_url,
                pairing_key: input.pairing_key,
                target_mode: input.target_mode,
                last_pull: None,
                created_at: local_timestamp(),
            };
            d.vps_peers.push(peer.clone());
            peer
        });
        tracing::info!(vps_peer_id = peer.id, "vps peer created");
        Ok(peer)
    }

    fn update(&self, id: u64, input: UpdateVpsPeerInput) -> Result<VpsPeer, BridgeError> {
        let updated = self.store.mutate_with(|d| {
            let p = d.vps_peers.iter_mut().find(|p| p.id == id)?;
            p.name = input.name.trim().to_string();
            p.endpoint = input.endpoint.trim().to_string();
            p.db_type = input.db_type;
            p.remote_url = input.remote_url;
            p.pairing_key = input.pairing_key;
            p.target_mode = input.target_mode;
            Some(p.clone())
        });
        updated.ok_or_else(|| BridgeError::NotFound(id.to_string()))
    }

    fn mark_last_pull(&self, id: u64) -> Result<(), BridgeError> {
        self.store.mutate_with(|d| {
            if let Some(p) = d.vps_peers.iter_mut().find(|p| p.id == id) {
                p.last_pull = Some(local_timestamp());
            }
        });
        Ok(())
    }

    fn delete(&self, id: u64) -> Result<(), BridgeError> {
        let removed = self.store.mutate_with(|d| {
            let before = d.vps_peers.len();
            d.vps_peers.retain(|p| p.id != id);
            d.vps_peers.len() < before
        });
        if !removed {
            return Err(BridgeError::NotFound(id.to_string()));
        }
        tracing::info!(vps_peer_id = id, "vps peer deleted");
        Ok(())
    }
}
