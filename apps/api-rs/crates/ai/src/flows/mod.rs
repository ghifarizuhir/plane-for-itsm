//! Guided flow modes for the agent: registry, validation, and preamble
//! composition. One flow (Incident) ships today; adding another flow is a new
//! module plus one registry entry.

pub mod incident;

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
    fn incident_playbook_covers_every_locked_outcome_and_guardrail() {
        let playbook = resolve("incident").unwrap().playbook;
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
}
