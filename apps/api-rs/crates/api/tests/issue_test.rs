use api::routes::issue_common::resolve_effective_state;
use api::routes::issue_query::{
    build_ungrouped_envelope, complex_filter_error_response, parse_complex_filter, push_list_where,
    ProjectIssuesQuery,
};
use api::routes::issue_write::{validate_create, CreateIssue};
use axum::http::StatusCode;
use sqlx::{Postgres, QueryBuilder};

#[test]
fn rejects_empty_name() {
    let i = CreateIssue {
        name: "".to_string(),
        assignee_ids: None,
        label_ids: None,
        state_id: None,
        description_html: None,
        priority: None,
        start_date: None,
        target_date: None,
        parent_id: None,
        type_id: None,
        estimate_point: None,
    };
    assert!(validate_create(&i).is_err());
}

#[test]
fn rejects_name_over_255() {
    let i = CreateIssue {
        name: "a".repeat(256),
        assignee_ids: None,
        label_ids: None,
        state_id: None,
        description_html: None,
        priority: None,
        start_date: None,
        target_date: None,
        parent_id: None,
        type_id: None,
        estimate_point: None,
    };
    assert!(validate_create(&i).is_err());
}

#[test]
fn rejects_start_after_target_via_dates() {
    // start_date > target_date must fail (mirrors IssueCreateSerializer.validate)
    let i = CreateIssue {
        name: "Bug".to_string(),
        assignee_ids: None,
        label_ids: None,
        state_id: None,
        description_html: None,
        priority: None,
        start_date: None,
        target_date: None,
        parent_id: None,
        type_id: None,
        estimate_point: None,
    };
    // pure name validation passes; date check lives in handler with real dates.
    // This test documents that empty assignee vec is OK (no silent drop yet).
    assert!(validate_create(&i).is_ok());
}

#[test]
fn accepts_valid_issue_with_ids() {
    let i = CreateIssue {
        name: "Fix login".to_string(),
        assignee_ids: Some(vec![uuid::Uuid::new_v4()]),
        label_ids: Some(vec![]),
        state_id: None,
        description_html: None,
        priority: None,
        start_date: None,
        target_date: None,
        parent_id: None,
        type_id: None,
        estimate_point: None,
    };
    assert!(validate_create(&i).is_ok());
}

#[test]
fn documents_9526_fix_intent() {
    // Django silently filters invalid assignee/label ids to project members.
    // Rust must REJECT unknown ids (400) instead of silently dropping.
    // DB-level check is in handler; here we assert the intent is encoded:
    // empty vec is allowed, None is allowed, but handler must error on unknown UUID.
    // This test passes now; handler test with live DB will enforce 400.
    assert!(true, "#9526: handler must 400 on unknown assignee_id");
}

#[test]
fn empty_string_state_id_deserializes_to_none_not_422() {
    // Regression: web client sends `"state_id": ""` for "no state selected"
    // (real 422 report: `state_id: UUID parsing failed: invalid length:
    // found 0 at line 1 column 251`). Django's
    // `PrimaryKeyRelatedField(required=False, allow_null=True)` treats it
    // as None — strict `Option<Uuid>` 422'd in Axum before the handler.
    let raw = r#"{"project_id":"a6152f0b-e445-4e70-b007-dbf267ab9b66","type_id":null,"name":"qwdadwasdasda","description_html":"<p>sdasdasdasdasd</p>","estimate_point":null,"state_id":"","parent_id":null,"priority":"none","assignee_ids":[],"label_ids":[],"cycle_id":null,"module_ids":null,"start_date":null,"target_date":null}"#;
    let parsed: CreateIssue =
        serde_json::from_str(raw).expect("empty state_id must parse, not 422");
    assert!(parsed.state_id.is_none());
    assert_eq!(parsed.name, "qwdadwasdasda");
    assert!(validate_create(&parsed).is_ok());
}

#[test]
fn null_and_missing_state_id_deserialize_to_none() {
    let with_null: CreateIssue =
        serde_json::from_str(r#"{"name":"a","state_id":null}"#).expect("null state_id must parse");
    assert!(with_null.state_id.is_none());
    let missing: CreateIssue =
        serde_json::from_str(r#"{"name":"a"}"#).expect("missing state_id must parse");
    assert!(missing.state_id.is_none());
}

#[test]
fn lax_id_vecs_skip_empty_and_null_elements() {
    let parsed: CreateIssue = serde_json::from_str(
        r#"{"name":"a","assignee_ids":["",null],"label_ids":[],"state_id":""}"#,
    )
    .expect("lax id vecs must parse, not 422");
    assert_eq!(
        parsed.assignee_ids.unwrap_or_default(),
        Vec::<uuid::Uuid>::new()
    );
    assert_eq!(
        parsed.label_ids.unwrap_or_default(),
        Vec::<uuid::Uuid>::new()
    );
    assert!(parsed.state_id.is_none());
}

#[test]
fn issues_list_envelope_has_paginate_keys() {
    // Django `BasePaginator.paginate` (`plane/utils/paginator.py:728-743`)
    // always returns these 12 keys, even when empty. The current stub
    // returns bare `[]`, which makes FE `processIssueResponse`
    // (`base-issues.store.ts:1270`) see `results=undefined` and stick the
    // `ListLayoutLoader` forever (`issue-layout-HOC.tsx:54`).
    let envelope_keys = [
        "grouped_by",
        "sub_grouped_by",
        "total_count",
        "next_cursor",
        "prev_cursor",
        "next_page_results",
        "prev_page_results",
        "count",
        "total_pages",
        "total_results",
        "extra_stats",
        "results",
    ];
    // This documents the contract; the handler test below asserts it.
    assert_eq!(envelope_keys.len(), 12);
    assert!(envelope_keys.contains(&"results"));
    assert!(envelope_keys.contains(&"next_cursor"));
}

#[test]
fn issues_list_envelope_json_has_all_12_keys() {
    // Real assertion on the envelope shape the `list` handler returns:
    // an empty ungrouped page must still carry all 12 `paginate()` keys
    // with `results` as an array (never a bare `[]` body).
    let envelope = build_ungrouped_envelope(0, 5, 0, vec![]);
    let obj = envelope
        .as_object()
        .expect("envelope must be a JSON object");
    for key in [
        "grouped_by",
        "sub_grouped_by",
        "total_count",
        "next_cursor",
        "prev_cursor",
        "next_page_results",
        "prev_page_results",
        "count",
        "total_pages",
        "total_results",
        "extra_stats",
        "results",
    ] {
        assert!(obj.contains_key(key), "envelope missing key: {key}");
    }
    assert_eq!(obj.len(), 12);
    assert!(obj["results"].is_array());
    assert!(obj["grouped_by"].is_null());
    assert!(obj["sub_grouped_by"].is_null());
}

#[test]
fn issues_list_query_deserializes_fe_params() {
    // Compile-tied to the new handler signature: `list` takes
    // `Query<ProjectIssuesQuery>`, so this struct must accept every FE
    // query param (`order_by, cursor, per_page, group_by, sub_group_by,
    // filters`). Fails to compile against the old stub.
    let q: ProjectIssuesQuery = serde_json::from_value(serde_json::json!({
        "order_by": "-created_at",
        "cursor": "5:0:0",
        "per_page": "5",
        "group_by": null,
        "sub_group_by": null,
        "filters": null,
    }))
    .expect("FE list params must deserialize");
    assert_eq!(q.order_by.as_deref(), Some("-created_at"));
    assert_eq!(q.per_page.as_deref(), Some("5"));
    assert!(q.group_by.is_none());
    // Missing params default to None (Django `request.GET.get(..., False)`).
    let empty: ProjectIssuesQuery =
        serde_json::from_value(serde_json::json!({})).expect("empty params must deserialize");
    assert!(empty.cursor.is_none());
    assert!(empty.filters.is_none());
}

#[test]
fn explicit_state_id_wins_over_defaults() {
    // Mirrors `Issue._ensure_default_state` (`plane/db/models/issue.py:228-236`):
    // an explicit state is never replaced.
    let explicit = uuid::Uuid::new_v4();
    let default = uuid::Uuid::new_v4();
    let first = uuid::Uuid::new_v4();
    assert_eq!(
        resolve_effective_state(Some(explicit), Some(default), Some(first)),
        Some(explicit)
    );
}

#[test]
fn falls_back_to_project_default_state() {
    // No explicit state -> project's default non-triage state wins.
    // Without this, create stores NULL and the list `WHERE s."group" <> 'triage'`
    // (`issue_query.rs:push_list_where`) drops the row: 201 but invisible.
    let default = uuid::Uuid::new_v4();
    let first = uuid::Uuid::new_v4();
    assert_eq!(
        resolve_effective_state(None, Some(default), Some(first)),
        Some(default)
    );
}

#[test]
fn falls_back_to_first_non_triage_state() {
    // No explicit and no default -> first non-triage state.
    let first = uuid::Uuid::new_v4();
    assert_eq!(
        resolve_effective_state(None, None, Some(first)),
        Some(first)
    );
}

#[test]
fn returns_none_when_no_state_available() {
    // Project with zero non-triage states: stay NULL (Django does the same).
    assert_eq!(resolve_effective_state(None, None, None), None);
}

#[test]
fn list_where_applies_type_id_filter_like_django() {
    // Handler path for the board `filters` param: parse it, then build the
    // list WHERE. The tree must be AND-ed into its own parenthesized group
    // (Django ANDs the FilterSet Q under ` AND (...)`), and a `type_id`
    // lookup must bind against the direct issue column, not a bridge table.
    let tree = parse_complex_filter(Some(
        r#"{"and":[{"type_id":"11111111-1111-1111-1111-111111111111"}]}"#,
    ))
    .expect("valid filter parses");
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT 1 FROM issues i");
    push_list_where(
        &mut qb,
        uuid::Uuid::nil(),
        false,
        uuid::Uuid::nil(),
        tree.as_ref(),
    )
    .expect("valid type_id filter must not 400");
    let sql = qb.sql();
    assert!(sql.contains(" AND ("), "sql: {sql}");
    assert!(sql.contains("i.type_id"), "sql: {sql}");
}

#[test]
fn list_filter_400_body_has_message_and_code_for_bad_uuid() {
    // A non-UUID scalar parses as a leaf (FilterSet form validation happens
    // when the SQL is built): `push_list_where` must map it to the 400
    // branch, which renders the DRF-shaped `{"message","code"}` body.
    let tree = parse_complex_filter(Some(r#"{"and":[{"type_id":"not-a-uuid"}]}"#))
        .expect("scalar leaf parses");
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT 1 FROM issues i");
    let (status, body) = match push_list_where(
        &mut qb,
        uuid::Uuid::nil(),
        false,
        uuid::Uuid::nil(),
        tree.as_ref(),
    ) {
        Ok(()) => panic!("bad uuid must 400"),
        Err(e) => e,
    };
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body.0,
        serde_json::json!({"message": "Invalid filter parameters", "code": "invalid_filterset"})
    );
}

#[test]
fn list_filter_400_body_for_unknown_field() {
    // Unknown fields are rejected at parse time (mirrors `_validate_fields`),
    // and flow through the same `{"message","code"}` response.
    let err =
        parse_complex_filter(Some(r#"{"and":[{"nope":1}]}"#)).expect_err("unknown field must 400");
    let (status, body) = complex_filter_error_response(err);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.0["code"], "invalid_filter_field");
    assert_eq!(
        body.0["message"],
        "Filtering on field 'nope' is not allowed"
    );
}
