//! Sprint (cycle) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature,
};
use crate::agent::{record, ToolTrace};

pub const SPRINT_STATUSES: [&str; 4] = ["current", "upcoming", "completed", "draft"];

pub fn sprint_status_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if SPRINT_STATUSES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "status must be one of: {}",
            SPRINT_STATUSES.join(", ")
        )))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SprintRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub owner: String,
    pub total: i64,
    pub completed: i64,
}

pub const SPRINTS_SQL: &str = "SELECT * FROM (SELECT c.id, c.name, \
     p.identifier AS project, c.start_date::text AS start_date, \
     c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, \
     COALESCE((SELECT count(*) FROM cycle_issues ci \
       JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
       WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM cycle_issues ci \
       JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND c.archived_at IS NULL \
     AND p.cycle_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($3::uuid IS NULL OR p.id = $3) \
     AND ($4::text IS NULL OR c.name ILIKE '%' || $4 || '%')) t \
     WHERE ($5::text IS NULL OR t.status = $5) \
     ORDER BY t.start_date DESC NULLS LAST, t.name LIMIT $6";

pub fn sprints_json(rows: &[SprintRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "project": row.project,
                "start_date": row.start_date,
                "end_date": row.end_date,
                "status": row.status,
                "owner": row.owner,
                "total": row.total,
                "completed": row.completed,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListSprintsArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: current, upcoming, completed, draft.
    pub status: Option<String>,
    /// Case-insensitive substring to match against sprint names.
    pub query: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListSprints {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListSprints {
    const NAME: &'static str = "list_sprints";
    type Args = ListSprintsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List sprints (cycles) the caller can access, newest first, with start \
         and end dates, computed status (current, upcoming, completed, draft), \
         owner, and progress (total and completed work items). Only projects \
         with the sprints feature enabled are included."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListSprintsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Cycles).await?;
        }
        let status = sprint_status_arg(args.status.as_deref())?;
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<SprintRow> = sqlx::query_as(SPRINTS_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(project.as_ref().map(|row| row.id))
            .bind(&query)
            .bind(&status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(sprints_json(&rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sprint_status_allowlist() {
        assert_eq!(sprint_status_arg(None).unwrap(), None);
        assert_eq!(
            sprint_status_arg(Some("Current")).unwrap(),
            Some("current".to_string())
        );
        assert!(sprint_status_arg(Some("someday")).is_err());
    }

    #[test]
    fn sprints_json_shape() {
        let rows = vec![SprintRow {
            id: Uuid::nil(),
            name: "Sprint 3".to_string(),
            project: "LTS".to_string(),
            start_date: Some("2026-10-01 00:00:00+00".to_string()),
            end_date: Some("2026-10-14 00:00:00+00".to_string()),
            status: "current".to_string(),
            owner: "Budi".to_string(),
            total: 12,
            completed: 5,
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&sprints_json(&rows)).unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["name"], json!("Sprint 3"));
        assert_eq!(parsed["items"][0]["status"], json!("current"));
        assert_eq!(parsed["items"][0]["completed"], json!(5));
    }

    #[test]
    fn sprints_sql_is_scoped_guarded_and_gated() {
        assert!(SPRINTS_SQL.contains("workspace_id = $1"));
        assert!(SPRINTS_SQL.contains("project_members"));
        assert!(SPRINTS_SQL.contains("p.cycle_view = true"));
        assert!(SPRINTS_SQL.contains("c.deleted_at IS NULL"));
    }
}
