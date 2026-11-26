//! Setup tool status view-model mapping.

use slint::Weak;

use chronobase_core::tools::ToolStatus;

use crate::ui::MainWindow;

/// Pushes discovered CLI tool status to both sidebar pills and the Setup page.
pub fn push_tools(weak: &Weak<MainWindow>, status: ToolStatus) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        let pg_ready = status.has_pg();
        let mongo_ready = status.has_mongo();

        win.set_pg_ready(pg_ready);
        win.set_pg_path_text(
            status
                .pg_dump
                .as_ref()
                .map(|p| format!("Found: {}", p.display()))
                .unwrap_or_else(|| "Not found — follow the steps below".to_string())
                .into(),
        );

        win.set_mongo_ready(mongo_ready);
        win.set_mongo_path_text(
            status
                .mongodump
                .as_ref()
                .map(|p| format!("Found: {}", p.display()))
                .unwrap_or_else(|| "Not found — follow the steps below".to_string())
                .into(),
        );
        win.set_recheck_busy(false);
    });
}
