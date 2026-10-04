//! Service catalog read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{clamp_limit, db_error, optional_text, resolve_project, schema_of};
use crate::agent::{record, ToolTrace};

pub const SERVICE_STATUSES: [&str; 5] = ["active", "planned", "maintenance", "deprecated", "retired"];
pub const SERVICE_CRITICALITIES: [&str; 4] = ["critical", "high", "medium", "low"];
pub const SERVICE_TYPES: [&str; 4] = ["internal", "external", "infrastructure", "third_party"];

pub fn service_allowlist(
    value: Option<&str>,
    allowed: &[&str],
    field: &str,
) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if allowed.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "{field} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub criticality: String,
    pub service_type: String,
    pub owner: String,
    pub project: String,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
}

pub const SERVICES_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.workspace_id = $1 AND sv.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($3::text IS NULL OR sv.name ILIKE '%' || $3 || '%') \
     AND ($4::text IS NULL OR sv.status = $4) \
     AND ($5::text IS NULL OR sv.criticality = $5) \
     AND ($6::text IS NULL OR sv.\"type\" = $6) \
     AND ($7::text IS NULL OR p.identifier ILIKE $7 OR p.name ILIKE '%' || $7 || '%') \
     ORDER BY sv.name LIMIT $8";

pub fn services_json(rows: &[ServiceRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "project": row.project,
                "status": row.status,
                "criticality": row.criticality,
                "type": row.service_type,
                "owner": row.owner,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListServicesArgs {
    /// Case-insensitive substring to match against service names.
    pub query: Option<String>,
    /// One of: active, planned, maintenance, deprecated, retired.
    pub status: Option<String>,
    /// One of: critical, high, medium, low.
    pub criticality: Option<String>,
    /// One of: internal, external, infrastructure, third_party.
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListServices {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListServices {
    const NAME: &'static str = "list_services";
    type Args = ListServicesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List services in the service catalog the caller can access, optionally \
         filtered by name substring, lifecycle status, criticality, type, and \
         project. Returns name, project, status, criticality, type, and owner."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListServicesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let status = service_allowlist(args.status.as_deref(), &SERVICE_STATUSES, "status")?;
        let criticality =
            service_allowlist(args.criticality.as_deref(), &SERVICE_CRITICALITIES, "criticality")?;
        let service_type =
            service_allowlist(args.service_type.as_deref(), &SERVICE_TYPES, "type")?;
        let project = optional_text(args.project.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<ServiceRow> = sqlx::query_as(SERVICES_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(&query)
            .bind(&status)
            .bind(&criticality)
            .bind(&service_type)
            .bind(&project)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(services_json(&rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn service_allowlist_rejects_unknown_values() {
        assert!(service_allowlist(Some("Active"), &SERVICE_STATUSES, "status").unwrap().is_some());
        assert!(service_allowlist(Some("burning"), &SERVICE_STATUSES, "status").is_err());
        assert!(service_allowlist(Some("CRITICAL"), &SERVICE_CRITICALITIES, "criticality").unwrap().is_some());
        assert!(service_allowlist(Some("nope"), &SERVICE_TYPES, "type").is_err());
        assert_eq!(service_allowlist(None, &SERVICE_TYPES, "type").unwrap(), None);
    }

    #[test]
    fn services_json_shape() {
        let rows = vec![ServiceRow {
            id: Uuid::nil(),
            name: "API".to_string(),
            description: String::new(),
            status: "active".to_string(),
            criticality: "critical".to_string(),
            service_type: "internal".to_string(),
            owner: "Budi".to_string(),
            project: "LTS".to_string(),
            repository_url: None,
            documentation_url: None,
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&services_json(&rows)).unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["name"], json!("API"));
        assert_eq!(parsed["items"][0]["type"], json!("internal"));
        assert_eq!(parsed["items"][0]["owner"], json!("Budi"));
    }

    #[test]
    fn services_sql_is_scoped_and_guarded() {
        assert!(SERVICES_SQL.contains("workspace_id = $1"));
        assert!(SERVICES_SQL.contains("project_members"));
        assert!(SERVICES_SQL.contains("sv.deleted_at IS NULL"));
    }
}
