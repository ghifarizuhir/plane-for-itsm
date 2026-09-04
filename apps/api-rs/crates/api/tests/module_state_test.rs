use api::routes::module::{validate_create as validate_module, CreateModule};
use api::routes::state::{validate_create as validate_state, CreateState};
use chrono::NaiveDate;

#[test]
fn module_rejects_empty_name() {
    let m = CreateModule {
        name: "".to_string(),
        start_date: None,
        target_date: None,
    };
    assert!(validate_module(&m).is_err());
}

#[test]
fn module_rejects_start_after_target() {
    let m = CreateModule {
        name: "M".to_string(),
        start_date: Some(NaiveDate::from_ymd_opt(2026, 9, 10).unwrap()),
        target_date: Some(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()),
    };
    let err = validate_module(&m).unwrap_err();
    assert!(err.to_lowercase().contains("start date"));
}

#[test]
fn module_accepts_valid() {
    let m = CreateModule {
        name: "Module A".to_string(),
        start_date: None,
        target_date: None,
    };
    assert!(validate_module(&m).is_ok());
}

#[test]
fn state_rejects_empty_name() {
    let s = CreateState {
        name: "".to_string(),
        group: "backlog".to_string(),
        color: "#60646C".to_string(),
    };
    assert!(validate_state(&s).is_err());
}

#[test]
fn state_rejects_triage_on_create() {
    let s = CreateState {
        name: "T".to_string(),
        group: "triage".to_string(),
        color: "#000000".to_string(),
    };
    let err = validate_state(&s).unwrap_err();
    assert!(err.to_lowercase().contains("triage"));
}

#[test]
fn state_rejects_unknown_group() {
    let s = CreateState {
        name: "T".to_string(),
        group: "nope".to_string(),
        color: "#000000".to_string(),
    };
    assert!(validate_state(&s).is_err());
}

#[test]
fn state_accepts_valid_groups() {
    for g in ["backlog", "unstarted", "started", "completed", "cancelled"] {
        let s = CreateState {
            name: "S".to_string(),
            group: g.to_string(),
            color: "#60646C".to_string(),
        };
        assert!(validate_state(&s).is_ok(), "group {g} must pass");
    }
}
