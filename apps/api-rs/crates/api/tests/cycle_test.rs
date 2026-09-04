use api::routes::cycle::{validate_create, CreateCycle};
use chrono::Utc;

#[test]
fn rejects_empty_name() {
    let c = CreateCycle {
        name: "".to_string(),
        start_date: None,
        end_date: None,
    };
    assert!(validate_create(&c).is_err());
}

#[test]
fn rejects_start_after_end() {
    let now = Utc::now();
    let c = CreateCycle {
        name: "Sprint".to_string(),
        start_date: Some(now),
        end_date: Some(now - chrono::Duration::days(1)),
    };
    let err = validate_create(&c).unwrap_err();
    assert!(err.to_lowercase().contains("start date"));
}

#[test]
fn rejects_archive_without_end_date() {
    // #9200 guard: archiving a cycle with no end_date must fail
    let err = api::routes::cycle::validate_archive(None).unwrap_err();
    assert!(err.to_lowercase().contains("end_date") || err.to_lowercase().contains("end date"));
}

#[test]
fn accepts_valid_cycle() {
    let now = Utc::now();
    let c = CreateCycle {
        name: "Sprint 1".to_string(),
        start_date: Some(now),
        end_date: Some(now + chrono::Duration::days(7)),
    };
    assert!(validate_create(&c).is_ok());
    assert!(api::routes::cycle::validate_archive(Some(now)).is_ok());
}
