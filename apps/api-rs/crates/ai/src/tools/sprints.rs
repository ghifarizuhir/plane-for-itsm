//! Sprint (cycle) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature, ProjectRow,
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

pub const SPRINT_BY_ID_SQL: &str = "SELECT c.id, c.name, p.identifier AS project, \
     c.start_date::text AS start_date, c.end_date::text AS end_date, \
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
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.id = $1 AND c.workspace_id = $2 AND c.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

pub const SPRINTS_BY_NAME_PROJECT_SQL: &str = "SELECT c.id, c.name, \
     p.identifier AS project, c.start_date::text AS start_date, \
     c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, 0::int8 AS total, \
     0::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.project_id = $1 AND c.workspace_id = $2 AND c.deleted_at IS NULL \
     AND c.archived_at IS NULL \
     AND (lower(c.name) = lower($3) OR c.name ILIKE '%' || $3 || '%') \
     ORDER BY c.start_date DESC NULLS LAST, c.name LIMIT 10";

pub const SPRINTS_BY_NAME_SQL: &str = "SELECT c.id, c.name, p.identifier AS project, \
     c.start_date::text AS start_date, c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, 0::int8 AS total, \
     0::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND c.archived_at IS NULL \
     AND p.cycle_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(c.name) = lower($3) OR c.name ILIKE '%' || $3 || '%') \
     ORDER BY c.start_date DESC NULLS LAST, c.name LIMIT 10";

pub fn pick_sprint(rows: Vec<SprintRow>, reference: &str) -> Result<SprintRow, ToolExecutionError> {
    let needle = reference.trim();
    if rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "sprint '{needle}' was not found or is not accessible"
        )));
    }
    let exact: Vec<&SprintRow> = rows
        .iter()
        .filter(|row| row.name.eq_ignore_ascii_case(needle))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }
    if exact.len() > 1 || rows.len() > 1 {
        let names = rows
            .iter()
            .map(|row| format!("{}/{}", row.project, row.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolExecutionError::invalid_args(format!(
            "sprint '{needle}' is ambiguous; use one of: {names}"
        )));
    }
    Ok(rows.into_iter().next().expect("one row"))
}

pub async fn resolve_sprint(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
    project: Option<&ProjectRow>,
) -> Result<SprintRow, ToolExecutionError> {
    if let Ok(id) = Uuid::parse_str(reference.trim()) {
        let row: Option<SprintRow> = sqlx::query_as(SPRINT_BY_ID_SQL)
            .bind(id)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
        return row.ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "sprint '{reference}' was not found or is not accessible"
            ))
        });
    }
    let rows: Vec<SprintRow> = match project {
        Some(project) => sqlx::query_as(SPRINTS_BY_NAME_PROJECT_SQL)
            .bind(project.id)
            .bind(workspace_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
        None => sqlx::query_as(SPRINTS_BY_NAME_SQL)
            .bind(workspace_id)
            .bind(user_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
    };
    pick_sprint(rows, reference)
}

pub const SPRINT_STATE_COUNTS_SQL: &str = "SELECT s.\"group\" AS state_group, \
     count(*)::int8 AS count \
     FROM cycle_issues ci \
     JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
     GROUP BY s.\"group\" ORDER BY s.\"group\"";

pub const SPRINT_OPEN_ITEMS_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, COALESCE(s.\"group\", '') AS state_group, i.priority \
     FROM cycle_issues ci \
     JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
     AND s.\"group\" NOT IN ('completed', 'cancelled') \
     ORDER BY i.updated_at DESC LIMIT 10";

pub fn sprint_detail_json(
    row: &SprintRow,
    counts: &[crate::tools::work_items::GroupCountRow],
    open_items: &[crate::tools::work_items::WorkItemBrief],
) -> String {
    json!({
        "name": row.name,
        "project": row.project,
        "status": row.status,
        "start_date": row.start_date,
        "end_date": row.end_date,
        "owner": row.owner,
        "total": row.total,
        "completed": row.completed,
        "state_counts": counts,
        "open_work_items": open_items,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSprintArgs {
    /// Sprint name, or a sprint id when known.
    pub sprint: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
}

pub struct GetSprint {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetSprint {
    const NAME: &'static str = "get_sprint";
    type Args = GetSprintArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one sprint by name or id: dates, computed status, owner, progress \
         (total and completed), the count of work items per state group \
         (computed live), and the 10 most recently updated unfinished items."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetSprintArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                ensure_feature(&self.pool, project.id, ProjectFeature::Cycles).await?;
                Some(project)
            }
            None => None,
        };
        let row = resolve_sprint(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.sprint,
            project.as_ref(),
        )
        .await?;
        let counts: Vec<crate::tools::work_items::GroupCountRow> =
            sqlx::query_as(SPRINT_STATE_COUNTS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        let open_items: Vec<crate::tools::work_items::WorkItemBrief> =
            sqlx::query_as(SPRINT_OPEN_ITEMS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        Ok(sprint_detail_json(&row, &counts, &open_items))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListSprintWorkItemsArgs {
    /// Sprint name, or a sprint id when known.
    pub sprint: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// Assignee display name, email, or "me" for the signed-in user.
    pub assignee: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub const SPRINT_ITEMS_SQL: &str = "SELECT p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, i.priority, \
     COALESCE((SELECT it.name FROM issue_types it \
       WHERE it.id = i.type_id AND it.deleted_at IS NULL), '') AS work_item_type, \
     COALESCE((SELECT string_agg(COALESCE(u.display_name, u.email, 'unknown'), ', ' \
         ORDER BY COALESCE(u.display_name, u.email, 'unknown')) \
       FROM issue_assignees ia JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '') AS assignees \
     FROM cycle_issues ci \
     JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
     AND ($2::text IS NULL OR s.\"group\" = $2) \
     AND ($3::text IS NULL OR EXISTS (SELECT 1 FROM issue_assignees ia \
       JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND (lower(COALESCE(u.display_name, '')) = lower($3) \
         OR lower(COALESCE(u.email, '')) = lower($3)))) \
     AND ($4::bool = false OR EXISTS (SELECT 1 FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL AND ia.assignee_id = $5)) \
     ORDER BY i.updated_at DESC LIMIT $6";

pub struct ListSprintWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListSprintWorkItems {
    const NAME: &'static str = "list_sprint_work_items";
    type Args = ListSprintWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List work items in one sprint, optionally filtered by state group and \
         assignee (a name, email, or \"me\"). Returns project, identifier, name, \
         state, priority, type, and assignees."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListSprintWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let state_group = super::state_group_arg(args.state_group.as_deref())?;
        let (assignee, assignee_me) = super::assignee_arg(args.assignee.as_deref());
        let limit = clamp_limit(args.limit);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Cycles).await?;
        }
        let sprint = resolve_sprint(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.sprint,
            project.as_ref(),
        )
        .await?;
        let rows: Vec<crate::tools::work_items::SearchRow> =
            sqlx::query_as(SPRINT_ITEMS_SQL)
                .bind(sprint.id)
                .bind(&state_group)
                .bind(&assignee)
                .bind(assignee_me)
                .bind(self.user_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        Ok(crate::tools::work_items::search_json(&rows))
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

    fn sprint(name: &str, project: &str) -> SprintRow {
        SprintRow {
            id: Uuid::new_v4(),
            name: name.to_string(),
            project: project.to_string(),
            start_date: None,
            end_date: None,
            status: "draft".to_string(),
            owner: String::new(),
            total: 0,
            completed: 0,
        }
    }

    #[test]
    fn pick_sprint_prefers_exact_then_unique_substring() {
        let rows = vec![sprint("Sprint 3", "LTS"), sprint("Sprint 30", "LTS")];
        assert_eq!(pick_sprint(rows.clone(), "sprint 3").unwrap().name, "Sprint 3");
        assert!(pick_sprint(rows, "sprint").is_err());
        assert!(pick_sprint(vec![], "sprint 3").is_err());
    }

    #[test]
    fn sprint_detail_json_shape() {
        let row = sprint("Sprint 3", "LTS");
        let counts = vec![crate::tools::work_items::GroupCountRow {
            state_group: "started".to_string(),
            count: 4,
        }];
        let items = vec![crate::tools::work_items::WorkItemBrief {
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "urgent".to_string(),
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&sprint_detail_json(&row, &counts, &items)).unwrap();
        assert_eq!(parsed["name"], json!("Sprint 3"));
        assert_eq!(parsed["state_counts"][0]["count"], json!(4));
        assert_eq!(parsed["open_work_items"][0]["identifier"], json!("LTS-42"));
    }

    #[tokio::test]
    async fn list_sprint_items_rejects_a_bad_state_group_before_querying() {
        let tool = ListSprintWorkItems {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                ListSprintWorkItemsArgs {
                    sprint: Uuid::new_v4().to_string(),
                    project: None,
                    state_group: Some("nope".to_string()),
                    assignee: None,
                    limit: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("state_group"));
    }
}
