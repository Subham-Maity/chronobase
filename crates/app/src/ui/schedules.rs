//! Schedules view-model mapping.

use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::retention::CleanupFrequency;
use chronobase_core::schedule::Schedule;
use chronobase_core::store::AppSettings;

use crate::ui::{MainWindow, ScheduleRow};

/// Pushes schedule rows and retention settings to the UI thread.
pub fn push_schedules(weak: &Weak<MainWindow>, schedules: Vec<Schedule>, settings: &AppSettings) {
    let settings = settings.clone();
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

        win.set_retention_enabled(settings.retention.enabled);
        win.set_retention_keep_last(
            settings
                .retention
                .keep_last
                .map(|v| v.to_string())
                .unwrap_or_default()
                .into(),
        );
        win.set_retention_max_age(
            settings
                .retention
                .max_age_days
                .map(|v| v.to_string())
                .unwrap_or_default()
                .into(),
        );
        win.set_retention_failed_age(
            settings
                .retention
                .failed_max_age_days
                .map(|v| v.to_string())
                .unwrap_or_default()
                .into(),
        );
        win.set_retention_freq_kind(settings.cleanup_frequency.kind_str().into());
        if let CleanupFrequency::Custom(expr) = settings.cleanup_frequency {
            win.set_retention_custom_cron(expr.into());
        }
    });
}
