//! Dedicated background thread running a `tokio` current-thread runtime
//! for all I/O, subprocess execution, VPS pulls, and the cron scheduler.

use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::{mpsc, watch};

use chronobase_core::backup::RestoreTarget;
use chronobase_core::bridge::{CreateVpsPeerInput, MigrateEndpoint, UpdateVpsPeerInput};
use chronobase_core::connection::{CreateConnectionInput, DbKind, UpdateConnectionInput};
use chronobase_core::schedule::CreateScheduleInput;
use chronobase_core::store::JsonStore;

use crate::bootstrap::AppState;

/// Commands dispatched from the Slint UI thread to the background runtime.
#[derive(Debug)]
pub enum AppCommand {
    /// Refresh all UI state from the store and cached tools.
    RefreshAll,
    /// Force a fresh probe of external CLI tools (`pg_dump`, `mongodump`, etc.).
    RecheckTools,
    /// Backup all connections (or a specific group).
    BackupGroup { group_name: String },
    /// Backup a single connection by ID.
    BackupConnection { connection_id: u64 },
    /// Create a new connection.
    CreateConnection { input: CreateConnectionInput },
    /// Update an existing connection.
    UpdateConnection {
        id: u64,
        input: UpdateConnectionInput,
    },
    /// Delete a connection by ID.
    DeleteConnection { id: u64 },
    /// Wipe all data inside a connection's database.
    WipeDatabase { connection_id: u64 },
    /// Restore a backup snapshot.
    RestoreBackup {
        backup_id: u64,
        target: RestoreTarget,
    },
    /// Delete a backup snapshot and its file on disk.
    DeleteBackup { backup_id: u64 },
    /// Reveal a backup file/folder in the OS file manager.
    RevealBackup { backup_id: u64 },
    /// Create a new cron schedule.
    CreateSchedule { input: CreateScheduleInput },
    /// Toggle enable/pause on a schedule.
    ToggleSchedule { id: u64 },
    /// Delete a schedule.
    DeleteSchedule { id: u64 },
    /// Create a new VPS pull peer.
    CreateVpsPeer { input: CreateVpsPeerInput },
    /// Update an existing VPS pull peer.
    UpdateVpsPeer { id: u64, input: UpdateVpsPeerInput },
    /// Delete a VPS pull peer.
    DeleteVpsPeer { id: u64 },
    /// Pull backup from a VPS peer (and optionally migrate into local DB).
    PullFromVps { peer_id: u64 },
    /// Direct or VPS-to-local one-click DB migration.
    MigrateEndpoints {
        source: MigrateEndpoint,
        dest: MigrateEndpoint,
    },
    /// Toggle running minimized in system tray on window close.
    SetRunInBackground { enabled: bool },
    /// Toggle launching ChronoBase minimized to tray at OS login.
    SetRunOnStartup { enabled: bool },
    /// Danger Zone: delete all backup files (`DELETE BACKUPS`).
    DeleteAllBackups { confirm: String },
    /// Danger Zone: reset everything (`DELETE EVERYTHING`).
    ResetEverything { confirm: String },
    /// Apply connection filter in Rust and push updated model.
    FilterConnections {
        group: Option<String>,
        db_type: Option<DbKind>,
    },
    /// Apply backup filter in Rust and push updated model.
    FilterBackups {
        group: Option<String>,
        db_type: Option<DbKind>,
        status: Option<chronobase_core::backup::BackupStatus>,
    },
}

/// Handle to the dedicated background runtime thread.
pub struct BackgroundRuntime {
    tx: mpsc::UnboundedSender<AppCommand>,
    shutdown_tx: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

impl BackgroundRuntime {
    /// Starts the dedicated background OS thread with a `current_thread` tokio runtime.
    pub fn start<F>(state: Arc<AppState>, on_command: F) -> Result<Self, std::io::Error>
    where
        F: Fn(
                Arc<AppState>,
                AppCommand,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let (tx, mut rx) = mpsc::unbounded_channel::<AppCommand>();
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let thread = std::thread::Builder::new()
            .name("chronobase-rt".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => {
                        tracing::error!(error = %err, "failed to build tokio runtime");
                        return;
                    }
                };

                rt.block_on(async move {
                    let flush_handle =
                        JsonStore::spawn_flush_loop(Arc::clone(&state.store), shutdown_rx.clone());
                    let sched_handle =
                        Arc::clone(&state.schedule_runner).spawn(shutdown_rx.clone());

                    let handler = Arc::new(on_command);
                    loop {
                        tokio::select! {
                            maybe_cmd = rx.recv() => {
                                match maybe_cmd {
                                    Some(cmd) => {
                                        let st = Arc::clone(&state);
                                        let h = Arc::clone(&handler);
                                        tokio::spawn(async move {
                                            h(st, cmd).await;
                                        });
                                    }
                                    None => break,
                                }
                            }
                            changed = shutdown_rx.changed() => {
                                if changed.is_err() || *shutdown_rx.borrow() {
                                    break;
                                }
                            }
                        }
                    }

                    // Intentionally ignore: shutdown tasks log their own errors.
                    let _ = sched_handle.await;
                    let _ = flush_handle.await;
                });
            })?;

        Ok(Self {
            tx,
            shutdown_tx,
            thread: Some(thread),
        })
    }

    /// Sends a command to the background runtime.
    pub fn send(&self, cmd: AppCommand) {
        // Intentionally ignore: channel only closes during shutdown.
        let _ = self.tx.send(cmd);
    }

    /// Signals shutdown, flushes the store, and joins the background thread.
    pub fn shutdown(mut self) {
        // Intentionally ignore: receiver may already be dropped.
        let _ = self.shutdown_tx.send(true);
        if let Some(handle) = self.thread.take() {
            // Intentionally ignore: thread panic is already reported by runtime.
            let _ = handle.join();
        }
    }
}
