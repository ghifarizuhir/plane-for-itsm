use api::routes::webhook::{validate_create, CreateWebhook};

fn hook(url: &str) -> CreateWebhook {
    CreateWebhook {
        url: url.to_string(),
        is_active: None,
        project: None,
        issue: None,
        cycle: None,
        module: None,
        issue_comment: None,
    }
}

#[test]
fn rejects_empty_url() {
    let err = validate_create(&hook("")).unwrap_err();
    assert!(err.to_lowercase().contains("url"));
}

#[test]
fn rejects_non_http_scheme() {
    let err = validate_create(&hook("ftp://example.com/hook")).unwrap_err();
    assert!(err.to_lowercase().contains("schema"));
}

#[test]
fn rejects_local_urls() {
    for u in ["http://localhost/hook", "https://127.0.0.1/hook"] {
        let err = validate_create(&hook(u)).unwrap_err();
        assert!(err.to_lowercase().contains("local"), "url={u}");
    }
}

#[test]
fn rejects_url_over_1024() {
    let long = format!("https://example.com/{}", "x".repeat(1024));
    let err = validate_create(&hook(&long)).unwrap_err();
    assert!(err.contains("1024"));
}

#[test]
fn accepts_valid_url() {
    assert!(validate_create(&hook("https://example.com/hook")).is_ok());
}
