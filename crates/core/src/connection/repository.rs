//! Repository trait and JSON-store implementation for database connections.

use std::sync::Arc;

use crate::connection::{
    Connection, ConnectionError, CreateConnectionInput, UpdateConnectionInput,
};
use crate::constant::DEFAULT_GROUP;
use crate::store::{JsonStore, local_timestamp};

/// Persistence boundary for database connections.
pub trait ConnectionRepository: Send + Sync {
    /// Lists all connections sorted by `(group_name, name)`.
    fn list(&self) -> Result<Vec<Connection>, ConnectionError>;

    /// Finds a connection by ID.
    fn find_by_id(&self, id: u64) -> Result<Option<Connection>, ConnectionError>;

    /// Lists all connections belonging to `group_name`, or all connections if `"All"`.
    fn list_by_group(&self, group_name: &str) -> Result<Vec<Connection>, ConnectionError>;

    /// Lists all distinct group names sorted alphabetically.
    fn list_groups(&self) -> Result<Vec<String>, ConnectionError>;

    /// Creates and persists a new connection.
    fn create(&self, input: CreateConnectionInput) -> Result<Connection, ConnectionError>;

    /// Updates an existing connection by ID.
    fn update(&self, id: u64, input: UpdateConnectionInput) -> Result<Connection, ConnectionError>;

    /// Deletes a connection by ID.
    fn delete(&self, id: u64) -> Result<(), ConnectionError>;
}

/// JSON-store-backed [`ConnectionRepository`].
pub struct StoreConnectionRepository {
    store: Arc<JsonStore>,
}

impl StoreConnectionRepository {
    /// Wraps an open [`JsonStore`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self { store }
    }
}

impl ConnectionRepository for StoreConnectionRepository {
    fn list(&self) -> Result<Vec<Connection>, ConnectionError> {
        let mut conns = self.store.read_with(|d| d.connections.clone());
        conns.sort_by(|a, b| {
            let ka = format!("{}{}", a.group_name, a.name).to_lowercase();
            let kb = format!("{}{}", b.group_name, b.name).to_lowercase();
            ka.cmp(&kb)
        });
        Ok(conns)
    }

    fn find_by_id(&self, id: u64) -> Result<Option<Connection>, ConnectionError> {
        Ok(self
            .store
            .read_with(|d| d.connections.iter().find(|c| c.id == id).cloned()))
    }

    fn list_by_group(&self, group_name: &str) -> Result<Vec<Connection>, ConnectionError> {
        let conns = self.store.read_with(|d| {
            if group_name == crate::constant::ALL_GROUPS {
                d.connections.clone()
            } else {
                d.connections
                    .iter()
                    .filter(|c| c.group_name == group_name)
                    .cloned()
                    .collect()
            }
        });
        Ok(conns)
    }

    fn list_groups(&self) -> Result<Vec<String>, ConnectionError> {
        let mut groups: Vec<String> = self.store.read_with(|d| {
            let mut set = std::collections::BTreeSet::new();
            for c in &d.connections {
                set.insert(c.group_name.clone());
            }
            set.into_iter().collect()
        });
        groups.sort();
        Ok(groups)
    }

    fn create(&self, input: CreateConnectionInput) -> Result<Connection, ConnectionError> {
        let group = if input.group_name.trim().is_empty() {
            DEFAULT_GROUP.to_string()
        } else {
            input.group_name.trim().to_string()
        };

        let conn = self.store.mutate_with(|d| {
            let id = d._seq.c;
            d._seq.c = d._seq.c.saturating_add(1);
            let conn = Connection {
                id,
                name: input.name.trim().to_string(),
                group_name: group,
                db_type: input.db_type,
                url: input.url,
                notes: input.notes.trim().to_string(),
                created_at: local_timestamp(),
            };
            d.connections.push(conn.clone());
            conn
        });

        tracing::info!(connection_id = conn.id, group = %conn.group_name, "connection created");
        Ok(conn)
    }

    fn update(&self, id: u64, input: UpdateConnectionInput) -> Result<Connection, ConnectionError> {
        let group = if input.group_name.trim().is_empty() {
            DEFAULT_GROUP.to_string()
        } else {
            input.group_name.trim().to_string()
        };

        let updated = self.store.mutate_with(|d| {
            let existing = d.connections.iter_mut().find(|c| c.id == id)?;
            existing.name = input.name.trim().to_string();
            existing.group_name = group;
            existing.db_type = input.db_type;
            existing.url = input.url;
            existing.notes = input.notes.trim().to_string();
            Some(existing.clone())
        });

        match updated {
            Some(conn) => {
                tracing::info!(connection_id = conn.id, "connection updated");
                Ok(conn)
            }
            None => Err(ConnectionError::NotFound(id)),
        }
    }

    fn delete(&self, id: u64) -> Result<(), ConnectionError> {
        let removed = self.store.mutate_with(|d| {
            let before = d.connections.len();
            d.connections.retain(|c| c.id != id);
            d.connections.len() < before
        });
        if !removed {
            return Err(ConnectionError::NotFound(id));
        }
        tracing::info!(connection_id = id, "connection deleted");
        Ok(())
    }
}
