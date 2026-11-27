# Changelog

All notable changes to ChronoBase will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Backup Retention & Auto-Cleanup (`chronobase cleanup [--dry-run]`):** Added pure `plan_retention` planner, `RetentionService::{preview, run}`, schema v4 (`AppSettings.retention`, `CleanupFrequency`, and per-connection `Connection.retention` overrides, disabled by default on v1/v3 upgrades), serialized locking against restore/migration, triggers (post-backup, startup, and `ScheduleRunner` cadence), and Slint **Schedules -> Automatic Retention** + Connection modal override controls. Retention deletes exclusively through `BackupService::delete_backup` (canonicalized `backups/` path containment), always protects the newest good snapshot per `(connection_id, source_kind)`, and never deletes snapshots under 1 hour old.
- **Scheduled Failure Notifications:** `ScheduleRunner::execute_schedule` now emits UI-free `ScheduleFailureEvent` broadcasts surfaced as GUI error toasts while preserving structured `tracing::error!` logs in `--headless`.

### Fixed
- **Backup History Cap Orphan Prevention:** `BackupRepository::record()` now evicts the oldest `Failed` entries first when exceeding 500 entries and never silently drops `Success` entries (preventing orphaned backup files/directories on disk).

## [0.3.0] - 2026-11-27

### Added
- **Unified Connections & VPS Groups (`group_name`):** Assign a `group_name` to every remote VPS peer, view and manage both Direct Connections and VPS Peers together under **Connections**, and run `ScheduleRunner` cron jobs, GUI `Back up all` / `Back up group`, and `chronobase backup` across both direct databases and VPS peers in the same group.
- **Version-Safe One-Click Migration (`VPS -> Local DB` & `DB -> DB`):** Migrate straight from any saved **VPS Peer** into a local database (or between any two direct connections) without hitting `"Add at least two connections"`, with preflight engine and PostgreSQL major-version checks before restore.
- **Background Tray Engine & OS Login Startup:** Prominent sidebar **Run in background** and **Run on OS startup** (`--tray` / `--minimized`) toggles powered by Slint's native `SystemTrayIcon` (`Shell_NotifyIconW` on Windows, `NSStatusItem` on macOS, pure-Rust `ksni` D-Bus on Linux), Windows `HKCU\...\Run` Registry, macOS `LaunchAgents`, and Linux XDG autostart. Closing the window hides to tray while keeping cron schedules live.
- **Expandable Dashboard Breakdown:** 4 KPI columns (Local + VPS targets, Success/Fail %, Direct vs VPS storage, Next Scheduled Cron Run), collapsible **Group & Storage Breakdown** with one-click per-group backup, collapsible **Tooling & Environment Health** with parsed CLI versions, and click-to-expand **Recent Snapshot Details** drawer with **Open Folder** and **Restore**.
- **Group + Date Backup History & Folder Reveal:** Snapshots are stored under `backups/<GroupSlug>/<YYYY-MM-DD>/`, grouped by **Group & Date** (`Today`, `Yesterday`, `YYYY-MM-DD`) in **Backups**, and include a **Folder** reveal button on every valid row (verified against canonical `backups/` containment before opening Explorer / Finder / `xdg-open`).

### Fixed
- **Long URL & Pairing Key Overflow:** Added `clip: true`, `min-width: 0px`, and elided placeholders to `FormField`, `CopyableCommand`, and backup error strips so long PostgreSQL/MongoDB URLs and 256-bit pairing keys stay cleanly inside input borders.

## [0.2.1] - 2026-11-27

### Fixed
- **Modal Centering & Compact Sizing:** Replaced manual `x`/`y` modal positioning and unbounded height in `ModalOverlay` with native Slint flex centering (`alignment: center`) and content-driven height (`card-content.preferred-height`), eliminating oversized vertical gaps in form modals and the stretched empty box inside `(i)` Guide modals.
- **Per-Tab `(i)` Info Button Alignment:** Constrained `PageInfoButton` to a compact `32x32` circular pill (`horizontal-stretch: 0`) aligned cleanly next to each page heading instead of stretching across the header row.
- **Toast Notification Visibility & Position:** Bound `ToastBanner` height and wrapped `Text` width explicitly (`min-width: 0px`) inside a bottom-right `alignment: end` stack so error, success, and info toast icons and messages render crisply above the bottom edge.

## [0.2.0] - 2026-11-27

### Added
- **Secure VPS Pull Bridge (`chronobase agent-init`, `chronobase --agent`, `chronobase pull`):** Pull dumps directly from remote VPS instances without exposing database ports, secured with mutual HMAC-SHA256 challenge-response, domain-separated SHA-256 subkeys, `ChaCha20-Poly1305` AEAD encryption, anti-replay nonces & 60s clock skew check, per-IP rate limiting, `--allow-ip` filtering, SHA-256 full-stream integrity verification, and guaranteed VPS staging cleanup (immediate + RAII `Drop`).
- **One-Click DB-to-DB Migration:** Migrate directly between any two configured connections (or route incoming VPS pulls straight into a local target database or `backups/` storage only).
- **Per-Tab `(i)` Help Modals:** Interactive info button on every tab (`Dashboard`, `Connections`, `Backups`, `VPS & Migrate`, `Schedules`, `Setup`, `Danger Zone`) detailing purpose and step-by-step setup.
- **Windows Installers:** Automated CI packaging for `ChronoBase-windows-x64-setup.exe` (NSIS) and `ChronoBase-windows-x64.msi` (WiX Toolset v3) alongside `ChronoBase-windows-x64-portable.zip`.

### Fixed
- **Danger Zone Layout:** Prevented action button text clipping on narrower viewports by adding `min-width: 0px` and `horizontal-stretch: 0` layout constraints.

## [0.1.0] - 2026-11-27

### Added
- Native Rust desktop UI built with **Slint** (`renderer-femtovg` default + `software` CPU fallback).
- Full feature parity with ChronoBase v1: Dashboard, Connections (grouped, masked URLs, Wipe DB), Backups (virtualized `ListView`, filters, Restore Same/Different DB, 500-entry cap), Schedules (5-field cron presets, enable/pause, `last_run`), Setup (auto-detection + OS install steps), and Danger Zone (`DELETE BACKUPS` / `DELETE EVERYTHING`).
- Headless scheduler daemon (`chronobase --headless`) and CLI subcommands (`chronobase list`, `chronobase backup --all | --group <name>`).
- 100% compatibility with v1 `chronobase-data.json` files, including automatic re-basing of stale absolute backup paths.
- Security hardening: zero shell invocation, `PGPASSWORD` env var for Postgres, `0600` temporary config/script files for MongoDB, redacted `Secret` newtype, canonicalized path-containment check before deletion, `fs4` advisory file lock, atomic writes with `sync_all` + rename, and `.corrupt-<timestamp>` recovery.
- Failed backup `error_msg` recording and inline UI inspection.

