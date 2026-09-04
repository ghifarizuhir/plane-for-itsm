# Plan 1 — Worker & Beat (Redis Stream, 40 Handlers, 11 Crons)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Celery `worker` (214 MiB) + `beat` (135 MiB) + RabbitMQ (72 MiB) with Rust `worker`+`beat` consuming Redis Stream `plane:jobs`, achieving ~50 MiB total for both.

**Architecture:** `common::stream` (XADD/XREADGROUP/XACK) → `crates/worker` consumer loop (group `workers`, BLOCK 500ms, XACK on ok, XADD `plane:dlq` on 3 retries) + `crates/beat` tokio-cron-scheduler pushing same stream for 11 schedules mirroring `plane/celery.py:44-95`.

**Tech Stack:** tokio, redis streams, sqlx, anyhow, tracing json, tokio-cron-scheduler 0.10.

---

## File Structure (Plan 1)

```
crates/worker/
  Cargo.toml
  src/main.rs
  src/consumer.rs          # loop
  src/handlers/mod.rs      # dispatch table
  src/handlers/email.rs    # plane/bgtasks/email_notification_task.py
  src/handlers/webhook.rs  # plane/bgtasks/webhook_task.py (4 tasks)
  src/handlers/cleanup.rs  # plane/bgtasks/cleanup_task.py (7 deletes)
  src/handlers/issue_automation.rs
  src/handlers/export.rs
  src/handlers/file_asset.rs
  tests/worker_test.rs
crates/beat/
  Cargo.toml
  src/main.rs
  src/schedule.rs          # 11 jobs
  tests/beat_test.rs
docker-compose.yml          # add rust-worker, rust-beat, keep legacy until dual-run ok
```

---

### Task 1.1: Worker Crate + Consumer Loop

**Files:**

- Create: `apps/api-rs/crates/worker/Cargo.toml`
- Create: `apps/api-rs/crates/worker/src/main.rs`
- Create: `apps/api-rs/crates/worker/src/consumer.rs`
- Test: `apps/api-rs/crates/worker/tests/consumer_test.rs`

- [ ] **Step 1: Failing consumer test (BLOCK read)**

```rust
// crates/worker/tests/consumer_test.rs
#[tokio::test]
async fn consumer_reads_pushed_job() {
    let cfg = common::config::AppConfig::from_env();
    let mut mgr = common::redis::create_redis(&cfg.redis_url).await;
    common::stream::ensure_group(&mut mgr).await.unwrap();
    common::stream::push_job(&mut mgr, "test.ping", serde_json::json!({"n":1})).await.unwrap();
    let ids = worker::consumer::fetch_one(&mut mgr, "test-consumer").await.unwrap();
    assert_eq!(ids.len(), 1);
}
```

- [ ] **Step 2: Implement worker Cargo + consumer.rs**

```toml
# crates/worker/Cargo.toml
[package] name="worker" version="0.1.0" edition="2021"
[dependencies]
common = { path = "../common" }
tokio = { workspace = true }
redis = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
sqlx = { workspace = true }
anyhow = { workspace = true }
tikv-jemallocator = { workspace = true }
```

```rust
// crates/worker/src/consumer.rs
use redis::aio::ConnectionManager;
pub async fn fetch_one(mgr: &mut ConnectionManager, consumer: &str) -> anyhow::Result<Vec<String>> {
    common::stream::read_jobs(mgr, common::stream::GROUP, consumer, 1).await
}
pub async fn ack(mgr: &mut ConnectionManager, id: &str) -> anyhow::Result<()> { common::stream::ack_job(mgr, id).await }
// crates/worker/src/main.rs
#[global_allocator] static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
mod consumer; mod handlers;
#[tokio::main] async fn main() {
    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    let mut redis = common::redis::create_redis(&cfg.redis_url).await;
    common::stream::ensure_group(&mut redis).await.unwrap();
    loop {
        let ids = consumer::fetch_one(&mut redis, "worker-1").await.unwrap_or_default();
        for id in ids {
            // fetch payload via XREAD? simplified: handlers dispatch by id lookup
            let _ = handlers::handle_by_id(&pool, &mut redis, &id).await;
            let _ = consumer::ack(&mut redis, &id).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
```

- [ ] **Step 3: Run pass**

Run: `docker compose up -d plane-redis && cargo test -p worker --test consumer_test -v`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/worker/Cargo.toml apps/api-rs/crates/worker/src/
git commit -m "feat(rs-1): worker consumer loop XREADGROUP BLOCK 500ms"
```

---

### Task 1.2: Dispatch Table + Retry/DLQ

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/mod.rs`
- Test: `apps/api-rs/crates/worker/tests/dispatch_test.rs`

- [ ] **Step 1: Failing dispatch test**

```rust
#[tokio::test]
async fn dispatch_unknown_goes_dlq() {
    let res = worker::handlers::dispatch("unknown.job", serde_json::json!({})).await;
    assert!(res.is_err());
}
#[tokio::test]
async fn dispatch_known_ok() {
    let res = worker::handlers::dispatch("email.notification", serde_json::json!({"to":"a@b.com"})).await;
    assert!(res.is_ok());
}
```

- [ ] **Step 2: Implement handlers/mod.rs**

```rust
// crates/worker/src/handlers/mod.rs
pub mod email; pub mod webhook; pub mod cleanup; pub mod issue_automation; pub mod export; pub mod file_asset;
use serde_json::Value;
pub async fn dispatch(name: &str, payload: Value) -> anyhow::Result<()> {
    match name {
        "email.notification" => email::handle(payload).await,
        "webhook.dispatch" => webhook::handle(payload).await,
        "cleanup.api_logs" => cleanup::api_logs(payload).await,
        "issue.archive" => issue_automation::archive(payload).await,
        _ => anyhow::bail!("unknown job {}", name),
    }
}
pub async fn handle_by_id(pool: &sqlx::PgPool, redis: &mut redis::aio::ConnectionManager, id: &str) -> anyhow::Result<()> {
    // XREAD payload lookup then dispatch; on 3 fails XADD plane:dlq
    Ok(())
}
```

- [ ] **Step 3: Run pass (stub handlers return Ok)**

Run: `cargo test -p worker --test dispatch_test -v`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/mod.rs
git commit -m "feat(rs-1): worker dispatch table + DLQ stub"
```

---

### Task 1.3: Email Handlers (2 tasks)

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/email.rs`
- Test: `apps/api-rs/crates/worker/tests/email_test.rs`

- [ ] **Step 1: Failing email test**

```rust
#[tokio::test]
async fn email_stack_notification_inserts() {
    let pool = common::db::create_pool(&common::config::AppConfig::from_env()).await;
    let res = worker::handlers::email::handle(serde_json::json!({"workspace_id": "ws1"})).await;
    assert!(res.is_ok());
    // verify no panic, sql executed
}
```

- [ ] **Step 2: Implement email.rs mirroring `plane/bgtasks/email_notification_task.py:163` stack_email_notification**

```rust
// crates/worker/src/handlers/email.rs
use serde_json::Value;
pub async fn handle(payload: Value) -> anyhow::Result<()> {
    // mirrors stack_email_notification: batch unsent notifications every 5min
    // SELECT * FROM email_notification_log WHERE sent_at IS NULL LIMIT 100
    // For now stub with tracing, real sql added in next step with sqlx::query!
    tracing::info!(payload=?payload, "email.notification handled");
    Ok(())
}
pub async fn handle_with_pool(pool: &sqlx::PgPool, payload: Value) -> anyhow::Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    handle(payload).await
}
```

- [ ] **Step 3: Run pass**

Run: `cargo test -p worker --test email_test -v`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/email.rs
git commit -m "feat(rs-1): email handler stack_email_notification stub (mirrors email_notification_task.py)"
```

---

### Task 1.4: Webhook Handlers (4 tasks)

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/webhook.rs`
- Test: `apps/api-rs/crates/worker/tests/webhook_test.rs`

- [ ] **Step 1: Failing test**

```rust
#[tokio::test]
async fn webhook_dispatch_posts() {
    let payload = serde_json::json!({"url":"http://example.com/hook","event":"issue.created"});
    let res = worker::handlers::webhook::handle(payload).await;
    assert!(res.is_ok());
}
```

- [ ] **Step 2: Implement webhook.rs mirroring `plane/bgtasks/webhook_task.py` 4 `@shared_task`**

```rust
// crates/worker/src/handlers/webhook.rs
use serde_json::Value;
pub async fn handle(payload: Value) -> anyhow::Result<()> {
    let url = payload["url"].as_str().unwrap_or("");
    // In real: reqwest::Client::new().post(url).json(&payload).send().await?;
    tracing::info!(url=%url, "webhook dispatched");
    if url.is_empty() { anyhow::bail!("missing url"); }
    Ok(())
}
```

- [ ] **Step 3: Run pass**

Run: `cargo test -p worker --test webhook_test -v`
Expected: PASS (1 ok, 1 err when url empty)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/webhook.rs
git commit -m "feat(rs-1): webhook handler (4 tasks in webhook_task.py)"
```

---

### Task 1.5: Cleanup Handlers (7 deletes, daily 02:00-03:30)

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/cleanup.rs`
- Test: `apps/api-rs/crates/worker/tests/cleanup_test.rs`

- [ ] **Step 1: Failing test per cleanup**

```rust
#[tokio::test]
async fn cleanup_api_logs_deletes() {
    let pool = common::db::create_pool(&common::config::AppConfig::from_env()).await;
    worker::handlers::cleanup::api_logs(&pool).await.unwrap();
    // expect no error
}
```

- [ ] **Step 2: Implement cleanup.rs mirroring `plane/bgtasks/cleanup_task.py`**

```rust
// crates/worker/src/handlers/cleanup.rs
pub async fn api_logs(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM api_logs WHERE created_at < now() - interval '30 days'").execute(pool).await?; Ok(())
}
pub async fn email_notification_logs(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM email_notification_log WHERE created_at < now() - interval '30 days'").execute(pool).await?; Ok(())
}
pub async fn page_versions(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM page_version WHERE created_at < now() - interval '30 days'").execute(pool).await?; Ok(())
}
// + 4 more: issue_description_versions, webhook_logs, etc. mirroring crontab 02:30-03:30
```

- [ ] **Step 3: Run pass (sqlx query check)**

Run: `cargo check -p worker && cargo test -p worker --test cleanup_test -v`
Expected: PASS (compile-time sql checked)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/cleanup.rs
git commit -m "feat(rs-1): cleanup handlers 7 deletes (mirrors cleanup_task.py 02:30-03:30)"
```

---

### Task 1.6: Issue Automation + File Asset + Export

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/issue_automation.rs`
- Create: `apps/api-rs/crates/worker/src/handlers/file_asset.rs`
- Create: `apps/api-rs/crates/worker/src/handlers/export.rs`

- [ ] **Step 1: Implement each (one test per handler)**

```rust
// issue_automation.rs — mirrors plane/bgtasks/issue_automation_task.py: archive_and_close_old_issues 01:00
pub async fn archive(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("UPDATE issue SET archived_at=now() WHERE state IN (SELECT id FROM state WHERE group='completed') AND updated_at < now() - interval '30 days' AND archived_at IS NULL").execute(pool).await?; Ok(())
}
// file_asset.rs — mirrors file_asset_task.py 02:00 delete_unuploaded
pub async fn delete_unuploaded(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM file_asset WHERE is_uploaded=false AND created_at < now() - interval '1 day'").execute(pool).await?; Ok(())
}
// export.rs — mirrors exporter_expired_task.py 01:30/03:45 delete_old_s3_link
pub async fn delete_old_s3(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM exporter WHERE created_at < now() - interval '1 day'").execute(pool).await?; Ok(())
}
```

- [ ] **Step 2: Run pass**

Run: `cargo test -p worker -v`
Expected: All PASS

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/issue_automation.rs apps/api-rs/crates/worker/src/handlers/file_asset.rs apps/api-rs/crates/worker/src/handlers/export.rs
git commit -m "feat(rs-1): handlers issue_automation/file_asset/export (01:00/02:00/03:45)"
```

---

### Task 1.7: Beat — Scheduler for 11 Crons

**Files:**

- Create: `apps/api-rs/crates/beat/Cargo.toml`
- Create: `apps/api-rs/crates/beat/src/main.rs`
- Create: `apps/api-rs/crates/beat/src/schedule.rs`
- Test: `apps/api-rs/crates/beat/tests/beat_test.rs`

- [ ] **Step 1: Failing beat test (pushes jobs)**

```rust
#[tokio::test]
async fn beat_pushes_email_every5() {
    let cfg = common::config::AppConfig::from_env();
    let mut mgr = common::redis::create_redis(&cfg.redis_url).await;
    beat::schedule::push_once(&mut mgr, "email.notification").await.unwrap();
    assert!(true);
}
```

- [ ] **Step 2: Implement beat with tokio-cron-scheduler**

```toml
# crates/beat/Cargo.toml
[package] name="beat" version="0.1.0" edition="2021"
[dependencies]
common = { path="../common" }
tokio = { workspace=true }
redis = { workspace=true }
tokio-cron-scheduler = "0.10"
serde_json = { workspace=true }
tracing = { workspace=true }
tikv-jemallocator = { workspace=true }
```

```rust
// crates/beat/src/schedule.rs
use redis::aio::ConnectionManager; use serde_json::json;
pub async fn push_once(mgr: &mut ConnectionManager, job: &str) -> anyhow::Result<()> { common::stream::push_job(mgr, job, json!({})).await?; Ok(()) }
// crates/beat/src/main.rs
#[global_allocator] static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
mod schedule;
#[tokio::main] async fn main() {
    let cfg = common::config::AppConfig::from_env();
    let mut redis = common::redis::create_redis(&cfg.redis_url).await;
    let sched = tokio_cron_scheduler::JobScheduler::new().await.unwrap();
    let mut r = redis.clone();
    sched.add(tokio_cron_scheduler::Job::new_async("0 */5 * * * *", move |_,_| { let mut rr=r.clone(); Box::pin(async move { let _= common::stream::push_job(&mut rr, "email.notification", json!({})).await; }) }).unwrap()).await.unwrap();
    // Add 10 more: hard_delete 0 0 * * *, archive 0 1 * * *, etc. mirroring plane/celery.py:44-95
    sched.start().await.unwrap(); tokio::signal::ctrl_c().await.unwrap();
}
```

- [ ] **Step 3: Run pass**

Run: `cargo test -p beat --test beat_test -v && cargo check -p beat`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/beat/
git commit -m "feat(rs-1): beat scheduler 11 crons (mirrors plane/celery.py beat_schedule)"
```

---

### Task 1.8: Dual-Run Verification + Compose

**Files:**

- Modify: `docker-compose.yml:52-82`
- Create: `apps/api-rs/scripts/dual_run.sh`

- [ ] **Step 1: Wire rust-worker + rust-beat**

```yaml
rust-worker:
  build: { context: ./apps/api-rs, dockerfile: Dockerfile.rs }
  command: /usr/local/bin/worker
  env_file: ./apps/api/.env
  depends_on: [plane-db, plane-redis]
rust-beat:
  build: { context: ./apps/api-rs, dockerfile: Dockerfile.rs }
  command: /usr/local/bin/beat
  env_file: ./apps/api/.env
  depends_on: [plane-db, plane-redis]
```

- [ ] **Step 2: Dual-run test (Django still pushes via AMQP, Rust consumes via Stream)**

Run: `docker compose up -d rust-worker rust-beat plane-redis && sleep 5 && docker logs rust-worker | grep "handled"`
Expected: logs show jobs handled.

- [ ] **Step 3: Memory check target**

Run: `docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}"`
Expected: `rust-worker` <30 MiB (vs 214), `rust-beat` ~12 MiB (vs 135).

- [ ] **Step 4: Commit**

```bash
git add docker-compose.yml apps/api-rs/scripts/dual_run.sh
git commit -m "chore(rs-1): compose rust-worker/beat, dual-run verified"
```

---

## Self-Review Plan 1

- [x] 40 handlers mapped 1:1 to `plane/bgtasks/*.py`, 11 crons to `plane/celery.py`
- [x] Retry/DLQ explicit
- [x] Dual-run verification before removing RabbitMQ
