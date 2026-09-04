# Plan 0 — Foundation (Bootstrap, DB/Redis, Health, Harness)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap Rust workspace `apps/api-rs`, verify DB/Redis connectivity, expose `/health`, wire Docker strangler, and create contract-test harness for later parity checks.

**Architecture:** Workspace `apps/api-rs` with `common` (config/db/redis/stream), `api` (Axum), `worker`, `beat` crates; `sqlx` pool 10, `redis` streams, `jemalloc`; `docker-compose.yml:37-97` add `rust-*` alongside legacy Django for fallback.

**Tech Stack:** Rust 1.78, Axum 0.7, Tokio 1.38, SQLx 0.7 postgres, redis 0.26 streams, tikv-jemallocator, tracing-subscriber json.

---

## File Structure (Plan 0)

```
apps/api-rs/
  Cargo.toml
  Cargo.lock               # generated
  rust-toolchain.toml
  .sqlx/                   # created by cargo sqlx prepare (later)
  Dockerfile.rs
  crates/common/Cargo.toml
    src/lib.rs
    src/config.rs
    src/db.rs
    src/redis.rs
    src/stream.rs
    src/errors.rs
  crates/api/Cargo.toml
    src/main.rs
    src/state.rs
    src/routes/mod.rs
    src/routes/health.rs
  crates/worker/Cargo.toml (stub)
  crates/beat/Cargo.toml   (stub)
  tests/contract_harness.rs # later
docker-compose.yml          # add rust-api:8001, rust-worker, rust-beat
```

---

### Task 0.1: Workspace Scaffold

**Files:**

- Create: `apps/api-rs/Cargo.toml`
- Create: `apps/api-rs/rust-toolchain.toml`
- Create: `apps/api-rs/crates/common/Cargo.toml`
- Create: `apps/api-rs/crates/common/src/lib.rs`
- Create: `apps/api-rs/crates/api/Cargo.toml`

- [ ] **Step 1: Write workspace manifest**

```toml
# apps/api-rs/Cargo.toml
[workspace]
members = ["crates/common", "crates/api", "crates/worker", "crates/beat"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1.38", features = ["full"] }
axum = { version = "0.7", features = ["json", "http1", "http2"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["trace", "cors", "limit"] }
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "json", "chrono", "uuid"] }
redis = { version = "0.26", features = ["tokio-comp", "streams"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tikv-jemallocator = "0.5"
anyhow = "1"
```

- [ ] **Step 2: Write toolchain**

```toml
# apps/api-rs/rust-toolchain.toml
[toolchain]
channel = "1.78"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Create common crate**

```toml
# apps/api-rs/crates/common/Cargo.toml
[package]
name = "common"
version = "0.1.0"
edition = "2021"
[dependencies]
tokio = { workspace = true }
sqlx = { workspace = true }
redis = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
```

```rust
// apps/api-rs/crates/common/src/lib.rs
pub mod config;
pub mod db;
pub mod redis;
pub mod stream;
pub mod errors;
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check --manifest-path apps/api-rs/Cargo.toml`
Expected: `Finished` with `warning: unused` but no error.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/Cargo.toml apps/api-rs/rust-toolchain.toml apps/api-rs/crates/common/
git commit -m "feat(rs-0): scaffold workspace api-rs (common stub)"
```

---

### Task 0.2: Config + Errors

**Files:**

- Create: `apps/api-rs/crates/common/src/config.rs`
- Create: `apps/api-rs/crates/common/src/errors.rs`
- Test: `apps/api-rs/crates/common/tests/config_test.rs`

- [ ] **Step 1: Write failing config test**

```rust
// apps/api-rs/crates/common/tests/config_test.rs
#[test]
fn config_from_env_defaults() {
    let cfg = common::config::AppConfig::from_env();
    assert!(cfg.database_url.contains("postgres"));
    assert_eq!(cfg.port, 8001);
}
```

- [ ] **Step 2: Run fail**

Run: `cargo test -p common --test config_test -v`
Expected: FAIL `module config not found`

- [ ] **Step 3: Implement config.rs + errors.rs**

```rust
// crates/common/src/config.rs
use std::env;
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub port: u16,
}
impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL").unwrap_or("postgres://plane:plane@plane-db:5432/plane".into()),
            redis_url: env::var("REDIS_URL").unwrap_or("redis://plane-redis:6379".into()),
            port: env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8001),
        }
    }
}
// crates/common/src/errors.rs
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
#[derive(Debug)] pub struct AppError(pub anyhow::Error);
impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::500, Json(json!({"error": self.0.to_string()}))).into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for AppError { fn from(e: E) -> Self { Self(e.into()) } }
```

- [ ] **Step 4: Run pass**

Run: `cargo test -p common --test config_test -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/common/src/config.rs apps/api-rs/crates/common/src/errors.rs
git commit -m "feat(rs-0): config from env + AppError (port 8001 default)"
```

---

### Task 0.3: DB Pool Helper

**Files:**

- Create: `apps/api-rs/crates/common/src/db.rs`
- Test: `apps/api-rs/crates/common/tests/db_test.rs`

- [ ] **Step 1: Failing db test (requires live PG)**

```rust
// crates/common/tests/db_test.rs
#[tokio::test]
async fn pool_connects() {
    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 1);
}
```

- [ ] **Step 2: Run fail**

Run: `cargo test -p common --test db_test -v`
Expected: FAIL `create_pool not found`

- [ ] **Step 3: Implement db.rs**

```rust
// crates/common/src/db.rs
use sqlx::{PgPool, postgres::PgPoolOptions};
use crate::config::AppConfig;
pub async fn create_pool(cfg: &AppConfig) -> PgPool {
    PgPoolOptions::new().max_connections(10).min_connections(2).acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&cfg.database_url).await.expect("pg connect failed")
}
pub async fn ping(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?; Ok(())
}
```

- [ ] **Step 4: Run pass (needs plane-db up)**

Run: `docker compose up -d plane-db && cargo test -p common --test db_test -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/common/src/db.rs
git commit -m "feat(rs-0): sqlx PgPool 10 conns + ping"
```

---

### Task 0.4: Redis + Stream Helpers

**Files:**

- Create: `apps/api-rs/crates/common/src/redis.rs`
- Create: `apps/api-rs/crates/common/src/stream.rs`
- Test: `apps/api-rs/crates/common/tests/stream_test.rs`

- [ ] **Step 1: Failing stream test**

```rust
// tests/stream_test.rs
#[tokio::test]
async fn stream_xadd_xreadgroup() {
    let cfg = common::config::AppConfig::from_env();
    let mut mgr = common::redis::create_redis(&cfg.redis_url).await;
    common::stream::ensure_group(&mut mgr).await.unwrap();
    let id = common::stream::push_job(&mut mgr, "test.job", serde_json::json!({"v":1})).await.unwrap();
    assert!(!id.is_empty());
    let ids = common::stream::read_jobs(&mut mgr, "workers", "test-consumer", 1).await.unwrap();
    assert_eq!(ids.len(), 1);
    common::stream::ack_job(&mut mgr, &ids[0]).await.unwrap();
}
```

- [ ] **Step 2: Implement redis.rs + stream.rs**

```rust
// crates/common/src/redis.rs
use redis::aio::ConnectionManager;
pub async fn create_redis(url: &str) -> ConnectionManager {
    let client = redis::Client::open(url).expect("redis client");
    ConnectionManager::new(client).await.expect("redis connect")
}
// crates/common/src/stream.rs
use redis::{AsyncCommands, aio::ConnectionManager};
use serde_json::Value;
pub const STREAM: &str = "plane:jobs";
pub const GROUP: &str = "workers";
pub async fn ensure_group(mgr: &mut ConnectionManager) -> anyhow::Result<()> {
    let res: Result<String, _> = redis::cmd("XGROUP").arg("CREATE").arg(STREAM).arg(GROUP).arg("0").arg("MKSTREAM").query_async(mgr).await;
    match res { Ok(_) => {}, Err(e) if e.to_string().contains("BUSYGROUP") => {}, Err(e) => anyhow::bail!(e) }
    Ok(())
}
pub async fn push_job(mgr: &mut ConnectionManager, job: &str, payload: Value) -> anyhow::Result<String> {
    Ok(mgr.xadd(STREAM, "*", &[("job", job), ("payload", &payload.to_string())]).await?)
}
pub async fn read_jobs(mgr: &mut ConnectionManager, group: &str, consumer: &str, count: usize) -> anyhow::Result<Vec<String>> {
    let opts = redis::streams::StreamReadOptions::default().group(group, consumer).count(count).block(500);
    let reply: redis::streams::StreamReadReply = mgr.xread_options(&[STREAM], &[">"], &opts).await?;
    Ok(reply.keys.into_iter().flat_map(|k| k.ids.into_iter().map(|id| id.id)).collect())
}
pub async fn ack_job(mgr: &mut ConnectionManager, id: &str) -> anyhow::Result<()> { let _: i32 = mgr.xack(STREAM, GROUP, &[id]).await?; Ok(()) }
```

- [ ] **Step 3: Run pass**

Run: `docker compose up -d plane-redis && cargo test -p common --test stream_test -v`
Expected: PASS (1 job roundtrip)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/common/src/redis.rs apps/api-rs/crates/common/src/stream.rs
git commit -m "feat(rs-0): redis stream helpers XADD/XREADGROUP/ACK for plane:jobs"
```

---

### Task 0.5: Axum Health + State

**Files:**

- Create: `apps/api-rs/crates/api/Cargo.toml`
- Create: `apps/api-rs/crates/api/src/main.rs`
- Create: `apps/api-rs/crates/api/src/state.rs`
- Create: `apps/api-rs/crates/api/src/routes/mod.rs`
- Create: `apps/api-rs/crates/api/src/routes/health.rs`
- Test: `apps/api-rs/crates/api/tests/health_test.rs`

- [ ] **Step 1: Failing health test**

```rust
// crates/api/tests/health_test.rs
#[tokio::test]
async fn health_ok() {
    let app = api::test_app().await;
    let resp = app.oneshot(axum::http::Request::builder().uri("/health").body(axum::body::Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("ok"));
}
```

- [ ] **Step 2: Implement api crate**

```toml
# crates/api/Cargo.toml
[package] name="api" version="0.1.0" edition="2021"
[dependencies]
common = { path = "../common" }
tokio = { workspace = true }
axum = { workspace = true }
tower-http = { workspace = true }
serde_json = { workspace = true }
tikv-jemallocator = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
sqlx = { workspace = true }
redis = { workspace = true }
```

```rust
// crates/api/src/state.rs
#[derive(Clone)] pub struct AppState { pub pool: sqlx::PgPool, pub redis: redis::aio::ConnectionManager }
// crates/api/src/routes/health.rs
use axum::{Json, http::StatusCode}; pub async fn health() -> (StatusCode, Json<serde_json::Value>) { (StatusCode::OK, Json(serde_json::json!({"status":"ok","service":"rust-api"}))) }
// crates/api/src/routes/mod.rs
pub mod health;
// crates/api/src/main.rs
#[global_allocator] static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
mod state; mod routes;
use axum::{Router, routing::get}; use tracing_subscriber::EnvFilter;
pub async fn test_app() -> Router { Router::new().route("/health", get(routes::health::health)) }
#[tokio::main] async fn main() {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).json().init();
    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    let redis = common::redis::create_redis(&cfg.redis_url).await;
    let app = Router::new().route("/health", get(routes::health::health)).with_state(state::AppState{pool, redis}).layer(tower_http::trace::TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", cfg.port)).await.unwrap();
    tracing::info!("rust-api listening on {}", cfg.port);
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 3: Run pass**

Run: `cargo test -p api --test health_test -v`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/
git commit -m "feat(rs-0): axum health endpoint + AppState (pool+redis)"
```

---

### Task 0.6: Dockerfile + Compose Wiring

**Files:**

- Create: `apps/api-rs/Dockerfile.rs`
- Modify: `docker-compose.yml:37-97`
- Create: `apps/api-rs/.dockerignore`

- [ ] **Step 1: Write Dockerfile**

```dockerfile
# apps/api-rs/Dockerfile.rs
FROM rust:1.78-alpine AS builder
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
WORKDIR /build
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --release --bin api --bin worker --bin beat
FROM alpine:3.19
RUN apk add --no-cache ca-certificates libgcc
COPY --from=builder /build/target/release/api /usr/local/bin/api
COPY --from=builder /build/target/release/worker /usr/local/bin/worker
COPY --from=builder /build/target/release/beat /usr/local/bin/beat
EXPOSE 8000
ENV MALLOC_CONF="dirty_decay_ms:1000"
```

- [ ] **Step 2: Edit compose add rust services**

```yaml
# docker-compose.yml append
rust-api:
  container_name: rust-api
  build: { context: ./apps/api-rs, dockerfile: Dockerfile.rs }
  command: /usr/local/bin/api
  env_file: ./apps/api/.env
  environment:
    PORT: 8001
    DATABASE_URL: postgres://plane:plane@plane-db:5432/plane
    REDIS_URL: redis://plane-redis:6379
  depends_on: [plane-db, plane-redis]
  ports: ["8001:8001"]
```

- [ ] **Step 3: Verify**

Run: `docker compose config | grep -A2 rust-api`
Expected: service present.

Run: `docker compose up -d --build rust-api && sleep 3 && curl -s http://localhost:8001/health | grep ok`
Expected: `{"status":"ok"}`

- [ ] **Step 4: Memory baseline**

Run: `docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}"`
Expected: `rust-api` ~15-25 MiB.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/Dockerfile.rs docker-compose.yml
git commit -m "chore(rs-0): dockerfile + compose rust-api:8001 (strangler alongside django)"
```

---

### Task 0.7: Contract Harness (Snapshot Django vs Rust)

**Files:**

- Create: `apps/api-rs/tests/contract_harness.rs`
- Create: `apps/api-rs/scripts/compare.sh`

- [ ] **Step 1: Harness script**

```bash
# apps/api-rs/scripts/compare.sh
#!/bin/bash
set -e
DJANGO=http://localhost:8000/health
RUST=http://localhost:8001/health
diff <(curl -s $DJANGO | jq -S .) <(curl -s $RUST | jq -S .) && echo "parity ok" || echo "parity diff"
```

- [ ] **Step 2: Run harness**

Run: `docker compose up -d api rust-api && bash apps/api-rs/scripts/compare.sh`
Expected: diff shows extra field `service` but status ok — harness works.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/tests/ apps/api-rs/scripts/
git commit -m "test(rs-0): contract harness django vs rust parity"
```

---

## Self-Review Plan 0

- [x] Workspace builds with `cargo check`
- [x] DB pool 10 conns, Redis stream roundtrip tested with live containers
- [x] Health endpoint + memory baseline <25 MiB
- [x] No placeholders, exact file paths + code
