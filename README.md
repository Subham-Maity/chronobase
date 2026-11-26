<div align="center">

<br />

# ChronoBase

### The backup tool you'll wish you had *before* that incident.

<p>Local-first native desktop backup manager for PostgreSQL and MongoDB.<br/>
Written in pure Rust + Slint — zero cloud, tiny binary, instant startup.</p>

<br />

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.95%2B-orange?style=flat-square)](https://www.rust-lang.org)
[![Built with Slint](https://img.shields.io/badge/UI-Slint-2379F4?style=flat-square)](https://slint.dev)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square)](./CONTRIBUTING.md)

<br />

</div>

---

## Supported Platforms

| Platform | Target | Status | Notes |
|---|---|---|---|
| Windows 10 / 11 (x64) | `x86_64-pc-windows-msvc` | Supported & Tested | Portable `.zip` |
| Linux (x86_64, X11 & Wayland) | `x86_64-unknown-linux-gnu` | Supported (CI) | `.tar.gz` & `.deb`; `software` feature runs without GPU |
| macOS Apple Silicon | `aarch64-apple-darwin` | Supported (CI) | `.dmg` & `.app.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | Build-only (CI) | Cross-compiled in release; not tested on Intel hardware |

> Looking for the original Node.js/Express version? It is preserved read-only in [`v1/`](./v1).

---

## What it does

- **Multi-database:** PostgreSQL (`pg_dump` / `psql`) and MongoDB (`mongodump` / `mongorestore` / `mongosh`) in one native app.
- **Group connections:** Organise by project (`Production`, `Staging`, `Default`) with masked URLs (`:***@`).
- **One-click & group backups:** Backup a single database, a filtered group, or all connections at once.
- **Automated cron schedules:** 5-field local-time cron expressions with quick presets (`Daily 2am`, `Every 6h`, `Hourly`, `Weekly Sun`, `Monthly`).
- **Safe restore & wipe:** Restore to the same database (`--clean --if-exists --no-owner`, `--single-transaction`, `mongorestore --drop`) or push to a different target.
- **Hardened security:** Zero shell invocation, `PGPASSWORD` env for Postgres, `0600` temp config files for MongoDB, canonicalized path-containment checks before deletion, `fs4` advisory locking, and atomic store writes.
- **Headless & CLI modes:** Run `chronobase --headless` as a background service or script backups with `chronobase backup --all | --group <name>`.

---

## Architecture

```
chronobase/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── core/                   # chronobase-core (UI-free, fully unit & integration tested)
│   │   └── src/
│   │       ├── store/          # Atomic JSON persistence, fs4 lock, v1 path re-basing
│   │       ├── connection/     # Connection CRUD, validation, URL masking
│   │       ├── backup/         # CommandRunner + pg_dump/psql/mongodump/mongorestore/mongosh
│   │       ├── schedule/       # 5-field cron validation & single-task ScheduleRunner
│   │       ├── tools/          # Cached cross-platform CLI tool discovery
│   │       └── system/         # Dashboard stats & Danger Zone phrase verification
│   └── app/                    # chronobase binary (Slint UI + CLI + headless daemon)
│       ├── src/                # main.rs, bootstrap/ (tokio current-thread runtime), ui/
│       └── ui/                 # Slint theme, SVG icons, Geist fonts, components, pages
├── scripts/                    # Windows, Linux, and macOS packaging scripts
└── .github/workflows/          # CI and Release automation
```

---

## Prerequisites

ChronoBase invokes the official PostgreSQL and MongoDB CLI tools directly (not bundled).

### 1. PostgreSQL (`pg_dump` & `psql`)

- **Windows:** Install from [postgresql.org/download/windows](https://www.postgresql.org/download/windows) (Command Line Tools). ChronoBase automatically scans `PATH` and `C:\Program Files\PostgreSQL\*\bin`.
- **macOS:** `brew install libpq && brew link --force libpq`
- **Linux (Ubuntu / Debian):** `sudo apt install postgresql-client`

### 2. MongoDB (`mongodump`, `mongorestore`, `mongosh`)

- **Windows:** Install Database Tools MSI from [mongodb.com/try/download/database-tools](https://www.mongodb.com/try/download/database-tools).
- **macOS:** `brew tap mongodb/brew && brew install mongodb-database-tools`
- **Linux (Ubuntu / Debian):** `sudo apt install mongo-tools`

> **Atlas users:** `mongodb+srv://` URLs work directly with `mongodump` and `mongorestore`.

---

## Build & Run

### Linux system packages (build-time)
```bash
sudo apt-get install -y libfontconfig1-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-dev libxcb1-dev libwayland-dev libgl1-mesa-dev
```

### Build & run
```bash
# Default (hardware-accelerated femtovg renderer)
cargo build --release --locked -p chronobase
cargo run --release -p chronobase

# Software renderer (no GPU required)
cargo build --release --locked -p chronobase --no-default-features --features software
```

---

## Headless & Auto-start on Boot

Run ChronoBase's scheduler in the background without opening a window:

```bash
chronobase --headless
```

<details>
<summary><strong>Windows — Task Scheduler or Startup folder</strong></summary>

1. Press `Win + R` → `shell:startup` → Enter.
2. Create a shortcut to `chronobase.exe` and append `--headless` to the **Target** field (e.g. `"C:\Tools\ChronoBase\chronobase.exe" --headless`).
</details>

<details>
<summary><strong>macOS — launchd</strong></summary>

Create `~/Library/LaunchAgents/com.chronobase.plist`:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>             <string>com.chronobase</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Applications/ChronoBase.app/Contents/MacOS/chronobase</string>
    <string>--headless</string>
  </array>
  <key>RunAtLoad</key>         <true/>
  <key>KeepAlive</key>         <true/>
</dict>
</plist>
```
Load with `launchctl load ~/Library/LaunchAgents/com.chronobase.plist`.
</details>

<details>
<summary><strong>Linux — systemd user unit</strong></summary>

Create `~/.config/systemd/user/chronobase.service`:
```ini
[Unit]
Description=ChronoBase Backup Scheduler
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/chronobase --headless
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```
Enable and start: `systemctl --user enable --now chronobase`
</details>

---

## Connection URL Formats

```text
# PostgreSQL
postgresql://username:password@host:5432/database_name

# MongoDB (local / self-hosted)
mongodb://username:password@host:27017/database_name

# MongoDB Atlas
mongodb+srv://username:password@cluster0.xxxxx.mongodb.net/database_name
```

---

## Where Data Lives

1. **Portable mode:** If `chronobase-data.json` exists next to the executable or in the current working directory, ChronoBase stores `chronobase-data.json` and `backups/` in that directory.
2. **Default OS data dir:** Otherwise, ChronoBase uses your OS user data directory (`%APPDATA%\chronobase\ChronoBase\data` on Windows, `~/Library/Application Support/com.chronobase.ChronoBase` on macOS, `~/.local/share/chronobase` on Linux).
3. **Override:** Pass `--data-dir <path>` or set `CHRONOBASE_HOME=<path>`.

Existing `chronobase-data.json` files from v1 work out of the box; stale absolute paths from another machine are automatically re-based onto the local `backups/` directory.

---

## CLI Reference

```text
chronobase [OPTIONS] [COMMAND]

COMMANDS:
  backup --all             Run backups for all connections (exit 0 if all succeed, 1 on any failure)
  backup --group <name>    Run backups for a specific group
  list                     List connections (masked URLs), schedules, and recent backups

OPTIONS:
  --headless               Run background scheduler without GUI
  --data-dir <path>        Override data directory (or set CHRONOBASE_HOME)
  -h, --help               Print help
  -V, --version            Print version
```

---

## Roadmap

- [ ] At-rest encryption for stored connection URLs
- [ ] Offsite upload (S3, Cloudflare R2, Backblaze B2)
- [ ] Optional gzip compression for SQL dumps
- [ ] Failure notifications (Slack, Discord, email)
- [ ] MySQL / MariaDB support

---

## Attribution & License

- Built with **[Slint](https://slint.dev)** (Royalty-Free Desktop License v2.0 / GNU GPLv3).
- Bundled UI fonts: **Geist** and **Geist Mono** (SIL Open Font License 1.1).
- ChronoBase is licensed under the [MIT License](./LICENSE).
