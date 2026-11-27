//! Dashboard view-model mapping with per-group breakdown, tool versions, and next cron run.

use std::path::Path;
use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::backup::{BackupEntry, BackupSourceKind};
use chronobase_core::store::AppSettings;
use chronobase_core::system::DashboardStats;
use chronobase_core::tools::ToolStatus;

use crate::ui::{
    GroupBreakdownRow, MainWindow, RecentBackupRow, ToolDetailRow, fmt_relative, fmt_size,
};

/// Pushes updated dashboard statistics, breakdowns, tool details, and recent snapshots to UI.
pub fn push_dashboard(
    weak: &Weak<MainWindow>,
    stats: DashboardStats,
    recent: Vec<BackupEntry>,
    tools: &ToolStatus,
    data_dir: &Path,
    settings: &AppSettings,
) {
    let tool_details = vec![
        ToolDetailRow {
            name: "pg_dump".into(),
            ready: tools.pg_dump.is_some(),
            version: tools
                .versions
                .pg_dump
                .as_deref()
                .map(|v| format!("v{v}"))
                .unwrap_or_else(|| "—".into())
                .into(),
            path: tools
                .pg_dump
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not found on PATH".into())
                .into(),
        },
        ToolDetailRow {
            name: "psql".into(),
            ready: tools.psql.is_some(),
            version: tools
                .versions
                .psql
                .as_deref()
                .map(|v| format!("v{v}"))
                .unwrap_or_else(|| "—".into())
                .into(),
            path: tools
                .psql
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not found on PATH".into())
                .into(),
        },
        ToolDetailRow {
            name: "mongodump".into(),
            ready: tools.mongodump.is_some(),
            version: tools
                .versions
                .mongodump
                .as_deref()
                .map(|v| format!("v{v}"))
                .unwrap_or_else(|| "—".into())
                .into(),
            path: tools
                .mongodump
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not found on PATH".into())
                .into(),
        },
        ToolDetailRow {
            name: "mongorestore".into(),
            ready: tools.mongorestore.is_some(),
            version: tools
                .versions
                .mongorestore
                .as_deref()
                .map(|v| format!("v{v}"))
                .unwrap_or_else(|| "—".into())
                .into(),
            path: tools
                .mongorestore
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not found on PATH".into())
                .into(),
        },
        ToolDetailRow {
            name: "mongosh".into(),
            ready: tools.mongosh.is_some(),
            version: tools
                .versions
                .mongosh
                .as_deref()
                .map(|v| format!("v{v}"))
                .unwrap_or_else(|| "—".into())
                .into(),
            path: tools
                .mongosh
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not found on PATH".into())
                .into(),
        },
    ];

    let data_dir_str = data_dir.display().to_string();
    let bg_status = if settings.run_in_background {
        if settings.run_on_startup {
            "Tray active · Start on login".to_string()
        } else {
            "Tray active · Manual launch".to_string()
        }
    } else {
        "Foreground only (quits on close)".to_string()
    };

    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        let total_targets = stats.total_conns + stats.total_vps;
        win.set_stat_conns(total_targets.to_string().into());
        win.set_stat_targets_detail(
            format!("{} local · {} VPS", stats.total_conns, stats.total_vps).into(),
        );
        win.set_stat_backups(stats.total_backups.to_string().into());
        let rate = (stats.success_count * 100)
            .checked_div(stats.total_backups)
            .map(|pct| format!("{pct}% ok · {} failed", stats.failed_count))
            .unwrap_or_else(|| "no backups yet".to_string());
        win.set_stat_rate(rate.into());
        win.set_stat_size(fmt_size(stats.total_size).into());
        win.set_stat_size_detail(
            format!(
                "Direct {} · VPS {}",
                fmt_size(stats.direct_size),
                fmt_size(stats.vps_size)
            )
            .into(),
        );
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

        if let Some(next) = stats.next_schedule {
            win.set_stat_next_cron(next.next_run_exact.into());
            win.set_stat_next_cron_detail(format!("{} ({})", next.name, next.group_name).into());
        } else {
            win.set_stat_next_cron("None".into());
            win.set_stat_next_cron_detail("No enabled schedule".into());
        }

        win.set_dash_data_dir(data_dir_str.into());
        win.set_dash_bg_status(bg_status.into());

        let groups: Vec<GroupBreakdownRow> = stats
            .group_breakdowns
            .into_iter()
            .map(|g| GroupBreakdownRow {
                group_name: g.group_name.into(),
                local_conns: g.local_conns as i32,
                vps_peers: g.vps_peers as i32,
                backup_count: g.backup_count as i32,
                failed_count: g.failed_count as i32,
                total_size: fmt_size(g.total_size).into(),
                last_backup: g
                    .last_backup
                    .as_deref()
                    .map(fmt_relative)
                    .unwrap_or_else(|| "never".to_string())
                    .into(),
            })
            .collect();
        win.set_dash_group_rows(Rc::new(VecModel::from(groups)).into());
        win.set_dash_tool_rows(Rc::new(VecModel::from(tool_details)).into());

        let rows: Vec<RecentBackupRow> = recent
            .into_iter()
            .take(10)
            .map(|b| RecentBackupRow {
                id: b.id as i32,
                connection_name: b.connection_name.into(),
                group_name: b.group_name.into(),
                source_kind: match b.source_kind {
                    BackupSourceKind::Direct => "Direct".into(),
                    BackupSourceKind::VpsPull => "VPS Pull".into(),
                },
                db_type: b.db_type.as_str().into(),
                status: b.status.as_str().into(),
                size_text: fmt_size(b.file_size).into(),
                when_text: fmt_relative(&b.created_at).into(),
                exact_time: b.created_at.into(),
                file_path: b.file_path.into(),
                error_msg: b.error_msg.unwrap_or_default().into(),
                missing_on_disk: b.missing_on_disk,
            })
            .collect();
        win.set_recent_rows(Rc::new(VecModel::from(rows)).into());
    });
}
