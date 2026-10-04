//! Intake triage questions: state, typed questions over project types +
//! priorities, and mapping Jev answers onto suggestion fields.

use serde_json::{json, Value};
use uuid::Uuid;

use crate::decision::{Answer, DecisionOutcome, Question};

pub const MAX_TYPES: usize = 20;
pub const MAX_SERVICES: usize = 20;
pub const DESCRIPTION_LIMIT: usize = 4000;
pub const SEVERITY_LEVELS: [&str; 5] = ["none", "low", "medium", "high", "urgent"];
pub const NO_SERVICE_CHOICE: &str = "No service / unsure";
pub const NO_SERVICE_SENTINEL: &str = "__none__";

#[derive(Clone)]
pub struct TypeOption {
    pub type_id: Uuid,
    pub name: String,
    pub description: String,
}

#[derive(Clone)]
pub struct ServiceOption {
    pub service_id: Uuid,
    pub name: String,
    pub criteria: String,
}

pub struct TriageState<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub project_name: &'a str,
    pub source: &'a str,
}

pub fn truncate_chars(input: &str, limit: usize) -> String {
    input.chars().take(limit).collect()
}

pub fn build_state(input: TriageState<'_>) -> Value {
    let mut work_item = serde_json::Map::new();
    work_item.insert("name".into(), json!(input.name));
    let description = truncate_chars(input.description.trim(), DESCRIPTION_LIMIT);
    if !description.is_empty() {
        work_item.insert("description".into(), json!(description));
    }
    json!({
        "work_item": work_item,
        "project": {"name": input.project_name},
        "source": input.source,
    })
}

const SEVERITY_CRITERIA: [&str; 5] = [
    "No user impact; cosmetic or informational",
    "Minor issue with an available workaround",
    "Degraded functionality; workaround is difficult",
    "Major functionality blocked for users",
    "Critical outage, security incident, or data loss",
];

pub fn build_questions(types: &[TypeOption], services: &[ServiceOption]) -> Vec<(String, Question)> {
    let mut questions = Vec::new();
    if !types.is_empty() {
        let criteria = types
            .iter()
            .take(MAX_TYPES)
            .map(|t| {
                let description = if t.description.trim().is_empty() {
                    t.name.clone()
                } else {
                    t.description.trim().to_string()
                };
                (t.name.clone(), description)
            })
            .collect();
        questions.push((
            "category".to_string(),
            Question::Choice {
                instructions: "Which work item type best fits this intake request?".to_string(),
                criteria,
            },
        ));
    }
    if !services.is_empty() {
        let mut criteria: Vec<(String, String)> = services
            .iter()
            .take(MAX_SERVICES)
            .map(|s| (s.name.clone(), s.criteria.clone()))
            .collect();
        criteria.push((
            NO_SERVICE_CHOICE.to_string(),
            "This request does not concern a specific service or the service is unknown".to_string(),
        ));
        questions.push((
            "service".to_string(),
            Question::Choice {
                instructions: "Which service does this intake item concern?".to_string(),
                criteria,
            },
        ));
    }
    questions.push((
        "severity".to_string(),
        Question::Score {
            instructions: "How severe is this request?".to_string(),
            criteria: SEVERITY_CRITERIA.iter().map(|s| s.to_string()).collect(),
        },
    ));
    questions.push((
        "needs_human".to_string(),
        Question::Noul {
            instructions: "Does this request need a human to handle it?".to_string(),
            criteria: Some((
                "A person should review or handle this request".to_string(),
                "Routine request suitable for normal automated triage".to_string(),
            )),
        },
    ));
    questions
}

pub fn priority_from_score(score: f64) -> &'static str {
    let index = score.round().clamp(0.0, (SEVERITY_LEVELS.len() - 1) as f64) as usize;
    SEVERITY_LEVELS[index]
}

pub struct TriageOutcome {
    pub model: String,
    pub category_type_id: Option<Uuid>,
    pub category_label: Option<String>,
    pub category_confidence: Option<f64>,
    pub severity_priority: Option<String>,
    pub severity_score: Option<f64>,
    pub severity_confidence: Option<f64>,
    pub service_id: Option<Uuid>,
    pub service_label: Option<String>,
    pub service_confidence: Option<f64>,
    pub needs_human: Option<f64>,
    pub answers: Value,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub fn triage_outcome(outcome: DecisionOutcome, types: &[TypeOption], services: &[ServiceOption]) -> TriageOutcome {
    let mut category: Option<(Option<Uuid>, String, f64)> = None;
    let mut severity: Option<(String, f64, f64)> = None;
    let mut service: Option<(Option<Uuid>, String, f64)> = None;
    let mut needs_human = None;
    for (id, answer) in &outcome.answers {
        match (id.as_str(), answer) {
            ("category", Answer::Choice { choice, confidence, .. }) => {
                let type_id = types.iter().find(|t| &t.name == choice).map(|t| t.type_id);
                category = Some((type_id, choice.clone(), *confidence));
            }
            ("service", Answer::Choice { choice, confidence, .. }) => {
                if choice == NO_SERVICE_CHOICE {
                    service = Some((None, NO_SERVICE_SENTINEL.to_string(), *confidence));
                } else {
                    let service_id = services.iter().find(|s| &s.name == choice).map(|s| s.service_id);
                    service = Some((service_id, choice.clone(), *confidence));
                }
            }
            ("severity", Answer::Score { score, confidence, .. }) => {
                severity = Some((priority_from_score(*score).to_string(), *score, *confidence));
            }
            ("needs_human", Answer::Noul { probability }) => needs_human = Some(*probability),
            _ => {}
        }
    }
    let answers = crate::decision::answers_json(&outcome.answers);
    TriageOutcome {
        model: outcome.model,
        category_type_id: category.as_ref().and_then(|(id, _, _)| *id),
        category_label: category.as_ref().map(|(_, label, _)| label.clone()),
        category_confidence: category.as_ref().map(|(_, _, confidence)| *confidence),
        severity_priority: severity.as_ref().map(|(priority, _, _)| priority.clone()),
        severity_score: severity.as_ref().map(|(_, score, _)| *score),
        severity_confidence: severity.as_ref().map(|(_, _, confidence)| *confidence),
        service_id: service.as_ref().and_then(|(id, _, _)| *id),
        service_label: service.as_ref().map(|(_, label, _)| label.clone()),
        service_confidence: service.as_ref().map(|(_, _, confidence)| *confidence),
        needs_human,
        answers,
        input_tokens: outcome.input_tokens,
        output_tokens: outcome.output_tokens,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn type_option(name: &str) -> TypeOption {
        TypeOption {
            type_id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
        }
    }

    fn service_option(name: &str) -> ServiceOption {
        ServiceOption {
            service_id: Uuid::new_v4(),
            name: name.to_string(),
            criteria: format!("{name} — internal, criticality high, status active"),
        }
    }

    #[test]
    fn state_truncates_and_omits_empty_description() {
        let long = "x".repeat(DESCRIPTION_LIMIT + 50);
        let state = build_state(TriageState {
            name: "Login is broken",
            description: &long,
            project_name: "Support",
            source: "IN_APP",
        });
        assert_eq!(state["work_item"]["name"], "Login is broken");
        assert_eq!(
            state["work_item"]["description"].as_str().unwrap().chars().count(),
            DESCRIPTION_LIMIT
        );
        assert_eq!(state["project"]["name"], "Support");
        assert_eq!(state["source"], "IN_APP");

        let state = build_state(TriageState {
            name: "No description",
            description: "   ",
            project_name: "Support",
            source: "IN_APP",
        });
        assert!(state["work_item"].get("description").is_none());
    }

    #[test]
    fn questions_omit_category_without_types() {
        let questions = build_questions(&[], &[]);
        let ids: Vec<&str> = questions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["severity", "needs_human"]);

        let types = vec![type_option("Incident"), type_option("Problem")];
        let questions = build_questions(&types, &[]);
        let ids: Vec<&str> = questions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["category", "severity", "needs_human"]);
        match &questions[0].1 {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria[0].0, "Incident");
                assert_eq!(criteria[0].1, "Incident");
                assert_eq!(criteria[1].0, "Problem");
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn questions_cap_types_and_order_severity_levels() {
        let types: Vec<TypeOption> = (0..30).map(|i| type_option(&format!("T{i}"))).collect();
        let questions = build_questions(&types, &[]);
        match &questions[0].1 {
            Question::Choice { criteria, .. } => assert_eq!(criteria.len(), MAX_TYPES),
            other => panic!("expected choice, got {other:?}"),
        }
        match &questions[1].1 {
            Question::Score { criteria, .. } => assert_eq!(criteria.len(), 5),
            other => panic!("expected score, got {other:?}"),
        }
    }

    #[test]
    fn priority_maps_rounding_boundaries() {
        assert_eq!(priority_from_score(0.0), "none");
        assert_eq!(priority_from_score(0.49), "none");
        assert_eq!(priority_from_score(0.5), "low");
        assert_eq!(priority_from_score(1.5), "medium");
        assert_eq!(priority_from_score(2.5), "high");
        assert_eq!(priority_from_score(3.6), "urgent");
        assert_eq!(priority_from_score(4.0), "urgent");
        assert_eq!(priority_from_score(9.0), "urgent");
    }

    #[test]
    fn outcome_maps_answers_to_fields() {
        let incident = type_option("Incident");
        let mut answers = BTreeMap::new();
        answers.insert(
            "category".to_string(),
            Answer::Choice {
                choice: "Incident".to_string(),
                confidence: 0.87,
                probabilities: BTreeMap::new(),
            },
        );
        answers.insert(
            "severity".to_string(),
            Answer::Score {
                score: 3.2,
                confidence: 0.91,
                probabilities: BTreeMap::new(),
            },
        );
        answers.insert(
            "needs_human".to_string(),
            Answer::Noul { probability: 0.78 },
        );
        let outcome = DecisionOutcome {
            model: "jev-1.13.0".to_string(),
            answers,
            input_tokens: 42,
            output_tokens: 7,
        };
        let mapped = triage_outcome(outcome, &[incident.clone()], &[]);
        assert_eq!(mapped.category_type_id, Some(incident.type_id));
        assert_eq!(mapped.category_label.as_deref(), Some("Incident"));
        assert_eq!(mapped.severity_priority.as_deref(), Some("high"));
        assert_eq!(mapped.needs_human, Some(0.78));
        assert_eq!(mapped.answers["severity"]["score"], 3.2);
    }

    #[test]
    fn questions_include_service_between_category_and_severity() {
        let types = vec![type_option("Incident")];
        let services = vec![service_option("Payment Gateway")];
        let questions = build_questions(&types, &services);
        let ids: Vec<&str> = questions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["category", "service", "severity", "needs_human"]);
        match &questions[1].1 {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria[0].0, "Payment Gateway");
                assert_eq!(criteria[1].0, NO_SERVICE_CHOICE);
            }
            other => panic!("expected choice, got {other:?}"),
        }
        assert_eq!(build_questions(&types, &[]).len(), 3);
    }

    #[test]
    fn outcome_maps_service_choice_and_abstain() {
        let service = service_option("Payment Gateway");
        let mut answers = BTreeMap::new();
        answers.insert(
            "service".to_string(),
            Answer::Choice {
                choice: "Payment Gateway".to_string(),
                confidence: 0.74,
                probabilities: BTreeMap::new(),
            },
        );
        let outcome = DecisionOutcome {
            model: "jev".to_string(),
            answers,
            input_tokens: 1,
            output_tokens: 1,
        };
        let mapped = triage_outcome(outcome, &[], &[service.clone()]);
        assert_eq!(mapped.service_id, Some(service.service_id));
        assert_eq!(mapped.service_label.as_deref(), Some("Payment Gateway"));
        assert_eq!(mapped.service_confidence, Some(0.74));

        let mut abstain = BTreeMap::new();
        abstain.insert(
            "service".to_string(),
            Answer::Choice {
                choice: NO_SERVICE_CHOICE.to_string(),
                confidence: 0.6,
                probabilities: BTreeMap::new(),
            },
        );
        let outcome = DecisionOutcome {
            model: "jev".to_string(),
            answers: abstain,
            input_tokens: 1,
            output_tokens: 1,
        };
        let mapped = triage_outcome(outcome, &[], &[service]);
        assert_eq!(mapped.service_id, None);
        assert_eq!(mapped.service_label.as_deref(), Some(NO_SERVICE_SENTINEL));
    }
}
