//! Danger Zone UI state helpers.

use slint::Weak;

use crate::ui::MainWindow;

/// Closes Danger Zone modals after a destructive action completes.
pub fn close_danger_modals(weak: &Weak<MainWindow>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        win.set_delete_backups_modal_open(false);
        win.set_delete_backups_input("".into());
        win.set_delete_all_modal_open(false);
        win.set_delete_all_input("".into());
    });
}
