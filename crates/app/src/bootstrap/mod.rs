//! Application composition root: state wiring and background tokio runtime.

pub mod runtime;
pub mod state;

pub use runtime::{AppCommand, BackgroundRuntime};
pub use state::AppState;
