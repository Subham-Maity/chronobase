<div align="center">

# ChronoBase
### Local-first native database backup manager for PostgreSQL and MongoDB, built in pure Rust + Slint.

<img src="assets/hero-architecture.svg" alt="ChronoBase Architecture" width="880" />

<br/>

**ChronoBase** is the pure-Rust native desktop successor to [**ChronoBase v1 (Original Node.js Version)**](https://github.com/Subham-Maity/chronobase/tree/d695de05d8e5966a39b9e0b37e612e182ab9eb48) *(the local backup tool you'll wish you had before that incident)*.<br/>
No Node.js runtime. No Express server. No webview. Just **Rust + Slint + Tokio** in a single native desktop app, headless scheduler, and encrypted VPS pull agent.

[![Release](https://img.shields.io/github/v/release/Subham-Maity/chronobase?label=release&color=6366f1)](https://github.com/Subham-Maity/chronobase/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/Subham-Maity/chronobase/ci.yml?label=build)](https://github.com/Subham-Maity/chronobase/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Built with Rust](https://img.shields.io/badge/core-Rust%20%2B%20Slint-orange?logo=rust)
![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-informational)
![100% Local](https://img.shields.io/badge/privacy-100%25%20local-10b981)

[**Download**](https://github.com/Subham-Maity/chronobase/releases) ·
[**v1 to Rust Rewrite**](#from-chronobase-v1-node-to-pure-rust) ·
[**Features & Security**](#features-and-security-engine) ·
[**VPS Pull & Migration**](#vps-pull-bridge-and-db-to-db-migration) ·
[**Installation**](#installation) ·
[**Headless, Agent & CLI**](#headless-daemon-and-cli-reference)

<br/>

**Created by [<img src="assets/icons/star.svg" width="15" height="15" align="absmiddle" alt="" /> Subham-Maity](https://github.com/Subham-Maity) · [ChronoBase v1 (d695de05)](https://github.com/Subham-Maity/chronobase/tree/d695de05d8e5966a39b9e0b37e612e182ab9eb48) · [Instagram](https://www.instagram.com/subham_xam/)**

</div>

---

## From ChronoBase v1 (Node) to Pure Rust

The original [ChronoBase v1 (`d695de05d8e5966a39b9e0b37e612e182ab9eb48`)](https://github.com/Subham-Maity/chronobase/tree/d695de05d8e5966a39b9e0b37e612e182ab9eb48) proved you can manage PostgreSQL and MongoDB backups locally without cloud lock-in using Node.js, Express, `node-cron`, and a vanilla-JS browser UI.

**ChronoBase** replaces the entire Node/browser stack with a single native binary that reads and migrates your existing `v1` `chronobase-data.json` seamlessly:

| Layer | ChronoBase v1 (Node.js) | ChronoBase (Pure Rust + Slint) |
|---|---|---|
| **Runtime & UI** | Node.js + Express + `http://localhost:3420` browser tab | **Single native Slint desktop app** (`femtovg` GPU + `software` CPU fallback) |
| **Concurrency** | Event-loop callbacks + per-schedule `node-cron` timers | **Dedicated `current_thread` Tokio runtime** + single sleep-until-next `ScheduleRunner` |
| **Command Execution** | `child_process.exec("pg_dump \"url\"")` shell strings | **Zero shell invocation** (`tokio::process::Command` argv arrays + `kill_on_drop` + timeouts) |
| **Credential Safety** | Raw database passwords passed on CLI `argv` | **`PGPASSWORD` env** for Postgres + **`0600` temp config** for MongoDB + `Secret` redaction |
| **Restore Safety** | Plain `pg_dump` / `mongorestore` without clean overwrite | **`--clean --if-exists --no-owner`**, **`psql -v ON_ERROR_STOP=1 --single-transaction`**, **`mongorestore --drop`** |
| **Persistence** | Direct `fs.writeFileSync`; corrupt JSON silently wiped | **Atomic temp + `sync_all` + rename**, `fs4` advisory lock, `.corrupt-<ts>` preservation |
| **Portability** | Stale absolute `file_path` entries broke across machines | **Relative paths + automatic re-basing** of legacy `v1` absolute paths on load |
| **Automation** | Required keeping the Express web server running | **Built-in `--headless` daemon** + `backup` / `list` CLI subcommands |

---

## <img src="assets/icons/sparkles.svg" width="22" height="22" align="absmiddle" alt="" /> At a Glance

| | |
|---|---|
| <img src="assets/icons/zap.svg" width="16" height="16" align="absmiddle" alt="" /> **Zero Cloud, Zero Telemetry** | All connection URLs, `.sql` dumps, MongoDB BSON directories, VPS peer keys, and schedules stay 100% under your control. |
| <img src="assets/icons/layers.svg" width="16" height="16" align="absmiddle" alt="" /> **Strata Stone-Glass Native UI** | Built with **Slint 1.19** (`Geist` & `Geist Mono`), expandable **Dashboard Breakdown**, virtualized `Group · Date` history with **Folder Reveal**, and per-tab **`(i)` Setup Guides**. |
| <img src="assets/icons/shield.svg" width="16" height="16" align="absmiddle" alt="" /> **Hardened Against Leaks & Traversal** | Passwords never enter `argv` or logs; snapshot deletion & reveal check canonicalized paths; VPS transfers use mutual HMAC-SHA256 + `ChaCha20-Poly1305` + SHA-256 checksums. |
| <img src="assets/icons/database.svg" width="16" height="16" align="absmiddle" alt="" /> **Unified Connections, VPS Pull & Migration** | Group Direct DBs & VPS Peers together so cron runs both; migrate **`VPS -> Local DB`** or **`DB -> DB`** in one click with engine & PostgreSQL major-version preflight checks. |
| <img src="assets/icons/clock.svg" width="16" height="16" align="absmiddle" alt="" /> **Background Tray, Retention & Cron** | Toggle **Run in background** & **Run on OS startup** (`--tray`) from the sidebar, schedule 5-field cron jobs, and enable **Automatic Retention** (always keeping your newest good backup; deletions are strictly confined to `backups/` via canonicalized paths). |
| <img src="assets/icons/database.svg" width="16" height="16" align="absmiddle" alt="" /> **Drop-In `v1` Migration** | Existing [ChronoBase v1 (`d695de05`)](https://github.com/Subham-Maity/chronobase/tree/d695de05d8e5966a39b9e0b37e612e182ab9eb48) `chronobase-data.json` files load directly and re-base stale paths. |
| <img src="assets/icons/terminal.svg" width="16" height="16" align="absmiddle" alt="" /> **GUI, Headless, Agent & CLI** | Run the desktop app interactively, `--headless` scheduler daemon, `--agent` VPS listener, or scriptable `backup`, `list`, `pull` commands. |

---

## <img src="assets/icons/layers.svg" width="22" height="22" align="absmiddle" alt="" /> Architecture

```mermaid
flowchart LR
    subgraph App["crates/app (chronobase binary)"]
        UI["Slint 1.19 Strata UI<br/>(Dashboard, Connections, Backups, VPS & Migrate, Schedules, Setup, Danger + Info Guides)"]
        CLI["CLI, --headless & --agent<br/>(backup, list, agent-init, pull)"]
        RT["BackgroundRuntime<br/>(1 dedicated tokio current_thread)"]
    end

    subgraph Core["crates/core (chronobase-core, UI-free)"]
        CONN["connection::Service"]
        BK["backup::Service + CommandRunner"]
        BRIDGE["bridge::Service + Client + Agent<br/>(HMAC-SHA256 + ChaCha20-Poly1305 + SHA-256)"]
        SCHED["schedule::Service + ScheduleRunner"]
        SYS["system::Stats + DangerService"]
        TOOLS["tools::ToolDiscovery (cached)"]
        STORE["store::JsonStore (fs4 lock + atomic write)"]
    end

    subgraph External["Database CLI Tools"]
        PG["pg_dump / psql<br/>(PGPASSWORD env)"]
        MG["mongodump / mongorestore / mongosh<br/>(0600 temp --config / --file)"]
    end

    UI <-->|mpsc + invoke_from_event_loop| RT
    CLI --> Core
    RT --> Core
    CONN --> STORE
    BK --> STORE
    BK --> TOOLS
    BRIDGE --> BK
    BRIDGE --> STORE
    SCHED --> BK
    SYS --> STORE
    BK --> PG
    BK --> MG
```

### Workspace Crates

| Crate | Purpose |
|---|---|
| [`crates/core`](crates/core/Cargo.toml) | Pure, UI-free domain crate (`chronobase-core`): typed `thiserror` errors, `Secret` redaction & URL masking, safe slugification, atomic `JsonStore` with `fs4` lock & `v1` path re-basing, cached OS tool discovery, `CommandRunner` trait (mocked via `mockall` in unit tests), backup/restore/wipe engine, `bridge` (VPS Pull Agent/Client, mutual HMAC + `ChaCha20-Poly1305` + SHA-256 verification + one-click DB-to-DB migration), single-task `ScheduleRunner`, and Danger Zone verification. |
| [`crates/app`](crates/app/Cargo.toml) | Native composition root (`chronobase`): hand-parsed CLI, `--headless` scheduler & `--agent` VPS server signal handlers, `AppState` wiring, dedicated background Tokio thread, Slint `MainWindow` callbacks & per-tab `(i)` setup guides, bundled `Geist` / `Geist Mono` fonts, and Windows `.ico` embedding. |

---

## <img src="assets/icons/shield.svg" width="22" height="22" align="absmiddle" alt="" /> Features and Security Engine

### Backup, Restore & Wipe Behavior
- **PostgreSQL (`pg_dump` & `psql`):**
  - Splits credentials using `url::Url` — passes password via `PGPASSWORD` env var and password-free URI in `argv`.
  - Dumps with `--clean --if-exists --no-owner -f <GroupSlug>/<NameSlug>_YYYYMMDD_HHMMSS.sql`.
  - Restores with `psql -v ON_ERROR_STOP=1 --single-transaction -f <file>` to either the original database or a different target URL.
  - Wipes `public` schema safely on confirmation (`60s` timeout).
- **MongoDB (`mongodump`, `mongorestore`, `mongosh`):**
  - Writes a temporary `0600` YAML config (`--config=<tmp.yaml>`) for `mongodump` / `mongorestore` (falling back to `--uri` only if the installed binary rejects `--config`) and a `0600` temporary JS script (`--file`) for `mongosh` wipe, deleted immediately via RAII `Drop` guard.
  - Same-database restores pass `--drop` so collections cleanly match the snapshot.
  - Supports `mongodb://` and MongoDB Atlas `mongodb+srv://` URIs natively.
- **Diagnostics & History Cap:**
  - Subprocess stderr is capped at 8 KB; timeouts are enforced (`600s` backup/restore, `60s` wipe, `5s` probe).
  - Failed backups record `error_msg` in `chronobase-data.json` and surface it in a dedicated detail strip on the **Backups** page.
  - Failed history entries are pruned first past 500 entries; successful snapshots are never silently evicted (only removed manually or by retention).

### Performance Profile (Measured on Windows 11 x64 Release — v0.1.0 baseline)

| Renderer Feature | Release Binary Size | Window Visible (Startup) | Idle RSS | Verification |
|---|---|---|---|---|
| `default` (`renderer-femtovg`) | **10.75 MB** (`11,270,656` B) | **139 ms** | **144.97 MB** *(includes OpenGL driver working set)* | Measured locally |
| `software` (`--no-default-features --features software`) | **10.71 MB** (`11,230,720` B) | **438 ms** | **42.16 MB** | Measured locally |

---

## <img src="assets/icons/shield.svg" width="22" height="22" align="absmiddle" alt="" /> VPS Pull Bridge and DB-to-DB Migration

Never expose PostgreSQL (`5432`) or MongoDB (`27017`) ports to the public internet just to pull production backups. ChronoBase lets you run a zero-trust, encrypted pull agent on any VPS (`chronobase --agent`) and pull snapshots in one click from your local desktop — either straight into local `backups/` storage **or** automatically restored into a local development/staging database — then **deletes the temporary dump from the VPS** once verified.

### 1. Zero-Trust VPS Pull Security Architecture

1. **Cryptographically Random 256-Bit Pairing Key:** Generated via `getrandom` inside `chronobase agent-init` and stored on the VPS in `chronobase-agent.json` with strict **`0600` POSIX permissions**.
2. **Domain-Separated Key Derivation:** Derives independent 32-byte `auth_key` (HMAC) and `enc_key` (`ChaCha20-Poly1305`) subkeys via domain-tagged SHA-256 (`chronobase-vps-bridge-v1|...`).
3. **Mutual HMAC-SHA256 Challenge-Response (`subtle::ConstantTimeEq`):**
   - Server sends `ServerHello(server_nonce)`.
   - Client replies with `ClientAuth(client_nonce, timestamp, HMAC(auth_key, server_nonce || client_nonce || ts))`.
   - Server validates timestamp skew (`+/-60s`), rejects replayed nonces (`ReplayGuard`), verifies HMAC in constant time, and proves identity back with `ServerAuth(HMAC(auth_key, client_nonce || server_nonce))`.
4. **Brute-Force Lockout & IP Allowlist:** `AuthRateLimiter` tracks failed handshakes per client IP and enforces a lockout after 5 failures; optional `--allow-ip <ip>` rejects non-allowlisted peers immediately on accept.
5. **Authenticated Encryption (`ChaCha20-Poly1305`):** Every subsequent frame (including your encrypted remote DB URL, chunked `.sql` / BSON files, and deletion acknowledgment) uses unique directional counter nonces (`DIR_C2S` / `DIR_S2C`) preventing cross-direction or replay attacks.
6. **SHA-256 Integrity & Mandatory VPS Cleanup:**
   - The VPS dumps into an isolated temporary `.cb_vps_stage_*` directory, hashes all payload bytes (`SHA-256`), and streams chunks.
   - Only after the client verifies the exact `SHA-256` digest does it send `AckDelete`; the VPS agent wipes `.cb_vps_stage_*` immediately AND via an **RAII `Drop` guard** even on disconnects or timeouts.
   - Client validates every streamed path through `safe_rel_path` (blocking `..` traversal or absolute paths).

### 2. Quick Setup (Local <-> VPS Pull)

**Step A — On your VPS:**
```bash
# 1. Generate a 256-bit pairing key and 0600 config (copy the printed key)
chronobase agent-init --bind 0.0.0.0:9443

# 2. Start the agent (recommended: add --allow-ip or bind 127.0.0.1 behind SSH/Tailscale)
chronobase --agent --bind 0.0.0.0:9443 --allow-ip <YOUR_HOME_OR_TAILSCALE_IP>
```

**Step B — On your desktop app:**
1. Open **VPS & Migrate** (click the **`(i)`** button on any tab for quick in-app setup steps).
2. Click **Add VPS peer**, fill in a name, endpoint (`host:9443`), engine (`Postgres` or `MongoDB`), the VPS-local DB URL (`postgresql://user:pass@127.0.0.1:5432/app`), and the pairing key (verify the SHA-256 fingerprint matches!).
3. Choose **Pull destination**: `Local backups/ storage ONLY` **or** any local ChronoBase database connection.
4. Click **Pull** anytime (or script `chronobase pull <vps-name>`): ChronoBase authenticates, downloads encrypted, verifies SHA-256, deletes the staging file on the VPS, records the snapshot locally, and optionally pushes it straight into your chosen local DB.

### 3. One-Click Migration (`VPS -> Local DB` & `DB -> DB`)

Click **Migrate DB -> DB** under **VPS & Migrate** to pick any Source (**VPS Peer** or **Direct Connection**) and any compatible Destination **Direct Connection**. ChronoBase verifies engine compatibility and PostgreSQL major-version parity before restoring cleanly (`--clean --if-exists --no-owner` / `mongorestore --drop`), saving the snapshot under `backups/<Group>/<YYYY-MM-DD>/`.

### 4. Background System Tray & OS Startup

Use the **Run in background** card at the bottom of the left sidebar to minimize ChronoBase to the native OS system tray on window close while keeping `ScheduleRunner` cron jobs active across both direct connections and grouped VPS peers. Enable **Run on OS startup** to launch automatically with `--tray` on login (Windows `HKCU\...\Run`, macOS `LaunchAgents`, Linux XDG autostart).

---

## <img src="assets/icons/package.svg" width="22" height="22" align="absmiddle" alt="" /> Installation

Download prebuilt installers and portable archives from the [Releases page](https://github.com/Subham-Maity/chronobase/releases). Every release ships `SHA256SUMS.txt`.

### Supported Platforms

| Platform | Target | Release Installers & Archives | Status & Notes |
|---|---|---|---|
| **Windows 10 / 11 (x64)** | `x86_64-pc-windows-msvc` | `ChronoBase-windows-x64-setup.exe` (NSIS), `ChronoBase-windows-x64.msi` (MSI), `ChronoBase-windows-x64-portable.zip` | Built & verified in CI; Start Menu + Desktop shortcuts in `.exe` / `.msi` |
| **Linux (x86_64, X11 & Wayland)** | `x86_64-unknown-linux-gnu` | `ChronoBase-linux-x86_64.tar.gz`, `ChronoBase-linux-x86_64.deb` | Tested in CI; `software` feature runs without GPU |
| **macOS (Apple Silicon)** | `aarch64-apple-darwin` | `ChronoBase-macos-arm64.dmg`, `ChronoBase-macos-arm64.app.tar.gz` | Native `arm64` build tested in CI |
| **macOS (Intel)** | `x86_64-apple-darwin` | `ChronoBase-macos-x64.dmg`, `ChronoBase-macos-x64.app.tar.gz` | Cross-compiled in release CI (build-only, not tested on Intel hardware) |

### Prerequisites (Database CLI Tools)

ChronoBase invokes the official PostgreSQL and MongoDB CLI utilities directly (not bundled). GUI apps on macOS and Linux do not inherit your shell `PATH`, so ChronoBase also probes standard installation directories automatically.

<details>
<summary><strong>PostgreSQL (<code>pg_dump</code> &amp; <code>psql</code>)</strong></summary>

- **Windows:** Install from [postgresql.org/download/windows](https://www.postgresql.org/download/windows) (Command Line Tools). ChronoBase scans `PATH` and `C:\Program Files\PostgreSQL\*\bin`.
- **macOS:** `brew install libpq && brew link --force libpq` (also probes Homebrew's keg-only `/opt/homebrew/opt/libpq/bin` and `/usr/local/opt/libpq/bin`).
- **Linux (Ubuntu / Debian):** `sudo apt update && sudo apt install -y postgresql-client` (also probes `/usr/lib/postgresql/*/bin`).

</details>

<details>
<summary><strong>MongoDB (<code>mongodump</code>, <code>mongorestore</code>, <code>mongosh</code>)</strong></summary>

- **Windows:** Install Database Tools MSI from [mongodb.com/try/download/database-tools](https://www.mongodb.com/try/download/database-tools). ChronoBase scans `PATH` and `C:\Program Files\MongoDB\Tools\*\bin`.
- **macOS:** `brew tap mongodb/brew && brew install mongodb-database-tools`
- **Linux (Ubuntu / Debian):** `sudo apt install -y mongo-tools` (or extract the official Database Tools tarball into `PATH`).

</details>

---

## <img src="assets/icons/terminal.svg" width="22" height="22" align="absmiddle" alt="" /> Headless Daemon and CLI Reference

### CLI Commands

```text
chronobase [OPTIONS] [COMMAND]

COMMANDS:
  backup --all             Run backups for all connections (exit 0 if all succeed, 1 on any failure)
  backup --group <name>    Run backups for a specific group
  cleanup [--dry-run]      Run backup retention cleanup (or preview without deleting)
  list                     List connections (masked URLs), schedules, and recent backups
  agent-init               Generate 256-bit VPS pairing key and chronobase-agent.json (0600)
  pull <vps-name-or-id>    Pull a verified encrypted snapshot from a configured VPS peer

OPTIONS:
  --headless               Run background cron scheduler without opening a GUI window
  --agent                  Run the encrypted VPS Pull Agent TCP server
  --bind <host:port>       Override VPS agent listen address (default: 127.0.0.1:9443)
  --allow-ip <ip>          Allowlist client IP for --agent (repeatable)
  --data-dir <path>        Override data directory (or set CHRONOBASE_HOME)
  -h, --help               Print help message
  -V, --version            Print version
```

### Auto-Start on Boot (`--headless`)

<details>
<summary><strong>Windows — Startup folder or Task Scheduler</strong></summary>

1. Press `Win + R` → type `shell:startup` → Enter.
2. Create a shortcut to `chronobase.exe` and append `--headless` to **Target** (e.g. `"C:\Tools\ChronoBase\chronobase.exe" --headless`).

</details>

<details>
<summary><strong>macOS — <code>launchd</code></strong></summary>

Create `~/Library/LaunchAgents/com.chronobase.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
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
<summary><strong>Linux — <code>systemd</code> user service</strong></summary>

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

Enable and start: `systemctl --user enable --now chronobase`.

</details>

---

## <img src="assets/icons/folder.svg" width="22" height="22" align="absmiddle" alt="" /> Data Location & Project Structure

### Where Data Lives
1. **Override:** `--data-dir <path>` or `CHRONOBASE_HOME=<path>`.
2. **Portable mode:** If `chronobase-data.json` exists next to the executable or in the current working directory, ChronoBase uses that folder.
3. **Default OS user data dir:** `%APPDATA%\chronobase\ChronoBase\data` (Windows), `~/Library/Application Support/com.chronobase.ChronoBase` (macOS), or `~/.local/share/chronobase` (Linux).

```text
<data_dir>/
├── chronobase-data.json              ← connections, backup history (500-entry failed-first cap), schedules, vps_peers, settings (tray/startup/retention), _seq
├── chronobase-agent.json             ← 0600 VPS pairing config (generated via `chronobase agent-init`)
├── chronobase.lock                   ← advisory fs4 lock preventing concurrent store corruption
└── backups/
    └── <GroupSlug>/
        └── <YYYY-MM-DD>/
            ├── MyApp_20261127_020001.sql ← PostgreSQL dump (--clean --if-exists --no-owner)
            └── Analytics_20261127_020015/← MongoDB BSON dump folder
```

### Repository Structure

```text
.
├── assets/                     README hero SVG diagram & icons
├── crates/
│   ├── core/                   chronobase-core (store, connection, backup, bridge, schedule, tools, system)
│   │   └── tests/              end-to-end backup & VPS bridge integration tests + opt-in real DB tests
│   └── app/                    chronobase binary (CLI/headless/agent + Slint UI, Geist fonts, icons, build.rs)
│       └── ui/                 theme.slint, icons.slint, main.slint, components/, pages/
├── scripts/                    Windows (.zip + .exe NSIS + .msi WiX), Linux (.tar.gz + .deb), and macOS (.app + .dmg) scripts
├── .github/workflows/          ci.yml (3-OS + software renderer checks) & release.yml (4-target releases)
├── CHANGELOG.md                Keep a Changelog history
├── CONTRIBUTING.md             contributor guide
├── LICENSE                     MIT License
└── README.md
```

---

## <img src="assets/icons/hammer.svg" width="22" height="22" align="absmiddle" alt="" /> Building from Source

### Prerequisites
- **All platforms:** Rust toolchain (`1.95+`, workspace tested on `1.98.0`).
- **Linux (Debian / Ubuntu) build & runtime libraries:**
  ```bash
  sudo apt-get update && sudo apt-get install -y --no-install-recommends \
    libfontconfig1-dev libxkbcommon-dev libxkbcommon-x11-dev \
    libx11-dev libxcb1-dev libwayland-dev libgl1-mesa-dev
  ```

### Build & Test Commands

```bash
# Run formatting, strict clippy, and workspace tests
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Build release binary (default hardware-accelerated femtovg renderer)
cargo build --release --locked -p chronobase

# Build release binary with CPU software renderer (no GPU required)
cargo build --release --locked -p chronobase --no-default-features --features software
```

---

## <img src="assets/icons/flask.svg" width="22" height="22" align="absmiddle" alt="" /> CI and Automated Releases

- **Continuous Integration ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):** Runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` on every push and PR across **Windows (`windows-latest`)**, **Linux (`ubuntu-22.04`)**, and **macOS (`macos-latest`)**, plus a dedicated Linux job checking the `--no-default-features --features software` build.
- **Automated Releases ([`.github/workflows/release.yml`](.github/workflows/release.yml)):** Triggered by pushing a `v*` tag (`git tag v0.1.0 && git push origin v0.1.0`) or manually via `workflow_dispatch`. Builds all 4 targets via [`scripts/`](scripts/package-windows.ps1), generates `SHA256SUMS.txt`, extracts release notes from [`CHANGELOG.md`](CHANGELOG.md), and publishes the GitHub Release.

---

## <img src="assets/icons/stethoscope.svg" width="22" height="22" align="absmiddle" alt="" /> Troubleshooting

- **Windows SmartScreen Warning:** Release binaries are unsigned. Click **More info → Run anyway** on first launch.
- **macOS Gatekeeper Block:** Right-click → **Open**, use **System Settings → Privacy & Security → Open Anyway**, or run `xattr -cr /Applications/ChronoBase.app`.
- **Linux Missing GPU / OpenGL:** Build with `--no-default-features --features software` to run entirely on the CPU software rasterizer; both X11 and Wayland are supported via Slint's `winit` backend.
- **Connection URL Formats:**
  - PostgreSQL: `postgresql://username:password@host:5432/database_name`
  - MongoDB: `mongodb://username:password@host:27017/database_name` or `mongodb+srv://username:password@cluster0.xxxxx.mongodb.net/database_name`

---

## <img src="assets/icons/handshake.svg" width="22" height="22" align="absmiddle" alt="" /> Author & Contributing

Built and maintained by **Subham Maity**:
- **GitHub:** [github.com/Subham-Maity](https://github.com/Subham-Maity)
- **Instagram:** [@subham_xam](https://www.instagram.com/subham_xam/)
- **Original v1:** [ChronoBase v1 (Original Node.js Version — `d695de05d8e5966a39b9e0b37e612e182ab9eb48`)](https://github.com/Subham-Maity/chronobase/tree/d695de05d8e5966a39b9e0b37e612e182ab9eb48)

Contributions, bug reports, and PRs are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).

---

## <img src="assets/icons/file-text.svg" width="22" height="22" align="absmiddle" alt="" /> License & Attribution

- ChronoBase is released under the [MIT License](LICENSE).
- Built with **[Slint](https://slint.dev)** (triple-licensed under **Slint Royalty-Free Desktop, Mobile, and Web Applications License v2.0**, **GNU GPLv3**, and **Commercial License**).
- Bundled UI fonts: **Geist** and **Geist Mono** ([SIL Open Font License 1.1](crates/app/ui/fonts/OFL.txt)).
