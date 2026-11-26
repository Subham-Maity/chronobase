//! JSON file persistence for ChronoBase.
//!
//! **Deliberate deviation from `RULE.md`:** ChronoBase uses an atomic JSON file
//! store (`chronobase-data.json`) instead of `sqlx`/`sea-orm`. This keeps the
//! binary tiny, avoids native C/SQLite dependencies, and maintains 100% data
//! compatibility with ChronoBase v1.

pub mod json_store;

pub use json_store::{JsonStore, Seq, StoreData, StoreError, local_timestamp};
