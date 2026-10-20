# Page Sync + Image Upload Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix knowledge-base image upload failures and page sync loss on page `66a351a1` (and everywhere) caused by 15-minute access-token expiry, refresh races, S3 copy signing, and stuck editor upload state.

**Architecture:** Four independent, separately shippable fixes: (1) live server refreshes its stale cookie and retries store on 401; (2) api-rs refresh keeps a 60s grace window so concurrent tab refreshes don't kill sessions; (3) S3 server-side copy signs with `us-east-1` when `AWS_REGION` is empty (presign paths untouched — they work today); (4) editor marks orphaned `uploading` nodes as failed instead of spinning forever.

**Tech Stack:** Rust (axum, sqlx, redis), TypeScript (live/Hocuspocus + axios, @plane/editor React/Tiptap), vitest, cargo test, MinIO/SigV4.

**Evidence this plan is based on (do not re-investigate):**

- `docker logs plane-for-itsm-api-1`: `s3 copy_object failed 403 Forbidden` on `POST /api/assets/v2/workspaces/itsm/duplicate-assets/*`; periodic `401` on `PATCH/GET .../pages/66a351a1-.../description/`; `/api/auth/refresh/` 51×200 vs 39×401 in 24h.
- `journalctl --user -u plane-live.service`: `Failed to update page description binary 401`, `Authentication failed 401` on `/api/users/me/`.
- DB: page `66a351a1` description_json contains block `43e92ae6` with `"src": null, "status": "uploading"` (stuck forever); asset `ebc3bef1` (`PAGE_DESCRIPTION`, `is_uploaded=false`) matches the 03:41 `PATCH` asset 401s.
- `ACCESS_TTL_SECS = 900` (`apps/api-rs/crates/api/src/routes/auth.rs:25`); live reuses WS-connect-time cookie forever (`apps/live/src/lib/auth.ts:63`, `database.ts:79`).

---

### Task 1: Live retries page save after refreshing stale cookie (fixes "page not sync")

**Files:**

- Create: `apps/live/src/utils/cookies.ts`
- Test: `apps/live/tests/utils/cookies.test.ts`
- Modify: `apps/live/src/services/api.service.ts:17-76` (add method to `APIService` class)
- Modify: `apps/live/src/extensions/database.ts:72-134` (retry in `storeDocument` catch)

- [ ] **Step 1: Write the failing test for cookie merge**

`apps/live/tests/utils/cookies.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, it, expect } from "vitest";
import { mergeRefreshedCookies } from "@/utils/cookies";

describe("mergeRefreshedCookies", () => {
  it("replaces rotated access + refresh cookies, keeps the rest", () => {
    const merged = mergeRefreshedCookies("plane_at=old-at; plane_rt=old-rt; other=1", [
      "plane_at=new-at; HttpOnly; Path=/; Max-Age=900; SameSite=Lax",
      "plane_rt=new-rt; HttpOnly; Path=/; Max-Age=2592000; SameSite=Lax",
    ]);
    expect(merged).toBe("other=1; plane_at=new-at; plane_rt=new-rt");
  });

  it("handles __Host- prefixed pairs", () => {
    const merged = mergeRefreshedCookies("__Host-plane_at=old; plane_rt=old-rt", [
      "__Host-plane_at=new; HttpOnly; Path=/; Max-Age=900; SameSite=Lax",
    ]);
    expect(merged).toBe("plane_rt=old-rt; __Host-plane_at=new");
  });

  it("returns the old cookie unchanged when there are no set-cookies", () => {
    expect(mergeRefreshedCookies("plane_at=a; plane_rt=b", [])).toBe("plane_at=a; plane_rt=b");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=live test -- tests/utils/cookies.test.ts`
Expected: FAIL with "Cannot find module '@/utils/cookies'" (or failed to resolve import).

- [ ] **Step 3: Implement the pure cookie-merge helper**

`apps/live/src/utils/cookies.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

/**
 * Merge `Set-Cookie` values from a refresh response into an existing
 * `Cookie` header value. Rotated names are replaced, all other pairs
 * (e.g. unrelated cookies) are preserved untouched.
 */
export function mergeRefreshedCookies(oldCookie: string, setCookies: string[]): string {
  const updated = new Map<string, string>();
  for (const header of setCookies) {
    const pair = header.split(";")[0] ?? "";
    const eq = pair.indexOf("=");
    if (eq === -1) continue;
    const name = pair.slice(0, eq).trim();
    const value = pair.slice(eq + 1).trim();
    if (name && value) updated.set(name, value);
  }
  if (updated.size === 0) return oldCookie;
  const kept = oldCookie
    .split(";")
    .map((p) => p.trim())
    .filter(Boolean)
    .filter((pair) => {
      const eq = pair.indexOf("=");
      const name = eq === -1 ? pair : pair.slice(0, eq).trim();
      return !updated.has(name);
    });
  for (const [name, value] of updated) kept.push(`${name}=${value}`);
  return kept.join("; ");
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=live test -- tests/utils/cookies.test.ts`
Expected: PASS (3 passed).

- [ ] **Step 5: Add `refreshAccessCookie` to live `APIService`**

In `apps/live/src/services/api.service.ts`, add the import at the top and the method inside the `APIService` class (after `setHeader`/`getHeader`):

```ts
import { mergeRefreshedCookies } from "@/utils/cookies";
```

```ts
  /**
   * Rotate the session using the refresh cookie embedded in `cookie`.
   * Returns the merged `Cookie` header value, or `null` when rotation
   * failed (caller must keep the old cookie and surface the error).
   */
  async refreshAccessCookie(cookie: string): Promise<string | null> {
    try {
      const response = await this.axiosInstance.post("/api/auth/refresh/", null, {
        headers: { Cookie: cookie },
      });
      const setCookies = response.headers?.["set-cookie"] as unknown;
      const list = Array.isArray(setCookies) ? (setCookies as string[]) : [];
      if (list.length === 0) return null;
      return mergeRefreshedCookies(cookie, list);
    } catch {
      return null;
    }
  }
```

- [ ] **Step 6: Retry `storeDocument` once on 401 with a refreshed cookie**

In `apps/live/src/extensions/database.ts`, replace the start of the `storeDocument` catch block (current lines 92-94):

```ts
  } catch (error) {
    const appError = new AppError(error, { context: { pageId } });
    logger.error("Error in updating document:", appError);
```

with:

```ts
  } catch (error) {
    const appError = new AppError(error, { context: { pageId } });
    // The WS-connection-time cookie expires 15 min after connect
    // (api-rs ACCESS_TTL_SECS). Refresh once and retry instead of
    // dropping the save, then persist the fresh cookie on context.
    if (appError.statusCode === 401) {
      try {
        const refreshed = await service.refreshAccessCookie(context.cookie);
        if (refreshed) {
          context.cookie = refreshed;
          const retryService = getPageService(context.documentType, context);
          await retryService.updateDescriptionBinary(pageId, payload);
          return;
        }
      } catch {
        // fall through to the broadcast below
      }
    }
    logger.error("Error in updating document:", appError);
```

`service`, `payload`, `pageId`, `context`, `getPageService` already exist in that scope — no new imports needed.

- [ ] **Step 7: Run live checks**

Run: `pnpm --filter=live test`
Expected: PASS (all suites, including the 3 new tests).

Run: `pnpm --filter=live check:types`
Expected: PASS (no output / exit 0).

- [ ] **Step 8: Commit**

```bash
git add apps/live/src/utils/cookies.ts apps/live/tests/utils/cookies.test.ts apps/live/src/services/api.service.ts apps/live/src/extensions/database.ts
git commit -m "fix(live): refresh stale cookie and retry page save on 401"
```

---

### Task 2: api-rs refresh grace window (fixes refresh 401 race between tabs)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/auth.rs:25-26` (add const), `:299-316` (grace instead of DEL)
- Test: `apps/api-rs/crates/api/tests/auth_refresh_test.rs` (new integration test)

- [ ] **Step 1: Write the failing test**

`apps/api-rs/crates/api/tests/auth_refresh_test.rs`:

```rust
//! Refresh rotation must tolerate a concurrent second refresh with the
//! same token (two tabs refreshing at once). Pre-fix, the second call
//! gets 401 because the old hash is DELeted immediately.

use api::routes::auth::refresh;
use api::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use common::config::AppConfig;
use sqlx::PgPool;
use uuid::Uuid;

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

async fn state() -> AppState {
    AppState {
        pool: pool().await,
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

fn cookie_headers(raw_rt: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("cookie", format!("plane_rt={raw_rt}").parse().unwrap());
    h
}

#[tokio::test]
async fn concurrent_refresh_with_same_token_both_succeed() {
    let st = state().await;
    let uid = Uuid::new_v4();
    insert_user(&st.pool, uid, &format!("rtgrace-{}", uid.simple())).await;

    let family = Uuid::new_v4().simple().to_string();
    let raw = format!("rt_{}", Uuid::new_v4().simple());
    let hash = common::authn::sha256hex(&raw);
    let mut conn = st.redis_client().await.expect("redis must be reachable");
    redis::cmd("SET")
        .arg(common::authn::refresh_key(&hash))
        .arg(format!("{uid}:{family}"))
        .arg("EX")
        .arg(2_592_000)
        .query_async::<()>(&mut conn)
        .await
        .expect("seed refresh hash");
    redis::cmd("SADD")
        .arg(format!("auth:family:{family}"))
        .arg(&hash)
        .query_async::<()>(&mut conn)
        .await
        .expect("seed family set");

    // First refresh (tab A wins).
    let first = refresh(State(st.clone()), cookie_headers(&raw)).await.expect("handler runs");
    assert_eq!(first.0, StatusCode::OK, "first refresh must succeed");

    // Second refresh with the SAME old token (tab B raced) must also
    // succeed inside the grace window instead of 401.
    let second = refresh(State(st.clone()), cookie_headers(&raw)).await.expect("handler runs");
    assert_eq!(second.0, StatusCode::OK, "raced refresh must succeed inside grace window");

    // Cleanup: scratch user + family keys.
    sqlx::query("DELETE FROM users WHERE id = $1").bind(uid).execute(&st.pool).await.ok();
    redis::cmd("DEL").arg(format!("auth:family:{family}")).query_async::<()>(&mut conn).await.ok();
}
```

Notes: `AppState` must be `Clone` (it is — `issue_create_test` clones pools; verify: `State(st.clone())` requires `AppState: Clone`. If compile fails, construct `state().await` twice instead — same pool config, independent handles). `st.redis_client()` — verify the accessor name on `AppState` matches `state.rs` (used in `refresh` as `st.redis_client().await` — same call, so it exists).

- [ ] **Step 2: Run test to verify it fails**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test auth_refresh_test`
Expected: FAIL — second refresh asserts 200 but gets 401 (old hash was DELeted).

- [ ] **Step 3: Implement the grace window**

In `apps/api-rs/crates/api/src/routes/auth.rs`, add after line 26 (`const REFRESH_TTL_SECS...`):

```rust
/// Grace window for refresh rotation: the previous hash stays usable this
/// long so a concurrent refresh from another tab (same old token) still
/// succeeds instead of 401. Standard single-use-with-grace rotation.
const REFRESH_GRACE_SECS: i64 = 60;
```

Replace the rotation block (current lines 299-316):

```rust
    // Hapus hash lama + keluarkan dari SET keluarga.
    redis::cmd("DEL")
        .arg(authn::refresh_key(&old_hash))
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "auth refresh: del failed");
            common::errors::AppError::internal()
        })?;
    redis::cmd("SREM")
        .arg(family_key(family))
        .arg(&old_hash)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "auth refresh: srem failed");
            common::errors::AppError::internal()
        })?;
```

with:

```rust
    // Keluarkan hash lama dari SET keluarga, tapi biarkan hash-nya hidup
    // selama REFRESH_GRACE_SECS agar refresh konkuren dari tab lain dengan
    // token lama yang sama tetap sukses (bukan 401).
    redis::cmd("SREM")
        .arg(family_key(family))
        .arg(&old_hash)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "auth refresh: srem failed");
            common::errors::AppError::internal()
        })?;
    redis::cmd("EXPIRE")
        .arg(authn::refresh_key(&old_hash))
        .arg(REFRESH_GRACE_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "auth refresh: grace expire failed");
            common::errors::AppError::internal()
        })?;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test auth_refresh_test`
Expected: PASS (1 passed). Then run existing auth tests: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test auth_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth.rs apps/api-rs/crates/api/tests/auth_refresh_test.rs
git commit -m "fix(api): refresh grace window for concurrent tab rotation"
```

---

### Task 3: S3 server-side copy signs with a real region (fixes duplicate/paste 403)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/asset.rs:541-589` (add helper + use in `s3_copy_object` only)
- Test: `apps/api-rs/crates/api/tests/asset_test.rs` (append test)

- [ ] **Step 1: Write the failing test**

Append to `apps/api-rs/crates/api/tests/asset_test.rs` (extends the existing `use api::routes::asset::{...}` import — update it to include the new helper):

```rust
#[test]
fn copy_defaults_empty_region_to_us_east_1() {
    assert_eq!(api::routes::asset::default_s3_region(""), "us-east-1");
    assert_eq!(api::routes::asset::default_s3_region("ap-southeast-1"), "ap-southeast-1");
}
```

and change the file's import line to:

```rust
use api::routes::asset::{clamp_size, default_s3_region, validate_upload_init, CreateAssetInit, FILE_SIZE_LIMIT};
```

(check the exact current import line at the top of `asset_test.rs` before editing — it is `use api::routes::asset::{clamp_size, validate_upload_init, CreateAssetInit, FILE_SIZE_LIMIT};`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test asset_test copy_defaults_empty_region_to_us_east_1`
Expected: FAIL with "unresolved import `default_s3_region` / no function".

- [ ] **Step 3: Implement the helper and use it in `s3_copy_object` only**

In `apps/api-rs/crates/api/src/routes/asset.rs`, add right before `async fn s3_copy_object` (line 541):

```rust
/// Region used for server-side copy signing. Production sets
/// `AWS_REGION=""`; MinIO rejects header-auth SigV4 with an empty region
/// scope (403 on copy), while presigned POST/GET work — so default ONLY
/// the copy path to `us-east-1` (MinIO default) and leave presigns untouched.
pub fn default_s3_region(region: &str) -> &str {
    if region.is_empty() {
        "us-east-1"
    } else {
        region
    }
}
```

Inside `s3_copy_object`, change the two uses of `conf.region`:

- line 544: `let scope = format!("{datestamp}/{}/s3/aws4_request", conf.region);` → `let scope = format!("{datestamp}/{}/s3/aws4_request", default_s3_region(&conf.region));`
- line 567: `let signing = derive_signing_key(&conf.secret, &datestamp, &conf.region, "s3");` → `let signing = derive_signing_key(&conf.secret, &datestamp, default_s3_region(&conf.region), "s3");`
  Do NOT touch `presign_post` (line 451) or `presign_get` (line 527) — uploads/downloads work today.

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p api --test asset_test`
Expected: PASS (all tests including the new one).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/asset.rs apps/api-rs/crates/api/tests/asset_test.rs
git commit -m "fix(api): default empty S3 region to us-east-1 for copy_object"
```

---

### Task 4: Editor shows failed state for orphaned uploading nodes (fixes eternal broken image)

**Files:**

- Modify: `packages/editor/src/extensions/custom-image/components/node-view.tsx:10-13,25-38,120-133` (import + effect)

- [ ] **Step 1: Implement the stuck-upload guard**

In `packages/editor/src/extensions/custom-image/components/node-view.tsx`, change the utils import (line 13):

```ts
import { hasImageDuplicationFailed } from "../utils";
```

to:

```ts
import { getImageComponentImageFileMap, hasImageDuplicationFailed } from "../utils";
```

Then add after the `isDuplicatingRef` declaration (line 38), before the first `useEffect` (line 40):

```tsx
const nodeEntityId = node.attrs[ECustomImageAttributeNames.ID];
// A node stuck in "uploading" with no src and no in-flight upload in this
// session is a leftover from a failed upload (e.g. 401 on mark-uploaded):
// show the error state instead of an eternal loader so the user can
// delete the block and re-upload.
useEffect(() => {
  if (status !== ECustomImageStatus.UPLOADING || imgNodeSrc) return;
  const inFlight = nodeEntityId ? getImageComponentImageFileMap(editor)?.has(nodeEntityId) : false;
  if (!inFlight) setFailedToLoadImage(true);
}, [status, imgNodeSrc, nodeEntityId, editor]);
```

(`ECustomImageAttributeNames`, `ECustomImageStatus`, `imgNodeSrc`, `status`, `editor`, `setFailedToLoadImage` all already exist in this file's scope. `getImageComponentImageFileMap` returns `editor.storage.imageComponent?.fileMap` per `utils.ts:24` — `.has(nodeEntityId)` matches the uploader's keying by `node.attrs.id`.)

- [ ] **Step 2: Verify types + lint**

Run: `pnpm --filter=@plane/editor check:types`
Expected: PASS (exit 0).

Run: `pnpm --filter=@plane/editor check:lint`
Expected: PASS (no new warnings; budget is `--max-warnings=416`).

- [ ] **Step 3: Manual verification on the broken page**

1. Rebuild + restart web prod: `pnpm --filter=web build && systemctl --user restart plane-web-prod.service` (per AGENTS.md — required for tunnel to serve the change).
2. Open `https://dashboard.terraline.space/itsm/projects/<projectId>/pages/66a351a1-af7b-48d4-a13b-417d39a05c4f` — the stuck block (`43e92ae6`) must now render "Error loading image" instead of an endless loader/blank.
3. Delete that block, upload a fresh image — it must reach `uploaded` with a visible image.

- [ ] **Step 4: Commit**

```bash
git add packages/editor/src/extensions/custom-image/components/node-view.tsx
git commit -m "fix(editor): surface failed state for orphaned uploading images"
```

---

## Self-Review

1. **Spec coverage:** Task 1 → "page is not sync" (live 401 on store). Task 2 → refresh 401 race (root cause shared by upload PATCH 401s and live staleness). Task 3 → `s3 copy_object 403` (duplicate/paste images). Task 4 → stuck `src: null, status: uploading` block (broken image after upload). All four reported symptoms have a task.
2. **Placeholder scan:** No TBD/TODO; every step has exact file paths, complete code, exact commands with expected output.
3. **Type consistency:** `mergeRefreshedCookies(oldCookie: string, setCookies: string[]) => string` used identically in test, helper, and service. `default_s3_region(&str) -> &str` used identically in test and both call sites. `REFRESH_GRACE_SECS: i64` matches `EXPIRE` arg type. Editor effect uses only names already in scope in `node-view.tsx`.
