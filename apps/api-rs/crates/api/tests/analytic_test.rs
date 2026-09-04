use api::routes::analytic::{validate_axes, AxisParams};

#[test]
fn rejects_missing_axes() {
    let p = AxisParams {
        x_axis: None,
        y_axis: Some("issue_count".to_string()),
        segment: None,
    };
    let err = validate_axes(&p).unwrap_err();
    assert!(err.to_lowercase().contains("x-axis and y-axis"));
}

#[test]
fn rejects_invalid_axis() {
    let p = AxisParams {
        x_axis: Some("color".to_string()),
        y_axis: Some("issue_count".to_string()),
        segment: None,
    };
    let err = validate_axes(&p).unwrap_err();
    assert!(err.to_lowercase().contains("x-axis and y-axis"));
}

#[test]
fn rejects_invalid_y_axis() {
    let p = AxisParams {
        x_axis: Some("priority".to_string()),
        y_axis: Some("vibes".to_string()),
        segment: None,
    };
    assert!(validate_axes(&p).is_err());
}

#[test]
fn rejects_segment_equal_to_x_axis() {
    let p = AxisParams {
        x_axis: Some("priority".to_string()),
        y_axis: Some("issue_count".to_string()),
        segment: Some("priority".to_string()),
    };
    let err = validate_axes(&p).unwrap_err();
    assert!(err.to_lowercase().contains("segment"));
}

#[test]
fn accepts_valid_axes() {
    let p = AxisParams {
        x_axis: Some("state__group".to_string()),
        y_axis: Some("issue_count".to_string()),
        segment: None,
    };
    assert!(validate_axes(&p).is_ok());
}
