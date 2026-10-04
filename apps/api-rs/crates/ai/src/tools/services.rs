//! Service catalog read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{clamp_limit, db_error, optional_text, resolve_project, schema_of, ProjectRow};
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

pub const SERVICE_BY_ID_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.id = $1 AND sv.workspace_id = $2 AND sv.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

pub const SERVICES_BY_NAME_PROJECT_SQL: &str = "SELECT sv.id, sv.name, sv.description, \
     sv.status, sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.project_id = $1 AND sv.workspace_id = $2 AND sv.deleted_at IS NULL \
     AND (lower(sv.name) = lower($3) OR sv.name ILIKE '%' || $3 || '%') \
     ORDER BY sv.name LIMIT 10";

pub const SERVICES_BY_NAME_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.workspace_id = $1 AND sv.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(sv.name) = lower($3) OR sv.name ILIKE '%' || $3 || '%') \
     ORDER BY sv.name LIMIT 10";

/// Pick one service: a single exact name match, else a unique substring.
pub fn pick_service(
    rows: Vec<ServiceRow>,
    reference: &str,
) -> Result<ServiceRow, ToolExecutionError> {
    let needle = reference.trim();
    if needle.is_empty() || rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "service '{needle}' was not found or is not accessible"
        )));
    }
    let exact: Vec<&ServiceRow> = rows
        .iter()
        .filter(|row| row.name.eq_ignore_ascii_case(needle))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }
    if exact.len() > 1 {
        let names = exact
            .iter()
            .map(|row| format!("{}/{}", row.project, row.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolExecutionError::invalid_args(format!(
            "service '{needle}' is ambiguous; use one of: {names}"
        )));
    }
    let lowered = needle.to_ascii_lowercase();
    let matching: Vec<&ServiceRow> = rows
        .iter()
        .filter(|row| row.name.to_ascii_lowercase().contains(&lowered))
        .collect();
    if matching.len() == 1 {
        return Ok(matching[0].clone());
    }
    if matching.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "service '{needle}' was not found or is not accessible"
        )));
    }
    let names = matching
        .iter()
        .map(|row| format!("{}/{}", row.project, row.name))
        .collect::<Vec<_>>()
        .join(", ");
    Err(ToolExecutionError::invalid_args(format!(
        "service '{needle}' is ambiguous; use one of: {names}"
    )))
}

pub async fn resolve_service(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
    project: Option<&ProjectRow>,
) -> Result<ServiceRow, ToolExecutionError> {
    if let Ok(id) = Uuid::parse_str(reference.trim()) {
        let row: Option<ServiceRow> = sqlx::query_as(SERVICE_BY_ID_SQL)
            .bind(id)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
        return row.ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "service '{reference}' was not found or is not accessible"
            ))
        });
    }
    let rows: Vec<ServiceRow> = match project {
        Some(project) => sqlx::query_as(SERVICES_BY_NAME_PROJECT_SQL)
            .bind(project.id)
            .bind(workspace_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
        None => sqlx::query_as(SERVICES_BY_NAME_SQL)
            .bind(workspace_id)
            .bind(user_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
    };
    pick_service(rows, reference)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct DependencyRow {
    pub direction: String,
    pub name: String,
    pub status: String,
}

pub const SERVICE_DEPENDENCIES_SQL: &str = "SELECT \
     CASE WHEN d.from_service_id = $1 THEN 'depends_on' ELSE 'needed_by' END AS direction, \
     sv.name, sv.status \
     FROM service_dependencies d \
     JOIN services sv ON sv.id = CASE WHEN d.from_service_id = $1 \
       THEN d.to_service_id ELSE d.from_service_id END \
     WHERE (d.from_service_id = $1 OR d.to_service_id = $1) AND d.deleted_at IS NULL \
     AND sv.deleted_at IS NULL \
     ORDER BY direction, sv.name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct ServiceItemRow {
    pub identifier: String,
    pub name: String,
    pub state: String,
    pub state_group: String,
    pub priority: String,
}

pub const SERVICE_OPEN_ITEMS_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, COALESCE(s.\"group\", '') AS state_group, i.priority \
     FROM service_issues si \
     JOIN issues i ON i.id = si.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE si.service_id = $1 AND si.deleted_at IS NULL \
     AND s.\"group\" NOT IN ('completed', 'cancelled') \
     ORDER BY i.updated_at DESC LIMIT 10";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct StateCountRow {
    pub state_group: String,
    pub count: i64,
}

pub const SERVICE_STATE_COUNTS_SQL: &str = "SELECT s.\"group\" AS state_group, \
     count(*)::int8 AS count \
     FROM service_issues si \
     JOIN issues i ON i.id = si.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE si.service_id = $1 AND si.deleted_at IS NULL \
     GROUP BY s.\"group\" ORDER BY s.\"group\"";

pub fn service_detail_json(
    row: &ServiceRow,
    dependencies: &[DependencyRow],
    open_items: &[ServiceItemRow],
    counts: &[StateCountRow],
) -> String {
    json!({
        "name": row.name,
        "project": row.project,
        "description": crate::tools::work_items::truncate_chars(&row.description, 2000),
        "status": row.status,
        "criticality": row.criticality,
        "type": row.service_type,
        "owner": row.owner,
        "repository_url": row.repository_url,
        "documentation_url": row.documentation_url,
        "dependencies": dependencies,
        "open_work_items": open_items,
        "state_counts": counts,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetServiceArgs {
    /// Service name, or a service id when known.
    pub service: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
}

pub struct GetService {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetService {
    const NAME: &'static str = "get_service";
    type Args = GetServiceArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one service from the catalog by name or id: status, criticality, \
         type, owner, description, links, its dependencies (depends_on / \
         needed_by), the open work items linked to it, and the count of linked \
         work items per state group."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetServiceArgs>()
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
        let row = resolve_service(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.service,
            project.as_ref(),
        )
        .await?;
        let dependencies: Vec<DependencyRow> = sqlx::query_as(SERVICE_DEPENDENCIES_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let open_items: Vec<ServiceItemRow> = sqlx::query_as(SERVICE_OPEN_ITEMS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let counts: Vec<StateCountRow> = sqlx::query_as(SERVICE_STATE_COUNTS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(service_detail_json(&row, &dependencies, &open_items, &counts))
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

    fn service(name: &str, project: &str) -> ServiceRow {
        ServiceRow {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            status: "active".to_string(),
            criticality: "high".to_string(),
            service_type: "internal".to_string(),
            owner: String::new(),
            project: project.to_string(),
            repository_url: None,
            documentation_url: None,
        }
    }

    #[test]
    fn pick_service_prefers_exact_then_unique_substring() {
        let rows = vec![service("API", "LTS"), service("API Gateway", "OPS")];
        assert_eq!(pick_service(rows.clone(), "api").unwrap().name, "API");
        assert_eq!(pick_service(rows.clone(), "gateway").unwrap().name, "API Gateway");
        assert!(pick_service(rows, "a").is_err());
        assert!(pick_service(vec![], "api").is_err());
    }

    #[test]
    fn pick_service_is_ambiguous_when_exact_matches_multiple_projects() {
        let rows = vec![service("API", "LTS"), service("API", "OPS")];
        let error = pick_service(rows, "API").unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
        assert!(error.to_string().contains("LTS/API"));
    }

    #[test]
    fn service_detail_json_shape() {
        let row = service("API", "LTS");
        let dependencies = vec![DependencyRow {
            direction: "depends_on".to_string(),
            name: "Database".to_string(),
            status: "active".to_string(),
        }];
        let items = vec![ServiceItemRow {
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "urgent".to_string(),
        }];
        let counts = vec![StateCountRow {
            state_group: "started".to_string(),
            count: 3,
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&service_detail_json(&row, &dependencies, &items, &counts)).unwrap();
        assert_eq!(parsed["name"], json!("API"));
        assert_eq!(parsed["dependencies"][0]["direction"], json!("depends_on"));
        assert_eq!(parsed["open_work_items"][0]["identifier"], json!("LTS-42"));
        assert_eq!(parsed["state_counts"][0]["count"], json!(3));
    }
}
