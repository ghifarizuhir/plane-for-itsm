# Legacy Issue Update (PATCH) Full Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `PATCH /api/workspaces/:slug/projects/:project_id/issues/:pk/` persists every write-field the web sends (state, assignees, labels, dates, parent, type, estimate, sort order, point), mirrors Django's serializer validation and `Issue.save` side-effects (`description_stripped`, `completed_at`, `updated_by`), writes per-field `issue_activities` + `issue_subscribers`, records `issue_description_versions`, and still answers 204 / 404 `{"error": "Issue not found"}` / 403.

**Architecture:** Approach split like the create slice — a new `issue_update.rs` owns the request struct, validation and handler; `issue_activity_write.rs` gains a generic activity-row writer; a new `issue_version_write.rs` owns the description-version merge/insert; `issue_common.rs` gains the shared tri-state deserializers and helpers moved out of `issue_write.rs`. All writes run in one transaction.

**Tech Stack:** Rust (axum, sqlx/Postgres), cargo integration tests against the live dev DB.

**Spec:** none — this plan is the design record. It supersedes the create-slice deviations only where noted below.

---

## Preflight (every test command)

- Work from `apps/api-rs`.
- Tests hit the live dev DB: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.
- Serial only: `-- --test-threads=1` (the test fixtures `purge()` all scratch rows globally).
- Baseline: `cargo test -p api -p common --no-fail-fast -- --test-threads=1` is green (1084 passed at the end of the create slice).
- DB shell: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "..."`.

---

## Verified parity gap (source of this plan)

Rust today (`work_item.rs:95-109`, `1789-1859`): `PatchIssue` has only `name`, `description_html`, `description`(→`description_json`), `priority`; one `UPDATE ... COALESCE` writes those 4 columns; no bridges, no activities, no side-effects. Web sends far more through this exact endpoint:

| Web field                                   | Call site                                                                                                                                                                                                                 | Rust today                      |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `state_id`                                  | `issue-layouts/properties/all-properties.tsx:108`, kanban dnd `utils.tsx:600-605`                                                                                                                                         | dropped                         |
| `label_ids`, `assignee_ids`                 | `all-properties.tsx:116,120`, `issue-detail/sidebar.tsx:106`, `peek-overview/properties.tsx:99`, `spreadsheet/columns/assignee-column.tsx:31`, `sub-issues/issues-list/properties.tsx:215`, `relations/properties.tsx:57` | dropped                         |
| `start_date`, `target_date`                 | `issue-detail/sidebar.tsx:148`; kanban dnd `target_date: null`                                                                                                                                                            | dropped                         |
| `parent_id`                                 | `parent-select-root.tsx:47`                                                                                                                                                                                               | dropped                         |
| `estimate_point`                            | `issue-detail/sidebar.tsx:192`                                                                                                                                                                                            | dropped                         |
| `sort_order`                                | kanban dnd `utils.tsx:506-533` → `use-group-dragndrop.ts:96`                                                                                                                                                              | dropped                         |
| `description_html` + `skip_activity:"true"` | `issue-detail/main-content.tsx:119-124`, `peek-overview/issue-detail.tsx:121`, `inbox/content/issue-root.tsx:157`                                                                                                         | stored, `skip_activity` ignored |
| `type_id`                                   | `issue-modal/form.tsx:239`                                                                                                                                                                                                | dropped (Django ignores it too) |

Django reference: `IssueViewSet.partial_update` (`plane/app/views/issue/base.py:627-713`) → `IssueCreateSerializer(partial=True)` (`serializers/issue.py:82-274`) → `Issue.save` (`db/models/issue.py:180-256`) → Celery `issue_activity.delay(type="issue.activity.updated")` (`bgtasks/issue_activities_task.py:594-638`) + `issue_description_version_task.delay` (`bgtasks/issue_description_version_task.py:44-80`).

---

## Django contract to mirror (locked facts)

Green path: gate → 404 `{"error": "Issue not found"}` (miss body verbatim) → serializer vs model → **204 empty**.

**Scalar null semantics** (DRF with `fields="__all__"` + `partial=True`):

| Key                                                                      | null                                                                 | absent                 |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------- | ---------------------- |
| `name`, `description_html`, `description_json`, `priority`, `sort_order` | 400 (model `null=False`)                                             | no change              |
| `state_id`                                                               | clear → `Issue._ensure_default_state` picks default/first non-triage | no change              |
| `parent_id`, `estimate_point`, `point`, `start_date`, `target_date`      | clear                                                                | no change              |
| `assignee_ids`, `label_ids`                                              | 400 (`ListField` has no `allow_null`)                                | no change; `[]` clears |

`type_id`: Django's serializer key is `type` (FK) — the web's `type_id` is silently ignored by Django. This plan accepts `type_id` only (a web-driven superset consistent with the create slice); Django's `type` key is not accepted and is a documented deviation.

**Validation (all → 400; Django order `serializers/issue.py:127-196`):** start>target only when both keys present; HTML sanitize (`nh3`; invalid → `"html content is not valid"`); assignee ids must be active project members role ≥ 15; label ids must be project labels; state must be in project; parent must be in project; estimate point must be in project. The fork's create decision (#9526 strict 400, not Django's silent filter) is kept for `assignee_ids`/`label_ids`.

**`Issue.save` side-effects on every update** (`db/models/issue.py:180-256`): `description_stripped = NULL` when html empty else `strip_tags(description_html)`; when `state_id` changes → `completed_at = now()` if new group is `completed` else `NULL`; `updated_by = request.user` + `updated_at = now()` (`db/models/base.py:43-46`); when the current state was NULL, `_ensure_default_state` assigns default/first non-triage state even without a `state_id` key.

**Activities** (`issue_activities_task.py:594-638`, written only for keys present in the request):

| Key                          | comparison                              | row                                                                                                                                                                       |
| ---------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`                       | raw vs current                          | field `name`, `"updated the name to"`                                                                                                                                     |
| `description_html`           | raw vs current html                     | field `description`, `"updated the description to"`; if the issue's **last** activity is `description` by the **same actor** → bump its `created_at` instead of inserting |
| `parent_id`                  | id                                      | field `parent`, values `IDENT-SEQ` or `""`, identifiers = parent issue ids                                                                                                |
| `priority`                   | raw                                     | field `priority`                                                                                                                                                          |
| `state_id`                   | raw (null ⇒ new side `None`) vs current | field `state`, values = state names (may be NULL), identifiers = state ids                                                                                                |
| `target_date` / `start_date` | raw vs current                          | fields `target_date` / `start_date`, values or `""`, comments `"updated the target date to"` / `"updated the start date to "` (trailing space)                            |
| `label_ids`                  | added/dropped sets                      | field `labels`, `"added label "` / `"removed label "`, values = label names                                                                                               |
| `assignee_ids`               | added/dropped sets                      | field `assignees`, `"added assignee "` / `"removed assignee "`, values = `display_name`; **added assignees also get `issue_subscribers` rows**                            |
| `estimate_point`             | id                                      | field `estimate_<estimates.type>`, `"updated the estimate point to "`, values = point `value`                                                                             |

Activity rows: `verb='updated'`, `attachments='{}'`, `actor_id=actor`, `created_by_id`/`updated_by_id` NULL (bulk_create), `epoch` = unix seconds.

**`skip_activity`** (`base.py:632-634,678-680`): web sends the string `"true"` with `description_html`. When truthy **and** `description_html` is present as a value, Django skips activities, `model_activity` and description versions (the issue update itself still happens). Without `description_html` the flag is ignored.

**Description versions** (`issue_description_version_task.py`): skip when the description is unchanged; else find the latest row for the issue — same `owned_by` and `last_saved_at` ≤ 600 s ago → update `description_html/json/binary/stripped/last_saved_at` in place (`update_fields`, so `updated_by`/`updated_at` untouched); else insert (`created_by_id = issue.created_by_id`, `updated_by_id = issue.updated_by_id` — the actor on update, NULL on create). Django calls this on **create** too (`base.py:483-488`, `is_creating=True`).

---

## Decisions & documented deviations

1. **Strict 400** for invalid assignee/label/state/parent/estimate/type ids (create-slice decision, not Django's silent filter).
2. **Malformed UUID/type in the body → 422** from the Axum `Json` extractor (create-slice precedent). Django 400s. Not changed in this slice.
3. **`assignee_ids: null` / `label_ids: null` → 400** (`ListField` has no `allow_null`, Django-exact); `[]` clears. Null/empty _elements_ inside the array are skipped by the lax deserializer (Django 400s them) — documented laxness, the web never sends them.
4. **Bridges soft-delete** (`replace_bridges`) — this MATCHES Django: `IssueAssignee`/`IssueLabel` use `SoftDeletionQuerySet.delete(soft=True)` (`db/mixins.py:56-63`), so `.filter(issue=...).delete()` sets `deleted_at` too. The real divergence is audit fields: new rows carry `created_by_id = updated_by_id = actor`, while Django copies the issue's `created_by_id` and old `updated_by_id` (`serializers/issue.py:283-284`). Already the v1/create behavior.
5. **`description` maps to `description_json`** (existing struct behavior; web sends only `description_html`). Django's own key `description_json` is not accepted.
6. **Estimate clear writes no estimate activity**: Django's `track_estimate_points` raises `AttributeError` on `new_estimate is None`, the task's try/except swallows it and the whole activity batch is lost. Rust writes every other activity and skips the estimate row (saner, documented).
7. **`closed_to`**, `archived_at`, `is_draft`, `description_stripped`, `description_binary`, `sequence_id`, `external_source`, `external_id` are out of scope: web never sends them through this endpoint (archive/draft/intake have their own endpoints); `description_stripped` is derived server-side, not client-writable.
8. **Notifications / `model_activity` webhooks / `origin` redis write** are not built (create-slice precedent, infra absent).
9. **One transaction** for issue + bridges + activities + versions; Django autocommits the issue then writes side-effects asynchronously.
10. **Activity batch loss on estimate clear is not reproduced** (deviation 6); otherwise the activity rows are byte-identical in shape.
11. **Activity batch ordering is a fixed field order** (description first, then name/parent/priority/state/dates/labels/assignees/estimate) with per-statement `clock_timestamp()`; Django's `bulk_create` timestamps rows in the request's JSON key order. Observable only when one request mixes `description_html` with other tracked fields AND a later same-actor description-only edit decides merge-vs-insert; the web sends description edits alone in practice.

---

## File structure

| File                                                     | Responsibility                                                                                                                                                           |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `crates/api/src/routes/issue_update.rs` **(new)**        | `PatchIssue` struct + tri-state deserializers, validation, current-row snapshot, the PATCH handler                                                                       |
| `crates/api/src/routes/issue_version_write.rs` **(new)** | `record_description_version` (merge ≤600 s / insert)                                                                                                                     |
| `crates/api/src/routes/issue_activity_write.rs`          | add `ActivityCtx`, `insert_activity_row`                                                                                                                                 |
| `crates/api/src/routes/issue_common.rs`                  | move `bad`, `PRIORITIES`, `dedupe_ids`, `resolve_issue_state`/`resolve_effective_state`, `de_opt_uuid_lax`, `de_opt_uuid_vec_lax`; add `de_double_opt_*` (incl. the vec) |
| `crates/api/src/routes/issue_write.rs`                   | import moved helpers; call `record_description_version` on create                                                                                                        |
| `crates/api/src/routes/work_item.rs`                     | remove the moved PATCH code + its unit test                                                                                                                              |
| `crates/api/src/routes/mod.rs`, `crates/api/src/main.rs` | module + route wiring                                                                                                                                                    |
| `crates/api/tests/issue_patch_test.rs` **(new)**         | integration tests (fixture, scalar/bridge/activity/version/validation)                                                                                                   |
| `crates/api/tests/issue_create_test.rs`                  | one create-path version test (Task 5)                                                                                                                                    |
| `crates/api/parity-inventory.json`                       | route note                                                                                                                                                               |

---

### Task 1: Extract the handler to `issue_update.rs`, full request surface, 400 validation

**Files:**

- Create: `crates/api/src/routes/issue_update.rs`
- Modify: `crates/api/src/routes/mod.rs` (add `pub mod issue_update;` next to `pub mod issue_write;`)
- Modify: `crates/api/src/main.rs:1197` and `:1274` (both `/issues/:pk/` and `/work-items/:pk/` route the same handler → `routes::issue_update::patch_issue`)
- Modify: `crates/api/src/routes/work_item.rs` (delete `PatchIssue` at 95-109, `validate_issue_patch` at 144-156, `ISSUE_PATCH_MISS_MSG` at 1787, `patch_issue` at 1789-1859, and the unit test `patch_miss_string_is_issue_not_found_not_missing` at 2186-2191)
- Modify: `crates/api/src/routes/issue_common.rs`
- Modify: `crates/api/src/routes/issue_write.rs`
- Modify: `crates/api/tests/issue_test.rs:2` (import `resolve_effective_state` from `issue_common` after the move)
- Create: `crates/api/tests/issue_patch_test.rs`

- [ ] **Step 1: Move shared helpers into `issue_common.rs`**

Append to `crates/api/src/routes/issue_common.rs` (add `use axum::http::StatusCode; use axum::Json; use serde::Deserialize;` to its imports):

```rust
/// 400 `{"error": msg}` — the flat error style both create and patch use
/// (moved from `issue_write.rs`).
pub(crate) fn bad(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg })))
}

/// Issue priority enum (`plane/db/models/issue.py:141-146`).
pub(crate) const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

/// Order-preserving dedupe (moved from `issue_write.rs`).
pub(crate) fn dedupe_ids(ids: &Option<Vec<Uuid>>) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    ids.as_deref()
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect()
}

/// `""`/null → `None`; otherwise a UUID (moved from `issue_write.rs`).
pub(crate) fn de_opt_uuid_lax<'de, D>(d: D) -> Result<Option<Uuid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<Value> = Option::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => Uuid::parse_str(s.trim()).map(Some).map_err(serde::de::Error::custom),
        Some(other) => Err(serde::de::Error::custom(format!("invalid UUID: {other}"))),
    }
}

/// Same lax rule for id vectors (moved from `issue_write.rs`).
pub(crate) fn de_opt_uuid_vec_lax<'de, D>(d: D) -> Result<Option<Vec<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<Value> = Option::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Null => continue,
                    Value::String(s) if s.trim().is_empty() => continue,
                    Value::String(s) => {
                        out.push(Uuid::parse_str(s.trim()).map_err(serde::de::Error::custom)?)
                    }
                    other => return Err(serde::de::Error::custom(format!("invalid UUID: {other}"))),
                }
            }
            Ok(Some(out))
        }
        Some(other) => Err(serde::de::Error::custom(format!("invalid UUID list: {other}"))),
    }
}

/// Tri-state UUID for PATCH: absent → `None`, `null`/`""` → `Some(None)`
/// (explicit clear), value → `Some(Some(id))`.
pub(crate) fn de_double_opt_uuid_lax<'de, D>(d: D) -> Result<Option<Option<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        Value::Null => Ok(Some(None)),
        Value::String(s) if s.trim().is_empty() => Ok(Some(None)),
        Value::String(s) => Uuid::parse_str(s.trim())
            .map(|u| Some(Some(u)))
            .map_err(serde::de::Error::custom),
        other => Err(serde::de::Error::custom(format!("invalid UUID: {other}"))),
    }
}

/// Tri-state id vector: absent → `None`, `null` → `Some(None)` (Django
/// 400s it), array → `Some(Some(ids))` with null/empty elements skipped.
pub(crate) fn de_double_opt_uuid_vec_lax<'de, D>(d: D) -> Result<Option<Option<Vec<Uuid>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        Value::Null => Ok(Some(None)),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Null => continue,
                    Value::String(s) if s.trim().is_empty() => continue,
                    Value::String(s) => {
                        out.push(Uuid::parse_str(s.trim()).map_err(serde::de::Error::custom)?)
                    }
                    other => return Err(serde::de::Error::custom(format!("invalid UUID: {other}"))),
                }
            }
            Ok(Some(Some(out)))
        }
        other => Err(serde::de::Error::custom(format!("invalid UUID list: {other}"))),
    }
}

/// Tri-state string: absent → `None`, `null` → `Some(None)`, value →
/// `Some(Some(s))`. Empty strings stay `Some(Some(""))` so per-field
/// validation can reject them where Django does.
pub(crate) fn de_double_opt_string<'de, D>(d: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        Value::Null => Ok(Some(None)),
        Value::String(s) => Ok(Some(Some(s))),
        other => Err(serde::de::Error::custom(format!("invalid string: {other}"))),
    }
}

/// Tri-state JSON: absent → `None`, `null` → `Some(None)`, value →
/// `Some(Some(v))`.
pub(crate) fn de_double_opt_json<'de, D>(d: D) -> Result<Option<Option<Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = <Option<Value>>::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        None => Ok(Some(None)),
        Some(v) => Ok(Some(Some(v))),
    }
}

/// Tri-state number: absent → `None`, `null` → `Some(None)`, number →
/// `Some(Some(f64))`.
pub(crate) fn de_double_opt_f64<'de, D>(d: D) -> Result<Option<Option<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        Value::Null => Ok(Some(None)),
        Value::Number(n) => n
            .as_f64()
            .map(|f| Some(Some(f)))
            .ok_or_else(|| serde::de::Error::custom("invalid number")),
        other => Err(serde::de::Error::custom(format!("invalid number: {other}"))),
    }
}

/// Tri-state integer: absent → `None`, `null` → `Some(None)`, number →
/// `Some(Some(i32))`.
pub(crate) fn de_double_opt_i32<'de, D>(d: D) -> Result<Option<Option<i32>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(d).map_err(serde::de::Error::custom)?;
    match v {
        Value::Null => Ok(Some(None)),
        Value::Number(n) => n
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .map(|v| Some(Some(v)))
            .ok_or_else(|| serde::de::Error::custom("invalid integer")),
        other => Err(serde::de::Error::custom(format!("invalid integer: {other}"))),
    }
}

/// Pick the effective state of a new issue, mirroring
/// `Issue._ensure_default_state` (`plane/db/models/issue.py:228-236`)
/// (moved from `issue_write.rs`, now shared with PATCH). Stays `pub`:
/// `tests/issue_test.rs` (separate crate) imports it.
pub fn resolve_effective_state(
    explicit: Option<Uuid>,
    default_id: Option<Uuid>,
    first_id: Option<Uuid>,
) -> Option<Uuid> {
    explicit.or(default_id).or(first_id)
}

/// DB lookup behind [`resolve_effective_state`] (moved from `issue_write.rs`).
pub(crate) async fn resolve_issue_state(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    explicit: Option<Uuid>,
) -> Result<Option<Uuid>, sqlx::Error> {
    if explicit.is_some() {
        return Ok(explicit);
    }
    let default_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    if default_id.is_some() {
        return Ok(default_id);
    }
    let first_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(resolve_effective_state(explicit, default_id, first_id))
}
```

Then in `crates/api/src/routes/issue_write.rs`: delete the local `PRIORITIES`, `bad`, `dedupe_ids`, `de_opt_uuid_lax`, `de_opt_uuid_vec_lax`, `resolve_effective_state`, `resolve_issue_state` definitions, and update the `use super::issue_common::{...}` list to import `bad, dedupe_ids, de_opt_uuid_lax, de_opt_uuid_vec_lax, parse_date, resolve_issue_state, resolve_effective_state, PRIORITIES` (keep `apply_create_bridges, fetch_project_member_role, is_workspace_admin, project_gate_allows, require_project_write, IssueOut`).

Then update `crates/api/tests/issue_test.rs:2` from `use api::routes::issue_write::{resolve_effective_state, validate_create, CreateIssue};` to:

```rust
use api::routes::issue_common::resolve_effective_state;
use api::routes::issue_write::{validate_create, CreateIssue};
```

- [ ] **Step 2: Write the failing unit tests**

Create `crates/api/tests/issue_patch_test.rs` by copying `crates/api/tests/issue_create_test.rs` lines 1-292 (imports, `pool()`/`state()`, `Scratch`, `insert_user`, `insert_workspace_member`, `insert_project_member`, `Scratch::new/add_actor/cleanup`), lines 343-359 (`insert_label`) and lines 380-409 (`insert_estimate_with_point`), then apply these edits:

1. Replace the module doc comment and imports:

```rust
//! Regression tests for the legacy issue PATCH handler
//! (`issue_update::patch_issue`): full web payload persistence, Django
//! serializer validation, bridges, update activities and description
//! versions.

use api::middleware::auth::AuthUser;
use api::routes::issue_update::{patch_issue, PatchIssue};
use api::routes::issue_write::{create, CreateIssue};
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
```

2. In `Scratch::new`, rename the slug prefix to `itpat-` (`let slug = format!("itpat-{}", ...)`) and the identifier prefix stays `ITSQ`.
3. Append to `Scratch::cleanup`, right after the `issue_subscribers` delete:

```rust
        sqlx::query("DELETE FROM issue_description_versions WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
```

4. Copy `purge` from `issue_create_test.rs:410-461`, changing `'itseq-%'` → `'itpat-%'` in both places and adding `"DELETE FROM issue_description_versions WHERE project_id = $1",` to the per-project statement list.
5. Append these helpers:

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

async fn create_issue(st: &AppState, scratch: &Scratch, name: &str) -> Uuid {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(base_body(name, scratch.state_id)),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().expect("id").parse().expect("uuid")
}

fn patch(body: Value) -> PatchIssue {
    serde_json::from_value(body).expect("patch body must deserialize")
}

async fn patch_issue_req(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    issue_id: Uuid,
    body: PatchIssue,
) -> (StatusCode, Value) {
    let (status, Json(body)) = patch_issue(
        State(st.clone()),
        AuthUser(actor),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(body),
    )
    .await
    .expect("PATCH must return a response");
    (status, body)
}

```

Now add the Task-1 tests at the bottom of the file:

```rust
#[tokio::test]
async fn patch_403_for_non_member_and_outsider() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "authz").await;
    let outsider = scratch.add_actor(&pool, None, None).await;
    let ws_only = scratch.add_actor(&pool, Some(15), None).await;

    let (status, _) = patch_issue_req(&st, &scratch, outsider, issue_id, patch(json!({"name": "x"}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = patch_issue_req(&st, &scratch, ws_only, issue_id, patch(json!({"name": "x"}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_404_miss_body_is_verbatim() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let (status, body) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        Uuid::new_v4(),
        patch(json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"error": "Issue not found"}));

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_400_validation_messages() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "validation").await;

    let label = insert_label(&pool, scratch.project_id, scratch.workspace_id, "l1").await;
    let estimate_point = insert_estimate_with_point(&pool, scratch.project_id, scratch.workspace_id).await;
    let other_issue_id = create_issue(&st, &scratch, "other").await;

    for (body, expected) in [
        (json!({"name": ""}), "name must not be blank"),
        (json!({"name": null}), "name may not be null"),
        (json!({"name": "a".repeat(256)}), "name max length 255"),
        (json!({"priority": "nope"}), "Invalid priority"),
        (json!({"priority": null}), "priority may not be null"),
        (json!({"start_date": "not-a-date"}), "Invalid date: not-a-date"),
        (
            json!({"start_date": "2026-09-30", "target_date": "2026-09-01"}),
            "Start date cannot exceed target date",
        ),
        (json!({"description_html": null}), "description_html may not be null"),
        (json!({"description": null}), "description may not be null"),
        (json!({"sort_order": null}), "sort_order may not be null"),
        (json!({"point": 13}), "point must be between 0 and 12"),
        (json!({"assignee_ids": null}), "assignee_ids may not be null"),
        (json!({"label_ids": null}), "label_ids may not be null"),
        (json!({"assignee_ids": [Uuid::new_v4()]}), "invalid assignee: not a project member"),
        (json!({"label_ids": [Uuid::new_v4()]}), "invalid label: not in project"),
        (json!({"state_id": Uuid::new_v4()}), "State is not valid please pass a valid state_id"),
        (json!({"parent_id": Uuid::new_v4()}), "parent is not valid"),
        (json!({"estimate_point": Uuid::new_v4()}), "estimate_point is not valid"),
        (json!({"type_id": Uuid::new_v4()}), "type_id is not valid"),
    ] {
        let (status, body) = patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
        assert_eq!(body["error"], json!(expected), "body: {body}");
    }

    // A start_date without target_date does NOT cross-check (Django compares
    // only when both keys are in `attrs`).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"start_date": "2026-09-30"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Valid refs pass validation (validation-only slice: writes come next).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "state_id": scratch.state_id,
            "label_ids": [label],
            "estimate_point": estimate_point,
            "parent_id": other_issue_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&pool).await;
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
```

Expected: compile error — `unresolved import api::routes::issue_update` (the module does not exist yet).

- [ ] **Step 4: Create `crates/api/src/routes/issue_update.rs`**

```rust
//! Legacy issue PATCH (`PATCH /api/workspaces/:slug/projects/:project_id/issues/:pk/`)
//! — full parity with Django `IssueViewSet.partial_update`
//! (`plane/app/views/issue/base.py:627-713`).
//!
//! Wire contract: ADMIN/MEMBER (or creator) gate → miss 404
//! `{"error": "Issue not found"}` verbatim → serializer validation (400) →
//! one transaction writing the issue row, bridges, activities and the
//! description version → 204 empty.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::issue_common::{
    bad, de_double_opt_f64, de_double_opt_i32, de_double_opt_json, de_double_opt_string,
    de_double_opt_uuid_lax, de_double_opt_uuid_vec_lax, dedupe_ids, fetch_project_member_role,
    is_workspace_admin, parse_date, project_gate_allows, PRIORITIES,
};
use super::work_item::ws_active_member;
use crate::routes::project::deny;
use crate::{middleware::auth::AuthUser, state::AppState};

/// Quoted from `plane/app/views/issue/base.py:659-661`
/// (`IssueViewSet.partial_update`): miss → 404 with this body verbatim.
pub(crate) const ISSUE_PATCH_MISS_MSG: &str = "Issue not found";

/// `PATCH /issues/:pk/` body. Tri-state fields: absent → `None`,
/// explicit `null` → `Some(None)`, value → `Some(Some(_))` — matching
/// Django's `partial=True` + model null flags (see plan table).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PatchIssue {
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub description_html: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_json")]
    pub description: Option<Option<Value>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub priority: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub state_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub parent_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub start_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub target_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_f64")]
    pub sort_order: Option<Option<f64>>,
    #[serde(default, deserialize_with = "de_double_opt_i32")]
    pub point: Option<Option<i32>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub estimate_point: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub type_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_vec_lax")]
    pub assignee_ids: Option<Option<Vec<Uuid>>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_vec_lax")]
    pub label_ids: Option<Option<Vec<Uuid>>>,
    #[serde(default)]
    pub skip_activity: Option<Value>,
}

/// Tri-state date: absent/null → `None`, `""` → `None` (lax, create
/// precedent), valid `%Y-%m-%d` → `Some(date)`.
fn parse_tri_date(raw: &Option<Option<String>>) -> Result<Option<chrono::NaiveDate>, String> {
    match raw {
        None | Some(None) => Ok(None),
        Some(Some(s)) => parse_date(&Some(s.clone())),
    }
}

fn has_value(v: &Option<Option<String>>) -> bool {
    matches!(v, Some(Some(_)))
}

/// Pure validation, Django order (`serializers/issue.py:127-196`).
pub fn validate_patch(body: &PatchIssue) -> Result<(), String> {
    let start = parse_tri_date(&body.start_date)?;
    let target = parse_tri_date(&body.target_date)?;
    // Django compares only when BOTH keys are in `attrs` (`serializers/issue.py:129-134`).
    if has_value(&body.start_date) && has_value(&body.target_date) {
        if let (Some(s), Some(t)) = (start, target) {
            if s > t {
                return Err("Start date cannot exceed target date".to_string());
            }
        }
    }
    if let Some(v) = &body.name {
        let Some(name) = v else {
            return Err("name may not be null".to_string());
        };
        if name.trim().is_empty() {
            return Err("name must not be blank".to_string());
        }
        if name.chars().count() > 255 {
            return Err("name max length 255".to_string());
        }
    }
    if let Some(v) = &body.description_html {
        if v.is_none() {
            return Err("description_html may not be null".to_string());
        }
    }
    if let Some(v) = &body.description {
        if v.is_none() {
            return Err("description may not be null".to_string());
        }
    }
    if let Some(v) = &body.priority {
        let Some(p) = v else {
            return Err("priority may not be null".to_string());
        };
        if !PRIORITIES.contains(&p.as_str()) {
            return Err("Invalid priority".to_string());
        }
    }
    if matches!(body.sort_order, Some(None)) {
        return Err("sort_order may not be null".to_string());
    }
    if let Some(Some(p)) = body.point {
        if !(0..=12).contains(&p) {
            return Err("point must be between 0 and 12".to_string());
        }
    }
    if matches!(body.assignee_ids, Some(None)) {
        return Err("assignee_ids may not be null".to_string());
    }
    if matches!(body.label_ids, Some(None)) {
        return Err("label_ids may not be null".to_string());
    }
    Ok(())
}

fn internal(_e: sqlx::Error) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "Something went wrong please try again later"})),
    )
}

/// DB-backed reference validation; every failure is 400 (strict, #9526).
async fn validate_patch_refs(
    st: &AppState,
    project_id: Uuid,
    body: &PatchIssue,
    assignees: &[Uuid],
    labels: &[Uuid],
) -> Result<(), (StatusCode, Json<Value>)> {
    if !assignees.is_empty() {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) \
             AND is_active = true AND role >= 15 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(assignees)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
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
        .map_err(internal)?;
        if n != labels.len() as i64 {
            return Err(bad("invalid label: not in project"));
        }
    }
    if let Some(Some(state_id)) = body.state_id {
        // Django validates against `State.objects` = `StateManager`
        // (`db/models/state.py:65-69`, a `SoftDeletionManager` that also
        // excludes `group='triage'`): soft-deleted and triage states 400.
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
             AND deleted_at IS NULL AND \"group\" != 'triage')",
        )
        .bind(state_id)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("State is not valid please pass a valid state_id"));
        }
    }
    if let Some(Some(parent)) = body.parent_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(parent)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("parent is not valid"));
        }
    }
    if let Some(Some(ep)) = body.estimate_point {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM estimate_points WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(ep)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("estimate_point is not valid"));
        }
    }
    if let Some(Some(t)) = body.type_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(t)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("type_id is not valid"));
        }
    }
    Ok(())
}

pub async fn patch_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `partial_update` (`base.py:627`): `@allow_permission([ADMIN,
    // MEMBER], creator=True, model=Issue)` runs BEFORE the body — gate
    // first (a denied miss is 403, not 404), then the fetch.
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    // Existence uses `get_queryset()` = `issue_objects` (`base.py:628-629`):
    // drafts, archived issues, triage-state issues and archived-project
    // issues all miss with 404 `{"error": "Issue not found"}` verbatim.
    let row: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT i.created_by_id FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false AND (s.id IS NULL OR s.\"group\" != 'triage') AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $2 AND p.archived_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if row.is_none() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": ISSUE_PATCH_MISS_MSG})),
        ));
    }
    if let Err(msg) = validate_patch(&body) {
        return Ok(bad(&msg));
    }
    let assignees = dedupe_ids(&body.assignee_ids.clone().flatten());
    let labels = dedupe_ids(&body.label_ids.clone().flatten());
    if let Err(e) = validate_patch_refs(&st, project_id, &body, &assignees, &labels).await {
        return Ok(e);
    }
    // TEMP (Task 2 replaces this with the full dynamic UPDATE + side effects):
    sqlx::query(
        "UPDATE issues SET name = COALESCE($1, name), description_html = COALESCE($2, description_html), description_json = COALESCE($3::jsonb, description_json), priority = COALESCE($4, priority), updated_at = now() WHERE id = $5 AND project_id = $6 AND deleted_at IS NULL",
    )
    .bind(body.name.clone().flatten())
    .bind(body.description_html.clone().flatten())
    .bind(body.description.clone().flatten())
    .bind(body.priority.clone().flatten())
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

Add to `crates/api/src/routes/mod.rs` (alphabetical block near `pub mod issue_sub;`):

```rust
pub mod issue_update;
pub mod issue_version_write;
```

(`issue_version_write` lands in Task 5; add only `issue_update` in this task.)

Update both `crates/api/src/main.rs:1197` and `:1274` to `.patch(routes::issue_update::patch_issue)`.

Delete from `crates/api/src/routes/work_item.rs`: the `PatchIssue` struct, `validate_issue_patch`, the `ISSUE_PATCH_MISS_MSG` const, the whole `patch_issue` function, and the `patch_miss_string_is_issue_not_found_not_missing` unit test.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
```

Expected: `4 passed` (3 validation/authz tests + the gate-order test). Then confirm the create suite still compiles/passes:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: all pass.

- [ ] **Step 6: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_update.rs crates/api/src/routes/issue_common.rs \
  crates/api/src/routes/issue_write.rs crates/api/src/routes/work_item.rs crates/api/tests/issue_patch_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/src/routes/issue_common.rs \
  apps/api-rs/crates/api/src/routes/issue_write.rs apps/api-rs/crates/api/src/routes/work_item.rs \
  apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/issue_patch_test.rs
git commit -m "refactor(api-rs): extract legacy issue PATCH with full request validation"
```

---

### Task 2: Scalar writes, `description_stripped`, `completed_at`, `updated_by`

**Files:**

- Modify: `crates/api/src/routes/issue_update.rs`
- Modify: `crates/api/src/routes/issue_common.rs` (no change expected — `resolve_issue_state` already moved)
- Test: `crates/api/tests/issue_patch_test.rs`

- [ ] **Step 1: Write the failing tests**

Append these helpers, then the tests, to `crates/api/tests/issue_patch_test.rs`:

```rust
async fn insert_state(
    pool: &PgPool,
    project_id: Uuid,
    workspace_id: Uuid,
    name: &str,
    group: &str,
    is_default: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, $2, '', '#60646C', $3, $4, $5, 65535, $6, $7, false, now(), now())",
    )
    .bind(id)
    .bind(name)
    .bind(name.to_lowercase())
    .bind(project_id)
    .bind(workspace_id)
    .bind(group)
    .bind(is_default)
    .execute(pool)
    .await
    .expect("scratch state");
    id
}

#[derive(Debug, sqlx::FromRow)]
struct IssueRow {
    name: String,
    description_html: String,
    description_stripped: Option<String>,
    description_json: Value,
    priority: String,
    state_id: Option<Uuid>,
    parent_id: Option<Uuid>,
    start_date: Option<chrono::NaiveDate>,
    target_date: Option<chrono::NaiveDate>,
    sort_order: f64,
    point: Option<i32>,
    estimate_point_id: Option<Uuid>,
    type_id: Option<Uuid>,
    updated_by_id: Option<Uuid>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn issue_row(pool: &PgPool, issue_id: Uuid) -> IssueRow {
    sqlx::query_as(
        "SELECT name, description_html, description_stripped, description_json, priority, state_id, \
         parent_id, start_date, target_date, sort_order, point, estimate_point_id, type_id, \
         updated_by_id, completed_at FROM issues WHERE id = $1",
    )
    .bind(issue_id)
    .fetch_one(pool)
    .await
    .expect("issue row")
}

```

```rust
#[tokio::test]
async fn patch_persists_scalars_and_recomputes_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "scalars").await;
    let label = insert_label(&pool, scratch.project_id, scratch.workspace_id, "l1").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "name": "renamed",
            "description_html": "<p>hello <b>world</b></p>",
            "description": {"type": "doc"},
            "priority": "high",
            "start_date": "2026-09-01",
            "target_date": "2026-09-30",
            "sort_order": 1234.5,
            "point": 3,
            "parent_id": null,
            "label_ids": [label],
            "project_id": Uuid::new_v4(),
            "id": Uuid::new_v4(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.name, "renamed");
    assert_eq!(row.description_html, "<p>hello <b>world</b></p>");
    assert_eq!(row.description_stripped.as_deref(), Some("hello world"));
    assert_eq!(row.description_json, json!({"type": "doc"}));
    assert_eq!(row.priority, "high");
    assert_eq!(row.start_date.map(|d| d.to_string()), Some("2026-09-01".to_string()));
    assert_eq!(row.target_date.map(|d| d.to_string()), Some("2026-09-30".to_string()));
    assert_eq!(row.sort_order, 1234.5);
    assert_eq!(row.point, Some(3));
    assert_eq!(row.updated_by_id, Some(scratch.user_id));

    // Explicit nulls clear nullable fields; empty html stores NULL stripped.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "start_date": null,
            "target_date": null,
            "point": null,
            "description_html": "",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.start_date, None);
    assert_eq!(row.target_date, None);
    assert_eq!(row.point, None);
    assert_eq!(row.description_html, "");
    assert_eq!(row.description_stripped, None);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_state_writes_completed_at_and_default_fallback() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "state").await;

    let done = insert_state(&pool, scratch.project_id, scratch.workspace_id, "Done", "completed", false).await;
    let started = insert_state(&pool, scratch.project_id, scratch.workspace_id, "Doing", "started", false).await;

    let (status, _) =
        patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(json!({"state_id": done}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(done));
    assert!(row.completed_at.is_some());

    let (status, _) =
        patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(json!({"state_id": started}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(started));
    assert_eq!(row.completed_at, None);

    // `state_id: null` falls back to the default state (Django
    // `_ensure_default_state`); the fixture default is `scratch.state_id`.
    let (status, _) =
        patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(json!({"state_id": null}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(scratch.state_id));
    assert_eq!(row.completed_at, None);

    // An issue whose state is NULL gets the default on ANY update
    // (`Issue.save` runs `_ensure_default_state` every save).
    sqlx::query("UPDATE issues SET state_id = NULL WHERE id = $1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) =
        patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(json!({"priority": "low"}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(scratch.state_id));

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_type_and_estimate_persist() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "refs").await;
    let estimate_point = insert_estimate_with_point(&pool, scratch.project_id, scratch.workspace_id).await;
    let issue_type = insert_issue_type(&pool, scratch.workspace_id, "Bug").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": estimate_point, "type_id": issue_type})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.estimate_point_id, Some(estimate_point));
    assert_eq!(row.type_id, Some(issue_type));

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": null, "type_id": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.estimate_point_id, None);
    assert_eq!(row.type_id, None);

    scratch.cleanup(&pool).await;
}
```

`insert_issue_type` does not exist in the patch test file yet — add it now, copying `issue_create_test.rs:327-342` (adjusting the closing brace to the copied range).

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
```

Expected: `patch_persists_scalars_and_recomputes_stripped` fails on `description_stripped`/dates/sort_order/point (the TEMP UPDATE writes only 4 columns).

- [ ] **Step 3: Implement the dynamic UPDATE**

In `crates/api/src/routes/issue_update.rs`, replace the TEMP block and add above `patch_issue`:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CurrentIssue {
    pub name: String,
    pub description_html: String,
    pub description_json: Value,
    pub priority: String,
    pub state_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sort_order: f64,
    pub point: Option<i32>,
    pub estimate_point_id: Option<Uuid>,
    pub created_by_id: Option<Uuid>,
}

enum BindValue {
    Text(Option<String>),
    Json(Value),
    Date(Option<chrono::NaiveDate>),
    Uuid(Option<Uuid>),
    Int(Option<i32>),
    Float(f64),
}

fn add(sets: &mut Vec<String>, values: &mut Vec<BindValue>, col: &str, v: BindValue) {
    values.push(v);
    sets.push(format!("{col} = ${}", values.len()));
}
```

Replace the existence query in `patch_issue` with the full snapshot fetch (same WHERE, so the 404 semantics do not change):

```rust
    let current: Option<CurrentIssue> = sqlx::query_as(
        "SELECT i.name, i.description_html, i.description_json, i.priority, i.state_id, i.parent_id, \
         i.start_date, i.target_date, i.sort_order, i.point, i.estimate_point_id, i.created_by_id \
         FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND (s.id IS NULL OR s.\"group\" != 'triage') \
         AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $2 AND p.archived_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": ISSUE_PATCH_MISS_MSG})),
        ));
    };
```

Then after the refs validation replace the TEMP UPDATE with:

```rust
    // Django sanitizes `description_html` in `IssueCreateSerializer.validate`
    // (`serializers/issue.py:135-143`); failures are 400
    // `{"error": "html content is not valid"}`.
    let sanitized_html: Option<String> = match &body.description_html {
        Some(Some(h)) => match super::page::clean_description_html(h) {
            Ok(v) => Some(v),
            Err(_) => return Ok(bad("html content is not valid")),
        },
        _ => None,
    };
    let start_date = match parse_tri_date(&body.start_date) {
        Ok(v) => v,
        Err(e) => return Ok(bad(&e)),
    };
    let target_date = match parse_tri_date(&body.target_date) {
        Ok(v) => v,
        Err(e) => return Ok(bad(&e)),
    };
    // `Issue._ensure_default_state` (`db/models/issue.py:180-236`).
    let new_state_id = match body.state_id {
        Some(Some(id)) => Some(id),
        Some(None) => resolve_issue_state(&st.pool, project_id, None).await?,
        None => match current.state_id {
            Some(id) => Some(id),
            None => resolve_issue_state(&st.pool, project_id, None).await?,
        },
    };
    let state_changed = new_state_id != current.state_id;
    let new_state_group: Option<String> = if state_changed {
        match new_state_id {
            Some(id) => {
                sqlx::query_scalar("SELECT \"group\" FROM states WHERE id = $1 AND project_id = $2")
                    .bind(id)
                    .bind(project_id)
                    .fetch_optional(&st.pool)
                    .await?
            }
            None => None,
        }
    } else {
        None
    };

    let mut tx = st.pool.begin().await?;
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<BindValue> = Vec::new();
    // `BaseModel.save` on update (`db/models/base.py:43-46`).
    add(&mut sets, &mut values, "updated_by_id", BindValue::Uuid(Some(auth.0)));
    sets.push("updated_at = now()".to_string());
    // `Issue.save` recomputes `description_stripped` on every update
    // (`db/models/issue.py:212-217`).
    let effective_html = sanitized_html.as_deref().unwrap_or(&current.description_html);
    let stripped = if effective_html.is_empty() {
        None
    } else {
        Some(super::page::strip_tags_text(effective_html))
    };
    add(&mut sets, &mut values, "description_stripped", BindValue::Text(stripped));
    if let Some(Some(v)) = body.name.clone() {
        add(&mut sets, &mut values, "name", BindValue::Text(Some(v)));
    }
    if let Some(v) = sanitized_html.clone() {
        add(&mut sets, &mut values, "description_html", BindValue::Text(Some(v)));
    }
    if let Some(Some(v)) = body.description.clone() {
        add(&mut sets, &mut values, "description_json", BindValue::Json(v));
    }
    if let Some(Some(v)) = body.priority.clone() {
        add(&mut sets, &mut values, "priority", BindValue::Text(Some(v)));
    }
    if body.start_date.is_some() {
        add(&mut sets, &mut values, "start_date", BindValue::Date(start_date));
    }
    if body.target_date.is_some() {
        add(&mut sets, &mut values, "target_date", BindValue::Date(target_date));
    }
    if let Some(Some(v)) = body.sort_order {
        add(&mut sets, &mut values, "sort_order", BindValue::Float(v));
    }
    if body.point.is_some() {
        add(&mut sets, &mut values, "point", BindValue::Int(body.point.flatten()));
    }
    if body.parent_id.is_some() {
        add(&mut sets, &mut values, "parent_id", BindValue::Uuid(body.parent_id.flatten()));
    }
    if body.estimate_point.is_some() {
        add(
            &mut sets,
            &mut values,
            "estimate_point_id",
            BindValue::Uuid(body.estimate_point.flatten()),
        );
    }
    if body.type_id.is_some() {
        add(&mut sets, &mut values, "type_id", BindValue::Uuid(body.type_id.flatten()));
    }
    if state_changed {
        add(&mut sets, &mut values, "state_id", BindValue::Uuid(new_state_id));
        if new_state_id.is_some() {
            // `_sync_completed_at` (`db/models/issue.py:240-256`).
            if new_state_group.as_deref() == Some("completed") {
                sets.push("completed_at = now()".to_string());
            } else {
                sets.push("completed_at = NULL".to_string());
            }
        }
    }

    let pk_pos = values.len() + 1;
    let project_pos = values.len() + 2;
    let sql = format!(
        "UPDATE issues SET {} WHERE id = ${pk_pos} AND project_id = ${project_pos} AND deleted_at IS NULL",
        sets.join(", ")
    );
    let mut q = sqlx::query(&sql);
    for v in &values {
        q = match v {
            BindValue::Text(s) => q.bind(s.clone()),
            BindValue::Json(j) => q.bind(j.clone()),
            BindValue::Date(d) => q.bind(*d),
            BindValue::Uuid(u) => q.bind(*u),
            BindValue::Int(n) => q.bind(*n),
            BindValue::Float(f) => q.bind(*f),
        };
    }
    q.bind(pk).bind(project_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
```

Add `resolve_issue_state` to the `use super::issue_common::{...}` import list.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
```

Expected: `11 passed`.

- [ ] **Step 5: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_update.rs crates/api/tests/issue_patch_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/tests/issue_patch_test.rs
git commit -m "feat(api-rs): legacy issue PATCH persists every scalar field with Django save side effects"
```

---

### Task 3: Assignee/label bridges

**Files:**

- Modify: `crates/api/src/routes/issue_update.rs`
- Test: `crates/api/tests/issue_patch_test.rs`

- [ ] **Step 1: Write the failing tests**

Append `live_labels` below and `live_assignees` (copy `issue_create_test.rs:369-379`), then the test:

```rust
async fn live_labels(pool: &PgPool, issue_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("label rows")
}

```

```rust
#[tokio::test]
async fn patch_replaces_assignee_and_label_bridges() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "bridges").await;
    let member = scratch.add_actor(&pool, Some(15), Some(15)).await;
    let label_a = insert_label(&pool, scratch.project_id, scratch.workspace_id, "a").await;
    let label_b = insert_label(&pool, scratch.project_id, scratch.workspace_id, "b").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [member, member], "label_ids": [label_a]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(live_assignees(&pool, issue_id).await, vec![member]);
    assert_eq!(live_labels(&pool, issue_id).await, vec![label_a]);

    // Replace: one live row, the previous row is soft-deleted.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [scratch.user_id], "label_ids": [label_b]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(live_assignees(&pool, issue_id).await, vec![scratch.user_id]);
    assert_eq!(live_labels(&pool, issue_id).await, vec![label_b]);
    let soft: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_assignees WHERE issue_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(soft, 1, "replaced bridge rows are soft-deleted");

    // `[]` clears.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [], "label_ids": []})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(live_assignees(&pool, issue_id).await.is_empty());
    assert!(live_labels(&pool, issue_id).await.is_empty());

    // Absent keys leave bridges untouched.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [member]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) =
        patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(json!({"name": "no bridge touch"}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(live_assignees(&pool, issue_id).await, vec![member]);

    scratch.cleanup(&pool).await;
}
```

- [ ] **Step 2: Run to verify failure**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test patch_replaces -- --test-threads=1
```

Expected: FAIL — `live_assignees` is empty.

- [ ] **Step 3: Implement**

In `crates/api/src/routes/issue_update.rs`, inside `patch_issue` after the UPDATE execution (before `tx.commit()`):

```rust
    // Django `IssueCreateSerializer.update` (`serializers/issue.py:276-320`):
    // present keys replace the whole bridge set.
    if matches!(body.assignee_ids, Some(Some(_))) {
        replace_bridges(&mut tx, pk, project_id, auth.0, Some(&assignees), None).await?;
    }
    if matches!(body.label_ids, Some(Some(_))) {
        replace_bridges(&mut tx, pk, project_id, auth.0, None, Some(&labels)).await?;
    }
```

Add `replace_bridges` to the `issue_common` import list. (Task 4 moves the live-id reads and the activity block **before** this bridge block; final tx order: UPDATE → live ids + activities → bridges → versions → commit.)

- [ ] **Step 4: Run to verify pass**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
```

Expected: `12 passed` (11 from Tasks 1-2 plus this test).

- [ ] **Step 5: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_update.rs crates/api/tests/issue_patch_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/tests/issue_patch_test.rs
git commit -m "feat(api-rs): legacy issue PATCH replaces assignee and label bridges"
```

---

### Task 4: Update activities + `skip_activity`

**Files:**

- Modify: `crates/api/src/routes/issue_activity_write.rs` (add `ActivityCtx`, `insert_activity_row`)
- Modify: `crates/api/src/routes/issue_update.rs` (diff writer + wiring)
- Test: `crates/api/tests/issue_patch_test.rs`

- [ ] **Step 1: Write the failing tests**

Append these helpers, then the tests:

```rust
type ActRow = (String, Option<String>, String, Option<String>, Option<String>, Option<Uuid>, Option<Uuid>);

async fn activities(pool: &PgPool, issue_id: Uuid) -> Vec<ActRow> {
    sqlx::query_as(
        "SELECT verb, field, comment, old_value, new_value, old_identifier, new_identifier \
         FROM issue_activities WHERE issue_id = $1 ORDER BY created_at, field",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("activity rows")
}

async fn activity_fields(pool: &PgPool, issue_id: Uuid) -> Vec<String> {
    activities(pool, issue_id)
        .await
        .into_iter()
        .filter_map(|r| r.1)
        .collect()
}
```

```rust
#[tokio::test]
async fn patch_writes_per_field_activities() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "activities").await;
    let member = scratch.add_actor(&pool, Some(15), Some(15)).await;
    let label = insert_label(&pool, scratch.project_id, scratch.workspace_id, "sev1").await;
    let started = insert_state(&pool, scratch.project_id, scratch.workspace_id, "Doing", "started", false).await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "name": "renamed",
            "description_html": "<p>body</p>",
            "priority": "urgent",
            "state_id": started,
            "start_date": "2026-09-01",
            "target_date": "2026-09-30",
            "assignee_ids": [member],
            "label_ids": [label],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let rows = activities(&pool, issue_id).await;
    for field in ["name", "description", "priority", "state", "start_date", "target_date", "assignees", "labels"] {
        assert!(rows.iter().any(|r| r.1.as_deref() == Some(field)), "missing {field}: {rows:?}");
    }
    let name_row = rows.iter().find(|r| r.1.as_deref() == Some("name")).unwrap();
    assert_eq!(name_row.0, "updated");
    assert_eq!(name_row.2, "updated the name to");
    assert_eq!(name_row.3.as_deref(), Some("activities"));
    assert_eq!(name_row.4.as_deref(), Some("renamed"));
    let state_row = rows.iter().find(|r| r.1.as_deref() == Some("state")).unwrap();
    assert_eq!(state_row.3.as_deref(), Some("Backlog"));
    assert_eq!(state_row.4.as_deref(), Some("Doing"));
    assert_eq!(state_row.5, Some(scratch.state_id));
    assert_eq!(state_row.6, Some(started));
    let date_row = rows.iter().find(|r| r.1.as_deref() == Some("start_date")).unwrap();
    assert_eq!(date_row.2, "updated the start date to ");
    assert_eq!(date_row.4.as_deref(), Some("2026-09-01"));
    let assignee_row = rows.iter().find(|r| r.1.as_deref() == Some("assignees")).unwrap();
    assert_eq!(assignee_row.2, "added assignee ");
    assert_eq!(assignee_row.6, Some(member));
    let subscriber: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_subscribers WHERE issue_id = $1 AND subscriber_id = $2 AND deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(member)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(subscriber, 1, "added assignees are subscribed");

    // Removals + clearing write the matching rows.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [], "label_ids": [], "start_date": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let removed_assignee = rows.iter().find(|r| r.2 == "removed assignee ").unwrap();
    assert!(removed_assignee.3.is_some(), "old_value is the display name");
    assert_eq!(removed_assignee.4.as_deref(), Some(""));
    let removed_label = rows.iter().find(|r| r.2 == "removed label ").unwrap();
    assert_eq!(removed_label.3.as_deref(), Some("sev1"));
    assert_eq!(removed_label.4.as_deref(), Some(""));
    let start_date_rows = rows.iter().filter(|r| r.1.as_deref() == Some("start_date")).count();
    assert_eq!(start_date_rows, 2, "set + cleared");

    // Description merge: two consecutive description-only patches by the same
    // actor keep one row (Django's `track_description` bumps `created_at`).
    let before = activities(&pool, issue_id).await.len();
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>body 2</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(activities(&pool, issue_id).await.len(), before + 1);
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>body 3</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        activities(&pool, issue_id).await.len(),
        before + 1,
        "same actor merges into the previous description row"
    );

    // Different actor → new row.
    let (status, _) = patch_issue_req(&st, &scratch, member, issue_id, patch(json!({"description_html": "<p>body 4</p>"}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(activities(&pool, issue_id).await.len(), before + 2);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_skip_activity_suppresses_activities_and_versions() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "skip").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>migrated</p>", "skip_activity": "true"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(activity_fields(&pool, issue_id).await.is_empty());
    assert_eq!(issue_row(&pool, issue_id).await.description_html, "<p>migrated</p>");

    // `skip_activity` without `description_html` is ignored (Django
    // `is_description_update` gate).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"name": "still logged", "skip_activity": "true"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(activity_fields(&pool, issue_id).await, vec!["name".to_string()]);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_estimate_activity_field_uses_estimate_type() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "estimate").await;
    let estimate_point = insert_estimate_with_point(&pool, scratch.project_id, scratch.workspace_id).await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": estimate_point})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("estimate_points"))
        .expect("estimate_points activity");
    assert_eq!(row.4.as_deref(), Some("1"));
    assert_eq!(row.6, Some(estimate_point));

    // Clearing the estimate writes no estimate row (Django's task NPEs and
    // loses the whole batch; documented deviation 6) but other fields still log.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": null, "priority": "low"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let estimate_rows = rows
        .iter()
        .filter(|r| r.1.as_deref() == Some("estimate_points"))
        .count();
    assert_eq!(estimate_rows, 1, "clearing writes no estimate row");
    assert!(rows.iter().any(|r| r.1.as_deref() == Some("priority") && r.4.as_deref() == Some("low")));

    scratch.cleanup(&pool).await;
}
```

- [ ] **Step 2: Run to verify failure**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test patch_writes_per_field -- --test-threads=1
```

Expected: FAIL — no activity rows.

- [ ] **Step 3: Add the generic writer to `issue_activity_write.rs`**

```rust
/// Context shared by the update-path activity rows.
pub(crate) struct ActivityCtx {
    pub issue_id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub actor: Uuid,
    pub epoch: f64,
}

/// One `verb`/`field` activity row for the update flow. `created_by_id` and
/// `updated_by_id` stay NULL (Django `bulk_create` skips `save()`);
/// `attachments` is `'{}'` (ArrayField default). Timestamps use
/// `clock_timestamp()` (per-statement) instead of `now()` (transaction
/// start) so batch rows get distinct, insertion-ordered `created_at` like
/// Django's per-instance defaults — `merge_last_description_activity` and
/// the activity feed depend on that ordering.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_activity_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &ActivityCtx,
    verb: &str,
    field: &str,
    comment: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
    old_identifier: Option<Uuid>,
    new_identifier: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_activities (id, verb, field, old_value, new_value, comment, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, updated_by_id, \
         old_identifier, new_identifier, epoch, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, '{}'::varchar[], $6, $7, $8, $9, NULL, NULL, \
         $10, $11, $12, clock_timestamp(), clock_timestamp())",
    )
    .bind(verb)
    .bind(field)
    .bind(old_value)
    .bind(new_value)
    .bind(comment)
    .bind(ctx.issue_id)
    .bind(ctx.project_id)
    .bind(ctx.workspace_id)
    .bind(ctx.actor)
    .bind(old_identifier)
    .bind(new_identifier)
    .bind(ctx.epoch)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 4: Add the diff writer to `issue_update.rs`**

Add to the module imports: `use super::issue_activity_write::{insert_activity_row, insert_assignee_activities, insert_subscribers, ActivityCtx};` (and drop the fully-qualified call form below accordingly). While there, switch `insert_assignee_activities`' `created_at`/`updated_at` from `now()` (transaction start) to `clock_timestamp()` so rows reused from the create writer keep the update batch's insertion ordering; the create path's observable behavior is unchanged.

```rust
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

async fn parent_label(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Option<Uuid>,
) -> Result<String, sqlx::Error> {
    let Some(id) = id else { return Ok(String::new()) };
    let label: Option<String> = sqlx::query_scalar(
        "SELECT p.identifier || '-' || i.sequence_id FROM issues i \
         JOIN projects p ON p.id = i.project_id WHERE i.id = $1 AND i.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(label.unwrap_or_default())
}

/// `(id, name)` when the state exists under Django's `State.objects`
/// (`StateManager`: soft-deleted + triage excluded); identifiers are only
/// written when the row exists (`issue_activities_task.py:205-236`).
async fn state_info(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Option<Uuid>,
    project_id: Uuid,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let Some(id) = id else { return Ok(None) };
    sqlx::query_as(
        "SELECT id, name FROM states WHERE id = $1 AND project_id = $2 \
         AND deleted_at IS NULL AND \"group\" != 'triage'",
    )
    .bind(id)
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await
}

async fn live_label_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL")
        .bind(issue_id)
        .fetch_all(&mut **tx)
        .await
}

/// Mirrors the `IssueDetailSerializer` assignee annotation
/// (`base.py:633-648`): live bridge rows whose member still has an active
/// project membership.
async fn live_assignee_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    // Exact `IssueDetailSerializer` annotation parity (`base.py:645-656`):
    // `assignees__member_project__is_active=True` is NOT project-scoped in
    // Django (a member active in ANY project qualifies), and joins use the
    // base manager (no `deleted_at` predicate) — `DISTINCT` mirrors
    // `ArrayAgg(distinct=True)`.
    sqlx::query_scalar(
        "SELECT DISTINCT ia.assignee_id FROM issue_assignees ia \
         JOIN project_members pm ON pm.member_id = ia.assignee_id AND pm.is_active = true \
         WHERE ia.issue_id = $1 AND ia.deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_all(&mut **tx)
    .await
}

async fn names_for(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    column: &str,
    ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let sql = format!("SELECT id, {column} FROM {table} WHERE id = ANY($1)");
    let rows: Vec<(Uuid, String)> = sqlx::query_as(&sql).bind(ids).fetch_all(&mut **tx).await?;
    Ok(rows.into_iter().collect())
}

/// `(value, estimates.type)` for an estimate point id.
async fn estimate_info(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ep.value, e.type FROM estimate_points ep \
         JOIN estimates e ON e.id = ep.estimate_id WHERE ep.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

/// Django's `track_description` merge rule: the issue's latest activity is a
/// `description` row by the same actor → bump its `created_at` instead of
/// inserting. Runs before this request's rows so the DB view matches
/// Django's pre-`bulk_create` lookup.
async fn merge_last_description_activity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    actor: Uuid,
) -> Result<bool, sqlx::Error> {
    let last: Option<(Uuid, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, field, actor_id FROM issue_activities WHERE issue_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((id, Some(field), Some(last_actor))) = last {
        if field == "description" && last_actor == actor {
            sqlx::query("UPDATE issue_activities SET created_at = clock_timestamp() WHERE id = $1")
                .bind(id)
                .execute(&mut **tx)
                .await?;
            return Ok(true);
        }
    }
    Ok(false)
}

async fn write_update_activities(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &ActivityCtx,
    current: &CurrentIssue,
    body: &PatchIssue,
    current_label_ids: &[Uuid],
    current_assignee_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    // description first: Django's merge lookup must see the pre-batch DB.
    if let Some(Some(html)) = &body.description_html {
        if &current.description_html != html {
            let merged = merge_last_description_activity(tx, ctx.issue_id, ctx.actor).await?;
            if !merged {
                insert_activity_row(
                    tx,
                    ctx,
                    "updated",
                    "description",
                    "updated the description to",
                    Some(current.description_html.as_str()),
                    Some(html),
                    None,
                    None,
                )
                .await?;
            }
        }
    }
    if let Some(Some(name)) = &body.name {
        if &current.name != name {
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "name",
                "updated the name to",
                Some(current.name.as_str()),
                Some(name),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = body.parent_id {
        if requested != current.parent_id {
            let old = parent_label(tx, current.parent_id).await?;
            let new = parent_label(tx, requested).await?;
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "parent",
                "updated the parent issue to",
                Some(old.as_str()),
                Some(new.as_str()),
                current.parent_id,
                requested,
            )
            .await?;
        }
    }
    if let Some(Some(priority)) = &body.priority {
        if &current.priority != priority {
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "priority",
                "updated the priority to",
                Some(current.priority.as_str()),
                Some(priority),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = body.state_id {
        if requested != current.state_id {
            let old = state_info(tx, current.state_id, ctx.project_id).await?;
            let new = state_info(tx, requested, ctx.project_id).await?;
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "state",
                "updated the state to",
                old.as_ref().map(|(_, name)| name.as_str()),
                new.as_ref().map(|(_, name)| name.as_str()),
                old.as_ref().map(|(id, _)| *id),
                new.as_ref().map(|(id, _)| *id),
            )
            .await?;
        }
    }
    if let Some(requested) = &body.target_date {
        let new = parse_tri_date(&body.target_date).unwrap_or(None);
        if new != current.target_date {
            let old_value = current.target_date.map(|d| d.to_string()).unwrap_or_default();
            let new_value = requested.clone().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "target_date",
                "updated the target date to",
                Some(old_value.as_str()),
                Some(new_value.as_str()),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = &body.start_date {
        let new = parse_tri_date(&body.start_date).unwrap_or(None);
        if new != current.start_date {
            let old_value = current.start_date.map(|d| d.to_string()).unwrap_or_default();
            let new_value = requested.clone().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "start_date",
                "updated the start date to ",
                Some(old_value.as_str()),
                Some(new_value.as_str()),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(Some(requested)) = &body.label_ids {
        let requested: std::collections::HashSet<Uuid> = requested.iter().copied().collect();
        let current_set: std::collections::HashSet<Uuid> = current_label_ids.iter().copied().collect();
        let added: Vec<Uuid> = requested.difference(&current_set).copied().collect();
        let dropped: Vec<Uuid> = current_set.difference(&requested).copied().collect();
        let names = names_for(tx, "labels", "name", &[added.clone(), dropped.clone()].concat()).await?;
        for id in added {
            let name = names.get(&id).cloned().unwrap_or_default();
            insert_activity_row(
                tx, ctx, "updated", "labels", "added label ", Some(""), Some(name.as_str()), None, Some(id),
            )
            .await?;
        }
        for id in dropped {
            let name = names.get(&id).cloned().unwrap_or_default();
            insert_activity_row(
                tx, ctx, "updated", "labels", "removed label ", Some(name.as_str()), Some(""), Some(id), None,
            )
            .await?;
        }
    }
    if let Some(Some(requested)) = &body.assignee_ids {
        let requested: std::collections::HashSet<Uuid> = requested.iter().copied().collect();
        let current_set: std::collections::HashSet<Uuid> = current_assignee_ids.iter().copied().collect();
        let added: Vec<Uuid> = requested.difference(&current_set).copied().collect();
        let dropped: Vec<Uuid> = current_set.difference(&requested).copied().collect();
        // Reuse the create-path "added assignee" writer (identical row shape:
        // `old_value=''`, `new_value=display_name`, `new_identifier=user`).
        insert_assignee_activities(
            tx,
            ctx.issue_id,
            ctx.project_id,
            ctx.workspace_id,
            ctx.actor,
            &added,
            ctx.epoch,
        )
        .await?;
        insert_subscribers(tx, ctx.issue_id, ctx.project_id, ctx.workspace_id, &added).await?;
        let names = names_for(tx, "users", "display_name", &dropped).await?;
        for id in dropped {
            let name = names.get(&id).cloned().unwrap_or_default();
            insert_activity_row(
                tx, ctx, "updated", "assignees", "removed assignee ", Some(name.as_str()), Some(""), Some(id), None,
            )
            .await?;
        }
    }
    if let Some(requested) = body.estimate_point {
        if requested != current.estimate_point_id {
            // `track_estimate_points` NPEs when the new estimate is None
            // (Django loses the whole batch); skip the row (deviation 6).
            if let Some(new_id) = requested {
                let old = match current.estimate_point_id {
                    Some(id) => estimate_info(tx, id).await?,
                    None => None,
                };
                let new = estimate_info(tx, new_id).await?;
                let (old_value, new_value, field) = match new {
                    Some((new_value, estimate_type)) => (
                        old.as_ref().map(|(v, _)| v.clone()),
                        Some(new_value),
                        format!("estimate_{estimate_type}"),
                    ),
                    None => (None, None, String::new()),
                };
                if !field.is_empty() {
                    insert_activity_row(
                        tx,
                        ctx,
                        "updated",
                        &field,
                        "updated the estimate point to ",
                        old_value.as_deref(),
                        new_value.as_deref(),
                        current.estimate_point_id,
                        Some(new_id),
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}
```

Wire it in `patch_issue` **before** the bridge block added in Task 3 (the diff must read the pre-request bridge sets). The final in-transaction order is: UPDATE → live ids + activities → `replace_bridges` → description version → `tx.commit()`:

```rust
    let skip_activity = body
        .skip_activity
        .as_ref()
        .map(is_truthy)
        .unwrap_or(false)
        && body.description_html.is_some();
    if !skip_activity {
        let workspace_id: Uuid = sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?;
        let ctx = ActivityCtx {
            issue_id: pk,
            project_id,
            workspace_id,
            actor: auth.0,
            epoch: chrono::Utc::now().timestamp() as f64,
        };
        let label_ids_current = live_label_ids(&mut tx, pk).await?;
        let assignee_ids_current = live_assignee_ids(&mut tx, pk).await?;
        write_update_activities(&mut tx, &ctx, &current, &body, &label_ids_current, &assignee_ids_current)
            .await?;
    }
    if matches!(body.assignee_ids, Some(Some(_))) {
        replace_bridges(&mut tx, pk, project_id, auth.0, Some(&assignees), None).await?;
    }
    if matches!(body.label_ids, Some(Some(_))) {
        replace_bridges(&mut tx, pk, project_id, auth.0, None, Some(&labels)).await?;
    }
```

(When Task 5 lands, its version block goes inside the same `if !skip_activity` guard, after `write_update_activities`; `workspace_id` is then already in scope.)

- [ ] **Step 5: Run to verify pass**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
```

Expected: `15 passed` (12 + 3).

- [ ] **Step 6: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_update.rs crates/api/src/routes/issue_activity_write.rs crates/api/tests/issue_patch_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/src/routes/issue_activity_write.rs apps/api-rs/crates/api/tests/issue_patch_test.rs
git commit -m "feat(api-rs): legacy issue PATCH writes per-field activities and subscribers"
```

---

### Task 5: Description versions (update + create)

**Files:**

- Create: `crates/api/src/routes/issue_version_write.rs`
- Modify: `crates/api/src/routes/mod.rs` (`pub mod issue_version_write;`)
- Modify: `crates/api/src/routes/issue_update.rs` (call on changed description)
- Modify: `crates/api/src/routes/issue_write.rs` (call on create)
- Test: `crates/api/tests/issue_patch_test.rs`, `crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/api/tests/issue_patch_test.rs`:

```rust
type VersionRow = (Uuid, String, Uuid, Option<Uuid>);

async fn versions(pool: &PgPool, issue_id: Uuid) -> Vec<VersionRow> {
    sqlx::query_as(
        "SELECT id, description_html, owned_by_id, created_by_id FROM issue_description_versions \
         WHERE issue_id = $1 ORDER BY last_saved_at",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("version rows")
}

#[tokio::test]
async fn patch_records_description_versions_with_merge_window() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "versions").await;
    // The create path records the initial snapshot (Task 5 create wiring).
    let baseline = versions(&pool, issue_id).await.len();
    assert_eq!(baseline, 1);

    // Non-description update → no version.
    let (status, _) =
        patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(json!({"priority": "high"}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(versions(&pool, issue_id).await.len(), baseline);

    // Same actor within 600 s → MERGES into the create row (Django
    // `should_update_existing_version`).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>v1</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = versions(&pool, issue_id).await;
    assert_eq!(rows.len(), baseline, "same owner + <600s merges");
    assert_eq!(rows.last().unwrap().1, "<p>v1</p>");

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>v2</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows2 = versions(&pool, issue_id).await;
    assert_eq!(rows2.len(), baseline);
    assert_eq!(rows2.last().unwrap().1, "<p>v2</p>");

    // Another actor → a new row (created_by = issue creator).
    let issue_creator: Uuid = sqlx::query_scalar("SELECT created_by_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let other = scratch.add_actor(&pool, Some(15), Some(15)).await;
    let (status, _) = patch_issue_req(&st, &scratch, other, issue_id, patch(json!({"description_html": "<p>v3</p>"}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows3 = versions(&pool, issue_id).await;
    assert_eq!(rows3.len(), baseline + 1);
    let row = rows3.last().unwrap();
    assert_eq!(row.1, "<p>v3</p>");
    assert_eq!(row.2, other);
    assert_eq!(row.3, Some(issue_creator));

    // Same value → no version (Django compares old vs new html).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        other,
        issue_id,
        patch(json!({"description_html": "<p>v3</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(versions(&pool, issue_id).await.len(), baseline + 1);

    scratch.cleanup(&pool).await;
}
```

Also add `DELETE FROM issue_description_versions WHERE project_id = $1` to `Scratch::cleanup` and to the `purge` statement list in `crates/api/tests/issue_create_test.rs` (the create path now writes version rows).

Append to `crates/api/tests/issue_create_test.rs`:

```rust
#[tokio::test]
async fn create_records_initial_description_version() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            name: "versioned create".to_string(),
            description_html: Some("<p>born</p>".to_string()),
            ..base_body("unused", scratch.state_id)
        }),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    let issue_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    let rows: Vec<(String, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT description_html, owned_by_id, updated_by_id FROM issue_description_versions WHERE issue_id = $1",
    )
    .bind(issue_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "<p>born</p>");
    assert_eq!(rows[0].1, Some(scratch.user_id));
    assert_eq!(rows[0].2, None, "create leaves updated_by NULL");

    sqlx::query("DELETE FROM issue_description_versions WHERE issue_id = $1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .ok();
    scratch.cleanup(&pool).await;
}
```

(`..base_body(...)` uses struct update syntax; `base_body` returns `CreateIssue` — `description_html` is set explicitly before the spread, which Rust allows because the spread fills the rest.)

- [ ] **Step 2: Run to verify failure**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test patch_records_description -- --test-threads=1
```

Expected: FAIL — zero version rows.

- [ ] **Step 3: Implement `issue_version_write.rs`**

```rust
//! Description-version writer — mirrors `issue_description_version_task`
//! (`plane/bgtasks/issue_description_version_task.py:44-80`): skip unchanged
//! descriptions, merge into the latest row when it is the same owner within
//! 600 s, otherwise insert a fresh snapshot.

use serde_json::Value;
use uuid::Uuid;

use super::page::strip_tags_text;

pub(crate) async fn record_description_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    issue_created_by_id: Option<Uuid>,
    issue_updated_by_id: Option<Uuid>,
    description_html: &str,
    description_json: &Value,
) -> Result<(), sqlx::Error> {
    let latest: Option<(Uuid, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, owned_by_id, last_saved_at FROM issue_description_versions \
         WHERE issue_id = $1 ORDER BY last_saved_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(&mut **tx)
    .await?;

    let stripped: Option<String> = if description_html.is_empty() {
        None
    } else {
        Some(strip_tags_text(description_html))
    };

    if let Some((id, owned_by, last_saved_at)) = latest {
        if owned_by == actor && (chrono::Utc::now() - last_saved_at).num_seconds() <= 600 {
            sqlx::query(
                "UPDATE issue_description_versions SET description_binary = NULL, description_html = $1, \
                 description_stripped = $2, description_json = $3, last_saved_at = now() WHERE id = $4",
            )
            .bind(description_html)
            .bind(stripped)
            .bind(description_json.clone())
            .bind(id)
            .execute(&mut **tx)
            .await?;
            return Ok(());
        }
    }

    sqlx::query(
        "INSERT INTO issue_description_versions (id, description_binary, description_html, \
         description_stripped, description_json, last_saved_at, owned_by_id, issue_id, project_id, \
         workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), NULL, $1, $2, $3, now(), $4, $5, $6, $7, $8, $9, now(), now())",
    )
    .bind(description_html)
    .bind(stripped)
    .bind(description_json.clone())
    .bind(actor)
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(issue_created_by_id)
    .bind(issue_updated_by_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

Add `pub mod issue_version_write;` to `routes/mod.rs`.

- [ ] **Step 4: Wire the update and create paths**

In `issue_update.rs`, inside the `if !skip_activity` block after `write_update_activities`:

```rust
        // `issue_description_version_task.delay` (`base.py:700-707`).
        let stored_html = sanitized_html.as_deref().unwrap_or(&current.description_html);
        if sanitized_html.is_some() && stored_html != current.description_html {
            let description_json = body
                .description
                .clone()
                .flatten()
                .unwrap_or_else(|| current.description_json.clone());
            record_description_version(
                &mut tx,
                pk,
                project_id,
                workspace_id,
                auth.0,
                current.created_by_id,
                Some(auth.0),
                stored_html,
                &description_json,
            )
            .await?;
        }
```

In `issue_write.rs::create`, after `insert_subscribers(...)` and before `tx.commit()`:

```rust
    // Django create also records the initial description version
    // (`base.py:483-488`, `is_creating=True`).
    super::issue_version_write::record_description_version(
        &mut tx,
        out.id,
        project_id,
        workspace_id,
        auth.0,
        Some(auth.0),
        None,
        description_html,
        &json!({}),
    )
    .await?;
```

(`json!` is already imported in `issue_write.rs`.)

- [ ] **Step 5: Run to verify pass**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: `16 passed` in the patch file plus the new create test (`create_records_initial_description_version`), all create tests passing.

- [ ] **Step 6: Format, lint, commit**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/issue_update.rs crates/api/src/routes/issue_write.rs \
  crates/api/src/routes/issue_version_write.rs crates/api/tests/issue_patch_test.rs crates/api/tests/issue_create_test.rs
cargo clippy -p api --all-targets 2>&1 | tail -n 20
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/src/routes/issue_write.rs \
  apps/api-rs/crates/api/src/routes/issue_version_write.rs apps/api-rs/crates/api/src/routes/mod.rs \
  apps/api-rs/crates/api/tests/issue_patch_test.rs apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "feat(api-rs): record issue description versions on create and update"
```

---

### Task 6: Verification, inventory note, rebuild, HTTP e2e

**Files:**

- Modify: `crates/api/parity-inventory.json`

- [ ] **Step 1: Full Rust suite (no regressions)**

```bash
cd /home/ghifari/plane-for-itsm/apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane REDIS_URL=redis://localhost:6379 \
  cargo test -p api -p common --no-fail-fast -- --test-threads=1
```

Expected: `0 failed` everywhere (baseline 1084 + the new tests).

- [ ] **Step 2: Update the parity inventory note**

In `crates/api/parity-inventory.json`, the entry at lines 188-210 (`/api/workspaces/:slug/projects/:project_id/issues/:pk/`) — append to `notes`:

```
 | Update-parity: PATCH persists all web-sent scalars (state/parent/dates/sort_order/point/type/estimate), replaces assignee/label bridges, mirrors Issue.save side effects (description_stripped, completed_at, updated_by), writes per-field activities + subscribers and description versions; skip_activity honoured; 404/403/204 unchanged.
```

- [ ] **Step 3: Rebuild the API image and restart**

```bash
cd /home/ghifari/plane-for-itsm
docker compose -f docker-compose-local.yml build api
docker compose -f docker-compose-local.yml up -d api worker beat-worker
sleep 5
docker logs plane-for-itsm-api-1 2>&1 | tail -n 3
```

Expected: rust-api listening on 8000.

- [ ] **Step 4: HTTP e2e**

The create e2e scratch data was cleaned up after the create slice, so re-create it first: run the scratch-data `INSERT` block **verbatim** from `docs/superpowers/plans/2026-09-21-legacy-issue-create-full-parity.md` Task 5 Step 5 (owner `...f001`, workspace `...f002`, project `...f003`, state `...f004`, assignee `...f006`, label `...f007`, API token `itseq-full-owner-key`). Then add a completed state and run the PATCH probes:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c \
 "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
  VALUES ('00000000-0000-0000-0000-00000000f008', 'Done', '', '#16A34A', 'done', \
  '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002', 65535, 'completed', false, false, now(), now());"
```

Create one issue and capture its id:

```bash
URL=http://localhost:8000/api/workspaces/itseq-full/projects/00000000-0000-0000-0000-00000000f003/issues/
H=(-H 'Content-Type: application/json' -H 'Origin: http://localhost:3000' -H 'X-Api-Key: itseq-full-owner-key')
# The project has default_assignee = ...f006, so the created issue starts
# assigned to f006 (create-slice behavior).
ISSUE=$(curl -sS -X POST "$URL" "${H[@]}" -d '{"name":"patch-probe","state_id":"00000000-0000-0000-0000-00000000f004"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
echo "issue=$ISSUE"

# Owner (f001) is a project member role 20 → valid assignee; switching to it
# produces one added + one removed assignee activity.
curl -sS -o /tmp/opencode/patch1.json -w 'patch-axis: HTTP %{http_code}\n' -X PATCH "$URL$ISSUE/" "${H[@]}" -d '{
  "state_id":"00000000-0000-0000-0000-00000000f008",
  "assignee_ids":["00000000-0000-0000-0000-00000000f001"],
  "label_ids":["00000000-0000-0000-0000-00000000f007"],
  "start_date":"2026-09-01","target_date":"2026-09-30","sort_order":1234.5,
  "description_html":"<p>edited</p>"
}'
# expect: HTTP 204

curl -sS -o /tmp/opencode/patch2.json -w 'patch-skip: HTTP %{http_code}\n' -X PATCH "$URL$ISSUE/" "${H[@]}" -d '{
  "description_html":"<p>migrated</p>","skip_activity":"true"
}'
# expect: HTTP 204
```

Assert DB state (one query):

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c \
 "SELECT i.state_id, i.completed_at IS NOT NULL AS completed, i.sort_order, i.description_stripped, \
   (SELECT COUNT(*) FROM issue_assignees WHERE issue_id = i.id AND deleted_at IS NULL) AS assignees, \
   (SELECT COUNT(*) FROM issue_labels WHERE issue_id = i.id AND deleted_at IS NULL) AS labels, \
   (SELECT COUNT(*) FROM issue_activities WHERE issue_id = i.id) AS activities, \
   (SELECT COUNT(*) FROM issue_description_versions WHERE issue_id = i.id) AS versions \
  FROM issues i WHERE i.id = '$ISSUE';"
```

Expected: `state=...f008 | completed=t | sort_order=1234.5 | description_stripped=migrated | assignees=1 | labels=1 | activities=9 | versions=1`

- activities: create's `created` row (1) + patch1's `state` (1), `assignees` (2: added f001, removed f006), `labels` (1), `description` (1), `start_date` (1), `target_date` (1) = 9; patch2 is `skip_activity` → 0.
- versions: create writes the initial row, then patch1's description edit from the same actor lands within 600 s → merged in place → 1 row holding `<p>edited</p>` (patch2 skipped).
- The live assignee is now `...f001`; run `SELECT assignee_id FROM issue_assignees WHERE issue_id = '$ISSUE' AND deleted_at IS NULL;` to confirm.

- [ ] **Step 5: Clean up the e2e scratch data**

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 -q <<'SQL'
DELETE FROM issue_description_versions WHERE project_id = '00000000-0000-0000-0000-00000000f003';
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
 "SELECT 'leftover_patch=' || count(*) FROM users WHERE username LIKE 'itseq-full-%';"
```

Expected: `leftover_patch=0`.

- [ ] **Step 6: Manual UI checks (report in the final review, not automatable here)**

1. Kanban: drag a card across state groups and reorder within a group → reload the board → new state/order persist.
2. Sidebar: change assignee, labels, start/target date, estimate, priority → reload → values persist.
3. Issue detail: edit the description → reload → persists; activity feed shows `updated the description to`.
4. Activity feed: each of the above appears with the right verb/wording.
5. Description history (work item versions): shows the initial create snapshot and one row per description edit session.
6. Permission: a project GUEST non-creator gets an error toast on edit; a MEMBER succeeds.

- [ ] **Step 7: Commit the inventory note**

```bash
cd /home/ghifari/plane-for-itsm
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "docs(api-rs): parity inventory reflects legacy issue PATCH full parity"
```

---

## Self-review notes

- **Spec coverage:** request surface + 400s (Task 1), scalar persistence + `Issue.save` side effects (Task 2), bridges (Task 3), activities + `skip_activity` (Task 4), description versions incl. the create call (Task 5), verification/inventory/e2e (Task 6). Web call sites from the gap table each map to a Task 1/2/3 field.
- **Type consistency:** `PatchIssue` is tri-state (`Option<Option<T>>`) everywhere; `ActivityCtx` fields are `issue_id/project_id/workspace_id/actor/epoch`; `CurrentIssue` column list matches the snapshot SELECT; `insert_activity_row` takes `Option<&str>`/`Option<Uuid>`; `record_description_version` is called with `(tx, issue_id, project_id, workspace_id, actor, issue_created_by_id, issue_updated_by_id, html, json)` from both update and create.
- **Known deviations are enumerated** above (1-10) and are either create-slice precedents or deliberate sane behavior replacing a Django crash.
- **Description-version merge is tested in both directions:** same owner within 600 s merges (including merging into the create-time row), a different owner inserts, an unchanged value is a no-op.
- **Activity ordering quirk is encoded:** the description merge lookup runs before this request's rows (Django builds the list in memory and looks at the DB before `bulk_create`), so `write_update_activities` handles `description` first.
- **Transaction order is explicit:** UPDATE → live bridge ids + activities → `replace_bridges` → description version → commit.
- **No placeholders:** every step carries the code/command to run (the two cross-references are exact: `issue_create_test.rs` line ranges for fixture copying and the create plan's e2e SQL block).
