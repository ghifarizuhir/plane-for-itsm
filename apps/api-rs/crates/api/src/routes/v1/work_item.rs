//! v1 work-item handlers (`/api/v1/.../work-items/...`).

use serde_json::{json, Value};

use crate::routes::issue_common::IssueListRow;

/// Serialize a list row and add the SDK-key aliases `assignees`/`labels`
/// (the fork's rows carry `assignee_ids`/`label_ids`). `WorkItemDetail`'s
/// `manage_assignee`/`manage_label` read `assignees`/`labels` back, so the
/// aliases are required for the MCP's merge-then-write path.
pub fn v1_work_item_json(row: &IssueListRow) -> Value {
    let mut v = serde_json::to_value(row).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        let assignees = o.get("assignee_ids").cloned().unwrap_or_else(|| json!([]));
        let labels = o.get("label_ids").cloned().unwrap_or_else(|| json!([]));
        o.insert("assignees".to_string(), assignees);
        o.insert("labels".to_string(), labels);
    }
    v
}

/// `WorkItemGroupedCountResponse`: flat `{grouped_by, sub_grouped_by,
/// total_count, grouped_counts}`. `counts` is `(key, count)` pairs; the
/// special key `"None"` represents a NULL dimension.
pub fn v1_count_json(
    grouped_by: Option<&str>,
    sub_grouped_by: Option<&str>,
    total: i64,
    counts: Vec<(String, i64)>,
) -> Value {
    let mut map = serde_json::Map::new();
    for (k, n) in counts {
        map.insert(k, json!({ "count": n }));
    }
    json!({
        "grouped_by": grouped_by,
        "sub_grouped_by": sub_grouped_by,
        "total_count": total,
        "grouped_counts": Value::Object(map),
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct V1SearchRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub project_identifier: String,
    pub workspace_slug: String,
}

pub fn v1_search_issue_json(row: &V1SearchRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "sequence_id": row.sequence_id,
        "project_id": row.project_id,
        "project__identifier": row.project_identifier,
        "workspace__slug": row.workspace_slug,
    })
}
