//! Guided flow modes for the agent: registry, validation, and preamble
//! composition. One flow (Incident) ships today; adding another flow is a new
//! module plus one registry entry.

pub mod incident;

use sqlx::PgPool;
use uuid::Uuid;

use crate::tools::{parse_work_item_ref, resolve_work_item};

pub struct Flow {
    pub id: &'static str,
    pub command: &'static str,
    pub expected_type: &'static str,
    pub playbook: &'static str,
}

pub static INCIDENT: Flow = Flow {
    id: "incident",
    command: "/incident",
    expected_type: "Incident",
    playbook: incident::PLAYBOOK,
};

static FLOWS: &[&Flow] = &[&INCIDENT];

/// Resolve a flow id to its definition. The only lookup authority.
pub fn resolve(id: &str) -> Option<&'static Flow> {
    FLOWS.iter().copied().find(|flow| flow.id == id)
}

/// A validated active flow attached to one agent turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFlow {
    pub id: &'static str,
    pub reference: String,
}

/// A validation failure, carrying machine-readable `code` for the FE plus a
/// human-readable `message` for the chat bubble.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowError {
    pub code: &'static str,
    pub message: String,
}

pub const CODE_UNKNOWN: &str = "flow_unknown";
pub const CODE_INVALID: &str = "flow_invalid";
pub const CODE_REF_GONE: &str = "flow_ref_gone";
pub const CODE_TYPE_MISMATCH: &str = "flow_type_mismatch";

/// `PROJ-123` shape check, exposed for the metadata allowlist.
pub fn is_valid_reference(reference: &str) -> bool {
    parse_work_item_ref(reference).is_ok()
}

const WORK_ITEM_TYPE_SQL: &str = "SELECT t.name FROM issues i \
     LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
     WHERE i.id = $1";

/// Validate a requested flow turn. Order: registered flow -> reference shape ->
/// work item exists and is accessible -> work item type matches the flow.
/// Pure for the first two steps (unit-testable), DB-backed for the rest.
pub async fn validate(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    flow_id: &str,
    reference: &str,
) -> Result<ActiveFlow, FlowError> {
    let Some(flow) = resolve(flow_id) else {
        return Err(FlowError {
            code: CODE_UNKNOWN,
            message: format!("unknown flow '{flow_id}'"),
        });
    };
    if parse_work_item_ref(reference).is_err() {
        return Err(FlowError {
            code: CODE_INVALID,
            message: format!("'{reference}' is not a work item reference (e.g. OPS-123)"),
        });
    }
    let resolved = resolve_work_item(pool, workspace_id, user_id, reference)
        .await
        .map_err(|_| FlowError {
            code: CODE_REF_GONE,
            message: format!("work item '{reference}' was not found or is not accessible"),
        })?;
    let type_name: Option<String> = sqlx::query_scalar(WORK_ITEM_TYPE_SQL)
        .bind(resolved.id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "ai-flow: type lookup failed");
            FlowError {
                code: CODE_REF_GONE,
                message: format!("work item '{}' could not be loaded", resolved.identifier),
            }
        })?
        .flatten();
    match type_name {
        Some(name) if name.eq_ignore_ascii_case(flow.expected_type) => {}
        Some(name) => {
            return Err(FlowError {
                code: CODE_TYPE_MISMATCH,
                message: format!(
                    "{} is a {name}, not an {}",
                    resolved.identifier, flow.expected_type
                ),
            });
        }
        None => {
            return Err(FlowError {
                code: CODE_TYPE_MISMATCH,
                message: format!("{} has no work item type", resolved.identifier),
            });
        }
    }
    Ok(ActiveFlow {
        id: flow.id,
        reference: resolved.identifier,
    })
}

/// Append the flow block to the base preamble. Unknown ids fall back to the
/// base preamble; `validate` makes that unreachable in the route.
pub fn preamble_with_flow(base: &str, active: &ActiveFlow) -> String {
    match resolve(active.id) {
        Some(flow) => format!(
            "{base}\n\n# Guided flow: {label} {command}\nActive work item: {reference}\n\n{playbook}",
            label = flow.expected_type,
            command = flow.command,
            reference = active.reference,
            playbook = flow.playbook,
        ),
        None => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_finds_the_incident_flow() {
        let flow = resolve("incident").expect("incident resolves");
        assert_eq!(flow.command, "/incident");
        assert_eq!(flow.expected_type, "Incident");
    }

    #[test]
    fn resolve_rejects_unknown_flows() {
        assert!(resolve("problem").is_none());
        assert!(resolve("").is_none());
        assert!(resolve("INCIDENT").is_none());
    }

    #[test]
    fn incident_playbook_covers_every_locked_outcome_and_guardrail() {        let playbook = resolve("incident").unwrap().playbook;
        for marker in [
            "Orient",
            "Clarify",
            "Classification",
            "priority",
            "state",
            "labels",
            "Service link",
            "Assignee",
            "First response",
            "Review",
            "skip",
            "confirm",
        ] {
            assert!(
                playbook.to_lowercase().contains(&marker.to_lowercase()),
                "playbook must cover {marker}"
            );
        }
    }

    #[test]
    fn is_valid_reference_accepts_proj_123_and_rejects_junk() {
        assert!(is_valid_reference("OPS-123"));
        assert!(is_valid_reference("lts-7"));
        assert!(!is_valid_reference("OPS"));
        assert!(!is_valid_reference("OPS-"));
        assert!(!is_valid_reference("-1"));
        assert!(!is_valid_reference(""));
        assert!(!is_valid_reference("OPS-1; DROP TABLE issues"));
    }

    #[tokio::test]
    async fn validate_rejects_unknown_flows_before_touching_the_db() {
        let pool = crate::tools::lazy_pool();
        let error = validate(&pool, uuid::Uuid::nil(), uuid::Uuid::nil(), "problem", "OPS-1")
            .await
            .expect_err("unknown flow");
        assert_eq!(error.code, CODE_UNKNOWN);
    }

    #[tokio::test]
    async fn validate_rejects_malformed_references_before_touching_the_db() {
        let pool = crate::tools::lazy_pool();
        let error = validate(&pool, uuid::Uuid::nil(), uuid::Uuid::nil(), "incident", "not-a-ref")
            .await
            .expect_err("malformed ref");
        assert_eq!(error.code, CODE_INVALID);
    }

    #[test]
    fn preamble_with_flow_names_the_work_item_and_embeds_the_playbook() {
        let active = ActiveFlow {
            id: "incident",
            reference: "OPS-123".to_string(),
        };
        let preamble = preamble_with_flow("BASE", &active);
        assert!(preamble.starts_with("BASE\n\n"));
        assert!(preamble.contains("Active work item: OPS-123"));
        assert!(preamble.contains("Incident triage"));
        assert!(preamble.contains("proposal"));
    }
}
