//! Fase 2 mutation proposal tools. They never touch the database; the UI
//! renders a confirmation card from the recorded trace.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{
    bounded_ref_list, bounded_ref_list_opt, enum_arg, optional_enum_arg, optional_text,
    parse_iso_date, parse_work_item_ref, priority_arg, schema_of, WORK_ITEM_DESCRIPTION_MAX,
    WORK_ITEM_NAME_MAX, WORK_ITEM_STATE_MAX,
};
use crate::agent::{record, ToolTrace};

pub const UPDATE_WORK_ITEM_NAME: &str = "update_work_item";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateWorkItemArgs {
    /// Work item identifier like "LTS-42". Required.
    pub work_item: String,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateWorkItemChanges,
}

/// One requested field change. Human-readable names (state, assignees, labels)
/// are kept as text; the UI resolves them against the project stores.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateWorkItemChanges {
    /// New title (1-255 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New plain-text description or markdown (max 5000 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New priority: urgent, high, medium, low, none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// New state name, e.g. "In Progress".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// New assignee display names or emails; an empty list clears them (max 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
    /// New label names; an empty list clears them (max 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// New start date "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// New target date "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
}

/// Normalized proposal recorded in the trace and rendered as a confirmation
/// card.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateWorkItemProposal {
    pub work_item: String,
    pub changes: UpdateWorkItemChanges,
}

fn normalize_changes(
    changes: UpdateWorkItemChanges,
) -> Result<UpdateWorkItemChanges, ToolExecutionError> {
    let name = match changes.name.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "name must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > WORK_ITEM_NAME_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "name must be at most {WORK_ITEM_NAME_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let description = match changes.description.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "description must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > WORK_ITEM_DESCRIPTION_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "description must be at most {WORK_ITEM_DESCRIPTION_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let priority = priority_arg(changes.priority.as_deref())?;
    let state = match changes.state.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "state must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > WORK_ITEM_STATE_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "state must be at most {WORK_ITEM_STATE_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let assignees = bounded_ref_list_opt(changes.assignees, "assignees")?;
    let labels = bounded_ref_list_opt(changes.labels, "labels")?;
    let start_date = changes
        .start_date
        .as_deref()
        .map(|value| parse_iso_date(value, "start_date"))
        .transpose()?;
    let target_date = changes
        .target_date
        .as_deref()
        .map(|value| parse_iso_date(value, "target_date"))
        .transpose()?;
    if let (Some(start_date), Some(target_date)) = (&start_date, &target_date) {
        if start_date > target_date {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    let normalized = UpdateWorkItemChanges {
        name,
        description,
        priority,
        state,
        assignees,
        labels,
        start_date: start_date.map(|date| date.to_string()),
        target_date: target_date.map(|date| date.to_string()),
    };
    if normalized == UpdateWorkItemChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    Ok(normalized)
}

/// Validate raw tool args into a normalized update proposal. Human-readable
/// names are kept as text; the UI resolves them.
pub fn update_work_item_proposal_from_args(
    args: UpdateWorkItemArgs,
) -> Result<UpdateWorkItemProposal, ToolExecutionError> {
    let work_item = args.work_item.trim();
    if work_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("work_item is required"));
    }
    parse_work_item_ref(work_item)?;
    let changes = normalize_changes(args.changes)?;
    Ok(UpdateWorkItemProposal {
        work_item: work_item.to_string(),
        changes,
    })
}

pub const ADD_COMMENT_NAME: &str = "add_comment";
pub const MAX_COMMENT_CHARS: usize = 5000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddCommentArgs {
    /// Work item identifier like "LTS-42". Required.
    pub work_item: String,
    /// Plain-text comment (1-5000 characters).
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddCommentProposal {
    pub work_item: String,
    pub comment: String,
}

/// Validate raw tool args into a normalized comment proposal.
pub fn add_comment_proposal_from_args(
    args: AddCommentArgs,
) -> Result<AddCommentProposal, ToolExecutionError> {
    let work_item = args.work_item.trim();
    if work_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("work_item is required"));
    }
    parse_work_item_ref(work_item)?;
    let comment = args.comment.trim();
    if comment.is_empty() {
        return Err(ToolExecutionError::invalid_args("comment is required"));
    }
    if comment.chars().count() > MAX_COMMENT_CHARS {
        return Err(ToolExecutionError::invalid_args(format!(
            "comment must be at most {MAX_COMMENT_CHARS} characters"
        )));
    }
    Ok(AddCommentProposal {
        work_item: work_item.to_string(),
        comment: comment.to_string(),
    })
}

pub struct AddComment {
    pub trace: ToolTrace,
}

impl Tool for AddComment {
    const NAME: &'static str = ADD_COMMENT_NAME;
    type Args = AddCommentArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose posting one comment on an existing work item. Only call this \
         when the user asks to comment on or reply to a work item. The work item \
         identifier is required; never guess it. Keep the comment in the user's \
         language. The user must confirm and may edit the text in the UI before \
         it is posted. Never claim the comment was posted until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<AddCommentArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = add_comment_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("AddCommentProposal serializes"))
    }
}

pub const MANAGE_SERVICE_LINKS_NAME: &str = "manage_service_links";
pub const MAX_SERVICE_REFS: usize = 10;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageServiceLinksArgs {
    /// Work item identifier like "LTS-42". Required.
    pub work_item: String,
    /// Service names or ids (1-10).
    pub services: Vec<String>,
    /// One of: link, unlink.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageServiceLinksProposal {
    pub work_item: String,
    pub services: Vec<String>,
    pub action: String,
}

/// Validate raw tool args into a normalized service link proposal.
pub fn manage_service_links_proposal_from_args(
    args: ManageServiceLinksArgs,
) -> Result<ManageServiceLinksProposal, ToolExecutionError> {
    let work_item = args.work_item.trim();
    if work_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("work_item is required"));
    }
    parse_work_item_ref(work_item)?;
    let action = enum_arg(&args.action, &["link", "unlink"], "action")?;
    if args.services.is_empty() {
        return Err(ToolExecutionError::invalid_args(
            "at least one service is required",
        ));
    }
    if args.services.len() > MAX_SERVICE_REFS {
        return Err(ToolExecutionError::invalid_args(format!(
            "at most {MAX_SERVICE_REFS} services are allowed"
        )));
    }
    let services = bounded_ref_list(Some(args.services), "services")?;
    Ok(ManageServiceLinksProposal {
        work_item: work_item.to_string(),
        services,
        action,
    })
}

pub struct ManageServiceLinks {
    pub trace: ToolTrace,
}

impl Tool for ManageServiceLinks {
    const NAME: &'static str = MANAGE_SERVICE_LINKS_NAME;
    type Args = ManageServiceLinksArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose linking or unlinking services to one work item. Only call this \
         when the user asks to attach or detach services on a work item. The work \
         item identifier and at least one service name or id are required; never \
         guess them. The user must confirm the card in the UI before anything is \
         saved. Never claim the links were changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageServiceLinksArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_service_links_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageServiceLinksProposal serializes"))
    }
}

pub const MANAGE_SPRINT_ITEMS_NAME: &str = "manage_sprint_items";
pub const MAX_BULK_ITEMS: usize = 25;
pub const CONTAINER_REF_MAX: usize = 255;
pub const CONTAINER_PROJECT_MAX: usize = 100;

fn required_text(
    value: &str,
    label: &str,
    max: usize,
) -> Result<String, ToolExecutionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} is required"
        )));
    }
    if trimmed.chars().count() > max {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must be at most {max} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn optional_container_project(
    value: Option<String>,
) -> Result<Option<String>, ToolExecutionError> {
    match value.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(project) if project.chars().count() > CONTAINER_PROJECT_MAX => {
            Err(ToolExecutionError::invalid_args(format!(
                "project must be at most {CONTAINER_PROJECT_MAX} characters"
            )))
        }
        Some(project) => Ok(Some(project.to_string())),
    }
}

/// Validate and dedupe a bulk list of `PROJ-123` references.
pub fn work_item_refs_arg(
    values: Vec<String>,
) -> Result<Vec<String>, ToolExecutionError> {
    if values.is_empty() {
        return Err(ToolExecutionError::invalid_args(
            "at least one work item is required",
        ));
    }
    if values.len() > MAX_BULK_ITEMS {
        return Err(ToolExecutionError::invalid_args(format!(
            "at most {MAX_BULK_ITEMS} work items are allowed"
        )));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let trimmed = value.trim();
        parse_work_item_ref(trimmed)?;
        let key = trimmed.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(trimmed.to_string());
    }
    Ok(out)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageSprintItemsArgs {
    /// Sprint name or id. Required.
    pub sprint: String,
    /// Project identifier or name; use it when the sprint name is not unique.
    pub project: Option<String>,
    /// Work item identifiers like "LTS-42" (1-25).
    pub work_items: Vec<String>,
    /// One of: add, remove.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageSprintItemsProposal {
    pub sprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub work_items: Vec<String>,
    pub action: String,
}

/// Validate raw tool args into a normalized sprint items proposal.
pub fn manage_sprint_items_proposal_from_args(
    args: ManageSprintItemsArgs,
) -> Result<ManageSprintItemsProposal, ToolExecutionError> {
    let sprint = required_text(&args.sprint, "sprint", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["add", "remove"], "action")?;
    let work_items = work_item_refs_arg(args.work_items)?;
    Ok(ManageSprintItemsProposal {
        sprint,
        project,
        work_items,
        action,
    })
}

pub struct ManageSprintItems {
    pub trace: ToolTrace,
}

impl Tool for ManageSprintItems {
    const NAME: &'static str = MANAGE_SPRINT_ITEMS_NAME;
    type Args = ManageSprintItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose adding or removing work items in a sprint. Only call this when \
         the user asks to plan or change sprint scope. The sprint name (or id) \
         and at least one work item identifier are required; never guess them. \
         Pass the project when the sprint name is not unique. The user must \
         confirm the card in the UI before anything is saved. Never claim the \
         sprint was changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageSprintItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_sprint_items_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageSprintItemsProposal serializes"))
    }
}

pub const MANAGE_TRACK_ITEMS_NAME: &str = "manage_track_items";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageTrackItemsArgs {
    /// Track (module) name or id. Required.
    pub track: String,
    /// Project identifier or name; use it when the track name is not unique.
    pub project: Option<String>,
    /// Work item identifiers like "LTS-42" (1-25).
    pub work_items: Vec<String>,
    /// One of: add, remove.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageTrackItemsProposal {
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub work_items: Vec<String>,
    pub action: String,
}

/// Validate raw tool args into a normalized track items proposal.
pub fn manage_track_items_proposal_from_args(
    args: ManageTrackItemsArgs,
) -> Result<ManageTrackItemsProposal, ToolExecutionError> {
    let track = required_text(&args.track, "track", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["add", "remove"], "action")?;
    let work_items = work_item_refs_arg(args.work_items)?;
    Ok(ManageTrackItemsProposal {
        track,
        project,
        work_items,
        action,
    })
}

pub struct ManageTrackItems {
    pub trace: ToolTrace,
}

impl Tool for ManageTrackItems {
    const NAME: &'static str = MANAGE_TRACK_ITEMS_NAME;
    type Args = ManageTrackItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose adding or removing work items in a track. Only call this when \
         the user asks to organize work into tracks. The track name (or id) and \
         at least one work item identifier are required; never guess them. Pass \
         the project when the track name is not unique. The user must confirm \
         the card in the UI before anything is saved. Never claim the track was \
         changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageTrackItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_track_items_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageTrackItemsProposal serializes"))
    }
}

/// Map a mutation tool name to its proposal `kind`.
pub fn mutation_kind_for_tool(name: &str) -> Option<&'static str> {
    match name {
        UPDATE_WORK_ITEM_NAME => Some("update_work_item"),
        ADD_COMMENT_NAME => Some("add_comment"),
        MANAGE_SERVICE_LINKS_NAME => Some("manage_service_links"),
        MANAGE_SPRINT_ITEMS_NAME => Some("manage_sprint_items"),
        MANAGE_TRACK_ITEMS_NAME => Some("manage_track_items"),
        CREATE_SERVICE_NAME => Some("create_service"),
        UPDATE_SERVICE_NAME => Some("update_service"),
        CREATE_SPRINT_NAME => Some("create_sprint"),
        UPDATE_SPRINT_NAME => Some("update_sprint"),
        CREATE_TRACK_NAME => Some("create_track"),
        UPDATE_TRACK_NAME => Some("update_track"),
        _ => None,
    }
}

pub struct UpdateWorkItem {
    pub trace: ToolTrace,
}

impl Tool for UpdateWorkItem {
    const NAME: &'static str = UPDATE_WORK_ITEM_NAME;
    type Args = UpdateWorkItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing work item. Only call this when the \
         user asks to change a work item's fields (title, description, priority, \
         state, assignees, labels, or dates). The work item identifier is \
         required; never guess it. Send only the fields that change. State, \
         assignee, and label names may be human-readable; the UI resolves them. \
         The user must confirm and may edit every field in the UI before \
         anything is saved. Never claim the work item was updated until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateWorkItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_work_item_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateWorkItemProposal serializes"))
    }
}

pub const SERVICE_NAME_MAX: usize = 255;
pub const SERVICE_DESCRIPTION_MAX: usize = 5000;
pub const SERVICE_URL_MAX: usize = 2048;
pub const MAX_MEMBER_REFS: usize = 10;
pub const SERVICE_STATUSES: [&str; 5] = ["active", "planned", "maintenance", "deprecated", "retired"];
pub const SERVICE_CRITICALITIES: [&str; 4] = ["critical", "high", "medium", "low"];
pub const SERVICE_TYPES: [&str; 4] = ["internal", "external", "infrastructure", "third_party"];
pub const MODULE_STATUSES: [&str; 6] = ["backlog", "planned", "in-progress", "paused", "completed", "cancelled"];

fn optional_bounded_text(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<Option<String>, ToolExecutionError> {
    let Some(text) = optional_text(value) else {
        return Ok(None);
    };
    if text.chars().count() > max {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must be at most {max} characters"
        )));
    }
    Ok(Some(text))
}

fn optional_url(value: Option<&str>, label: &str) -> Result<Option<String>, ToolExecutionError> {
    let Some(url) = optional_text(value) else {
        return Ok(None);
    };
    if url.chars().count() > SERVICE_URL_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must be at most {SERVICE_URL_MAX} characters"
        )));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must start with http:// or https://"
        )));
    }
    Ok(Some(url))
}

pub const CREATE_SERVICE_NAME: &str = "create_service";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateServiceArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Service name (1-255 characters), unique per project.
    pub name: String,
    /// Plain-text description (max 5000 characters).
    pub description: Option<String>,
    /// One of: active, planned, maintenance, deprecated, retired.
    pub status: Option<String>,
    /// One of: critical, high, medium, low.
    pub criticality: Option<String>,
    /// One of: internal, external, infrastructure, third_party.
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    /// Owner display name or email; resolved by the UI.
    pub owner: Option<String>,
    /// Repository URL starting with http:// or https://.
    pub repository_url: Option<String>,
    /// Documentation URL starting with http:// or https://.
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateServiceProposal {
    pub project: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criticality: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

/// Validate raw tool args into a normalized create-service proposal.
pub fn create_service_proposal_from_args(
    args: CreateServiceArgs,
) -> Result<CreateServiceProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let description =
        optional_bounded_text(args.description.as_deref(), "description", SERVICE_DESCRIPTION_MAX)?;
    let status = optional_enum_arg(args.status.as_deref(), &SERVICE_STATUSES, "status")?;
    let criticality =
        optional_enum_arg(args.criticality.as_deref(), &SERVICE_CRITICALITIES, "criticality")?;
    let service_type = optional_enum_arg(args.service_type.as_deref(), &SERVICE_TYPES, "type")?;
    let owner = optional_bounded_text(args.owner.as_deref(), "owner", CONTAINER_REF_MAX)?;
    let repository_url = optional_url(args.repository_url.as_deref(), "repository_url")?;
    let documentation_url = optional_url(args.documentation_url.as_deref(), "documentation_url")?;
    Ok(CreateServiceProposal {
        project,
        name,
        description,
        status,
        criticality,
        service_type,
        owner,
        repository_url,
        documentation_url,
    })
}

pub struct CreateService {
    pub trace: ToolTrace,
}

impl Tool for CreateService {
    const NAME: &'static str = CREATE_SERVICE_NAME;
    type Args = CreateServiceArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one service in a project. Only call this when the user \
         asks to create a service. The project and service name are required; \
         never guess them. Status, criticality, type, owner, and URLs are \
         optional. The user must confirm and may edit every field in the UI \
         before anything is saved. Never claim the service was created until \
         they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateServiceArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_service_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateServiceProposal serializes"))
    }
}

fn required_when_present(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Err(ToolExecutionError::invalid_args(format!(
            "{label} must not be empty when provided"
        ))),
        Some(text) if text.chars().count() > max => Err(ToolExecutionError::invalid_args(
            format!("{label} must be at most {max} characters"),
        )),
        Some(text) => Ok(Some(text.to_string())),
    }
}

/// Optional text where a provided blank value means "clear the field".
fn clearable_text(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Ok(Some(String::new())),
        Some(text) if text.chars().count() > max => Err(ToolExecutionError::invalid_args(
            format!("{label} must be at most {max} characters"),
        )),
        Some(text) => Ok(Some(text.to_string())),
    }
}

/// Optional URL where blank clears; non-blank must be http(s).
fn clearable_url(value: Option<&str>, label: &str) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Ok(Some(String::new())),
        Some(url) if url.chars().count() > SERVICE_URL_MAX => Err(ToolExecutionError::invalid_args(
            format!("{label} must be at most {SERVICE_URL_MAX} characters"),
        )),
        Some(url) if !(url.starts_with("http://") || url.starts_with("https://")) => Err(
            ToolExecutionError::invalid_args(format!("{label} must start with http:// or https://")),
        ),
        Some(url) => Ok(Some(url.to_string())),
    }
}

pub const UPDATE_SERVICE_NAME: &str = "update_service";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateServiceChanges {
    /// New status: active, planned, maintenance, deprecated, retired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// New criticality: critical, high, medium, low.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criticality: Option<String>,
    /// New type: internal, external, infrastructure, third_party.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    /// New owner display name or email; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// New plain-text description; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New repository URL; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    /// New documentation URL; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

fn normalize_service_changes(
    changes: UpdateServiceChanges,
) -> Result<UpdateServiceChanges, ToolExecutionError> {
    let normalized = UpdateServiceChanges {
        status: optional_enum_arg(changes.status.as_deref(), &SERVICE_STATUSES, "status")?,
        criticality: optional_enum_arg(
            changes.criticality.as_deref(),
            &SERVICE_CRITICALITIES,
            "criticality",
        )?,
        service_type: optional_enum_arg(changes.service_type.as_deref(), &SERVICE_TYPES, "type")?,
        owner: clearable_text(changes.owner.as_deref(), "owner", CONTAINER_REF_MAX)?,
        description: clearable_text(
            changes.description.as_deref(),
            "description",
            SERVICE_DESCRIPTION_MAX,
        )?,
        repository_url: clearable_url(changes.repository_url.as_deref(), "repository_url")?,
        documentation_url: clearable_url(changes.documentation_url.as_deref(), "documentation_url")?,
    };
    if normalized == UpdateServiceChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateServiceArgs {
    /// Service name or id. Required.
    pub service: String,
    /// Project identifier or name; use it when the service name is not unique.
    pub project: Option<String>,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateServiceChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateServiceProposal {
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub changes: UpdateServiceChanges,
}

/// Validate raw tool args into a normalized update-service proposal.
pub fn update_service_proposal_from_args(
    args: UpdateServiceArgs,
) -> Result<UpdateServiceProposal, ToolExecutionError> {
    let service = required_text(&args.service, "service", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let changes = normalize_service_changes(args.changes)?;
    Ok(UpdateServiceProposal {
        service,
        project,
        changes,
    })
}

pub struct UpdateService {
    pub trace: ToolTrace,
}

impl Tool for UpdateService {
    const NAME: &'static str = UPDATE_SERVICE_NAME;
    type Args = UpdateServiceArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing service. Only call this when the \
         user asks to change a service's status, criticality, type, owner, \
         description, or URLs. The service name or id is required; never guess \
         it. Send only the fields that change; an empty string clears a field. \
         The user must confirm and may edit every field in the UI before \
         anything is saved. Never claim the service was updated until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateServiceArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_service_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateServiceProposal serializes"))
    }
}

fn date_change(
    value: Option<&str>,
    label: &str,
    allow_clear: bool,
) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") if allow_clear => Ok(Some(String::new())),
        Some("") => Err(ToolExecutionError::invalid_args(format!(
            "{label} must not be empty when provided"
        ))),
        Some(text) => parse_iso_date(text, label).map(|date| Some(date.to_string())),
    }
}

pub const CREATE_SPRINT_NAME: &str = "create_sprint";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSprintArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Sprint name (1-255 characters).
    pub name: String,
    /// Plain-text description (max 5000 characters).
    pub description: Option<String>,
    /// Start date "YYYY-MM-DD"; provide both dates or neither.
    pub start_date: Option<String>,
    /// End date "YYYY-MM-DD"; provide both dates or neither.
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateSprintProposal {
    pub project: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

/// Validate raw tool args into a normalized create-sprint proposal.
pub fn create_sprint_proposal_from_args(
    args: CreateSprintArgs,
) -> Result<CreateSprintProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let description =
        optional_bounded_text(args.description.as_deref(), "description", SERVICE_DESCRIPTION_MAX)?;
    let start_date = date_change(args.start_date.as_deref(), "start_date", false)?;
    let end_date = date_change(args.end_date.as_deref(), "end_date", false)?;
    if start_date.is_some() != end_date.is_some() {
        return Err(ToolExecutionError::invalid_args(
            "provide both start_date and end_date or neither",
        ));
    }
    if let (Some(start), Some(end)) = (&start_date, &end_date) {
        if start > end {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after end_date",
            ));
        }
    }
    Ok(CreateSprintProposal {
        project,
        name,
        description,
        start_date,
        end_date,
    })
}

pub struct CreateSprint {
    pub trace: ToolTrace,
}

impl Tool for CreateSprint {
    const NAME: &'static str = CREATE_SPRINT_NAME;
    type Args = CreateSprintArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one sprint (cycle) in a project. Only call this when \
         the user asks to create a sprint. The project and sprint name are \
         required; never guess them. Provide both start and end dates or \
         neither. The sprint owner is always the confirming user. The user must \
         confirm and may edit every field in the UI before anything is saved. \
         Never claim the sprint was created until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateSprintArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_sprint_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateSprintProposal serializes"))
    }
}

pub const UPDATE_SPRINT_NAME: &str = "update_sprint";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSprintChanges {
    /// New sprint name (1-255 characters); must not be blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New plain-text description; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New start date "YYYY-MM-DD"; cannot be cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// New end date "YYYY-MM-DD"; cannot be cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

fn normalize_sprint_changes(
    changes: UpdateSprintChanges,
) -> Result<UpdateSprintChanges, ToolExecutionError> {
    let normalized = UpdateSprintChanges {
        name: required_when_present(changes.name.as_deref(), "name", SERVICE_NAME_MAX)?,
        description: clearable_text(
            changes.description.as_deref(),
            "description",
            SERVICE_DESCRIPTION_MAX,
        )?,
        start_date: date_change(changes.start_date.as_deref(), "start_date", false)?,
        end_date: date_change(changes.end_date.as_deref(), "end_date", false)?,
    };
    if normalized == UpdateSprintChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    if let (Some(start), Some(end)) = (&normalized.start_date, &normalized.end_date) {
        if start > end {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after end_date",
            ));
        }
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSprintArgs {
    /// Sprint name or id. Required.
    pub sprint: String,
    /// Project identifier or name; use it when the sprint name is not unique.
    pub project: Option<String>,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateSprintChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateSprintProposal {
    pub sprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub changes: UpdateSprintChanges,
}

/// Validate raw tool args into a normalized update-sprint proposal.
pub fn update_sprint_proposal_from_args(
    args: UpdateSprintArgs,
) -> Result<UpdateSprintProposal, ToolExecutionError> {
    let sprint = required_text(&args.sprint, "sprint", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let changes = normalize_sprint_changes(args.changes)?;
    Ok(UpdateSprintProposal {
        sprint,
        project,
        changes,
    })
}

pub struct UpdateSprint {
    pub trace: ToolTrace,
}

impl Tool for UpdateSprint {
    const NAME: &'static str = UPDATE_SPRINT_NAME;
    type Args = UpdateSprintArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing sprint. Only call this when the \
         user asks to change a sprint's name, description, or dates. The sprint \
         name or id is required; never guess it. Send only the fields that \
         change; an empty description clears it and dates cannot be cleared. \
         Pass the project when the sprint name is not unique. The user must \
         confirm and may edit every field in the UI before anything is saved. \
         Never claim the sprint was updated until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateSprintArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_sprint_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateSprintProposal serializes"))
    }
}

pub const CREATE_TRACK_NAME: &str = "create_track";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTrackArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Track (module) name (1-255 characters), unique per project.
    pub name: String,
    /// Plain-text description (max 5000 characters).
    pub description: Option<String>,
    /// Start date "YYYY-MM-DD".
    pub start_date: Option<String>,
    /// Target date "YYYY-MM-DD".
    pub target_date: Option<String>,
    /// One of: backlog, planned, in-progress, paused, completed, cancelled.
    pub status: Option<String>,
    /// Lead display name or email; resolved by the UI.
    pub lead: Option<String>,
    /// Member display names or emails (max 10); resolved by the UI.
    pub members: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateTrackProposal {
    pub project: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    pub members: Vec<String>,
}

/// Validate raw tool args into a normalized create-track proposal.
pub fn create_track_proposal_from_args(
    args: CreateTrackArgs,
) -> Result<CreateTrackProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let description =
        optional_bounded_text(args.description.as_deref(), "description", SERVICE_DESCRIPTION_MAX)?;
    let start_date = date_change(args.start_date.as_deref(), "start_date", false)?;
    let target_date = date_change(args.target_date.as_deref(), "target_date", false)?;
    if let (Some(start), Some(target)) = (&start_date, &target_date) {
        if start > target {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    let status = optional_enum_arg(args.status.as_deref(), &MODULE_STATUSES, "status")?;
    let lead = optional_bounded_text(args.lead.as_deref(), "lead", CONTAINER_REF_MAX)?;
    let members = bounded_ref_list(args.members, "members")?;
    Ok(CreateTrackProposal {
        project,
        name,
        description,
        start_date,
        target_date,
        status,
        lead,
        members,
    })
}

pub struct CreateTrack {
    pub trace: ToolTrace,
}

impl Tool for CreateTrack {
    const NAME: &'static str = CREATE_TRACK_NAME;
    type Args = CreateTrackArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one track (module) in a project. Only call this when \
         the user asks to create a track. The project and track name are \
         required; never guess them. Dates, status, lead, and members are \
         optional; lead and member names are human-readable and resolved by the \
         UI. The user must confirm and may edit every field in the UI before \
         anything is saved. Never claim the track was created until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateTrackArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_track_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateTrackProposal serializes"))
    }
}

pub const UPDATE_TRACK_NAME: &str = "update_track";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTrackChanges {
    /// New track name (1-255 characters); must not be blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New plain-text description; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New start date "YYYY-MM-DD"; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// New target date "YYYY-MM-DD"; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
    /// New status: backlog, planned, in-progress, paused, completed, cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// New lead display name or email; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    /// New member display names or emails; an empty list clears them (max 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<String>>,
}

fn normalize_track_changes(
    changes: UpdateTrackChanges,
) -> Result<UpdateTrackChanges, ToolExecutionError> {
    let normalized = UpdateTrackChanges {
        name: required_when_present(changes.name.as_deref(), "name", SERVICE_NAME_MAX)?,
        description: clearable_text(
            changes.description.as_deref(),
            "description",
            SERVICE_DESCRIPTION_MAX,
        )?,
        start_date: date_change(changes.start_date.as_deref(), "start_date", true)?,
        target_date: date_change(changes.target_date.as_deref(), "target_date", true)?,
        status: optional_enum_arg(changes.status.as_deref(), &MODULE_STATUSES, "status")?,
        lead: clearable_text(changes.lead.as_deref(), "lead", CONTAINER_REF_MAX)?,
        members: bounded_ref_list_opt(changes.members, "members")?,
    };
    if normalized == UpdateTrackChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    if let (Some(start), Some(target)) = (&normalized.start_date, &normalized.target_date) {
        if !start.is_empty() && !target.is_empty() && start > target {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTrackArgs {
    /// Track (module) name or id. Required.
    pub track: String,
    /// Project identifier or name; use it when the track name is not unique.
    pub project: Option<String>,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateTrackChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateTrackProposal {
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub changes: UpdateTrackChanges,
}

/// Validate raw tool args into a normalized update-track proposal.
pub fn update_track_proposal_from_args(
    args: UpdateTrackArgs,
) -> Result<UpdateTrackProposal, ToolExecutionError> {
    let track = required_text(&args.track, "track", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let changes = normalize_track_changes(args.changes)?;
    Ok(UpdateTrackProposal {
        track,
        project,
        changes,
    })
}

pub struct UpdateTrack {
    pub trace: ToolTrace,
}

impl Tool for UpdateTrack {
    const NAME: &'static str = UPDATE_TRACK_NAME;
    type Args = UpdateTrackArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing track. Only call this when the \
         user asks to change a track's name, description, dates, status, lead, \
         or members. The track name or id is required; never guess it. Send \
         only the fields that change; empty strings and empty member lists \
         clear those fields. Pass the project when the track name is not \
         unique. The user must confirm and may edit every field in the UI \
         before anything is saved. Never claim the track was updated until \
         they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateTrackArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_track_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateTrackProposal serializes"))
    }
}

pub const CREATE_ARTICLE_NAME: &str = "create_article";
pub const ARTICLE_CONTENT_MAX: usize = 20_000;
pub const ARTICLE_ACCESSES: [&str; 2] = ["public", "private"];

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArticleArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Article title (1-255 characters).
    pub name: String,
    /// Plain-text body (1-20000 characters); the UI converts it to rich text.
    pub content: String,
    /// Parent article (page) id, when the user wants a sub-page.
    pub parent_article: Option<String>,
    /// One of: public, private. Defaults to public.
    pub access: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateArticleProposal {
    pub project: String,
    pub name: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_article: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
}

/// Validate raw tool args into a normalized create-article proposal.
pub fn create_article_proposal_from_args(
    args: CreateArticleArgs,
) -> Result<CreateArticleProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let content = required_text(&args.content, "content", ARTICLE_CONTENT_MAX)?;
    let parent_article = match args.parent_article.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => {
            let parsed = Uuid::parse_str(raw).map_err(|_| {
                ToolExecutionError::invalid_args("parent_article must be a page uuid")
            })?;
            Some(parsed.to_string())
        }
    };
    let access = optional_enum_arg(args.access.as_deref(), &ARTICLE_ACCESSES, "access")?;
    Ok(CreateArticleProposal {
        project,
        name,
        content,
        parent_article,
        access,
    })
}

pub struct CreateArticle {
    pub trace: ToolTrace,
}

impl Tool for CreateArticle {
    const NAME: &'static str = CREATE_ARTICLE_NAME;
    type Args = CreateArticleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one knowledge base article (page) in a project. Only \
         call this when the user asks to write an article. The project, title, \
         and body text are required; never guess them. Access defaults to \
         public. The user must confirm and may edit the article in the UI \
         before anything is saved. Never claim the article was created until \
         they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateArticleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_article_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateArticleProposal serializes"))
    }
}

pub const UPDATE_ARTICLE_NAME: &str = "update_article";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArticleArgs {
    /// Article (page) id, a uuid.
    pub article: String,
    /// Project identifier or name; use it when the article id is not unique.
    pub project: Option<String>,
    /// One of: append, replace. Required so a body is never replaced by accident.
    pub action: String,
    /// New title (1-255 characters).
    pub name: Option<String>,
    /// New plain-text body (1-20000 characters).
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateArticleProposal {
    pub article: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Validate raw tool args into a normalized update-article proposal.
pub fn update_article_proposal_from_args(
    args: UpdateArticleArgs,
) -> Result<UpdateArticleProposal, ToolExecutionError> {
    let article = Uuid::parse_str(args.article.trim())
        .map_err(|_| ToolExecutionError::invalid_args("article must be a page uuid"))?
        .to_string();
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["append", "replace"], "action")?;
    let name = match args.name.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "name must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > SERVICE_NAME_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "name must be at most {SERVICE_NAME_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let content = match args.content.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "content must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > ARTICLE_CONTENT_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "content must be at most {ARTICLE_CONTENT_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    if name.is_none() && content.is_none() {
        return Err(ToolExecutionError::invalid_args(
            "at least one of name or content is required",
        ));
    }
    Ok(UpdateArticleProposal {
        article,
        project,
        action,
        name,
        content,
    })
}

pub struct UpdateArticle {
    pub trace: ToolTrace,
}

impl Tool for UpdateArticle {
    const NAME: &'static str = UPDATE_ARTICLE_NAME;
    type Args = UpdateArticleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing one knowledge base article (page). Only call this when \
         the user asks to change an article's title or body. The article id is \
         required; never guess it. The action is required: append adds the text \
         at the end, replace swaps the whole body. Pass the project when known. \
         The user must confirm and may edit the article in the UI before \
         anything is saved. Never claim the article was updated until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateArticleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_article_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateArticleProposal serializes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_args(changes: UpdateWorkItemChanges) -> UpdateWorkItemArgs {
        UpdateWorkItemArgs {
            work_item: "LTS-42".to_string(),
            changes,
        }
    }

    #[test]
    fn update_work_item_requires_at_least_one_change() {
        let err =
            update_work_item_proposal_from_args(update_args(UpdateWorkItemChanges::default()))
                .unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn update_work_item_normalizes_text_refs_and_dates() {
        let proposal = update_work_item_proposal_from_args(update_args(UpdateWorkItemChanges {
            name: Some("  Pump failure  ".into()),
            priority: Some("HIGH".into()),
            assignees: Some(vec![" Budi ".into(), "budi".into(), "Sari".into()]),
            labels: Some(vec![]),
            start_date: Some("2026-10-01".into()),
            target_date: Some("2026-10-05".into()),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(proposal.work_item, "LTS-42");
        assert_eq!(proposal.changes.name.as_deref(), Some("Pump failure"));
        assert_eq!(proposal.changes.priority.as_deref(), Some("high"));
        assert_eq!(
            proposal.changes.assignees,
            Some(vec!["Budi".to_string(), "Sari".to_string()])
        );
        assert_eq!(proposal.changes.labels, Some(vec![]));
        assert_eq!(proposal.changes.start_date.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn update_work_item_rejects_invalid_values() {
        let blank_name = update_work_item_proposal_from_args(update_args(
            UpdateWorkItemChanges {
                name: Some("   ".into()),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));

        let bad_priority = update_work_item_proposal_from_args(update_args(
            UpdateWorkItemChanges {
                priority: Some("p0".into()),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(bad_priority.to_string().contains("priority"));

        let bad_dates = update_work_item_proposal_from_args(update_args(
            UpdateWorkItemChanges {
                start_date: Some("2026-10-05".into()),
                target_date: Some("2026-10-01".into()),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(bad_dates.to_string().contains("start_date"));

        let bad_ref = update_work_item_proposal_from_args(UpdateWorkItemArgs {
            work_item: "LTS".into(),
            changes: UpdateWorkItemChanges {
                name: Some("x".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));
    }

    #[test]
    fn mutation_kind_mapping_covers_update_work_item() {
        assert_eq!(
            mutation_kind_for_tool(UPDATE_WORK_ITEM_NAME),
            Some("update_work_item")
        );
        assert_eq!(mutation_kind_for_tool("search_work_items"), None);
    }

    #[test]
    fn add_comment_normalizes_and_bounds() {
        let proposal = add_comment_proposal_from_args(AddCommentArgs {
            work_item: " lts-7 ".into(),
            comment: "  replaced the filter  ".into(),
        })
        .unwrap();
        assert_eq!(proposal.work_item, "lts-7");
        assert_eq!(proposal.comment, "replaced the filter");

        let empty = add_comment_proposal_from_args(AddCommentArgs {
            work_item: "LTS-7".into(),
            comment: "   ".into(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("comment"));

        let too_long = add_comment_proposal_from_args(AddCommentArgs {
            work_item: "LTS-7".into(),
            comment: "a".repeat(MAX_COMMENT_CHARS + 1),
        })
        .unwrap_err();
        assert!(too_long.to_string().contains("at most"));

        let bad_ref = add_comment_proposal_from_args(AddCommentArgs {
            work_item: "nope".into(),
            comment: "hi".into(),
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));
    }

    #[test]
    fn mutation_kind_mapping_covers_add_comment() {
        assert_eq!(
            mutation_kind_for_tool(ADD_COMMENT_NAME),
            Some("add_comment")
        );
    }

    #[test]
    fn service_links_normalize_and_validate() {
        let proposal = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: " LTS-7 ".into(),
            services: vec![" Email ".into(), "email".into(), "VPN".into()],
            action: " LINK ".into(),
        })
        .unwrap();
        assert_eq!(proposal.work_item, "LTS-7");
        assert_eq!(
            proposal.services,
            vec!["Email".to_string(), "VPN".to_string()]
        );
        assert_eq!(proposal.action, "link");

        let no_services = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: "LTS-7".into(),
            services: vec![],
            action: "link".into(),
        })
        .unwrap_err();
        assert!(no_services.to_string().contains("service"));

        let too_many = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: "LTS-7".into(),
            services: (0..=MAX_SERVICE_REFS)
                .map(|index| format!("svc-{index}"))
                .collect(),
            action: "link".into(),
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("at most"));

        let bad_action = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: "LTS-7".into(),
            services: vec!["Email".into()],
            action: "attach".into(),
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("link, unlink"));
    }

    #[test]
    fn sprint_items_normalize_and_validate() {
        let proposal = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: " Sprint 3 ".into(),
            project: Some(" LTS ".into()),
            work_items: vec![" lts-1 ".into(), "LTS-1".into(), "LTS-2".into()],
            action: " Add ".into(),
        })
        .unwrap();
        assert_eq!(proposal.sprint, "Sprint 3");
        assert_eq!(proposal.project.as_deref(), Some("LTS"));
        assert_eq!(
            proposal.work_items,
            vec!["lts-1".to_string(), "LTS-2".to_string()]
        );
        assert_eq!(proposal.action, "add");

        let blank_sprint = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "   ".into(),
            project: None,
            work_items: vec!["LTS-1".into()],
            action: "add".into(),
        })
        .unwrap_err();
        assert!(blank_sprint.to_string().contains("sprint"));

        let empty_items = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: vec![],
            action: "add".into(),
        })
        .unwrap_err();
        assert!(empty_items.to_string().contains("work item"));

        let bad_ref = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: vec!["nope".into()],
            action: "add".into(),
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));

        let too_many = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: (0..=MAX_BULK_ITEMS)
                .map(|index| format!("LTS-{index}"))
                .collect(),
            action: "add".into(),
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("at most"));

        let bad_action = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: vec!["LTS-1".into()],
            action: "attach".into(),
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("add, remove"));
    }

    #[test]
    fn track_items_normalize_and_validate() {
        let proposal = manage_track_items_proposal_from_args(ManageTrackItemsArgs {
            track: " Onboarding ".into(),
            project: None,
            work_items: vec!["LTS-9".into()],
            action: " REMOVE ".into(),
        })
        .unwrap();
        assert_eq!(proposal.track, "Onboarding");
        assert_eq!(proposal.project, None);
        assert_eq!(proposal.work_items, vec!["LTS-9".to_string()]);
        assert_eq!(proposal.action, "remove");

        let blank_track = manage_track_items_proposal_from_args(ManageTrackItemsArgs {
            track: "  ".into(),
            project: None,
            work_items: vec!["LTS-9".into()],
            action: "remove".into(),
        })
        .unwrap_err();
        assert!(blank_track.to_string().contains("track"));

        let bad_action = manage_track_items_proposal_from_args(ManageTrackItemsArgs {
            track: "Onboarding".into(),
            project: None,
            work_items: vec!["LTS-9".into()],
            action: "detach".into(),
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("add, remove"));
    }

    #[test]
    fn mutation_kind_mapping_covers_link_tools() {
        assert_eq!(
            mutation_kind_for_tool(MANAGE_SERVICE_LINKS_NAME),
            Some("manage_service_links")
        );
        assert_eq!(
            mutation_kind_for_tool(MANAGE_SPRINT_ITEMS_NAME),
            Some("manage_sprint_items")
        );
        assert_eq!(
            mutation_kind_for_tool(MANAGE_TRACK_ITEMS_NAME),
            Some("manage_track_items")
        );
    }

    #[test]
    fn mutation_kind_mapping_covers_create_and_update_tools() {
        assert_eq!(
            mutation_kind_for_tool(CREATE_SERVICE_NAME),
            Some("create_service")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_SERVICE_NAME),
            Some("update_service")
        );
        assert_eq!(
            mutation_kind_for_tool(CREATE_SPRINT_NAME),
            Some("create_sprint")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_SPRINT_NAME),
            Some("update_sprint")
        );
        assert_eq!(
            mutation_kind_for_tool(CREATE_TRACK_NAME),
            Some("create_track")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_TRACK_NAME),
            Some("update_track")
        );
    }

    #[test]
    fn create_service_normalizes_and_validates() {
        let proposal = create_service_proposal_from_args(CreateServiceArgs {
            project: " LTS ".into(),
            name: " Email Gateway ".into(),
            description: Some("  Handles mail  ".into()),
            status: Some(" ACTIVE ".into()),
            criticality: Some("High".into()),
            service_type: Some("internal".into()),
            owner: Some(" Budi ".into()),
            repository_url: Some(" https://git.example.com/email ".into()),
            documentation_url: None,
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Email Gateway");
        assert_eq!(proposal.description.as_deref(), Some("Handles mail"));
        assert_eq!(proposal.status.as_deref(), Some("active"));
        assert_eq!(proposal.criticality.as_deref(), Some("high"));
        assert_eq!(proposal.service_type.as_deref(), Some("internal"));
        assert_eq!(proposal.owner.as_deref(), Some("Budi"));
        assert_eq!(
            proposal.repository_url.as_deref(),
            Some("https://git.example.com/email")
        );
        assert_eq!(proposal.documentation_url, None);

        let blank_name = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "  ".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: None,
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));

        let bad_status = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Email".into(),
            description: None,
            status: Some("broken".into()),
            criticality: None,
            service_type: None,
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap_err();
        assert!(bad_status.to_string().contains("active"));

        let bad_url = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Email".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: None,
            owner: None,
            repository_url: Some("git.example.com".into()),
            documentation_url: None,
        })
        .unwrap_err();
        assert!(bad_url.to_string().contains("http"));
    }

    #[test]
    fn create_service_proposal_serializes_type_key() {
        let proposal = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Email".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: Some("internal".into()),
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap();
        let value = serde_json::to_value(&proposal).unwrap();
        assert_eq!(value["type"], serde_json::json!("internal"));
        assert!(value.get("service_type").is_none());
    }

    #[test]
    fn update_service_normalizes_clears_and_requires_a_change() {
        let proposal = update_service_proposal_from_args(UpdateServiceArgs {
            service: " Email ".into(),
            project: Some(" LTS ".into()),
            changes: UpdateServiceChanges {
                status: Some(" DEPRECATED ".into()),
                owner: Some("".into()),
                repository_url: Some("".into()),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(proposal.service, "Email");
        assert_eq!(proposal.project.as_deref(), Some("LTS"));
        assert_eq!(proposal.changes.status.as_deref(), Some("deprecated"));
        assert_eq!(proposal.changes.owner.as_deref(), Some(""));
        assert_eq!(proposal.changes.repository_url.as_deref(), Some(""));

        let empty = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Email".into(),
            project: None,
            changes: UpdateServiceChanges::default(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let blank_service = update_service_proposal_from_args(UpdateServiceArgs {
            service: "  ".into(),
            project: None,
            changes: UpdateServiceChanges {
                status: Some("active".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_service.to_string().contains("service"));

        let bad_type = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Email".into(),
            project: None,
            changes: UpdateServiceChanges {
                service_type: Some("legacy".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_type.to_string().contains("internal"));

        let bad_url = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Email".into(),
            project: None,
            changes: UpdateServiceChanges {
                documentation_url: Some("docs.example.com".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_url.to_string().contains("http"));
    }

    #[test]
    fn create_sprint_requires_paired_dates() {
        let proposal = create_sprint_proposal_from_args(CreateSprintArgs {
            project: " LTS ".into(),
            name: " Sprint 4 ".into(),
            description: Some(" Q4 hardening ".into()),
            start_date: Some("2026-11-01".into()),
            end_date: Some("2026-11-14".into()),
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Sprint 4");
        assert_eq!(proposal.description.as_deref(), Some("Q4 hardening"));
        assert_eq!(proposal.start_date.as_deref(), Some("2026-11-01"));
        assert_eq!(proposal.end_date.as_deref(), Some("2026-11-14"));

        let single = create_sprint_proposal_from_args(CreateSprintArgs {
            project: "LTS".into(),
            name: "Sprint 4".into(),
            description: None,
            start_date: Some("2026-11-01".into()),
            end_date: None,
        })
        .unwrap_err();
        assert!(single.to_string().contains("both"));

        let reversed = create_sprint_proposal_from_args(CreateSprintArgs {
            project: "LTS".into(),
            name: "Sprint 4".into(),
            description: None,
            start_date: Some("2026-11-14".into()),
            end_date: Some("2026-11-01".into()),
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));

        let bad_date = create_sprint_proposal_from_args(CreateSprintArgs {
            project: "LTS".into(),
            name: "Sprint 4".into(),
            description: None,
            start_date: Some("01-11-2026".into()),
            end_date: Some("2026-11-14".into()),
        })
        .unwrap_err();
        assert!(bad_date.to_string().contains("YYYY-MM-DD"));
    }

    #[test]
    fn update_sprint_allows_single_date_and_rejects_blank() {
        let proposal = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: " Sprint 4 ".into(),
            project: None,
            changes: UpdateSprintChanges {
                end_date: Some("2026-11-21".into()),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(proposal.sprint, "Sprint 4");
        assert_eq!(proposal.changes.end_date.as_deref(), Some("2026-11-21"));

        let empty = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges::default(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let blank_date = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges {
                start_date: Some("".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_date.to_string().contains("must not be empty"));

        let blank_name = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges {
                name: Some("  ".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));

        let reversed = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges {
                start_date: Some("2026-11-21".into()),
                end_date: Some("2026-11-14".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));
    }

    #[test]
    fn create_track_normalizes_and_bounds_members() {
        let proposal = create_track_proposal_from_args(CreateTrackArgs {
            project: " LTS ".into(),
            name: " Onboarding ".into(),
            description: Some(" New joiner path ".into()),
            start_date: Some("2026-11-01".into()),
            target_date: Some("2026-12-01".into()),
            status: Some(" PLANNED ".into()),
            lead: Some(" Budi ".into()),
            members: Some(vec![" Budi ".into(), "budi".into(), "Sari".into()]),
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Onboarding");
        assert_eq!(proposal.status.as_deref(), Some("planned"));
        assert_eq!(proposal.lead.as_deref(), Some("Budi"));
        assert_eq!(
            proposal.members,
            vec!["Budi".to_string(), "Sari".to_string()]
        );

        let bad_status = create_track_proposal_from_args(CreateTrackArgs {
            project: "LTS".into(),
            name: "Onboarding".into(),
            description: None,
            start_date: None,
            target_date: None,
            status: Some("live".into()),
            lead: None,
            members: None,
        })
        .unwrap_err();
        assert!(bad_status.to_string().contains("backlog"));

        let reversed = create_track_proposal_from_args(CreateTrackArgs {
            project: "LTS".into(),
            name: "Onboarding".into(),
            description: None,
            start_date: Some("2026-12-01".into()),
            target_date: Some("2026-11-01".into()),
            status: None,
            lead: None,
            members: None,
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));

        let too_many = create_track_proposal_from_args(CreateTrackArgs {
            project: "LTS".into(),
            name: "Onboarding".into(),
            description: None,
            start_date: None,
            target_date: None,
            status: None,
            lead: None,
            members: Some((0..=MAX_MEMBER_REFS).map(|index| format!("u{index}")).collect()),
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("at most"));
    }

    #[test]
    fn update_track_supports_clears_and_member_replace() {
        let proposal = update_track_proposal_from_args(UpdateTrackArgs {
            track: " Onboarding ".into(),
            project: None,
            changes: UpdateTrackChanges {
                description: Some("".into()),
                start_date: Some("".into()),
                target_date: Some("".into()),
                lead: Some("".into()),
                members: Some(vec![]),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(proposal.track, "Onboarding");
        assert_eq!(proposal.changes.description.as_deref(), Some(""));
        assert_eq!(proposal.changes.start_date.as_deref(), Some(""));
        assert_eq!(proposal.changes.target_date.as_deref(), Some(""));
        assert_eq!(proposal.changes.lead.as_deref(), Some(""));
        assert_eq!(proposal.changes.members, Some(vec![]));

        let empty = update_track_proposal_from_args(UpdateTrackArgs {
            track: "Onboarding".into(),
            project: None,
            changes: UpdateTrackChanges::default(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let blank_name = update_track_proposal_from_args(UpdateTrackArgs {
            track: "Onboarding".into(),
            project: None,
            changes: UpdateTrackChanges {
                name: Some("  ".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));
    }

    #[test]
    fn create_article_normalizes_and_validates() {
        let proposal = create_article_proposal_from_args(CreateArticleArgs {
            project: " LTS ".into(),
            name: " Runbook: restart API ".into(),
            content: " Step 1\nStep 2 ".into(),
            parent_article: Some(" 0f3f3f3f-0000-0000-0000-000000000001 ".into()),
            access: Some(" PRIVATE ".into()),
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Runbook: restart API");
        assert_eq!(proposal.content, "Step 1\nStep 2");
        assert_eq!(
            proposal.parent_article.as_deref(),
            Some("0f3f3f3f-0000-0000-0000-000000000001")
        );
        assert_eq!(proposal.access.as_deref(), Some("private"));

        let blank_content = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "   ".into(),
            parent_article: None,
            access: None,
        })
        .unwrap_err();
        assert!(blank_content.to_string().contains("content"));

        let long_content = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "x".repeat(ARTICLE_CONTENT_MAX + 1),
            parent_article: None,
            access: None,
        })
        .unwrap_err();
        assert!(long_content.to_string().contains("at most"));

        let bad_parent = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "Step".into(),
            parent_article: Some("not-a-uuid".into()),
            access: None,
        })
        .unwrap_err();
        assert!(bad_parent.to_string().contains("parent_article"));

        let bad_access = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "Step".into(),
            parent_article: None,
            access: Some("secret".into()),
        })
        .unwrap_err();
        assert!(bad_access.to_string().contains("public"));
    }

    #[test]
    fn update_article_requires_action_and_a_change() {
        let proposal = update_article_proposal_from_args(UpdateArticleArgs {
            article: " 0f3f3f3f-0000-0000-0000-000000000002 ".into(),
            project: Some(" LTS ".into()),
            action: " APPEND ".into(),
            name: None,
            content: Some(" Step 3 ".into()),
        })
        .unwrap();
        assert_eq!(proposal.article, "0f3f3f3f-0000-0000-0000-000000000002");
        assert_eq!(proposal.project.as_deref(), Some("LTS"));
        assert_eq!(proposal.action, "append");
        assert_eq!(proposal.content.as_deref(), Some("Step 3"));

        let empty = update_article_proposal_from_args(UpdateArticleArgs {
            article: "0f3f3f3f-0000-0000-0000-000000000002".into(),
            project: None,
            action: "replace".into(),
            name: None,
            content: None,
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let bad_action = update_article_proposal_from_args(UpdateArticleArgs {
            article: "0f3f3f3f-0000-0000-0000-000000000002".into(),
            project: None,
            action: "rewrite".into(),
            name: Some("x".into()),
            content: None,
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("append"));

        let bad_article = update_article_proposal_from_args(UpdateArticleArgs {
            article: "LTS-7".into(),
            project: None,
            action: "append".into(),
            name: Some("x".into()),
            content: None,
        })
        .unwrap_err();
        assert!(bad_article.to_string().contains("uuid"));

        let blank_name = update_article_proposal_from_args(UpdateArticleArgs {
            article: "0f3f3f3f-0000-0000-0000-000000000002".into(),
            project: None,
            action: "append".into(),
            name: Some("  ".into()),
            content: None,
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));
    }
}
