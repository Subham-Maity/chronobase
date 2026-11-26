//! Cached discovery of `pg_dump`, `psql`, `mongodump`, `mongorestore`, and `mongosh`.
//!
//! GUI applications on macOS and Linux do not inherit the user's shell `PATH`,
//! so discovery probes both `PATH` and OS-specific well-known directories
//! defined in [`crate::platform`].

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::RwLock;

use tokio::process::Command;

use crate::constant::PROBE_TIMEOUT;
use crate::platform;

/// Discovered paths for all external database CLI tools.
#[derive(Debug, Clone, Default)]
pub struct ToolStatus {
    /// Path to `pg_dump`, if found and runnable.
    pub pg_dump: Option<PathBuf>,
    /// Path to `psql`, if found and runnable.
    pub psql: Option<PathBuf>,
    /// Path to `mongodump`, if found and runnable.
    pub mongodump: Option<PathBuf>,
    /// Path to `mongorestore`, if found and runnable.
    pub mongorestore: Option<PathBuf>,
    /// Path to `mongosh` (or legacy `mongo`), if found and runnable.
    pub mongosh: Option<PathBuf>,
}

impl ToolStatus {
    /// Returns `true` if `pg_dump` is available.
    pub fn has_pg(&self) -> bool {
        self.pg_dump.is_some()
    }

    /// Returns `true` if `mongodump` is available.
    pub fn has_mongo(&self) -> bool {
        self.mongodump.is_some()
    }
}

/// Abstraction for discovering and caching external CLI tools.
pub trait ToolDiscovery: Send + Sync {
    /// Returns the cached [`ToolStatus`], probing on first call if not yet cached.
    fn status(&self) -> Pin<Box<dyn Future<Output = ToolStatus> + Send + '_>>;

    /// Forces a fresh probe of all tools and updates the cache.
    fn refresh(&self) -> Pin<Box<dyn Future<Output = ToolStatus> + Send + '_>>;
}

/// Default [`ToolDiscovery`] implementation that caches results in memory.
pub struct DefaultToolDiscovery {
    cache: RwLock<Option<ToolStatus>>,
}

impl DefaultToolDiscovery {
    /// Creates a new uncached [`DefaultToolDiscovery`].
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(None),
        }
    }
}

impl Default for DefaultToolDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDiscovery for DefaultToolDiscovery {
    fn status(&self) -> Pin<Box<dyn Future<Output = ToolStatus> + Send + '_>> {
        Box::pin(async move {
            {
                let guard = match self.cache.read() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(cached) = guard.as_ref() {
                    return cached.clone();
                }
            }
            self.refresh().await
        })
    }

    fn refresh(&self) -> Pin<Box<dyn Future<Output = ToolStatus> + Send + '_>> {
        Box::pin(async move {
            let pg_dirs = platform::known_postgres_dirs();
            let mongo_dirs = platform::known_mongo_dirs();

            let (pg_dump, psql, mongodump, mongorestore, mongosh) = tokio::join!(
                find_tool("pg_dump", &pg_dirs),
                find_tool("psql", &pg_dirs),
                find_tool("mongodump", &mongo_dirs),
                find_tool("mongorestore", &mongo_dirs),
                find_first_tool(&["mongosh", "mongo"], &mongo_dirs),
            );

            let status = ToolStatus {
                pg_dump,
                psql,
                mongodump,
                mongorestore,
                mongosh,
            };

            let mut guard = match self.cache.write() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            *guard = Some(status.clone());
            status
        })
    }
}

async fn find_first_tool(names: &[&str], extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    for name in names {
        if let Some(path) = find_tool(name, extra_dirs).await {
            return Some(path);
        }
    }
    None
}

async fn find_tool(base_name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let exe_name = platform::tool_filename(base_name);
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join(&exe_name);
            if seen.insert(p.clone()) {
                candidates.push(p);
            }
        }
    }

    for dir in extra_dirs {
        let p = dir.join(&exe_name);
        if seen.insert(p.clone()) {
            candidates.push(p);
        }
    }

    // Only probe candidates that exist on disk, plus bare exe_name as final fallback.
    for candidate in candidates {
        if candidate.is_file() && probe_executable(&candidate).await {
            return Some(candidate);
        }
    }

    let bare = PathBuf::from(&exe_name);
    if probe_executable(&bare).await {
        return Some(bare);
    }

    None
}

async fn probe_executable(bin: &Path) -> bool {
    let mut cmd = Command::new(bin);
    cmd.arg("--version").kill_on_drop(true);
    platform::configure_background_command(&mut cmd);

    match tokio::time::timeout(PROBE_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out.status.success(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn discovery_caches_result_across_calls() {
        let discovery = DefaultToolDiscovery::new();
        let first = discovery.status().await;
        let second = discovery.status().await;
        assert_eq!(first.pg_dump, second.pg_dump);
        assert_eq!(first.mongodump, second.mongodump);
    }
}
