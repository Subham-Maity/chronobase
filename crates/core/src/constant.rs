//! Application-wide constants, timeouts, and safety limits.

use std::time::Duration;

/// Name of the JSON persistence file (compatible with ChronoBase v1).
pub const DATA_FILE_NAME: &str = "chronobase-data.json";

/// Name of the advisory lock file preventing concurrent store corruption.
pub const LOCK_FILE_NAME: &str = "chronobase.lock";

/// Directory name under the data directory where backup files are stored.
pub const BACKUPS_DIR_NAME: &str = "backups";

/// Default group name assigned when a connection's group is omitted or blank.
pub const DEFAULT_GROUP: &str = "Default";

/// Special group selector targeting all connections.
pub const ALL_GROUPS: &str = "All";

/// Maximum number of backup history entries retained in the store (matches v1).
pub const BACKUP_HISTORY_CAP: usize = 500;

/// Maximum time allowed for a backup or restore subprocess (10 minutes).
pub const BACKUP_TIMEOUT: Duration = Duration::from_secs(600);

/// Maximum time allowed for a database wipe subprocess (60 seconds).
pub const WIPE_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum time allowed for probing a CLI tool with `--version` (5 seconds).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of bytes captured from a subprocess's stderr (~8 KB).
pub const MAX_STDERR_BYTES: usize = 8 * 1024;

/// Confirmation phrase required to delete all backup files.
pub const PHRASE_DELETE_BACKUPS: &str = "DELETE BACKUPS";

/// Confirmation phrase required to reset all ChronoBase data.
pub const PHRASE_DELETE_ALL: &str = "DELETE EVERYTHING";

/// Debounce window before flushing in-memory store changes to disk.
pub const STORE_FLUSH_DEBOUNCE: Duration = Duration::from_millis(250);

/// Current JSON schema version written by this version of ChronoBase.
pub const CURRENT_SCHEMA_VERSION: u32 = 4;
