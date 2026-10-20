//! Schedule management tools: two read tools plus an `update_schedule`
//! proposal. The proposal never touches the database; the UI confirms it.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{db_error, optional_text, schema_of};
use crate::agent::{record, ToolTrace};
use crate::schedule::{ScheduleChanges, ScheduleSpec};

pub const UPDATE_SCHEDULE_NAME: &str = "update_schedule";
const SCHEDULE_LIST_DEFAULT: i64 = 20;
const SCHEDULE_LIST_MAX: i64 = 50;

pub const LIST_SCHEDULES_SQL: &str = "SELECT s.id, s.name, s.enabled, s.spec, s.prompt, \
     s.frequency, s.time_of_day, s.day_of_week, s.day_of_month, s.timezone, s.next_run_at, \
     (s.created_by_id = $2 OR EXISTS ( \
       SELECT 1 FROM workspace_members wm \
       WHERE wm.workspace_id = s.workspace_id AND wm.member_id = $2 \
         AND wm.role = 20 AND wm.is_active = true)) AS can_edit \
     FROM ai_schedules s \
     WHERE s.workspace_id = $1 AND s.deleted_at IS NULL \
       AND ($3::text IS NULL OR s.name ILIKE '%' || $3 || '%') \
       AND ($4::bool IS NULL OR s.enabled = $4) \
     ORDER BY s.created_at DESC, s.id LIMIT $5";

#[derive(sqlx::FromRow)]
struct ScheduleListRow {
    id: Uuid,
    name: String,
    enabled: bool,
    spec: Option<Value>,
    prompt: String,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
    next_run_at: chrono::DateTime<chrono::Utc>,
    can_edit: bool,
}

fn schedule_summary(row: &ScheduleListRow) -> Value {
    let description = row.spec.as_ref().and_then(|value| {
        serde_json::from_value::<ScheduleSpec>(value.clone())
            .ok()
            .map(|spec| spec.description)
    });
    json!({
        "id": row.id,
        "name": row.name,
        "enabled": row.enabled,
        "description": description,
        "frequency": row.frequency,
        "time": row.time_of_day,
        "day_of_week": row.day_of_week,
        "day_of_month": row.day_of_month,
        "timezone": row.timezone,
        "next_run_at": row.next_run_at,
        "can_edit": row.can_edit,
    })
}

fn schedule_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(SCHEDULE_LIST_DEFAULT)
        .clamp(1, SCHEDULE_LIST_MAX)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListSchedulesArgs {
    /// Case-insensitive substring matched against the schedule name.
    pub query: Option<String>,
    /// Filter by enabled/paused state.
    pub enabled: Option<bool>,
    /// Maximum schedules to return, 1-50 (default 20).
    pub limit: Option<i64>,
}

pub struct ListSchedules {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListSchedules {
    const NAME: &'static str = "list_schedules";
    type Args = ListSchedulesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List saved AI schedules in this workspace, newest first. Use this to \
         find the schedule the user wants to change; can_edit is false when the \
         signed-in user is neither the creator nor a workspace admin."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListSchedulesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let rows: Vec<ScheduleListRow> = sqlx::query_as(LIST_SCHEDULES_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(&query)
            .bind(args.enabled)
            .bind(schedule_limit(args.limit))
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let items: Vec<Value> = rows.iter().map(schedule_summary).collect();
        Ok(json!({"items": items, "returned": items.len()}).to_string())
    }
}

pub const GET_SCHEDULE_SQL: &str = "SELECT s.id, s.name, s.enabled, s.spec, s.prompt, \
     s.frequency, s.time_of_day, s.day_of_week, s.day_of_month, s.timezone, s.next_run_at, \
     (s.created_by_id = $3 OR EXISTS ( \
       SELECT 1 FROM workspace_members wm \
       WHERE wm.workspace_id = s.workspace_id AND wm.member_id = $3 \
         AND wm.role = 20 AND wm.is_active = true)) AS can_edit \
     FROM ai_schedules s \
     WHERE s.workspace_id = $1 AND s.id = $2 AND s.deleted_at IS NULL";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetScheduleArgs {
    /// Schedule UUID returned by list_schedules.
    pub schedule_id: String,
}

pub struct GetSchedule {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetSchedule {
    const NAME: &'static str = "get_schedule";
    type Args = GetScheduleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Load one saved AI schedule, including its full recipe and raw prompt. \
         Use this before proposing a change so the proposal is grounded in the \
         current values."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetScheduleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let schedule_id = Uuid::parse_str(args.schedule_id.trim())
            .map_err(|_| ToolExecutionError::invalid_args("schedule_id must be a uuid"))?;
        let row: Option<ScheduleListRow> = sqlx::query_as(GET_SCHEDULE_SQL)
            .bind(self.workspace_id)
            .bind(schedule_id)
            .bind(self.user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Err(ToolExecutionError::invalid_args("schedule was not found"));
        };
        let spec = row.spec.clone().unwrap_or(Value::Null);
        let mut value = schedule_summary(&row);
        value["spec"] = spec;
        value["prompt"] = json!(row.prompt);
        Ok(value.to_string())
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct UpdateScheduleArgs {
    /// Schedule UUID returned by list_schedules.
    pub schedule_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub how_to: Option<Vec<String>>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub expected_output: Option<String>,
    /// Raw prompt for legacy schedules only; never mix with recipe fields.
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub frequency: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub day_of_week: Option<i16>,
    #[serde(default)]
    pub day_of_month: Option<i16>,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateScheduleProposal {
    pub schedule_id: String,
    pub changes: ScheduleChanges,
}

impl UpdateScheduleArgs {
    fn into_proposal(self) -> Result<UpdateScheduleProposal, ToolExecutionError> {
        let schedule_id = self.schedule_id.trim();
        Uuid::parse_str(schedule_id)
            .map_err(|_| ToolExecutionError::invalid_args("schedule_id must be a uuid"))?;
        let mut changes = ScheduleChanges {
            name: self.name,
            description: self.description,
            how_to: self.how_to,
            tools: self.tools,
            expected_output: self.expected_output,
            prompt: self.prompt,
            frequency: self.frequency,
            time: self.time,
            day_of_week: self.day_of_week,
            day_of_month: self.day_of_month,
            timezone: self.timezone,
        };
        if let Some(frequency) = changes.frequency.as_mut() {
            *frequency = frequency.trim().to_ascii_lowercase();
        }
        changes
            .validate()
            .map_err(ToolExecutionError::invalid_args)?;
        Ok(UpdateScheduleProposal {
            schedule_id: schedule_id.to_string(),
            changes,
        })
    }
}

pub struct UpdateSchedule {
    pub trace: ToolTrace,
}

impl Tool for UpdateSchedule {
    const NAME: &'static str = UPDATE_SCHEDULE_NAME;
    type Args = UpdateScheduleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing one saved AI schedule. First call list_schedules and \
         get_schedule; if more than one schedule could match, ask the user. Send \
         the schedule_id and only the fields that change. Structured recipe \
         fields cannot be mixed with a raw prompt; the raw prompt is only for \
         legacy schedules. Pause/resume/delete/run-now are not tools. The user \
         must confirm the before/after card before anything is saved; never \
         claim the schedule was updated before that confirmation."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateScheduleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = args.into_proposal()?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateScheduleProposal serializes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::new_trace;

    #[test]
    fn list_schedules_sql_is_workspace_scoped() {
        assert!(LIST_SCHEDULES_SQL.contains("workspace_id = $1"));
        assert!(LIST_SCHEDULES_SQL.contains("deleted_at IS NULL"));
        assert!(LIST_SCHEDULES_SQL.contains("role = 20"));
    }

    #[test]
    fn update_args_reject_empty_and_mixed_prompt() {
        let empty = UpdateScheduleArgs {
            schedule_id: Uuid::nil().to_string(),
            ..Default::default()
        };
        assert!(empty
            .into_proposal()
            .unwrap_err()
            .to_string()
            .contains("at least one"));

        let mixed = UpdateScheduleArgs {
            schedule_id: Uuid::nil().to_string(),
            prompt: Some("raw".to_string()),
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        assert!(mixed
            .into_proposal()
            .unwrap_err()
            .to_string()
            .contains("cannot be combined"));
    }

    #[tokio::test]
    async fn update_schedule_records_a_normalized_proposal() {
        let trace = new_trace();
        let tool = UpdateSchedule {
            trace: trace.clone(),
        };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                UpdateScheduleArgs {
                    schedule_id: Uuid::nil().to_string(),
                    frequency: Some("WEEKLY".to_string()),
                    day_of_week: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["schedule_id"], json!(Uuid::nil().to_string()));
        assert_eq!(parsed["changes"]["frequency"], json!("weekly"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded[0].name, "update_schedule");
    }
}
