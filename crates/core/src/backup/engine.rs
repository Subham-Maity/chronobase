//! Subprocess execution engine for `pg_dump`, `psql`, `mongodump`, `mongorestore`, and `mongosh`.
//!
//! Security guarantees:
//! - **Zero shell invocation:** every command uses `tokio::process::Command` with an
//!   explicit argument array (`kill_on_drop(true)` and strict timeouts).
//! - **Secrets out of `argv`:** PostgreSQL passwords are passed via `PGPASSWORD` env;
//!   MongoDB URIs are written to a `0600` temporary config/script file and deleted
//!   immediately via RAII guard (falling back to `--uri` only if `--config` is unsupported).

use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::backup::BackupError;
use crate::constant::MAX_STDERR_BYTES;
use crate::platform;
use crate::secret::split_postgres_url;

/// Specification for a single external tool invocation.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Executable path.
    pub program: PathBuf,
    /// Argument array (never passed through a shell).
    pub args: Vec<String>,
    /// Environment variables to set on the child process.
    pub env: Vec<(String, String)>,
    /// Maximum execution duration before killing the child.
    pub timeout: Duration,
}

/// Captured outcome of a subprocess invocation.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// Whether the process exited with status 0.
    pub success: bool,
    /// Exit code if terminated normally.
    pub exit_code: Option<i32>,
    /// Stderr output capped at [`MAX_STDERR_BYTES`].
    pub stderr_tail: String,
}

/// Trait abstracting subprocess execution so the backup engine can be unit-tested
/// with `mockall` without spawning real database tools.
#[cfg_attr(test, mockall::automock)]
pub trait CommandRunner: Send + Sync {
    /// Runs the specified command with its configured timeout and returns captured stderr.
    fn run(
        &self,
        spec: CommandSpec,
    ) -> Pin<Box<dyn Future<Output = Result<CommandOutput, BackupError>> + Send>>;
}

/// Production [`CommandRunner`] using `tokio::process::Command`.
#[derive(Debug, Default)]
pub struct TokioCommandRunner;

impl TokioCommandRunner {
    /// Creates a new [`TokioCommandRunner`].
    pub fn new() -> Self {
        Self
    }
}

impl CommandRunner for TokioCommandRunner {
    fn run(
        &self,
        spec: CommandSpec,
    ) -> Pin<Box<dyn Future<Output = Result<CommandOutput, BackupError>> + Send>> {
        Box::pin(async move {
            let mut cmd = Command::new(&spec.program);
            cmd.args(&spec.args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            for (k, v) in &spec.env {
                cmd.env(k, v);
            }
            platform::configure_background_command(&mut cmd);

            let mut child = cmd.spawn().map_err(BackupError::Io)?;
            let mut stderr_pipe = child.stderr.take();

            let stderr_task = tokio::spawn(async move {
                let mut buf = Vec::with_capacity(1024);
                if let Some(ref mut pipe) = stderr_pipe {
                    let mut limited = pipe.take(MAX_STDERR_BYTES as u64);
                    // Intentionally ignore: partial stderr is still useful on error.
                    let _ = limited.read_to_end(&mut buf).await;
                }
                String::from_utf8_lossy(&buf).trim().to_string()
            });

            let status = match tokio::time::timeout(spec.timeout, child.wait()).await {
                Ok(res) => res.map_err(BackupError::Io)?,
                Err(_) => {
                    // Intentionally ignore: child may have already exited.
                    let _ = child.kill().await;
                    let _ = stderr_task.await;
                    return Err(BackupError::TimedOut(spec.timeout.as_secs()));
                }
            };

            let stderr_tail = stderr_task.await.unwrap_or_default();
            Ok(CommandOutput {
                success: status.success(),
                exit_code: status.code(),
                stderr_tail,
            })
        })
    }
}

/// RAII guard for a `0600` temporary credential file deleted on drop.
pub struct TempSecretFile {
    path: PathBuf,
}

impl TempSecretFile {
    /// Creates a private (`0600` on Unix) temporary file in `dir` with `contents`.
    pub fn create(
        dir: &Path,
        prefix: &str,
        ext: &str,
        contents: &str,
    ) -> Result<Self, BackupError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let path = dir.join(format!(".{prefix}_{pid}_{nanos}.{ext}"));
        let mut file = platform::create_private_file(&path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(Self { path })
    }

    /// Returns the path to the temporary file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSecretFile {
    fn drop(&mut self) {
        // Intentionally ignore: best-effort cleanup of temporary secret file.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Builds a `pg_dump` [`CommandSpec`] with the password moved to `PGPASSWORD`.
pub fn build_pg_dump_spec(
    pg_dump_bin: PathBuf,
    raw_url: &str,
    output_file: &Path,
    timeout: Duration,
) -> Result<CommandSpec, BackupError> {
    let (clean_url, password) = split_postgres_url(raw_url)
        .map_err(|e| BackupError::Validation(format!("Invalid PostgreSQL URL: {e}")))?;

    let mut env = Vec::new();
    if let Some(pass) = password {
        env.push(("PGPASSWORD".to_string(), pass));
    }

    Ok(CommandSpec {
        program: pg_dump_bin,
        args: vec![
            clean_url,
            "--clean".to_string(),
            "--if-exists".to_string(),
            "--no-owner".to_string(),
            "-f".to_string(),
            output_file.to_string_lossy().into_owned(),
        ],
        env,
        timeout,
    })
}

/// Builds a `psql` restore [`CommandSpec`] with the password moved to `PGPASSWORD`.
pub fn build_psql_restore_spec(
    psql_bin: PathBuf,
    raw_url: &str,
    input_file: &Path,
    timeout: Duration,
) -> Result<CommandSpec, BackupError> {
    let (clean_url, password) = split_postgres_url(raw_url)
        .map_err(|e| BackupError::Validation(format!("Invalid PostgreSQL URL: {e}")))?;

    let mut env = Vec::new();
    if let Some(pass) = password {
        env.push(("PGPASSWORD".to_string(), pass));
    }

    Ok(CommandSpec {
        program: psql_bin,
        args: vec![
            clean_url,
            "-v".to_string(),
            "ON_ERROR_STOP=1".to_string(),
            "--single-transaction".to_string(),
            "-f".to_string(),
            input_file.to_string_lossy().into_owned(),
        ],
        env,
        timeout,
    })
}

/// Builds a `psql` wipe [`CommandSpec`] with the password moved to `PGPASSWORD`.
pub fn build_psql_wipe_spec(
    psql_bin: PathBuf,
    raw_url: &str,
    timeout: Duration,
) -> Result<CommandSpec, BackupError> {
    let (clean_url, password) = split_postgres_url(raw_url)
        .map_err(|e| BackupError::Validation(format!("Invalid PostgreSQL URL: {e}")))?;

    let mut env = Vec::new();
    if let Some(pass) = password {
        env.push(("PGPASSWORD".to_string(), pass));
    }

    Ok(CommandSpec {
        program: psql_bin,
        args: vec![
            clean_url,
            "-v".to_string(),
            "ON_ERROR_STOP=1".to_string(),
            "-c".to_string(),
            "DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO public;"
                .to_string(),
        ],
        env,
        timeout,
    })
}

/// Builds a `mongodump` [`CommandSpec`] using `--config` (preferred) or `--uri` (fallback).
pub fn build_mongodump_spec(
    mongodump_bin: PathBuf,
    config_path: Option<&Path>,
    raw_url: &str,
    out_dir: &Path,
    timeout: Duration,
) -> CommandSpec {
    let auth_arg = match config_path {
        Some(cfg) => format!("--config={}", cfg.to_string_lossy()),
        None => format!("--uri={raw_url}"),
    };
    CommandSpec {
        program: mongodump_bin,
        args: vec![auth_arg, format!("--out={}", out_dir.to_string_lossy())],
        env: Vec::new(),
        timeout,
    }
}

/// Builds a `mongorestore` [`CommandSpec`] using `--config` (preferred) or `--uri` (fallback).
pub fn build_mongorestore_spec(
    mongorestore_bin: PathBuf,
    config_path: Option<&Path>,
    raw_url: &str,
    backup_dir: &Path,
    drop_collections: bool,
    timeout: Duration,
) -> CommandSpec {
    let auth_arg = match config_path {
        Some(cfg) => format!("--config={}", cfg.to_string_lossy()),
        None => format!("--uri={raw_url}"),
    };
    let mut args = vec![auth_arg];
    if drop_collections {
        args.push("--drop".to_string());
    }
    args.push(backup_dir.to_string_lossy().into_owned());
    CommandSpec {
        program: mongorestore_bin,
        args,
        env: Vec::new(),
        timeout,
    }
}

/// Returns `true` if `mongodump`/`mongorestore` stderr indicates `--config` is unsupported.
pub fn is_config_flag_unsupported(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("unknown option") || lower.contains("unknown flag") || lower.contains("--config")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant::BACKUP_TIMEOUT;

    #[test]
    fn pg_dump_spec_keeps_password_out_of_argv() {
        let spec = build_pg_dump_spec(
            PathBuf::from("pg_dump"),
            "postgresql://alice:TopSecret123@db.local:5432/prod",
            Path::new("/tmp/out.sql"),
            BACKUP_TIMEOUT,
        )
        .expect("spec");

        for arg in &spec.args {
            assert!(
                !arg.contains("TopSecret123"),
                "password must never appear in argv: {arg}"
            );
        }
        assert!(spec.args.contains(&"--clean".to_string()));
        assert!(spec.args.contains(&"--if-exists".to_string()));
        assert!(spec.args.contains(&"--no-owner".to_string()));
        assert_eq!(
            spec.env,
            vec![("PGPASSWORD".to_string(), "TopSecret123".to_string())]
        );
    }

    #[test]
    fn psql_restore_spec_uses_single_transaction_and_on_error_stop() {
        let spec = build_psql_restore_spec(
            PathBuf::from("psql"),
            "postgresql://alice:pw@db.local:5432/prod",
            Path::new("/tmp/in.sql"),
            BACKUP_TIMEOUT,
        )
        .expect("spec");

        for arg in &spec.args {
            assert!(!arg.contains("pw"));
        }
        assert!(spec.args.contains(&"ON_ERROR_STOP=1".to_string()));
        assert!(spec.args.contains(&"--single-transaction".to_string()));
        assert_eq!(spec.env, vec![("PGPASSWORD".to_string(), "pw".to_string())]);
    }
}
