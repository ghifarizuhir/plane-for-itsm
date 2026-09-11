use api::routes::misc::{validate_export_provider, default_token_label};

#[test]
fn rejects_unknown_export_provider() {
    let err = validate_export_provider(Some("xml")).unwrap_err();
    assert!(err.contains("Provider 'xml' not found."));
}

#[test]
fn rejects_missing_export_provider() {
    assert!(validate_export_provider(None).is_err());
}

#[test]
fn accepts_valid_export_providers() {
    for p in ["csv", "xlsx", "json"] {
        assert!(validate_export_provider(Some(p)).is_ok());
    }
}

#[test]
fn token_label_defaults_to_hex() {
    let label = default_token_label(None);
    assert_eq!(label.len(), 32);
    assert!(label.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(default_token_label(Some("ci".to_string())), "ci");
}

#[tokio::test]
async fn timezones_envelope_with_offsets_sorted() {
    // Django `TimezoneEndpoint.get` (`timezone/base.py:184-215`):
    // `{"timezones": [{utc_offset, gmt_offset, value, label}]}` sorted by
    // (offset, label); offsets are DST-aware live values.
    let data = api::routes::misc::timezones().await.0;
    let arr = data.get("timezones").and_then(|v| v.as_array()).expect("envelope");
    assert!(arr.len() > 100, "codegen list intact: {}", arr.len());
    let mut prev: Option<(i32, String)> = None;
    for tz in arr {
        for k in ["utc_offset", "gmt_offset", "value", "label"] {
            assert!(tz.get(k).is_some(), "missing key {k}");
        }
        let utc = tz["utc_offset"].as_str().unwrap();
        let gmt = tz["gmt_offset"].as_str().unwrap();
        assert!(utc.starts_with("UTC") && gmt.starts_with("GMT"));
        assert_eq!(&utc[3..], &gmt[3..], "same stamp");
        // Parse "+HH:MM" back to seconds for the sort check.
        let s: Vec<char> = utc[3..].chars().collect();
        let secs: i32 = {
            let h: i32 = s[1..3].iter().collect::<String>().parse().unwrap();
            let m: i32 = s[4..6].iter().collect::<String>().parse().unwrap();
            (h * 3600 + m * 60) * if s[0] == '-' { -1 } else { 1 }
        };
        let label = tz["label"].as_str().unwrap().to_string();
        if let Some((po, pl)) = prev {
            assert!((secs, label.clone()) >= (po, pl), "sorted by (offset,label)");
        }
        prev = Some((secs, label));
    }
}
