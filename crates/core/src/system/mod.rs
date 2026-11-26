//! System-level dashboard statistics and Danger Zone operations.

pub mod danger;
pub mod error;
pub mod stats;

pub use danger::{DangerService, DefaultDangerService};
pub use error::SystemError;
pub use stats::DashboardStats;
