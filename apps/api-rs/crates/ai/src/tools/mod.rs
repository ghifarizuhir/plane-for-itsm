//! Shared tool infrastructure: constants, validation helpers, project/work
//! item resolvers, and the tool registry. Domain tools live in sibling
//! modules and are re-exported here so `crate::tools::<Item>` keeps working.

pub mod projects;
pub mod proposals;
pub mod work_items;

pub use projects::*;
pub use proposals::*;
pub use work_items::*;

use rig::tool::{Tool, ToolExecutionError};
use schemars::JsonSchema;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent::ToolTrace;

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

pub fn db_error(error: sqlx::Error) -> ToolExecutionError {
    tracing::warn!(error = %error, "ai-agent: tool query failed");
    ToolExecutionError::from_error(error)
}

pub fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| json!({"type": "object", "properties": {}}))
}

/// Project reference parsed from a work item identifier like "LTS-42".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItemRef {
    pub project: String,
    pub sequence_id: i64,
}

pub fn parse_work_item_ref(reference: &str) -> Result<WorkItemRef, ToolExecutionError> {
    let trimmed = reference.trim();
    let Some((project, sequence)) = trimmed.rsplit_once('-') else {
        return Err(ToolExecutionError::invalid_args(
            "work_item must look like PROJ-123",
        ));
    };
    let project = project.trim();
    if project.is_empty() || !project.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ToolExecutionError::invalid_args(
            "work_item must look like PROJ-123",
        ));
    }
    let sequence_id: i64 = sequence
        .trim()
        .parse()
        .map_err(|_| ToolExecutionError::invalid_args("work_item must look like PROJ-123"))?;
    if sequence_id <= 0 {
        return Err(ToolExecutionError::invalid_args(
            "work_item must look like PROJ-123",
        ));
    }
    Ok(WorkItemRef {
        project: project.to_string(),
        sequence_id,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: Uuid,
    pub identifier: String,
    pub name: String,
}

pub const RESOLVE_PROJECTS_SQL: &str = "SELECT p.id, p.identifier, p.name FROM projects p \
     WHERE p.workspace_id = $1 AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.member_id = $2 \
       AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(p.identifier) = lower($3) OR lower(p.name) = lower($3) \
       OR p.name ILIKE '%' || $3 || '%') \
     ORDER BY p.name LIMIT 10";

/// Pick one project: identifier exact, then name exact, then a unique name
/// substring. Anything else is a model-visible error; never guess.
pub fn pick_project(
    rows: &[ProjectRow],
    reference: &str,
) -> Result<ProjectRow, ToolExecutionError> {
    let needle = reference.trim().to_ascii_lowercase();
    if needle.is_empty() || rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "project '{reference}' was not found or is not accessible"
        )));
    }
    if let Some(exact) = rows
        .iter()
        .find(|row| row.identifier.to_ascii_lowercase() == needle)
    {
        return Ok(exact.clone());
    }
    let exact_names: Vec<&ProjectRow> = rows
        .iter()
        .filter(|row| row.name.to_ascii_lowercase() == needle)
        .collect();
    if exact_names.len() == 1 {
        return Ok(exact_names[0].clone());
    }
    if exact_names.len() > 1 {
        return Err(ambiguous_project_error(rows, reference));
    }
    let matching: Vec<&ProjectRow> = rows
        .iter()
        .filter(|row| {
            row.identifier.to_ascii_lowercase().contains(&needle)
                || row.name.to_ascii_lowercase().contains(&needle)
        })
        .collect();
    if matching.len() == 1 {
        return Ok(matching[0].clone());
    }
    if matching.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "project '{reference}' was not found or is not accessible"
        )));
    }
    Err(ambiguous_project_error(rows, reference))
}

fn ambiguous_project_error(rows: &[ProjectRow], reference: &str) -> ToolExecutionError {
    let identifiers = rows
        .iter()
        .map(|row| row.identifier.clone())
        .collect::<Vec<_>>()
        .join(", ");
    ToolExecutionError::invalid_args(format!(
        "project '{reference}' is ambiguous; use one identifier: {identifiers}"
    ))
}

pub async fn resolve_project(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
) -> Result<ProjectRow, ToolExecutionError> {
    let rows: Vec<ProjectRow> = sqlx::query_as(RESOLVE_PROJECTS_SQL)
        .bind(workspace_id)
        .bind(user_id)
        .bind(reference.trim())
        .fetch_all(pool)
        .await
        .map_err(db_error)?;
    pick_project(&rows, reference)
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ResolvedWorkItem {
    pub id: Uuid,
    pub project_id: Uuid,
    pub identifier: String,
}

pub const RESOLVE_WORK_ITEM_SQL: &str = "SELECT i.id, i.project_id, \
     p.identifier || '-' || i.sequence_id AS identifier \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 AND p.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND p.identifier ILIKE $2 AND i.sequence_id = $3 \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.member_id = $4 \
       AND pm.is_active = true AND pm.deleted_at IS NULL) \
     LIMIT 1";

pub async fn resolve_work_item(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
) -> Result<ResolvedWorkItem, ToolExecutionError> {
    let parsed = parse_work_item_ref(reference)?;
    let row: Option<ResolvedWorkItem> = sqlx::query_as(RESOLVE_WORK_ITEM_SQL)
        .bind(workspace_id)
        .bind(&parsed.project)
        .bind(parsed.sequence_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?;
    row.ok_or_else(|| {
        ToolExecutionError::invalid_args(format!(
            "work item '{reference}' was not found or is not accessible"
        ))
    })
}

/// Build the production tool server: three read-only tools plus the
/// `create_schedule` and `create_work_item` proposal tools, all scoped to one
/// workspace and sharing the caller's trace handle.
pub fn workspace_tools(
    pool: PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
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
            user_id,
            trace: trace.clone(),
        })
        .tool(SearchWorkItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetWorkItem {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListWorkItemComments {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListWorkItemRelations {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(CreateSchedule {
            trace: trace.clone(),
        })
        .tool(CreateWorkItem {
            trace: trace.clone(),
        })
        .run()
}

/// Build the schedule-run tool server: only the allowed read tools, never
/// `create_schedule`. Unknown names are ignored (specs are validated before
/// this is called).
pub fn read_tools(
    pool: PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    trace: ToolTrace,
    allowed: &[&str],
) -> rig::tool::server::ToolServerHandle {
    let mut server = rig::tool::server::ToolServer::new();
    if allowed.contains(&ListProjects::NAME) {
        server = server.tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&CountWorkItems::NAME) {
        server = server.tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&SearchWorkItems::NAME) {
        server = server.tool(SearchWorkItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&GetWorkItem::NAME) {
        server = server.tool(GetWorkItem {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListWorkItemComments::NAME) {
        server = server.tool(ListWorkItemComments {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListWorkItemRelations::NAME) {
        server = server.tool(ListWorkItemRelations {
            pool,
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    server.run()
}

#[cfg(test)]
pub(crate) fn lazy_pool() -> PgPool {
    sqlx::PgPool::connect_lazy("postgres://user:pass@127.0.0.1:1/plane").expect("lazy pool")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::Tool;
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
        let count = count_json(
            7,
            &WorkItemFilters {
                project: Some("LTS".to_string()),
                state_group: Some("started".to_string()),
                priority: Some("urgent".to_string()),
                ..Default::default()
            },
            false,
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&count).unwrap(),
            json!({
                "count": 7,
                "filters": {
                    "project": "LTS",
                    "state_group": "started",
                    "priority": "urgent",
                    "include_archived": false,
                    "type": null,
                    "service": null,
                    "assignee": null,
                    "sprint": null,
                    "track": null,
                    "label": null
                }
            })
        );
        let search = search_json(&[SearchRow {
            project: "LTS".to_string(),
            identifier: "LTS-12".to_string(),
            name: "Fix pump".to_string(),
            state: "In Progress".to_string(),
            priority: "urgent".to_string(),
            work_item_type: "Incident".to_string(),
            assignees: "Budi, Sari".to_string(),
        }]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&search).unwrap(),
            json!({"returned": 1, "items": [{
                "project": "LTS",
                "identifier": "LTS-12",
                "name": "Fix pump",
                "state": "In Progress",
                "priority": "urgent",
                "type": "Incident",
                "assignees": "Budi, Sari"
            }]})
        );
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
            user_id: Uuid::nil(),
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
            user_id: Uuid::nil(),
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
        let _handle = workspace_tools(lazy_pool(), Uuid::nil(), Uuid::nil(), crate::agent::new_trace());
    }

    #[tokio::test]
    async fn invalid_allowlist_values_surface_before_any_query() {
        let tool = CountWorkItems {
            pool: lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
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
                    work_item_type: None,
                    service: None,
                    assignee: None,
                    sprint: None,
                    track: None,
                    label: None,
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

    #[test]
    fn create_schedule_recipe_normalizes_defaults() {
        let recipe = recipe_from_args(CreateScheduleArgs {
            name: " Daily overdue ".to_string(),
            description: " Summarize overdue work ".to_string(),
            how_to: vec![" Count overdue items ".to_string()],
            tools: vec!["count_work_items".to_string()],
            expected_output: " A short list ".to_string(),
            frequency: "weekly".to_string(),
            time: None,
            day_of_week: Some(1),
            day_of_month: None,
            timezone: Some("Asia/Jakarta".to_string()),
        })
        .expect("valid args");
        assert_eq!(recipe.proposal.name, "Daily overdue");
        assert_eq!(recipe.spec.description, "Summarize overdue work");
        assert_eq!(recipe.proposal.time, "09:00");
        assert_eq!(recipe.proposal.timezone, "Asia/Jakarta");
        assert_eq!(recipe.proposal.day_of_week, Some(1));
        assert_eq!(recipe.spec.tools, vec!["count_work_items".to_string()]);
    }

    #[test]
    fn create_schedule_recipe_rejects_bad_args() {
        let err = recipe_from_args(CreateScheduleArgs {
            name: "x".to_string(),
            description: "d".to_string(),
            how_to: vec!["s".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "o".to_string(),
            frequency: "sometimes".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: None,
        })
        .unwrap_err();
        assert!(err.to_string().contains("frequency"));
    }

    #[tokio::test]
    async fn create_schedule_tool_records_recipe() {
        let trace = crate::agent::new_trace();
        let tool = CreateSchedule {
            trace: trace.clone(),
        };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateScheduleArgs {
                    name: "Daily".to_string(),
                    description: "Report".to_string(),
                    how_to: vec!["Count overdue".to_string()],
                    tools: vec!["count_work_items".to_string()],
                    expected_output: "Summary".to_string(),
                    frequency: "daily".to_string(),
                    time: Some("08:00".to_string()),
                    day_of_week: None,
                    day_of_month: None,
                    timezone: Some("UTC".to_string()),
                },
            )
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("recipe json");
        assert_eq!(parsed["frequency"], json!("daily"));
        assert_eq!(parsed["time"], json!("08:00"));
        assert_eq!(parsed["tools"][0], json!("count_work_items"));
        assert_eq!(parsed["how_to"][0], json!("Count overdue"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "create_schedule");
        assert_eq!(recorded[0].arguments["time"], json!("08:00"));
        assert_eq!(recorded[0].arguments["description"], json!("Report"));
    }

    #[test]
    fn create_schedule_recipe_hourly_default_and_rejections() {
        let hourly = recipe_from_args(CreateScheduleArgs {
            name: "Hourly".to_string(),
            description: "Check".to_string(),
            how_to: vec!["Check".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "Notes".to_string(),
            frequency: "hourly".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: None,
        })
        .expect("valid args");
        assert_eq!(hourly.proposal.time, "00:00");
        assert_eq!(hourly.proposal.timezone, "UTC");

        let bad_tz = recipe_from_args(CreateScheduleArgs {
            name: "x".to_string(),
            description: "d".to_string(),
            how_to: vec!["s".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "o".to_string(),
            frequency: "daily".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: Some("Mars/Olympus".to_string()),
        });
        assert!(bad_tz.is_err());

        let monthly_without_day = recipe_from_args(CreateScheduleArgs {
            name: "x".to_string(),
            description: "d".to_string(),
            how_to: vec!["s".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "o".to_string(),
            frequency: "monthly".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: None,
        });
        assert!(monthly_without_day.is_err());
    }

    #[tokio::test]
    async fn rejected_create_schedule_is_not_traced() {
        let trace = crate::agent::new_trace();
        let tool = CreateSchedule {
            trace: trace.clone(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateScheduleArgs {
                    name: "x".to_string(),
                    description: "d".to_string(),
                    how_to: vec!["s".to_string()],
                    tools: vec![],
                    expected_output: "o".to_string(),
                    frequency: "daily".to_string(),
                    time: None,
                    day_of_week: None,
                    day_of_month: None,
                    timezone: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("tools"));
        assert!(trace.lock().unwrap().is_empty());
        assert!(crate::agent::pending_action(&trace).is_none());
    }

    #[tokio::test]
    async fn read_tools_accepts_a_subset() {
        let _handle = read_tools(
            lazy_pool(),
            Uuid::nil(),
            Uuid::nil(),
            crate::agent::new_trace(),
            &["list_projects"],
        );
    }

    fn base_work_item_args() -> CreateWorkItemArgs {
        CreateWorkItemArgs {
            project: "LTS".to_string(),
            name: "Fix pump".to_string(),
            description: None,
            priority: None,
            state: None,
            assignees: None,
            labels: None,
            start_date: None,
            target_date: None,
        }
    }

    #[test]
    fn create_work_item_normalizes_and_dedupes() {
        let proposal = work_item_proposal_from_args(CreateWorkItemArgs {
            project: " LTS ".to_string(),
            name: " Fix pump ".to_string(),
            description: Some(" Pump is noisy ".to_string()),
            priority: Some("URGENT".to_string()),
            state: Some(" In Progress ".to_string()),
            assignees: Some(vec![
                "Budi".to_string(),
                " budi ".to_string(),
                "Sari".to_string(),
            ]),
            labels: Some(vec!["maintenance".to_string()]),
            start_date: Some("2026-10-01".to_string()),
            target_date: Some("2026-10-05".to_string()),
        })
        .expect("valid args");
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Fix pump");
        assert_eq!(proposal.description.as_deref(), Some("Pump is noisy"));
        assert_eq!(proposal.priority.as_deref(), Some("urgent"));
        assert_eq!(proposal.state.as_deref(), Some("In Progress"));
        assert_eq!(
            proposal.assignees,
            vec!["Budi".to_string(), "Sari".to_string()]
        );
        assert_eq!(proposal.labels, vec!["maintenance".to_string()]);
        assert_eq!(proposal.start_date.as_deref(), Some("2026-10-01"));
        assert_eq!(proposal.target_date.as_deref(), Some("2026-10-05"));
    }

    #[test]
    fn create_work_item_rejects_bad_args() {
        let empty_project = work_item_proposal_from_args(CreateWorkItemArgs {
            project: "  ".to_string(),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(empty_project.to_string().contains("project"));

        let empty_name = work_item_proposal_from_args(CreateWorkItemArgs {
            name: "  ".to_string(),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(empty_name.to_string().contains("name"));

        let long_name = work_item_proposal_from_args(CreateWorkItemArgs {
            name: "x".repeat(256),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(long_name.to_string().contains("255"));

        let bad_priority = work_item_proposal_from_args(CreateWorkItemArgs {
            priority: Some("p0".to_string()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(bad_priority.to_string().contains("priority"));

        let bad_date = work_item_proposal_from_args(CreateWorkItemArgs {
            start_date: Some("01-10-2026".to_string()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(bad_date.to_string().contains("start_date"));

        let reversed = work_item_proposal_from_args(CreateWorkItemArgs {
            start_date: Some("2026-10-05".to_string()),
            target_date: Some("2026-10-01".to_string()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));

        let too_many = work_item_proposal_from_args(CreateWorkItemArgs {
            assignees: Some((0..11).map(|index| format!("user-{index}")).collect()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("10"));

        let empty_ref = work_item_proposal_from_args(CreateWorkItemArgs {
            labels: Some(vec!["  ".to_string()]),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(empty_ref.to_string().contains("labels"));
    }

    #[tokio::test]
    async fn create_work_item_tool_records_proposal() {
        let trace = crate::agent::new_trace();
        let tool = CreateWorkItem {
            trace: trace.clone(),
        };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateWorkItemArgs {
                    project: "LTS".to_string(),
                    name: "Fix pump".to_string(),
                    priority: Some("urgent".to_string()),
                    ..base_work_item_args()
                },
            )
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("proposal json");
        assert_eq!(parsed["project"], json!("LTS"));
        assert_eq!(parsed["name"], json!("Fix pump"));
        assert_eq!(parsed["priority"], json!("urgent"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "create_work_item");
        assert_eq!(recorded[0].arguments["name"], json!("Fix pump"));
    }

    #[tokio::test]
    async fn rejected_create_work_item_is_not_traced() {
        let trace = crate::agent::new_trace();
        let tool = CreateWorkItem {
            trace: trace.clone(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateWorkItemArgs {
                    name: "  ".to_string(),
                    ..base_work_item_args()
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("name"));
        assert!(trace.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn workspace_tools_builds_a_server_handle_with_create_work_item() {
        let _handle = workspace_tools(lazy_pool(), Uuid::nil(), Uuid::nil(), crate::agent::new_trace());
    }

    #[tokio::test]
    async fn new_work_item_tools_expose_metadata() {
        let pool = lazy_pool();
        let trace = crate::agent::new_trace();

        let get = GetWorkItem {
            pool: pool.clone(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: trace.clone(),
        };
        assert_eq!(GetWorkItem::NAME, "get_work_item");
        assert_eq!(get.parameters()["type"], json!("object"));
        assert!(get.parameters()["properties"]["work_item"].is_object());
        assert!(!get.description().is_empty());

        let comments = ListWorkItemComments {
            pool: pool.clone(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: trace.clone(),
        };
        assert_eq!(ListWorkItemComments::NAME, "list_work_item_comments");
        assert!(comments.parameters()["properties"]["limit"].is_object());

        let relations = ListWorkItemRelations {
            pool,
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace,
        };
        assert_eq!(ListWorkItemRelations::NAME, "list_work_item_relations");
        assert!(!relations.description().is_empty());
    }

    #[test]
    fn parse_work_item_ref_rules() {
        assert_eq!(
            parse_work_item_ref("LTS-42").unwrap(),
            WorkItemRef {
                project: "LTS".to_string(),
                sequence_id: 42
            }
        );
        assert_eq!(parse_work_item_ref(" lts-7 ").unwrap().project, "lts");
        for bad in ["LTS", "LTS-0", "LTS--1", "LTS-abc", "-5"] {
            assert!(parse_work_item_ref(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn pick_project_prefers_identifier_then_exact_name_then_unique_substring() {
        let rows = vec![
            ProjectRow { id: Uuid::from_u128(1), identifier: "LTS".into(), name: "Logistics".into() },
            ProjectRow { id: Uuid::from_u128(2), identifier: "OPS".into(), name: "Operations".into() },
        ];
        assert_eq!(pick_project(&rows, "lts").unwrap().id, Uuid::from_u128(1));
        assert_eq!(pick_project(&rows, "logistics").unwrap().id, Uuid::from_u128(1));
        assert_eq!(pick_project(&rows, "oper").unwrap().id, Uuid::from_u128(2));
        assert!(pick_project(&rows, "o").is_err());
        assert!(pick_project(&[], "lts").is_err());
    }

    #[test]
    fn resolver_sql_is_workspace_scoped_and_requires_membership() {
        for sql in [RESOLVE_PROJECTS_SQL, RESOLVE_WORK_ITEM_SQL] {
            assert!(sql.contains("workspace_id = $1"));
            assert!(sql.contains("project_members"));
            assert!(sql.contains("is_active = true"));
        }
    }
}
