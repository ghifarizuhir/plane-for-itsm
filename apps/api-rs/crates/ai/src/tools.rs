//! Workspace-scoped tools for the Rig agent: three read-only queries plus the
//! `create_schedule` proposal tool.
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
use crate::schedule::ScheduleRecipe;

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

pub const CREATE_SCHEDULE_NAME: &str = "create_schedule";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateScheduleArgs {
    /// Short human-readable schedule name (1-120 characters), e.g. "Daily overdue report".
    pub name: String,
    /// What the schedule is for: one or two sentences of context (1-500 characters).
    pub description: String,
    /// Ordered, concrete steps the agent must follow on every fire (1-10 steps, each 1-500 characters).
    pub how_to: Vec<String>,
    /// Tools the run may use: at least one of list_projects, count_work_items, search_work_items.
    pub tools: Vec<String>,
    /// What the result should contain, e.g. "a markdown table of overdue items with owner and due date" (1-1000 characters).
    pub expected_output: String,
    /// One of: hourly, daily, weekly, monthly.
    pub frequency: String,
    /// Time of day "HH:MM" (24h). For hourly only the minutes are used. Defaults to 09:00 (00:00 for hourly).
    pub time: Option<String>,
    /// For weekly schedules: 1 = Monday … 7 = Sunday.
    pub day_of_week: Option<i16>,
    /// For monthly schedules: day of month, 1-31. Short months clamp to the last day.
    pub day_of_month: Option<i16>,
    /// IANA timezone, e.g. "Asia/Jakarta". Defaults to UTC when omitted; unknown zones are rejected.
    pub timezone: Option<String>,
}

/// Validate raw tool args into a normalized recipe (defaults applied, prompt rendered).
pub fn recipe_from_args(args: CreateScheduleArgs) -> Result<ScheduleRecipe, ToolExecutionError> {
    ScheduleRecipe::new(
        &args.name,
        &args.description,
        &args.how_to,
        &args.tools,
        &args.expected_output,
        &args.frequency,
        args.time.as_deref(),
        args.day_of_week,
        args.day_of_month,
        args.timezone.as_deref(),
    )
    .map_err(ToolExecutionError::invalid_args)
}

pub struct CreateSchedule {
    pub trace: ToolTrace,
}

impl Tool for CreateSchedule {
    const NAME: &'static str = CREATE_SCHEDULE_NAME;
    type Args = CreateScheduleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose a recurring scheduled task for this workspace. Only call this when the user explicitly asks for a recurring or scheduled task (for example a message starting with /schedule), and only after the recipe is complete: description, ordered how_to steps, the tools it needs (at least one of list_projects, count_work_items, search_work_items), expected_output, and the frequency. The user must confirm and may edit every field in the UI before anything is saved. Never claim the schedule exists until they confirm.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateScheduleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let recipe = recipe_from_args(args)?;
        record(&self.trace, Self::NAME, &recipe);
        Ok(serde_json::to_string(&recipe).expect("ScheduleRecipe serializes"))
    }
}

pub const CREATE_WORK_ITEM_NAME: &str = "create_work_item";

pub const WORK_ITEM_PROJECT_MAX: usize = 100;
pub const WORK_ITEM_NAME_MAX: usize = 255;
pub const WORK_ITEM_DESCRIPTION_MAX: usize = 5000;
pub const WORK_ITEM_STATE_MAX: usize = 100;
pub const WORK_ITEM_REFS_MAX: usize = 10;
pub const WORK_ITEM_REF_MAX: usize = 100;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateWorkItemArgs {
    /// Project identifier (e.g. "LTS") or project name the work item belongs to. Required.
    pub project: String,
    /// Short work item title (1-255 characters).
    pub name: String,
    /// Plain-text description or markdown (max 5000 characters).
    pub description: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// State name to start in, e.g. "In Progress". Omitted means the project default.
    pub state: Option<String>,
    /// Assignee display names or emails (max 10).
    pub assignees: Option<Vec<String>>,
    /// Label names (max 10).
    pub labels: Option<Vec<String>>,
    /// Start date "YYYY-MM-DD".
    pub start_date: Option<String>,
    /// Target date "YYYY-MM-DD"; must not be before start_date.
    pub target_date: Option<String>,
}

/// Normalized proposal recorded in the trace and rendered as a confirmation card.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkItemProposal {
    pub project: String,
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub state: Option<String>,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
}

fn bounded_ref_list(
    values: Option<Vec<String>>,
    label: &str,
) -> Result<Vec<String>, ToolExecutionError> {
    let values = values.unwrap_or_default();
    if values.len() > WORK_ITEM_REFS_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "at most {WORK_ITEM_REFS_MAX} {label} are allowed"
        )));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ToolExecutionError::invalid_args(format!(
                "{label} entries must not be empty"
            )));
        }
        if trimmed.chars().count() > WORK_ITEM_REF_MAX {
            return Err(ToolExecutionError::invalid_args(format!(
                "{label} entries must be at most {WORK_ITEM_REF_MAX} characters"
            )));
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(trimmed.to_string());
    }
    Ok(out)
}

fn parse_iso_date(value: &str, label: &str) -> Result<chrono::NaiveDate, ToolExecutionError> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| ToolExecutionError::invalid_args(format!("{label} must be YYYY-MM-DD")))
}

/// Validate raw tool args into a normalized work item proposal. Human-readable
/// names (state, assignees, labels) are kept as text; the UI resolves them.
pub fn work_item_proposal_from_args(
    args: CreateWorkItemArgs,
) -> Result<WorkItemProposal, ToolExecutionError> {
    let project = args.project.trim();
    if project.is_empty() {
        return Err(ToolExecutionError::invalid_args("project is required"));
    }
    if project.chars().count() > WORK_ITEM_PROJECT_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "project must be at most {WORK_ITEM_PROJECT_MAX} characters"
        )));
    }
    let name = args.name.trim();
    if name.is_empty() {
        return Err(ToolExecutionError::invalid_args("name is required"));
    }
    if name.chars().count() > WORK_ITEM_NAME_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "name must be at most {WORK_ITEM_NAME_MAX} characters"
        )));
    }
    let description = optional_text(args.description.as_deref());
    if let Some(description) = description.as_ref() {
        if description.chars().count() > WORK_ITEM_DESCRIPTION_MAX {
            return Err(ToolExecutionError::invalid_args(format!(
                "description must be at most {WORK_ITEM_DESCRIPTION_MAX} characters"
            )));
        }
    }
    let priority = priority_arg(args.priority.as_deref())?;
    let state = optional_text(args.state.as_deref());
    if let Some(state) = state.as_ref() {
        if state.chars().count() > WORK_ITEM_STATE_MAX {
            return Err(ToolExecutionError::invalid_args(format!(
                "state must be at most {WORK_ITEM_STATE_MAX} characters"
            )));
        }
    }
    let assignees = bounded_ref_list(args.assignees, "assignees")?;
    let labels = bounded_ref_list(args.labels, "labels")?;
    let start_date = args
        .start_date
        .as_deref()
        .map(|value| parse_iso_date(value, "start_date"))
        .transpose()?;
    let target_date = args
        .target_date
        .as_deref()
        .map(|value| parse_iso_date(value, "target_date"))
        .transpose()?;
    if let (Some(start_date), Some(target_date)) = (start_date, target_date) {
        if start_date > target_date {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    Ok(WorkItemProposal {
        project: project.to_string(),
        name: name.to_string(),
        description,
        priority,
        state,
        assignees,
        labels,
        start_date: start_date.map(|date| date.to_string()),
        target_date: target_date.map(|date| date.to_string()),
    })
}

pub struct CreateWorkItem {
    pub trace: ToolTrace,
}

impl Tool for CreateWorkItem {
    const NAME: &'static str = CREATE_WORK_ITEM_NAME;
    type Args = CreateWorkItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one work item in a named project for this workspace. \
         Only call this when the user clearly asks to create a work item or task \
         (natural language or a message starting with /task), and only after the \
         project is known: never guess the project, ask when it is missing or \
         ambiguous. Fill what you can from the conversation (title, description, \
         priority, state, assignee names or emails, label names, dates) and leave \
         the rest out. The user must confirm and may edit every field in the UI \
         before anything is saved. Never claim the work item exists until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateWorkItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = work_item_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("WorkItemProposal serializes"))
    }
}

/// Build the production tool server: three read-only tools plus the
/// `create_schedule` and `create_work_item` proposal tools, all scoped to one
/// workspace and sharing the caller's trace handle.
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
            trace: trace.clone(),
        });
    }
    if allowed.contains(&SearchWorkItems::NAME) {
        server = server.tool(SearchWorkItems {
            pool,
            workspace_id,
            trace: trace.clone(),
        });
    }
    server.run()
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
        let _handle = workspace_tools(lazy_pool(), Uuid::nil(), crate::agent::new_trace());
    }
}
