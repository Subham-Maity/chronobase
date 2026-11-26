//! Dashboard view-model mapping.

use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::backup::BackupEntry;
use chronobase_core::system::DashboardStats;

use crate::ui::{MainWindow, RecentBackupRow, fmt_relative, fmt_size};

/// Pushes updated dashboard statistics and the latest 8 snapshots to the UI thread.
pub fn push_dashboard(weak: &Weak<MainWindow>, stats: DashboardStats, recent: Vec<BackupEntry>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        win.set_stat_conns(stats.total_conns.to_string().into());
        win.set_stat_backups(stats.total_backups.to_string().into());
        let rate = (stats.success_count * 100)
            .checked_div(stats.total_backups)
            .map(|pct| format!("{pct}% success"))
            .unwrap_or_else(|| "no backups yet".to_string());
        win.set_stat_rate(rate.into());
        win.set_stat_size(fmt_size(stats.total_size).into());
        win.set_stat_last(
            stats
                .last_backup
                .as_deref()
                .map(fmt_relative)
                .unwrap_or_else(|| "—".to_string())
                .into(),
        );
        let sched_label = if stats.active_scheds == 1 {
            "1 active schedule".to_string()
        } else {
            format!("{} active schedules", stats.active_scheds)
        };
        win.set_stat_scheds(sched_label.into());

        let rows: Vec<RecentBackupRow> = recent
            .into_iter()
            .take(8)
            .map(|b| RecentBackupRow {
                id: b.id as i32,
                connection_name: b.connection_name.into(),
                group_name: b.group_name.into(),
                db_type: b.db_type.as_str().into(),
                status: b.status.as_str().into(),
                size_text: fmt_size(b.file_size).into(),
                when_text: fmt_relative(&b.created_at).into(),
            })
            .collect();
        win.set_recent_rows(Rc::new(VecModel::from(rows)).into());
    });
}
