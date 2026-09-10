//! Shared grouped-pagination core (F5): mirrors Django's
//! `GroupedOffsetPaginator` / `SubGroupedOffsetPaginator`
//! (`plane/utils/paginator.py:195-633`) over the `issue_on_results` row
//! contract (`plane/utils/grouper.py:93-141`) + `issue_group_values`
//! universes (`grouper.py:143+`).
//!
//! Django shape (key SET, `paginator.py:728-743`):
//! ```json
//! {
//!   "grouped_by": "<field>", "sub_grouped_by": "<field>" | null,
//!   "total_count": N, "next_cursor": "50:1:0", "prev_cursor": "50:-1:1",
//!   "next_page_results": bool, "prev_page_results": bool,
//!   "count": <rows on this page>, "total_pages": M, "total_results": N,
//!   "extra_stats": null,
//!   "results": {
//!     "<group>": {"results": [...], "total_results": K}               // single
//!     "<group>": {"results": {"<sub>": {"results": [...],
//!       "total_results": K}}, "total_results": M}                      // nested
//!   }
//! }
//! ```
//!
//! Bucket-membership rules (mirrored exactly):
//! - Single + scalar field (`__query_grouper`): every universe key appears
//!   (empty ones with `results: []`); rows whose key is outside the universe
//!   are DROPPED from `results` (membership check, still counted in scope
//!   totals).
//! - Single + m2m field (`__query_multi_grouper`): one issue lands in every
//!   group it belongs to; only groups present on THIS page appear, in
//!   encounter order; empty membership lands in the dynamic `"None"` group.
//! - Nested (any sub level): universe groups with data-driven sub combos per
//!   group (`__get_field_dict`); rows outside the seeded `(group, sub)`
//!   combos are dropped. (Django would `KeyError`-500 on an unseeded group
//!   in the scalar-sub path; the membership check in the multi path drops
//!   them — Rust drops uniformly, documented sane-mapping.)
//!
//! Windowing mirrors the `ROW_NUMBER() OVER (PARTITION BY <group[, sub]>
//! ORDER BY <key> [DESC NULLS LAST,] created_at DESC)` slices
//! (`paginator.py:250-268, 452-470`): page P with per-page N takes partition
//! rows `(P*N, P*N+N]`.
//!
//! Deviations (documented, reviewer-adjudicable):
//! - Two-phase fetch: handlers scan `(id + group keys)` over their full
//!   scope (same WHERE/ORDER as the flat path) and fetch full rows only for
//!   the page ids. Django windows in SQL and materializes the page; the
//!   slices are identical whenever the scan order equals Django's
//!   `(order key, created_at DESC)` partition order.
//! - `id ASC` is appended as a final tiebreak (Django leaves ties
//!   unspecified — Postgres order); only observable on equal
//!   `(order key, created_at)` ties.
//! - `created_by`/`target_date`/`start_date` universes derive from the scan
//!   (scope-distinct); Django derives them from the endpoint queryset — the
//!   same scope, so the sets match.
//! - Per-group `total_results` derive from handler-scope totals; Django
//!   intersects an extra `count_filter` (intake/draft/archived exclusions).
//!   Identical whenever the handler scope already implies the filter.
//! - Sub-key order within a group is scan-encounter order (Django's is the
//!   totals-query order — unspecified).
//! - Miss bodies stay the handlers' `missing()` (DRF `{"detail": ...}`
//!   normalized per repo rule); row shapes stay each handler's existing
//!   rows (full serializer shapes remain T1 per entry).

use serde_json::{Map, Value};
use std::collections::HashMap;

/// m2m fields that explode one issue into many groups
/// (`paginator.py:197-201, 393-397` FIELD_MAPPER keys).
pub(crate) const GROUP_M2M_FIELDS: &[&str] = &["labels__id", "assignees__id", "issue_module__module_id"];

/// Fixed universes (`issue_group_values`, `grouper.py:187-196`).
pub(crate) const PRIORITY_UNIVERSE: &[&str] = &["low", "medium", "high", "urgent", "none"];
pub(crate) const STATE_GROUP_UNIVERSE: &[&str] = &["backlog", "unstarted", "started", "completed", "cancelled"];

/// Scope-distinct fields whose universe derives from the scan rows
/// (same scope as Django's queryset-derived universes).
pub(crate) const SCAN_DERIVED_FIELDS: &[&str] = &["created_by", "target_date", "start_date"];

/// One scanned scope row: the issue id plus every groupable key as text.
/// UUIDs render `::text` (lowercase hex, same as Django `str(uuid)`);
/// dates render `::text` (`YYYY-MM-DD`, same as `str(date)`); NULL stays
/// `None` (Django `str(None) == "None"` is applied at bucketing time).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ScanRow {
    pub id: uuid::Uuid,
    pub state_id: Option<String>,
    pub state_group: Option<String>,
    pub priority: Option<String>,
    pub label_ids: Vec<String>,
    pub assignee_ids: Vec<String>,
    pub module_ids: Vec<String>,
    pub cycle_id: Option<String>,
    pub project_id: Option<String>,
    pub created_by: Option<String>,
    pub target_date: Option<String>,
    pub start_date: Option<String>,
}

impl ScanRow {
    fn scalar(&self, field: &str) -> Option<&str> {
        match field {
            "state_id" => self.state_id.as_deref(),
            "state__group" => self.state_group.as_deref(),
            "priority" => self.priority.as_deref(),
            "cycle_id" => self.cycle_id.as_deref(),
            "project_id" => self.project_id.as_deref(),
            "created_by" => self.created_by.as_deref(),
            "target_date" => self.target_date.as_deref(),
            "start_date" => self.start_date.as_deref(),
            _ => None,
        }
    }

    fn multi(&self, field: &str) -> Option<&[String]> {
        match field {
            "labels__id" => Some(&self.label_ids),
            "assignees__id" => Some(&self.assignee_ids),
            "issue_module__module_id" => Some(&self.module_ids),
            _ => None,
        }
    }
}

/// Group keys for one row under `field`: m2m members, or the scalar, with
/// missing/empty mapping to the dynamic `"None"` group (Django `str(None)`).
pub(crate) fn keys_for(row: &ScanRow, field: &str) -> Vec<String> {
    if let Some(members) = row.multi(field) {
        if members.is_empty() {
            return vec!["None".to_string()];
        }
        return members.to_vec();
    }
    vec![row.scalar(field).unwrap_or("None").to_string()]
}

pub(crate) fn is_m2m(field: &str) -> bool {
    GROUP_M2M_FIELDS.contains(&field)
}

/// One planned (sub-)group: scope total + page-window issue ids in scan order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedBucket {
    pub key: String,
    pub total: i64,
    pub page_ids: Vec<uuid::Uuid>,
    pub subs: Vec<PlannedBucket>,
}

/// Planned grouped page over scan rows (scan order = partition order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupedPlan {
    /// Buckets per the membership rules above.
    pub buckets: Vec<PlannedBucket>,
    /// Scope row count (Django `hits`).
    pub total_count: i64,
    /// Flat page row count (Django `count`).
    pub page_rows: i64,
    /// Any partition holds rows past this window (Django `next.has_results`).
    pub next_has: bool,
    /// Max single-group scope total (Django `max_hits` numerator).
    pub max_group: i64,
}

/// Plan one grouped page. Returns `None` when the window is negative
/// (Django `BadPaginationError` → 400); saturating-huge offsets plan an
/// empty page (Django slices to `[]` with a 200).
pub(crate) fn plan_grouped(
    scan: &[ScanRow],
    group_by: &str,
    sub_group_by: Option<&str>,
    group_universe: &[String],
    per_page: i64,
    page: i128,
) -> Option<GroupedPlan> {
    let total_count = scan.len() as i64;
    let offset128 = page.saturating_mul(i128::from(per_page));
    if offset128 < 0 {
        return None;
    }
    // Partition (group[, sub]) -> row indices in scan order, plus scope
    // totals per group and per (group, sub).
    let mut part_idx: HashMap<(String, Option<String>), Vec<usize>> = HashMap::new();
    let mut group_seen: Vec<String> = Vec::new();
    let mut sub_seen: HashMap<String, Vec<String>> = HashMap::new();
    for (idx, row) in scan.iter().enumerate() {
        let gkeys = keys_for(row, group_by);
        let skeys: Vec<Option<String>> = match sub_group_by {
            Some(sub) => keys_for(row, sub).into_iter().map(Some).collect(),
            None => vec![None],
        };
        for g in &gkeys {
            if !group_seen.contains(g) {
                group_seen.push(g.clone());
            }
            for s in &skeys {
                if let Some(s) = s {
                    let entry = sub_seen.entry(g.clone()).or_default();
                    if !entry.contains(s) {
                        entry.push(s.clone());
                    }
                }
                part_idx.entry((g.clone(), s.clone())).or_default().push(idx);
            }
        }
    }
    let group_total = |g: &str| -> i64 {
        part_idx
            .iter()
            .filter(|((key, _), _)| key == g)
            .map(|(_, idxs)| idxs.len() as i64)
            .sum()
    };
    let max_group = group_seen.iter().map(|g| group_total(g)).max().unwrap_or(0);
    // BeyondEnd: Django's window filter matches nothing → `results: {}`.
    if offset128 > i128::from(i64::MAX) {
        return Some(GroupedPlan {
            buckets: Vec::new(),
            total_count,
            page_rows: 0,
            next_has: false,
            max_group: 0,
        });
    }
    let offset = offset128 as i64;
    // Windowed ids for one partition: row numbers (offset, offset + N].
    let window_ids = |idxs: &[usize]| -> Vec<uuid::Uuid> {
        idxs.iter()
            .enumerate()
            .filter(|(pos, _)| {
                let rn = *pos as i64 + 1;
                rn > offset && rn <= offset + per_page
            })
            .map(|(_, i)| scan[*i].id)
            .collect()
    };
    let part_has_next = |idxs: &[usize]| -> bool { idxs.len() as i64 > offset + per_page };
    let mut buckets = Vec::new();
    let mut page_rows: i64 = 0;
    let mut next_has = false;
    if sub_group_by.is_some() {
        // Nested: universe groups only; sub combos data-driven per group.
        for g in group_universe {
            let mut subs = Vec::new();
            let mut g_page = Vec::new();
            for s in sub_seen.get(g).map(Vec::as_slice).unwrap_or(&[]) {
                let idxs = part_idx.get(&(g.clone(), Some(s.clone())));
                let idxs = idxs.map(Vec::as_slice).unwrap_or(&[]);
                let ids = window_ids(idxs);
                if part_has_next(idxs) {
                    next_has = true;
                }
                page_rows += ids.len() as i64;
                g_page.extend(ids.iter().cloned());
                subs.push(PlannedBucket {
                    key: s.clone(),
                    total: idxs.len() as i64,
                    page_ids: ids,
                    subs: Vec::new(),
                });
            }
            buckets.push(PlannedBucket {
                key: g.clone(),
                total: group_total(g),
                page_ids: g_page,
                subs,
            });
        }
    } else if is_m2m(group_by) {
        // Single m2m: only groups present on this page, encounter order.
        for g in &group_seen {
            let idxs = part_idx.get(&(g.clone(), None));
            let idxs = idxs.map(Vec::as_slice).unwrap_or(&[]);
            let ids = window_ids(idxs);
            if ids.is_empty() {
                continue;
            }
            if part_has_next(idxs) {
                next_has = true;
            }
            page_rows += ids.len() as i64;
            buckets.push(PlannedBucket {
                key: g.clone(),
                total: idxs.len() as i64,
                page_ids: ids,
                subs: Vec::new(),
            });
        }
    } else {
        // Single scalar: every universe key (rows outside it dropped).
        for g in group_universe {
            let idxs = part_idx.get(&(g.clone(), None));
            let idxs = idxs.map(Vec::as_slice).unwrap_or(&[]);
            let ids = window_ids(idxs);
            if part_has_next(idxs) {
                next_has = true;
            }
            page_rows += ids.len() as i64;
            buckets.push(PlannedBucket {
                key: g.clone(),
                total: idxs.len() as i64,
                page_ids: ids,
                subs: Vec::new(),
            });
        }
    }
    Some(GroupedPlan {
        buckets,
        total_count,
        page_rows,
        next_has,
        max_group,
    })
}

/// Render the Django grouped envelope (`paginator.py:728-743`). `rows_by_id`
/// maps page issue ids to their serialized row values.
pub(crate) fn grouped_envelope(
    group_by: &str,
    sub_group_by: Option<&str>,
    total_count: i64,
    per_page: i64,
    page: i128,
    plan: &GroupedPlan,
    rows_by_id: &HashMap<uuid::Uuid, Value>,
) -> Value {
    let rows_of = |ids: &[uuid::Uuid]| -> Vec<Value> {
        ids.iter().filter_map(|id| rows_by_id.get(id)).cloned().collect()
    };
    let mut results = Map::new();
    for b in &plan.buckets {
        if sub_group_by.is_some() {
            let mut subs = Map::new();
            for s in &b.subs {
                subs.insert(
                    s.key.clone(),
                    serde_json::json!({"results": rows_of(&s.page_ids), "total_results": s.total}),
                );
            }
            results.insert(
                b.key.clone(),
                serde_json::json!({"results": Value::Object(subs), "total_results": b.total}),
            );
        } else {
            results.insert(
                b.key.clone(),
                serde_json::json!({"results": rows_of(&b.page_ids), "total_results": b.total}),
            );
        }
    }
    let max_hits = if plan.page_rows > 0 {
        crate::routes::issue_common::total_pages(plan.max_group, per_page.max(1))
    } else {
        0
    };
    serde_json::json!({
        "grouped_by": group_by,
        "sub_grouped_by": sub_group_by,
        "total_count": total_count,
        "next_cursor": crate::routes::issue_common::next_cursor_str(per_page, page),
        "prev_cursor": crate::routes::issue_common::prev_cursor_str(per_page, page),
        "next_page_results": plan.next_has,
        "prev_page_results": page > 0,
        "count": plan.page_rows,
        "total_pages": max_hits,
        "total_results": total_count,
        "extra_stats": null,
        "results": results,
    })
}

/// Field universe (`issue_group_values`, `grouper.py:143+`). `project_id`
/// scopes project-level universes; `slug` scopes workspace-level ones.
/// Date/`created_by` universes are scan-derived by callers (same scope as
/// Django's queryset-derived lists) via [`scan_universe`].
pub(crate) async fn group_universe(
    pool: &sqlx::PgPool,
    field: &str,
    slug: &str,
    project_id: Option<uuid::Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    let mut out: Vec<String> = match field {
        "priority" => PRIORITY_UNIVERSE.iter().map(|s| s.to_string()).collect(),
        "state__group" => STATE_GROUP_UNIVERSE.iter().map(|s| s.to_string()).collect(),
        "state_id" => {
            let rows: Vec<uuid::Uuid> = sqlx::query_scalar(
                "SELECT s.id FROM states s JOIN workspaces w ON w.id = s.workspace_id WHERE w.slug = $1 AND ($2::uuid IS NULL OR s.project_id = $2) AND s.is_triage = false AND s.deleted_at IS NULL ORDER BY s.sequence ASC",
            )
            .bind(slug)
            .bind(project_id)
            .fetch_all(pool)
            .await?;
            rows.into_iter().map(|id| id.to_string()).collect()
        }
        "labels__id" => {
            let mut rows: Vec<String> = sqlx::query_scalar(
                "SELECT l.id::text FROM labels l JOIN workspaces w ON w.id = l.workspace_id WHERE w.slug = $1 AND ($2::uuid IS NULL OR l.project_id = $2) AND l.deleted_at IS NULL",
            )
            .bind(slug)
            .bind(project_id)
            .fetch_all(pool)
            .await?;
            rows.push("None".to_string());
            rows
        }
        "assignees__id" => match project_id {
            Some(pid) => {
                sqlx::query_scalar(
                    "SELECT pm.member_id::text FROM project_members pm JOIN workspaces w ON w.id = pm.workspace_id WHERE w.slug = $1 AND pm.project_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL",
                )
                .bind(slug)
                .bind(pid)
                .fetch_all(pool)
                .await?
            }
            None => {
                sqlx::query_scalar(
                    "SELECT wm.member_id::text FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id WHERE w.slug = $1 AND wm.is_active = true AND wm.deleted_at IS NULL",
                )
                .bind(slug)
                .fetch_all(pool)
                .await?
            }
        },
        "issue_module__module_id" => {
            let mut rows: Vec<String> = sqlx::query_scalar(
                "SELECT m.id::text FROM modules m JOIN workspaces w ON w.id = m.workspace_id WHERE w.slug = $1 AND ($2::uuid IS NULL OR m.project_id = $2) AND m.deleted_at IS NULL",
            )
            .bind(slug)
            .bind(project_id)
            .fetch_all(pool)
            .await?;
            rows.push("None".to_string());
            rows
        }
        "cycle_id" => {
            let mut rows: Vec<String> = sqlx::query_scalar(
                "SELECT c.id::text FROM cycles c JOIN workspaces w ON w.id = c.workspace_id WHERE w.slug = $1 AND ($2::uuid IS NULL OR c.project_id = $2) AND c.deleted_at IS NULL",
            )
            .bind(slug)
            .bind(project_id)
            .fetch_all(pool)
            .await?;
            rows.push("None".to_string());
            rows
        }
        "project_id" => {
            sqlx::query_scalar(
                "SELECT p.id::text FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $1 AND p.deleted_at IS NULL",
            )
            .bind(slug)
            .fetch_all(pool)
            .await?
        }
        _ => Vec::new(),
    };
    out.retain(|s| !s.is_empty());
    Ok(out)
}

/// Scan-derived universe for `created_by`/`target_date`/`start_date`
/// (scope-distinct, first-seen order).
pub(crate) fn scan_universe(scan: &[ScanRow], field: &str) -> Vec<String> {
    let mut out = Vec::new();
    for row in scan {
        for key in keys_for(row, field) {
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

#[cfg(test)]
mod grouped_tests {
    use super::*;

    fn row(id: u128, prio: Option<&str>, labels: &[&str]) -> ScanRow {
        ScanRow {
            id: uuid::Uuid::from_u128(id),
            state_id: None,
            state_group: None,
            priority: prio.map(|s| s.to_string()),
            label_ids: labels.iter().map(|s| s.to_string()).collect(),
            assignee_ids: Vec::new(),
            module_ids: Vec::new(),
            cycle_id: None,
            project_id: None,
            created_by: None,
            target_date: None,
            start_date: None,
        }
    }

    #[test]
    fn single_scalar_windows_per_group_and_seeds_universe() {
        let scan = vec![
            row(1, Some("high"), &[]),
            row(2, Some("high"), &[]),
            row(3, Some("high"), &[]),
            row(4, Some("low"), &[]),
        ];
        let plan = plan_grouped(&scan, "priority", None, &["high".into(), "low".into()], 2, 0).unwrap();
        assert_eq!(plan.total_count, 4);
        assert_eq!(plan.page_rows, 3);
        assert!(plan.next_has);
        assert_eq!(plan.max_group, 3);
        assert_eq!(plan.buckets[0].key, "high");
        assert_eq!(plan.buckets[0].total, 3);
        assert_eq!(plan.buckets[0].page_ids.len(), 2);
        assert_eq!(plan.buckets[1].page_ids.len(), 1);
        // Empty universe keys still appear.
        let plan = plan_grouped(&scan, "priority", None, &["high".into(), "low".into(), "urgent".into()], 50, 0).unwrap();
        assert_eq!(plan.buckets.len(), 3);
        assert_eq!(plan.buckets[2].total, 0);
        // Page 1 → high's last row only.
        let plan = plan_grouped(&scan, "priority", None, &["high".into(), "low".into()], 2, 1).unwrap();
        assert_eq!(plan.page_rows, 1);
        assert!(!plan.next_has);
        assert!(plan.buckets[1].page_ids.is_empty());
    }

    #[test]
    fn single_scalar_drops_out_of_universe_rows() {
        // NULL priority → "None", absent from the universe → dropped.
        let scan = vec![row(1, None, &[]), row(2, Some("high"), &[])];
        let plan = plan_grouped(&scan, "priority", None, &["high".into()], 50, 0).unwrap();
        assert_eq!(plan.total_count, 2);
        assert_eq!(plan.page_rows, 1);
        assert_eq!(plan.buckets.len(), 1);
    }

    #[test]
    fn single_m2m_lists_page_groups_in_encounter_order() {
        let scan = vec![row(1, None, &["a", "b"]), row(2, None, &[])];
        let plan = plan_grouped(&scan, "labels__id", None, &["a".into(), "b".into()], 50, 0).unwrap();
        let ids = |key: &str| plan.buckets.iter().find(|b| b.key == key).unwrap().page_ids.clone();
        assert_eq!(ids("a").len(), 1);
        assert_eq!(ids("b").len(), 1);
        assert_eq!(ids("a"), ids("b"));
        assert_eq!(ids("None").len(), 1);
        // Empty universe group with no page rows is omitted (multi path).
        assert!(plan.buckets.iter().all(|b| b.key != "zzz"));
        assert_eq!(plan.total_count, 2);
    }

    #[test]
    fn nested_partitions_within_group() {
        let scan = vec![row(1, Some("high"), &["x"]), row(2, Some("high"), &["y"]), row(3, Some("high"), &["x"])];
        let plan = plan_grouped(&scan, "priority", Some("labels__id"), &["high".into(), "low".into()], 1, 0).unwrap();
        assert_eq!(plan.buckets.len(), 2);
        let high = &plan.buckets[0];
        assert_eq!(high.total, 3);
        assert_eq!(high.subs.len(), 2);
        let x = high.subs.iter().find(|s| s.key == "x").unwrap();
        assert_eq!(x.total, 2);
        assert_eq!(x.page_ids.len(), 1);
        assert!(plan.next_has);
        // Seeded-but-empty group keeps data-driven (empty) subs.
        assert!(plan.buckets[1].subs.is_empty());
    }

    #[test]
    fn negative_window_is_none() {
        let scan = vec![row(1, Some("high"), &[])];
        assert!(plan_grouped(&scan, "priority", None, &["high".into()], 50, -1).is_none());
    }

    #[test]
    fn envelope_keys_match_django() {
        let scan = vec![row(1, Some("high"), &[])];
        let plan = plan_grouped(&scan, "priority", None, &["high".into()], 50, 0).unwrap();
        let mut rows = HashMap::new();
        rows.insert(uuid::Uuid::from_u128(1), serde_json::json!({"id": "x"}));
        let env = grouped_envelope("priority", None, 1, 50, 0, &plan, &rows);
        for key in [
            "grouped_by", "sub_grouped_by", "total_count", "next_cursor", "prev_cursor",
            "next_page_results", "prev_page_results", "count", "total_pages",
            "total_results", "extra_stats", "results",
        ] {
            assert!(env.get(key).is_some(), "missing {key}");
        }
        assert_eq!(env["grouped_by"], serde_json::json!("priority"));
        assert!(env["sub_grouped_by"].is_null());
        assert_eq!(env["next_cursor"], serde_json::json!("50:1:0"));
        assert_eq!(env["prev_cursor"], serde_json::json!("50:-1:1"));
        assert_eq!(env["results"]["high"]["total_results"], serde_json::json!(1));
        assert_eq!(env["total_pages"], serde_json::json!(1));
    }
}
