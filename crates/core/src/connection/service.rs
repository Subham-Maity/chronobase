//! Business logic for managing database connections.

use std::sync::Arc;

use url::Url;

use crate::connection::{
    Connection, ConnectionError, ConnectionRepository, CreateConnectionInput, DbKind,
    UpdateConnectionInput,
};

/// Service interface for connection CRUD and group queries.
pub trait ConnectionService: Send + Sync {
    /// Lists connections sorted by group and name, optionally filtered by group and type.
    fn list_filtered(
        &self,
        group: Option<&str>,
        db_type: Option<DbKind>,
    ) -> Result<Vec<Connection>, ConnectionError>;

    /// Finds a single connection by ID.
    fn get(&self, id: u64) -> Result<Connection, ConnectionError>;

    /// Lists all distinct group names sorted alphabetically.
    fn list_groups(&self) -> Result<Vec<String>, ConnectionError>;

    /// Validates and creates a new connection.
    fn create(&self, input: CreateConnectionInput) -> Result<Connection, ConnectionError>;

    /// Validates and updates an existing connection.
    fn update(&self, id: u64, input: UpdateConnectionInput) -> Result<Connection, ConnectionError>;

    /// Deletes a connection by ID (existing backup files on disk are kept).
    fn delete(&self, id: u64) -> Result<(), ConnectionError>;
}

/// Default [`ConnectionService`] backed by a [`ConnectionRepository`].
pub struct DefaultConnectionService {
    repo: Arc<dyn ConnectionRepository>,
}

impl DefaultConnectionService {
    /// Creates a new service using the given repository.
    pub fn new(repo: Arc<dyn ConnectionRepository>) -> Self {
        Self { repo }
    }
}

fn validate_connection_fields(
    name: &str,
    db_type: DbKind,
    url_raw: &str,
) -> Result<(), ConnectionError> {
    if name.trim().is_empty() {
        return Err(ConnectionError::Validation(
            "Connection name is required".to_string(),
        ));
    }
    let trimmed_url = url_raw.trim();
    if trimmed_url.is_empty() {
        return Err(ConnectionError::Validation(
            "Connection URL is required".to_string(),
        ));
    }

    let parsed = Url::parse(trimmed_url).map_err(|_| {
        ConnectionError::Validation("Connection URL is not a valid URL".to_string())
    })?;

    let scheme = parsed.scheme();
    match db_type {
        DbKind::Postgres if scheme != "postgres" && scheme != "postgresql" => {
            return Err(ConnectionError::Validation(
                "PostgreSQL URL must start with postgres:// or postgresql://".to_string(),
            ));
        }
        DbKind::Mongodb if scheme != "mongodb" && scheme != "mongodb+srv" => {
            return Err(ConnectionError::Validation(
                "MongoDB URL must start with mongodb:// or mongodb+srv://".to_string(),
            ));
        }
        _ => {}
    }

    Ok(())
}

impl ConnectionService for DefaultConnectionService {
    fn list_filtered(
        &self,
        group: Option<&str>,
        db_type: Option<DbKind>,
    ) -> Result<Vec<Connection>, ConnectionError> {
        let all = self.repo.list()?;
        Ok(all
            .into_iter()
            .filter(|c| {
                group.is_none_or(|g| g.is_empty() || c.group_name == g)
                    && db_type.is_none_or(|t| c.db_type == t)
            })
            .collect())
    }

    fn get(&self, id: u64) -> Result<Connection, ConnectionError> {
        self.repo
            .find_by_id(id)?
            .ok_or(ConnectionError::NotFound(id))
    }

    fn list_groups(&self) -> Result<Vec<String>, ConnectionError> {
        self.repo.list_groups()
    }

    fn create(&self, input: CreateConnectionInput) -> Result<Connection, ConnectionError> {
        validate_connection_fields(&input.name, input.db_type, input.url.expose())?;
        self.repo.create(input)
    }

    fn update(&self, id: u64, input: UpdateConnectionInput) -> Result<Connection, ConnectionError> {
        validate_connection_fields(&input.name, input.db_type, input.url.expose())?;
        self.repo.update(id, input)
    }

    fn delete(&self, id: u64) -> Result<(), ConnectionError> {
        self.repo.delete(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;
    use crate::store::JsonStore;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chronobase_conn_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp");
        dir
    }

    #[test]
    fn crud_and_validation_work() {
        let dir = temp_dir("crud");
        let store = Arc::new(JsonStore::open(dir.clone()).expect("open"));
        let repo = Arc::new(crate::connection::StoreConnectionRepository::new(
            Arc::clone(&store),
        ));
        let svc = DefaultConnectionService::new(repo);

        // Invalid scheme rejected
        let bad = svc.create(CreateConnectionInput {
            name: "Bad".into(),
            group_name: "".into(),
            db_type: DbKind::Postgres,
            url: Secret::new("mongodb://localhost:27017/db"),
            notes: "".into(),
        });
        assert!(matches!(bad, Err(ConnectionError::Validation(_))));

        let created = svc
            .create(CreateConnectionInput {
                name: "Prod".into(),
                group_name: "".into(),
                db_type: DbKind::Postgres,
                url: Secret::new("postgresql://user:pass@localhost:5432/db"),
                notes: "main".into(),
            })
            .expect("create");
        assert_eq!(created.group_name, "Default");
        assert_eq!(svc.list_filtered(None, None).expect("list").len(), 1);

        svc.delete(created.id).expect("delete");
        assert!(svc.list_filtered(None, None).expect("list").is_empty());

        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
