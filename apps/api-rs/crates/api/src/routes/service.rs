use uuid::Uuid;

/// Allowed `status` values (`packages/types/src/service/core.ts`).
pub const SERVICE_STATUSES: &[&str] = &["active", "planned", "maintenance", "deprecated", "retired"];
/// Allowed `criticality` values.
pub const SERVICE_CRITICALITIES: &[&str] = &["critical", "high", "medium", "low"];
/// Allowed `type` values.
pub const SERVICE_TYPES: &[&str] = &["internal", "external", "infrastructure", "third_party"];

/// trim + lowercase, matching the mock's case-insensitive uniqueness.
pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Enum validation; `field` appears in the error message (`"Invalid status"`).
pub fn validate_enum(field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("Invalid {field}"))
    }
}

/// True when adding edge `from -> to` creates a cycle, i.e. `from` is
/// reachable from `to` following existing `(from, to)` edges. `from == to`
/// is a self-edge and also reported as a cycle.
pub fn would_create_cycle(edges: &[(Uuid, Uuid)], from: Uuid, to: Uuid) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = std::collections::HashSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        for (a, b) in edges {
            if *a == node {
                if *b == from {
                    return true;
                }
                stack.push(*b);
            }
        }
    }
    false
}

/// Append order: `max(0, existing_max) + 65535`, matching the mock.
pub fn next_sort_order(max_existing: f64) -> f64 {
    max_existing.max(0.0) + 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_name_trims_and_lowercases() {
        assert_eq!(normalize_name("  Web API "), "web api");
    }

    #[test]
    fn validate_enum_accepts_known_and_rejects_unknown() {
        assert!(validate_enum("status", "active", SERVICE_STATUSES).is_ok());
        assert_eq!(
            validate_enum("status", "bogus", SERVICE_STATUSES).unwrap_err(),
            "Invalid status"
        );
        assert_eq!(
            validate_enum("criticality", "nope", SERVICE_CRITICALITIES).unwrap_err(),
            "Invalid criticality"
        );
        assert_eq!(
            validate_enum("type", "nope", SERVICE_TYPES).unwrap_err(),
            "Invalid type"
        );
    }

    #[test]
    fn cycle_detection_self_and_transitive() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        // existing: a -> b, b -> c (a depends on b, b depends on c)
        let edges = vec![(a, b), (b, c)];
        assert!(would_create_cycle(&edges, a, a));
        assert!(would_create_cycle(&edges, c, a));
        assert!(!would_create_cycle(&edges, a, c));
    }

    #[test]
    fn next_sort_order_appends() {
        assert_eq!(next_sort_order(0.0), 65535.0);
        assert_eq!(next_sort_order(65535.0), 131070.0);
    }
}
