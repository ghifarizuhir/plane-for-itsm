# Migrate Lock Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `common::db::migrate()` unable to wedge backend startup: fix the duplicate migration version that breaks every run, and bound + release the Postgres advisory lock on all paths.

**Architecture:** Renumber the duplicate `0002_remove_workflows.sql` to `0016` (rename-only) so validation passes and pending 13–16 apply. Rewire `migrate()` to run on a short-lived single-connection pool with a `statement_timeout` (bounds the advisory-lock wait when api/worker race at boot) that is always closed afterwards (releases the session lock sqlx 0.7.4 only unlocks on the success path). Callers keep warn-and-continue semantics.

**Tech Stack:** Rust, sqlx 0.7.4, Postgres 15 advisory locks, tokio, cargo test.

---

## Background (root causes, verified 2026-10-09)

1. **Duplicate version 2:** `apps/api-rs/migrations/` contains both `0002_issue_project_archived_idx.sql` (applied 2026-09-24) and `0002_remove_workflows.sql` (added 2026-10-03). sqlx iterates both version-2 entries; the second checksum mismatches the recorded one → `VersionMismatch(2)` on every boot, so pending migrations 13–15 never apply.
2. **Leak on every error path:** sqlx 0.7.4 `Migrator::run_direct` calls `conn.unlock()` only on full success (`sqlx-core-0.7.4/src/migrate/migrator.rs:182-185`); `Dirty`, validation, `VersionMismatch`, and apply errors all return early holding the session-level `pg_advisory_lock`. The pooled connection returns to the pool with the lock held → every later migrator blocks on `SELECT pg_advisory_lock($1)` forever. Observed twice (2026-10-08, 2026-10-09): worker leaks, api wedges, tunnel returns 502.
3. **Contributing doc bug:** `apps/api-rs/migrations/README.md` says new schema changes go in `0002_*.sql`, which is how the duplicate was created.

## File Structure

- Create `apps/api-rs/crates/common/tests/migration_versions_test.rs` — no-DB uniqueness test.
- Modify `apps/api-rs/migrations/0002_remove_workflows.sql` → rename to `0016_remove_workflows.sql` (content untouched).
- Modify `apps/api-rs/crates/common/src/db.rs` — new `migrate()` signature + hardening.
- Modify `apps/api-rs/crates/api/src/main.rs:45-47` and `apps/api-rs/crates/worker/src/main.rs:14-16` — call-site update (signature only).
- Modify `apps/api-rs/crates/common/tests/migrate_test.rs` — signature update.
- Modify `apps/api-rs/crates/common/Cargo.toml` — add `crc` dev-dependency.
- Create `apps/api-rs/crates/common/tests/migrate_lock_test.rs` — contention + leak regression tests.
- Modify `apps/api-rs/migrations/README.md` — immutability rule.

---

### Task 1: Version-uniqueness regression test (RED)

**Files:**

- Create: `apps/api-rs/crates/common/tests/migration_versions_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Migration hygiene: every file under `apps/api-rs/migrations/` must carry a
//! unique version number. A duplicate version makes `migrate()` fail with
//! `VersionMismatch` on every boot (2026-10-08/09: two `0002_*` files wedged
//! api + worker startup on the migration advisory lock).

#[test]
fn migration_versions_are_unique() {
    let migrator = sqlx::migrate!("../../migrations");
    let mut versions: Vec<i64> = migrator.iter().map(|m| m.version).collect();
    let total = versions.len();
    versions.sort_unstable();
    versions.dedup();
    assert_eq!(
        total,
        versions.len(),
        "duplicate migration version numbers in apps/api-rs/migrations"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common --test migration_versions_test`
Expected: FAIL — assertion fires on the two version-2 entries. (No DB needed for this test; `DATABASE_URL` is exported only so compilation succeeds if any `query!` macro in the crate tries to connect.)

- [ ] **Step 3: No commit yet**

Leave the failing test uncommitted; Task 2 turns it green and commits both together.

---

### Task 2: Renumber the duplicate migration to 0016

**Files:**

- Rename: `apps/api-rs/migrations/0002_remove_workflows.sql` → `apps/api-rs/migrations/0016_remove_workflows.sql` (via `git mv`, content byte-identical)
- Test: `apps/api-rs/crates/common/tests/migration_versions_test.rs` (from Task 1)

- [ ] **Step 1: Rename (no content change)**

```bash
git mv apps/api-rs/migrations/0002_remove_workflows.sql apps/api-rs/migrations/0016_remove_workflows.sql
git diff HEAD --stat -- apps/api-rs/migrations/
```

Expected: one rename, `similarity index 100%` (proves content untouched).

- [ ] **Step 2: Confirm no code references the old filename**

Run: `grep -rn "0002_remove_workflows" --include="*.rs" --include="*.py" --include="*.yml" --include="*.toml" . | grep -v docs/superpowers`
Expected: no output. (Historical references under `docs/superpowers/` stay as written history.)

- [ ] **Step 3: Run the uniqueness test to verify it passes**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common --test migration_versions_test`
Expected: PASS.

- [ ] **Step 4: Back up the dev database (cheap insurance)**

Run: `docker exec plane-for-itsm-plane-db-1 pg_dump -U plane -d plane -F c > /tmp/plane-pre-migrate-fix.dump && ls -la /tmp/plane-pre-migrate-fix.dump`
Expected: a non-empty dump file. (All deltas are `IF NOT EXISTS`-guarded per their headers, but a backup makes the apply below reversible.)

- [ ] **Step 5: Apply pending migrations once**

The backend app containers must be stopped so nothing contends while the repair lands; the database stays up. **Note: the API stays down until Task 3 finishes — run Tasks 2 and 3 back-to-back.**

```bash
docker compose -f docker-compose-local.yml stop api worker beat-worker
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common --test migrate_test
```

Expected: PASS. (This runs the pre-existing `migrate_runs` test, which applies pending 13–16 and then re-applies everything idempotently.)

- [ ] **Step 6: Verify the tracking table**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT version, description, success FROM _sqlx_migrations ORDER BY version;"`
Expected: versions 1–16 continuous, all `success = t` (12 old + 13 `review briefing`, 14 `intake service gate`, 15 `intake sources`, 16 `remove workflows` — descriptions may differ slightly; what matters is no gaps and all true).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/common/tests/migration_versions_test.rs apps/api-rs/migrations/0002_remove_workflows.sql apps/api-rs/migrations/0016_remove_workflows.sql
git commit -m "fix(api-rs): renumber duplicate migration 0002 to 0016"
```

(The `git mv` stages the rename; the `git add` of both paths is belt-and-braces. Verify with `git status --short` that nothing else — especially `apps/admin/*` — is staged.)

---

### Task 3: Harden `migrate()` — dedicated pool, bounded wait, always release

**Files:**

- Modify: `apps/api-rs/crates/common/src/db.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs:45-47`
- Modify: `apps/api-rs/crates/worker/src/main.rs:14-16`
- Modify: `apps/api-rs/crates/common/tests/migrate_test.rs`
- Modify: `apps/api-rs/crates/common/Cargo.toml` (dev-dependency)
- Create: `apps/api-rs/crates/common/tests/migrate_lock_test.rs`

- [ ] **Step 1: Rewire the signature (no behavior change yet)**

In `apps/api-rs/crates/common/src/db.rs`, replace:

```rust
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}
```

with:

```rust
pub async fn migrate(database_url: &str) -> anyhow::Result<()> {
    migrate_with_statement_timeout(database_url, "60s").await
}

pub async fn migrate_with_statement_timeout(database_url: &str, statement_timeout: &str) -> anyhow::Result<()> {
    let _ = statement_timeout; // bound in Step 4; no behavior change yet
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(())
}
```

Update both call sites (`apps/api-rs/crates/api/src/main.rs` and `apps/api-rs/crates/worker/src/main.rs`):

```rust
    if let Err(e) = common::db::migrate(&cfg.database_url).await {
        tracing::warn!(error=%e, "migrate failed");
    }
```

Update `apps/api-rs/crates/common/tests/migrate_test.rs`:

```rust
#[tokio::test]
async fn migrate_runs() {
    let cfg = common::config::AppConfig::from_env();
    common::db::migrate(&cfg.database_url).await.unwrap();
    let pool = common::db::create_pool(&cfg).await;
    // Baseline applied: core tables exist.
    let (exists,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name='workspaces')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(exists, "workspaces table must exist after migrate");
    // Idempotency: forget the tracked version and re-apply raw SQL guards.
    sqlx::query("DELETE FROM _sqlx_migrations")
        .execute(&pool)
        .await
        .unwrap();
    common::db::migrate(&cfg.database_url).await.unwrap();
}
```

- [ ] **Step 2: Write the lock regression test (will FAIL at runtime)**

Create `apps/api-rs/crates/common/tests/migrate_lock_test.rs`:

```rust
//! Regression tests: `migrate()` must fail fast (not hang) when another
//! session holds sqlx's migration advisory lock, and must never leak the lock
//! itself (2026-10-08/09: a leaked lock wedged api startup forever).
//!
//! Run with the backend app containers STOPPED (plane-db stays up):
//!   docker compose -f docker-compose-local.yml stop api worker beat-worker
//!   DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!     cargo test -p common --test migrate_lock_test

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// The two tests below must never overlap: the contention test holds sqlx's
/// migration advisory lock while the leak test asserts no session holds one.
/// A process-wide mutex keeps them serial however cargo schedules threads.
fn serial() -> &'static Mutex<()> {
    static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();
    SERIAL.get_or_init(|| Mutex::new(()))
}

/// sqlx 0.7.4's migration lock key: `0x3d32ad9e * CRC32_ISO_HDLC(db_name)`
/// (`sqlx-postgres-0.7.4/src/migrate.rs::generate_lock_id`).
fn sqlx_migration_lock_id(database_name: &str) -> i64 {
    const CRC_IEEE: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    0x3d32ad9e * (CRC_IEEE.checksum(database_name.as_bytes()) as i64)
}

#[tokio::test]
async fn migrate_fails_fast_when_lock_is_held() {
    let _guard = serial().lock().unwrap();
    let cfg = common::config::AppConfig::from_env();
    let holder = common::db::create_pool(&cfg).await;
    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&holder)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(sqlx_migration_lock_id(&db_name))
        .execute(&holder)
        .await
        .unwrap();

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(25),
        common::db::migrate_with_statement_timeout(&cfg.database_url, "5s"),
    )
    .await;
    let elapsed = started.elapsed();

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(sqlx_migration_lock_id(&db_name))
        .execute(&holder)
        .await
        .unwrap();

    let result = outcome.expect("migrate() hung waiting for the migration advisory lock");
    assert!(result.is_err(), "migrate() should fail while the lock is held");
    assert!(
        elapsed < Duration::from_secs(25),
        "migrate() took too long to fail ({elapsed:?})"
    );
}

#[tokio::test]
async fn migrate_leaves_no_advisory_lock_behind() {
    let _guard = serial().lock().unwrap();
    let cfg = common::config::AppConfig::from_env();
    tokio::time::timeout(
        Duration::from_secs(120),
        common::db::migrate(&cfg.database_url),
    )
    .await
    .expect("migrate() hung")
    .expect("migrate() failed on a healthy DB");
    let pool = common::db::create_pool(&cfg).await;
    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND granted AND pid <> pg_backend_pid()",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(held, 0, "migrate() leaked a postgres advisory lock");
}
```

Add the dev-dependency in `apps/api-rs/crates/common/Cargo.toml` (append a section if none exists):

```toml
[dev-dependencies]
crc = "3"
```

- [ ] **Step 3: Run the lock test to verify it FAILS (behavioral RED)**

Run with app containers stopped:

```bash
docker compose -f docker-compose-local.yml stop api worker beat-worker
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common --test migrate_lock_test migrate_fails_fast_when_lock_is_held
```

Expected: FAIL after ~25s — the outer `tokio::time::timeout` fires because the rewired (but not yet bounded) `migrate` blocks forever on the held lock. This reproduces the 2026-10-08/09 outage on demand. (`migrate_leaves_no_advisory_lock_behind` is expected to PASS at this stage — the success path already unlocks; keep it as a guard.)

- [ ] **Step 4: Implement the hardening**

In `apps/api-rs/crates/common/src/db.rs`:

1. Extend the import: `use sqlx::{postgres::{PgConnection, PgPoolOptions}, PgPool};`
2. Replace the `migrate_with_statement_timeout` body with:

```rust
pub async fn migrate_with_statement_timeout(database_url: &str, statement_timeout: &str) -> anyhow::Result<()> {
    let timeout = statement_timeout.to_string();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn: &mut PgConnection, _| {
            let timeout = timeout.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET statement_timeout = '{timeout}'"))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;
    let result = sqlx::migrate!("../../migrations").run(&pool).await;
    // Always close: dropping the session releases any advisory lock that
    // `Migrator::run` leaves held on its error paths (it only unlocks on
    // full success), so a failed run can never wedge the next migrator.
    pool.close().await;
    result?;
    Ok(())
}
```

(`statement_timeout` is an internal constant (`"60s"` in production, `"5s"` in tests), never user input, so string interpolation is safe.)

- [ ] **Step 5: Run tests to verify they pass (GREEN)**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common --test migrate_lock_test
```

Expected: both tests PASS in well under 60s (`migrate_fails_fast_when_lock_is_held` returns `Err` at ~5s; the unlock in the test body releases the held lock).

Then run the rest of the affected surface:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common
```

Expected: PASS (uniqueness + lock tests + `migrate_runs`).

Then typecheck the binary crates whose call sites changed:

```bash
cd apps/api-rs && cargo check -p api -p worker
```

Expected: clean (warnings from pre-existing code acceptable; no new warnings in `db.rs` or the two `main.rs` files).

- [ ] **Step 6: Restart the app containers**

```bash
docker compose -f docker-compose-local.yml up -d api worker beat-worker
curl -s -o /dev/null -w "api:%{http_code}\n" http://localhost:8000/health --max-time 10
```

Expected: 200 (the stack was only stopped for the test window).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/common/src/db.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/worker/src/main.rs apps/api-rs/crates/common/tests/migrate_test.rs apps/api-rs/crates/common/tests/migrate_lock_test.rs apps/api-rs/crates/common/Cargo.toml
git commit -m "fix(api-rs): bound and release the migration advisory lock"
```

(Verify with `git status --short` that nothing else — especially `apps/admin/*` — is staged.)

---

### Task 4: Restart verification (the user-visible acceptance)

**Files:** none (operations only — no commit).

- [ ] **Step 0: Rebuild the backend images (new code must be baked in)**

The api/worker containers run a compiled binary with migrations embedded at compile time (`sqlx::migrate!`), so a plain restart would keep running the OLD code. Rebuild detached (Rust LTO link takes 10+ min with no output — not hung, do not abort):

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll until the build finishes (`docker images plane-api-rs:local` timestamp updates / build log shows success), then restart the live server (it holds a Redis connection that gets recreated):

```bash
systemctl --user restart plane-live.service && curl http://localhost:3100/live/health/
```

Expected: live health 200.

- [ ] **Step 1: Record the pre-restart state**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND granted;"`
Expected: `0` (no leaked locks outstanding before the test).

- [ ] **Step 2: Restart twice and check fast healthy startup**

```bash
for i in 1 2; do
  docker compose -f docker-compose-local.yml restart api worker
  for n in $(seq 1 18); do
    code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/health --max-time 10);
    if [ "$code" = "200" ]; then echo "restart $i healthy after ~$((n*5))s"; break; fi;
    sleep 5;
  done
done
```

Expected: both restarts reach `200` well within 90s, with NO manual `pg_terminate_backend` in between (this is the exact scenario that wedged twice on 2026-10-08/09).

- [ ] **Step 3: Confirm no lock contention in the logs**

Run: `docker logs plane-for-itsm-api-1 --since 10m 2>&1 | grep -cE "pg_advisory_lock|migrate failed|migration .* (modified|missing)"`
Expected: `0` — no lock waits, no migrate errors (proves both the rename and the hardening hold under the real race). Note: bare `SELECT pg_advisory_lock($1)` lines at DEBUG level with sub-millisecond `elapsed` are the healthy acquire-and-release cycle, not contention — only `migrate failed`, `VersionMismatch`, or multi-second `elapsed` indicate a problem.

- [ ] **Step 4: Confirm tracking table state**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT version FROM _sqlx_migrations ORDER BY version;"`
Expected: `1`–`16` continuous.

---

### Task 5: Document the rule + final checks

**Files:**

- Modify: `apps/api-rs/migrations/README.md`

- [ ] **Step 1: Fix the rule that caused the duplicate**

In `apps/api-rs/migrations/README.md`, replace the line `New schema changes go in \`0002\_\*.sql\` (plain, non-idempotent deltas).` with:

```md
New schema changes go in a new file with the next free version number
(`0017_<name>.sql`, then `0018_`, …). Version numbers are unique and
immutable: never edit an applied migration and never reuse a version —
`common::db::migrate` aborts the whole run on `VersionMismatch`, which used
to wedge backend startup (2026-10-08/09). The
`migration_versions_are_unique` test enforces this in CI.
```

- [ ] **Step 2: Final verification**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p common
```

Expected: all green (`migration_versions_test`, `migrate_lock_test` ×2, `migrate_test`).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/README.md
git commit -m "docs(api-rs): migration versions are unique and immutable"
```

---

## Verification checklist (end of plan)

- [ ] Uniqueness test RED before rename, GREEN after.
- [ ] `_sqlx_migrations` holds versions 1–16, all `success = t`.
- [ ] Lock test RED (hang → outer timeout) before hardening, GREEN (~5s) after.
- [ ] `migrate_runs` still passes with the new signature.
- [ ] Two consecutive `restart api worker` cycles reach health 200 ≤90s with zero manual lock releases and zero migrate warnings in logs.
- [ ] `cargo test -p common` fully green; `cargo check -p api -p worker` clean.
- [ ] Only plan files committed; `apps/admin/*` and other workstreams untouched.

## Known limitations (accepted for v1)

- `statement_timeout` (60s) also bounds slow DDL statements during migration, not just the lock wait; on very large fresh-DB applies a single statement slower than 60s would abort the run (warn-and-continue, retried next boot). Acceptable: current deltas run in milliseconds.
- Boot semantics unchanged: if the api/worker race is lost, the loser warns and boots potentially before the winner finishes applying — pre-existing behavior, out of scope.
- The `crc` dev-dependency pins the lock-key algorithm to sqlx 0.7.4's `generate_lock_id`; a sqlx upgrade that changes the key breaks `migrate_fails_fast_when_lock_is_held` (it would pass vacuously while holding the wrong key — the test would still pass, but the contention simulation becomes moot). Re-verify the key on sqlx upgrades.
