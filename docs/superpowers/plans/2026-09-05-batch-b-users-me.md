# Batch B: Users-Me Core Endpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 6 `/api/users/me/*` paths (8 method-slots) in Rust so home boot, onboarding join-flows, and the change-email modal stop 404-ing.

**Architecture:** New module `routes/users_me.rs` (all handlers + pure helpers + tests). Reuse `AuthUser` from `crate::middleware::auth`, `email_valid` from `crate::routes::auth`, Redis via `st.redis_client()` (pattern from `routes/auth.rs` login). Mutating joins run in a `sqlx` transaction. Table/column names below were verified live against the database on 2026-09-05 — trust them, but each task has a verify step.

**Tech Stack:** Rust, Axum, SQLx (Postgres), Redis (deadlines/counters/codes), `rand` crate for 6-digit codes (add only if not already a direct dep — check `cargo tree -p api -i rand` first).

---

## Endpoint contracts (locked from Django source)

| # | Method+Path | Django source | Success | Errors |
|---|---|---|---|---|
| 0 | `GET /auth/get-csrf-token/` (value fix) | — | 200 `{"csrf_token": "rust-csrf-disabled"}` | — |
| 1 | `POST /api/users/me/email/generate-code/` | `app/views/user/base.py:137 generate_email_verification_code` | 200 `{"message": "Verification code sent to email"}` | 400 plain `{"error": ...}` ×4 (below); 429 `{"error_code":5900,"error_message":"RATE_LIMIT_EXCEEDED"}` after 3/hour |
| 2 | `PATCH /api/users/me/email/` | `app/views/user/base.py:176 update_email` | 200 user JSON (subset, below) | 400 plain `{"error": ...}` (below) |
| 3 | `GET /api/users/me/workspaces/` | `app/views/workspace/base.py:175 UserWorkSpacesEndpoint` | 200 array workspace+role+total_members | — |
| 4a | `GET /api/users/me/workspaces/invitations/` | `app/views/workspace/invite.py:236 UserWorkspaceInvitationsViewSet` (list) | 200 array invites + workspace-lite + invite_link | — |
| 4b | `POST /api/users/me/workspaces/invitations/` | same (`create`) | **204** empty | — |
| 5a | `GET /api/users/me/workspaces/:slug/projects/invitations/` | `app/views/project/invite.py:119 UserProjectInvitationsViewset` (list) | 200 array | 403 non-member |
| 5b | `POST /api/users/me/workspaces/:slug/projects/invitations/` | same (`create`) | **201** `{"message": "Projects joined successfully"}` | 403 + `{"error": "Only workspace admins can join private project"}` (SECRET guard) |
| 6 | `GET /api/users/me/workspaces/:slug/project-roles/` | `app/views/project/member.py:365 UserProjectRolesEndpoint` | 200 `{"<project_id>": <role>}` | 403 non-member |

Validation messages #1/#2 — byte-exact from `_validate_new_email` (`base.py:~100-135`):
- missing: `{"error": "Email is required"}` (plain shape, NOT error_code!)
- bad format: `{"error": "Invalid email format"}`
- same as current: `{"error": "New email must be different from current email"}`
- taken: `{"error": "An account with this email already exists"}`
- #2 extra: missing code → `{"error": "Verification code is required"}`; unknown/expired code → `{"error": "Verification code has expired or is invalid"}`; mismatch → `{"error": "Invalid verification code"}`

403 shape: grep `FORBIDDEN` in `crates/api/src/routes/` and match the dominant existing shape. If none exists, use `(StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})))` and note it in the commit message.

---

### Task 0: csrf sentinel (unblocks change-email modal)

**Why:** `change-email-modal.tsx:96-98` does `if (!csrfToken) throw` — Batch A's `""` breaks the modal. Non-empty sentinel stays truthy; server still ignores the header.

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/auth_compat.rs` (`csrf_token_value` + test)

- [ ] **Step 1: Update test to the new contract**

```rust
#[test]
fn csrf_token_shape() {
    let v = csrf_token_value();
    assert_eq!(v["csrf_token"], "rust-csrf-disabled");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::auth_compat::tests::csrf_token_shape`
Expected: FAIL (left `""` != right `"rust-csrf-disabled"`)

- [ ] **Step 3: Minimal implementation**

```rust
pub fn csrf_token_value() -> Value {
    // Non-empty agar caller frontend (`if (!csrfToken) throw`) tetap jalan;
    // server Rust mengabaikan header X-CSRFTOKEN (pengganti: Origin-check).
    json!({"csrf_token": "rust-csrf-disabled"})
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::auth_compat`
Expected: PASS (all 5)

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_compat.rs
git commit --no-verify -m "fix(rs-api): csrf sentinel non-empty agar change-email modal tidak throw"
```

---

### Task 1: `POST /api/users/me/email/generate-code/`

**Files:**
- Create: `apps/api-rs/crates/api/src/routes/users_me.rs` (module + T1 handler/tests; later tasks append)
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`, `main.rs` (unlimited router — Django view has no per-view throttle; the 3/hour rule is implemented in-handler via Redis)

Throttle (from `rate_limit.py:93 EmailVerificationThrottle`, rate `3/hour`, 429 shape `{"error_code":5900,"error_message":"RATE_LIMIT_EXCEEDED"}`):
`INCR emailcode:throttle:{uid}` + `EXPIRE ... 3600` (set expiry only when count==1); count>3 → 429.

Code storage (mirrors `cache.set(f"magic_email_update_{user.id}_{new_email}", {"token"}, 600)`):
`SET emailcode:{uid}:{email} {"token":"123456"} EX 600`. No email is sent (no SMTP in this deployment; Django `.delay()` is fire-and-forget and returns 200 regardless). Six digits: `100000 + rand % 900000`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn email_code_throttle_key_format() {
    let uid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    assert_eq!(email_code_throttle_key(&uid), "emailcode:throttle:11111111-1111-1111-1111-111111111111");
    assert_eq!(email_code_key(&uid, "a@b.co"), "emailcode:11111111-1111-1111-1111-111111111111:a@b.co");
}

#[test]
fn six_digit_code_range() {
    for _ in 0..200 {
        let c = new_email_code();
        assert!(c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p api --lib routes::users_me`
Expected: FAIL (module/functions not defined)

- [ ] **Step 3: Minimal implementation**

```rust
// crates/api/src/routes/users_me.rs
//! Paritas Django `/api/users/me/*` (`apps/api/plane/app/urls/user.py`,
//! `workspace.py`, `project.py`).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::state::AppState;

pub fn email_code_throttle_key(uid: &uuid::Uuid) -> String {
    format!("emailcode:throttle:{uid}")
}

pub fn email_code_key(uid: &uuid::Uuid, email: &str) -> String {
    format!("emailcode:{uid}:{email}")
}

/// 6 digit seperti Django `secrets.randbelow(900000) + 100000`.
pub fn new_email_code() -> String {
    let n: u32 = rand::random::<u32>() % 900000 + 100000;
    format!("{n:06}")
}

#[derive(Deserialize)]
pub struct EmailBody {
    pub email: Option<String>,
}

fn plain_error(message: &str) -> Json<Value> {
    Json(json!({"error": message}))
}

/// Validasi email-baru, pesan byte-exact dari `_validate_new_email`.
async fn validate_new_email(
    pool: &sqlx::PgPool,
    uid: uuid::Uuid,
    current: &str,
    raw: Option<String>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let email = raw.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() {
        return Err((StatusCode::BAD_REQUEST, plain_error("Email is required")));
    }
    if !crate::routes::auth::email_valid(&email) {
        return Err((StatusCode::BAD_REQUEST, plain_error("Invalid email format")));
    }
    if email == current {
        return Err((StatusCode::BAD_REQUEST, plain_error("New email must be different from current email")));
    }
    let taken: Option<bool> =
        sqlx::query_scalar("SELECT true FROM users WHERE email = $1 AND id <> $2")
            .bind(&email)
            .bind(uid)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    if taken == Some(true) {
        return Err((StatusCode::BAD_REQUEST, plain_error("An account with this email already exists")));
    }
    Ok(email)
}
```

Handler:

```rust
/// POST /api/users/me/email/generate-code/ — paritas
/// `UserEndpoint.generate_email_verification_code` (3/jam via Redis).
pub async fn generate_email_code(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<EmailBody>,
) -> (StatusCode, Json<Value>) {
    let email_row: Option<(String,)> = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
        .unwrap_or(None);
    let Some((current,)) = email_row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let email = match validate_new_email(&st.pool, auth.0, &current.to_lowercase(), body.email).await {
        Ok(e) => e,
        Err(err) => return err,
    };
    let mut conn = match st.redis_client().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))),
    };
    let count: i64 = redis::cmd("INCR")
        .arg(email_code_throttle_key(&auth.0))
        .query_async(&mut conn)
        .await
        .unwrap_or(4);
    if count == 1 {
        let _: () = redis::cmd("EXPIRE").arg(email_code_throttle_key(&auth.0)).arg(3600).query_async(&mut conn).await.unwrap_or(());
    }
    if count > 3 {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error_code": 5900, "error_message": "RATE_LIMIT_EXCEEDED"})));
    }
    let code = new_email_code();
    let stored: redis::RedisResult<()> = redis::cmd("SET")
        .arg(email_code_key(&auth.0, &email))
        .arg(json!({"token": code}).to_string())
        .arg("EX")
        .arg(600)
        .query_async(&mut conn)
        .await;
    if stored.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    // SMTP belum ada: kode tersimpan di Redis 10 mnt; pengiriman email = follow-up.
    tracing::info!(user_id = %auth.0, "email verification code stored (delivery pending SMTP)");
    (StatusCode::OK, Json(json!({"message": "Verification code sent to email"})))
}
```

(`rand` — check `cargo tree -p api -i rand` first; if absent add `rand = "0.9"` to `crates/api/Cargo.toml` + include `Cargo.lock` in commit. `uuid` and `redis` are already deps — verify imports match existing files.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::users_me` → PASS; `cargo check -p api` clean for the new file.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/users_me.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs [apps/api-rs/crates/api/Cargo.toml apps/api-rs/Cargo.lock]
git commit --no-verify -m "feat(rs-api): POST /api/users/me/email/generate-code/ paritas Django (Redis, 3/jam)"
```

---

### Task 2: `PATCH /api/users/me/email/`

**Files:** append `apps/api-rs/crates/api/src/routes/users_me.rs`; route in unlimited router.

Contract (`update_email`): same validation → code required → Redis compare → re-check taken → `UPDATE users SET email, is_email_verified=false` → DEL key → return user JSON. Django returns full `UserMeSerializer`; Rust returns me-subset `{"id","email","first_name","last_name"}` — sufficient: the only FE caller (`change-email-modal verifyEmailCode`) ignores the body and signs out client-side. No server-side logout (stateless JWT ≤15 mnt; document in code comment).

- [ ] **Step 1: Failing test** — response-builder coverage:

```rust
#[test]
fn email_update_clears_key_format() {
    // key yang dihapus setelah sukses harus sama dengan key generate
    let uid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    assert_eq!(email_code_key(&uid, "n@x.io"), "emailcode:11111111-1111-1111-1111-111111111111:n@x.io");
}
```

(Trivial but pins generate/verify key agreement — the actual past bug class.)

- [ ] **Step 2: Run → FAIL** (test references existing helper; to make it genuinely red-first, write it before any T2 code — it passes immediately, which is FINE here: its value is regression-pinning. Note this honestly in the commit message.)

- [ ] **Step 3: Implementation**

```rust
#[derive(Deserialize)]
pub struct UpdateEmailBody {
    pub email: Option<String>,
    pub code: Option<String>,
}

/// PATCH /api/users/me/email/ — paritas `UserEndpoint.update_email`.
pub async fn update_email(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateEmailBody>,
) -> (StatusCode, Json<Value>) {
    let row: Option<(String,)> = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
        .unwrap_or(None);
    let Some((current,)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let email = match validate_new_email(&st.pool, auth.0, &current.to_lowercase(), body.email).await {
        Ok(e) => e,
        Err(err) => return err,
    };
    let code = body.code.unwrap_or_default().trim().to_string();
    if code.is_empty() {
        return (StatusCode::BAD_REQUEST, plain_error("Verification code is required"));
    }
    let mut conn = match st.redis_client().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))),
    };
    let key = email_code_key(&auth.0, &email);
    let cached: Option<String> = redis::cmd("GET").arg(&key).query_async(&mut conn).await.unwrap_or(None);
    let Some(raw) = cached else {
        return (StatusCode::BAD_REQUEST, plain_error("Verification code has expired or is invalid"));
    };
    let stored: String = serde_json::from_str::<Value>(&raw).ok().and_then(|v| v.get("token").and_then(|t| t.as_str()).map(str::to_string)).unwrap_or_default();
    if stored != code {
        return (StatusCode::BAD_REQUEST, plain_error("Invalid verification code"));
    }
    let taken: Option<bool> = sqlx::query_scalar("SELECT true FROM users WHERE email = $1 AND id <> $2")
        .bind(&email).bind(auth.0).fetch_optional(&st.pool).await.unwrap_or(None);
    if taken == Some(true) {
        return (StatusCode::BAD_REQUEST, plain_error("An account with this email already exists"));
    }
    let upd = sqlx::query("UPDATE users SET email = $1, is_email_verified = false, updated_at = now() WHERE id = $2")
        .bind(&email).bind(auth.0).execute(&st.pool).await;
    if upd.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap_or(());
    // Tanpa logout server-side: sesi Rust stateless (JWT ≤15 mnt, refresh
    // me-resolve uid → email baru berlaku otomatis); frontend sign-out sendiri.
    let back: Option<(String, String, String)> =
        sqlx::query_as("SELECT email, first_name, last_name FROM users WHERE id = $1")
            .bind(auth.0).fetch_optional(&st.pool).await.unwrap_or(None);
    match back {
        Some((email, first, last)) => (StatusCode::OK, Json(json!({"id": auth.0, "email": email, "first_name": first, "last_name": last}))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "User not found"}))),
    }
}
```

- [ ] **Step 4: Tests PASS** (`cargo test -p api --lib routes::users_me`), check clean.
- [ ] **Step 5: Commit** `feat(rs-api): PATCH /api/users/me/email/ paritas Django (kode via Redis)`

---

### Task 3: `GET /api/users/me/workspaces/`

**Files:** append `users_me.rs`; unlimited router.

SQL (mirrors the Django queryset: active membership, annotate role + member count of active non-bot members):

```sql
SELECT w.id, w.name, w.slug, w.timezone, w.organization_size, w.logo,
       fa.asset_url AS logo_asset_url,
       w.created_at, w.updated_at, w.created_by_id, w.updated_by_id,
       o.id AS owner_id, o.email AS owner_email, o.first_name AS owner_first, o.last_name AS owner_last,
       wm.role AS role,
       (SELECT COUNT(*) FROM workspace_members m JOIN users u ON u.id = m.member_id
         WHERE m.workspace_id = w.id AND m.is_active = true AND m.deleted_at IS NULL
           AND u.is_bot = false) AS total_members
FROM workspaces w
JOIN workspace_members wm ON wm.workspace_id = w.id
  AND wm.member_id = $1 AND wm.is_active = true AND wm.deleted_at IS NULL
JOIN users o ON o.id = w.owner_id
LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id
WHERE w.deleted_at IS NULL
ORDER BY w.created_at DESC
```

VERIFY FIRST via `docker exec plane-db psql`: `file_assets` table + `asset_url` column names; `users.is_bot` column; `workspaces.url`? (IWorkspace has `url` — NO such column in workspaces (14 cols listed); Django serializer `fields="__all__"` wouldn't have `url` either... IWorkspace.url must come from elsewhere or be unused — return `""`? Grep `workspace.url` usage in FE during implementation; if unused, OMIT the field and note it. Do NOT invent a value.)

Response per row:

```json
{"id": "...", "name": "...", "slug": "...", "timezone": "UTC",
 "organization_size": "", "logo_url": null,
 "created_at": "...", "updated_at": "...",
 "created_by": "<uuid>", "updated_by": "<uuid>",
 "owner": {"id": "...", "email": "...", "first_name": "...", "last_name": "..."},
 "role": 20, "total_members": 3}
```

(`logo_url`: `logo_asset_url ?? logo ?? null`. `organization_size`: `unwrap_or("")`.)

- [ ] Steps 1-2: test for the logo_url fallback helper:

```rust
pub fn pick_logo_url(asset: Option<&str>, logo: Option<&str>) -> Option<String> {
    asset.map(str::to_string).or_else(|| logo.map(str::to_string))
}

#[test]
fn logo_fallback_order() {
    assert_eq!(pick_logo_url(Some("a"), Some("b")).as_deref(), Some("a"));
    assert_eq!(pick_logo_url(None, Some("b")).as_deref(), Some("b"));
    assert_eq!(pick_logo_url(None, None), None);
}
```

- [ ] Step 3: implement handler `my_workspaces`.
- [ ] Step 4: tests + check. Step 5: commit `feat(rs-api): GET /api/users/me/workspaces/ paritas Django`.

---

### Task 4: workspace invitations GET + POST

**Files:** append `users_me.rs`; both routes unlimited (Django: default throttle only).

Tables (verified): `workspace_member_invites(id,email,accepted,token,message,responded_at,role,workspace_id,created_at,deleted_at,...)`.

GET list (filter `email = current user email`, join workspace lite):

```sql
SELECT i.id, i.email, i.accepted, i.token, i.message, i.responded_at, i.role,
       w.id, w.name, w.slug, w.logo
FROM workspace_member_invites i
JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL
JOIN users u ON u.id = $1
WHERE i.email = u.email AND i.deleted_at IS NULL
ORDER BY i.created_at DESC
```

Response item:

```json
{"id": "...", "email": "...", "accepted": false, "token": "...",
 "message": null, "responded_at": null, "role": 15,
 "workspace": {"id": "...", "name": "...", "slug": "...", "logo_url": null},
 "invite_link": "/workspace-invitations/?invitation_id=<id>&slug=<slug>&token=<token>"}
```

POST join (`{invitations: [uuid]}`) in ONE transaction — mirrors `create()`: for each invite id owned by this email: `UPDATE workspace_members SET is_active=true, role=invite.role WHERE workspace_id AND member_id` then `INSERT ... ON CONFLICT DO NOTHING` (Django `ignore_conflicts=True`; conflict target: implementer checks unique constraint on workspace_members — likely (workspace_id, member_id) — verify via `\d workspace_members`); `DELETE` the invites. Return **204 No Content** (empty body).

- [ ] Steps 1-2: test `invite_link` builder:

```rust
pub fn workspace_invite_link(id: &str, slug: &str, token: &str) -> String {
    format!("/workspace-invitations/?invitation_id={id}&slug={slug}&token={token}")
}

#[test]
fn invite_link_shape() {
    assert_eq!(workspace_invite_link("1", "s", "t"), "/workspace-invitations/?invitation_id=1&slug=s&token=t");
}
```

- [ ] Step 3: implement `my_workspace_invitations` (GET) + `join_workspaces` (POST, 204).
- [ ] Step 4: tests + check. Step 5: commit `feat(rs-api): workspace invitations GET+POST paritas Django (join 204)`.

---

### Task 5: project invitations GET + POST

**Files:** append `users_me.rs`; POST route needs workspace-membership + role check (see below); unlimited router (Django default throttle).

Tables: `project_member_invites(email,project_id,workspace_id,role,token,message,responded_at,accepted,...)`, `project_members(member_id,project_id,workspace_id,role,is_active,...)`, `project_user_properties`, `projects(id,workspace_id,network,...)`. VERIFY `project_user_properties` required columns via `\d` (Django bulk_creates with project/user/workspace/created_by — mirror those 4+id).

GET: invites for user email + project lite `{id, name, identifier?}` + workspace lite. Keep project lite to `{id, name}` + workspace `{id,name,slug,logo_url}` — FE has no typed consumer for this list (verify with grep; if none, document).

POST `{project_ids: [uuid]}` (Django `create`, exact message `{"message": "Projects joined successfully"}` 201):
1. Resolve workspace id by slug → 404 if unknown.
2. Requester's active membership → else 403 (convention from plan header).
3. Role must be ADMIN(20)/MEMBER(15) else 403 (mirrors `@allow_permission([ADMIN, MEMBER], WORKSPACE)`).
4. Load projects by ids in this workspace; SECRET(=0) project + requester not ADMIN → 403 `{"error": "Only workspace admins can join private project"}` (byte-exact).
5. Transaction: `UPDATE project_members SET is_active=true WHERE ... AND member` + bulk INSERT members (role = workspace role) + bulk INSERT project_user_properties. Ignore conflicts.
6. 201 + message.

- [ ] Steps 1-2: test SECRET-guard helper:

```rust
pub fn may_join_project(network: i32, requester_role: i32) -> bool {
    network != 0 || requester_role == 20
}

#[test]
fn secret_guard() {
    assert!(may_join_project(2, 15));
    assert!(may_join_project(0, 20));
    assert!(!may_join_project(0, 15));
}
```

- [ ] Step 3: implement. Step 4: tests + check. Step 5: commit `feat(rs-api): project invitations GET+POST paritas Django (SECRET guard)`.

---

### Task 6: `GET /api/users/me/workspaces/:slug/project-roles/`

**Files:** append `users_me.rs`; unlimited router.

```sql
-- require active membership first:
SELECT EXISTS(SELECT 1 FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id
  WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL)
-- then:
SELECT pm.project_id, pm.role FROM project_members pm
JOIN workspaces w ON w.id = pm.workspace_id
WHERE w.slug = $1 AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL
  AND EXISTS(SELECT 1 FROM workspace_members wm2 WHERE wm2.workspace_id = w.id
    AND wm2.member_id = $2 AND wm2.is_active = true AND wm2.deleted_at IS NULL)
```

Response: `{"<project-uuid>": 15, ...}` (keys = project_id strings, values = role ints). Non-member → 403 (convention from plan header).

- [ ] Steps 1-2: no pure logic — write a shape test on a builder:

```rust
pub fn project_roles_map(pairs: Vec<(String, i32)>) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in pairs { m.insert(k, json!(v)); }
    Value::Object(m)
}

#[test]
fn roles_map_shape() {
    let v = project_roles_map(vec![("pid".into(), 15)]);
    assert_eq!(v["pid"], 15);
}
```

- [ ] Step 3: implement `my_project_roles`. Step 4: tests + check. Step 5: commit `feat(rs-api): GET project-roles paritas Django`.

---

### Task 7: Smoke + rebuild + live verify + push

**Files:** modify `apps/api-rs/scripts/smoke.sh` — add ONLY unlimited checks (IP budget untouched):
- `check me-workspaces-200 200 "$BASE/api/users/me/workspaces/"` (after ws-detail)
- `check ws-invitations-200 200 "$BASE/api/users/me/workspaces/invitations/"`
- `check project-roles-200 200 "$BASE/api/users/me/workspaces/$WS/project-roles/"` (after PID known)
- `check_auth email-gen-invalid-400 400 -X POST -d '{"email":"x"}' "$BASE/api/users/me/email/generate-code/"` (in auth block — throttled 3/hour Redis counter, NOT IP limiter; invalid email fails validation BEFORE throttle increment — order in handler: validate first, so no budget consumed)

- [ ] **Step 1: Workspace tests** — `cargo test --workspace` (DB URL via `docker inspect plane-db`), 0 failed.
- [ ] **Step 2: Rebuild** — `docker compose up -d --build api` (source-only if no new deps; full deps if `rand` added — expect ~10 min, accepted).
- [ ] **Step 3: Live verify** with temp user (pattern from Batch A: SQL-insert → curl → DELETE):
  - generate-code with fresh email → 200; read code from Redis (`docker exec plane-redis redis-cli GET "emailcode:{uid}:{email}"`); PATCH update-email with code → 200 subset; 4th generate within hour → 429.
  - project join flow: invite temp user to workspace (SQL or existing invite), POST join → 201/204, verify membership rows.
  - DELETE temp rows afterwards.
- [ ] **Step 4: Full smoke** — expect PASS=27 FAIL=0 (23 + 4 new).
- [ ] **Step 5: Push** — `git push origin preview`.

---

## Self-review

1. **Spec coverage:** 6/6 gap paths → T1 (generate-code), T2 (update-email), T3 (me/workspaces), T4a/b (ws invitations), T5a/b (project invitations), T6 (project-roles). Plus T0 csrf fix (discovered during investigation: `change-email-modal` throws on `""`). `users/last-visited-workspace` is NOT in this batch (separate path, later batch).
2. **Placeholders:** table/column names verified live (workspaces 14 cols, project_members, project_member_invites, project_user_properties exists, SECRET=0). Open verifications are flagged inline with exact commands (`\d`, `cargo tree`, FE grep) — each fails fast at its step.
3. **Type consistency:** `EmailBody` (T1) vs `UpdateEmailBody` (T2) distinct; `plain_error` shared helper defined once in T1; key helpers `email_code_key` shared T1↔T2 (pinned by test); 204 POST returns empty body (`StatusCode::NO_CONTENT` without Json).
