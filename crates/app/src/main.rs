//! ChronoBase — Native desktop database backup manager (Slint) + headless/CLI runner.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod bootstrap;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use chronobase_core::backup::{BackupRunResult, BackupStatus};
use chronobase_core::bridge::{
    DEFAULT_AGENT_BIND, init_agent_config, load_agent_config, run_agent_server,
};
use chronobase_core::constant::ALL_GROUPS;
use chronobase_core::platform;
use chronobase_core::store::JsonStore;

use crate::bootstrap::AppState;

enum CliAction {
    Gui {
        start_in_tray: bool,
    },
    Headless,
    Agent {
        bind: Option<String>,
        allow_ips: Vec<String>,
    },
    AgentInit {
        bind: Option<String>,
    },
    PullVps {
        target: String,
    },
    BackupAll,
    BackupGroup(String),
    List,
    Help,
    Version,
}

struct CliArgs {
    data_dir: Option<PathBuf>,
    action: CliAction,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut args = std::env::args().skip(1).peekable();
    let mut data_dir: Option<PathBuf> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut headless = false;
    let mut start_in_tray = false;
    let mut agent = false;
    let mut bind: Option<String> = None;
    let mut allow_ips: Vec<String> = Vec::new();
    let mut backup_all = false;
    let mut backup_group: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                return Ok(CliArgs {
                    data_dir,
                    action: CliAction::Help,
                });
            }
            "-V" | "--version" => {
                return Ok(CliArgs {
                    data_dir,
                    action: CliAction::Version,
                });
            }
            "--headless" => headless = true,
            "--tray" | "--minimized" => start_in_tray = true,
            "--agent" => agent = true,
            "--bind" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--bind requires a host:port argument".to_string())?;
                bind = Some(val);
            }
            "--allow-ip" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--allow-ip requires an IP address".to_string())?;
                allow_ips.push(val);
            }
            "--data-dir" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--data-dir requires a path argument".to_string())?;
                data_dir = Some(PathBuf::from(val));
            }
            "--all" => backup_all = true,
            "--group" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--group requires a group name".to_string())?;
                backup_group = Some(val);
            }
            other if other.starts_with('-') => {
                return Err(format!("Unknown option: {other}"));
            }
            other => positional.push(other.to_string()),
        }
    }

    let action = if agent {
        CliAction::Agent { bind, allow_ips }
    } else if headless {
        CliAction::Headless
    } else if let Some(cmd) = positional.first() {
        match cmd.as_str() {
            "list" => CliAction::List,
            "agent-init" => CliAction::AgentInit { bind },
            "pull" => {
                let target = positional
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "Usage: chronobase pull <vps-name-or-id>".to_string())?;
                CliAction::PullVps { target }
            }
            "backup" => {
                if backup_all {
                    CliAction::BackupAll
                } else if let Some(group) = backup_group {
                    CliAction::BackupGroup(group)
                } else {
                    return Err("Usage: chronobase backup --all | --group <name>".to_string());
                }
            }
            other => return Err(format!("Unknown command: {other}")),
        }
    } else {
        CliAction::Gui { start_in_tray }
    };

    Ok(CliArgs { data_dir, action })
}

/// On Windows, the release binary uses `windows_subsystem = "windows"`, which
/// detaches from the parent console. Re-attach for CLI/headless output.
fn attach_parent_console_if_needed(is_gui: bool) {
    #[cfg(windows)]
    if !is_gui {
        unsafe {
            use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
            // Intentionally ignore: fails if launched without a parent console (e.g. Task Scheduler).
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = is_gui;
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Intentionally ignore: subscriber may already be initialized in tests.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn print_help() {
    println!(
        "ChronoBase {} — Local-first PostgreSQL & MongoDB backup manager\n\n\
         USAGE:\n  \
           chronobase [OPTIONS] [COMMAND]\n\n\
         COMMANDS:\n  \
           backup --all             Run backups for all connections\n  \
           backup --group <name>    Run backups for a specific group\n  \
           list                     List connections, schedules, and recent backups\n  \
           agent-init               Generate VPS pairing key and chronobase-agent.json (0600)\n  \
           pull <vps-name-or-id>    Pull a verified encrypted backup from a configured VPS peer\n\n\
         OPTIONS:\n  \
           --headless               Run background scheduler without GUI\n  \
           --tray                   Start GUI minimized to system tray\n  \
           --agent                  Run the secure VPS Pull Agent TCP server\n  \
           --bind <host:port>       Override VPS agent listen address (default: {DEFAULT_AGENT_BIND})\n  \
           --allow-ip <ip>          Allowlist client IP for --agent (repeatable)\n  \
           --data-dir <path>        Override data directory (or set CHRONOBASE_HOME)\n  \
           -h, --help               Print this help message\n  \
           -V, --version            Print version",
        env!("CARGO_PKG_VERSION")
    );
}

fn run_cli_list(state: &AppState) -> ExitCode {
    let conns = match state.connection_service.list_filtered(None, None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading connections: {e}");
            return ExitCode::FAILURE;
        }
    };
    let scheds = match state.schedule_service.list() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading schedules: {e}");
            return ExitCode::FAILURE;
        }
    };
    let backups = match state.backup_service.list_filtered(None, None, None) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error reading backups: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!("Connections ({}):", conns.len());
    for c in &conns {
        println!(
            "  [{}] {} | group={} | type={} | url={}",
            c.id,
            c.name,
            c.group_name,
            c.db_type.as_str(),
            c.url.masked()
        );
    }

    println!("\nSchedules ({}):", scheds.len());
    for s in &scheds {
        println!(
            "  [{}] {} | group={} | cron=\"{}\" | enabled={} | last_run={}",
            s.id,
            s.name,
            s.group_name,
            s.cron_expr,
            s.enabled,
            s.last_run.as_deref().unwrap_or("never")
        );
    }

    println!("\nRecent Backups (showing up to 10 of {}):", backups.len());
    for b in backups.iter().take(10) {
        println!(
            "  [{}] {} | {} | {} | {} bytes | {}",
            b.id,
            b.connection_name,
            b.status.as_str(),
            b.created_at,
            b.file_size,
            b.file_path
        );
    }

    ExitCode::SUCCESS
}

fn run_cli_backup(state: Arc<AppState>, group: &str) -> ExitCode {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let results: Result<Vec<BackupRunResult>, _> =
        rt.block_on(state.bridge_service.backup_all_in_group(group));
    if let Err(e) = state.store.flush_now() {
        eprintln!("Warning: failed to flush store: {e}");
    }

    match results {
        Ok(list) if list.is_empty() => {
            println!("No connections matched group '{group}'.");
            ExitCode::SUCCESS
        }
        Ok(list) => {
            let mut any_failed = false;
            for r in &list {
                match r.entry.status {
                    BackupStatus::Success => {
                        println!(
                            "OK     {} ({} bytes) -> {}",
                            r.connection_name, r.entry.file_size, r.entry.file_path
                        );
                    }
                    BackupStatus::Failed => {
                        any_failed = true;
                        eprintln!(
                            "FAILED {} -> {}",
                            r.connection_name,
                            r.entry.error_msg.as_deref().unwrap_or("unknown error")
                        );
                    }
                }
            }
            if any_failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("Backup failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_headless(state: Arc<AppState>) -> ExitCode {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async move {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let flush_handle =
            JsonStore::spawn_flush_loop(Arc::clone(&state.store), shutdown_rx.clone());
        let sched_handle = Arc::clone(&state.schedule_runner).spawn(shutdown_rx);

        tracing::info!(
            data_dir = %state.store.data_dir().display(),
            "ChronoBase headless scheduler started"
        );

        wait_for_shutdown_signal().await;
        tracing::info!("shutdown signal received, flushing store");

        let _ = shutdown_tx.send(true);
        let _ = sched_handle.await;
        let _ = flush_handle.await;
        if let Err(e) = state.store.flush_now() {
            tracing::error!(error = %e, "final store flush failed");
            return ExitCode::FAILURE;
        }
        ExitCode::SUCCESS
    })
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(ref mut s) = sigterm {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        // Intentionally ignore: ctrl_c future only errors on platform signal failure.
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn run_cli_agent_init(data_dir: &std::path::Path, bind: Option<&str>) -> ExitCode {
    match init_agent_config(data_dir, bind) {
        Ok((path, key, fingerprint)) => {
            let bind_str = bind.unwrap_or(DEFAULT_AGENT_BIND);
            println!("ChronoBase VPS Agent initialized: {}", path.display());
            println!("Bind address:        {bind_str}");
            println!("SHA-256 fingerprint: {fingerprint}");
            println!("Pairing key (copy into ChronoBase -> VPS & Migrate -> Add VPS):");
            println!("  {}", key.expose());
            println!();
            println!("Start the VPS agent with:");
            println!(
                "  chronobase --agent --bind {bind_str} --data-dir {}",
                data_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Failed to initialize VPS agent config: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli_agent(data_dir: PathBuf, bind: Option<String>, allow_ips: Vec<String>) -> ExitCode {
    let config = match load_agent_config(&data_dir, bind.as_deref(), allow_ips) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async move {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        println!(
            "ChronoBase VPS Pull Agent listening on {} (data_dir: {})",
            config.bind_addr,
            data_dir.display()
        );
        tokio::select! {
            res = run_agent_server(data_dir, config, shutdown_rx) => {
                if let Err(err) = res {
                    eprintln!("VPS agent exited with error: {err}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            }
            () = wait_for_shutdown_signal() => {
                tracing::info!("shutdown signal received, stopping VPS agent");
                let _ = shutdown_tx.send(true);
                ExitCode::SUCCESS
            }
        }
    })
}

fn run_cli_pull(state: Arc<AppState>, target: &str) -> ExitCode {
    let peers = match state.bridge_service.list_peers() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to read VPS peers: {e}");
            return ExitCode::FAILURE;
        }
    };

    let peer_id = if let Ok(id) = target.parse::<u64>() {
        peers.iter().find(|p| p.id == id).map(|p| p.id)
    } else {
        peers
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(target))
            .map(|p| p.id)
    };

    let Some(peer_id) = peer_id else {
        eprintln!("No VPS peer found matching '{target}'.");
        return ExitCode::FAILURE;
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async move {
        let exit = match state.bridge_service.pull_from_vps(peer_id).await {
            Ok(res) => {
                println!(
                    "OK {} -> {} ({} bytes, remote staging deleted: {}, migrated to local DB: {})",
                    res.peer_name,
                    res.entry.file_path,
                    res.entry.file_size,
                    res.remote_deleted,
                    res.restored_to_local_db
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("VPS pull failed: {e}");
                ExitCode::FAILURE
            }
        };
        if let Err(e) = state.store.flush_now() {
            eprintln!("Warning: failed to flush store: {e}");
        }
        exit
    })
}

fn main() -> ExitCode {
    let parsed = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            attach_parent_console_if_needed(false);
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };

    let is_gui = matches!(parsed.action, CliAction::Gui { .. });
    attach_parent_console_if_needed(is_gui);
    init_tracing();

    match parsed.action {
        CliAction::Help => {
            print_help();
            return ExitCode::SUCCESS;
        }
        CliAction::Version => {
            println!("chronobase {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let data_dir = match platform::resolve_data_dir(parsed.data_dir.as_deref()) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("Failed to resolve data directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    match parsed.action {
        CliAction::AgentInit { bind } => {
            return run_cli_agent_init(&data_dir, bind.as_deref());
        }
        CliAction::Agent { bind, allow_ips } => {
            return run_cli_agent(data_dir, bind, allow_ips);
        }
        _ => {}
    }

    let state = match AppState::build(&data_dir) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Failed to initialize ChronoBase store: {e}");
            return ExitCode::FAILURE;
        }
    };

    match parsed.action {
        CliAction::List => run_cli_list(&state),
        CliAction::PullVps { target } => run_cli_pull(state, &target),
        CliAction::BackupAll => run_cli_backup(state, ALL_GROUPS),
        CliAction::BackupGroup(group) => run_cli_backup(state, &group),
        CliAction::Headless => run_headless(state),
        CliAction::Gui { start_in_tray } => ui::run_gui(state, start_in_tray),
        CliAction::Help
        | CliAction::Version
        | CliAction::AgentInit { .. }
        | CliAction::Agent { .. } => ExitCode::SUCCESS,
    }
}
