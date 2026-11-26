//! Backups view-model mapping.

use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::backup::BackupEntry;

use crate::ui::{BackupRow, MainWindow, fmt_relative, fmt_size};

/// Pushes filtered backup history rows to the virtualized `ListView` model.
pub fn push_backups(weak: &Weak<MainWindow>, backups: Vec<BackupEntry>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        let rows: Vec<BackupRow> = backups
            .into_iter()
            .map(|b| BackupRow {
                id: b.id as i32,
                connection_name: b.connection_name.into(),
                group_name: b.group_name.into(),
                db_type: b.db_type.as_str().into(),
                status: b.status.as_str().into(),
                missing_on_disk: b.missing_on_disk,
                error_msg: b.error_msg.unwrap_or_default().into(),
                size_text: fmt_size(b.file_size).into(),
                when_text: fmt_relative(&b.created_at).into(),
                exact_time: b.created_at.into(),
            })
            .collect();
        win.set_backup_rows(Rc::new(VecModel::from(rows)).into());
    });
}
