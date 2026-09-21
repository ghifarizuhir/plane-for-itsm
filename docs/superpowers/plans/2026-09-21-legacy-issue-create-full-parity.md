# Legacy Issue Create Full Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Legacy `POST /api/workspaces/:slug/projects/:id/issues/` persists every write-field the web sends, writes assignee/label bridges with the Django default-assignee fallback, writes `issue_activities` + `issue_subscribers`, and returns Django's 26-key issue row.

**Architecture:** Approach 1 — extend the existing handler in place, move shared helpers into `issue_common.rs`, add a focused `issue_activity_write.rs` module, and reuse the list projection (`LIST_SELECT_SQL` + `IssueListRow`) for the response. All writes (issue + bridges + activities + subscribers) run in one transaction.

**Tech Stack:** Rust (axum, sqlx/Postgres), cargo integration tests against the live dev DB.

**Spec:** `docs/superpowers/specs/2026-09-21-legacy-issue-create-full-parity-design.md`

**Preflight (every test command):**

- Work from `apps/api-rs`.
- Tests hit the live dev DB: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.
- Serial only: `-- --test-threads=1` (the `purge()` fixture deletes all `itseq-%` rows globally).

---

### Task 1: Extended request fields + 400 validation

**Files:**

- Modify: `crates/api/src/routes/issue_write.rs` (`CreateIssue`, `validate_create`, new `validate_create_refs`, `bad`, `NewIssue`, `insert_issue`, `create`)
- Modify: `crates/api/src/routes/issue_common.rs` (new `parse_date`)
- Modify: `crates/api/src/routes/v1/work_item.rs` (import `parse_date`, delete local copy)
- Modify: `crates/api/src/routes/intake.rs` (fill new `NewIssue` fields with `None`/default)
- Modify: `crates/api/tests/issue_test.rs` (new struct fields in 4 literals)
- Test: `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/api/tests/issue_create_test.rs`, add the helpers after `Scratch::cleanup` (before `purge`). `base_body` is used by every test from here on:

```rust
fn base_body(name: &str, state_id: Uuid) -> CreateIssue {
    CreateIssue {
        name: name.to_string(),
        assignee_ids: None,
        label_ids: None,
        state_id: Some(state_id),
        description_html: None,
        priority: None,
        start_date: None,
        target_date: None,
        parent_id: None,
        type_id: None,
        estimate_point: None,
    }
}

async fn create_body(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    body: CreateIssue,
) -> (StatusCode, Value) {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(actor),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body),
    )
    .await
    .expect("handler must return a response");
    (status, serde_json::to_value(body).expect("json body"))
}

async fn insert_issue_type(pool: &PgPool, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
         is_default, level, is_epic, created_at, updated_at) \
         VALUES ($1, $2, '', '{}'::jsonb, $3, true, false, 0, false, now(), now())",
    )
    .bind(id)
    .bind(name)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch issue type");
    id
}

async fn insert_estimate_with_point(pool: &PgPool, project_id: Uuid, workspace_id: Uuid) -> Uuid {
    let estimate_id = Uuid::new_v4();
    let point_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO estimates (id, name, description, type, last_used, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, 'Points', '', 'points', true, $2, $3, now(), now())",
    )
    .bind(estimate_id)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch estimate");
    sqlx::query(
        "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, \
         workspace_id, created_at, updated_at) \
         VALUES ($1, $2, 0, '1', '', $3, $4, now(), now())",
    )
    .bind(point_id)
    .bind(estimate_id)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch estimate point");
    point_id
}
```

Replace the `create_as` helper (currently near line 534) with a delegating version:

```rust
async fn create_as(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    name: &str,
) -> (StatusCode, Value) {
    create_body(st, scratch, actor, base_body(name, scratch.state_id)).await
}
```

Replace the `body` closure in `create_allocates_distinct_sequences_and_records_counter_rows` (around line 318) with:

```rust
    let body = |name: &str| base_body(name, scratch.state_id);
```

Add these tests at the end of the file:

```rust
#[tokio::test]
async fn create_persists_extended_fields() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (parent_status, parent_body) = create_as(&st, &scratch, scratch.user_id, "parent-probe").await;
    assert_eq!(parent_status, StatusCode::CREATED);
    let parent_id = Uuid::parse_str(parent_body["id"].as_str().expect("id")).unwrap();

    let mut body = base_body("extended-probe", scratch.state_id);
    body.description_html = Some("<p>hello world</p>".to_string());
    body.priority = Some("high".to_string());
    body.start_date = Some("2026-09-01".to_string());
    body.target_date = Some("2026-09-30".to_string());
    body.parent_id = Some(parent_id);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let row: (String, String, Option<String>, Option<String>, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT description_html, priority, start_date::text, target_date::text, parent_id, \
             updated_by_id FROM issues WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(row.0, "<p>hello world</p>");
    assert_eq!(row.1, "high");
    assert_eq!(row.2.as_deref(), Some("2026-09-01"));
    assert_eq!(row.3.as_deref(), Some("2026-09-30"));
    assert_eq!(row.4, Some(parent_id));
    assert_eq!(row.5, None, "updated_by must stay NULL on create (Django parity)");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_persists_type_and_estimate_point() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let type_id = insert_issue_type(&st.pool, scratch.workspace_id, "Bug").await;
    let point_id =
        insert_estimate_with_point(&st.pool, scratch.project_id, scratch.workspace_id).await;

    let mut body = base_body("typed-probe", scratch.state_id);
    body.type_id = Some(type_id);
    body.estimate_point = Some(point_id);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let row: (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT type_id, estimate_point_id FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(row.0, Some(type_id));
    assert_eq!(row.1, Some(point_id));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_defaults_description_and_priority_when_absent() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (status, payload) =
        create_body(&st, &scratch, scratch.user_id, base_body("defaults-probe", scratch.state_id))
            .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let row: (String, String, Option<String>, Option<Uuid>, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT description_html, priority, description_stripped, parent_id, type_id, \
             estimate_point_id FROM issues WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(row.0, "<p></p>");
    assert_eq!(row.1, "none");
    assert_eq!(row.2, None);
    assert_eq!(row.3, None);
    assert_eq!(row.4, None);
    assert_eq!(row.5, None);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_validation_errors_return_400() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let cases: Vec<(&str, CreateIssue, &str)> = vec![
        ("blank-name", base_body("", scratch.state_id), "name is required"),
        (
            "unknown-assignee",
            {
                let mut b = base_body("unknown-assignee", scratch.state_id);
                b.assignee_ids = Some(vec![Uuid::new_v4()]);
                b
            },
            "invalid assignee: not a project member",
        ),
        (
            "unknown-label",
            {
                let mut b = base_body("unknown-label", scratch.state_id);
                b.label_ids = Some(vec![Uuid::new_v4()]);
                b
            },
            "invalid label: not in project",
        ),
        (
            "unknown-state",
            {
                let mut b = base_body("unknown-state", scratch.state_id);
                b.state_id = Some(Uuid::new_v4());
                b
            },
            "State is not valid please pass a valid state_id",
        ),
        (
            "unknown-type",
            {
                let mut b = base_body("unknown-type", scratch.state_id);
                b.type_id = Some(Uuid::new_v4());
                b
            },
            "type_id is not valid",
        ),
        (
            "unknown-parent",
            {
                let mut b = base_body("unknown-parent", scratch.state_id);
                b.parent_id = Some(Uuid::new_v4());
                b
            },
            "parent is not valid",
        ),
        (
            "unknown-estimate",
            {
                let mut b = base_body("unknown-estimate", scratch.state_id);
                b.estimate_point = Some(Uuid::new_v4());
                b
            },
            "estimate_point is not valid",
        ),
        (
            "bad-date",
            {
                let mut b = base_body("bad-date", scratch.state_id);
                b.start_date = Some("09/01/2026".to_string());
                b
            },
            "Invalid date: 09/01/2026",
        ),
    ];

    for (case, body, expected) in cases {
        let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "case {case} must 400");
        assert_eq!(payload, json!({"error": expected}), "case {case} body");
    }

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = $1")
        .bind(scratch.project_id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(persisted, 0, "failed validation must not insert an issue");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn invalid_priority_returns_400() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let mut body = base_body("bad-priority", scratch.state_id);
    body.priority = Some("critical".to_string());

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload, json!({"error": "Invalid priority"}));

    scratch.cleanup(&st.pool).await;
}
```

Extend `Scratch::cleanup` and `purge` so the new fixture tables never leak (add to the existing statement lists; in `cleanup` before `DELETE FROM issues`, in `purge` before `DELETE FROM issues`):

```rust
        sqlx::query("DELETE FROM estimate_points WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM estimates WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_types WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
```

In `purge`, add `"DELETE FROM estimate_points WHERE project_id = $1"`, `"DELETE FROM estimates WHERE project_id = $1"` to the per-project `for stmt in [...]` array, and after the per-workspace project loop add:

```rust
        let _ = sqlx::query("DELETE FROM issue_types WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await;
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: compile error (`CreateIssue` has no field `description_html`, `insert_issue` missing new args) — the tests cannot pass before the struct exists.

- [ ] **Step 3: Implement the request fields, validation, and insert**

In `crates/api/src/routes/issue_common.rs`, append (needs `use sqlx::Postgres;` and `use uuid::Uuid;` at the top if not present; keep the file's existing imports):

```rust
/// Shared `%Y-%m-%d` parser (moved from `v1::work_item`).
pub(crate) fn parse_date(raw: &Option<String>) -> Result<Option<chrono::NaiveDate>, String> {
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| format!("Invalid date: {s}")),
    }
}
```

In `crates/api/src/routes/v1/work_item.rs`, delete the local `parse_date` (lines ~538-545) and add `parse_date` to the `issue_common` import block (line 8-11).

In `crates/api/src/routes/issue_write.rs`, replace the `CreateIssue` struct:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIssue {
    pub name: String,
    #[serde(default, deserialize_with = "de_opt_uuid_vec_lax")]
    pub assignee_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default, deserialize_with = "de_opt_uuid_vec_lax")]
    pub label_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default, deserialize_with = "de_opt_uuid_lax")]
    pub state_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default, deserialize_with = "de_opt_uuid_lax")]
    pub parent_id: Option<uuid::Uuid>,
    #[serde(default, deserialize_with = "de_opt_uuid_lax")]
    pub type_id: Option<uuid::Uuid>,
    #[serde(default, deserialize_with = "de_opt_uuid_lax")]
    pub estimate_point: Option<uuid::Uuid>,
}
```

Replace `validate_create` and add the two new items (before `create`):

```rust
const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

pub fn validate_create(body: &CreateIssue) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(p) = &body.priority {
        if !PRIORITIES.contains(&p.as_str()) {
            return Err("Invalid priority".to_string());
        }
    }
    super::issue_common::parse_date(&body.start_date)?;
    super::issue_common::parse_date(&body.target_date)?;
    Ok(())
}

fn bad(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"error": msg})))
}

/// DB-backed create validation; every failure is 400 (brainstorming decision:
/// strict, not Django's silent filter for assignee/label ids).
async fn validate_create_refs(
    st: &AppState,
    project_id: uuid::Uuid,
    body: &CreateIssue,
    assignees: &[uuid::Uuid],
    labels: &[uuid::Uuid],
) -> Result<(), (StatusCode, Json<Value>)> {
    let error = |e: sqlx::Error| {
        let _ = e;
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Something went wrong please try again later"})),
        )
    };
    if !assignees.is_empty() {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) \
             AND is_active = true AND role >= 15 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(assignees)
        .fetch_one(&st.pool)
        .await
        .map_err(error)?;
        if n != assignees.len() as i64 {
            return Err(bad("invalid assignee: not a project member"));
        }
    }
    if !labels.is_empty() {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM labels WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(labels)
        .fetch_one(&st.pool)
        .await
        .map_err(error)?;
        if n != labels.len() as i64 {
            return Err(bad("invalid label: not in project"));
        }
    }
    if let Some(state_id) = body.state_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2)",
        )
        .bind(state_id)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(error)?;
        if !ok {
            return Err(bad("State is not valid please pass a valid state_id"));
        }
    }
    if let Some(t) = body.type_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(t)
        .fetch_one(&st.pool)
        .await
        .map_err(error)?;
        if !ok {
            return Err(bad("type_id is not valid"));
        }
    }
    if let Some(parent) = body.parent_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(parent)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(error)?;
        if !ok {
            return Err(bad("parent is not valid"));
        }
    }
    if let Some(ep) = body.estimate_point {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM estimate_points WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(ep)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(error)?;
        if !ok {
            return Err(bad("estimate_point is not valid"));
        }
    }
    Ok(())
}
```

Replace `NewIssue` and the `insert_issue` INSERT statement:

```rust
pub struct NewIssue<'a> {
    pub slug: &'a str,
    pub project_id: uuid::Uuid,
    pub state_id: Option<uuid::Uuid>,
    pub name: &'a str,
    pub description_html: &'a str,
    pub priority: &'a str,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub parent_id: Option<uuid::Uuid>,
    pub type_id: Option<uuid::Uuid>,
    pub estimate_point_id: Option<uuid::Uuid>,
    pub created_by: uuid::Uuid,
}
```

Inside `insert_issue`, replace the issue INSERT (the `sequence` query stays as-is):

```rust
    let row: (uuid::Uuid, String) = sqlx::query_as(
        "INSERT INTO issues (id, name, description_html, description_json, priority, start_date, \
         target_date, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, \
         created_by_id, updated_by_id, estimate_point_id, type_id, parent_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, '{}', $3, $4, $5, false, \
         COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $6 AND state_id IS NOT DISTINCT FROM $7), 65535 - 10000) + 10000, \
         $8, $7, $6, w.id, $9, NULL, $10, $11, $12, now(), now() \
         FROM workspaces w WHERE w.slug = $13 RETURNING id, name",
    )
    .bind(issue.name)
    .bind(issue.description_html)
    .bind(issue.priority)
    .bind(issue.start_date)
    .bind(issue.target_date)
    .bind(issue.project_id)
    .bind(issue.state_id)
    .bind(sequence as i32)
    .bind(issue.created_by)
    .bind(issue.estimate_point_id)
    .bind(issue.type_id)
    .bind(issue.parent_id)
    .bind(issue.slug)
    .fetch_one(&mut **tx)
    .await?;
```

Replace the `create` handler (validation now 400, fields persisted):

```rust
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django's permission decorator runs before serializer validation
    // (`views/issue/base.py:404`): a non-member gets 403 even with a bad body.
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if let Err(msg) = validate_create(&body) {
        return Ok(bad(&msg));
    }
    if let Err(e) = validate_create_refs(
        &st,
        project_id,
        &body,
        body.assignee_ids.as_deref().unwrap_or(&[]),
        body.label_ids.as_deref().unwrap_or(&[]),
    )
    .await
    {
        return Ok(e);
    }

    let state_id = resolve_issue_state(&st.pool, project_id, body.state_id).await?;
    let start_date = match super::issue_common::parse_date(&body.start_date) {
        Ok(v) => v,
        Err(e) => return Ok(bad(&e)),
    };
    let target_date = match super::issue_common::parse_date(&body.target_date) {
        Ok(v) => v,
        Err(e) => return Ok(bad(&e)),
    };
    let description_html = body
        .description_html
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("<p></p>");
    let priority = body.priority.as_deref().unwrap_or("none");

    let mut tx = st.pool.begin().await?;
    let out = insert_issue(
        &mut tx,
        NewIssue {
            slug: &slug,
            project_id,
            state_id,
            name: &body.name,
            description_html,
            priority,
            start_date,
            target_date,
            parent_id: body.parent_id,
            type_id: body.type_id,
            estimate_point_id: body.estimate_point,
            created_by: auth.0,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(out).expect("IssueOut serializes")),
    ))
}
```

In `crates/api/src/routes/intake.rs` (around line 569), fill the new fields (intake behavior unchanged):

```rust
        super::issue_write::NewIssue {
            slug: &slug,
            project_id,
            state_id: Some(triage_id),
            name: &name,
            description_html: "<p></p>",
            priority: &priority,
            start_date: None,
            target_date: None,
            parent_id: None,
            type_id: None,
            estimate_point_id: None,
            created_by: auth.0,
        },
```

In `crates/api/tests/issue_test.rs`, add the new fields (`description_html: None, priority: None, start_date: None, target_date: None, parent_id: None, type_id: None, estimate_point: None,`) to the four `CreateIssue` literals in `rejects_empty_name`, `rejects_name_over_255`, `rejects_start_after_target_via_dates`, and `accepts_valid_issue_with_ids`.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_test
```

Expected: `issue_create_test` 16 passed, 0 failed; `issue_test` all pass.

- [ ] **Step 5: Commit**

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_write.rs \
        apps/api-rs/crates/api/src/routes/issue_common.rs \
        apps/api-rs/crates/api/src/routes/v1/work_item.rs \
        apps/api-rs/crates/api/src/routes/intake.rs \
        apps/api-rs/crates/api/tests/issue_create_test.rs \
        apps/api-rs/crates/api/tests/issue_test.rs
git commit -m "feat(api-rs): legacy issue create persists full web payload with 400 validation"
```

---

### Task 2: Assignee/label bridges + default assignee

**Files:**

- Modify: `crates/api/src/routes/issue_common.rs` (port `replace_bridges`, add `apply_create_bridges`)
- Modify: `crates/api/src/routes/v1/work_item.rs` (delete local `replace_bridges`, import from `issue_common`)
- Modify: `crates/api/src/routes/issue_write.rs` (`dedupe_ids`, handler)
- Test: `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing tests**

Add helpers next to `insert_issue_type`:

```rust
async fn insert_label(pool: &PgPool, project_id: Uuid, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO labels (id, name, color, description, sort_order, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, $2, '#60646C', '', 65535, $3, $4, now(), now())",
    )
    .bind(id)
    .bind(name)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch label");
    id
}

async fn set_default_assignee(pool: &PgPool, project_id: Uuid, user_id: Uuid) {
    sqlx::query("UPDATE projects SET default_assignee_id = $2 WHERE id = $1")
        .bind(project_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("set default assignee");
}

async fn live_assignees(pool: &PgPool, issue_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT assignee_id FROM issue_assignees WHERE issue_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("assignee rows")
}
```

Extend `Scratch::cleanup` (before `DELETE FROM issues`) and `purge`'s per-project statement list:

```rust
        sqlx::query("DELETE FROM issue_assignees WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_labels WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM labels WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
```

(`purge`: add `"DELETE FROM issue_assignees WHERE project_id = $1"`, `"DELETE FROM issue_labels WHERE project_id = $1"`, `"DELETE FROM labels WHERE project_id = $1"` to the `for stmt in [...]` array.)

Add the tests:

```rust
#[tokio::test]
async fn create_writes_assignee_and_label_bridges() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let label = insert_label(&st.pool, scratch.project_id, scratch.workspace_id, "urgent").await;

    let mut body = base_body("bridge-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member]);
    body.label_ids = Some(vec![label]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT assignee_id, created_by_id, updated_by_id FROM issue_assignees \
         WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "exactly one live assignee bridge");
    assert_eq!(rows[0].0, member);
    assert_eq!(rows[0].1, Some(scratch.user_id));
    assert_eq!(rows[0].2, None, "bridge updated_by_id stays NULL on create");

    let labels: Vec<Uuid> = sqlx::query_scalar(
        "SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(labels, vec![label]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_applies_default_assignee_when_absent() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let (status, payload) =
        create_body(&st, &scratch, scratch.user_id, base_body("default-absent", scratch.state_id))
            .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(live_assignees(&st.pool, id).await, vec![default_user]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_applies_default_assignee_when_empty() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let mut body = base_body("default-empty", scratch.state_id);
    body.assignee_ids = Some(vec![]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(live_assignees(&st.pool, id).await, vec![default_user]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_ignores_default_assignee_when_assignees_sent() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let chosen = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let mut body = base_body("default-ignored", scratch.state_id);
    body.assignee_ids = Some(vec![chosen]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(live_assignees(&st.pool, id).await, vec![chosen]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_skips_ineligible_default_assignee() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let guest = scratch.add_actor(&st.pool, Some(15), Some(5)).await;
    set_default_assignee(&st.pool, scratch.project_id, guest).await;

    let (status, payload) =
        create_body(&st, &scratch, scratch.user_id, base_body("default-ineligible", scratch.state_id))
            .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert!(
        live_assignees(&st.pool, id).await.is_empty(),
        "role < 15 default assignee must not be bridged"
    );

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn duplicate_assignee_ids_are_deduped() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;

    let mut body = base_body("dedupe-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member, member]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(
        live_assignees(&st.pool, id).await,
        vec![member],
        "duplicate ids must collapse to one bridge row"
    );

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: `create_writes_assignee_and_label_bridges` fails with `exactly one live assignee bridge` (0 rows) and `duplicate_assignee_ids_are_deduped` fails with 400 (`invalid assignee: not a project member`) or 0 rows; default-assignee tests fail with empty live assignees.

- [ ] **Step 3: Implement the bridges and the default fallback**

In `crates/api/src/routes/issue_common.rs`, append (the `replace_bridges` body is moved verbatim from `v1/work_item.rs`, only visibility changes):

```rust
/// Replaces the live assignee/label bridge rows when the SDK sent the key.
pub(crate) async fn replace_bridges(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
    assignees: Option<&[Uuid]>,
    labels: Option<&[Uuid]>,
) -> Result<(), common::errors::AppError> {
    if let Some(ids) = assignees {
        sqlx::query("UPDATE issue_assignees SET deleted_at = now() WHERE issue_id = $1 AND deleted_at IS NULL")
            .bind(issue_id).execute(&mut **tx).await?;
        for id in ids {
            sqlx::query(
                "INSERT INTO issue_assignees (id, issue_id, assignee_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, $4, now(), now() FROM workspaces w \
                 WHERE w.id = (SELECT workspace_id FROM projects WHERE id = $3)",
            ).bind(issue_id).bind(id).bind(project_id).bind(user_id).execute(&mut **tx).await?;
        }
    }
    if let Some(ids) = labels {
        sqlx::query("UPDATE issue_labels SET deleted_at = now() WHERE issue_id = $1 AND deleted_at IS NULL")
            .bind(issue_id).execute(&mut **tx).await?;
        for id in ids {
            sqlx::query(
                "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, $4, now(), now() FROM workspaces w \
                 WHERE w.id = (SELECT workspace_id FROM projects WHERE id = $3)",
            ).bind(issue_id).bind(id).bind(project_id).bind(user_id).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

async fn insert_assignee_bridge(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    issue_id: Uuid,
    assignee_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    creator: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_assignees (id, issue_id, assignee_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, NULL, now(), now()) \
         ON CONFLICT DO NOTHING",
    )
    .bind(issue_id)
    .bind(assignee_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(creator)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_label_bridge(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    issue_id: Uuid,
    label_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    creator: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, NULL, now(), now()) \
         ON CONFLICT DO NOTHING",
    )
    .bind(issue_id)
    .bind(label_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(creator)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Create-path bridge writer (Django `IssueCreateSerializer.create`,
/// `serializers/issue.py:214-272`): requested ids win; otherwise the project
/// default assignee is applied when it is an active project member role >= 15.
/// Rows carry `updated_by_id = NULL` (issue's `updated_by` is NULL on create).
pub(crate) async fn apply_create_bridges(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    creator: Uuid,
    requested_assignees: &[Uuid],
    requested_labels: &[Uuid],
) -> Result<(), sqlx::Error> {
    if requested_assignees.is_empty() {
        let default_assignee: Option<Uuid> = sqlx::query_scalar(
            "SELECT p.default_assignee_id FROM projects p \
             WHERE p.id = $1 AND p.default_assignee_id IS NOT NULL \
             AND EXISTS(SELECT 1 FROM project_members pm \
                        WHERE pm.project_id = p.id AND pm.member_id = p.default_assignee_id \
                        AND pm.role >= 15 AND pm.is_active = true AND pm.deleted_at IS NULL)",
        )
        .bind(project_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(assignee_id) = default_assignee {
            insert_assignee_bridge(tx, issue_id, assignee_id, project_id, workspace_id, creator).await?;
        }
    } else {
        for assignee_id in requested_assignees {
            insert_assignee_bridge(tx, issue_id, *assignee_id, project_id, workspace_id, creator).await?;
        }
    }
    for label_id in requested_labels {
        insert_label_bridge(tx, issue_id, *label_id, project_id, workspace_id, creator).await?;
    }
    Ok(())
}
```

In `crates/api/src/routes/v1/work_item.rs`, delete the local `replace_bridges` (lines ~629-661) and add `replace_bridges` to the `issue_common` import block (lines 8-11).

In `crates/api/src/routes/issue_write.rs`, add the dedupe helper and update the handler. Add next to `bad`:

```rust
/// Order-preserving dedupe (Django's bulk_create loses the whole batch on a
/// duplicate; we keep the first occurrence of each id).
fn dedupe_ids(ids: &Option<Vec<uuid::Uuid>>) -> Vec<uuid::Uuid> {
    let mut seen = std::collections::HashSet::new();
    ids.as_deref()
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect()
}
```

In `create`, replace the body between the gate and `let state_id = ...` with:

```rust
    let assignees = dedupe_ids(&body.assignee_ids);
    let labels = dedupe_ids(&body.label_ids);
    if let Err(msg) = validate_create(&body) {
        return Ok(bad(&msg));
    }
    if let Err(e) = validate_create_refs(&st, project_id, &body, &assignees, &labels).await {
        return Ok(e);
    }
```

and replace the line `tx.commit().await?;` that follows the `insert_issue(...).await?;` call with:

```rust
    let workspace_id: uuid::Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?;
    apply_create_bridges(
        &mut tx,
        out.id,
        project_id,
        workspace_id,
        auth.0,
        &assignees,
        &labels,
    )
    .await?;
    tx.commit().await?;
```

Add `apply_create_bridges` to the `issue_common` import list at line 7.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: 22 passed, 0 failed.

- [ ] **Step 5: Commit**

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_common.rs \
        apps/api-rs/crates/api/src/routes/v1/work_item.rs \
        apps/api-rs/crates/api/src/routes/issue_write.rs \
        apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "feat(api-rs): legacy issue create writes assignee/label bridges with default assignee"
```

---

### Task 3: Activities + subscribers

**Files:**

- Create: `crates/api/src/routes/issue_activity_write.rs`
- Modify: `crates/api/src/routes/mod.rs` (register module)
- Modify: `crates/api/src/routes/issue_write.rs` (handler calls)
- Test: `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing tests**

Extend `Scratch::cleanup` (before `DELETE FROM issues`) and `purge`'s per-project statement list with:

```rust
        sqlx::query("DELETE FROM issue_activities WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_subscribers WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
```

(`purge`: add `"DELETE FROM issue_activities WHERE project_id = $1"` and `"DELETE FROM issue_subscribers WHERE project_id = $1"` to the `for stmt in [...]` array.)

Add the tests:

```rust
#[tokio::test]
async fn create_writes_created_activity_and_assignee_artifacts() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let display_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(member)
        .fetch_one(&st.pool)
        .await
        .unwrap();

    let mut body = base_body("activity-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let created: Vec<(String, Option<String>, String, Option<Uuid>, Option<Uuid>, Option<Uuid>)> =
        sqlx::query_as(
            "SELECT verb, field, comment, actor_id, created_by_id, updated_by_id FROM issue_activities \
             WHERE issue_id = $1 AND verb = 'created'",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await
        .unwrap();
    assert_eq!(created.len(), 1, "exactly one created activity row");
    assert_eq!(created[0].0, "created");
    assert_eq!(created[0].1, None);
    assert_eq!(created[0].2, "created the issue");
    assert_eq!(created[0].3, Some(scratch.user_id));
    assert_eq!(created[0].4, Some(scratch.user_id));
    assert_eq!(created[0].5, None);

    let assignee_acts: Vec<(String, String, String, String, Option<Uuid>, Option<Uuid>)> =
        sqlx::query_as(
            "SELECT field, old_value, new_value, comment, new_identifier, created_by_id \
             FROM issue_activities WHERE issue_id = $1 AND field = 'assignees'",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await
        .unwrap();
    assert_eq!(assignee_acts.len(), 1);
    assert_eq!(assignee_acts[0].0, "assignees");
    assert_eq!(assignee_acts[0].1, "");
    assert_eq!(assignee_acts[0].2, display_name);
    assert_eq!(assignee_acts[0].3, "added assignee ");
    assert_eq!(assignee_acts[0].4, Some(member));
    assert_eq!(assignee_acts[0].5, None, "bulk_create parity leaves created_by_id NULL");

    let subs: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT subscriber_id, created_by_id, updated_by_id FROM issue_subscribers \
         WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].0, member);
    assert_eq!(subs[0].1, Some(member), "subscriber row is authored by the assignee");
    assert_eq!(subs[0].2, Some(member));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn default_assignee_gets_no_activity_or_subscriber() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let (status, payload) =
        create_body(&st, &scratch, scratch.user_id, base_body("default-activity", scratch.state_id))
            .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
         (SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'assignees'), \
         (SELECT COUNT(*) FROM issue_subscribers WHERE issue_id = $1 AND deleted_at IS NULL), \
         (SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND verb = 'created')",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 1), "default assignee is not tracked");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn labels_get_no_activity_rows() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let label = insert_label(&st.pool, scratch.project_id, scratch.workspace_id, "bug").await;

    let mut body = base_body("label-activity", scratch.state_id);
    body.label_ids = Some(vec![label]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let label_activities: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'labels'",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(label_activities, 0, "Django has no track_labels on create");

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: the three new tests fail — `exactly one created activity row` (0 rows), assignee activity count 1 (0 rows), subscriber count 1 (0 rows).

- [ ] **Step 3: Create the activity write module and wire it**

Create `crates/api/src/routes/issue_activity_write.rs`:

```rust
//! Create-path writers for `issue_activities` and `issue_subscribers`,
//! mirroring Django's `issue_activities_task` create flow
//! (`plane/bgtasks/issue_activities_task.py:557-423`).

use uuid::Uuid;

/// One `verb="created"` row. `actor_id` is the issue creator;
/// `created_by_id` follows Django's `objects.create()` (crum user),
/// `updated_by_id` stays NULL (BaseModel create semantics).
pub(crate) async fn insert_created_activity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    epoch: f64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_activities (id, verb, field, old_value, new_value, comment, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, updated_by_id, epoch, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), 'created', NULL, NULL, NULL, 'created the issue', '{}'::varchar[], \
                $1, $2, $3, $4, $4, NULL, $5, i.created_at, now() \
         FROM issues i WHERE i.id = $1",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(actor)
    .bind(epoch)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One `verb="updated"`, `field="assignees"` "added" row per requested
/// assignee. `created_by_id`/`updated_by_id` stay NULL, matching Django's
/// `bulk_create` path which skips `save()`.
pub(crate) async fn insert_assignee_activities(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    added: &[Uuid],
    epoch: f64,
) -> Result<(), sqlx::Error> {
    if added.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO issue_activities (id, verb, field, old_value, new_value, comment, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, updated_by_id, new_identifier, \
         epoch, created_at, updated_at) \
         SELECT gen_random_uuid(), 'updated', 'assignees', '', u.display_name, 'added assignee ', \
                '{}'::varchar[], $1, $2, $3, $4, NULL, NULL, u.id, $5, now(), now() \
         FROM users u WHERE u.id = ANY($6)",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(actor)
    .bind(epoch)
    .bind(added)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Subscriber rows for added assignees (`ignore_conflicts=True` parity).
/// Django authors each row as the assignee, not the actor.
pub(crate) async fn insert_subscribers(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    subscriber_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if subscriber_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO issue_subscribers (id, issue_id, subscriber_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, u.id, $2, $3, u.id, u.id, now(), now() \
         FROM users u WHERE u.id = ANY($4) \
         ON CONFLICT DO NOTHING",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(subscriber_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

In `crates/api/src/routes/mod.rs`, add `pub mod issue_activity_write;` immediately before the existing `pub mod issue_common;` line (alphabetical order).

In `crates/api/src/routes/issue_write.rs`, add to the `issue_common`/module imports:

```rust
use super::issue_activity_write::{
    insert_assignee_activities, insert_created_activity, insert_subscribers,
};
```

and in `create`, after the `apply_create_bridges(...)` call and before `tx.commit().await?;`:

```rust
    let epoch = chrono::Utc::now().timestamp() as f64;
    insert_created_activity(&mut tx, out.id, project_id, workspace_id, auth.0, epoch).await?;
    if body.assignee_ids.is_some() {
        insert_assignee_activities(
            &mut tx,
            out.id,
            project_id,
            workspace_id,
            auth.0,
            &assignees,
            epoch,
        )
        .await?;
    }
    insert_subscribers(&mut tx, out.id, project_id, workspace_id, &assignees).await?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: 25 passed, 0 failed.

- [ ] **Step 5: Commit**

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_activity_write.rs \
        apps/api-rs/crates/api/src/routes/mod.rs \
        apps/api-rs/crates/api/src/routes/issue_write.rs \
        apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "feat(api-rs): legacy issue create writes activities and subscribers"
```

---

### Task 4: 26-key list-shaped response

**Files:**

- Modify: `crates/api/src/routes/issue_query.rs` (new `fetch_issue_row`)
- Modify: `crates/api/src/routes/issue_write.rs` (response + `missing` import)
- Test: `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing test**

Add at the end of `crates/api/tests/issue_create_test.rs`:

```rust
#[tokio::test]
async fn create_response_has_26_key_list_shape() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let label = insert_label(&st.pool, scratch.project_id, scratch.workspace_id, "urgent").await;

    let mut body = base_body("response-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member]);
    body.label_ids = Some(vec![label]);
    body.priority = Some("medium".to_string());
    body.description_html = Some("<p>resp</p>".to_string());

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);

    let obj = payload.as_object().expect("response must be a JSON object");
    assert_eq!(obj.len(), 26, "response must have exactly the 26 list keys");
    for key in [
        "id",
        "name",
        "state_id",
        "sort_order",
        "completed_at",
        "estimate_point",
        "priority",
        "start_date",
        "target_date",
        "sequence_id",
        "project_id",
        "parent_id",
        "cycle_id",
        "module_ids",
        "label_ids",
        "assignee_ids",
        "sub_issues_count",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
        "attachment_count",
        "link_count",
        "is_draft",
        "archived_at",
        "deleted_at",
    ] {
        assert!(obj.contains_key(key), "missing response key: {key}");
    }
    for absent in ["description_html", "type_id"] {
        assert!(!obj.contains_key(absent), "unexpected response key: {absent}");
    }

    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    let sequence_id = sequence_of(&st.pool, id).await;
    assert_eq!(payload["name"], "response-probe");
    assert_eq!(payload["priority"], "medium");
    assert_eq!(payload["state_id"].as_str().unwrap(), scratch.state_id.to_string());
    assert_eq!(payload["project_id"].as_str().unwrap(), scratch.project_id.to_string());
    assert_eq!(payload["sequence_id"].as_i64(), Some(sequence_id as i64));
    assert_eq!(payload["assignee_ids"], json!([member.to_string()]));
    assert_eq!(payload["label_ids"], json!([label.to_string()]));
    assert_eq!(payload["module_ids"], json!([]));
    assert_eq!(payload["sub_issues_count"], 0);
    assert_eq!(payload["attachment_count"], 0);
    assert_eq!(payload["link_count"], 0);
    assert_eq!(payload["is_draft"], false);
    assert!(payload["updated_by"].is_null());
    assert_eq!(payload["created_by"].as_str().unwrap(), scratch.user_id.to_string());
    assert!(payload["archived_at"].is_null());
    assert!(payload["deleted_at"].is_null());

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test create_response_has_26_key_list_shape -- --test-threads=1
```

Expected: FAIL — response only has 2 keys (`assert_eq!(obj.len(), 26)` fails).

- [ ] **Step 3: Implement the response fetch**

In `crates/api/src/routes/issue_query.rs`, add after `list_by_ids` (the function ending around line 429):

```rust
/// Fetch one issue in the same 26-key shape as the list page. Mirrors the
/// Django create response re-query (`views/issue/base.py:432-441`), which runs
/// through `Issue.issue_objects` (`db/models/issue.py:92-101`): non-deleted,
/// non-archived, non-draft, non-triage, project not archived.
pub(crate) async fn fetch_issue_row(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
) -> Result<Option<IssueListRow>, sqlx::Error> {
    let sql = format!(
        "{LIST_SELECT_SQL} WHERE i.project_id = $1 AND i.id = $2 \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND s.\"group\" <> 'triage' \
         AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id \
         AND p.archived_at IS NULL AND p.deleted_at IS NULL)"
    );
    sqlx::query_as::<_, IssueListRow>(&sql)
        .bind(project_id)
        .bind(issue_id)
        .fetch_optional(pool)
        .await
}
```

In `crates/api/src/routes/issue_write.rs`, change the import at line 5 from `use crate::routes::project::deny;` to `use crate::routes::project::{deny, missing};` and replace the final `Ok((StatusCode::CREATED, ...))` block of `create` with:

```rust
    match super::issue_query::fetch_issue_row(&st.pool, project_id, out.id).await? {
        Some(row) => Ok((
            StatusCode::CREATED,
            Json(serde_json::to_value(row).expect("IssueListRow serializes")),
        )),
        None => Ok(missing()),
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: 26 passed, 0 failed (all previous tests still parse `payload["id"]`).

- [ ] **Step 5: Commit**

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_query.rs \
        apps/api-rs/crates/api/src/routes/issue_write.rs \
        apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "feat(api-rs): legacy issue create returns the 26-key issue row"
```

---

### Task 5: Verification, inventory note, rebuild, HTTP e2e

**Files:**

- Modify: `crates/api/parity-inventory.json`

- [ ] **Step 1: Update the parity inventory note**

In `crates/api/parity-inventory.json` (the entry for `/api/workspaces/:slug/projects/:project_id/issues/`, around line 151), replace:

```json
          "notes": "verified IssueViewSet.list 12-key paginated envelope vs list; POST vs issue_write::create 201"
```

with:

```json
          "notes": "verified IssueViewSet.list 12-key paginated envelope vs list; POST create: full payload persisted, assignee/label bridges + default assignee, created/assignee activities + subscribers, 26-key row 201"
```

- [ ] **Step 2: Format and lint**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 --check \
  crates/api/src/routes/issue_write.rs \
  crates/api/src/routes/issue_common.rs \
  crates/api/src/routes/issue_activity_write.rs \
  crates/api/src/routes/issue_query.rs \
  crates/api/src/routes/v1/work_item.rs \
  crates/api/src/routes/intake.rs \
  crates/api/tests/issue_create_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
```

Expected: rustfmt exit 0; clippy no `error`; pre-existing warnings (e.g. `V1WorkItemQuery` dead code) may remain but no new warning may point into the changed functions.

- [ ] **Step 3: Full Rust suite (no regressions)**

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane REDIS_URL=redis://localhost:6379 \
  cargo test -p api -p common --no-fail-fast -- --test-threads=1
```

Expected: `0 failed` in every target (baseline 1069 passed + 15 new tests).

- [ ] **Step 4: Rebuild the API image and restart the stack**

```bash
cd /home/ghifari/plane-for-itsm
docker compose -f docker-compose-local.yml build api
docker compose -f docker-compose-local.yml up -d api worker beat-worker
sleep 5
docker logs plane-for-itsm-api-1 2>&1 | tail -n 3   # expect: rust-api listening on 8000
```

- [ ] **Step 5: HTTP e2e**

Create scratch data — owner (admin), a project-member assignee, a label, and the default assignee:

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO users (id, password, username, email, first_name, last_name, avatar, date_joined,
                   created_at, updated_at, last_location, created_location, is_superuser,
                   is_managed, is_password_expired, is_active, is_staff, is_email_verified,
                   is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip,
                   last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid,
                   is_password_reset_required)
VALUES ('00000000-0000-0000-0000-00000000f001', '', 'itseq-full-owner', 'o@example.invalid', '', '', '',
        now(), now(), now(), '', '', false, false, false, true, false, false, true, 'tk-f001',
        'UTC', '', '', '', '', false, 'itseq-full-owner', true, false),
       ('00000000-0000-0000-0000-00000000f006', '', 'itseq-full-assignee', 'a@example.invalid', '', '', '',
        now(), now(), now(), '', '', false, false, false, true, false, false, true, 'tk-f006',
        'UTC', '', '', '', '', false, 'itseq-full-assignee', true, false);
INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color)
VALUES ('00000000-0000-0000-0000-00000000f002', 'IT Full E2E', 'itseq-full',
        '00000000-0000-0000-0000-00000000f001', now(), now(), 'UTC', '#FFFFFF');
INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, workspace_id, view_props,
        default_props, issue_props, explored_features, getting_started_checklist, tips, is_active)
VALUES (gen_random_uuid(), now(), now(), 20, '00000000-0000-0000-0000-00000000f001',
        '00000000-0000-0000-0000-00000000f002', '{}', '{}', '{}', '{}', '{}', '{}', true);
INSERT INTO projects (id, created_at, updated_at, name, description, network, identifier,
        workspace_id, cycle_view, module_view, issue_views_view, page_view, intake_view, archive_in,
        close_in, logo_props, is_time_tracking_enabled, is_issue_type_enabled,
        guest_view_all_features, timezone, default_assignee_id)
VALUES ('00000000-0000-0000-0000-00000000f003', now(), now(), 'IT Full E2E', '', 2, 'FULL',
        '00000000-0000-0000-0000-00000000f002', false, false, false, false, false, 30, 30,
        '{}'::jsonb, false, false, false, 'UTC', '00000000-0000-0000-0000-00000000f006');
INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, view_props,
        default_props, sort_order, preferences, created_at, updated_at)
VALUES (gen_random_uuid(), '00000000-0000-0000-0000-00000000f001', 20,
        '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002', true,
        '{}', '{}', 65535, '{}', now(), now()),
       (gen_random_uuid(), '00000000-0000-0000-0000-00000000f006', 15,
        '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002', true,
        '{}', '{}', 65535, '{}', now(), now());
INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, "group",
        "default", is_triage, created_at, updated_at)
VALUES ('00000000-0000-0000-0000-00000000f004', 'Backlog', '', '#60646C', 'backlog',
        '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002', 65535,
        'backlog', true, false, now(), now());
INSERT INTO labels (id, name, color, description, sort_order, project_id, workspace_id,
        created_at, updated_at)
VALUES ('00000000-0000-0000-0000-00000000f007', 'sev1', '#EF4444', '', 65535,
        '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002',
        now(), now());
INSERT INTO api_tokens (id, token, label, user_type, user_id, description, is_active, is_service,
        allowed_rate_limit, created_at, updated_at)
VALUES (gen_random_uuid(), 'itseq-full-owner-key', 'full-owner', 0,
        '00000000-0000-0000-0000-00000000f001', '', true, false, '60/minute', now(), now());
SQL
```

Run the two creates (requested assignee+label+fields, then the default-assignee path):

```bash
URL=http://localhost:8000/api/workspaces/itseq-full/projects/00000000-0000-0000-0000-00000000f003/issues/
H=(-H 'Content-Type: application/json' -H 'Origin: http://localhost:3000' -H 'X-Api-Key: itseq-full-owner-key')

curl -sS -o /tmp/opencode/full_requested.json -w 'requested: HTTP %{http_code}\n' -X POST "$URL" "${H[@]}" -d '{
  "name":"full-requested-probe",
  "description_html":"<p>hello from e2e</p>",
  "priority":"high",
  "start_date":"2026-09-01",
  "target_date":"2026-09-30",
  "state_id":"00000000-0000-0000-0000-00000000f004",
  "assignee_ids":["00000000-0000-0000-0000-00000000f006"],
  "label_ids":["00000000-0000-0000-0000-00000000f007"]
}'

curl -sS -o /tmp/opencode/full_default.json -w 'default: HTTP %{http_code}\n' -X POST "$URL" "${H[@]}" -d '{
  "name":"full-default-probe",
  "state_id":"00000000-0000-0000-0000-00000000f004",
  "assignee_ids":[]
}'

grep -o '"[a-z_]*":' /tmp/opencode/full_requested.json | sort -u | wc -l   # expect: 26
grep -o '"[a-z_]*":' /tmp/opencode/full_default.json | sort -u | wc -l     # expect: 26
```

Expected: `requested: HTTP 201`, `default: HTTP 201`, both key counts 26. Verify DB state:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c \
 "SELECT i.name, i.description_html, i.priority, \
   (SELECT COUNT(*) FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL) AS assignees, \
   (SELECT COUNT(*) FROM issue_labels il WHERE il.issue_id = i.id AND il.deleted_at IS NULL) AS labels, \
   (SELECT COUNT(*) FROM issue_activities a WHERE a.issue_id = i.id) AS activities, \
   (SELECT COUNT(*) FROM issue_subscribers s WHERE s.issue_id = i.id AND s.deleted_at IS NULL) AS subscribers \
  FROM issues i WHERE i.project_id = '00000000-0000-0000-0000-00000000f003' ORDER BY i.created_at;"
```

Expected two rows:

- `full-requested-probe | <p>hello from e2e</p> | high | assignees=1 | labels=1 | activities=2 | subscribers=1`
- `full-default-probe | <p></p> | none | assignees=1 | labels=0 | activities=1 | subscribers=0`

- [ ] **Step 6: Clean up the e2e scratch data**

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 -q <<'SQL'
DELETE FROM issue_activities WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM issue_subscribers WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM issue_assignees WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM issue_labels WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM issue_sequences WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM issues WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM labels WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM states WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM project_members WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM projects WHERE id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM workspace_members WHERE workspace_id = '00000000-0000-0000-0000-00000000f002';
DELETE FROM workspaces WHERE id = '00000000-0000-0000-0000-00000000f002';
DELETE FROM api_tokens WHERE token = 'itseq-full-owner-key';
DELETE FROM users WHERE id IN ('00000000-0000-0000-0000-00000000f001', '00000000-0000-0000-0000-00000000f006');
SQL
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -t -A -c \
 "SELECT 'leftover_full=' || count(*) FROM users WHERE username LIKE 'itseq-full-%';"
```

Expected: `leftover_full=0`.

- [ ] **Step 7: Commit the inventory note**

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "docs(api-rs): parity inventory reflects legacy create full parity"
```

---

## Self-Review Notes

- Spec coverage: fields/validation (Task 1), bridges + default assignee (Task 2), activities + subscribers (Task 3), 26-key response (Task 4), testing + docs + e2e (Tasks 1-5). Deviations (dedupe, single tx, UTC timestamps, no notifications, strict 400) are implemented as decided.
- Type consistency: `parse_date` lives in `issue_common`; `apply_create_bridges` takes slices; `fetch_issue_row` returns `Option<IssueListRow>`; `NewIssue` field names match the INSERT binds.
- No placeholders: every step carries the exact code or command.
