//! Work item read tools: count, search, detail, comments, relations.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, optional_text, priority_arg, resolve_work_item, schema_of,
    state_group_arg,
};
use crate::agent::{record, ToolTrace};

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

/// Normalized filter set shared by `search_work_items` and `count_work_items`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkItemFilters {
    pub query: Option<String>,
    pub project: Option<String>,
    pub state_group: Option<String>,
    pub priority: Option<String>,
    pub work_item_type: Option<String>,
    pub service: Option<String>,
    pub assignee: Option<String>,
    pub assignee_me: bool,
    pub sprint: Option<String>,
    pub track: Option<String>,
    pub label: Option<String>,
}

impl WorkItemFilters {
    #[allow(clippy::too_many_arguments)]
    pub fn normalize(
        query: Option<&str>,
        project: Option<&str>,
        state_group: Option<&str>,
        priority: Option<&str>,
        work_item_type: Option<&str>,
        service: Option<&str>,
        assignee: Option<&str>,
        sprint: Option<&str>,
        track: Option<&str>,
        label: Option<&str>,
    ) -> Result<Self, ToolExecutionError> {
        let assignee = optional_text(assignee);
        let (assignee, assignee_me) = match assignee {
            Some(value) if value.eq_ignore_ascii_case("me") => (None, true),
            other => (other, false),
        };
        Ok(Self {
            query: optional_text(query),
            project: optional_text(project),
            state_group: state_group_arg(state_group)?,
            priority: priority_arg(priority)?,
            work_item_type: optional_text(work_item_type),
            service: optional_text(service),
            assignee,
            assignee_me,
            sprint: optional_text(sprint),
            track: optional_text(track),
            label: optional_text(label),
        })
    }

    /// Echo of the assignee filter for tool output: "me", a name, or nothing.
    pub fn assignee_label(&self) -> Option<String> {
        if self.assignee_me {
            Some("me".to_string())
        } else {
            self.assignee.clone()
        }
    }
}

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

pub const DESCRIPTION_MAX: usize = 4000;
pub const RECENT_COMMENTS_LIMIT: i64 = 5;

/// Char-bounded truncation with an ASCII ellipsis so tool output stays small.
pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WorkItemDetailRow {
    pub project: String,
    pub identifier: String,
    pub name: String,
    pub description: String,
    pub state: String,
    pub state_group: String,
    pub priority: String,
    pub work_item_type: String,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub sub_issues: i64,
    pub comment_count: i64,
    pub relation_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct CommentView {
    pub author: String,
    pub at: String,
    pub text: String,
}

pub const WORK_ITEM_DETAIL_SQL: &str = "SELECT \
     p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, \
     i.name, \
     COALESCE(i.description_stripped, '') AS description, \
     COALESCE(s.name, '') AS state, \
     COALESCE(s.\"group\", '') AS state_group, \
     i.priority, \
     COALESCE(t.name, '') AS work_item_type, \
     i.start_date::text AS start_date, \
     i.target_date::text AS target_date, \
     i.completed_at::text AS completed_at, \
     i.created_at::text AS created_at, \
     i.updated_at::text AS updated_at, \
     (SELECT count(*) FROM issues c WHERE c.parent_id = i.id AND c.deleted_at IS NULL)::int8 \
       AS sub_issues, \
     (SELECT count(*) FROM issue_comments cm WHERE cm.issue_id = i.id AND cm.deleted_at IS NULL)::int8 \
       AS comment_count, \
     (SELECT count(*) FROM issue_relations r \
       WHERE (r.issue_id = i.id OR r.related_issue_id = i.id) AND r.deleted_at IS NULL)::int8 \
       AS relation_count \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     LEFT JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
     WHERE i.id = $1 AND i.workspace_id = $2 AND i.deleted_at IS NULL";

pub const WORK_ITEM_ASSIGNEES_SQL: &str =
    "SELECT COALESCE(u.display_name, u.email, 'unknown') FROM issue_assignees ia \
     JOIN users u ON u.id = ia.assignee_id \
     WHERE ia.issue_id = $1 AND ia.deleted_at IS NULL ORDER BY 1";

pub const WORK_ITEM_LABELS_SQL: &str =
    "SELECT l.name FROM issue_labels il \
     JOIN labels l ON l.id = il.label_id AND l.deleted_at IS NULL \
     WHERE il.issue_id = $1 AND il.deleted_at IS NULL ORDER BY l.name";

pub const WORK_ITEM_SPRINT_SQL: &str =
    "SELECT c.name FROM cycle_issues ci \
     JOIN cycles c ON c.id = ci.cycle_id AND c.deleted_at IS NULL \
     WHERE ci.issue_id = $1 AND ci.deleted_at IS NULL \
     ORDER BY ci.created_at DESC, ci.id DESC LIMIT 1";

pub const WORK_ITEM_TRACKS_SQL: &str =
    "SELECT m.name FROM module_issues mi \
     JOIN modules m ON m.id = mi.module_id AND m.deleted_at IS NULL \
     WHERE mi.issue_id = $1 AND mi.deleted_at IS NULL ORDER BY m.name";

pub const WORK_ITEM_SERVICES_SQL: &str =
    "SELECT sv.name FROM service_issues si \
     JOIN services sv ON sv.id = si.service_id AND sv.deleted_at IS NULL \
     WHERE si.issue_id = $1 AND si.deleted_at IS NULL ORDER BY sv.name";

pub const WORK_ITEM_RECENT_COMMENTS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS author, \
     cm.created_at::text AS at, \
     left(cm.comment_stripped, 500) AS text \
     FROM issue_comments cm \
     LEFT JOIN users u ON u.id = COALESCE(cm.actor_id, cm.created_by_id) \
     WHERE cm.issue_id = $1 AND cm.deleted_at IS NULL \
     ORDER BY cm.created_at DESC, cm.id DESC LIMIT $2";

#[allow(clippy::too_many_arguments)]
pub fn work_item_detail_json(
    row: &WorkItemDetailRow,
    assignees: &[String],
    labels: &[String],
    sprint: Option<&str>,
    tracks: &[String],
    services: &[String],
    comments: &[CommentView],
) -> String {
    json!({
        "project": row.project,
        "identifier": row.identifier,
        "name": row.name,
        "description": truncate_chars(&row.description, DESCRIPTION_MAX),
        "state": row.state,
        "state_group": row.state_group,
        "priority": row.priority,
        "type": row.work_item_type,
        "assignees": assignees,
        "labels": labels,
        "sprint": sprint,
        "tracks": tracks,
        "services": services,
        "start_date": row.start_date,
        "target_date": row.target_date,
        "completed_at": row.completed_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "sub_issues": row.sub_issues,
        "comment_count": row.comment_count,
        "relation_count": row.relation_count,
        "recent_comments": comments,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetWorkItemArgs {
    /// Work item identifier, e.g. "LTS-42".
    pub work_item: String,
}

pub struct GetWorkItem {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetWorkItem {
    const NAME: &'static str = "get_work_item";
    type Args = GetWorkItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one work item by identifier (e.g. LTS-42): name, description \
         (truncated), state and state group, priority, type, assignees, labels, \
         sprint, tracks, services, dates, counts, and the 5 most recent comments. \
         Use this before answering questions about a specific work item."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetWorkItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let resolved =
            resolve_work_item(&self.pool, self.workspace_id, self.user_id, &args.work_item).await?;
        let row: WorkItemDetailRow = sqlx::query_as(WORK_ITEM_DETAIL_SQL)
            .bind(resolved.id)
            .bind(self.workspace_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        let assignees: Vec<String> = sqlx::query_scalar(WORK_ITEM_ASSIGNEES_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let labels: Vec<String> = sqlx::query_scalar(WORK_ITEM_LABELS_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let sprint: Option<String> = sqlx::query_scalar(WORK_ITEM_SPRINT_SQL)
            .bind(resolved.id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let tracks: Vec<String> = sqlx::query_scalar(WORK_ITEM_TRACKS_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let services: Vec<String> = sqlx::query_scalar(WORK_ITEM_SERVICES_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let comments: Vec<CommentView> = sqlx::query_as(WORK_ITEM_RECENT_COMMENTS_SQL)
            .bind(resolved.id)
            .bind(RECENT_COMMENTS_LIMIT)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(work_item_detail_json(
            &row,
            &assignees,
            &labels,
            sprint.as_deref(),
            &tracks,
            &services,
            &comments,
        ))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListWorkItemCommentsArgs {
    /// Work item identifier, e.g. "LTS-42".
    pub work_item: String,
    /// Maximum comments to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub const WORK_ITEM_COMMENTS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS author, \
     cm.created_at::text AS at, \
     left(cm.comment_stripped, 500) AS text \
     FROM issue_comments cm \
     LEFT JOIN users u ON u.id = COALESCE(cm.actor_id, cm.created_by_id) \
     WHERE cm.issue_id = $1 AND cm.deleted_at IS NULL \
     ORDER BY cm.created_at DESC, cm.id DESC LIMIT $2";

pub fn comments_json(work_item: &str, rows: &[CommentView]) -> String {
    json!({
        "work_item": work_item,
        "returned": rows.len(),
        "comments": rows,
    })
    .to_string()
}

pub struct ListWorkItemComments {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListWorkItemComments {
    const NAME: &'static str = "list_work_item_comments";
    type Args = ListWorkItemCommentsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List the most recent comments on one work item (newest first), each \
         with author, timestamp, and text truncated to 500 characters."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListWorkItemCommentsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let resolved =
            resolve_work_item(&self.pool, self.workspace_id, self.user_id, &args.work_item).await?;
        let limit = clamp_limit(args.limit);
        let rows: Vec<CommentView> = sqlx::query_as(WORK_ITEM_COMMENTS_SQL)
            .bind(resolved.id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(comments_json(&resolved.identifier, &rows))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListWorkItemRelationsArgs {
    /// Work item identifier, e.g. "LTS-42".
    pub work_item: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RelationRow {
    pub relation_type: String,
    pub identifier: String,
    pub name: String,
}

pub const WORK_ITEM_RELATIONS_SQL: &str = "SELECT \
     r.relation_type AS relation_type, \
     p.identifier || '-' || other.sequence_id AS identifier, \
     other.name AS name \
     FROM issue_relations r \
     JOIN issues other ON other.id = \
       CASE WHEN r.issue_id = $1 THEN r.related_issue_id ELSE r.issue_id END \
       AND other.deleted_at IS NULL \
     JOIN projects p ON p.id = other.project_id AND p.deleted_at IS NULL \
     WHERE (r.issue_id = $1 OR r.related_issue_id = $1) AND r.deleted_at IS NULL \
     ORDER BY r.relation_type, identifier";

pub fn relations_json(work_item: &str, rows: &[RelationRow]) -> String {
    let relations: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "type": row.relation_type,
                "identifier": row.identifier,
                "name": row.name,
            })
        })
        .collect();
    json!({
        "work_item": work_item,
        "returned": relations.len(),
        "relations": relations,
    })
    .to_string()
}

pub struct ListWorkItemRelations {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListWorkItemRelations {
    const NAME: &'static str = "list_work_item_relations";
    type Args = ListWorkItemRelationsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List the relations of one work item (e.g. blocked_by, blocking, \
         relates_to, duplicate) with the other item's identifier and name."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListWorkItemRelationsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let resolved =
            resolve_work_item(&self.pool, self.workspace_id, self.user_id, &args.work_item).await?;
        let rows: Vec<RelationRow> = sqlx::query_as(WORK_ITEM_RELATIONS_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(relations_json(&resolved.identifier, &rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{lazy_pool, ResolvedWorkItem};
    use rig::tool::ToolContext;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_row() -> WorkItemDetailRow {
        WorkItemDetailRow {
            project: "LTS".to_string(),
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
            description: "x".repeat(4100),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "urgent".to_string(),
            work_item_type: "Incident".to_string(),
            start_date: Some("2026-10-01".to_string()),
            target_date: None,
            completed_at: None,
            created_at: "2026-09-30 08:00:00+00".to_string(),
            updated_at: "2026-10-02 09:00:00+00".to_string(),
            sub_issues: 2,
            comment_count: 4,
            relation_count: 1,
        }
    }

    #[test]
    fn truncate_chars_respects_char_boundaries() {
        assert_eq!(truncate_chars("short", 10), "short");
        assert_eq!(truncate_chars("abcdef", 5), "ab...");
        assert_eq!(truncate_chars("ééééé", 4), "é...");
    }

    #[test]
    fn detail_json_is_compact_and_truncated() {
        let comments = vec![CommentView {
            author: "Budi".to_string(),
            at: "2026-10-01 10:00:00+00".to_string(),
            text: "looking into it".to_string(),
        }];
        let out = work_item_detail_json(
            &sample_row(),
            &["Budi".to_string()],
            &["maintenance".to_string()],
            Some("Sprint 3"),
            &["Track A".to_string()],
            &["API".to_string()],
            &comments,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["identifier"], json!("LTS-42"));
        assert_eq!(parsed["state"], json!("In Progress"));
        assert_eq!(parsed["type"], json!("Incident"));
        assert_eq!(parsed["assignees"][0], json!("Budi"));
        assert_eq!(parsed["sprint"], json!("Sprint 3"));
        assert_eq!(parsed["recent_comments"][0]["author"], json!("Budi"));
        let description = parsed["description"].as_str().unwrap();
        assert_eq!(description.chars().count(), DESCRIPTION_MAX);
        assert!(description.ends_with("..."));
    }

    #[tokio::test]
    async fn get_work_item_rejects_a_malformed_ref_before_querying() {
        let trace = crate::agent::new_trace();
        let tool = GetWorkItem {
            pool: lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: trace.clone(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                GetWorkItemArgs {
                    work_item: "nope".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("PROJ-123"));
        assert_eq!(trace.lock().unwrap().len(), 1, "read tools trace before validation");
    }

    #[test]
    fn resolved_work_item_struct_shape() {
        let item = ResolvedWorkItem {
            id: Uuid::nil(),
            project_id: Uuid::nil(),
            identifier: "LTS-1".to_string(),
        };
        assert_eq!(item.identifier, "LTS-1");
    }

    #[test]
    fn comments_json_shape() {
        let rows = vec![
            CommentView {
                author: "Budi".to_string(),
                at: "2026-10-01 10:00:00+00".to_string(),
                text: "first".to_string(),
            },
            CommentView {
                author: "Sari".to_string(),
                at: "2026-10-02 10:00:00+00".to_string(),
                text: "second".to_string(),
            },
        ];
        let parsed: serde_json::Value =
            serde_json::from_str(&comments_json("LTS-42", &rows)).unwrap();
        assert_eq!(parsed["work_item"], json!("LTS-42"));
        assert_eq!(parsed["returned"], json!(2));
        assert_eq!(parsed["comments"][0]["author"], json!("Budi"));
        assert_eq!(parsed["comments"][1]["text"], json!("second"));
    }

    #[tokio::test]
    async fn list_comments_rejects_a_malformed_ref_before_querying() {
        let tool = ListWorkItemComments {
            pool: lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                ListWorkItemCommentsArgs {
                    work_item: "no-dash".to_string(),
                    limit: Some(5),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("PROJ-123"));
    }

    #[test]
    fn relations_json_shape() {
        let rows = vec![RelationRow {
            relation_type: "blocked_by".to_string(),
            identifier: "OPS-3".to_string(),
            name: "Fix network".to_string(),
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&relations_json("LTS-42", &rows)).unwrap();
        assert_eq!(parsed["work_item"], json!("LTS-42"));
        assert_eq!(parsed["relations"][0]["type"], json!("blocked_by"));
        assert_eq!(parsed["relations"][0]["identifier"], json!("OPS-3"));
    }

    #[tokio::test]
    async fn list_relations_rejects_a_malformed_ref_before_querying() {
        let tool = ListWorkItemRelations {
            pool: lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                ListWorkItemRelationsArgs {
                    work_item: "nope".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("PROJ-123"));
    }

    #[test]
    fn work_item_filters_normalize_me_and_trim() {
        let filters = WorkItemFilters::normalize(
            None,
            Some(" lts "),
            Some("Started"),
            Some("URGENT"),
            Some(" Incident "),
            Some(" API "),
            Some("me"),
            Some(" Sprint 3 "),
            Some(" Track A "),
            Some(" bug "),
        )
        .unwrap();
        assert_eq!(filters.project.as_deref(), Some("lts"));
        assert_eq!(filters.state_group.as_deref(), Some("started"));
        assert_eq!(filters.priority.as_deref(), Some("urgent"));
        assert_eq!(filters.work_item_type.as_deref(), Some("Incident"));
        assert_eq!(filters.service.as_deref(), Some("API"));
        assert!(filters.assignee_me);
        assert_eq!(filters.assignee, None);
        assert_eq!(filters.sprint.as_deref(), Some("Sprint 3"));
        assert_eq!(filters.track.as_deref(), Some("Track A"));
        assert_eq!(filters.label.as_deref(), Some("bug"));
    }

    #[test]
    fn work_item_filters_keep_a_named_assignee() {
        let filters = WorkItemFilters::normalize(
            None, None, None, None, None, None, Some("Budi"), None, None, None,
        )
        .unwrap();
        assert_eq!(filters.assignee.as_deref(), Some("Budi"));
        assert!(!filters.assignee_me);
    }

    #[test]
    fn work_item_filters_validate_allowlists() {
        assert!(
            WorkItemFilters::normalize(
                None, None, Some("nope"), None, None, None, None, None, None, None
            )
            .is_err()
        );
        assert!(
            WorkItemFilters::normalize(
                None, None, None, Some("p0"), None, None, None, None, None, None
            )
            .is_err()
        );
    }

    #[test]
    fn work_item_filters_assignee_label_prefers_me() {
        let me = WorkItemFilters {
            assignee_me: true,
            ..Default::default()
        };
        assert_eq!(me.assignee_label().as_deref(), Some("me"));
        let named = WorkItemFilters {
            assignee: Some("Budi".to_string()),
            ..Default::default()
        };
        assert_eq!(named.assignee_label().as_deref(), Some("Budi"));
        assert_eq!(WorkItemFilters::default().assignee_label(), None);
    }
}
