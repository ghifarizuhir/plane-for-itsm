//! Lookup tools: members, states, labels, and work item types.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{clamp_limit, db_error, optional_text, resolve_project, schema_of};
use crate::agent::{record, ToolTrace};

/// Plane project/workspace role codes to display labels.
pub fn role_label(role: i16) -> &'static str {
    match role {
        20 => "admin",
        15 => "member",
        5 => "guest",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MemberRow {
    pub name: String,
    pub email: String,
    pub role: i16,
}

pub fn members_json(scope: &str, rows: &[MemberRow]) -> String {
    let members: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "email": row.email,
                "role": role_label(row.role),
            })
        })
        .collect();
    json!({"scope": scope, "returned": members.len(), "members": members}).to_string()
}

pub const PROJECT_MEMBERS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS name, \
     COALESCE(u.email, '') AS email, pm.role AS role \
     FROM project_members pm JOIN users u ON u.id = pm.member_id \
     WHERE pm.project_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL \
     AND ($2::text IS NULL OR u.display_name ILIKE '%' || $2 || '%' \
       OR COALESCE(u.email, '') ILIKE '%' || $2 || '%') \
     ORDER BY u.display_name, u.id LIMIT $3";

pub const WORKSPACE_MEMBERS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS name, \
     COALESCE(u.email, '') AS email, wm.role AS role \
     FROM workspace_members wm JOIN users u ON u.id = wm.member_id \
     WHERE wm.workspace_id = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
     AND ($2::text IS NULL OR u.display_name ILIKE '%' || $2 || '%' \
       OR COALESCE(u.email, '') ILIKE '%' || $2 || '%') \
     ORDER BY u.display_name, u.id LIMIT $3";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListMembersArgs {
    /// Project identifier or name to scope the list; omitted means the whole workspace.
    pub project: Option<String>,
    /// Case-insensitive substring matched against display name or email.
    pub query: Option<String>,
    /// Maximum members to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListMembers {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListMembers {
    const NAME: &'static str = "list_members";
    type Args = ListMembersArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List people with their role: project members when a project is given, \
         otherwise workspace members. Optionally filtered by a name or email \
         substring. Use this to resolve a person before proposing assignments."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListMembersArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                let rows: Vec<MemberRow> = sqlx::query_as(PROJECT_MEMBERS_SQL)
                    .bind(project.id)
                    .bind(&query)
                    .bind(limit)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)?;
                Ok(members_json(
                    &format!("project {}", project.identifier),
                    &rows,
                ))
            }
            None => {
                let rows: Vec<MemberRow> = sqlx::query_as(WORKSPACE_MEMBERS_SQL)
                    .bind(self.workspace_id)
                    .bind(&query)
                    .bind(limit)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)?;
                Ok(members_json("workspace", &rows))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn role_label_maps_plane_roles() {
        assert_eq!(role_label(20), "admin");
        assert_eq!(role_label(15), "member");
        assert_eq!(role_label(5), "guest");
        assert_eq!(role_label(99), "unknown");
    }

    #[test]
    fn members_json_shape() {
        let rows = vec![
            MemberRow {
                name: "Budi".to_string(),
                email: "budi@example.com".to_string(),
                role: 20,
            },
            MemberRow {
                name: "Sari".to_string(),
                email: String::new(),
                role: 15,
            },
        ];
        let parsed: serde_json::Value =
            serde_json::from_str(&members_json("project LTS", &rows)).unwrap();
        assert_eq!(parsed["scope"], json!("project LTS"));
        assert_eq!(parsed["returned"], json!(2));
        assert_eq!(parsed["members"][0]["role"], json!("admin"));
        assert_eq!(parsed["members"][1]["email"], json!(""));
    }
}
