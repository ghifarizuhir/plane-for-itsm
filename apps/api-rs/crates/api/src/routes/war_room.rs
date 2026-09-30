/// Allowed `status` values (`packages/types/src/war-room/core.ts`).
pub const WAR_ROOM_STATUSES: &[&str] = &["active", "monitoring", "resolved", "archived"];
/// Allowed `severity` values.
pub const WAR_ROOM_SEVERITIES: &[&str] = &["sev1", "sev2", "sev3", "sev4"];
/// Allowed participant roles (coordination labels, not permission gates).
pub const PARTICIPANT_ROLES: &[&str] = &["commander", "comms", "scribe", "responder"];

/// Default severity derived from the incident priority.
pub fn severity_from_priority(priority: &str) -> &'static str {
    match priority {
        "urgent" => "sev1",
        "high" => "sev2",
        "medium" => "sev3",
        _ => "sev4",
    }
}

pub fn is_active_status(status: &str) -> bool {
    status == "active" || status == "monitoring"
}

/// Allowed status transitions; `archived` is terminal.
pub fn transitions_allowed(status: &str) -> &'static [&'static str] {
    match status {
        "active" => &["monitoring", "resolved", "archived"],
        "monitoring" => &["active", "resolved", "archived"],
        "resolved" => &["active", "archived"],
        _ => &[],
    }
}

pub fn status_transition_allowed(from: &str, to: &str) -> bool {
    transitions_allowed(from).contains(&to)
}

/// Built-in runbook per work item type name (lowercased, trimmed).
/// Returns `(template_key, title)` pairs.
pub fn runbook_template(type_name: Option<&str>) -> Vec<(&'static str, &'static str)> {
    match type_name.map(|n| n.trim().to_lowercase()).as_deref() {
        Some("incident") => vec![
            ("triage", "Triage & assess impact"),
            ("mitigate", "Mitigate (rollback/redeploy)"),
            ("communicate", "Communicate status update"),
            ("verify", "Verify recovery & monitor"),
            ("postmortem", "Schedule postmortem"),
        ],
        Some("problem") => vec![
            ("confirm_cause", "Confirm root cause hypothesis"),
            ("evidence", "Collect evidence & timeline"),
            ("fix", "Identify permanent fix"),
            ("change_plan", "Create change plan"),
            ("knowledge", "Update knowledge base"),
        ],
        Some("change") => vec![
            ("pre_verify", "Pre-change verification"),
            ("execute", "Execute change steps"),
            ("validate", "Validate service health"),
            ("rollback", "Rollback if needed"),
            ("close", "Close change record"),
        ],
        Some("request") => vec![
            ("requester", "Confirm requester details"),
            ("steps", "Check fulfilment steps"),
            ("fulfil", "Execute fulfilment"),
            ("notify", "Notify requester"),
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_mapping_follows_priority() {
        assert_eq!(severity_from_priority("urgent"), "sev1");
        assert_eq!(severity_from_priority("high"), "sev2");
        assert_eq!(severity_from_priority("medium"), "sev3");
        assert_eq!(severity_from_priority("low"), "sev4");
        assert_eq!(severity_from_priority("none"), "sev4");
        assert_eq!(severity_from_priority("bogus"), "sev4");
    }

    #[test]
    fn transition_map_is_terminal_on_archived() {
        assert!(status_transition_allowed("active", "monitoring"));
        assert!(status_transition_allowed("monitoring", "active"));
        assert!(status_transition_allowed("active", "resolved"));
        assert!(status_transition_allowed("monitoring", "resolved"));
        assert!(status_transition_allowed("resolved", "active"));
        assert!(status_transition_allowed("resolved", "archived"));
        assert!(status_transition_allowed("active", "archived"));
        assert!(!status_transition_allowed("monitoring", "monitoring"));
        assert!(!status_transition_allowed("archived", "active"));
        assert!(!status_transition_allowed("archived", "resolved"));
        assert!(!status_transition_allowed("resolved", "monitoring"));
    }

    #[test]
    fn runbook_template_matches_type_name_case_insensitively() {
        let incident = runbook_template(Some("Incident"));
        assert_eq!(incident.len(), 5);
        assert_eq!(incident[0].0, "triage");
        assert_eq!(incident[4].0, "postmortem");
        assert_eq!(runbook_template(Some(" problem ")).len(), 5);
        assert_eq!(runbook_template(Some("Change")).len(), 5);
        assert_eq!(runbook_template(Some("Request")).len(), 4);
        assert!(runbook_template(Some("Custom type")).is_empty());
        assert!(runbook_template(None).is_empty());
    }

    #[test]
    fn active_status_check() {
        assert!(is_active_status("active"));
        assert!(is_active_status("monitoring"));
        assert!(!is_active_status("resolved"));
        assert!(!is_active_status("archived"));
    }
}
