//! Pure, IO-free backup retention planner.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone};

use crate::backup::{BackupEntry, BackupSourceKind, BackupStatus};
use crate::retention::model::{
    RetentionCandidate, RetentionPlan, RetentionPolicy, RetentionReason,
};

/// Parses a `YYYY-MM-DD HH:MM:SS` local timestamp string.
fn parse_created_at(raw: &str) -> Option<DateTime<Local>> {
    let naive = NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
}

fn is_within_last_hour(dt: DateTime<Local>, now: DateTime<Local>) -> bool {
    now.signed_duration_since(dt) < Duration::hours(1)
}

/// Computes which backup entries should be deleted under the resolved per-connection policies.
///
/// This function is pure (no filesystem or store IO) and enforces all safety invariants:
/// - Groups snapshots by `(connection_id, source_kind)` so `Direct` and `VpsPull` never mix.
/// - `keep_last` and `max_age_days` both act as limits (an entry exceeding either is deleted).
/// - Never deletes the newest valid `Success` snapshot in each group, regardless of age.
/// - Never deletes any snapshot created within the last hour (`< 1h`).
/// - Keeps any entry whose `created_at` cannot be parsed.
/// - Drops `missing_on_disk` `Success` metadata entries (counting `0` bytes freed).
pub fn plan_retention<F>(
    entries: &[BackupEntry],
    policy_for_conn: F,
    now: DateTime<Local>,
) -> RetentionPlan
where
    F: Fn(u64) -> Option<RetentionPolicy>,
{
    let mut groups: HashMap<(u64, BackupSourceKind), Vec<&BackupEntry>> = HashMap::new();
    for entry in entries {
        groups
            .entry((entry.connection_id, entry.source_kind))
            .or_default()
            .push(entry);
    }

    // Deterministic ordering by group key for predictable plans/tests.
    let mut group_keys: Vec<(u64, BackupSourceKind)> = groups.keys().copied().collect();
    group_keys.sort_by_key(|(cid, kind)| {
        let k = match kind {
            BackupSourceKind::Direct => 0u8,
            BackupSourceKind::VpsPull => 1u8,
        };
        (*cid, k)
    });

    let mut candidates = Vec::new();
    let mut to_delete = Vec::new();
    let mut bytes_freed: u64 = 0;

    for key in group_keys {
        let Some(policy) = policy_for_conn(key.0) else {
            continue;
        };
        if !policy.enabled {
            continue;
        }
        let Some(group_entries) = groups.get(&key) else {
            continue;
        };

        // 1. Missing-on-disk Success entries: drop metadata (unless created < 1h ago).
        for entry in group_entries {
            if entry.status == BackupStatus::Success && entry.missing_on_disk {
                if let Some(dt) = parse_created_at(&entry.created_at)
                    && is_within_last_hour(dt, now)
                {
                    continue;
                }
                candidates.push(RetentionCandidate {
                    entry: (*entry).clone(),
                    reason: RetentionReason::MissingOnDisk,
                });
                to_delete.push((*entry).clone());
            }
        }

        // 2. On-disk Success entries: sort newest-first by parsed created_at, tie-break id desc.
        // Entries with unparseable timestamps are always kept and excluded from deletion.
        let mut valid_success: Vec<(&BackupEntry, DateTime<Local>)> = group_entries
            .iter()
            .copied()
            .filter(|e| e.status == BackupStatus::Success && !e.missing_on_disk)
            .filter_map(|e| parse_created_at(&e.created_at).map(|dt| (e, dt)))
            .collect();
        valid_success.sort_by(|(a, a_dt), (b, b_dt)| b_dt.cmp(a_dt).then_with(|| b.id.cmp(&a.id)));

        for (idx, (entry, dt)) in valid_success.into_iter().enumerate() {
            // Invariant: NEVER delete the newest Success snapshot of each group.
            if idx == 0 {
                continue;
            }
            // Invariant: Never delete a snapshot created within the last hour.
            if is_within_last_hour(dt, now) {
                continue;
            }

            let exceeds_keep = policy.keep_last.is_some_and(|k| idx >= k as usize);
            let exceeds_age = policy.max_age_days.is_some_and(|days| {
                now.signed_duration_since(dt) > Duration::days(i64::from(days))
            });

            let reason = if exceeds_keep {
                Some(RetentionReason::ExceedsKeepLast)
            } else if exceeds_age {
                Some(RetentionReason::ExceedsMaxAge)
            } else {
                None
            };

            if let Some(reason) = reason {
                bytes_freed = bytes_freed.saturating_add(entry.file_size);
                candidates.push(RetentionCandidate {
                    entry: entry.clone(),
                    reason,
                });
                to_delete.push(entry.clone());
            }
        }

        // 3. Failed entries: prune when older than failed_max_age_days.
        if let Some(failed_days) = policy.failed_max_age_days {
            let max_age = Duration::days(i64::from(failed_days));
            for entry in group_entries {
                if entry.status != BackupStatus::Failed {
                    continue;
                }
                let Some(dt) = parse_created_at(&entry.created_at) else {
                    continue;
                };
                if is_within_last_hour(dt, now) {
                    continue;
                }
                if now.signed_duration_since(dt) > max_age {
                    candidates.push(RetentionCandidate {
                        entry: (*entry).clone(),
                        reason: RetentionReason::FailedExpired,
                    });
                    to_delete.push((*entry).clone());
                }
            }
        }
    }

    RetentionPlan {
        to_delete,
        candidates,
        bytes_freed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::DbKind;

    fn mk_entry(
        id: u64,
        conn_id: u64,
        source_kind: BackupSourceKind,
        status: BackupStatus,
        created_at: &str,
        size: u64,
    ) -> BackupEntry {
        BackupEntry {
            id,
            connection_id: conn_id,
            connection_name: format!("DB {conn_id}"),
            group_name: "Prod".into(),
            db_type: DbKind::Postgres,
            source_kind,
            file_path: if status == BackupStatus::Success {
                format!("Prod/2026-11-27/db_{id}.sql")
            } else {
                String::new()
            },
            file_size: size,
            status,
            error_msg: None,
            created_at: created_at.into(),
            missing_on_disk: false,
        }
    }

    fn fixed_now() -> DateTime<Local> {
        parse_created_at("2026-11-27 12:00:00").expect("valid fixed now")
    }

    #[test]
    fn keep_last_only_keeps_newest_n() {
        let now = fixed_now();
        let entries = vec![
            mk_entry(
                1,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-27 10:00:00",
                100,
            ),
            mk_entry(
                2,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-26 10:00:00",
                200,
            ),
            mk_entry(
                3,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-25 10:00:00",
                300,
            ),
        ];
        let policy = RetentionPolicy {
            enabled: true,
            keep_last: Some(2),
            max_age_days: None,
            failed_max_age_days: Some(7),
        };
        let plan = plan_retention(&entries, |_| Some(policy.clone()), now);
        assert_eq!(plan.to_delete.len(), 1);
        assert_eq!(plan.to_delete[0].id, 3);
        assert_eq!(plan.bytes_freed, 300);
    }

    #[test]
    fn max_age_only_deletes_older_and_keeps_newest_even_if_old() {
        let now = fixed_now();
        let entries = vec![
            // Even the newest is 40 days old — MUST be kept!
            mk_entry(
                1,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-10-18 12:00:00",
                100,
            ),
            mk_entry(
                2,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-10-10 12:00:00",
                250,
            ),
        ];
        let policy = RetentionPolicy {
            enabled: true,
            keep_last: None,
            max_age_days: Some(30),
            failed_max_age_days: Some(7),
        };
        let plan = plan_retention(&entries, |_| Some(policy.clone()), now);
        assert_eq!(plan.to_delete.len(), 1);
        assert_eq!(plan.to_delete[0].id, 2);
        assert_eq!(plan.bytes_freed, 250);
    }

    #[test]
    fn both_keep_last_and_max_age_act_as_limits() {
        let now = fixed_now();
        let entries = vec![
            mk_entry(
                1,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-27 10:00:00",
                100,
            ),
            mk_entry(
                2,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-10 10:00:00",
                200,
            ), // > 14 days
            mk_entry(
                3,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-26 10:00:00",
                150,
            ),
            mk_entry(
                4,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-25 10:00:00",
                175,
            ), // rank 2 (beyond keep_last=2)
        ];
        let policy = RetentionPolicy {
            enabled: true,
            keep_last: Some(2),
            max_age_days: Some(14),
            failed_max_age_days: Some(7),
        };
        let plan = plan_retention(&entries, |_| Some(policy.clone()), now);
        let ids: Vec<u64> = plan.to_delete.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![4, 2]);
        assert_eq!(plan.bytes_freed, 375);
    }

    #[test]
    fn groups_by_connection_and_separates_direct_from_vps_pull() {
        let now = fixed_now();
        let entries = vec![
            mk_entry(
                1,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-27 10:00:00",
                100,
            ),
            mk_entry(
                2,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-26 10:00:00",
                100,
            ),
            // Same connection ID = 1, but VpsPull — its newest must be kept independently
            mk_entry(
                3,
                1,
                BackupSourceKind::VpsPull,
                BackupStatus::Success,
                "2026-11-25 10:00:00",
                100,
            ),
            // Different connection ID = 2
            mk_entry(
                4,
                2,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-24 10:00:00",
                100,
            ),
        ];
        let policy = RetentionPolicy {
            enabled: true,
            keep_last: Some(1),
            max_age_days: None,
            failed_max_age_days: Some(7),
        };
        let plan = plan_retention(&entries, |_| Some(policy.clone()), now);
        assert_eq!(plan.to_delete.len(), 1);
        assert_eq!(plan.to_delete[0].id, 2);
    }

    #[test]
    fn bad_timestamp_and_recent_within_one_hour_are_kept() {
        let now = fixed_now();
        let entries = vec![
            mk_entry(
                1,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-27 11:45:00",
                100,
            ),
            // Within last hour (11:30 vs 12:00), even with keep_last = 1
            mk_entry(
                2,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "2026-11-27 11:30:00",
                100,
            ),
            // Unparseable timestamp
            mk_entry(
                3,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Success,
                "not-a-timestamp",
                100,
            ),
        ];
        let policy = RetentionPolicy {
            enabled: true,
            keep_last: Some(1),
            max_age_days: Some(0),
            failed_max_age_days: Some(0),
        };
        let plan = plan_retention(&entries, |_| Some(policy.clone()), now);
        assert!(plan.to_delete.is_empty());
    }

    #[test]
    fn failed_entries_pruned_after_failed_max_age_days() {
        let now = fixed_now();
        let entries = vec![
            mk_entry(
                1,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Failed,
                "2026-11-18 10:00:00",
                0,
            ), // 9 days old -> delete
            mk_entry(
                2,
                1,
                BackupSourceKind::Direct,
                BackupStatus::Failed,
                "2026-11-25 10:00:00",
                0,
            ), // 2 days old -> keep
        ];
        let policy = RetentionPolicy {
            enabled: true,
            keep_last: Some(5),
            max_age_days: None,
            failed_max_age_days: Some(7),
        };
        let plan = plan_retention(&entries, |_| Some(policy.clone()), now);
        assert_eq!(plan.to_delete.len(), 1);
        assert_eq!(plan.to_delete[0].id, 1);
        assert_eq!(plan.bytes_freed, 0);
    }
}
