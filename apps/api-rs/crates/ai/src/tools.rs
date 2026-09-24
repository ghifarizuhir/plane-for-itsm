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

use crate::agent::{record, ToolTrace};

pub const PROJECTS_SQL: &str = "SELECT identifier, name FROM projects \
     WHERE workspace_id = $1 AND deleted_at IS NULL AND archived_at IS NULL \
     ORDER BY name LIMIT 50";

pub const COUNT_SQL: &str = "SELECT count(*)::int8 FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 \
       AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.workspace_id = $1 \
       AND s.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND s.\"group\" <> 'triage' \
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
     JOIN states s ON s.id = i.state_id AND s.workspace_id = $1 \
       AND s.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND s.\"group\" <> 'triage' \
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

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ListProjectsArgs {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CountWorkItemsArgs {
    /// Project identifier (case-insensitive exact, e.g. "LTS") or project name (case-insensitive substring).
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Include archived work items. Defaults to false.
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchWorkItemsArgs {
    /// Case-insensitive substring to match against work item names.
    pub query: Option<String>,
    /// Project identifier (case-insensitive exact, e.g. "LTS") or project name (case-insensitive substring).
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

fn db_error(error: sqlx::Error) -> ToolExecutionError {
    tracing::warn!(error = %error, "ai-agent: tool query failed");
    ToolExecutionError::from_error(error)
}

fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| json!({"type": "object", "properties": {}}))
}

pub struct ListProjects {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListProjects {
    const NAME: &'static str = "list_projects";
    type Args = ListProjectsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List up to 50 non-archived projects in the current workspace with identifier and name."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListProjectsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let rows: Vec<(String, String)> = sqlx::query_as(PROJECTS_SQL)
            .bind(self.workspace_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(projects_json(&rows))
    }
}

pub struct CountWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for CountWorkItems {
    const NAME: &'static str = "count_work_items";
    type Args = CountWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Count non-deleted, non-draft work items in the current workspace, excluding triage items, issues with a missing or deleted state, and issues in deleted or archived projects. Archived items are excluded unless include_archived is true. Optionally filtered by project, state group, and priority.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CountWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = optional_text(args.project.as_deref());
        let state_group = state_group_arg(args.state_group.as_deref())?;
        let priority = priority_arg(args.priority.as_deref())?;
        let include_archived = args.include_archived.unwrap_or(false);
        let count: i64 = sqlx::query_scalar(COUNT_SQL)
            .bind(self.workspace_id)
            .bind(&project)
            .bind(&state_group)
            .bind(&priority)
            .bind(include_archived)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(count_json(
            count,
            project.as_deref(),
            state_group.as_deref(),
            priority.as_deref(),
            include_archived,
        ))
    }
}

pub struct SearchWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for SearchWorkItems {
    const NAME: &'static str = "search_work_items";
    type Args = SearchWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Search non-archived, non-draft work items in the current workspace by name substring, optionally filtered by project, state group, and priority. Excludes triage items, issues with a missing or deleted state, and issues in deleted or archived projects. `returned` is the number of rows returned (at most limit), not the total match count. Returns project, identifier, name, state, and priority.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<SearchWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let project = optional_text(args.project.as_deref());
        let state_group = state_group_arg(args.state_group.as_deref())?;
        let priority = priority_arg(args.priority.as_deref())?;
        let limit = clamp_limit(args.limit);
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(SEARCH_SQL)
            .bind(self.workspace_id)
            .bind(&query)
            .bind(&project)
            .bind(&state_group)
            .bind(&priority)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(search_json(&rows))
    }
}

/// Build the production tool server: three read-only tools scoped to one
/// workspace, all sharing the caller's trace handle.
pub fn workspace_tools(
    pool: PgPool,
    workspace_id: Uuid,
    trace: ToolTrace,
) -> rig::tool::server::ToolServerHandle {
    rig::tool::server::ToolServer::new()
        .tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(SearchWorkItems {
            pool,
            workspace_id,
            trace,
        })
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sql_constants_are_workspace_scoped() {
        for sql in [PROJECTS_SQL, COUNT_SQL, SEARCH_SQL] {
            assert!(
                sql.contains("workspace_id = $1"),
                "missing workspace scope: {sql}"
            );
        }
    }

    #[test]
    fn sql_joins_visible_states_and_excludes_triage() {
        for sql in [COUNT_SQL, SEARCH_SQL] {
            assert!(sql.contains("JOIN states s ON s.id = i.state_id"));
            assert!(sql.contains("s.deleted_at IS NULL"));
            assert!(sql.contains("s.\"group\" <> 'triage'"));
            assert!(!sql.contains("LEFT JOIN"));
        }
    }

    #[test]
    fn state_group_allowlist() {
        assert_eq!(state_group_arg(None).unwrap(), None);
        assert_eq!(state_group_arg(Some("  ")).unwrap(), None);
        assert_eq!(
            state_group_arg(Some("Started")).unwrap(),
            Some("started".to_string())
        );
        let err = state_group_arg(Some("nope")).unwrap_err();
        assert!(err.to_string().contains("backlog"));
    }

    #[test]
    fn priority_allowlist() {
        assert_eq!(priority_arg(None).unwrap(), None);
        assert_eq!(
            priority_arg(Some("URGENT")).unwrap(),
            Some("urgent".to_string())
        );
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

    fn lazy_pool() -> PgPool {
        sqlx::PgPool::connect_lazy("postgres://user:pass@127.0.0.1:1/plane").expect("lazy pool")
    }

    #[tokio::test]
    async fn tool_metadata_is_exposed() {
        let pool = lazy_pool();
        let trace = crate::agent::new_trace();

        let list = ListProjects {
            pool: pool.clone(),
            workspace_id: Uuid::nil(),
            trace: trace.clone(),
        };
        assert_eq!(ListProjects::NAME, "list_projects");
        assert!(!list.description().is_empty());
        assert_eq!(list.parameters()["type"], json!("object"));

        let count = CountWorkItems {
            pool: pool.clone(),
            workspace_id: Uuid::nil(),
            trace: trace.clone(),
        };
        assert_eq!(CountWorkItems::NAME, "count_work_items");
        let count_params = count.parameters();
        assert!(count_params["properties"]["project"].is_object());
        assert!(count_params["properties"]["state_group"].is_object());
        assert!(count_params["properties"]["priority"].is_object());
        assert!(count_params["properties"]["include_archived"].is_object());

        let search = SearchWorkItems {
            pool,
            workspace_id: Uuid::nil(),
            trace,
        };
        assert_eq!(SearchWorkItems::NAME, "search_work_items");
        let search_params = search.parameters();
        assert!(search_params["properties"]["query"].is_object());
        assert!(search_params["properties"]["limit"].is_object());
        assert!(search_params["properties"]["project"].is_object());
        assert!(!search.description().is_empty());
    }

    #[tokio::test]
    async fn workspace_tools_builds_a_server_handle() {
        let _handle = workspace_tools(lazy_pool(), Uuid::nil(), crate::agent::new_trace());
    }

    #[tokio::test]
    async fn invalid_allowlist_values_surface_before_any_query() {
        let tool = CountWorkItems {
            pool: lazy_pool(),
            workspace_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CountWorkItemsArgs {
                    project: None,
                    state_group: Some("nope".to_string()),
                    priority: None,
                    include_archived: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "state_group must be one of: backlog, unstarted, started, completed, cancelled"
        );
        let recorded = tool.trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "count_work_items");
    }
}
