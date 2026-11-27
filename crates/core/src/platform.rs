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

/// Opens the OS file manager highlighting or displaying `path` (no shell invoked).
pub fn reveal_in_file_manager(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        std::process::Command::new("xdg-open").arg(target).spawn()?;
        Ok(())
    }
}

/// Formats an XDG `.desktop` autostart entry launching `exe_path` minimized to tray.
pub fn format_xdg_autostart(exe_path: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=ChronoBase\nComment=Local-first PostgreSQL and MongoDB backup scheduler\nExec=\"{}\" --tray\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe_path.display()
    )
}

/// Formats a macOS `LaunchAgents` `.plist` launching `exe_path` minimized to tray.
pub fn format_launchagent_plist(exe_path: &Path) -> String {
    let escaped = exe_path
        .display()
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>com.chronobase.app</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{escaped}</string>\n    <string>--tray</string>\n  </array>\n  <key>RunAtLoad</key>\n  <true/>\n</dict>\n</plist>\n"
    )
}

/// Registers or removes ChronoBase from OS login startup (launching with `--tray`).
pub fn set_autostart(enabled: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows_sys::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, RegCloseKey, RegCreateKeyExW,
            RegDeleteValueW, RegSetValueExW,
        };

        struct HKeyGuard(HKEY);
        impl Drop for HKeyGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = RegCloseKey(self.0);
                }
            }
        }

        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
            .encode_utf16()
            .collect();
        let value_name: Vec<u16> = "ChronoBase\0".encode_utf16().collect();
        let mut hkey: HKEY = std::ptr::null_mut();

        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_WRITE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let _guard = HKeyGuard(hkey);

        if enabled {
            let cmd = format!("\"{}\" --tray\0", exe.display());
            let cmd_utf16: Vec<u16> = cmd.encode_utf16().collect();
            let byte_len = (cmd_utf16.len() * std::mem::size_of::<u16>()) as u32;
            let res = unsafe {
                RegSetValueExW(
                    hkey,
                    value_name.as_ptr(),
                    0,
                    REG_SZ,
                    cmd_utf16.as_ptr().cast::<u8>(),
                    byte_len,
                )
            };
            if res != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(res as i32));
            }
        } else {
            let res = unsafe { RegDeleteValueW(hkey, value_name.as_ptr()) };
            if res != ERROR_SUCCESS && res != ERROR_FILE_NOT_FOUND {
                return Err(io::Error::from_raw_os_error(res as i32));
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let base = directories::BaseDirs::new()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory unavailable"))?;
        let agents_dir = base.home_dir().join("Library").join("LaunchAgents");
        let plist_path = agents_dir.join("com.chronobase.app.plist");
        if enabled {
            std::fs::create_dir_all(&agents_dir)?;
            std::fs::write(&plist_path, format_launchagent_plist(&exe))?;
        } else if plist_path.exists() {
            std::fs::remove_file(&plist_path)?;
        }
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = directories::BaseDirs::new().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "config directory unavailable")
        })?;
        let autostart_dir = base.config_dir().join("autostart");
        let desktop_path = autostart_dir.join("chronobase.desktop");
        if enabled {
            std::fs::create_dir_all(&autostart_dir)?;
            std::fs::write(&desktop_path, format_xdg_autostart(&exe))?;
        } else if desktop_path.exists() {
            std::fs::remove_file(&desktop_path)?;
        }
        Ok(())
    }
}

/// Returns `true` if ChronoBase is currently registered in OS login autostart.
pub fn is_autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
        };

        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
            .encode_utf16()
            .collect();
        let value_name: Vec<u16> = "ChronoBase\0".encode_utf16().collect();
        let mut hkey: HKEY = std::ptr::null_mut();

        let status =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
        if status != ERROR_SUCCESS {
            return false;
        }
        let res = unsafe {
            RegQueryValueExW(
                hkey,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        res == ERROR_SUCCESS
    }

    #[cfg(target_os = "macos")]
    {
        directories::BaseDirs::new()
            .map(|b| {
                b.home_dir()
                    .join("Library")
                    .join("LaunchAgents")
                    .join("com.chronobase.app.plist")
                    .exists()
            })
            .unwrap_or(false)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        directories::BaseDirs::new()
            .map(|b| {
                b.config_dir()
                    .join("autostart")
                    .join("chronobase.desktop")
                    .exists()
            })
            .unwrap_or(false)
    }
}

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

    #[test]
    fn autostart_templates_include_tray_flag() {
        let exe = Path::new("/usr/local/bin/chronobase");
        let xdg = format_xdg_autostart(exe);
        assert!(xdg.contains("--tray"));
        assert!(xdg.contains("X-GNOME-Autostart-enabled=true"));

        let plist = format_launchagent_plist(exe);
        assert!(plist.contains("<string>--tray</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }
}
