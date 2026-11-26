//! Schedules view-model mapping.

use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::schedule::Schedule;

use crate::ui::{MainWindow, ScheduleRow};

/// Pushes schedule rows to the UI thread.
pub fn push_schedules(weak: &Weak<MainWindow>, schedules: Vec<Schedule>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        let rows: Vec<ScheduleRow> = schedules
            .into_iter()
            .map(|s| ScheduleRow {
                id: s.id as i32,
                name: s.name.into(),
                group_name: s.group_name.into(),
                cron_expr: s.cron_expr.into(),
                enabled: s.enabled,
                last_run: s.last_run.unwrap_or_else(|| "never".to_string()).into(),
            })
            .collect();
        win.set_schedule_rows(Rc::new(VecModel::from(rows)).into());
    });
}
