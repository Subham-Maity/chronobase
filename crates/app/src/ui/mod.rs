//! Slint UI composition, callback wiring, and background command dispatch.

pub mod backups;
pub mod bridge;
pub mod connections;
pub mod danger;
pub mod dashboard;
pub mod schedules;
pub mod setup;
pub mod toast;

use std::cell::RefCell;
use std::collections::HashMap;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDateTime, TimeZone};
use slint::{CloseRequestResponse, ComponentHandle, Weak};

use chronobase_core::backup::{BackupStatus, RestoreTarget};
use chronobase_core::bridge::{
    CreateVpsPeerInput, MigrateEndpoint, PullTargetMode, UpdateVpsPeerInput, generate_pairing_key,
    key_fingerprint,
};
use chronobase_core::connection::{CreateConnectionInput, DbKind, UpdateConnectionInput};
use chronobase_core::constant::ALL_GROUPS;
use chronobase_core::schedule::CreateScheduleInput;
use chronobase_core::secret::Secret;
use chronobase_core::system::DashboardStats;

use crate::bootstrap::{AppCommand, AppState, BackgroundRuntime};
use crate::ui::toast::{ToastKind, show_toast};

slint::include_modules!();

#[derive(Debug, Clone)]
struct FilterState {
    conn_group: Option<String>,
    conn_type: Option<DbKind>,
    bk_group: Option<String>,
    bk_type: Option<DbKind>,
    bk_status: Option<BackupStatus>,
    bk_grouped_by_date: bool,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            conn_group: None,
            conn_type: None,
            bk_group: None,
            bk_type: None,
            bk_status: None,
            bk_grouped_by_date: true,
        }
    }
}

#[derive(Debug, Clone)]
enum PendingConfirm {
    WipeConnection(u64),
    DeleteConnection(u64),
    DeleteBackup(u64),
    DeleteSchedule(u64),
    DeleteVpsPeer(u64),
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

/// Launches the Slint GUI (or starts minimized to the system tray) and spawns the background runtime.
pub fn run_gui(state: Arc<AppState>, start_in_tray: bool) -> ExitCode {
    let window = match MainWindow::new() {
        Ok(w) => w,
        Err(err) => {
            eprintln!("Failed to initialize GUI window: {err}");
            return ExitCode::FAILURE;
        }
    };

    let settings = state.settings_service.get();
    window.set_bg_run_enabled(settings.run_in_background);
    window.set_startup_enabled(settings.run_on_startup);

    let tray = match AppTray::new() {
        Ok(t) => Some(t),
        Err(err) => {
            tracing::warn!(error = %err, "System tray is unavailable in this desktop session");
            None
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

    if let Some(ref tray_inst) = tray {
        if settings.run_in_background || start_in_tray {
            let _ = tray_inst.show();
        }
        let weak_open = window.as_weak();
        tray_inst.on_open_window(move || {
            if let Some(w) = weak_open.upgrade() {
                let _ = w.show();
            }
        });
        let rt_bk = Rc::clone(&rt);
        let weak_bk = window.as_weak();
        tray_inst.on_backup_all_now(move || {
            if let Some(w) = weak_bk.upgrade() {
                w.set_backup_all_busy(true);
            }
            rt_bk.send(AppCommand::BackupGroup {
                group_name: ALL_GROUPS.to_string(),
            });
        });
        tray_inst.on_quit_app(move || {
            let _ = slint::quit_event_loop();
        });
    }

    {
        let st = Arc::clone(&state);
        let weak_close = window.as_weak();
        window.window().on_close_requested(move || {
            let bg_enabled = st.settings_service.get().run_in_background;
            if bg_enabled {
                if let Some(w) = weak_close.upgrade() {
                    let _ = w.hide();
                }
                CloseRequestResponse::HideWindow
            } else {
                let _ = slint::quit_event_loop();
                CloseRequestResponse::KeepWindowShown
            }
        });
    }

    let tray_weak = tray.as_ref().map(|t| t.as_weak());
    wire_callbacks(&window, &state, &rt, &filters, tray_weak);
    rt.send(AppCommand::RefreshAll);

    if !start_in_tray && let Err(err) = window.show() {
        tracing::error!(error = %err, "Failed to show main window");
        if let Ok(rt_owned) = Rc::try_unwrap(rt) {
            rt_owned.shutdown();
        }
        return ExitCode::FAILURE;
    }

    if let Err(err) = slint::run_event_loop() {
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
    tray_weak: Option<Weak<AppTray>>,
) {
    let pending_confirm: Rc<RefCell<Option<PendingConfirm>>> = Rc::new(RefCell::new(None));

    // Sidebar Background Engine & OS Startup toggles
    {
        let rt = Rc::clone(rt);
        let tray_w = tray_weak.clone();
        window.on_toggle_run_in_background(move |enabled| {
            if let Some(ref tw) = tray_w
                && let Some(t) = tw.upgrade()
            {
                if enabled {
                    let _ = t.show();
                } else {
                    let _ = t.hide();
                }
            }
            rt.send(AppCommand::SetRunInBackground { enabled });
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_toggle_run_on_startup(move |enabled| {
            rt.send(AppCommand::SetRunOnStartup { enabled });
        });
    }

    // Navigation
    {
        let rt = Rc::clone(rt);
        window.on_navigate(move |_| {
            rt.send(AppCommand::RefreshAll);
        });
    }

    // Dashboard Backup All & Group Backup
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
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_backup_group_named(move |group_name| {
            if let Some(w) = weak.upgrade() {
                w.set_backup_group_busy(true);
            }
            rt.send(AppCommand::BackupGroup {
                group_name: group_name.into(),
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

    // Connection + VPS unified CRUD & row actions
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_backup_one_entry(move |origin, id| {
            if origin == "vps" {
                if let Some(w) = weak.upgrade() {
                    w.set_pull_busy(true);
                }
                rt.send(AppCommand::PullFromVps { peer_id: id as u64 });
            } else {
                rt.send(AppCommand::BackupConnection {
                    connection_id: id as u64,
                });
            }
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
        window.on_edit_one_entry(move |origin, id| {
            if origin == "vps" {
                open_edit_vps_modal(&st, &weak, id);
            } else if let Ok(conn) = st.connection_service.get(id as u64)
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
        window.on_delete_one_entry(move |origin, id| {
            if origin == "vps" {
                *pending.borrow_mut() = Some(PendingConfirm::DeleteVpsPeer(id as u64));
                if let Some(w) = weak.upgrade() {
                    w.set_confirm_title("Delete this VPS peer?".into());
                    w.set_confirm_body(
                        "Removes this VPS peer configuration locally. Existing snapshots in backups/ are kept."
                            .into(),
                    );
                    w.set_confirm_label("Delete VPS peer".into());
                    w.set_confirm_destructive(true);
                    w.set_confirm_open(true);
                }
            } else {
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
        window.on_toggle_bk_grouped_view(move |grouped| {
            let (group, db_type, status) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                f.bk_grouped_by_date = grouped;
                (f.bk_group.clone(), f.bk_type, f.bk_status)
            };
            rt.send(AppCommand::FilterBackups {
                group,
                db_type,
                status,
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_reveal_backup(move |id| {
            rt.send(AppCommand::RevealBackup {
                backup_id: id as u64,
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
        window.on_toggle_bk_grouped_view(move |grouped| {
            let (group, db_type, status) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                f.bk_grouped_by_date = grouped;
                (f.bk_group.clone(), f.bk_type, f.bk_status)
            };
            rt.send(AppCommand::FilterBackups {
                group,
                db_type,
                status,
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_reveal_backup(move |id| {
            rt.send(AppCommand::RevealBackup {
                backup_id: id as u64,
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
        let rt = Rc::clone(rt);
        let filters = Arc::clone(filters);
        window.on_toggle_bk_grouped_view(move |grouped| {
            let (group, db_type, status) = {
                let mut f = filters.lock().unwrap_or_else(|p| p.into_inner());
                f.bk_grouped_by_date = grouped;
                (f.bk_group.clone(), f.bk_type, f.bk_status)
            };
            rt.send(AppCommand::FilterBackups {
                group,
                db_type,
                status,
            });
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_reveal_backup(move |id| {
            rt.send(AppCommand::RevealBackup {
                backup_id: id as u64,
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
                    PendingConfirm::DeleteVpsPeer(id) => {
                        rt.send(AppCommand::DeleteVpsPeer { id });
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

    // Per-tab Page Guide (i) modal
    {
        let weak = window.as_weak();
        window.on_open_page_guide(move |page| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let (title, purpose, s1, s2, s3, s4) = page_guide_content(page.as_str());
            w.set_guide_title(title.into());
            w.set_guide_purpose(purpose.into());
            w.set_guide_step_1(s1.into());
            w.set_guide_step_2(s2.into());
            w.set_guide_step_3(s3.into());
            w.set_guide_step_4(s4.into());
            w.set_guide_open(true);
        });
    }

    // VPS Pull Bridge & One-Click DB-to-DB Migrate
    {
        let weak = window.as_weak();
        window.on_open_add_vps(move || {
            if let Some(w) = weak.upgrade() {
                w.set_vps_edit_id(-1);
                w.set_vps_form_name("".into());
                w.set_vps_form_group("Default".into());
                w.set_vps_form_endpoint("".into());
                w.set_vps_form_type("postgres".into());
                w.set_vps_form_remote_url("".into());
                w.set_vps_form_key("".into());
                w.set_vps_form_fingerprint("Generate or paste pairing key (min 16 chars)".into());
                w.set_vps_form_target_id(0);
                w.set_vps_form_target_label("Local backups/ storage ONLY".into());
                w.set_vps_modal_open(true);
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_open_edit_vps(move |id| {
            open_edit_vps_modal(&st, &weak, id);
        });
    }
    {
        let weak = window.as_weak();
        window.on_generate_vps_key(move || match generate_pairing_key() {
            Ok(key) => {
                let fp = key_fingerprint(&key);
                if let Some(w) = weak.upgrade() {
                    w.set_vps_form_key(key.expose().into());
                    w.set_vps_form_fingerprint(fp.into());
                }
            }
            Err(err) => {
                show_toast(&weak, err.to_string(), ToastKind::Error);
            }
        });
    }
    {
        let weak = window.as_weak();
        window.on_preview_vps_fingerprint(move |key| {
            let trimmed = key.trim();
            let fp = if trimmed.len() >= 16 {
                key_fingerprint(&Secret::new(trimmed.to_string()))
            } else {
                "Key must be at least 16 chars".to_string()
            };
            if let Some(w) = weak.upgrade() {
                w.set_vps_form_fingerprint(fp.into());
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_cycle_vps_target(move || {
            let conns = st
                .connection_service
                .list_filtered(None, None)
                .unwrap_or_default();
            let mut options: Vec<(i32, String)> =
                vec![(0, "Local backups/ storage ONLY".to_string())];
            for c in conns {
                options.push((
                    c.id as i32,
                    format!("Local DB: {} ({})", c.name, c.db_type.as_str()),
                ));
            }
            if let Some(w) = weak.upgrade() {
                let current = w.get_vps_form_target_id();
                let idx = options
                    .iter()
                    .position(|(id, _)| *id == current)
                    .map(|i| (i + 1) % options.len())
                    .unwrap_or(0);
                let (next_id, ref next_label) = options[idx];
                w.set_vps_form_target_id(next_id);
                w.set_vps_form_target_label(next_label.as_str().into());
            }
        });
    }
    {
        let rt = Rc::clone(rt);
        window.on_save_vps_peer(
            move |edit_id, name, group_name, endpoint, db_type, remote_url, key, target_id| {
                let target_mode = if target_id <= 0 {
                    PullTargetMode::StorageOnly
                } else {
                    PullTargetMode::LocalConnection(target_id as u64)
                };
                let db_kind = if db_type == "mongodb" {
                    DbKind::Mongodb
                } else {
                    DbKind::Postgres
                };
                if edit_id < 0 {
                    rt.send(AppCommand::CreateVpsPeer {
                        input: CreateVpsPeerInput {
                            name: name.into(),
                            group_name: group_name.into(),
                            endpoint: endpoint.into(),
                            db_type: db_kind,
                            remote_url: Secret::new(remote_url.to_string()),
                            pairing_key: Secret::new(key.to_string()),
                            target_mode,
                        },
                    });
                } else {
                    rt.send(AppCommand::UpdateVpsPeer {
                        id: edit_id as u64,
                        input: UpdateVpsPeerInput {
                            name: name.into(),
                            group_name: group_name.into(),
                            endpoint: endpoint.into(),
                            db_type: db_kind,
                            remote_url: Secret::new(remote_url.to_string()),
                            pairing_key: Secret::new(key.to_string()),
                            target_mode,
                        },
                    });
                }
            },
        );
    }
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_pull_vps_peer(move |id| {
            if let Some(w) = weak.upgrade() {
                w.set_pull_busy(true);
            }
            rt.send(AppCommand::PullFromVps { peer_id: id as u64 });
        });
    }
    {
        let weak = window.as_weak();
        let pending = Rc::clone(&pending_confirm);
        window.on_request_delete_vps_peer(move |id| {
            *pending.borrow_mut() = Some(PendingConfirm::DeleteVpsPeer(id as u64));
            if let Some(w) = weak.upgrade() {
                w.set_confirm_title("Delete this VPS peer?".into());
                w.set_confirm_body(
                    "Removes this VPS peer configuration locally. Existing snapshots in backups/ are kept."
                        .into(),
                );
                w.set_confirm_label("Delete VPS peer".into());
                w.set_confirm_destructive(true);
                w.set_confirm_open(true);
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_open_migrate_modal(move || {
            let sources = build_migrate_sources(&st);
            let conns = st
                .connection_service
                .list_filtered(None, None)
                .unwrap_or_default();
            if conns.is_empty() || (sources.len() < 2) {
                show_toast(
                    &weak,
                    "Add a destination connection plus at least one other connection or VPS peer to migrate"
                        .to_string(),
                    ToastKind::Error,
                );
                return;
            }
            // Prefer starting on a VPS peer if available, otherwise first connection
            let (src_kind, src_id, src_type, src_label) = sources
                .iter()
                .find(|(k, _, _, _)| *k == "vps")
                .cloned()
                .unwrap_or_else(|| sources[0].clone());
            let dst = conns
                .iter()
                .find(|c| {
                    !(src_kind == "conn" && c.id == src_id) && c.db_type == src_type
                })
                .or_else(|| {
                    conns
                        .iter()
                        .find(|c| !(src_kind == "conn" && c.id == src_id))
                })
                .unwrap_or(&conns[0]);
            if let Some(w) = weak.upgrade() {
                w.set_migrate_src_kind(src_kind.into());
                w.set_migrate_src_id(src_id as i32);
                w.set_migrate_src_label(src_label.into());
                w.set_migrate_dst_id(dst.id as i32);
                w.set_migrate_dst_label(format!("{} ({})", dst.name, dst.db_type.as_str()).into());
                w.set_migrate_busy(false);
                w.set_migrate_modal_open(true);
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_cycle_migrate_src(move || {
            let sources = build_migrate_sources(&st);
            if sources.is_empty() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let cur_kind = w.get_migrate_src_kind();
                let cur_id = w.get_migrate_src_id();
                let idx = sources
                    .iter()
                    .position(|(k, id, _, _)| *k == cur_kind.as_str() && *id as i32 == cur_id)
                    .map(|i| (i + 1) % sources.len())
                    .unwrap_or(0);
                let (kind, id, _, ref label) = sources[idx];
                w.set_migrate_src_kind(kind.into());
                w.set_migrate_src_id(id as i32);
                w.set_migrate_src_label(label.as_str().into());
            }
        });
    }
    {
        let st = Arc::clone(state);
        let weak = window.as_weak();
        window.on_cycle_migrate_dst(move || {
            let conns = st
                .connection_service
                .list_filtered(None, None)
                .unwrap_or_default();
            if conns.is_empty() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let src_kind = w.get_migrate_src_kind();
                let src_id = w.get_migrate_src_id();
                let candidates: Vec<_> = if conns.len() > 1 {
                    conns
                        .iter()
                        .filter(|c| !(src_kind == "conn" && c.id as i32 == src_id))
                        .collect()
                } else {
                    conns.iter().collect()
                };
                if candidates.is_empty() {
                    return;
                }
                let current = w.get_migrate_dst_id();
                let idx = candidates
                    .iter()
                    .position(|c| c.id as i32 == current)
                    .map(|i| (i + 1) % candidates.len())
                    .unwrap_or(0);
                let next = candidates[idx];
                w.set_migrate_dst_id(next.id as i32);
                w.set_migrate_dst_label(
                    format!("{} ({})", next.name, next.db_type.as_str()).into(),
                );
            }
        });
    }
    {
        let rt = Rc::clone(rt);
        let weak = window.as_weak();
        window.on_submit_migrate(move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let src_kind = w.get_migrate_src_kind();
            let src_id = w.get_migrate_src_id();
            let dst_id = w.get_migrate_dst_id();
            if src_id <= 0 || dst_id <= 0 || (src_kind == "conn" && src_id == dst_id) {
                show_toast(
                    &weak,
                    "Choose a valid source (VPS or DB) and a distinct destination connection"
                        .to_string(),
                    ToastKind::Error,
                );
                return;
            }
            let source = if src_kind == "vps" {
                MigrateEndpoint::VpsPeer(src_id as u64)
            } else {
                MigrateEndpoint::Connection(src_id as u64)
            };
            w.set_migrate_busy(true);
            rt.send(AppCommand::MigrateEndpoints {
                source,
                dest: MigrateEndpoint::Connection(dst_id as u64),
            });
        });
    }
}

fn open_edit_vps_modal(st: &Arc<AppState>, weak: &Weak<MainWindow>, id: i32) {
    let Ok(peer) = st.bridge_service.get_peer(id as u64) else {
        return;
    };
    let conns = st
        .connection_service
        .list_filtered(None, None)
        .unwrap_or_default();
    let conn_names: HashMap<u64, String> = conns.into_iter().map(|c| (c.id, c.name)).collect();
    let (target_id, target_label) = match peer.target_mode {
        PullTargetMode::StorageOnly => (0, "Local backups/ storage ONLY".to_string()),
        PullTargetMode::LocalConnection(cid) => (
            cid as i32,
            bridge::format_target_label(peer.target_mode, &conn_names),
        ),
    };
    let fp = key_fingerprint(&peer.pairing_key);
    if let Some(w) = weak.upgrade() {
        w.set_vps_edit_id(peer.id as i32);
        w.set_vps_form_name(peer.name.into());
        w.set_vps_form_group(peer.group_name.into());
        w.set_vps_form_endpoint(peer.endpoint.into());
        w.set_vps_form_type(peer.db_type.as_str().into());
        w.set_vps_form_remote_url(peer.remote_url.expose().into());
        w.set_vps_form_key(peer.pairing_key.expose().into());
        w.set_vps_form_fingerprint(fp.into());
        w.set_vps_form_target_id(target_id);
        w.set_vps_form_target_label(target_label.into());
        w.set_vps_modal_open(true);
    }
}

fn build_migrate_sources(st: &Arc<AppState>) -> Vec<(&'static str, u64, DbKind, String)> {
    let mut out = Vec::new();
    if let Ok(peers) = st.bridge_service.list_peers() {
        for p in peers {
            out.push((
                "vps",
                p.id,
                p.db_type,
                format!("VPS: {} ({})", p.name, p.db_type.as_str()),
            ));
        }
    }
    if let Ok(conns) = st.connection_service.list_filtered(None, None) {
        for c in conns {
            out.push((
                "conn",
                c.id,
                c.db_type,
                format!("{} ({})", c.name, c.db_type.as_str()),
            ));
        }
    }
    out
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
            let all_conns = state
                .connection_service
                .list_filtered(None, None)
                .unwrap_or_default();
            let peers = state.bridge_service.list_peers().unwrap_or_default();
            let total = all_conns.len() + peers.len();
            if let Ok(list) = state
                .connection_service
                .list_filtered(group.as_deref(), db_type)
            {
                let filtered_peers = peers
                    .into_iter()
                    .filter(|p| {
                        group
                            .as_deref()
                            .is_none_or(|g| p.group_name.eq_ignore_ascii_case(g))
                            && db_type.is_none_or(|t| p.db_type == t)
                    })
                    .collect();
                connections::push_connections(weak, list, filtered_peers, &all_conns, total);
            }
        }
        AppCommand::FilterBackups {
            group,
            db_type,
            status,
        } => {
            let grouped = filters
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .bk_grouped_by_date;
            if let Ok(list) = state
                .backup_service
                .list_filtered(group.as_deref(), db_type, status)
            {
                backups::push_backups(weak, list, grouped);
            }
        }
        AppCommand::BackupGroup { group_name } => {
            match state.bridge_service.backup_all_in_group(&group_name).await {
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
        AppCommand::CreateVpsPeer { input } => match state.bridge_service.create_peer(input) {
            Ok(_) => {
                let weak_c = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak_c.upgrade() {
                        w.set_vps_modal_open(false);
                    }
                });
                show_toast(weak, "VPS peer added".to_string(), ToastKind::Success);
                refresh_all_views(&state, weak, filters).await;
            }
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::UpdateVpsPeer { id, input } => {
            match state.bridge_service.update_peer(id, input) {
                Ok(_) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_vps_modal_open(false);
                        }
                    });
                    show_toast(weak, "VPS peer updated".to_string(), ToastKind::Success);
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
        AppCommand::DeleteVpsPeer { id } => match state.bridge_service.delete_peer(id) {
            Ok(()) => {
                show_toast(weak, "VPS peer removed".to_string(), ToastKind::Info);
                refresh_all_views(&state, weak, filters).await;
            }
            Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
        },
        AppCommand::PullFromVps { peer_id } => {
            match state.bridge_service.pull_from_vps(peer_id).await {
                Ok(res) => {
                    let msg = format!(
                        "Pulled {} ({}){}",
                        res.peer_name,
                        fmt_size(res.entry.file_size),
                        if res.restored_to_local_db {
                            " & restored to local DB"
                        } else {
                            " (VPS staging deleted)"
                        }
                    );
                    show_toast(weak, msg, ToastKind::Success);
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
            let weak_c = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = weak_c.upgrade() {
                    w.set_pull_busy(false);
                }
            });
            refresh_all_views(&state, weak, filters).await;
        }
        AppCommand::RevealBackup { backup_id } => {
            if let Err(err) = state.backup_service.reveal_backup(backup_id) {
                show_toast(weak, err.to_string(), ToastKind::Error);
            }
        }
        AppCommand::MigrateEndpoints { source, dest } => {
            match state.bridge_service.migrate_endpoints(source, dest).await {
                Ok(res) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_migrate_busy(false);
                            w.set_migrate_modal_open(false);
                        }
                    });
                    show_toast(
                        weak,
                        format!(
                            "Migrated {} -> {} ({})",
                            res.source_name,
                            res.dest_name,
                            fmt_size(res.entry.file_size)
                        ),
                        ToastKind::Success,
                    );
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_migrate_busy(false);
                        }
                    });
                    show_toast(weak, err.to_string(), ToastKind::Error);
                }
            }
        }
        AppCommand::SetRunInBackground { enabled } => {
            match state.settings_service.set_run_in_background(enabled) {
                Ok(_) => {
                    show_toast(
                        weak,
                        if enabled {
                            "Background mode enabled — closing window keeps ChronoBase in tray"
                                .to_string()
                        } else {
                            "Background mode disabled".to_string()
                        },
                        ToastKind::Info,
                    );
                    refresh_all_views(&state, weak, filters).await;
                }
                Err(err) => show_toast(weak, err.to_string(), ToastKind::Error),
            }
        }
        AppCommand::SetRunOnStartup { enabled } => {
            match state.settings_service.set_run_on_startup(enabled) {
                Ok(_) => {
                    show_toast(
                        weak,
                        if enabled {
                            "ChronoBase will start in tray on OS login".to_string()
                        } else {
                            "OS startup disabled".to_string()
                        },
                        ToastKind::Info,
                    );
                }
                Err(err) => {
                    let weak_c = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak_c.upgrade() {
                            w.set_startup_enabled(!enabled);
                        }
                    });
                    show_toast(weak, err.to_string(), ToastKind::Error);
                }
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

fn page_guide_content(
    page: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match page {
        "connections" => (
            "Connections Guide",
            "Store PostgreSQL and MongoDB URLs organised by group. Credentials are redacted in logs and masked on screen.",
            "Click Add connection, pick PostgreSQL or MongoDB, assign a group, and paste your connection URL.",
            "Filter by Group or Type, then click Back up group to snapshot an entire environment together.",
            "Run single-row Backup, Edit, or Wipe (requires confirmation) directly from any connection row.",
            "Run `chronobase --headless` in Task Scheduler or systemd for unattended background execution.",
        ),
        "backups" => (
            "Backups Guide",
            "Browse local .sql and MongoDB BSON snapshots stored under backups/, inspect status/size, or restore into any database.",
            "Filter backup history by group, engine type, or Success/Failed status.",
            "Click Restore on any healthy snapshot to overwrite its source DB or clone into a different URL.",
            "Failed runs record diagnostic tool output so you can spot wrong credentials or missing binaries quickly.",
            "All backup files remain portable plain dumps inside your ChronoBase backups/ folder.",
        ),
        "schedules" => (
            "Schedules Guide",
            "Run automatic recurring snapshots on standard 5-field cron expressions using local wall-clock time.",
            "Click Add schedule, give it a clear name, and cycle Target group (All Connections or one group).",
            "Choose a preset chip (Daily 2am, Every 6h, Hourly, Weekly, Monthly) or enter custom 5-field cron.",
            "Toggle switches let you pause or resume schedules at any time without losing settings.",
            "Keep ChronoBase open or run `chronobase --headless` so scheduled jobs fire reliably.",
        ),
        "bridge" => (
            "VPS & Migrate Guide",
            "Pull encrypted snapshots from a remote VPS without exposing DB ports, OR migrate directly from one saved DB to another in one click.",
            "On the VPS run `chronobase agent-init` to generate chronobase-agent.json (0600) & 256-bit key, then start `chronobase --agent --bind 0.0.0.0:9443`.",
            "Click Add VPS peer here, enter the VPS endpoint, remote DB URL (localhost inside VPS), pairing key, and Local storage or Local DB target.",
            "Click Pull Now: authenticates via mutual HMAC-SHA256 + ChaCha20-Poly1305 AEAD, checks SHA-256, and deletes the VPS staging dump automatically.",
            "For two already configured local/remote connections, click Migrate DB -> DB and pick Source -> Destination in one click.",
        ),
        "setup" => (
            "Setup Guide",
            "Verify native PostgreSQL (pg_dump / psql) and MongoDB (mongodump / mongorestore) binaries are installed and discoverable.",
            "Install PostgreSQL Client Tools and MongoDB Database Tools using the OS tabs below.",
            "Click Re-check Tools in the header once installation completes to update sidebar pills.",
            "ChronoBase scans PATH and common OS installation directories automatically.",
            "For remote VPS pulls, install these same DB tools on the VPS where `chronobase --agent` runs.",
        ),
        "danger" => (
            "Danger Zone Guide",
            "Execute irreversible storage cleanup or a complete application reset with explicit typed confirmation.",
            "Delete all backup files wipes backups/ contents and backup history while keeping Connections, Schedules, and VPS peers.",
            "Reset everything clears connections, schedules, VPS peers, history, and files on disk back to a blank state.",
            "Both modals require typing the exact uppercase confirmation phrase before the destructive action enables.",
            "Need to empty just one database? Use Wipe on that specific row inside the Connections tab.",
        ),
        _ => (
            "Dashboard Guide",
            "Monitor overall protection status, total snapshots, disk usage, active schedules, and kick off an immediate Back up all.",
            "Add databases under Connections or configure a remote VPS peer under VPS & Migrate.",
            "Click Back up all in the header to run snapshots concurrently across all connections.",
            "Use Quick navigation tiles to jump straight to Schedules, Backups, VPS Pull, or Setup.",
            "Check Recent snapshots below for instant visibility into status, size, and relative timestamp.",
        ),
    }
}

async fn refresh_all_views(
    state: &Arc<AppState>,
    weak: &Weak<MainWindow>,
    filters: &Arc<Mutex<FilterState>>,
) {
    let f = filters.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let tools = state.tool_discovery.status().await;
    let settings = state.settings_service.get();
    let stats = DashboardStats::from_store(&state.store);
    let all_backups = state
        .backup_service
        .list_filtered(None, None, None)
        .unwrap_or_default();
    dashboard::push_dashboard(
        weak,
        stats,
        all_backups,
        &tools,
        state.store.data_dir(),
        &settings,
    );

    let all_conns = state
        .connection_service
        .list_filtered(None, None)
        .unwrap_or_default();
    let all_peers = state.bridge_service.list_peers().unwrap_or_default();
    let total_targets = all_conns.len() + all_peers.len();
    if let Ok(conns) = state
        .connection_service
        .list_filtered(f.conn_group.as_deref(), f.conn_type)
    {
        let filtered_peers = all_peers
            .iter()
            .filter(|p| {
                f.conn_group
                    .as_deref()
                    .is_none_or(|g| p.group_name.eq_ignore_ascii_case(g))
                    && f.conn_type.is_none_or(|t| p.db_type == t)
            })
            .cloned()
            .collect();
        connections::push_connections(weak, conns, filtered_peers, &all_conns, total_targets);
    }

    if let Ok(bks) =
        state
            .backup_service
            .list_filtered(f.bk_group.as_deref(), f.bk_type, f.bk_status)
    {
        backups::push_backups(weak, bks, f.bk_grouped_by_date);
    }

    if let Ok(scheds) = state.schedule_service.list() {
        schedules::push_schedules(weak, scheds);
    }

    bridge::push_vps_peers(weak, all_peers, &all_conns);
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
