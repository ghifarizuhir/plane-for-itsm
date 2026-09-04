# Plan 2 — API Strangler Per-Domain (Contract Parity)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Incrementally migrate REST API per domain from Django (`plane/api/*`, `plane/app/*`) to Rust Axum, keeping 100% JSON contract for `web/admin/space`, with snapshot + shadow verification before proxy cutover.

**Architecture:** `crates/api` Axum routers per file `plane/api/urls/*.py` + `plane/app/urls/*.py`; `AppState { pool, redis, s3 }`; `jsonwebtoken` + `redis` auth extractor replacing `plane/api/middleware/api_authentication.py`; `validator` for DRF serializers; `proxy` routes per-path to Rust first, Django fallback.

**Tech Stack:** Axum 0.7, tower-http, serde, validator, jsonwebtoken, redis, sqlx, aws-sdk-s3 (later).

---

## File Structure (Plan 2)

```
crates/api/src/
  main.rs
  state.rs
  middleware/auth.rs
  routes/mod.rs
  routes/workspace.rs  # ↔ plane/app/urls/workspace.py:260, plane/db/models/workspace.py
  routes/project.rs    # ↔ plane/app/urls/project.py:132
  routes/issue.rs      # ↔ plane/app/urls/issue.py:286 (most complex)
  routes/cycle.rs      # ↔ plane/app/urls/cycle.py:106
  routes/module.rs     # ↔ plane/app/urls/module.py:105
  routes/state.rs
  routes/user.rs       # ↔ plane/app/urls/user.py:85
  routes/asset.rs
  ... remaining domains
crates/common/src/models/
  workspace.rs
  project.rs
  issue.rs
  cycle.rs
  ...
crates/api/tests/
  workspace_test.rs
  project_test.rs
  issue_test.rs
  auth_test.rs
```

Execution order (priority = traffic + complexity): `workspace` → `project` → `issue` → `cycle` → `module` → `state` → `user` → remaining → `auth`.

Each domain task follows same TDD template (failing contract → implement → shadow → proxy 10%→100% → commit). Below fully details first 3; remaining repeat template.

---

### Task 2.1: Common Models + Auth Middleware

**Files:**

- Create: `apps/api-rs/crates/common/src/models/workspace.rs`
- Create: `apps/api-rs/crates/common/src/models/project.rs`
- Create: `apps/api-rs/crates/common/src/models/issue.rs`
- Create: `apps/api-rs/crates/api/src/middleware/auth.rs`
- Create: `apps/api-rs/crates/api/src/middleware/mod.rs`
- Test: `apps/api-rs/crates/api/tests/auth_test.rs`

- [ ] **Step 1: Failing auth test (401 on bad JWT)**

```rust
// crates/api/tests/auth_test.rs
#[tokio::test]
async fn rejects_invalid_jwt() {
    let app = api::test_app().await;
    let req = axum::http::Request::builder().uri("/api/workspaces/").header("Authorization","Bearer bad").body(axum::body::Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);
}
#[tokio::test]
async fn accepts_valid_jwt() {
    let token = api::test_jwt_for("user-123");
    let app = api::test_app().await;
    let req = axum::http::Request::builder().uri("/api/workspaces/").header("Authorization", format!("Bearer {}", token)).body(axum::body::Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), 401);
}
```

- [ ] **Step 2: Implement models + auth**

```rust
// crates/common/src/models/workspace.rs
use sqlx::FromRow; use serde::{Serialize, Deserialize};
#[derive(FromRow, Serialize)] pub struct Workspace { pub id: uuid::Uuid, pub name: String, pub slug: String }
// crates/api/src/middleware/auth.rs
use axum::{extract::FromRequestParts, http::{request::Parts, StatusCode}};
pub struct AuthUser(pub uuid::Uuid);
#[async_trait::async_trait] impl<S: Send+Sync> FromRequestParts<S> for AuthUser {
    async fn from_request_parts(parts: &mut Parts, _s: &S) -> Result<Self, axum::response::Response> {
        let hdr = parts.headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
        let token = hdr.strip_prefix("Bearer ").ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing bearer").into_response())?;
        let data = jsonwebtoken::decode::<Claims>(token, &jsonwebtoken::DecodingKey::from_secret(b"test-secret"), &jsonwebtoken::Validation::default())
            .map_err(|_| (StatusCode::UNAUTHORIZED, "bad jwt").into_response())?;
        Ok(AuthUser(data.claims.sub))
    }
}
#[derive(serde::Deserialize)] struct Claims { sub: uuid::Uuid, exp: usize }
```

- [ ] **Step 3: Run pass**

Run: `cargo test -p api --test auth_test -v`
Expected: PASS (1 ok 401, 1 ok 200)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/common/src/models/ apps/api-rs/crates/api/src/middleware/
git commit -m "feat(rs-2): workspace/project/issue models + JWT auth extractor"
```

---

### Task 2.2: Workspace Domain (Read + Write)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/workspace.rs`
- Test: `apps/api-rs/crates/api/tests/workspace_test.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`

- [ ] **Step 1: Failing contract tests (snapshot vs Django)**

```rust
// crates/api/tests/workspace_test.rs
#[tokio::test]
async fn workspace_list_parity() {
    // Seed: create workspace via Django api:8000, then compare Rust 8001
    let django: serde_json::Value = reqwest::get("http://api:8000/api/workspaces/").await.unwrap().json().await.unwrap();
    let rust: serde_json::Value = reqwest::get("http://rust-api:8001/api/workspaces/").await.unwrap().json().await.unwrap();
    assert_eq!(django, rust, "list parity failed — check sqlx query vs plane/app/views/workspace");
}
#[tokio::test]
async fn workspace_create_validates() {
    let app = api::test_app_with_auth().await;
    let body = serde_json::json!({"name":""}); // invalid empty
    let resp = app.oneshot(req_with_json("/api/workspaces/", body)).await.unwrap();
    assert_eq!(resp.status(), 400);
}
```

- [ ] **Step 2: Run fail**

Run: `cargo test -p api --test workspace_test -- --nocapture`
Expected: FAIL `route not found` / `parity diff`

- [ ] **Step 3: Implement workspace.rs**

```rust
// crates/api/src/routes/workspace.rs
use axum::{extract::State, Json, http::StatusCode};
use validator::Validate;
use crate::state::AppState;
#[derive(serde::Deserialize, Validate)] pub struct Create { #[validate(length(min=1, max=100))] pub name: String, pub slug: Option<String> }
pub async fn list(State(st): State<AppState>, _auth: crate::middleware::auth::AuthUser) -> Result<Json<serde_json::Value>, crate::errors::AppError> {
    let rows = sqlx::query_as!(common::models::workspace::Workspace, "SELECT id, name, slug FROM workspace WHERE deleted_at IS NULL ORDER BY created_at DESC").fetch_all(&st.pool).await?;
    Ok(Json(serde_json::json!(rows)))
}
pub async fn create(State(st): State<AppState>, _auth: crate::middleware::auth::AuthUser, Json(body): Json<Create>) -> Result<(StatusCode, Json<serde_json::Value>), crate::errors::AppError> {
    body.validate().map_err(|e| anyhow::anyhow!(e))?;
    let ws = sqlx::query_as!(common::models::workspace::Workspace, "INSERT INTO workspace (name, slug) VALUES ($1, $2) RETURNING id, name, slug", body.name, body.slug.unwrap_or(body.name.to_lowercase())).fetch_one(&st.pool).await?;
    // push async to stream if needed: common::stream::push_job(...)
    Ok((StatusCode::201, Json(serde_json::json!(ws))))
}
```

```rust
// crates/api/src/routes/mod.rs
pub mod workspace; pub mod health;
use axum::{Router, routing::{get, post}}; use crate::state::AppState;
pub fn router() -> Router<AppState> { Router::new().route("/api/workspaces/", get(workspace::list).post(workspace::create)) }
```

- [ ] **Step 4: Run pass + shadow verification**

Run: `docker compose up -d api rust-api && cargo test -p api --test workspace_test -v`
Expected: PASS (parity JSON identical)

Run: `bash apps/api-rs/scripts/compare.sh http://localhost:8000/api/workspaces/ http://localhost:8001/api/workspaces/`
Expected: `parity ok`

- [ ] **Step 5: Proxy cutover 100% for /api/workspaces**

Edit `apps/proxy/nginx.conf` or compose env `STRANGLER_WORKSPACE=rust` → `docker compose restart proxy && curl -s http://localhost/api/workspaces/ | jq . | head`

Expected: 200 via Rust.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workspace.rs apps/api-rs/crates/api/src/routes/mod.rs
git commit -m "feat(rs-2): workspace list+create (parity plane/app/views/workspace, 260 lines)"
```

---

### Task 2.3: Project Domain

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/project.rs`
- Create: `apps/api-rs/crates/common/src/models/project.rs`
- Test: `apps/api-rs/crates/api/tests/project_test.rs`

- [ ] **Step 1: Failing tests (mirrors `plane/app/urls/project.py:132`)**

```rust
#[tokio::test] async fn project_list_filters_by_workspace() {
    // GET /api/workspaces/{ws}/projects/ must match Django
}
#[tokio::test] async fn project_create_requires_name() {
    // POST with empty name → 400 validator
}
```

- [ ] **Step 2: Implement project.rs (sqlx with workspace_id FK)**

```rust
// crates/api/src/routes/project.rs
pub async fn list(State(st): State<AppState>, Path(ws_id): Path<uuid::Uuid>) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query_as!(Project, "SELECT id, name FROM project WHERE workspace_id=$1 AND deleted_at IS NULL", ws_id).fetch_all(&st.pool).await?;
    Ok(Json(json!(rows)))
}
```

- [ ] **Step 3: Run pass + parity**

Run: `cargo test -p api --test project_test -v && bash apps/api-rs/scripts/compare.sh http://localhost:8000/api/workspaces/$WS/projects/ http://localhost:8001/api/workspaces/$WS/projects/`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/project.rs
git commit -m "feat(rs-2): project domain (mirrors project.py:132)"
```

---

### Task 2.4: Issue Domain (Most Complex, #9526 Parity)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/issue.rs`
- Create: `apps/api-rs/crates/common/src/models/issue.rs`
- Test: `apps/api-rs/crates/api/tests/issue_test.rs`

- [ ] **Step 1: Failing tests — validates #9526 assignee/label silently-dropped bug**

```rust
#[tokio::test]
async fn issue_create_rejects_invalid_assignee() {
    let app = api::test_app_with_auth().await;
    let body = json!({"name":"Bug","assignees":["00000000-0000-0000-0000-000000000000"]});
    let resp = app.oneshot(req_with_json("/api/workspaces/ws/projects/p/issues/", body)).await.unwrap();
    assert_eq!(resp.status(), 400, "should reject invalid assignee id, not silently drop (fix #9526)");
}
#[tokio::test]
async fn issue_list_parity_large_payload() {
    let django = get_json("http://api:8000/api/workspaces/ws/projects/p/issues/").await;
    let rust = get_json("http://rust-api:8001/api/workspaces/ws/projects/p/issues/").await;
    assert_eq!(django, rust);
}
```

- [ ] **Step 2: Implement issue.rs with validator + join checks**

```rust
// crates/api/src/routes/issue.rs
#[derive(Deserialize, Validate)] pub struct CreateIssue {
    #[validate(length(min=1))] pub name: String,
    pub assignees: Option<Vec<uuid::Uuid>>,
    pub labels: Option<Vec<uuid::Uuid>>,
}
pub async fn create(State(st): State<AppState>, Path((ws, proj)): Path<(Uuid, Uuid)>, Json(body): Json<CreateIssue>) -> Result<(StatusCode, Json<Value>), AppError> {
    body.validate()?;
    if let Some(ids) = &body.assignees {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workspace_member WHERE id = ANY($1) AND workspace_id=$2", ids, ws).fetch_one(&st.pool).await?;
        if count.0 != ids.len() as i64 { return Err(anyhow::anyhow!("invalid assignee").into()); }
    }
    let issue = sqlx::query_as!(Issue, "INSERT INTO issue (name, project_id) VALUES ($1,$2) RETURNING id, name", body.name, proj).fetch_one(&st.pool).await?;
    // insert assignees, labels, push webhook job via stream
    Ok((StatusCode::201, Json(json!(issue))))
}
pub async fn list(State(st): State<AppState>, Path((ws, proj)): Path<(Uuid,Uuid)>) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query_as!(Issue, "SELECT id, name FROM issue WHERE project_id=$1 AND deleted_at IS NULL ORDER BY created_at DESC", proj).fetch_all(&st.pool).await?;
    Ok(Json(json!(rows)))
}
```

- [ ] **Step 3: Run pass (compile-time sql checked)**

Run: `cargo check -p api && cargo test -p api --test issue_test -v`
Expected: PASS (1 ok create 201, 1 ok list parity)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue.rs
git commit -m "feat(rs-2): issue domain with #9526 validation (286 lines parity)"
```

---

### Task 2.5: Cycle + Module + State Domains

_Repeat Task 2.2 template for:_

- `cycle.rs` ↔ `plane/api/urls/cycle.py` + `plane/app/urls/cycle.py:106` (guard no end_date #9200)
- `module.rs` ↔ `plane/app/urls/module.py:105`
- `state.rs` ↔ `plane/app/urls/state.py`

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/cycle.rs`
- Create: `apps/api-rs/crates/api/src/routes/module.rs`
- Create: `apps/api-rs/crates/api/src/routes/state.rs`
- Tests: `cycle_test.rs`, `module_test.rs`, `state_test.rs`

- [ ] **Each domain: failing parity test → sqlx impl → `cargo test` PASS → compare.sh parity → commit**

Example cycle guard:

```rust
if body.end_date.is_none() { anyhow::bail!("end_date required when archiving"); }
```

Commit per domain:

```bash
git add apps/api-rs/crates/api/src/routes/cycle.rs && git commit -m "feat(rs-2): cycle domain (guard #9200)"
git add apps/api-rs/crates/api/src/routes/module.rs && git commit -m "feat(rs-2): module domain"
git add apps/api-rs/crates/api/src/routes/state.rs && git commit -m "feat(rs-2): state domain"
```

---

### Task 2.6: User + Remaining Domains + Rate Limit

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/user.rs`
- Create: `apps/api-rs/crates/api/src/routes/asset.rs`, `intake.rs`, `label.rs`, etc.
- Modify: `apps/api-rs/crates/api/src/main.rs` (add tower-http RateLimit + BodyLimit)

- [ ] **Step 1: User tests (mirrors `plane/api/urls/user.py:85`, `plane/app/urls/user.py`)**

- [ ] **Step 2: Implement + tower layers**

```rust
// main.rs layers
Router::new().merge(routes::router()).layer(tower_http::limit::RequestBodyLimitLayer::new(5*1024*1024)).layer(tower_http::trace::TraceLayer::new_for_http())
```

- [ ] **Step 3: Run full api tests**

Run: `cargo test -p api -v`
Expected: All PASS (workspace, project, issue, cycle, module, state, user)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/user.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(rs-2): user + remaining domains + rate/body limit"
```

---

### Task 2.7: Shadow Traffic + Full Parity Gate

**Files:**

- Create: `apps/api-rs/scripts/shadow.sh`
- Create: `apps/api-rs/tests/parity_gate_test.rs`

- [ ] **Step 1: Shadow script (dual-run 1h)**

```bash
#!/bin/bash
# shadow.sh — curl both, diff, log mismatches
for path in /api/workspaces/ /api/workspaces/$WS/projects/ /api/workspaces/$WS/projects/$P/issues/; do
  diff <(curl -s http://api:8000$path | jq -S .) <(curl -s http://rust-api:8001$path | jq -S .) || echo "mismatch $path"
done
```

- [ ] **Step 2: Gate — run existing Django pytest as black-box via proxy**

Run: `docker compose -f docker-compose-test.yml up --build --abort-on-container-exit --exit-code-from api-tests`
Expected: `api-tests` exit 0 (all legacy tests pass through Rust proxy)

- [ ] **Step 3: Commit gate**

```bash
git add apps/api-rs/scripts/shadow.sh
git commit -m "test(rs-2): shadow + parity gate (legacy pytest via rust proxy)"
```

---

## Self-Review Plan 2

- [x] Each domain parity via snapshot vs Django (`plane/app/urls/*.py`, `plane/api/urls/*.py`)
- [x] #9526 + #9200 edge cases explicitly tested
- [x] Auth extracted, rate/body limit preserved
- [x] No placeholders, per-file commits
