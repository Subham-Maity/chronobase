//! Slint UI composition, callback wiring, and background command dispatch.

pub mod backups;
pub mod connections;
pub mod danger;
pub mod dashboard;
pub mod schedules;
pub mod setup;
pub mod toast;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDateTime, TimeZone};
use slint::{ComponentHandle, Weak};

use chronobase_core::backup::{BackupStatus, RestoreTarget};
use chronobase_core::connection::{CreateConnectionInput, DbKind, UpdateConnectionInput};
use chronobase_core::constant::ALL_GROUPS;
use chronobase_core::schedule::CreateScheduleInput;
use chronobase_core::secret::Secret;
use chronobase_core::system::DashboardStats;

use crate::bootstrap::{AppCommand, AppState, BackgroundRuntime};
use crate::ui::toast::{ToastKind, show_toast};

slint::include_modules!();

#[derive(Debug, Clone, Default)]
struct FilterState {
    conn_group: Option<String>,
    conn_type: Option<DbKind>,
    bk_group: Option<String>,
    bk_type: Option<DbKind>,
    bk_status: Option<BackupStatus>,
}

#[derive(Debug, Clone)]
enum PendingConfirm {
    WipeConnection(u64),
    DeleteConnection(u64),
    DeleteBackup(u64),
    DeleteSchedule(u64),
}

/// Formats a byte count into human-readable units (`0 B`, `1.4 KB`, `3.2 MB`, `1.1 GB`).
pub fn fmt_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut val = bytes as f64;
    let mut idx = 0;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    format!("{val:.1} {}", UNITS[idx])
}

/// Formats a `YYYY-MM-DD HH:MM:SS` local timestamp as relative time (`just now`, `5m ago`, `2h ago`, `3d ago`).
pub fn fmt_relative(raw: &str) -> String {
    let Ok(naive) = NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S") else {
        return raw.to_string();
    };
    let Some(dt) = Local.from_local_datetime(&naive).single() else {
        return raw.to_string();
    };
    let mins = (Local::now() - dt).num_minutes();
    if mins < 1 {
        "just now".to_string()
    } else if mins < 60 {
        format!("{mins}m ago")
    } else {
        let hours = mins / 60;
        if hours < 24 {
            format!("{hours}h ago")
        } else {
            format!("{}d ago", hours / 24)
        }
    }
}

/// Launches the Slint GUI on the current thread and spawns the dedicated background runtime.
pub fn run_gui(state: Arc<AppState>) -> ExitCode {
    let window = match MainWindow::new() {
        Ok(w) => w,
        Err(err) => {
            eprintln!("Failed to initialize GUI window: {err}");
            return ExitCode::FAILURE;
        }
    };

    let weak = window.as_weak();
    let filters = Arc::new(Mutex::new(FilterState::default()));
    let filters_bg = Arc::clone(&filters);

    let rt = match BackgroundRuntime::start(Arc::clone(&state), move |st, cmd| {
        let weak = weak.clone();
        let filters = Arc::clone(&filters_bg);
        Box::pin(async move {
            handle_command(st, cmd, &weak, &filters).await;
        })
    }) {
        Ok(rt) => Rc::new(rt),
        Err(err) => {
            eprintln!("Failed to start background runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    wire_callbacks(&window, &state, &rt, &filters);
    rt.send(AppCommand::RefreshAll);

    if let Err(err) = window.run() {
        tracing::error!(error = %err, "Slint event loop exited with error");
        if let Ok(rt_owned) = Rc::try_unwrap(rt) {
            rt_owned.shutdown();
        }
        return ExitCode::FAILURE;
    }

    if let Ok(rt_owned) = Rc::try_unwrap(rt) {
        rt_owned.shutdown();
    }
    ExitCode::SUCCESS
}

fn wire_callbacks(
    window: &MainWindow,
    state: &Arc<AppState>,
    rt: &Rc<BackgroundRuntime>,
    filters: &Arc<Mutex<FilterState>>,
) {
    let pending_confirm: Rc<RefCell<Option<PendingConfirm>>> = Rc::new(RefCell::new(None));

    // Navigation
    {
        let rt = Rc::clone(rt);
        window.on_navigate(move |_| {
            rt.send(AppCommand::RefreshAll);
        });
    }

    // Dashboard Backup All
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_backup_all(move || {
            if let Some(w) = weak.upgrade() {
                w.set_backup_all_busy(true);
            }
            rt.send(AppCommand::BackupGroup {
                group_name: ALL_GROUPS.to_string(),
            });
        });
    }

    // Connections filters & Backup Group
    {
        let rt = Rc::clone(rt);
        let st = Arc::clone(state);
        let filters = Arc::clone(filters);
        let weak = window.as_weak();
        window.on_cycle_conn_group_filter(move || {
            let groups = st.connection_service.list_groups().unwrap_or_default();
            let (group, db_type, label) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                let next = next_group_option(f.conn_group.as_deref(), &groups);
                let label = next.clone().unwrap_or_else(|| "All Groups".to_string());
                f.conn_group = next.clone();
                (next, f.conn_type, label)
            };
            if let Some(w) = weak.upgrade() {
                w.set_conn_group_filter_label(label.into());
            }
            rt.send(AppCommand::FilterConnections { group, db_type });
        });
    }
    {
        let rt = Rc::clone(rt);
        let filters = Arc::clone(filters);
        let weak = window.as_weak();
        window.on_cycle_conn_type_filter(move || {
            let (group, db_type, label) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                let next = next_type_option(f.conn_type);
                f.conn_type = next;
                let label = next.map(|t| t.display_name()).unwrap_or("All Types");
                (f.conn_group.clone(), next, label.to_string())
            };
            if let Some(w) = weak.upgrade() {
                w.set_conn_type_filter_label(label.into());
            }
            rt.send(AppCommand::FilterConnections { group, db_type });
        });
    }
    {
        let rt = Rc::clone(rt);
        let filters = Arc::clone(filters);
        let weak = window.as_weak();
        window.on_backup_group(move || {
            let group_name = {
                let f = filters.lock().unwrap_or_else(|p| p.into_inner());
                f.conn_group
                    .clone()
                    .unwrap_or_else(|| ALL_GROUPS.to_string())
            };
            if let Some(w) = weak.upgrade() {
                w.set_backup_group_busy(true);
            }
            rt.send(AppCommand::BackupGroup { group_name });
        });
    }

    // Connection CRUD & row actions
    {
        let rt = Rc::clone(rt);
        window.on_backup_connection(move |id| {
            rt.send(AppCommand::BackupConnection {
                connection_id: id as u64,
            });
        });
    }
    {
        let weak = window.as_weak();
        window.on_open_add_connection(move || {
            if let Some(w) = weak.upgrade() {
                w.set_conn_edit_id(-1);
                w.set_conn_form_name("".into());
                w.set_conn_form_group("Default".into());
                w.set_conn_form_type("postgres".into());
                w.set_conn_form_url("".into());
                w.set_conn_form_notes("".into());
                w.set_conn_modal_open(true);
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_open_edit_connection(move |id| {
            if let Ok(conn) = st.connection_service.get(id as u64)
                && let Some(w) = weak.upgrade()
            {
                w.set_conn_edit_id(conn.id as i32);
                w.set_conn_form_name(conn.name.into());
                w.set_conn_form_group(conn.group_name.into());
                w.set_conn_form_type(conn.db_type.as_str().into());
                w.set_conn_form_url(conn.url.expose().into());
                w.set_conn_form_notes(conn.notes.into());
                w.set_conn_modal_open(true);
            }
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_save_connection(move |edit_id, name, group, db_type, url, notes| {
            let kind = if db_type == "mongodb" {
                DbKind::Mongodb
            } else {
                DbKind::Postgres
            };
            if edit_id < 0 {
                rt.send(AppCommand::CreateConnection {
                    input: CreateConnectionInput {
                        name: name.into(),
                        group_name: group.into(),
                        db_type: kind,
                        url: Secret::new(url.to_string()),
                        notes: notes.into(),
                    },
                });
            } else {
                rt.send(AppCommand::UpdateConnection {
                    id: edit_id as u64,
                    input: UpdateConnectionInput {
                        name: name.into(),
                        group_name: group.into(),
                        db_type: kind,
                        url: Secret::new(url.to_string()),
                        notes: notes.into(),
                    },
                });
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        let pending = Rc::clone(&pending_confirm);
        window.on_request_wipe_connection(move |id| {
            let name = st
                .connection_service
                .get(id as u64)
                .map(|c| c.name)
                .unwrap_or_else(|_| "this database".to_string());
            *pending.borrow_mut() = Some(PendingConfirm::WipeConnection(id as u64));
            if let Some(w) = weak.upgrade() {
                w.set_confirm_title("Wipe entire database?".into());
                w.set_confirm_body(
                    format!(
                        "This will permanently delete every table, row, and collection inside {name}. Your ChronoBase connection entry is kept."
                    )
                    .into(),
                );
                w.set_confirm_label("Yes, wipe it".into());
                w.set_confirm_destructive(true);
                w.set_confirm_open(true);
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        let pending = Rc::clone(&pending_confirm);
        window.on_request_delete_connection(move |id| {
            let name = st
                .connection_service
                .get(id as u64)
                .map(|c| c.name)
                .unwrap_or_else(|_| "this connection".to_string());
            *pending.borrow_mut() = Some(PendingConfirm::DeleteConnection(id as u64));
            if let Some(w) = weak.upgrade() {
                w.set_confirm_title("Remove connection?".into());
                w.set_confirm_body(
                    format!(
                        "{name} will be removed from ChronoBase. Existing backup files on disk are kept."
                    )
                    .into(),
                );
                w.set_confirm_label("Remove".into());
                w.set_confirm_destructive(false);
                w.set_confirm_open(true);
            }
        });
    }

    // Backups filters, restore & delete
    {
        let rt = Rc::clone(rt);
        let st = Arc::clone(state);
        let filters = Arc::clone(filters);
        let weak = window.as_weak();
        window.on_cycle_bk_group_filter(move || {
            let groups = st.connection_service.list_groups().unwrap_or_default();
            let (group, db_type, status, label) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                let next = next_group_option(f.bk_group.as_deref(), &groups);
                let label = next.clone().unwrap_or_else(|| "All Groups".to_string());
                f.bk_group = next.clone();
                (next, f.bk_type, f.bk_status, label)
            };
            if let Some(w) = weak.upgrade() {
                w.set_bk_group_filter_label(label.into());
            }
            rt.send(AppCommand::FilterBackups {
                group,
                db_type,
                status,
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        let filters = Arc::clone(filters);
        let weak = window.as_weak();
        window.on_cycle_bk_type_filter(move || {
            let (group, db_type, status, label) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                let next = next_type_option(f.bk_type);
                f.bk_type = next;
                let label = next.map(|t| t.display_name()).unwrap_or("All Types");
                (f.bk_group.clone(), next, f.bk_status, label.to_string())
            };
            if let Some(w) = weak.upgrade() {
                w.set_bk_type_filter_label(label.into());
            }
            rt.send(AppCommand::FilterBackups {
                group,
                db_type,
                status,
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        let filters = Arc::clone(filters);
        let weak = window.as_weak();
        window.on_cycle_bk_status_filter(move || {
            let (group, db_type, status, label) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                let next = match f.bk_status {
                    None => Some(BackupStatus::Success),
                    Some(BackupStatus::Success) => Some(BackupStatus::Failed),
                    Some(BackupStatus::Failed) => None,
                };
                f.bk_status = next;
                let label = match next {
                    None => "All Status",
                    Some(BackupStatus::Success) => "Success",
                    Some(BackupStatus::Failed) => "Failed",
                };
                (f.bk_group.clone(), f.bk_type, next, label.to_string())
            };
            if let Some(w) = weak.upgrade() {
                w.set_bk_status_filter_label(label.into());
            }
            rt.send(AppCommand::FilterBackups {
                group,
                db_type,
                status,
            });
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_open_restore_modal(move |id| {
            let list = st
                .backup_service
                .list_filtered(None, None, None)
                .unwrap_or_default();
            let Some(entry) = list.into_iter().find(|b| b.id == id as u64) else {
                return;
            };
            let orig = st.connection_service.get(entry.connection_id).ok();
            if let Some(w) = weak.upgrade() {
                w.set_restore_backup_id(entry.id as i32);
                w.set_restore_info_db(entry.connection_name.into());
                w.set_restore_info_type(entry.db_type.as_str().into());
                w.set_restore_info_size(fmt_size(entry.file_size).into());
                w.set_restore_info_date(entry.created_at.into());
                w.set_restore_has_original(orig.is_some());
                w.set_restore_same_url_masked(
                    orig.map(|c| c.url.masked())
                        .unwrap_or_else(|| {
                            "(original connection not found — use Different Database)".to_string()
                        })
                        .into(),
                );
                w.set_restore_mode("same".into());
                w.set_restore_other_url("".into());
                w.set_restore_busy(false);
                w.set_restore_modal_open(true);
            }
        });
    }
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_submit_restore(move |id, mode, other_url| {
            if let Some(w) = weak.upgrade() {
                w.set_restore_busy(true);
            }
            let target = if mode == "same" {
                RestoreTarget::SameDatabase
            } else {
                RestoreTarget::DifferentDatabase(Secret::new(other_url.to_string()))
            };
            rt.send(AppCommand::RestoreBackup {
                backup_id: id as u64,
                target,
            });
        });
    }
    {
        let weak = window.as_weak();
        let pending = Rc::clone(&pending_confirm);
        window.on_request_delete_backup(move |id| {
            *pending.borrow_mut() = Some(PendingConfirm::DeleteBackup(id as u64));
            if let Some(w) = weak.upgrade() {
                w.set_confirm_title("Delete this backup?".into());
                w.set_confirm_body(
                    "This snapshot will be permanently removed from disk. The connection itself is not affected."
                        .into(),
                );
                w.set_confirm_label("Delete Backup".into());
                w.set_confirm_destructive(true);
                w.set_confirm_open(true);
            }
        });
    }

    // Schedules
    {
        let weak = window.as_weak();
        window.on_open_add_schedule(move || {
            if let Some(w) = weak.upgrade() {
                w.set_sched_form_name("".into());
                w.set_sched_form_group(ALL_GROUPS.into());
                w.set_sched_form_cron("0 2 * * *".into());
                w.set_sched_modal_open(true);
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_cycle_sched_group(move || {
            let groups = st.connection_service.list_groups().unwrap_or_default();
            if let Some(w) = weak.upgrade() {
                let current = w.get_sched_form_group().to_string();
                let cur_opt = if current == ALL_GROUPS {
                    None
                } else {
                    Some(current.as_str())
                };
                let next =
                    next_group_option(cur_opt, &groups).unwrap_or_else(|| ALL_GROUPS.to_string());
                w.set_sched_form_group(next.into());
            }
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_save_schedule(move |name, group, cron| {
            rt.send(AppCommand::CreateSchedule {
                input: CreateScheduleInput {
                    name: name.into(),
                    group_name: group.into(),
                    cron_expr: cron.into(),
                },
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_toggle_schedule(move |id| {
            rt.send(AppCommand::ToggleSchedule { id: id as u64 });
        });
    }
    {
        let weak = window.as_weak();
        let pending = Rc::clone(&pending_confirm);
        window.on_request_delete_schedule(move |id| {
            *pending.borrow_mut() = Some(PendingConfirm::DeleteSchedule(id as u64));
            if let Some(w) = weak.upgrade() {
                w.set_confirm_title("Delete this schedule?".into());
                w.set_confirm_body("Automated backups for this schedule will stop.".into());
                w.set_confirm_label("Delete".into());
                w.set_confirm_destructive(true);
                w.set_confirm_open(true);
            }
        });
    }

    // Setup
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_recheck_tools(move || {
            if let Some(w) = weak.upgrade() {
                w.set_recheck_busy(true);
            }
            rt.send(AppCommand::RecheckTools);
        });
    }

    // Danger Zone
    {
        let rt = Rc::clone(rt);
        window.on_submit_delete_all_backups(move |confirm| {
            rt.send(AppCommand::DeleteAllBackups {
                confirm: confirm.into(),
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_submit_reset_everything(move |confirm| {
            rt.send(AppCommand::ResetEverything {
                confirm: confirm.into(),
            });
        });
    }

    // Generic confirm resolution
    {
        let rt = Rc::clone(rt);
        let pending = Rc::clone(&pending_confirm);
        window.on_confirm_accepted(move || {
            if let Some(action) = pending.borrow_mut().take() {
                match action {
                    PendingConfirm::WipeConnection(id) => {
                        rt.send(AppCommand::WipeDatabase { connection_id: id });
                    }
                    PendingConfirm::DeleteConnection(id) => {
                        rt.send(AppCommand::DeleteConnection { id });
                    }
                    PendingConfirm::DeleteBackup(id) => {
                        rt.send(AppCommand::DeleteBackup { backup_id: id });
                    }
                    PendingConfirm::DeleteSchedule(id) => {
                        rt.send(AppCommand::DeleteSchedule { id });
                    }
                }
            }
        });
    }
    {
        let pending = Rc::clone(&pending_confirm);
        window.on_confirm_cancelled(move || {
            pending.borrow_mut().take();
        });
    }
}

async fn handle_command(
    state: Arc<AppState>,
    cmd: AppCommand,
    weak: &Weak<MainWindow>,
    filters: &Arc<Mutex<FilterState>>,
) {
    match cmd {
        AppCommand::RefreshAll => {
            refresh_all_views(&state, weak, filters).await;
        }
        AppCommand::RecheckTools => {
            let status = state.tool_discovery.refresh().await;
            setup::push_tools(weak, status);
            show_toast(weak, "Tool check complete".to_string(), ToastKind::Info);
        }
        AppCommand::FilterConnections { group, db_type } => {
            let total = state
                .connection_service
                .list_filtered(None, None)
                .map(|v| v.len())
                .unwrap_or(0);
            if let Ok(list) = state
                .connection_service
                .list_filtered(group.as_deref(), db_type)
            {
                connections::push_connections(weak, list, total);
            }
        }
        AppCommand::FilterBackups {
            group,
            db_type,
            status,
        } => {
            if let Ok(list) = state
                .backup_service
                .list_filtered(group.as_deref(), db_type, status)
            {
                backups::push_backups(weak, list);
            }
        }
        AppCommand::BackupGroup { group_name } => {
            match state.backup_service.backup_group(&group_name).await {
                Ok(results) => {
                    let ok = results
                        .iter()
                        .filter(|r| r.entry.status == BackupStatus::Success)
                        .count();
                    let fail = results.len().saturating_sub(ok);
                    let msg = format!(
                        "Done: {ok} success{}",
                        if fail > 0 {
                            format!(", {fail} failed")
                        } else {
                            String::new()
                        }
                    );
                    show_toast(
                        weak,
                        msg,
                        if fail > 0 {
                            ToastKind::Error
                        } else {
                            ToastKind::Success
                        },
                    );
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
            let weak_c = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = weak_c.upgrade() {
                    w.set_backup_all_busy(false);
                    w.set_backup_group_busy(false);
                }
            });
            refresh_all_views(&state, weak, filters).await;
        }
        AppCommand::BackupConnection { connection_id } => {
            match state.backup_service.backup_connection(connection_id).await {
                Ok(res) if res.entry.status == BackupStatus::Success => {
                    show_toast(weak, "Backup saved".to_string(), ToastKind::Success);
                }
                Ok(res) => {
                    show_toast(
                        weak,
                        res.entry
                            .error_msg
                            .unwrap_or_else(|| "Backup failed".to_string()),
                        ToastKind::Error,
                    );
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
            refresh_all_views(&state, weak, filters).await;
        }
        AppCommand::CreateConnection { input } => match state.connection_service.create(input) {
            Ok(_) => {
                let weak_c = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak_c.upgrade() {
                        w.set_conn_modal_open(false);
                    }
                });
                show_toast(weak, "Connection added".to_string(), ToastKind::Success);
                refresh_all_views(&state, weak, filters).await;
            }
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::UpdateConnection { id, input } => {
            match state.connection_service.update(id, input) {
                Ok(_) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_conn_modal_open(false);
                        }
                    });
                    show_toast(weak, "Connection updated".to_string(), ToastKind::Success);
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
        AppCommand::DeleteConnection { id } => match state.connection_service.delete(id) {
            Ok(()) => {
                show_toast(weak, "Connection removed".to_string(), ToastKind::Info);
                refresh_all_views(&state, weak, filters).await;
            }
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::WipeDatabase { connection_id } => {
            match state.backup_service.wipe_database(connection_id).await {
                Ok(()) => show_toast(weak, "Database wiped".to_string(), ToastKind::Success),
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
        AppCommand::RestoreBackup { backup_id, target } => {
            match state.backup_service.restore(backup_id, target).await {
                Ok(()) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_restore_busy(false);
                            w.set_restore_modal_open(false);
                        }
                    });
                    show_toast(weak, "Restore complete".to_string(), ToastKind::Success);
                }
                Err(err) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_restore_busy(false);
                        }
                    });
                    show_toast(weak, err.to_string(), ToastKind::Error);
                }
            }
        }
        AppCommand::DeleteBackup { backup_id } => {
            match state.backup_service.delete_backup(backup_id) {
                Ok(()) => {
                    show_toast(weak, "Backup deleted".to_string(), ToastKind::Info);
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
        AppCommand::CreateSchedule { input } => match state.schedule_service.create(input) {
            Ok(_) => {
                let weak_c = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak_c.upgrade() {
                        w.set_sched_modal_open(false);
                    }
                });
                show_toast(weak, "Schedule created".to_string(), ToastKind::Success);
                refresh_all_views(&state, weak, filters).await;
            }
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::ToggleSchedule { id } => match state.schedule_service.toggle(id) {
            Ok(_) => refresh_all_views(&state, weak, filters).await,
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::DeleteSchedule { id } => match state.schedule_service.delete(id) {
            Ok(()) => {
                show_toast(weak, "Schedule deleted".to_string(), ToastKind::Info);
                refresh_all_views(&state, weak, filters).await;
            }
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::DeleteAllBackups { confirm } => {
            match state.danger_service.delete_all_backups(&confirm) {
                Ok(()) => {
                    danger::close_danger_modals(weak);
                    show_toast(
                        weak,
                        "All backup files deleted".to_string(),
                        ToastKind::Success,
                    );
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
        AppCommand::ResetEverything { confirm } => {
            match state.danger_service.reset_everything(&confirm) {
                Ok(()) => {
                    danger::close_danger_modals(weak);
                    show_toast(
                        weak,
                        "ChronoBase has been reset".to_string(),
                        ToastKind::Info,
                    );
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
    }
}

async fn refresh_all_views(
    state: &Arc<AppState>,
    weak: &Weak<MainWindow>,
    filters: &Arc<Mutex<FilterState>>,
) {
    let f = filters.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let stats = DashboardStats::from_store(&state.store);
    let all_backups = state
        .backup_service
        .list_filtered(None, None, None)
        .unwrap_or_default();
    dashboard::push_dashboard(weak, stats, all_backups);

    let total_conns = state
        .connection_service
        .list_filtered(None, None)
        .map(|v| v.len())
        .unwrap_or(0);
    if let Ok(conns) = state
        .connection_service
        .list_filtered(f.conn_group.as_deref(), f.conn_type)
    {
        connections::push_connections(weak, conns, total_conns);
    }

    if let Ok(bks) =
        state
            .backup_service
            .list_filtered(f.bk_group.as_deref(), f.bk_type, f.bk_status)
    {
        backups::push_backups(weak, bks);
    }

    if let Ok(scheds) = state.schedule_service.list() {
        schedules::push_schedules(weak, scheds);
    }

    let tools = state.tool_discovery.status().await;
    setup::push_tools(weak, tools);
}

fn next_group_option(current: Option<&str>, groups: &[String]) -> Option<String> {
    if groups.is_empty() {
        return None;
    }
    match current {
        None => Some(groups[0].clone()),
        Some(cur) => {
            let pos = groups.iter().position(|g| g == cur);
            match pos {
                Some(i) if i + 1 < groups.len() => Some(groups[i + 1].clone()),
                _ => None,
            }
        }
    }
}

fn next_type_option(current: Option<DbKind>) -> Option<DbKind> {
    match current {
        None => Some(DbKind::Postgres),
        Some(DbKind::Postgres) => Some(DbKind::Mongodb),
        Some(DbKind::Mongodb) => None,
    }
}
