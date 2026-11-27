//! System-level dashboard statistics, settings, and Danger Zone operations.

pub mod danger;
pub mod error;
pub mod settings;
pub mod stats;

pub use danger::{DangerService, DefaultDangerService};
pub use error::SystemError;
pub use settings::{DefaultSettingsService, SettingsService};
pub use stats::{DashboardStats, GroupBreakdown, NextScheduleInfo};
