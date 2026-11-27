//! VPS Pull Bridge and DB-to-DB migration view-model mapping.

use std::collections::HashMap;
use std::rc::Rc;

use slint::{VecModel, Weak};

use chronobase_core::bridge::{PullTargetMode, VpsPeer, key_fingerprint};
use chronobase_core::connection::Connection;

use crate::ui::{MainWindow, VpsPeerRow, fmt_relative};

/// Formats the target destination label shown for a VPS peer row or modal selector.
pub fn format_target_label(mode: PullTargetMode, conn_names: &HashMap<u64, String>) -> String {
    match mode {
        PullTargetMode::StorageOnly => "Local backups/ storage".to_string(),
        PullTargetMode::LocalConnection(id) => conn_names
            .get(&id)
            .map(|name| format!("Local DB: {name}"))
            .unwrap_or_else(|| format!("Local DB (#{id})")),
    }
}

/// Pushes VPS peer rows to the Slint UI thread.
pub fn push_vps_peers(weak: &Weak<MainWindow>, peers: Vec<VpsPeer>, connections: &[Connection]) {
    let conn_names: HashMap<u64, String> =
        connections.iter().map(|c| (c.id, c.name.clone())).collect();
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak.upgrade() else {
            return;
        };
        let rows: Vec<VpsPeerRow> = peers
            .into_iter()
            .map(|p| {
                let fp = key_fingerprint(&p.pairing_key);
                let target_label = format_target_label(p.target_mode, &conn_names);
                let last_pull = p
                    .last_pull
                    .as_deref()
                    .map(fmt_relative)
                    .unwrap_or_else(|| "never".to_string());
                VpsPeerRow {
                    id: p.id as i32,
                    name: p.name.into(),
                    group_name: p.group_name.into(),
                    endpoint: p.endpoint.into(),
                    db_type: p.db_type.as_str().into(),
                    fingerprint: fp.into(),
                    target_label: target_label.into(),
                    last_pull: last_pull.into(),
                }
            })
            .collect();
        win.set_vps_rows(Rc::new(VecModel::from(rows)).into());
    });
}
