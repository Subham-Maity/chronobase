//! Backups view-model mapping with Group + Date section headers and filename extraction.

use std::rc::Rc;

use chrono::{Duration, Local, NaiveDate};
use slint::{VecModel, Weak};

use chronobase_core::backup::{BackupEntry, BackupSourceKind};

use crate::ui::{BackupRow, MainWindow, fmt_relative, fmt_size};

/// Formats a `YYYY-MM-DD HH:MM:SS` timestamp + group name into `"<Group> · Today (2026-11-27)"`.
fn format_date_group(group_name: &str, created_at: &str) -> String {
    let date_part = created_at
        .split_whitespace()
        .next()
        .unwrap_or(created_at)
        .trim();
    let today = Local::now().date_naive();
    let yesterday = today - Duration::days(1);

    let day_label = match NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
        Ok(d) if d == today => format!("Today ({date_part})"),
        Ok(d) if d == yesterday => format!("Yesterday ({date_part})"),
        _ => date_part.to_string(),
    };
    format!("{group_name} · {day_label}")
}

fn extract_file_name(file_path: &str) -> String {
    file_path
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("—")
        .to_string()
}

/// Pushes filtered backup history rows sorted by Group + Date (when grouped) or timeline.
pub fn push_backups(weak: &Weak<MainWindow>, mut backups: Vec<BackupEntry>, grouped_by_date: bool) {
    if grouped_by_date {
        backups.sort_by(|a, b| {
            let a_date = a.created_at.split_whitespace().next().unwrap_or("");
            let b_date = b.created_at.split_whitespace().next().unwrap_or("");
            b_date
                .cmp(a_date)
                .then_with(|| {
                    a.group_name
                        .to_lowercase()
                        .cmp(&b.group_name.to_lowercase())
                })
                .then_with(|| b.created_at.cmp(&a.created_at))
        });
    }

    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        let rows: Vec<BackupRow> = backups
            .into_iter()
            .map(|b| {
                let date_group = format_date_group(&b.group_name, &b.created_at);
                let file_name = extract_file_name(&b.file_path);
                let source_kind = match b.source_kind {
                    BackupSourceKind::Direct => "Direct",
                    BackupSourceKind::VpsPull => "VPS Pull",
                };
                BackupRow {
                    id: b.id as i32,
                    connection_name: b.connection_name.into(),
                    group_name: b.group_name.into(),
                    date_group: date_group.into(),
                    file_name: file_name.into(),
                    source_kind: source_kind.into(),
                    db_type: b.db_type.as_str().into(),
                    status: b.status.as_str().into(),
                    missing_on_disk: b.missing_on_disk,
                    error_msg: b.error_msg.unwrap_or_default().into(),
                    size_text: fmt_size(b.file_size).into(),
                    when_text: fmt_relative(&b.created_at).into(),
                    exact_time: b.created_at.into(),
                }
            })
            .collect();
        win.set_backup_rows(Rc::new(VecModel::from(rows)).into());
    });
}
