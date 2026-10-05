//! Fase 2 mutation proposal tools. They never touch the database; the UI
//! renders a confirmation card from the recorded trace.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    bounded_ref_list_opt, parse_iso_date, parse_work_item_ref, priority_arg, schema_of,
    WORK_ITEM_DESCRIPTION_MAX, WORK_ITEM_NAME_MAX, WORK_ITEM_STATE_MAX,
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

/// Map a mutation tool name to its proposal `kind`. Plan 2B extends the match.
pub fn mutation_kind_for_tool(name: &str) -> Option<&'static str> {
    match name {
        UPDATE_WORK_ITEM_NAME => Some("update_work_item"),
        ADD_COMMENT_NAME => Some("add_comment"),
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
}
