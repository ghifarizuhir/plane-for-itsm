//! Intake (triage inbox) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, optional_text, priority_arg, resolve_project, schema_of, ProjectFeature,
};
use crate::agent::{record, ToolTrace};

/// Intake status codes, mirroring Django `intake.py` status choices.
pub const INTAKE_STATUSES: [(&str, i32); 5] = [
    ("pending", -2),
    ("rejected", -1),
    ("snoozed", 0),
    ("accepted", 1),
    ("duplicate", 2),
];

pub fn intake_status_arg(value: Option<&str>) -> Result<i32, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(-2);
    };
    INTAKE_STATUSES
        .iter()
        .find(|(label, _)| *label == normalized)
        .map(|(_, code)| *code)
        .ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "status must be one of: {}",
                INTAKE_STATUSES
                    .iter()
                    .map(|(label, _)| *label)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

pub fn status_label(code: i32) -> &'static str {
    INTAKE_STATUSES
        .iter()
        .find(|(_, value)| *value == code)
        .map(|(label, _)| *label)
        .unwrap_or("unknown")
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct IntakeItemRow {
    pub id: Uuid,
    pub identifier: String,
    pub name: String,
    pub priority: String,
    pub source: String,
    pub created_at: String,
    pub project: String,
    pub suggestion_status: Option<String>,
    pub category_label: Option<String>,
    pub severity_priority: Option<String>,
    pub service_label: Option<String>,
    pub needs_human: Option<f64>,
}

pub const INTAKE_ITEMS_SQL: &str = "SELECT i.id, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, i.priority, \
     COALESCE(ii.source, '') AS source, ii.created_at::text AS created_at, \
     p.identifier AS project, s.status AS suggestion_status, s.category_label, \
     s.severity_priority, s.service_label, s.needs_human \
     FROM intake_issues ii \
     JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = ii.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     LEFT JOIN intake_triage_suggestions s ON s.intake_issue_id = ii.id \
     WHERE ii.workspace_id = $1 AND ii.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ii.status = $3 \
     AND ($4::uuid IS NULL OR p.id = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     AND ($6::text IS NULL OR i.name ILIKE '%' || $6 || '%') \
     ORDER BY ii.created_at DESC LIMIT $7";

pub const INTAKE_COUNT_SQL: &str = "SELECT count(*)::int8 \
     FROM intake_issues ii \
     JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = ii.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     WHERE ii.workspace_id = $1 AND ii.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ii.status = $3 \
     AND ($4::uuid IS NULL OR p.id = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     AND ($6::text IS NULL OR i.name ILIKE '%' || $6 || '%')";

pub fn intake_items_json(rows: &[IntakeItemRow], status: &str) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let suggestion = row.suggestion_status.as_ref().map(|suggestion_status| {
                json!({
                    "status": suggestion_status,
                    "category": row.category_label,
                    "severity": row.severity_priority,
                    "service": row.service_label,
                    "needs_human": row.needs_human,
                })
            });
            json!({
                "id": row.id,
                "identifier": row.identifier,
                "name": row.name,
                "priority": row.priority,
                "source": row.source,
                "created_at": row.created_at,
                "project": row.project,
                "suggestion": suggestion,
            })
        })
        .collect();
    json!({"status": status, "returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListIntakeItemsArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: pending, rejected, snoozed, accepted, duplicate. Defaults to pending.
    pub status: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Case-insensitive substring to match against item names.
    pub query: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListIntakeItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListIntakeItems {
    const NAME: &'static str = "list_intake_items";
    type Args = ListIntakeItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List intake (triage inbox) items the caller can access, newest first, \
         with their triage suggestion summary when one exists \
         (category, severity, service, needs_human). Defaults to pending items. \
         Use the returned id with get_intake_item."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListIntakeItemsArgs>()
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
                super::ensure_feature(&self.pool, project.id, ProjectFeature::Intake).await?;
                Some(project)
            }
            None => None,
        };
        let status = intake_status_arg(args.status.as_deref())?;
        let priority = priority_arg(args.priority.as_deref())?;
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<IntakeItemRow> = sqlx::query_as(INTAKE_ITEMS_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(status)
            .bind(project.as_ref().map(|row| row.id))
            .bind(&priority)
            .bind(&query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(intake_items_json(&rows, status_label(status)))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CountIntakeItemsArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: pending, rejected, snoozed, accepted, duplicate. Defaults to pending.
    pub status: Option<String>,
}

pub struct CountIntakeItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for CountIntakeItems {
    const NAME: &'static str = "count_intake_items";
    type Args = CountIntakeItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Count intake (triage inbox) items the caller can access for one \
         status (default pending), optionally scoped to a project."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CountIntakeItemsArgs>()
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
                super::ensure_feature(&self.pool, project.id, ProjectFeature::Intake).await?;
                Some(project)
            }
            None => None,
        };
        let status = intake_status_arg(args.status.as_deref())?;
        let count: i64 = sqlx::query_scalar(INTAKE_COUNT_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(status)
            .bind(project.as_ref().map(|row| row.id))
            .bind(None::<String>)
            .bind(None::<String>)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(json!({
            "count": count,
            "status": status_label(status),
            "project": project.as_ref().map(|row| row.identifier.clone()),
        })
        .to_string())
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct IntakeDetailRow {
    pub intake_issue_id: Uuid,
    pub identifier: String,
    pub name: String,
    pub priority: String,
    pub source: String,
    pub intake_status: i32,
    pub snoozed_till: Option<String>,
    pub created_at: String,
    pub project: String,
    pub intake_view: bool,
}

pub const INTAKE_DETAIL_SQL: &str = "SELECT ii.id AS intake_issue_id, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, i.priority, \
     COALESCE(ii.source, '') AS source, ii.status AS intake_status, \
     ii.snoozed_till::text AS snoozed_till, ii.created_at::text AS created_at, \
     p.identifier AS project, p.intake_view AS intake_view \
     FROM intake_issues ii \
     JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = ii.project_id AND p.deleted_at IS NULL \
     WHERE ii.issue_id = $1 AND ii.workspace_id = $2 AND ii.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SuggestionRow {
    pub status: String,
    pub model: Option<String>,
    pub category_label: Option<String>,
    pub category_confidence: Option<f64>,
    pub severity_priority: Option<String>,
    pub severity_score: Option<f64>,
    pub severity_confidence: Option<f64>,
    pub service_label: Option<String>,
    pub service_confidence: Option<f64>,
    pub needs_human: Option<f64>,
    pub applied_fields: Vec<String>,
    pub dismissed_fields: Vec<String>,
}

pub const INTAKE_SUGGESTION_SQL: &str = "SELECT status, model, category_label, \
     category_confidence, severity_priority, severity_score, severity_confidence, \
     service_label, service_confidence, needs_human, applied_fields, dismissed_fields \
     FROM intake_triage_suggestions WHERE intake_issue_id = $1";

pub fn intake_detail_json(row: &IntakeDetailRow, suggestion: Option<&SuggestionRow>) -> String {
    json!({
        "id": row.intake_issue_id,
        "identifier": row.identifier,
        "name": row.name,
        "priority": row.priority,
        "source": row.source,
        "status": status_label(row.intake_status),
        "snoozed_till": row.snoozed_till,
        "created_at": row.created_at,
        "project": row.project,
        "suggestion": suggestion,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetIntakeItemArgs {
    /// Intake item id (uuid) as returned by list_intake_items.
    pub intake_item: String,
}

pub struct GetIntakeItem {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetIntakeItem {
    const NAME: &'static str = "get_intake_item";
    type Args = GetIntakeItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one intake (triage inbox) item by id with its full AI triage \
         suggestion: category, severity, and service with confidence scores, \
         which fields were already applied or dismissed, and needs_human."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetIntakeItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let intake_item = Uuid::parse_str(args.intake_item.trim()).map_err(|_| {
            ToolExecutionError::invalid_args("intake_item must be a uuid from list_intake_items")
        })?;
        let row: Option<IntakeDetailRow> = sqlx::query_as(INTAKE_DETAIL_SQL)
            .bind(intake_item)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Err(ToolExecutionError::invalid_args(format!(
                "intake item '{}' was not found or is not accessible",
                args.intake_item
            )));
        };
        if !row.intake_view {
            return Err(ToolExecutionError::invalid_args(
                "the intake feature is disabled in this project",
            ));
        }
        let suggestion: Option<SuggestionRow> = sqlx::query_as(INTAKE_SUGGESTION_SQL)
            .bind(row.intake_issue_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(intake_detail_json(&row, suggestion.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn intake_status_arg_maps_labels_and_defaults_to_pending() {
        assert_eq!(intake_status_arg(None).unwrap(), -2);
        assert_eq!(intake_status_arg(Some(" PENDING ")).unwrap(), -2);
        assert_eq!(intake_status_arg(Some("accepted")).unwrap(), 1);
        assert_eq!(intake_status_arg(Some("duplicate")).unwrap(), 2);
        assert!(intake_status_arg(Some("maybe")).is_err());
    }

    #[test]
    fn status_label_roundtrips_every_code() {
        for (label, code) in INTAKE_STATUSES {
            assert_eq!(status_label(code), label);
        }
    }

    #[test]
    fn intake_items_json_shapes_suggestion_or_null() {
        let rows = vec![
            IntakeItemRow {
                id: Uuid::nil(),
                identifier: "LTS-7".to_string(),
                name: "Alert: disk full".to_string(),
                priority: "none".to_string(),
                source: "webhook".to_string(),
                created_at: "2026-10-01 00:00:00+00".to_string(),
                project: "LTS".to_string(),
                suggestion_status: Some("ready".to_string()),
                category_label: Some("Incident".to_string()),
                severity_priority: Some("high".to_string()),
                service_label: Some("API".to_string()),
                needs_human: Some(0.2),
            },
            IntakeItemRow {
                id: Uuid::new_v4(),
                identifier: "LTS-8".to_string(),
                name: "Manual request".to_string(),
                priority: "low".to_string(),
                source: String::new(),
                created_at: "2026-10-02 00:00:00+00".to_string(),
                project: "LTS".to_string(),
                suggestion_status: None,
                category_label: None,
                severity_priority: None,
                service_label: None,
                needs_human: None,
            },
        ];
        let parsed: serde_json::Value =
            serde_json::from_str(&intake_items_json(&rows, "pending")).unwrap();
        assert_eq!(parsed["status"], json!("pending"));
        assert_eq!(parsed["items"][0]["suggestion"]["category"], json!("Incident"));
        assert_eq!(parsed["items"][0]["suggestion"]["service"], json!("API"));
        assert_eq!(parsed["items"][1]["suggestion"], json!(null));
    }

    #[test]
    fn intake_sql_is_scoped_guarded_and_filtered_by_status() {
        for sql in [INTAKE_ITEMS_SQL, INTAKE_COUNT_SQL] {
            assert!(sql.contains("workspace_id = $1"));
            assert!(sql.contains("project_members"));
            assert!(sql.contains("ii.status = $3"));
        }
    }

    #[tokio::test]
    async fn get_intake_item_rejects_a_non_uuid_before_querying() {
        let tool = GetIntakeItem {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                GetIntakeItemArgs {
                    intake_item: "LTS-7".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("uuid"));
    }

    #[test]
    fn intake_detail_json_shape() {
        let row = IntakeDetailRow {
            intake_issue_id: Uuid::nil(),
            identifier: "LTS-7".to_string(),
            name: "Alert: disk full".to_string(),
            priority: "high".to_string(),
            source: "webhook".to_string(),
            intake_status: -2,
            snoozed_till: None,
            created_at: "2026-10-01 00:00:00+00".to_string(),
            project: "LTS".to_string(),
            intake_view: true,
        };
        let suggestion = SuggestionRow {
            status: "ready".to_string(),
            model: Some("test-model".to_string()),
            category_label: Some("Incident".to_string()),
            category_confidence: Some(0.9),
            severity_priority: Some("high".to_string()),
            severity_score: Some(0.8),
            severity_confidence: Some(0.7),
            service_label: Some("API".to_string()),
            service_confidence: Some(0.6),
            needs_human: Some(0.1),
            applied_fields: vec!["category".to_string()],
            dismissed_fields: vec![],
        };
        let parsed: serde_json::Value =
            serde_json::from_str(&intake_detail_json(&row, Some(&suggestion))).unwrap();
        assert_eq!(parsed["identifier"], json!("LTS-7"));
        assert_eq!(parsed["status"], json!("pending"));
        assert_eq!(parsed["suggestion"]["service_confidence"], json!(0.6));
        assert_eq!(parsed["suggestion"]["applied_fields"][0], json!("category"));

        let without: serde_json::Value =
            serde_json::from_str(&intake_detail_json(&row, None)).unwrap();
        assert_eq!(without["suggestion"], json!(null));
    }
}
