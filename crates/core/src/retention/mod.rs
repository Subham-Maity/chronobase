//! Backup retention planning, scheduling, and safe auto-cleanup.

pub mod error;
pub mod model;
pub mod planner;
pub mod service;

pub use error::RetentionError;
pub use model::{
    CleanupFrequency, RetentionCandidate, RetentionPlan, RetentionPolicy, RetentionReason,
    RetentionRunReport,
};
pub use planner::plan_retention;
pub use service::{DefaultRetentionService, RetentionService};
