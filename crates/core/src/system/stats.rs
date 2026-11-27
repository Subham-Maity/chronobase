//! Dashboard summary statistics and per-group / schedule breakdown computed from the store.

use std::collections::BTreeMap;

use chrono::Local;

use crate::backup::{BackupSourceKind, BackupStatus};
use crate::schedule::parse_cron_expr;
use crate::store::JsonStore;

/// Next upcoming scheduled cron run computed from active schedules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextScheduleInfo {
    /// Schedule name.
    pub name: String,
    /// Target group (`"All"` or specific group).
    pub group_name: String,
    /// 5-field cron expression.
    pub cron_expr: String,
    /// Exact next local run timestamp (`YYYY-MM-DD HH:MM`).
    pub next_run_exact: String,
}

/// Per-group inventory and backup footprint summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupBreakdown {
    /// Group name (`"Default"`, `"FXam"`, etc.).
    pub group_name: String,
    /// Number of direct DB connections in this group.
    pub local_conns: usize,
    /// Number of VPS peers in this group.
    pub vps_peers: usize,
    /// Total recorded snapshots in this group.
    pub backup_count: usize,
    /// Failed snapshots in this group.
    pub failed_count: usize,
    /// Total disk bytes across snapshots in this group.
    pub total_size: u64,
    /// Timestamp of newest snapshot in this group, if any.
    pub last_backup: Option<String>,
}

/// Aggregated statistics and breakdown displayed on the Dashboard page.
#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    /// Total saved direct connections.
    pub total_conns: usize,
    /// Total saved VPS peers.
    pub total_vps: usize,
    /// Total backup history entries.
    pub total_backups: usize,
    /// Count of successful backups.
    pub success_count: usize,
    /// Count of failed backups.
    pub failed_count: usize,
    /// Total bytes across all recorded backups.
    pub total_size: u64,
    /// Total bytes across direct DB backups.
    pub direct_size: u64,
    /// Total bytes across VPS pull backups.
    pub vps_size: u64,
    /// Timestamp of the most recent backup, if any.
    pub last_backup: Option<String>,
    /// Number of currently enabled schedules.
    pub active_scheds: usize,
    /// Earliest upcoming enabled schedule run, if any.
    pub next_schedule: Option<NextScheduleInfo>,
    /// Per-group breakdown sorted alphabetically.
    pub group_breakdowns: Vec<GroupBreakdown>,
}

impl DashboardStats {
    /// Computes current dashboard statistics and per-group breakdown from the in-memory store.
    pub fn from_store(store: &JsonStore) -> Self {
        store.read_with(|d| {
            let success_count = d
                .backups
                .iter()
                .filter(|b| b.status == BackupStatus::Success)
                .count();
            let failed_count = d.backups.len().saturating_sub(success_count);
            let mut total_size = 0u64;
            let mut direct_size = 0u64;
            let mut vps_size = 0u64;
            for b in &d.backups {
                total_size = total_size.saturating_add(b.file_size);
                match b.source_kind {
                    BackupSourceKind::Direct => {
                        direct_size = direct_size.saturating_add(b.file_size);
                    }
                    BackupSourceKind::VpsPull => {
                        vps_size = vps_size.saturating_add(b.file_size);
                    }
                }
            }

            let last_backup = d.backups.first().map(|b| b.created_at.clone());
            let active_scheds = d.schedules.iter().filter(|s| s.enabled).count();

            let now = Local::now();
            let mut earliest: Option<(chrono::DateTime<Local>, NextScheduleInfo)> = None;
            for sched in d.schedules.iter().filter(|s| s.enabled) {
                if let Ok(parsed) = parse_cron_expr(&sched.cron_expr)
                    && let Some(next_dt) = parsed.after(&now).next()
                {
                    let info = NextScheduleInfo {
                        name: sched.name.clone(),
                        group_name: sched.group_name.clone(),
                        cron_expr: sched.cron_expr.clone(),
                        next_run_exact: next_dt.format("%Y-%m-%d %H:%M").to_string(),
                    };
                    if earliest.as_ref().is_none_or(|(dt, _)| next_dt < *dt) {
                        earliest = Some((next_dt, info));
                    }
                }
            }

            let mut groups: BTreeMap<String, GroupBreakdown> = BTreeMap::new();
            for conn in &d.connections {
                let entry =
                    groups
                        .entry(conn.group_name.clone())
                        .or_insert_with(|| GroupBreakdown {
                            group_name: conn.group_name.clone(),
                            ..Default::default()
                        });
                entry.local_conns += 1;
            }
            for peer in &d.vps_peers {
                let entry =
                    groups
                        .entry(peer.group_name.clone())
                        .or_insert_with(|| GroupBreakdown {
                            group_name: peer.group_name.clone(),
                            ..Default::default()
                        });
                entry.vps_peers += 1;
            }
            for bk in &d.backups {
                let entry = groups
                    .entry(bk.group_name.clone())
                    .or_insert_with(|| GroupBreakdown {
                        group_name: bk.group_name.clone(),
                        ..Default::default()
                    });
                entry.backup_count += 1;
                if bk.status == BackupStatus::Failed {
                    entry.failed_count += 1;
                }
                entry.total_size = entry.total_size.saturating_add(bk.file_size);
                if entry.last_backup.is_none() {
                    entry.last_backup = Some(bk.created_at.clone());
                }
            }

            Self {
                total_conns: d.connections.len(),
                total_vps: d.vps_peers.len(),
                total_backups: d.backups.len(),
                success_count,
                failed_count,
                total_size,
                direct_size,
                vps_size,
                last_backup,
                active_scheds,
                next_schedule: earliest.map(|(_, info)| info),
                group_breakdowns: groups.into_values().collect(),
            }
        })
    }
}
