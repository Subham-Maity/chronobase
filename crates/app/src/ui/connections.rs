//! Connections view-model mapping.

use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::connection::Connection;

use crate::ui::{ConnectionRow, MainWindow};

/// Pushes filtered connection rows and total connection badge count to the UI thread.
pub fn push_connections(weak: &Weak<MainWindow>, connections: Vec<Connection>, total_count: usize) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        win.set_conn_count(total_count as i32);
        let rows: Vec<ConnectionRow> = connections
            .into_iter()
            .map(|c| ConnectionRow {
                id: c.id as i32,
                name: c.name.into(),
                group_name: c.group_name.into(),
                db_type: c.db_type.as_str().into(),
                masked_url: c.url.masked().into(),
            })
            .collect();
        win.set_connection_rows(Rc::new(VecModel::from(rows)).into());
    });
}
