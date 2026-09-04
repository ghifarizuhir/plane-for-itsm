use api::routes::estimate::{validate_create, validate_point_create, CreateEstimate, CreateEstimatePoint};

#[test]
fn rejects_empty_name() {
    let e = CreateEstimate {
        name: "".to_string(),
        description: None,
        estimate_type: None,
    };
    assert!(validate_create(&e).is_err());
}

#[test]
fn rejects_name_over_255() {
    let e = CreateEstimate {
        name: "x".repeat(256),
        description: None,
        estimate_type: None,
    };
    let err = validate_create(&e).unwrap_err();
    assert!(err.to_lowercase().contains("255"));
}

#[test]
fn rejects_invalid_type() {
    let e = CreateEstimate {
        name: "Fib".to_string(),
        description: None,
        estimate_type: Some("tshirt".to_string()),
    };
    let err = validate_create(&e).unwrap_err();
    assert!(err.to_lowercase().contains("type"));
}

#[test]
fn accepts_valid_estimate() {
    for t in [None, Some("categories".to_string()), Some("points".to_string())] {
        let e = CreateEstimate {
            name: "Fib".to_string(),
            description: Some("fib scale".to_string()),
            estimate_type: t,
        };
        assert!(validate_create(&e).is_ok());
    }
}

#[test]
fn point_rejects_missing_key_or_value() {
    let p = CreateEstimatePoint {
        key: None,
        value: None,
        description: None,
    };
    let err = validate_point_create(&p).unwrap_err();
    assert!(err.to_lowercase().contains("key and value"));
}

#[test]
fn point_rejects_value_over_20() {
    let p = CreateEstimatePoint {
        key: Some(1),
        value: Some("x".repeat(21)),
        description: None,
    };
    let err = validate_point_create(&p).unwrap_err();
    assert!(err.to_lowercase().contains("20"));
}

#[test]
fn point_accepts_valid() {
    let p = CreateEstimatePoint {
        key: Some(1),
        value: Some("1".to_string()),
        description: None,
    };
    assert!(validate_point_create(&p).is_ok());
}
