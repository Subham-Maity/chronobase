//! Dashboard summary statistics computed from the store.

use crate::backup::BackupStatus;
use crate::store::JsonStore;

/// Aggregated statistics displayed on the Dashboard page.
#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    /// Total saved connections.
    pub total_conns: usize,
    /// Total backup history entries.
    pub total_backups: usize,
    /// Count of successful backups.
    pub success_count: usize,
    /// Total bytes across all recorded backups.
    pub total_size: u64,
    /// Timestamp of the most recent backup, if any.
    pub last_backup: Option<String>,
    /// Number of currently enabled schedules.
    pub active_scheds: usize,
}

impl DashboardStats {
    /// Computes current dashboard statistics from the in-memory store.
    pub fn from_store(store: &JsonStore) -> Self {
        store.read_with(|d| {
            let success_count = d
                .backups
                .iter()
                .filter(|b| b.status == BackupStatus::Success)
                .count();
            let total_size = d.backups.iter().map(|b| b.file_size).sum();
            let last_backup = d.backups.first().map(|b| b.created_at.clone());
            let active_scheds = d.schedules.iter().filter(|s| s.enabled).count();

            Self {
                total_conns: d.connections.len(),
                total_backups: d.backups.len(),
                success_count,
                total_size,
                last_backup,
                active_scheds,
            }
        })
    }
}
