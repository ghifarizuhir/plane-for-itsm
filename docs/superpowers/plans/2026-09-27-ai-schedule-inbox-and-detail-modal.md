# AI Schedule: Inbox Run Notifications + Scheduler Detail Modal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kirim notifikasi in-app ke pembuat jadwal setiap run terjadwal selesai (success/failed), dan ubah halaman Scheduler menjadi list ringkas dengan search/filter + modal detail (recipe read-only, history, aksi).

**Architecture:** Worker Rust menulis satu row `notifications` bertipe `ai_schedule_run` lewat helper SQL idempotent setelah run mencapai status terminal; filter list Inbox di API Rust (dan parity Django) diperluas; FE menambah cabang render/klik di kartu notifikasi, lalu Scheduler memakai helper filter/sort murni + `ModalCore` dengan deep link `?schedule=<id>`.

**Tech Stack:** Rust (axum, sqlx, tokio) di `apps/api-rs`; Django (parity/contract tests) di `apps/api`; React + MobX + react-router/next-navigation shim + vitest di `apps/web`; `@plane/ui` `ModalCore`.

**Spec:** `docs/superpowers/specs/2026-09-27-ai-schedule-inbox-and-detail-modal-design.md`

**Execution notes (baca sebelum mulai):**

- Semua perintah Rust dijalankan dari `apps/api-rs`; test DB-backed butuh `-- --test-threads=1`.
- DB test default `postgres://plane:plane@localhost:5432/plane`; Redis default `redis://127.0.0.1:6379`.
- Deploy: worker/API harus naik bersamaan (worker menulis row, API menampilkannya).
- Lint web menegakkan `no-array-index-key`; jangan pakai `key={index}` — pakai id stabil.
- Commit hanya file yang disebut di task (working tree punya file unrelated yang termodifikasi).

---

### Task 1: Worker — helper notifikasi + jalur sukses

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`
- Test: `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan test berikut di akhir `apps/api-rs/crates/worker/tests/ai_schedule_test.rs` (sebelum helper `spawn_fake_upstream` boleh di mana saja, asal setelah fungsi `spawn_fake_upstream` dideklarasikan — Rust tidak peduli urutan):

```rust
#[tokio::test]
async fn run_success_notifies_scheduled_creator_once() {
    let pool = pool().await;
    let slug = format!("aisn-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Notify', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
         timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily digest', 'Summarize', 'daily', '09:00', 'UTC', true, \
         now() + interval '1 day', $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize', now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let base_url = spawn_fake_upstream().await;
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", &base_url);
    std::env::set_var("LLM_MODEL", "test-model");
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id }))
        .await
        .expect("run");

    let (receiver, notification_workspace, title, status, schedule_ref): (Uuid, Uuid, String, String, String) =
        sqlx::query_as(
            "SELECT receiver_id, workspace_id, title, data->'ai_schedule'->>'status', \
                    data->'ai_schedule'->>'schedule_id' \
             FROM notifications WHERE entity_name = 'ai_schedule_run' AND entity_identifier = $1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(receiver, user_id);
    assert_eq!(notification_workspace, workspace_id);
    assert_eq!(title, "Daily digest");
    assert_eq!(status, "success");
    assert_eq!(schedule_ref, schedule_id.to_string());

    // helper idempotent per run
    ai_schedule::insert_run_notification(&pool, run_id)
        .await
        .expect("second insert");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE entity_name = 'ai_schedule_run' AND entity_identifier = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);

    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");
    sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p worker --test ai_schedule_test run_success_notifies_scheduled_creator_once -- --test-threads=1`
Expected: FAIL — `insert_run_notification` belum ada (compile error) atau `fetch_one` panic (no rows).

- [ ] **Step 3: Implementasi helper + panggilan di jalur sukses**

Di `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`, tambahkan fungsi berikut setelah `prune_runs`:

```rust
/// One in-app notification per scheduled run that reaches a terminal status.
/// Idempotent per run; skips manual runs and soft-deleted schedules.
pub async fn insert_run_notification(pool: &PgPool, run_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO notifications (id, workspace_id, receiver_id, entity_name, entity_identifier, \
         title, sender, data, message_html, created_at, updated_at) \
         SELECT gen_random_uuid(), r.workspace_id, s.created_by_id, 'ai_schedule_run', r.id, \
                s.name, 'in_app:ai_schedule:run', \
                jsonb_build_object('ai_schedule', jsonb_build_object( \
                  'schedule_id', s.id, 'run_id', r.id, 'name', s.name, 'status', r.status, \
                  'finished_at', r.finished_at, 'error', r.error)), \
                '<p></p>', now(), now() \
         FROM ai_schedule_runs r JOIN ai_schedules s ON s.id = r.schedule_id \
         WHERE r.id = $1 AND r.trigger = 'scheduled' AND r.status IN ('success', 'failed') \
           AND s.deleted_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.entity_name = 'ai_schedule_run' \
                           AND n.entity_identifier = r.id)",
    )
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}
```

Di `run()`, ubah blok sukses menjadi:

```rust
            if updated.rows_affected() == 0 {
                tracing::warn!(
                    run_id=%run.id,
                    "ai.schedule.run: run no longer running, success not recorded"
                );
            } else {
                insert_run_notification(pool, run.id).await?;
                tracing::info!(
                    run_id=%run.id,
                    schedule_id=%run.schedule_id,
                    workspace_id=%run.workspace_id,
                    duration_ms=%started.elapsed().as_millis(),
                    "ai.schedule.run finished"
                );
            }
```

- [ ] **Step 4: Tambahkan cleanup notifikasi ke test lama yang kini memicu notifikasi**

Di `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`, pada blok cleanup `run_success_records_response_and_prunes` dan `run_marks_failed_when_llm_is_not_configured` dan `run_fails_on_invalid_spec_before_llm` dan `tick_sweeps_stuck_runs`, tambahkan baris pertama berikut **sebelum** `DELETE FROM ai_schedule_runs`:

```rust
    sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
```

- [ ] **Step 5: Jalankan test worker, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p worker --test ai_schedule_test -- --test-threads=1`
Expected: PASS semua (termasuk `run_success_notifies_scheduled_creator_once`).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/api-rs/crates/worker/tests/ai_schedule_test.rs
git commit -m "feat(worker): insert in-app notification when a scheduled run finishes"
```

---

### Task 2: Worker — jalur gagal, sweep, dan guard manual/terhapus

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`
- Test: `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

(a) Di `run_marks_failed_when_llm_is_not_configured`, setelah assert error, tambahkan:

```rust
    let (receiver, title, status): (Uuid, String, String) = sqlx::query_as(
        "SELECT receiver_id, title, data->'ai_schedule'->>'status' FROM notifications \
         WHERE entity_name = 'ai_schedule_run' AND entity_identifier = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receiver, user_id);
    assert_eq!(title, "Daily");
    assert_eq!(status, "failed");
```

(b) Tambahkan dua test baru:

```rust
#[tokio::test]
async fn run_does_not_notify_manual_runs() {
    let pool = pool().await;
    let slug = format!("aism-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Manual', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
         timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Manual digest', 'Summarize', 'daily', '09:00', 'UTC', true, \
         now() + interval '1 day', $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'manual', 'Summarize', now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let base_url = spawn_fake_upstream().await;
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", &base_url);
    std::env::set_var("LLM_MODEL", "test-model");
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id }))
        .await
        .expect("run");

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE entity_name = 'ai_schedule_run' AND entity_identifier = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "manual runs must not notify");

    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");
    sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn soft_deleted_schedule_does_not_notify() {
    let pool = pool().await;
    let slug = format!("aisd-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Deleted', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
         timezone, enabled, next_run_at, proposal_key, created_at, updated_at, deleted_at) \
         VALUES ($1, $2, $3, 'Gone digest', 'Summarize', 'daily', '09:00', 'UTC', true, \
         now() + interval '1 day', $4, now(), now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize', now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    // No LLM configured: the run fails before any model call.
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::remove_var("LLM_API_KEY");
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id }))
        .await
        .expect("run handled");
    std::env::remove_var("SKIP_ENV_VAR");

    let (status, _): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM ai_schedule_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE entity_name = 'ai_schedule_run' AND entity_identifier = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "soft-deleted schedules must not notify");

    sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}
```

(c) Di `tick_sweeps_stuck_runs`, tambahkan run manual stuck dan assert notifikasi. Sisipkan setelah insert `stuck_queued`:

```rust
    let stuck_manual = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at, started_at) \
         VALUES ($1, $2, $3, 'running', 'manual', 'x', now() - interval '1 hour', now() - interval '20 minutes')",
    )
    .bind(stuck_manual)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();
```

Lalu setelah assert `queued_error`, tambahkan:

```rust
    let notified: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE entity_name = 'ai_schedule_run' AND workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(notified, 2, "swept scheduled runs notify; the manual one does not");
    let manual_notified: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE entity_name = 'ai_schedule_run' AND entity_identifier = $1",
    )
    .bind(stuck_manual)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(manual_notified, 0);
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p worker --test ai_schedule_test run_marks_failed_when_llm_is_not_configured -- --test-threads=1`
Expected: FAIL — `fetch_one` panic karena belum ada row notifikasi.

- [ ] **Step 3: Implementasi jalur gagal + sweep**

(a) Di `finish_failed`, ubah blok penutup menjadi:

```rust
    if updated.rows_affected() == 0 {
        tracing::warn!(
            run_id=%run_id,
            "ai.schedule.run: run no longer running, failure not recorded"
        );
    } else {
        insert_run_notification(pool, run_id).await?;
    }
    Ok(())
```

(b) Ganti seluruh isi `sweep_stuck_runs` dengan:

```rust
async fn sweep_stuck_runs(pool: &PgPool) -> anyhow::Result<()> {
    let swept_running: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run did not finish within 15 minutes', finished_at = now() \
         WHERE status = 'running' AND started_at < now() - interval '15 minutes' \
         RETURNING id",
    )
    .fetch_all(pool)
    .await?;
    let swept_queued: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run was never started (worker backlog or lost job)', finished_at = now() \
         WHERE status = 'queued' AND created_at < now() - interval '6 hours' \
         RETURNING id",
    )
    .fetch_all(pool)
    .await?;
    for run_id in swept_running.into_iter().chain(swept_queued) {
        insert_run_notification(pool, run_id).await?;
    }
    Ok(())
}
```

- [ ] **Step 4: Jalankan semua test worker, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p worker --test ai_schedule_test -- --test-threads=1`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/api-rs/crates/worker/tests/ai_schedule_test.rs
git commit -m "feat(worker): notify scheduled failures and swept stuck runs only"
```

---

### Task 3: API Rust — filter list Inbox menyertakan tipe baru

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/notification.rs:265`
- Test: `apps/api-rs/crates/api/tests/notification_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Ganti seluruh isi `apps/api-rs/crates/api/tests/notification_test.rs` dengan (mempertahankan unit test lama + menambah test DB):

```rust
use api::middleware::auth::AuthUser;
use api::routes::notification::{self, validate_preference_patch, PREFERENCE_KEYS};
use api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn rejects_unknown_preference_keys() {
    let mut patch = HashMap::new();
    patch.insert("telepathy".to_string(), serde_json::json!(true));
    let err = validate_preference_patch(&patch).unwrap_err();
    assert!(err.to_lowercase().contains("unknown"));
}

#[test]
fn accepts_known_preference_keys() {
    for key in PREFERENCE_KEYS {
        let mut patch = HashMap::new();
        patch.insert(key.to_string(), serde_json::json!(false));
        assert!(validate_preference_patch(&patch).is_ok(), "key={key}");
    }
}

#[test]
fn rejects_non_bool_preference_value() {
    let mut patch = HashMap::new();
    patch.insert("comment".to_string(), serde_json::json!("yes"));
    let err = validate_preference_patch(&patch).unwrap_err();
    assert!(err.to_lowercase().contains("boolean"));
}

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
}

async fn state(pool: &PgPool) -> AppState {
    AppState {
        pool: pool.clone(),
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) {
    sqlx::query(
        "INSERT INTO users (id, password, username, email, first_name, last_name, avatar, \
         date_joined, created_at, updated_at, last_location, created_location, is_superuser, \
         is_managed, is_password_expired, is_active, is_staff, is_email_verified, \
         is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip, \
         last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid, \
         is_password_reset_required) \
         VALUES ($1, '', $2, $3, '', '', '', now(), now(), now(), '', '', false, false, \
         false, true, false, false, true, $4, 'UTC', '', '', '', '', false, $2, true, false)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
}

#[tokio::test]
async fn list_includes_schedule_run_notifications() {
    let pool = pool().await;
    let slug = format!("ntf-{}", Uuid::new_v4().simple());
    let user_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
         background_color) VALUES ($1, 'Inbox', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(user_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();
    let run_id = Uuid::new_v4();
    for (entity_name, title, sender, data) in [
        (
            "issue",
            "Issue notification",
            "in_app:issue_activities:created",
            json!({}),
        ),
        (
            "ai_schedule_run",
            "Daily report",
            "in_app:ai_schedule:run",
            json!({"ai_schedule": {"schedule_id": schedule_id.to_string(), "run_id": run_id.to_string()}}),
        ),
        ("mystery", "Hidden", "in_app:other", json!({})),
    ] {
        sqlx::query(
            "INSERT INTO notifications (id, workspace_id, receiver_id, entity_name, \
             entity_identifier, title, sender, data, message_html, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, '<p></p>', now(), now())",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(entity_name)
        .bind(Uuid::new_v4())
        .bind(title)
        .bind(sender)
        .bind(data)
        .execute(&pool)
        .await
        .unwrap();
    }

    let state = state(&pool).await;
    let (status, Json(rows)) = notification::list(
        State(state.clone()),
        AuthUser(user_id),
        Path(slug.clone()),
        Query(HashMap::<String, String>::new()),
    )
    .await
    .expect("list ok");
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["entity_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"issue"), "issue notifications still listed");
    assert!(
        names.contains(&"ai_schedule_run"),
        "schedule run notifications must be listed"
    );
    assert!(!names.contains(&"mystery"), "unknown entity types stay hidden");

    let Json(counts) = notification::unread(State(state.clone()), AuthUser(user_id), Path(slug.clone()))
        .await
        .expect("unread ok");
    // The unread badge intentionally counts every entity_name (pre-existing
    // behaviour); only the list filter hides unknown types.
    assert_eq!(counts["total_unread_notifications_count"], json!(3));

    sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --test notification_test list_includes_schedule_run_notifications -- --test-threads=1`
Expected: FAIL — `names.contains(&"ai_schedule_run")` false (filter masih `entity_name = 'issue'`).

- [ ] **Step 3: Implementasi filter**

Di `apps/api-rs/crates/api/src/routes/notification.rs`, pada `base_where` (sekitar baris 265) ubah:

```
WHERE w.slug = $1 AND n.receiver_id = $2 AND n.entity_name = 'issue' \
```

menjadi:

```
WHERE w.slug = $1 AND n.receiver_id = $2 \
  AND n.entity_name IN ('issue', 'ai_schedule_run') \
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p api --test notification_test -- --test-threads=1`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/notification.rs apps/api-rs/crates/api/tests/notification_test.rs
git commit -m "feat(api): include schedule run notifications in the inbox list"
```

---

### Task 4: API Rust — cleanup notifikasi saat delete + cap 100

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_schedule.rs:25,504-524`
- Test: `apps/api-rs/crates/api/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

(a) Di `Scratch::purge` (`apps/api-rs/crates/api/tests/ai_schedule_test.rs`), tambahkan baris pertama:

```rust
        sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
```

(b) Tambahkan test baru:

```rust
#[tokio::test]
async fn destroy_soft_deletes_schedule_run_notifications() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let notification_id: Uuid = sqlx::query_scalar(
        "INSERT INTO notifications (id, workspace_id, receiver_id, entity_name, entity_identifier, \
         title, sender, data, message_html, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 'ai_schedule_run', $3, 'Daily report', \
                 'in_app:ai_schedule:run', \
                 jsonb_build_object('ai_schedule', jsonb_build_object('schedule_id', $4::text)), \
                 '<p></p>', now(), now()) RETURNING id",
    )
    .bind(scratch.workspace_id)
    .bind(scratch.user_id)
    .bind(Uuid::new_v4())
    .bind(schedule_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, _) = ai_schedule::destroy(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("destroy ok");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let soft_deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM notifications WHERE id = $1")
            .bind(notification_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(soft_deleted, "notifications follow the schedule soft delete");

    scratch.purge(&pool).await;
}
```

(c) Ubah test cap: ganti nama `create_enforces_twenty_schedule_limit` menjadi `create_enforces_hundred_schedule_limit`, ganti `for _ in 0..20` menjadi `for _ in 0..100`, dan ganti assert pesan:

```rust
    assert!(err["error"].as_str().unwrap().contains("100"));
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --test ai_schedule_test destroy_soft_deletes_schedule_run_notifications create_enforces_hundred_schedule_limit -- --test-threads=1`
Expected: FAIL — notifikasi belum ter-soft-delete; cap masih 20.

- [ ] **Step 3: Implementasi**

(a) Di `apps/api-rs/crates/api/src/routes/ai_schedule.rs` baris 25:

```rust
pub const MAX_SCHEDULES_PER_WORKSPACE: i64 = 100;
```

(b) Di `destroy`, setelah `UPDATE ai_schedules ... WHERE id = $1`, tambahkan:

```rust
    sqlx::query(
        "UPDATE notifications SET deleted_at = now(), updated_at = now() \
         WHERE entity_name = 'ai_schedule_run' AND deleted_at IS NULL \
           AND data->'ai_schedule'->>'schedule_id' = $1",
    )
    .bind(schedule_id.to_string())
    .execute(&st.pool)
    .await?;
```

- [ ] **Step 4: Jalankan seluruh test api, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p api --test ai_schedule_test -- --test-threads=1`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_schedule.rs apps/api-rs/crates/api/tests/ai_schedule_test.rs
git commit -m "feat(api): clean up run notifications on schedule delete and raise the cap"
```

---

### Task 5: Django — parity filter list notifikasi

**Files:**

- Modify: `apps/api/plane/app/views/notification/base.py:66`
- Test: `apps/api/plane/tests/contract/api/test_issue_notifications.py`

- [ ] **Step 1: Tulis test yang gagal**

Di akhir `apps/api/plane/tests/contract/api/test_issue_notifications.py`, tambahkan (dan tambahkan import `Notification` + `uuid4` di bagian import atas: `from uuid import uuid4`, serta ubah import model menjadi `from plane.db.models import Issue, Notification, Project, ProjectMember, State, User`):

```python
@pytest.mark.contract
class TestNotificationEntityFilterContract:
    """
    Contract: the workspace notification list includes work-item notifications
    and AI schedule run notifications, and nothing else.
    """

    @pytest.mark.django_db
    def test_list_includes_schedule_run_notifications(self, session_client, workspace, create_user):
        Notification.objects.create(
            workspace=workspace,
            receiver=create_user,
            entity_name="issue",
            entity_identifier=uuid4(),
            title="Issue notification",
            sender="in_app:issue_activities:created",
            data={},
        )
        Notification.objects.create(
            workspace=workspace,
            receiver=create_user,
            entity_name="ai_schedule_run",
            entity_identifier=uuid4(),
            title="Daily report",
            sender="in_app:ai_schedule:run",
            data={"ai_schedule": {"schedule_id": str(uuid4())}},
        )
        Notification.objects.create(
            workspace=workspace,
            receiver=create_user,
            entity_name="mystery",
            entity_identifier=uuid4(),
            title="Hidden",
            sender="in_app:other",
        )

        response = session_client.get(f"/api/workspaces/{workspace.slug}/users/notifications/")

        assert response.status_code == status.HTTP_200_OK
        assert {row["entity_name"] for row in response.data} == {"issue", "ai_schedule_run"}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/contract/api/test_issue_notifications.py -k EntityFilter`
Expected: FAIL — set hanya `{"issue"}`.

- [ ] **Step 3: Implementasi parity**

Di `apps/api/plane/app/views/notification/base.py` baris 66, ubah:

```python
            .filter(entity_name="issue")
```

menjadi:

```python
            .filter(entity_name__in=["issue", "ai_schedule_run"])
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/contract/api/test_issue_notifications.py -m contract`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/app/views/notification/base.py apps/api/plane/tests/contract/api/test_issue_notifications.py
git commit -m "test(api): keep django notification list filter in parity"
```

---

### Task 6: Web — tipe notifikasi + helper murni

**Files:**

- Modify: `packages/types/src/workspace-notifications.ts`
- Modify: `apps/web/core/lib/ai-schedule.ts`
- Test: `apps/web/core/lib/ai-schedule.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `apps/web/core/lib/ai-schedule.test.ts`, tambahkan helper baru ke import yang sudah ada (jangan buat import kedua dari modul yang sama):

```ts
import {
  filterSchedules,
  humanizeSchedule,
  isScheduleCommand,
  isScheduleRunNotification,
  isStructuredProposal,
  runDurationInSeconds,
  scheduleDescription,
  scheduleRunNotificationHref,
  scheduleRunNotificationText,
  scheduleStatusLabel,
  validateScheduleSpec,
} from "./ai-schedule";
import type { TAiSchedule, TAiScheduleSpec } from "./ai-schedule";
```

Lalu tambahkan blok test berikut di akhir file:

```ts
const schedule = (overrides: Partial<TAiSchedule>): TAiSchedule => ({
  id: "s1",
  name: "Daily report",
  frequency: "daily",
  time: "09:00",
  timezone: "UTC",
  enabled: true,
  next_run_at: "2026-09-28T09:00:00Z",
  created_by_id: "u1",
  created_at: "2026-09-01T00:00:00Z",
  prompt: "Summarize",
  spec: null,
  ...overrides,
});

describe("filterSchedules", () => {
  it("filters by name case-insensitively", () => {
    const rows = [schedule({ id: "a", name: "Daily report" }), schedule({ id: "b", name: "Weekly digest" })];
    expect(filterSchedules(rows, { query: "daily", status: "all" }).map((row) => row.id)).toEqual(["a"]);
    expect(filterSchedules(rows, { query: "WEEKLY", status: "all" }).map((row) => row.id)).toEqual(["b"]);
    expect(filterSchedules(rows, { query: "  ", status: "all" })).toHaveLength(2);
  });

  it("filters by status", () => {
    const rows = [schedule({ id: "a", enabled: true }), schedule({ id: "b", enabled: false })];
    expect(filterSchedules(rows, { query: "", status: "active" }).map((row) => row.id)).toEqual(["a"]);
    expect(filterSchedules(rows, { query: "", status: "paused" }).map((row) => row.id)).toEqual(["b"]);
  });

  it("sorts active before paused, then by next run ascending", () => {
    const rows = [
      schedule({ id: "paused", enabled: false, next_run_at: "2026-09-01T00:00:00Z" }),
      schedule({ id: "late", enabled: true, next_run_at: "2026-09-30T09:00:00Z" }),
      schedule({ id: "soon", enabled: true, next_run_at: "2026-09-28T09:00:00Z" }),
    ];
    expect(filterSchedules(rows, { query: "", status: "all" }).map((row) => row.id)).toEqual([
      "soon",
      "late",
      "paused",
    ]);
  });
});

describe("schedule run notifications", () => {
  it("detects the ai_schedule payload", () => {
    expect(
      isScheduleRunNotification({
        ai_schedule: {
          schedule_id: "s1",
          run_id: "r1",
          name: "Daily report",
          status: "success",
          finished_at: null,
        },
      })
    ).toBe(true);
    expect(isScheduleRunNotification({})).toBe(false);
    expect(isScheduleRunNotification(undefined)).toBe(false);
    expect(
      isScheduleRunNotification({
        issue: { id: "i1" },
        issue_activity: { id: "a1", actor: "u1", field: "state", issue_comment: "", verb: "updated" },
      } as never)
    ).toBe(false);
  });

  it("builds the scheduler deep link and status sentence", () => {
    expect(scheduleRunNotificationHref("acme", "s1")).toBe("/acme/scheduler?schedule=s1");
    expect(scheduleRunNotificationText("success")).toBe("Scheduled run finished");
    expect(scheduleRunNotificationText("failed")).toBe("Scheduled run failed");
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm --filter=web test -- ai-schedule` (atau `pnpm --filter=web exec vitest run core/lib/ai-schedule.test.ts`)
Expected: FAIL — helper belum diekspor.

- [ ] **Step 3: Implementasi tipe + helper**

(a) Di `packages/types/src/workspace-notifications.ts`, ubah `TNotificationData` menjadi:

```ts
export type TNotificationScheduleRun = {
  schedule_id: string;
  run_id: string;
  name: string;
  status: "success" | "failed";
  finished_at: string | null;
  error?: string | null;
};

export type TNotificationData = {
  issue?: TNotificationIssueLite | undefined;
  issue_activity?: {
    id: string | undefined;
    actor: string | undefined;
    field: string | undefined;
    issue_comment: string | undefined;
    verb: "created" | "updated" | "deleted";
    new_value: string | undefined;
    old_value: string | undefined;
  };
  ai_schedule?: TNotificationScheduleRun | undefined;
};
```

(b) Di `apps/web/core/lib/ai-schedule.ts`, tambahkan import di baris pertama setelah komentar lisensi:

```ts
import { orderBy } from "lodash-es";
import type { TNotificationData, TNotificationScheduleRun } from "@plane/types";
```

Lalu tambahkan di akhir file:

```ts
export type TAiScheduleListFilter = {
  query: string;
  status: "all" | "active" | "paused";
};

export const RUN_STATUS_BADGE_VARIANTS: Record<TAiScheduleRun["status"], "success" | "danger" | "brand"> = {
  success: "success",
  failed: "danger",
  queued: "brand",
  running: "brand",
};

export const isScheduleRunNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { ai_schedule: TNotificationScheduleRun } =>
  typeof data?.ai_schedule?.schedule_id === "string" && data.ai_schedule.schedule_id.length > 0;

export const scheduleRunNotificationHref = (workspaceSlug: string, scheduleId: string): string =>
  `/${workspaceSlug}/scheduler?schedule=${scheduleId}`;

export const scheduleRunNotificationText = (status: TNotificationScheduleRun["status"]): string =>
  status === "success" ? "Scheduled run finished" : "Scheduled run failed";

/** Search + status filter + deterministic ordering for the scheduler list. */
export const filterSchedules = (schedules: TAiSchedule[], filter: TAiScheduleListFilter): TAiSchedule[] => {
  const query = filter.query.trim().toLowerCase();
  const matches = schedules.filter((schedule) => {
    if (filter.status === "active" && !schedule.enabled) return false;
    if (filter.status === "paused" && schedule.enabled) return false;
    if (!query) return true;
    return schedule.name.toLowerCase().includes(query);
  });
  // NOTE: do not use `.sort()` here — the repo's `oxlint --fix` pre-commit
  // hook rewrites it to `.toSorted()`, which the TS lib target does not know.
  return orderBy(
    matches,
    [
      (schedule) => (schedule.enabled ? 0 : 1),
      (schedule) => (schedule.next_run_at ? 0 : 1),
      (schedule) => schedule.next_run_at ?? "",
      (schedule) => schedule.created_at,
    ],
    ["asc", "asc", "asc", "desc"]
  );
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm --filter=web exec vitest run core/lib/ai-schedule.test.ts`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add packages/types/src/workspace-notifications.ts apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts
git commit -m "feat(web): notification and schedule list helpers with types"
```

---

### Task 7: Web — kartu notifikasi Inbox

**Files:**

- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`
- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/content.tsx`

- [ ] **Step 1: Perbaiki optional chaining di `content.tsx`**

Di `content.tsx` baris 163-166, ubah menjadi:

```tsx
const notificationField = data?.issue_activity?.field;
const newValue = data?.issue_activity?.new_value;
const oldValue = data?.issue_activity?.old_value;
const verb = data?.issue_activity?.verb;
```

- [ ] **Step 2: Ganti `item.tsx` dengan versi baru**

Ganti seluruh isi `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useRouter } from "next/navigation";
import { CalendarOutline, ClockOutline } from "@makeplane/propel/icons";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { Badge } from "@plane/propel/badge";
import { Row } from "@plane/ui";
import { cn, calculateTimeAgo, renderFormattedDate, renderFormattedTime, getFileURL } from "@plane/utils";
// hooks
import { useWorkspaceNotifications } from "@/hooks/store/notifications";
import { useNotification } from "@/hooks/store/notifications/use-notification";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import {
  isScheduleRunNotification,
  scheduleRunNotificationHref,
  scheduleRunNotificationText,
  scheduleStatusLabel,
} from "@/lib/ai-schedule";
// local imports
import { NotificationContent } from "./content";
import { NotificationOption } from "./options";

type TNotificationItem = {
  workspaceSlug: string;
  notificationId: string;
};

export const NotificationItem = observer(function NotificationItem(props: TNotificationItem) {
  const { workspaceSlug, notificationId } = props;
  // router
  const router = useRouter();
  // hooks
  const { currentSelectedNotificationId, setCurrentSelectedNotificationId } = useWorkspaceNotifications();
  const { asJson: notification, markNotificationAsRead } = useNotification(notificationId);
  const { getIsIssuePeeked, setPeekIssue } = useIssueDetail();
  const { getWorkspaceBySlug } = useWorkspace();
  // states
  const [isSnoozeStateModalOpen, setIsSnoozeStateModalOpen] = useState(false);
  const [customSnoozeModal, setCustomSnoozeModal] = useState(false);

  // derived values
  const projectId = notification?.project || undefined;
  const issueId = notification?.data?.issue?.id || undefined;
  const workspace = getWorkspaceBySlug(workspaceSlug);
  const scheduleRun = isScheduleRunNotification(notification?.data) ? notification?.data?.ai_schedule : undefined;

  const notificationField = notification?.data?.issue_activity?.field || undefined;
  const notificationTriggeredBy = notification.triggered_by_details || undefined;

  const handleNotificationClick = async () => {
    if (!workspaceSlug || !notification?.id || isSnoozeStateModalOpen || customSnoozeModal) return;

    setPeekIssue(undefined);
    setCurrentSelectedNotificationId(notificationId);

    // make the notification as read
    if (notification.read_at === null) {
      try {
        await markNotificationAsRead(workspaceSlug);
      } catch (error) {
        console.error(error);
      }
    }

    if (scheduleRun) {
      router.push(scheduleRunNotificationHref(workspaceSlug, scheduleRun.schedule_id));
      return;
    }

    if (projectId && issueId && notification?.is_inbox_issue === false && !getIsIssuePeeked(issueId)) {
      setPeekIssue({ workspaceSlug, projectId, issueId });
    }
  };

  if (!workspaceSlug || !notificationId || !notification?.id || !workspace?.id) return <></>;
  if (!scheduleRun && (!notificationField || !projectId)) return <></>;

  return (
    <Row
      className={cn(
        "group relative flex cursor-pointer items-center gap-2 border-b border-subtle py-4 transition-all",
        {
          "bg-layer-1/30": currentSelectedNotificationId === notification?.id,
          "bg-accent-primary/5": notification.read_at === null,
        }
      )}
      onClick={handleNotificationClick}
    >
      {notification.read_at === null && (
        <div className="absolute top-[50%] left-2 h-1.5 w-1.5 flex-shrink-0 rounded-full bg-accent-primary" />
      )}

      <div className="relative flex w-full gap-2">
        <div className="relative flex h-12 w-12 flex-shrink-0 items-center justify-center rounded-full bg-layer-1">
          {notificationTriggeredBy ? (
            <Avatar
              alt={notificationTriggeredBy.display_name || notificationTriggeredBy?.first_name}
              fallback={(notificationTriggeredBy.display_name ||
                notificationTriggeredBy?.first_name)?.[0]?.toUpperCase()}
              src={getFileURL(notificationTriggeredBy.avatar_url)}
              size="xl"
            />
          ) : scheduleRun ? (
            <CalendarOutline className="h-5 w-5 text-tertiary" />
          ) : null}
        </div>

        <div className="-mt-2 w-full space-y-1">
          <div className="relative flex h-8 items-center gap-3">
            <div className="line-clamp-1 w-full truncate overflow-hidden text-body-xs-medium break-all whitespace-normal text-primary">
              {scheduleRun ? (
                <span className="font-medium text-primary">{scheduleRun.name}</span>
              ) : (
                <NotificationContent
                  notification={notification}
                  workspaceId={workspace.id}
                  workspaceSlug={workspaceSlug}
                  projectId={projectId}
                />
              )}
            </div>
            <NotificationOption
              workspaceSlug={workspaceSlug}
              notificationId={notification?.id}
              isSnoozeStateModalOpen={isSnoozeStateModalOpen}
              setIsSnoozeStateModalOpen={setIsSnoozeStateModalOpen}
              customSnoozeModal={customSnoozeModal}
              setCustomSnoozeModal={setCustomSnoozeModal}
            />
          </div>

          <div className="relative flex items-center gap-3 text-caption-sm-regular text-secondary">
            <div className="line-clamp-1 w-full truncate overflow-hidden break-words whitespace-normal">
              {scheduleRun ? (
                <span className="flex items-center gap-2">
                  <Badge variant={scheduleRun.status === "success" ? "success" : "danger"} size="sm">
                    {scheduleStatusLabel(scheduleRun.status)}
                  </Badge>
                  <span>{scheduleRunNotificationText(scheduleRun.status)}</span>
                  {scheduleRun.status === "failed" && scheduleRun.error && (
                    <span className="truncate text-danger-primary">{scheduleRun.error}</span>
                  )}
                </span>
              ) : (
                <>
                  {notification?.data?.issue?.identifier}-{notification?.data?.issue?.sequence_id}&nbsp;
                  {notification?.data?.issue?.name}
                </>
              )}
            </div>
            <div className="flex-shrink-0">
              {notification?.snoozed_till ? (
                <p className="flex flex-shrink-0 items-center justify-end gap-x-1 text-tertiary">
                  <ClockOutline className="h-4 w-4" />
                  <span>
                    Till {renderFormattedDate(notification.snoozed_till)},&nbsp;
                    {renderFormattedTime(notification.snoozed_till, "12-hour")}
                  </span>
                </p>
              ) : (
                <p className="mt-auto flex-shrink-0 text-tertiary">
                  {notification.created_at && calculateTimeAgo(notification.created_at)}
                </p>
              )}
            </div>
          </div>
        </div>
      </div>
    </Row>
  );
});
```

- [ ] **Step 3: Verifikasi tipe + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: tidak ada error baru (warning lama boleh).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx apps/web/core/components/workspace-notifications/sidebar/notification-card/content.tsx
git commit -m "feat(web): render schedule run notifications in the inbox card"
```

---

### Task 8: Web — list Scheduler dengan search/filter + item ringkas

**Files:**

- Modify: `apps/web/core/components/ai-scheduler/scheduler-view.tsx`
- Modify: `apps/web/core/components/ai-scheduler/schedule-item.tsx`

- [ ] **Step 1: Ganti `schedule-item.tsx`**

Ganti seluruh isi `apps/web/core/components/ai-scheduler/schedule-item.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { Badge } from "@plane/propel/badge";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// hooks
import { useMember } from "@/hooks/store/use-member";
// lib
import {
  RUN_STATUS_BADGE_VARIANTS,
  humanizeSchedule,
  scheduleDescription,
  scheduleStatusLabel,
  type TAiSchedule,
} from "@/lib/ai-schedule";

type Props = {
  schedule: TAiSchedule;
  onOpen: (scheduleId: string) => void;
};

export const ScheduleItem = observer(function ScheduleItem({ schedule, onOpen }: Props) {
  const {
    workspace: { getWorkspaceMemberDetails },
  } = useMember();
  const creator = getWorkspaceMemberDetails(schedule.created_by_id);

  return (
    <button
      type="button"
      className="w-full rounded-lg border border-subtle bg-layer-1 p-3 text-left transition-colors hover:bg-layer-2"
      onClick={() => onOpen(schedule.id)}
    >
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-medium break-words text-primary">{schedule.name}</p>
          {!schedule.enabled && (
            <Badge variant="neutral" size="sm">
              Paused
            </Badge>
          )}
          {schedule.last_status && (
            <Badge variant={RUN_STATUS_BADGE_VARIANTS[schedule.last_status]} size="sm">
              {scheduleStatusLabel(schedule.last_status)}
            </Badge>
          )}
        </div>
        <p className="mt-0.5 text-xs text-secondary">{humanizeSchedule(schedule)}</p>
        <p className="line-clamp-1 text-xs text-tertiary">{scheduleDescription(schedule)}</p>
        {schedule.enabled && schedule.next_run_at && (
          <p className="mt-0.5 text-xs text-tertiary">
            Next run: {renderFormattedDate(schedule.next_run_at)} at {renderFormattedTime(schedule.next_run_at)}
          </p>
        )}
        <p className="mt-0.5 text-xs text-tertiary">
          Created by {creator?.member?.display_name ?? "a workspace member"}
        </p>
      </div>
    </button>
  );
});
```

- [ ] **Step 2: Ganti `scheduler-view.tsx`**

Ganti seluruh isi `apps/web/core/components/ai-scheduler/scheduler-view.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { SearchOutline } from "@makeplane/propel/icons";
import { useParams, useRouter, useSearchParams } from "next/navigation";
// plane imports
import { Button } from "@plane/propel/button";
// hooks
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useQueryParams } from "@/hooks/use-query-params";
// lib
import { filterSchedules, type TAiScheduleListFilter } from "@/lib/ai-schedule";
// local imports
import { ScheduleDetailModal } from "./schedule-detail-modal";
import { ScheduleItem } from "./schedule-item";

const STATUS_FILTERS: { key: TAiScheduleListFilter["status"]; label: string }[] = [
  { key: "all", label: "All" },
  { key: "active", label: "Active" },
  { key: "paused", label: "Paused" },
];

export const SchedulerView = observer(function SchedulerView() {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const router = useRouter();
  const searchParams = useSearchParams();
  const { updateQueryParams } = useQueryParams();
  // store hooks
  const { schedules, runsBySchedule, loader, error, setWorkspace, fetchSchedules, fetchRuns } = useAiSchedules();
  // local state
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<TAiScheduleListFilter["status"]>("all");
  const [openScheduleId, setOpenScheduleId] = useState<string | null>(null);

  const requestedScheduleId = searchParams.get("schedule");
  const visibleSchedules = useMemo(() => filterSchedules(schedules, { query, status }), [schedules, query, status]);
  const openSchedule = schedules.find((schedule) => schedule.id === openScheduleId) ?? null;

  const expandedRunsRef = useRef(runsBySchedule);
  useEffect(() => {
    expandedRunsRef.current = runsBySchedule;
  }, [runsBySchedule]);

  const refreshSchedules = useCallback(() => {
    if (!slug) return;
    void fetchSchedules(slug).catch(() => {});
    for (const scheduleId of Object.keys(expandedRunsRef.current)) void fetchRuns(slug, scheduleId).catch(() => {});
  }, [slug, fetchSchedules, fetchRuns]);

  useEffect(() => {
    if (!slug) return;
    setWorkspace(slug);
    refreshSchedules();
    const interval = window.setInterval(refreshSchedules, 30_000);
    return () => window.clearInterval(interval);
  }, [slug, setWorkspace, refreshSchedules]);

  useEffect(() => {
    if (!requestedScheduleId) {
      setOpenScheduleId(null);
      return;
    }
    if (schedules.some((schedule) => schedule.id === requestedScheduleId)) setOpenScheduleId(requestedScheduleId);
  }, [requestedScheduleId, schedules]);

  const openScheduleModal = (scheduleId: string) => {
    setOpenScheduleId(scheduleId);
    router.replace(updateQueryParams({ paramsToAdd: { schedule: scheduleId } }));
  };

  const closeScheduleModal = () => {
    setOpenScheduleId(null);
    if (requestedScheduleId) router.replace(updateQueryParams({ paramsToRemove: ["schedule"] }));
  };

  const showErrorState = !!error && schedules.length === 0;

  return (
    <div className="mx-auto max-w-3xl space-y-3 px-4 py-6">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-lg font-semibold text-primary">AI Scheduler</h2>
        <Button size="sm" variant="secondary" loading={loader} onClick={refreshSchedules}>
          Refresh
        </Button>
      </div>
      {schedules.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-1.5 rounded-md border border-subtle bg-surface-1 px-2.5 py-1.5">
            <SearchOutline className="h-3.5 w-3.5 text-placeholder" />
            <input
              className="w-full max-w-[234px] border-none bg-transparent text-body-xs-regular outline-none placeholder:text-placeholder"
              placeholder="Search schedules..."
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          {STATUS_FILTERS.map((filter) => (
            <Button
              key={filter.key}
              size="sm"
              variant={status === filter.key ? "secondary" : "ghost"}
              onClick={() => setStatus(filter.key)}
            >
              {filter.label}
            </Button>
          ))}
          <span className="text-xs text-tertiary">
            {visibleSchedules.length} of {schedules.length}
          </span>
        </div>
      )}
      {loader && schedules.length === 0 ? (
        <p className="text-sm text-tertiary">Loading schedules…</p>
      ) : showErrorState ? (
        <div className="flex flex-col items-start gap-2">
          <p className="text-sm text-danger-primary">{error}</p>
          <Button size="sm" variant="secondary" onClick={refreshSchedules}>
            Retry
          </Button>
        </div>
      ) : schedules.length === 0 ? (
        <p className="text-sm text-tertiary">
          No schedules yet. Create one from the AI chat by typing <code>/schedule …</code>
        </p>
      ) : visibleSchedules.length === 0 ? (
        <p className="text-sm text-tertiary">No schedules match your search.</p>
      ) : (
        <div className="space-y-3">
          {error && <p className="text-xs text-danger-primary">Couldn't refresh — retrying automatically.</p>}
          {visibleSchedules.map((schedule) => (
            <ScheduleItem key={schedule.id} schedule={schedule} onOpen={openScheduleModal} />
          ))}
        </div>
      )}
      <ScheduleDetailModal schedule={openSchedule} isOpen={!!openSchedule} onClose={closeScheduleModal} />
    </div>
  );
});
```

Catatan: file `schedule-detail-modal.tsx` dibuat di Task 9; langkah ini belum bisa lulus typecheck sampai Task 9 selesai. Jangan commit Task 8 sendirian — gabungkan commit dengan Task 9 (lihat Task 9 Step 4).

---

### Task 9: Web — modal detail + deep link

**Files:**

- Create: `apps/web/core/components/ai-scheduler/schedule-detail-modal.tsx`
- Modify: `apps/web/core/components/ai-scheduler/schedule-runs-list.tsx`

- [ ] **Step 1: Buat `schedule-detail-modal.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Badge } from "@plane/propel/badge";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { EUserWorkspaceRoles } from "@plane/types";
import { AlertModalCore, ModalCore } from "@plane/ui";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// hooks
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useMember } from "@/hooks/store/use-member";
import { useUser, useUserPermissions } from "@/hooks/store/user";
// lib
import {
  RUN_STATUS_BADGE_VARIANTS,
  humanizeSchedule,
  scheduleDescription,
  scheduleStatusLabel,
  type TAiSchedule,
} from "@/lib/ai-schedule";
// local imports
import { ScheduleRunsList } from "./schedule-runs-list";

type Props = {
  schedule: TAiSchedule | null;
  isOpen: boolean;
  onClose: () => void;
};

export const ScheduleDetailModal = observer(function ScheduleDetailModal({ schedule, isOpen, onClose }: Props) {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  // store hooks
  const { toggleSchedule, deleteSchedule, runNow, fetchRuns, runsBySchedule } = useAiSchedules();
  const {
    workspace: { getWorkspaceMemberDetails },
  } = useMember();
  const { data: currentUser } = useUser();
  const { getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  // local state
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [running, setRunning] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [loadingRuns, setLoadingRuns] = useState(false);

  const scheduleId = schedule?.id;

  useEffect(() => {
    if (!isOpen || !slug || !scheduleId) return;
    setLoadingRuns(true);
    void fetchRuns(slug, scheduleId)
      .catch(() => {})
      .finally(() => setLoadingRuns(false));
  }, [isOpen, slug, scheduleId, fetchRuns]);

  if (!schedule) return null;

  const role = slug ? getWorkspaceRoleByWorkspaceSlug(slug) : undefined;
  const canManage = currentUser?.id === schedule.created_by_id || role === EUserWorkspaceRoles.ADMIN;
  const runs = runsBySchedule[schedule.id] ?? schedule.runs ?? [];
  const creator = getWorkspaceMemberDetails(schedule.created_by_id);
  const recipeSteps = schedule.spec?.how_to.map((step, index) => ({ id: `${index}-${step}`, step })) ?? [];

  const handleToggle = async (checked: boolean) => {
    if (!slug || toggling) return;
    setToggling(true);
    try {
      await toggleSchedule(slug, schedule.id, checked);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not update the schedule" });
    } finally {
      setToggling(false);
    }
  };

  const handleRunNow = async () => {
    if (!slug) return;
    setRunning(true);
    try {
      await runNow(slug, schedule.id);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not queue the run" });
    } finally {
      setRunning(false);
    }
  };

  const handleDelete = async () => {
    if (!slug) return;
    setDeleting(true);
    try {
      await deleteSchedule(slug, schedule.id);
      setDeleteOpen(false);
      onClose();
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not delete the schedule" });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <>
      <ModalCore isOpen={isOpen} handleClose={onClose}>
        <div className="max-h-[80vh] space-y-4 overflow-y-auto p-5">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-base font-semibold break-words text-primary">{schedule.name}</h3>
            {!schedule.enabled && (
              <Badge variant="neutral" size="sm">
                Paused
              </Badge>
            )}
            {schedule.last_status && (
              <Badge variant={RUN_STATUS_BADGE_VARIANTS[schedule.last_status]} size="sm">
                {scheduleStatusLabel(schedule.last_status)}
              </Badge>
            )}
          </div>
          <p className="text-xs text-secondary">{humanizeSchedule(schedule)}</p>
          <p className="text-xs text-tertiary">
            {schedule.enabled && schedule.next_run_at
              ? `Next run: ${renderFormattedDate(schedule.next_run_at)} at ${renderFormattedTime(schedule.next_run_at)}`
              : "Paused"}
          </p>
          <p className="text-xs text-tertiary">Created by {creator?.member?.display_name ?? "a workspace member"}</p>

          <div className="rounded-md border border-subtle bg-layer-2 p-3">
            <p className="text-xs font-medium text-secondary">Recipe</p>
            {schedule.spec ? (
              <>
                <p className="mt-1 text-xs text-tertiary">{schedule.spec.description}</p>
                <p className="mt-2 text-xs font-medium text-secondary">How to</p>
                <ol className="mt-1 list-decimal space-y-0.5 pl-4 text-xs text-tertiary">
                  {recipeSteps.map((entry) => (
                    <li key={entry.id}>{entry.step}</li>
                  ))}
                </ol>
                <p className="mt-2 text-xs font-medium text-secondary">Tools</p>
                <div className="mt-1 flex flex-wrap gap-1">
                  {schedule.spec.tools.map((tool) => (
                    <Badge key={tool} variant="neutral" size="sm">
                      {tool}
                    </Badge>
                  ))}
                </div>
                <p className="mt-2 text-xs font-medium text-secondary">Expected output</p>
                <p className="mt-1 text-xs text-tertiary">{schedule.spec.expected_output}</p>
              </>
            ) : (
              <>
                <p className="mt-2 text-xs font-medium text-secondary">Prompt (legacy schedule)</p>
                <p className="mt-1 text-xs whitespace-pre-wrap text-tertiary">{schedule.prompt}</p>
              </>
            )}
          </div>

          <div>
            <p className="text-xs font-medium text-secondary">History</p>
            {loadingRuns ? (
              <p className="mt-2 text-xs text-tertiary">Loading history…</p>
            ) : (
              <ScheduleRunsList runs={runs} />
            )}
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-subtle p-4">
          {canManage && (
            <Button
              size="sm"
              variant="secondary"
              disabled={toggling}
              onClick={() => void handleToggle(!schedule.enabled)}
            >
              {schedule.enabled ? "Pause" : "Resume"}
            </Button>
          )}
          {canManage && (
            <Button size="sm" variant="secondary" disabled={running} onClick={() => void handleRunNow()}>
              {running ? "Queueing…" : "Run now"}
            </Button>
          )}
          {canManage && (
            <Button size="sm" variant="error-outline" onClick={() => setDeleteOpen(true)}>
              Delete
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </div>
      </ModalCore>
      <AlertModalCore
        isOpen={deleteOpen}
        handleClose={() => setDeleteOpen(false)}
        handleSubmit={() => void handleDelete()}
        isSubmitting={deleting}
        title="Delete schedule"
        content={`"${schedule.name}" will stop running and be hidden. Existing runs are kept.`}
        primaryButtonText={{ loading: "Deleting", default: "Delete" }}
      />
    </>
  );
});
```

- [ ] **Step 2: Pakai varian badge bersama di `schedule-runs-list.tsx`**

Di `apps/web/core/components/ai-scheduler/schedule-runs-list.tsx`:

Hapus konstanta lokal:

```tsx
const STATUS_BADGE_VARIANTS: Record<TAiScheduleRun["status"], "success" | "danger" | "brand"> = {
  success: "success",
  failed: "danger",
  queued: "brand",
  running: "brand",
};
```

Ubah import lib menjadi:

```tsx
import {
  RUN_STATUS_BADGE_VARIANTS,
  runDurationInSeconds,
  scheduleStatusLabel,
  type TAiScheduleRun,
} from "@/lib/ai-schedule";
```

Dan ganti pemakaian `STATUS_BADGE_VARIANTS[run.status]` menjadi `RUN_STATUS_BADGE_VARIANTS[run.status]`.

- [ ] **Step 3: Verifikasi tipe, lint, dan test web**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web test`
Expected: typecheck bersih; lint 0 error; semua test web lulus.

- [ ] **Step 4: Commit Task 8 + Task 9 sekaligus**

```bash
git add apps/web/core/components/ai-scheduler/scheduler-view.tsx apps/web/core/components/ai-scheduler/schedule-item.tsx apps/web/core/components/ai-scheduler/schedule-detail-modal.tsx apps/web/core/components/ai-scheduler/schedule-runs-list.tsx
git commit -m "feat(web): searchable scheduler list with a detail modal and deep link"
```

---

### Task 10: Gate penuh, rebuild, smoke

**Files:**

- Tidak ada perubahan kode (kecuali perbaikan bila gate menemukan masalah)

- [ ] **Step 1: Gate Rust**

Run:

```bash
cd apps/api-rs && cargo fmt --check
cd apps/api-rs && cargo clippy -p ai -p worker -p api --all-targets -- -D warnings
cd apps/api-rs && cargo test -p ai
cd apps/api-rs && cargo test -p worker --test ai_schedule_test -- --test-threads=1
cd apps/api-rs && cargo test -p api --test ai_schedule_test -- --test-threads=1
cd apps/api-rs && cargo test -p api --test notification_test -- --test-threads=1
```

Expected: semua lulus. `cargo fmt --check` hanya memeriksa file yang kita ubah; jika ia mengeluh file lain, format file kita saja (`cargo fmt -- crates/worker/src/handlers/ai_schedule.rs crates/worker/tests/ai_schedule_test.rs crates/api/src/routes/notification.rs crates/api/src/routes/ai_schedule.rs crates/api/tests/notification_test.rs crates/api/tests/ai_schedule_test.rs`).

- [ ] **Step 2: Gate web**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web test`
Expected: lulus.

- [ ] **Step 3: Rebuild backend (detached) + verifikasi**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll `docker compose -f docker-compose-local.yml ps` sampai semua `Up`, lalu:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8000/health   # 200
systemctl --user restart plane-live.service && sleep 8 && curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3100/live/health/  # 200
```

- [ ] **Step 4: Rebuild web prod + restart**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/   # 200
```

- [ ] **Step 5: Smoke manual di tunnel**

1. Buat jadwal baru via chat `/schedule …` → Confirm.
2. `Run now` (manual) → tidak ada notifikasi baru di Inbox.
3. Paksa run terjadwal: tunggu tick berikutnya, atau di DB set `next_run_at = now()` untuk jadwal uji lalu tunggu ≤1 menit.
4. Inbox → notifikasi "Daily digest" muncul dengan badge Success/Failed; klik → halaman Scheduler terbuka + modal jadwal terbuka (deep link `?schedule=<id>`).
5. Di modal: Recipe tampil, History tampil, Pause/Resume, Run now, Delete bekerja; non-creator melihat modal read-only (tanpa tombol aksi).
6. Legacy schedule (tanpa spec) → modal menampilkan "Prompt (legacy schedule)".
7. Tutup modal → param `?schedule=` hilang dari URL.
8. Hapus schedule uji → notifikasinya hilang dari Inbox (ter-soft-delete).

- [ ] **Step 6: Commit perbaikan bila ada**

Jika smoke menemukan bug, perbaiki, ulangi gate yang relevan, lalu commit dengan pesan `fix(...)` yang spesifik (hanya file terkait).

---

## Catatan penutup

- Task 8 dan Task 9 harus di-commit bersamaan karena `scheduler-view.tsx` mengimpor modal yang dibuat di Task 9.
- Setelah semua task selesai dan smoke lulus, lanjutkan dengan skill `superpowers:finishing-a-development-branch`.
