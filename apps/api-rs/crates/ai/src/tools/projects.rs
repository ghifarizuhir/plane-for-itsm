//! `list_projects` read tool.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{db_error, schema_of};
use crate::agent::{record, ToolTrace};

pub const PROJECTS_SQL: &str = "SELECT identifier, name FROM projects \
     WHERE workspace_id = $1 AND deleted_at IS NULL AND archived_at IS NULL \
     ORDER BY name LIMIT 50";

pub fn projects_json(rows: &[(String, String)]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|(identifier, name)| json!({"identifier": identifier, "name": name}))
        .collect();
    json!({"items": items}).to_string()
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ListProjectsArgs {}

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
