//! Read-only, workspace-scoped tools for the Rig agent.
//!
//! Every query filters `workspace_id = $1` captured from the authenticated
//! handler; the model never chooses the workspace. Filters are optional
//! nullable bind parameters (`$n::text IS NULL OR ...`) so the SQL stays
//! static and the pure helpers below are unit-testable without a DB.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{record, ToolTrace};

pub const PROJECTS_SQL: &str = "SELECT identifier, name FROM projects \
     WHERE workspace_id = $1 AND deleted_at IS NULL AND archived_at IS NULL \
     ORDER BY name LIMIT 50";

pub const COUNT_SQL: &str = "SELECT count(*)::int8 FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 \
       AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     LEFT JOIN states s ON s.id = i.state_id AND s.workspace_id = $1 \
       AND s.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND (s.\"group\" IS NULL OR s.\"group\" <> 'triage') \
     AND ($2::text IS NULL OR p.identifier ILIKE $2 OR p.name ILIKE '%' || $2 || '%') \
     AND ($3::text IS NULL OR s.\"group\" = $3) \
     AND ($4::text IS NULL OR i.priority = $4) \
     AND ($5::bool OR i.archived_at IS NULL)";

pub const SEARCH_SQL: &str = "SELECT p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, \
     i.name, COALESCE(s.name, '') AS state, i.priority \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 \
       AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     LEFT JOIN states s ON s.id = i.state_id AND s.workspace_id = $1 \
       AND s.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND (s.\"group\" IS NULL OR s.\"group\" <> 'triage') \
     AND i.archived_at IS NULL \
     AND ($2::text IS NULL OR i.name ILIKE '%' || $2 || '%') \
     AND ($3::text IS NULL OR p.identifier ILIKE $3 OR p.name ILIKE '%' || $3 || '%') \
     AND ($4::text IS NULL OR s.\"group\" = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     ORDER BY i.updated_at DESC LIMIT $6";

pub const STATE_GROUPS: [&str; 5] = ["backlog", "unstarted", "started", "completed", "cancelled"];
pub const PRIORITIES: [&str; 5] = ["urgent", "high", "medium", "low", "none"];
pub const DEFAULT_LIMIT: i64 = 10;
pub const MAX_LIMIT: i64 = 25;

pub fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn state_group_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|s| s.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if STATE_GROUPS.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "state_group must be one of: {}",
            STATE_GROUPS.join(", ")
        )))
    }
}

pub fn priority_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|s| s.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if PRIORITIES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "priority must be one of: {}",
            PRIORITIES.join(", ")
        )))
    }
}

pub fn clamp_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub fn projects_json(rows: &[(String, String)]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|(identifier, name)| json!({"identifier": identifier, "name": name}))
        .collect();
    json!({"items": items}).to_string()
}

pub fn count_json(
    count: i64,
    project: Option<&str>,
    state_group: Option<&str>,
    priority: Option<&str>,
    include_archived: bool,
) -> String {
    json!({
        "count": count,
        "filters": {
            "project": project,
            "state_group": state_group,
            "priority": priority,
            "include_archived": include_archived
        }
    })
    .to_string()
}

pub fn search_json(rows: &[(String, String, String, String, String)]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|(project, identifier, name, state, priority)| {
            json!({
                "project": project,
                "identifier": identifier,
                "name": name,
                "state": state,
                "priority": priority
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sql_constants_are_workspace_scoped() {
        for sql in [PROJECTS_SQL, COUNT_SQL, SEARCH_SQL] {
            assert!(sql.contains("workspace_id = $1"), "missing workspace scope: {sql}");
        }
    }

    #[test]
    fn state_group_allowlist() {
        assert_eq!(state_group_arg(None).unwrap(), None);
        assert_eq!(state_group_arg(Some("  ")).unwrap(), None);
        assert_eq!(state_group_arg(Some("Started")).unwrap(), Some("started".to_string()));
        let err = state_group_arg(Some("nope")).unwrap_err();
        assert!(err.to_string().contains("backlog"));
    }

    #[test]
    fn priority_allowlist() {
        assert_eq!(priority_arg(None).unwrap(), None);
        assert_eq!(priority_arg(Some("URGENT")).unwrap(), Some("urgent".to_string()));
        let err = priority_arg(Some("p0")).unwrap_err();
        assert!(err.to_string().contains("urgent"));
    }

    #[test]
    fn limit_is_clamped() {
        assert_eq!(clamp_limit(None), 10);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(3)), 3);
        assert_eq!(clamp_limit(Some(999)), 25);
    }

    #[test]
    fn optional_text_trims_and_drops_empty() {
        assert_eq!(optional_text(None), None);
        assert_eq!(optional_text(Some("  ")), None);
        assert_eq!(optional_text(Some(" LT ")), Some("LT".to_string()));
    }

    #[test]
    fn output_json_shapes() {
        let projects = projects_json(&[("LTS".to_string(), "Logistics".to_string())]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&projects).unwrap(),
            json!({"items": [{"identifier": "LTS", "name": "Logistics"}]})
        );
        let count = count_json(7, Some("LTS"), Some("started"), Some("urgent"), false);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&count).unwrap(),
            json!({
                "count": 7,
                "filters": {
                    "project": "LTS",
                    "state_group": "started",
                    "priority": "urgent",
                    "include_archived": false
                }
            })
        );
        let search = search_json(&[(
            "LTS".to_string(),
            "LTS-12".to_string(),
            "Fix pump".to_string(),
            "In Progress".to_string(),
            "urgent".to_string(),
        )]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&search).unwrap(),
            json!({"returned": 1, "items": [{
                "project": "LTS",
                "identifier": "LTS-12",
                "name": "Fix pump",
                "state": "In Progress",
                "priority": "urgent"
            }]})
        );
    }
}
