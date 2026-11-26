# Changelog

All notable changes to ChronoBase will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-27

### Added
- Native Rust desktop UI built with **Slint** (`renderer-femtovg` default + `software` CPU fallback).
- Full feature parity with ChronoBase v1: Dashboard, Connections (grouped, masked URLs, Wipe DB), Backups (virtualized `ListView`, filters, Restore Same/Different DB, 500-entry cap), Schedules (5-field cron presets, enable/pause, `last_run`), Setup (auto-detection + OS install steps), and Danger Zone (`DELETE BACKUPS` / `DELETE EVERYTHING`).
- Headless scheduler daemon (`chronobase --headless`) and CLI subcommands (`chronobase list`, `chronobase backup --all | --group <name>`).
- 100% compatibility with v1 `chronobase-data.json` files, including automatic re-basing of stale absolute backup paths.
- Security hardening: zero shell invocation, `PGPASSWORD` env var for Postgres, `0600` temporary config/script files for MongoDB, redacted `Secret` newtype, canonicalized path-containment check before deletion, `fs4` advisory file lock, atomic writes with `sync_all` + rename, and `.corrupt-<timestamp>` recovery.
- Failed backup `error_msg` recording and inline UI inspection.

