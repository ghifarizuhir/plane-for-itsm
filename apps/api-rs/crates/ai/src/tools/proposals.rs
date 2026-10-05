//! Proposal tools: `create_schedule` and `create_work_item`. They never touch
//! the database; the UI renders a confirmation card from the recorded trace.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{optional_text, priority_arg, schema_of};
use crate::agent::{record, ToolTrace};
use crate::schedule::ScheduleRecipe;

pub const CREATE_SCHEDULE_NAME: &str = "create_schedule";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateScheduleArgs {
    /// Short human-readable schedule name (1-120 characters), e.g. "Daily overdue report".
    pub name: String,
    /// What the schedule is for: one or two sentences of context (1-500 characters).
    pub description: String,
    /// Ordered, concrete steps the agent must follow on every fire (1-10 steps, each 1-500 characters).
    pub how_to: Vec<String>,
    /// Tools the run may use: at least one read tool name from the schema's list.
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
        "Propose a recurring scheduled task for this workspace. Only call this \
         when the user explicitly asks for a recurring or scheduled task (for \
         example a message starting with /schedule), and only after the recipe \
         is complete: description, ordered how_to steps, the read tools it \
         needs (at least one), expected_output, and the frequency. The user \
         must confirm and may edit every field in the UI before anything is \
         saved. Never claim the schedule exists until they confirm."
            .to_string()
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

pub fn bounded_ref_list(
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

/// Like `bounded_ref_list` but preserves "field absent" as `None` (an empty
/// list means "clear the field").
pub fn bounded_ref_list_opt(
    values: Option<Vec<String>>,
    label: &str,
) -> Result<Option<Vec<String>>, ToolExecutionError> {
    match values {
        None => Ok(None),
        Some(values) => bounded_ref_list(Some(values), label).map(Some),
    }
}

pub fn parse_iso_date(value: &str, label: &str) -> Result<chrono::NaiveDate, ToolExecutionError> {
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
