//! Platform-specific helpers isolated behind `cfg` gates.
//!
//! Business logic must never sprinkle `#[cfg(...)]` attributes; all OS
//! differences (executable extensions, `CREATE_NO_WINDOW`, `0600` file
//! permissions, known tool directories, and data directories) live here.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::constant::DATA_FILE_NAME;

/// Returns the platform-appropriate executable filename for a CLI tool
/// (appends `.exe` on Windows).
pub fn tool_filename(base: &str) -> String {
    #[cfg(windows)]
    {
        format!("{base}.exe")
    }
    #[cfg(not(windows))]
    {
        base.to_string()
    }
}

/// Configures a `tokio::process::Command` so it never flashes a console window
/// on Windows when invoked from the GUI process.
pub fn configure_background_command(cmd: &mut Command) {
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW (0x0800_0000) prevents child console tools (pg_dump,
        // psql, mongodump, mongorestore, mongosh) from popping up a visible
        // terminal window on Windows when launched from the GUI subsystem.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Creates a new file with restrictive owner-only permissions (`0600` on Unix)
/// so temporary database credential configs and lock files cannot be read by
/// other local users.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Mode 0o600 ensures only the owning user can read or write the file,
        // protecting temporary MongoDB URI config files from other local users.
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        // On Windows, the file is placed inside the user's private data/temp
        // directory (inherited user-only ACL) and deleted immediately after use.
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
    }
}

/// Opens or creates a lock file with owner-only permissions (`0600` on Unix)
/// for advisory `fs4` locking.
pub fn open_lock_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
    }
}

/// Returns well-known directories to search for PostgreSQL CLI tools (`pg_dump`, `psql`)
/// in addition to `PATH` (since GUI apps on macOS/Linux do not inherit shell PATH).
pub fn known_postgres_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            // Homebrew's libpq is keg-only and not symlinked into /opt/homebrew/bin by default.
            PathBuf::from("/opt/homebrew/opt/libpq/bin"),
            PathBuf::from("/usr/local/opt/libpq/bin"),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        let mut dirs = vec![PathBuf::from("/usr/bin"), PathBuf::from("/usr/local/bin")];
        if let Ok(entries) = std::fs::read_dir("/usr/lib/postgresql") {
            let mut versions: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path().join("bin"))
                .filter(|p| p.is_dir())
                .collect();
            // Prefer newest PostgreSQL version first.
            versions.sort();
            versions.reverse();
            dirs.extend(versions);
        }
        dirs
    }
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        let pg_root = Path::new(r"C:\Program Files\PostgreSQL");
        if let Ok(entries) = std::fs::read_dir(pg_root) {
            let mut versions: Vec<(u32, PathBuf)> = entries
                .flatten()
                .filter_map(|e| {
                    let bin = e.path().join("bin");
                    if bin.is_dir() {
                        let ver = e.file_name().to_string_lossy().parse::<u32>().unwrap_or(0);
                        Some((ver, bin))
                    } else {
                        None
                    }
                })
                .collect();
            versions.sort_by_key(|a| std::cmp::Reverse(a.0));
            dirs.extend(versions.into_iter().map(|(_, p)| p));
        }
        dirs
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Vec::new()
    }
}

/// Returns well-known directories to search for MongoDB Database Tools (`mongodump`,
/// `mongorestore`, `mongosh`) in addition to `PATH`.
pub fn known_mongo_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![PathBuf::from("/usr/bin"), PathBuf::from("/usr/local/bin")]
    }
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        for root in [
            Path::new(r"C:\Program Files\MongoDB\Tools"),
            Path::new(r"C:\Program Files\MongoDB\Server"),
            Path::new(r"C:\Program Files\mongosh"),
        ] {
            if let Ok(entries) = std::fs::read_dir(root) {
                let mut versions: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path().join("bin"))
                    .filter(|p| p.is_dir())
                    .collect();
                versions.sort();
                versions.reverse();
                dirs.extend(versions);
            }
        }
        dirs
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Vec::new()
    }
}

/// Resolves the active data directory according to ChronoBase's precedence rules:
/// 1. Explicit override (`--data-dir` or `CHRONOBASE_HOME` env var)
/// 2. Portable mode: next to the executable if `chronobase-data.json` exists there
/// 3. Portable mode: current working directory if `chronobase-data.json` exists there
/// 4. OS user data directory via `directories::ProjectDirs`
pub fn resolve_data_dir(cli_override: Option<&Path>) -> io::Result<PathBuf> {
    if let Some(dir) = cli_override {
        return Ok(dir.to_path_buf());
    }

    if let Ok(env_dir) = std::env::var("CHRONOBASE_HOME") {
        let trimmed = env_dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
        && exe_dir.join(DATA_FILE_NAME).exists()
    {
        return Ok(exe_dir.to_path_buf());
    }

    if let Ok(cwd) = std::env::current_dir()
        && cwd.join(DATA_FILE_NAME).exists()
    {
        return Ok(cwd);
    }

    if let Some(proj) = directories::ProjectDirs::from("com", "chronobase", "ChronoBase") {
        return Ok(proj.data_dir().to_path_buf());
    }

    std::env::current_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_filename_matches_platform() {
        let name = tool_filename("pg_dump");
        if cfg!(windows) {
            assert_eq!(name, "pg_dump.exe");
        } else {
            assert_eq!(name, "pg_dump");
        }
    }

    #[test]
    fn resolve_data_dir_prefers_explicit_override() {
        let custom = Path::new("/tmp/chronobase-custom");
        let resolved = resolve_data_dir(Some(custom)).expect("resolve");
        assert_eq!(resolved, custom);
    }
}
