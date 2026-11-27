//! CLI tool discovery for PostgreSQL and MongoDB utilities.

pub mod discovery;

pub use discovery::{
    DefaultToolDiscovery, ToolDiscovery, ToolStatus, ToolVersions, parse_major_version,
    parse_version_string,
};
