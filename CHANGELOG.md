# Changelog

All notable changes to ChronoBase will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

