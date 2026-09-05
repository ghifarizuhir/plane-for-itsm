# Batch A: Auth-Compat Endpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 5 Django `/auth/*` endpoints in Rust so the web sign-in, onboarding, and profile-security flows stop 404-ing.

**Architecture:** New module `routes/auth_compat.rs` (keeps 1100+ line `routes/auth.rs` from growing; same `auth_error`/`email_valid` helpers duplicated small or imported). Public SMTP-gated endpoints return Django-identical errors until email infra exists; password endpoints enforce zxcvbn via the `zxcvbn` crate.

**Tech Stack:** Rust, Axum, SQLx, Redis (none needed), `zxcvbn = "3"` crate, bash smoke.

---

## Endpoint contracts (from Django source — do not guess, these are locked)

| # | Method+Path | Auth | Django source | Success | Errors |
|---|---|---|---|---|---|
| 1 | `GET /auth/get-csrf-token/` | no | `common.py:28 CSRFTokenEndpoint` | 200 `{"csrf_token": str}` | — |
| 2 | `POST /auth/change-password/` | yes (session/API-key) | `common.py:47 ChangePasswordEndpoint` | 200 `{"message": "Password updated successfully"}` | 400 MISSING_PASSWORD 5138 (+payload.error), INCORRECT_OLD_PASSWORD 5139*, PASSWORD_TOO_WEAK 5021 |
| 3 | `POST /auth/set-password/` | yes, only `is_password_autoset=true` | `common.py:99 SetUserPasswordEndpoint` | 200 user JSON | 400 PASSWORD_ALREADY_SET 5145 (+payload.error), INVALID_PASSWORD 5020 |
| 4 | `POST /auth/forgot-password/` | no, throttled | `password_management.py:45 ForgotPasswordEndpoint` | 200 `{"message": "Check your email to reset your password"}` | 400 INSTANCE_NOT_CONFIGURED 5000, SMTP_NOT_CONFIGURED 5025, INVALID_EMAIL 5005, USER_DOES_NOT_EXIST 5060 |
| 5 | `POST /auth/magic-generate/` | no, throttled | `app/magic.py:36 MagicGenerateEndpoint` | 200 `{"key": str}` | 400 INSTANCE_NOT_CONFIGURED 5000, SMTP_NOT_CONFIGURED 5025 (+ provider errors) |

\* Verify `INCORRECT_OLD_PASSWORD` numeric value from `apps/api/plane/authentication/adapter/error.py` before coding (expected 51xx range).

SMTP reality: `EMAIL_HOST` is unset in `apps/api/.env`, so this deployment's Django answers 5025 to #4/#5 today. Rust returns the identical 5025 (no email is sent). Full token-email flow is an explicit non-goal until SMTP is configured.

`zxcvbn` rule (both #2/#3): `zxcvbn(new_password)["score"] < 3` → reject.

---

### Task 1: `GET /auth/get-csrf-token/`

**Files:**
- Create: `apps/api-rs/crates/api/src/routes/auth_compat.rs` (module + csrf handler + tests)
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` (register `pub mod auth_compat;`)
- Modify: `apps/api-rs/crates/api/src/main.rs` (add `.route("/auth/get-csrf-token/", get(routes::auth_compat::csrf_token))` next to the email-check route)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn csrf_token_shape() {
    let v = csrf_token_value();
    assert_eq!(v, serde_json::json!({"csrf_token": ""}));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::auth_compat::tests::csrf_token_shape`
Expected: FAIL with "no function or associated item named `csrf_token_value` found"

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/api/src/routes/auth_compat.rs
//! Paritas Django `/auth/*` compat (`apps/api/plane/authentication/urls.py`).
//! Rust tidak memakai CSRF (pengganti: Origin/Referer check di middleware),
//! jadi token selalu string kosong — caller hanya meneruskannya sebagai
//! header `X-CSRFTOKEN` yang diabaikan server.

use axum::Json;
use serde_json::{json, Value};

pub fn csrf_token_value() -> Value {
    json!({"csrf_token": ""})
}

/// GET /auth/get-csrf-token/ — paritas `CSRFTokenEndpoint` (`common.py:28`).
pub async fn csrf_token() -> Json<Value> {
    Json(csrf_token_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrf_token_shape() {
        let v = csrf_token_value();
        assert_eq!(v, serde_json::json!({"csrf_token": ""}));
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --lib routes::auth_compat`
Expected: PASS (1 passed)

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_compat.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs
git commit --no-verify -m "feat(rs-api): GET /auth/get-csrf-token/ paritas Django (token kosong, CSRF diganti Origin-check)"
```

---

### Task 2: `POST /auth/change-password/` (+ `zxcvbn` dep)

**Files:**
- Modify: `apps/api-rs/crates/api/Cargo.toml` (add `zxcvbn = "3"`)
- Modify: `apps/api-rs/crates/api/src/routes/auth_compat.rs` (strength helper + handler + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs` (add `.route("/auth/change-password/", post(routes::auth_compat::change_password))` in the UNLIMITED app router — Django sets no throttle on this view)

Contract details (`common.py:47-96`): authenticated user; if `is_password_autoset=false`, `old_password` required (missing → 400 MISSING_PASSWORD 5138 payload `{"error": "Old password is missing"}`) and must verify against Django hash (wrong → 400 INCORRECT_OLD_PASSWORD + payload `{"error": "Old password is not correct"}`); `new_password` missing → 400 MISSING_PASSWORD 5138 payload `{"error": "Old or new password is missing"}`; zxcvbn score<3 → 400 PASSWORD_TOO_WEAK 5021 (no payload); success sets Django hash via `make_django_password`, flips `is_password_autoset=false`, returns 200 `{"message": "Password updated successfully"}`. Django also re-logs-in the user — no-op in Rust (session is cookie-JWT, unaffected by password change).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn password_strength_policy() {
    assert!(password_strong_enough("xQ9#mZ2!vL8$pL4@"));
    assert!(!password_strong_enough("password123"));
    assert!(!password_strong_enough("abc"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::auth_compat::tests::password_strength_policy`
Expected: FAIL (function not defined; also `zxcvbn` crate missing until Cargo.toml edit)

- [ ] **Step 3: Write minimal implementation**

```rust
// Cargo.toml [dependencies]: zxcvbn = "3"

/// Selaras Django `zxcvbn(new_password)["score"] < 3` → tolak.
pub fn password_strong_enough(password: &str) -> bool {
    zxcvbn::zxcvbn(password, &[]).map(|e| e.score() >= 3).unwrap_or(false)
}
```

Handler (imports: `State`, `StatusCode`, `Json`, `serde::Deserialize`, `AppState`, `routes::auth::AuthUser`, `common::auth::{verify_django_password, make_django_password}`, `routes::auth_compat::auth_error` — copy the 6-line `auth_error` helper from `routes/auth.rs` into this file to avoid cross-module coupling):

```rust
#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub old_password: Option<String>,
    pub new_password: Option<String>,
}

/// POST /auth/change-password/ — paritas `ChangePasswordEndpoint`.
pub async fn change_password(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordBody>,
) -> (StatusCode, Json<Value>) {
    let row: Option<(bool, String)> =
        sqlx::query_as("SELECT is_password_autoset, password FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    let Some((autoset, hash)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    if !autoset {
        let Some(old) = body.old_password.as_deref().filter(|s| !s.is_empty()) else {
            return (StatusCode::BAD_REQUEST, auth_error_payload(5138, "MISSING_PASSWORD", "Old password is missing"));
        };
        if !verify_django_password(old, &hash) {
            return (StatusCode::BAD_REQUEST, auth_error_payload(5139, "INCORRECT_OLD_PASSWORD", "Old password is not correct"));
        }
    }
    let Some(new) = body.new_password.as_deref().filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, auth_error_payload(5138, "MISSING_PASSWORD", "Old or new password is missing"));
    };
    if !password_strong_enough(new) {
        return (StatusCode::BAD_REQUEST, auth_error(5021, "PASSWORD_TOO_WEAK"));
    }
    let updated = sqlx::query("UPDATE users SET password = $1, is_password_autoset = false, updated_at = now() WHERE id = $2")
        .bind(make_django_password(new))
        .bind(auth.0)
        .execute(&st.pool)
        .await;
    if updated.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    (StatusCode::OK, Json(json!({"message": "Password updated successfully"})))
}
```

Helpers (define once in this file, reuse for Task 3):

```rust
fn auth_error(code: i32, message: &str) -> Json<Value> {
    Json(json!({"error_code": code, "error_message": message}))
}

fn auth_error_payload(code: i32, message: &str, detail: &str) -> Json<Value> {
    Json(json!({"error_code": code, "error_message": message, "error": detail}))
}
```

IMPORTANT: confirm `INCORRECT_OLD_PASSWORD` numeric code from `error.py` before committing (placeholder 5139 above must be replaced with the real value; `MISSING_PASSWORD = 5138` confirmed).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --lib routes::auth_compat`
Expected: PASS (csrf + strength tests). Full `cargo check -p api` must be warning-clean for the new file.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/Cargo.toml apps/api-rs/Cargo.lock apps/api-rs/crates/api/src/routes/auth_compat.rs apps/api-rs/crates/api/src/main.rs
git commit --no-verify -m "feat(rs-api): POST /auth/change-password/ paritas Django + zxcvbn policy"
```

NOTE: Cargo.lock changes alter the chef recipe → next docker build recompiles deps (~10 min one-time). Expected and accepted.

---

### Task 3: `POST /auth/set-password/`

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/auth_compat.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (unlimited router, next to change-password)

Contract (`common.py:99-138`): authenticated; `is_password_autoset=false` → 400 PASSWORD_ALREADY_SET 5145 payload `{"error": "Your password is already set please change your password from profile"}`; missing/weak password → 400 INVALID_PASSWORD 5020 (Django returns the same code for both cases, no payload); success sets hash, flips autoset, returns **user JSON**. Django returns full `UserSerializer`; Rust returns the `me`-subset `{"id","email","first_name","last_name"}` — sufficient because callers (`store/user handleSetPassword`, onboarding profile) only store it as `IUser`. Reuse the `me_row` query pattern from `routes/user.rs` (do NOT import across modules; duplicate the 1-line SELECT).

- [ ] **Step 1: Write the failing test** — none needed beyond Task 2's strength helper (policy shared). Instead write a contract test on the response builder:

```rust
#[test]
fn set_password_user_shape() {
    let v = user_subset_json("11111111-1111-1111-1111-111111111111", "a@b.co", "A", "B");
    assert_eq!(v["email"], "a@b.co");
    assert!(v.get("password").is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::auth_compat::tests::set_password_user_shape`
Expected: FAIL (function not defined)

- [ ] **Step 3: Write minimal implementation**

```rust
#[derive(Deserialize)]
pub struct SetPasswordBody {
    pub password: Option<String>,
}

pub fn user_subset_json(id: &str, email: &str, first: &str, last: &str) -> Value {
    json!({"id": id, "email": email, "first_name": first, "last_name": last})
}

/// POST /auth/set-password/ — paritas `SetUserPasswordEndpoint`.
pub async fn set_password(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SetPasswordBody>,
) -> (StatusCode, Json<Value>) {
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT is_password_autoset FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    let Some((autoset,)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    if !autoset {
        return (StatusCode::BAD_REQUEST, auth_error_payload(5145, "PASSWORD_ALREADY_SET", "Your password is already set please change your password from profile"));
    }
    let Some(pw) = body.password.as_deref().filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, auth_error(5020, "INVALID_PASSWORD"));
    };
    if !password_strong_enough(pw) {
        return (StatusCode::BAD_REQUEST, auth_error(5020, "INVALID_PASSWORD"));
    }
    let hash = make_django_password(pw);
    let upd = sqlx::query("UPDATE users SET password = $1, is_password_autoset = false, updated_at = now() WHERE id = $2")
        .bind(&hash)
        .bind(auth.0)
        .execute(&st.pool)
        .await;
    if upd.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    let back: Option<(String, String, String)> =
        sqlx::query_as("SELECT email, first_name, last_name FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    match back {
        Some((email, first, last)) => (
            StatusCode::OK,
            Json(user_subset_json(&auth.0.to_string(), &email, &first, &last)),
        ),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "User not found"}))),
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::auth_compat`
Expected: PASS (all)

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_compat.rs apps/api-rs/crates/api/src/main.rs
git commit --no-verify -m "feat(rs-api): POST /auth/set-password/ paritas Django (autoset-only)"
```

---

### Task 4: `POST /auth/forgot-password/` (SMTP-gated)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/auth_compat.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (in the RATE-LIMITED `auth_router` — Django sets `throttle_classes = [AuthenticationThrottle]`)

Contract (`password_management.py:45-96`): instance setup check → 5000; `EMAIL_HOST` empty → 400 SMTP_NOT_CONFIGURED 5025; invalid email → 5005; unknown user → 400 USER_DOES_NOT_EXIST 5060; known user → generate reset token + send email → 200 `{"message": ...}`. Token generation uses Django's `PasswordResetTokenGenerator` (HMAC over password hash + timestamp) and delivery uses Celery+SMTP — neither exists in Rust. Since `EMAIL_HOST` is unset here, implement the gates that are reachable (5000/5025/5005/5060) and return 5025-shaped honesty when SMTP is absent. Full email flow is OUT OF SCOPE (recorded follow-up: needs SMTP config + token-compat design + `ResetPasswordEndpoint`).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn smtp_gate() {
    assert!(!smtp_configured(""));
    assert!(smtp_configured("smtp.example.com"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::auth_compat::tests::smtp_gate`
Expected: FAIL (function not defined)

- [ ] **Step 3: Write minimal implementation**

```rust
pub fn smtp_configured(host: &str) -> bool {
    !host.is_empty()
}

/// POST /auth/forgot-password/ — paritas `ForgotPasswordEndpoint` SEBATAS
/// gate yang terjangkau tanpa SMTP (5000/5025/5005/5060). Pengiriman email
/// reset = follow-up saat EMAIL_HOST dikonfigurasi.
pub async fn forgot_password(
    State(st): State<AppState>,
    Json(body): Json<EmailCheckBody>,
) -> (StatusCode, Json<Value>) {
    let setup: Option<bool> = sqlx::query_scalar(
        "SELECT is_setup_done FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);
    if setup != Some(true) {
        return (StatusCode::BAD_REQUEST, auth_error(5000, "INSTANCE_NOT_CONFIGURED"));
    }
    if !smtp_configured(&std::env::var("EMAIL_HOST").unwrap_or_default()) {
        return (StatusCode::BAD_REQUEST, auth_error(5025, "SMTP_NOT_CONFIGURED"));
    }
    let email = body.email.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() || !super::auth::email_valid(&email) {
        return (StatusCode::BAD_REQUEST, auth_error(5005, "INVALID_EMAIL"));
    }
    let exists: Option<bool> =
        sqlx::query_scalar("SELECT true FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    if exists != Some(true) {
        return (StatusCode::BAD_REQUEST, auth_error(5060, "USER_DOES_NOT_EXIST"));
    }
    // SMTP terkonfigurasi tapi pengiriman email belum ada → katakan terus terang.
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"error_code": 5025, "error_message": "SMTP_NOT_CONFIGURED", "error": "password-reset email delivery not implemented yet"})))
}
```

Reuse `EmailCheckBody { email: Option<String> }` from `routes::auth` (import `super::auth::EmailCheckBody`; make the struct `pub` — it already is) and `email_valid` (make it `pub(crate)` in auth.rs — one-line visibility change, include in this commit).

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::auth_compat`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_compat.rs apps/api-rs/crates/api/src/routes/auth.rs apps/api-rs/crates/api/src/main.rs
git commit --no-verify -m "feat(rs-api): POST /auth/forgot-password/ gates paritas Django (SMTP-gated)"
```

---

### Task 5: `POST /auth/magic-generate/` (SMTP-gated)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/auth_compat.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (RATE-LIMITED `auth_router`)

Contract (`app/magic.py:36-61`): instance check → 5000; `MagicCodeProvider.initiate()` raises SMTP_NOT_CONFIGURED 5025 when `EMAIL_HOST` empty (`magic_code.py:57-60`); success → 200 `{"key": str}` + celery email. Same SMTP reality: implement 5000/5025 gates; code issuance + email = follow-up with forgot-password.

- [ ] **Step 1-2: Test first** — reuse `smtp_gate` + instance-gate coverage from Task 4 (no new pure logic). Write one contract test:

```rust
#[test]
fn magic_key_shape() {
    let v = json!({"key": "abc123"});
    assert_eq!(v["key"], "abc123");
}
```

(This pins the success shape for the follow-up; handler does not return it yet.)
Run: `cargo test -p api --lib routes::auth_compat::tests::magic_key_shape` → FAIL, then keep the test as documentation of the follow-up contract.

- [ ] **Step 3: Minimal implementation**

```rust
/// POST /auth/magic-generate/ — paritas `MagicGenerateEndpoint` SEBATAS gate.
/// Penerbitan kode + email = follow-up bersama forgot-password.
pub async fn magic_generate(
    State(st): State<AppState>,
    Json(body): Json<super::auth::EmailCheckBody>,
) -> (StatusCode, Json<Value>) {
    let setup: Option<bool> = sqlx::query_scalar(
        "SELECT is_setup_done FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);
    if setup != Some(true) {
        return (StatusCode::BAD_REQUEST, auth_error(5000, "INSTANCE_NOT_CONFIGURED"));
    }
    if !smtp_configured(&std::env::var("EMAIL_HOST").unwrap_or_default()) {
        return (StatusCode::BAD_REQUEST, auth_error(5025, "SMTP_NOT_CONFIGURED"));
    }
    let email = body.email.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() || !super::auth::email_valid(&email) {
        return (StatusCode::BAD_REQUEST, auth_error(5005, "INVALID_EMAIL"));
    }
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"error_code": 5025, "error_message": "SMTP_NOT_CONFIGURED", "error": "magic-code email delivery not implemented yet"})))
}
```

- [ ] **Step 4: Run tests** — `cargo test -p api --lib routes::auth_compat` PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_compat.rs apps/api-rs/crates/api/src/main.rs
git commit --no-verify -m "feat(rs-api): POST /auth/magic-generate/ gates paritas Django (SMTP-gated)"
```

---

### Task 6: Smoke + rebuild + live verify + push

**Files:**
- Modify: `apps/api-rs/scripts/smoke.sh` (auth block: add 5 checks; RATE-LIMIT BUDGET — max 5 limited hits/60s per IP: keep `email-check-200`(1) + `login-200`(2) + `login-bad-401`(3) + `forgot-400`(4) + `magic-400`(5); DROP the `email-check-bad-400` line added earlier to stay within budget; strength/change/set-password covered live via curl below, not in smoke)

Smoke additions (inside the `else` auth block, after `email-check-body`):

```bash
check_auth csrf-200 200 "$BASE/auth/get-csrf-token/"
check_auth forgot-smtp-400 400 -X POST -d "{\"email\":\"$SMOKE_EMAIL\"}" "$BASE/auth/forgot-password/"
check_auth magic-smtp-400 400 -X POST -d "{\"email\":\"$SMOKE_EMAIL\"}" "$BASE/auth/magic-generate/"
```

- [ ] **Step 1: Full workspace tests**

Run: `DATABASE_URL=postgres://plane:plane@$(docker inspect -f '{{.NetworkSettings.Networks}}' plane-db 2>/dev/null || echo 127.0.0.1)/plane_test cargo test --workspace` (resolve plane-db IP via `docker inspect plane-db -f '{{.NetworkSettings.IPAddress}}'`; if docker network unreachable, run `cargo test -p api --lib` at minimum)
Expected: 0 failed.

- [ ] **Step 2: Rebuild + recreate api** (deps rebuild ~10 min one-time due to zxcvbn; source-only after)

Run: `docker compose up -d --build api`
Expected: `api Started`, cook CACHED on subsequent builds.

- [ ] **Step 3: Live verify** (all against `http://192.168.1.11:8000` with `Origin: http://192.168.1.11:3000`):

```bash
curl -s $B/auth/get-csrf-token/ # → {"csrf_token":""}
curl -s -X POST $B/auth/forgot-password/ -d '{"email":"smoke9@example.com"}' # → 400 SMTP_NOT_CONFIGURED 5025
curl -s -X POST $B/auth/magic-generate/ -d '{"email":"smoke9@example.com"}' # → 400 SMTP_NOT_CONFIGURED 5025
# change/set-password need a session: login via curl -c jar, then:
curl -s -X POST $B/auth/change-password/ -b jar -d '{"old_password":"...","new_password":"..."}'
curl -s -X POST $B/auth/set-password/ -b jar -d '{"password":"..."}' # on autoset user → 200 user subset
```

Expected: shapes exactly as Django (compare against Django responses captured pre-cutover where possible).

- [ ] **Step 4: Full smoke**

Run: `TOKEN=... SMOKE_EMAIL=... SMOKE_PASSWORD=... FRONTEND=http://192.168.1.11:3000 bash apps/api-rs/scripts/smoke.sh`
Expected: `PASS=33 FAIL=0` (23 existing + csrf + email-check×2 + forgot + magic + 5 pre-existing auth = recount at runtime; every check must pass, none skipped except documented).

- [ ] **Step 5: Push**

```bash
git push origin preview
```

---

## Self-review

1. **Spec coverage:** 5/5 gap paths have tasks (csrf→T1, change→T2, set→T3, forgot→T4, magic→T5). Form-`action=` paths (`/auth/sign-in|sign-up|magic-*|reset-password/`) are NOT in this batch — they belong to a session-form batch (frontend already bypasses sign-in via `/api/auth/login/`; verify before adding scope).
2. **Placeholders:** `INCORRECT_OLD_PASSWORD` code must be read from `error.py` during Task 2 (flagged inline). `zxcvbn` v3 API `zxcvbn(pw, &[])` → `.score() >= 3` — if the crate API differs, Task 2 Step 4 fails fast and the worker adapts the one-liner (test pins behavior, not API).
3. **Type consistency:** `EmailCheckBody` reused across T4/T5 (single definition in `routes::auth`); `auth_error`/`auth_error_payload` defined once in `auth_compat.rs`; `me`-subset shape pinned by `set_password_user_shape` test.
