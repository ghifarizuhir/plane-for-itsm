# PATCH Parity Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the small, verified loose ends left by the legacy issue update parity slice: create-path `description_stripped`/`completed_at`, draft default-state ordering, intake description versions, and parity guard comments for the notification/cycle assignee queries.

**Architecture:** Three focused code tasks plus a verification task. Task 1 fixes the shared `insert_issue` creator. Task 2 aligns the draft fallback with `State.Meta.ordering` and documents (does NOT change) the assignee queries that intentionally lack soft-delete filters. Task 3 wires `record_description_version` into the intake create/patch paths, which requires relaxing its executor type from `&mut Transaction` to `&mut PgConnection`.

**Tech Stack:** Rust (axum, sqlx/Postgres), cargo integration tests against the live dev DB.

**Source of this slice:** review findings from the 2026-09-22 legacy PATCH parity plan (`docs/superpowers/plans/2026-09-22-legacy-issue-update-full-parity.md`, deviations 12 and the final-review follow-up list).

---

## Preflight (every test command)

- Work from `apps/api-rs`.
- Tests hit the live dev DB: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.
- Serial only: `-- --test-threads=1`.
- Baseline at slice start: `issue_patch_test` 18 passed, `issue_create_test` 27 passed, `issue_test` 15 passed; full `cargo test -p api -p common --no-fail-fast` green.

## Reconnaissance facts (verified)

| Item | Django behavior                                                                                                                                                                                                                                             | Rust today                                                                             |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| 1a   | `Issue.save` create branch sets `description_stripped = NULL` when html empty else `strip_tags`                                                                                                                                                             | `insert_issue` never writes the column (NULL) → `issue_create_test.rs:955` pins `None` |
| 1b   | `_sync_completed_at` runs on adding: state group `completed` → `completed_at = now()`, else NULL                                                                                                                                                            | `insert_issue` never writes `completed_at`                                             |
| 2a   | `State.objects.filter(...).first()` uses `Meta.ordering = ("sequence",)` (`db/models/state.py:115`)                                                                                                                                                         | `draft.rs::resolve_default_state` orders both lookups by `created_at` (2 queries)      |
| 2b   | `IssueAssignee.objects` is a **plain** manager and `issue__assignees` joins use base managers — Django's notification filters (`views/notification/base.py:112,119`) and cycle lists (`views/workspace/user.py:504-516`) have **no** `deleted_at` predicate | Rust mirrors that (no filter) — correct; add guard comments so it is not "fixed" later |
| 3a   | Intake create calls `issue_description_version_task(..., is_creating=True)` (`views/intake/base.py:292-297`)                                                                                                                                                | `intake.rs::create_issue` does not write a version                                     |
| 3b   | Intake partial update calls the task when an issue was updated (`base.py:447-456`); note `is_description_update` reads the TOP-LEVEL `description_html`, which the web nests under `issue`, so `skip_activity` never suppresses it on this path             | `intake.rs::patch_issue` does not write a version                                      |

## Decisions

1. `description_stripped` is computed in Rust inside `insert_issue` (`strip_tags_text`), so every caller (legacy create, intake create) gets it with no signature change.
2. `completed_at` on create is computed in SQL: `CASE WHEN (SELECT "group" FROM states WHERE id = <state>) = 'completed' THEN now() ELSE NULL END` — no extra roundtrip, correct for NULL state.
3. `record_description_version` changes its executor parameter from `&mut sqlx::Transaction<'_, Postgres>` to `&mut sqlx::PgConnection` so the intake patch path (no transaction) can call it; existing callers pass `&mut *tx`.
4. **Item 2 is a documentation fix, not a behavior fix**: adding `deleted_at` predicates would _break_ Django parity (the final review's suggestion was based on the wrong assumption that Django filters them). Guard comments prevent future "fixes".
5. Intake patch ignores `skip_activity` deliberately: Django's `is_description_update = request.data.get("description_html") is not None` checks the top level while the web sends `issue.description_html`, so the migration flag never suppresses the version on this endpoint (quirk mirrored).
6. The draft ordering test lives in `issue_create_test.rs` (fixture reuse: workspace/project/state already set up there); it is a state-allocation test, same theme as the file's sequence tests.

## File structure

| File                                                      | Change                                                                                             |
| --------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `crates/api/src/routes/issue_write.rs`                    | `insert_issue`: bind `description_stripped`, add `completed_at` CASE                               |
| `crates/api/src/routes/draft.rs`                          | `resolve_default_state`: `ORDER BY sequence ASC, created_at ASC` (both lookups)                    |
| `crates/api/src/routes/notification.rs`                   | guard comments on the three assignee subqueries                                                    |
| `crates/api/src/routes/user.rs`                           | strengthen the cycle-query parity comment                                                          |
| `crates/api/src/routes/issue_version_write.rs`            | executor type `&mut PgConnection`                                                                  |
| `crates/api/src/routes/issue_update.rs`, `issue_write.rs` | call sites pass `&mut *tx`                                                                         |
| `crates/api/src/routes/intake.rs`                         | create: version after `insert_issue`; patch: pre-snapshot + version after the issue UPDATE         |
| `crates/api/tests/issue_create_test.rs`                   | create stripped/completed assertions, draft ordering test, intake version tests, cleanup additions |

---

### Task 1: Create-path `description_stripped` + `completed_at`

**Files:**

- Modify: `crates/api/src/routes/issue_write.rs` (`insert_issue`)
- Test: `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/api/tests/issue_create_test.rs`, update the existing assertion in the create-fields test (around line 955, the `row: (String, String, Option<String>, ...)` block) from

```rust
    assert_eq!(row.2, None);
```

to

```rust
    // `Issue.save` create branch: non-empty html → `strip_tags` ("" here).
    assert_eq!(row.2.as_deref(), Some(""));
```

and append this test:

```rust
#[tokio::test]
async fn create_writes_stripped_and_completed_at() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let done = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'Done', '', '#16A34A', 'done', $2, $3, 65535, 'completed', false, false, now(), now())",
    )
    .bind(done)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            state_id: Some(done),
            description_html: Some("<p>hi <b>there</b></p>".to_string()),
            ..base_body("completed-create", scratch.state_id)
        }),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    let (stripped, completed): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT description_stripped, completed_at FROM issues WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stripped.as_deref(), Some("hi there"));
    assert!(completed.is_some(), "create into a completed state stamps completed_at");

    // Non-completed state (fixture backlog) → completed_at stays NULL.
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(base_body("backlog-create", scratch.state_id)),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    let completed: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(completed, None);

    scratch.cleanup(&pool).await;
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: the updated assertion and the new test fail (`None` vs `Some("")`, `completed_at` NULL).

- [ ] **Step 3: Implement**

In `crates/api/src/routes/issue_write.rs::insert_issue`, before the INSERT:

```rust
    // `Issue.save` create branch (`db/models/issue.py:200-205`): empty html
    // stores NULL, anything else stores the tag-stripped text.
    let description_stripped: Option<String> = if issue.description_html.is_empty() {
        None
    } else {
        Some(super::page::strip_tags_text(issue.description_html))
    };
```

Replace the INSERT with (columns and values updated; everything else unchanged):

```rust
    let row: (uuid::Uuid, String) = sqlx::query_as(
        "INSERT INTO issues (id, name, description_html, description_json, priority, start_date, \
         target_date, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, \
         created_by_id, updated_by_id, estimate_point_id, type_id, parent_id, description_stripped, \
         completed_at, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, '{}', $3, $4, $5, false, \
         COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $6 AND state_id IS NOT DISTINCT FROM $7), 65535 - 10000) + 10000, \
         $8, $7, $6, w.id, $9, NULL, $10, $11, $12, $14, \
         CASE WHEN (SELECT \"group\" FROM states WHERE id = $7) = 'completed' THEN now() ELSE NULL END, \
         now(), now() \
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
    .bind(description_stripped)
    .fetch_one(&mut **tx)
    .await?;
```

- [ ] **Step 4: Run to verify pass**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: all pass (28 tests), including the intake create test (shared creator) and the version test (its stripped computation is independent).

- [ ] **Step 5: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_write.rs crates/api/tests/issue_create_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_write.rs apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "fix(api-rs): create path writes description_stripped and completed_at"
```

---

### Task 2: Draft state ordering, conversion stripped, parity guard comments

**Files:**

- Modify: `crates/api/src/routes/draft.rs` (`resolve_default_state`, `create_draft_to_issue`, module note)
- Modify: `crates/api/src/routes/notification.rs` (3 subqueries)
- Modify: `crates/api/src/routes/user.rs` (cycle queries comment)
- Test: `crates/api/tests/issue_create_test.rs` (draft ordering test + cleanup)

- [ ] **Step 1: Write the failing test**

Append to `crates/api/tests/issue_create_test.rs` (add `use api::routes::draft::{create as draft_create, CreateDraftBody};` to its imports):

```rust
#[tokio::test]
async fn draft_default_state_uses_state_sequence_ordering() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    // Fixture state: sequence 65535, default=true, created FIRST. A second
    // default with a LOWER sequence but created LATER must win — Django
    // resolves via `State.objects.filter(...).first()` with
    // `Meta.ordering = ("sequence",)` (`db/models/state.py:115`,
    // `db/models/draft.py:84-98`).
    let second = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'First by sequence', '', '#60646C', 'first-by-seq', $2, $3, 100, 'backlog', true, false, now(), now())",
    )
    .bind(second)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, Json(_)) = draft_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Some(Json(CreateDraftBody {
            project_id: Some(scratch.project_id),
            ..Default::default()
        })),
    )
    .await
    .expect("draft create must respond");
    assert_eq!(status, StatusCode::CREATED);

    let state_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT state_id FROM draft_issues WHERE project_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(scratch.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_id, Some(second), "sequence ordering must win over created_at");

    scratch.cleanup(&pool).await;
}
```

Also add to `Scratch::cleanup` (before the `states` delete, since `draft_issues.state_id` FKs states) and to `purge`'s per-project statement list:

```rust
        sqlx::query("DELETE FROM draft_issues WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
```

(`purge` gets `"DELETE FROM draft_issues WHERE project_id = $1",` in its statement array.)

Append the conversion test as well:

```rust
#[tokio::test]
async fn draft_conversion_writes_description_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let draft_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO draft_issues (id, name, description_html, description_json, priority, sort_order, \
         state_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
         VALUES ($1, 'draft-probe', '<p>draft</p>', '{}', 'none', 65535, $2, $3, $4, $5, now(), now())",
    )
    .bind(draft_id)
    .bind(scratch.state_id)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .bind(scratch.user_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, Json(body)) = draft_convert(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), draft_id)),
        Some(Json(ConvertBody {
            name: Some("converted-probe".to_string()),
            description_html: Some("<p>converted <b>body</b></p>".to_string()),
            ..Default::default()
        })),
    )
    .await
    .expect("draft conversion must respond");
    assert_eq!(status, StatusCode::CREATED);
    let issue_id: Uuid = body["id"].as_str().expect("id").parse().unwrap();

    let stripped: Option<String> = sqlx::query_scalar("SELECT description_stripped FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stripped.as_deref(), Some("converted body"));

    scratch.cleanup(&pool).await;
}
```

And the import line becomes:

```rust
use api::routes::draft::{
    create as draft_create, create_draft_to_issue as draft_convert, ConvertBody, CreateDraftBody,
};
```

- [ ] **Step 2: Run to verify failure**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test draft_default_state -- --test-threads=1
```

Expected: FAIL — the draft gets the fixture state (created first), not `second`.

- [ ] **Step 3: Implement**

`crates/api/src/routes/draft.rs::create_draft_to_issue` — replace the promotion INSERT so it also writes `description_stripped` (Django computes it in `Issue.save`, `db/models/issue.py:200-205`; the module note claiming it is never computed is now false). Before the INSERT add:

```rust
    // Same rule as `insert_issue`/`Issue.save`: empty → NULL else stripped.
    // The INSERT stores `COALESCE($2, '<p></p>')`, so mirror that value.
    let effective_html: String = b
        .description_html
        .clone()
        .unwrap_or_else(|| "<p></p>".to_string());
    let description_stripped: Option<String> = if effective_html.is_empty() {
        None
    } else {
        Some(super::page::strip_tags_text(&effective_html))
    };
```

then add the column/values/bind (new `$16`):

```rust
        "INSERT INTO issues (id, name, description_html, description_json, priority, \
         start_date, target_date, sequence_id, sort_order, completed_at, is_draft, \
         estimate_point_id, parent_id, type_id, state_id, description_stripped, project_id, \
         workspace_id, created_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, COALESCE($2, '<p></p>'), '{}', COALESCE($3, 'none'), \
         $4, $5, $6, $7, $8, false, $9, $10, $11, $12, $16, $13, $14, $15, now(), now()) RETURNING id",
```

```rust
    .bind(auth.0)
    .bind(description_stripped)
    .fetch_one(&mut *tx)
    .await?;
```

Also refresh the module note (draft.rs:96-98) from

```rust
/// - `description_stripped` never computed (needs the html parser;
///   sibling writers skip it too); `sort_order`/default-state/`completed_at`
///   ARE mirrored from `DraftIssue.save` / `Issue.save` (see handlers).
```

to

```rust
/// - `sort_order`/default-state/`completed_at`/`description_stripped` are
///   mirrored from `DraftIssue.save` / `Issue.save` (see handlers).
```

**Plus (from the Task 2 code-quality review):** `crates/api/src/routes/issue_write.rs` legacy create must stop normalizing an explicit `description_html: ""` to `<p></p>` — Django's `Issue.save` (`db/models/issue.py:196-206`) stores `""` with `description_stripped = NULL` (DRF maps `blank=True` → `allow_blank=True`), while omitting the field uses the model default `<p></p>` (stripped `""`). Use `body.description_html.as_deref().unwrap_or("<p></p>")`, add `create_with_explicit_empty_html_stores_empty_html_and_null_stripped`, and keep the omitted-html assertion (`create_defaults_description_and_priority_when_absent`). Also: the second conversion case (`description_html: Some("")` → stored `""`/stripped `None`), a `// $16 = description_stripped, bound last.` comment above the promotion INSERT, the ordering test asserting the fixture state's sequence exceeds the probe's, and accurate guard-comment citations in `notification.rs` (`IssueSubscriber.objects`/`IssueAssignee.objects`; mark-all-read `base.py:269-273`).

`crates/api/src/routes/draft.rs::resolve_default_state` — both queries change their ORDER BY and the doc comment gains the ordering note:

```rust
/// Resolves the effective state for a new draft/issue, mirroring
/// `DraftIssue.save` (`db/models/draft.py:84-98`) / `Issue._ensure_default_state`
/// (`db/models/issue.py:231-243`): explicit id wins; else the project's
/// default non-triage state, else the first non-triage state, else None.
/// Django's `.first()` uses `State.Meta.ordering = ("sequence",)`
/// (`db/models/state.py:115`), hence `sequence, created_at` here.
async fn resolve_default_state(
```

```rust
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
```

```rust
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
```

`crates/api/src/routes/notification.rs` — add the same guard comment above each of the three assignee subqueries (the two `type_clauses` at ~205-214 and the mark-all-read `assigned` clause at ~397):

```rust
    // Django's `IssueAssignee.objects` is a plain manager and related
    // lookups use base managers — these filters intentionally have NO
    // `deleted_at` predicate (parity with `views/notification/base.py:112,119`).
```

`crates/api/src/routes/user.rs` — extend the existing cycle comment (the two `JOIN issue_assignees ia` queries) to:

```rust
    // Cycles literal Django (`workspace/user.py:504-516`): no
    // deleted/archived guard, and `issue__assignees` joins do not apply
    // soft-delete managers — intentional parity.
```

- [ ] **Step 4: Run to verify pass**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: all pass (32 tests: 31 + the legacy-create empty-html test), no leftovers (`scratch.cleanup` now removes drafts).

- [ ] **Step 5: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/draft.rs crates/api/src/routes/notification.rs crates/api/src/routes/user.rs crates/api/tests/issue_create_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/draft.rs apps/api-rs/crates/api/src/routes/notification.rs \
  apps/api-rs/crates/api/src/routes/user.rs apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "fix(api-rs): draft default state follows Django sequence ordering"
```

---

### Task 3: Intake description versions (create + patch)

**Files:**

- Modify: `crates/api/src/routes/issue_version_write.rs` (executor type)
- Modify: `crates/api/src/routes/issue_update.rs`, `crates/api/src/routes/issue_write.rs` (call sites)
- Modify: `crates/api/src/routes/intake.rs` (create + patch)
- Test: `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/api/tests/issue_create_test.rs` (add `api::routes::intake::{patch_issue as intake_patch_issue, InboxIssueFields, InboxIssuePatch}` to the intake import):

```rust
#[tokio::test]
async fn intake_create_and_patch_record_description_versions() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    sqlx::query(
        "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, \
         project_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Intake', '', true, '{}'::jsonb, '{}'::jsonb, $1, $2, now(), now())",
    )
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .expect("scratch intake");

    let (status, Json(_)) = intake_create_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIntakeIssue {
            issue: IntakeIssuePayload {
                name: Some("intake-version-probe".to_string()),
                priority: Some("none".to_string()),
            },
        }),
    )
    .await
    .expect("intake create must respond");
    assert_eq!(status, StatusCode::OK);

    let issue_id: Uuid = sqlx::query_scalar("SELECT id FROM issues WHERE project_id = $1")
        .bind(scratch.project_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    // Django calls the task with `is_creating=True` (`intake/base.py:292-297`).
    let versions: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT description_html, updated_by_id FROM issue_description_versions WHERE issue_id = $1",
    )
    .bind(issue_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 1, "intake create records the initial version");
    assert_eq!(versions[0].0, "<p></p>");
    assert_eq!(versions[0].1, None, "create leaves updated_by NULL");

    // Intake PATCH with a nested description (`base.py:447-456`); the same
    // actor edits within 600 s, so the create row is merged in place.
    let (status, _) = intake_patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(InboxIssuePatch {
            issue: Some(InboxIssueFields {
                description_html: Some("<p>intake edit</p>".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
    )
    .await
    .expect("intake patch must respond");
    assert_eq!(status, StatusCode::OK);

    let versions: Vec<(String,)> = sqlx::query_as(
        "SELECT description_html FROM issue_description_versions WHERE issue_id = $1",
    )
    .bind(issue_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 1, "same-owner edit merges into the create row");
    assert_eq!(versions[0].0, "<p>intake edit</p>");

    // Unchanged description → no new version.
    let (status, _) = intake_patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(InboxIssuePatch {
            issue: Some(InboxIssueFields {
                description_html: Some("<p>intake edit</p>".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
    )
    .await
    .expect("intake patch must respond");
    assert_eq!(status, StatusCode::OK);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_description_versions WHERE issue_id = $1",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);

    scratch.cleanup(&pool).await;
}
```

- [ ] **Step 2: Run to verify failure**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test intake_create_and_patch -- --test-threads=1
```

Expected: FAIL — zero version rows after intake create.

- [ ] **Step 3: Relax the version writer's executor**

In `crates/api/src/routes/issue_version_write.rs`, change both occurrences of

```rust
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
```

to

```rust
    conn: &mut sqlx::PgConnection,
```

rename the body's `&mut **tx` to `&mut *conn` (three call sites inside), and update the doc comment to note the executor is a plain connection so the intake patch path (no transaction) can call it too.

Update the two existing callers:

- `crates/api/src/routes/issue_update.rs`: `record_description_version(&mut *tx, ...)`
- `crates/api/src/routes/issue_write.rs`: `super::issue_version_write::record_description_version(&mut *tx, ...)`

- [ ] **Step 4: Wire the intake paths**

In `crates/api/src/routes/intake.rs::create_issue`, after `insert_issue(...)` and before the `intake_issues` insert (inside the transaction):

```rust
    // Django create records the initial description version
    // (`views/intake/base.py:292-297`, `is_creating=True`).
    super::issue_version_write::record_description_version(
        &mut *tx,
        issue.id,
        project_id,
        workspace_id,
        auth.0,
        Some(auth.0),
        None,
        "<p></p>",
        &json!({}),
    )
    .await?;
```

In `crates/api/src/routes/intake.rs::patch_issue`, in BOTH issue-write branches (`!narrowed` and `narrowed`), before the branch, snapshot the pre-update row:

```rust
    // Pre-update snapshot for the description-version diff
    // (`issue_description_version_task` compares the old html with the
    // stored one, `bgtasks/issue_description_version_task.py:57-59`).
    let pre: (String, Value, Option<uuid::Uuid>, uuid::Uuid) = sqlx::query_as(
        "SELECT i.description_html, i.description_json, i.created_by_id, i.workspace_id \
         FROM issues i WHERE i.id = $1",
    )
    .bind(issue_id)
    .fetch_one(&st.pool)
    .await?;
```

and after the whole `if !narrowed { ... } else { ... }` block, before the intake-level write:

```rust
    // Intake PATCH version parity (`views/intake/base.py:447-456`). Django
    // suppresses it only for the migration-client case
    // (`skip_activity and is_description_update`, base.py:337,437), where
    // `is_description_update` probes the TOP-LEVEL `description_html`; the
    // web nests it under `issue`, so the probe is None and the version is
    // written. Mirrored here: Rust's intake body drops top-level keys, so
    // that migration shape has no counterpart on this path.
    if let Some(new_html) = issue.and_then(|i| i.description_html.clone()) {
        if new_html != pre.0 {
            let new_json = issue
                .and_then(|i| i.description_json.clone())
                .unwrap_or(pre.1.clone());
            let mut conn = st.pool.acquire().await?;
            super::issue_version_write::record_description_version(
                &mut conn,
                issue_id,
                project_id,
                pre.3,
                user_id,
                pre.2,
                Some(user_id),
                &new_html,
                &new_json,
            )
            .await?;
        }
    }
```

(`&mut conn` where `conn: PoolConnection<Postgres>` derefs to `&mut PgConnection`.)

- [ ] **Step 5: Run to verify pass**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_test -- --test-threads=1
```

Expected: create 33 passed (32 + the new intake version test), patch 18 passed, intake unit tests pass.

- [ ] **Step 6: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_version_write.rs crates/api/src/routes/issue_update.rs \
  crates/api/src/routes/issue_write.rs crates/api/src/routes/intake.rs crates/api/tests/issue_create_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_version_write.rs apps/api-rs/crates/api/src/routes/issue_update.rs \
  apps/api-rs/crates/api/src/routes/issue_write.rs apps/api-rs/crates/api/src/routes/intake.rs \
  apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "feat(api-rs): intake create and patch record description versions"
```

---

### Task 4: Verification, rebuild, restart

- [ ] **Step 1: Full Rust suite**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane REDIS_URL=redis://localhost:6379 \
  cargo test -p api -p common --no-fail-fast -- --test-threads=1
```

Expected: 0 failed (baseline 1118 + 3 new tests).

- [ ] **Step 2: Rebuild the API image and restart the stack**

```bash
cd /home/ghifari/plane-for-itsm
docker compose -f docker-compose-local.yml build api
docker compose -f docker-compose-local.yml up -d api worker beat-worker
sleep 5
docker logs plane-for-itsm-api-1 2>&1 | tail -n 3
```

Expected: `rust-api listening on 8000`.

- [ ] **Step 3: Live health check**

```bash
curl -s -o /dev/null -w 'health: HTTP %{http_code}\n' https://api.terraline.space/health
```

Expected: `health: HTTP 200`.

- [ ] **Step 4: Note the intake behavior in the parity inventory**

In `crates/api/parity-inventory.json`, append to the notes of the intake-issues PATCH route entry (search for `inbox-issues`/`intake-issues`):

```
 | Follow-ups: intake create/patch now record issue_description_versions (Django base.py:292-297,447-456).
```

Commit:

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "docs(api-rs): parity inventory notes intake description versions"
```

---

## Self-review notes

- Coverage: item 1 → Task 1; item 2 (documentation, not behavior) → Task 2; item 3 → Task 2 (+ the draft→issue conversion gap found in Task 1's review); item 4 → Task 2; intake versions + verification → Tasks 3-4.
- Type consistency: `record_description_version` executor is `&mut PgConnection` everywhere after Task 3 (`&mut *tx` in tx callers, `&mut conn` from the pool in intake patch).
- The draft test's cleanup addition prevents an FK-blocked workspace delete (draft rows reference states/projects).
- No placeholders: every step carries the exact code/command.
