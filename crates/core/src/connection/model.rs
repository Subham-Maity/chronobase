//! Domain models for database connections.

use serde::{Deserialize, Serialize};

use crate::secret::Secret;

/// Supported database engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    /// PostgreSQL database (`pg_dump` / `psql`).
    Postgres,
    /// MongoDB database (`mongodump` / `mongorestore` / `mongosh`).
    Mongodb,
}

impl DbKind {
    /// Returns the human-readable label for UI display.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Postgres => "PostgreSQL",
            Self::Mongodb => "MongoDB",
        }
    }

    /// Returns the short lowercase identifier (`"postgres"` or `"mongodb"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mongodb => "mongodb",
        }
    }
}

/// A saved database connection entry (matches ChronoBase v1 JSON schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// Auto-incremented unique identifier.
    pub id: u64,
    /// User-friendly display name.
    pub name: String,
    /// Logical group name (defaults to `"Default"`).
    pub group_name: String,
    /// Database engine kind.
    #[serde(rename = "type")]
    pub db_type: DbKind,
    /// Connection URI wrapped in `Secret` so it is never printed in logs.
    pub url: Secret,
    /// Optional user notes.
    #[serde(default)]
    pub notes: String,
    /// Creation timestamp (`YYYY-MM-DD HH:MM:SS` local time, matching v1).
    pub created_at: String,
}

/// Input payload for creating a new connection.
#[derive(Debug, Clone)]
pub struct CreateConnectionInput {
    /// Display name (required, non-empty).
    pub name: String,
    /// Group name (defaults to `"Default"` if blank).
    pub group_name: String,
    /// Database engine kind.
    pub db_type: DbKind,
    /// Connection URI.
    pub url: Secret,
    /// Optional notes.
    pub notes: String,
}

/// Input payload for updating an existing connection.
#[derive(Debug, Clone)]
pub struct UpdateConnectionInput {
    /// Updated display name (required, non-empty).
    pub name: String,
    /// Updated group name (defaults to `"Default"` if blank).
    pub group_name: String,
    /// Updated database engine kind.
    pub db_type: DbKind,
    /// Updated connection URI.
    pub url: Secret,
    /// Updated notes.
    pub notes: String,
}
