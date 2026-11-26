//! Core domain logic, persistence, backup/restore engine, and scheduler for ChronoBase.
//!
//! This crate is completely UI-free so it can be shared between the native Slint
//! desktop application, the headless scheduler daemon, and the CLI.

pub mod backup;
pub mod connection;
pub mod constant;
pub mod error;
pub mod platform;
pub mod schedule;
pub mod secret;
pub mod slug;
pub mod store;
pub mod system;
pub mod tools;
