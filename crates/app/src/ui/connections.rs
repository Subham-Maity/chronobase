//! Unified Connections + VPS Peers view-model mapping.

use std::collections::HashMap;
use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::bridge::{PullTargetMode, VpsPeer};
use chronobase_core::connection::Connection;

use crate::ui::{ConnectionRow, MainWindow};

/// Pushes unified direct connections + VPS peers sorted by group and name to the UI thread.
pub fn push_connections(
    weak: &Weak<MainWindow>,
    connections: Vec<Connection>,
    peers: Vec<VpsPeer>,
    all_connections: &[Connection],
    total_count: usize,
) {
    let conn_names: HashMap<u64, String> = all_connections
        .iter()
        .map(|c| (c.id, c.name.clone()))
        .collect();

    let mut combined: Vec<(String, String, ConnectionRow)> =
        Vec::with_capacity(connections.len() + peers.len());

    for c in connections {
        let sort_group = c.group_name.to_lowercase();
        let sort_name = c.name.to_lowercase();
        combined.push((
            sort_group,
            sort_name,
            ConnectionRow {
                id: c.id as i32,
                origin: "direct".into(),
                name: c.name.into(),
                group_name: c.group_name.into(),
                db_type: c.db_type.as_str().into(),
                masked_url: c.url.masked().into(),
                endpoint: "".into(),
                target_label: "Local backups/".into(),
            },
        ));
    }

    for p in peers {
        let sort_group = p.group_name.to_lowercase();
        let sort_name = p.name.to_lowercase();
        let target_label = match p.target_mode {
            PullTargetMode::StorageOnly => "Local storage".to_string(),
            PullTargetMode::LocalConnection(cid) => conn_names
                .get(&cid)
                .map(|n| format!("-> {n}"))
                .unwrap_or_else(|| format!("-> DB #{cid}")),
        };
        combined.push((
            sort_group,
            sort_name,
            ConnectionRow {
                id: p.id as i32,
                origin: "vps".into(),
                name: p.name.into(),
                group_name: p.group_name.into(),
                db_type: p.db_type.as_str().into(),
                masked_url: format!("{} ({})", p.endpoint, p.remote_url.masked()).into(),
                endpoint: p.endpoint.into(),
                target_label: target_label.into(),
            },
        ));
    }

    combined.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let rows: Vec<ConnectionRow> = combined.into_iter().map(|(_, _, r)| r).collect();

    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        win.set_conn_count(total_count as i32);
        win.set_connection_rows(Rc::new(VecModel::from(rows)).into());
    });
}
