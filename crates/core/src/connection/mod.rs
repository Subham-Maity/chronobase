//! Connection management feature module.

pub mod error;
pub mod model;
pub mod repository;
pub mod service;

pub use error::ConnectionError;
pub use model::{Connection, CreateConnectionInput, DbKind, UpdateConnectionInput};
pub use repository::{ConnectionRepository, StoreConnectionRepository};
pub use service::{ConnectionService, DefaultConnectionService};
