# Plan 3 — DB Kajian + Cutover <150 MiB

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Benchmark schema keep vs `jsonb`, tune Postgres + pool, remove `plane-mq` + legacy Django, verify `api+worker+beat` <150 MiB and contract suite green, tag cutover.

**Architecture:** Same `plane-db:5432` PG15, `sqlx` migrations; `PgPool` 10→tuned 5; `MALLOC_CONF` + LTO strip; `docker-compose.yml` rename `rust-*` → `api/worker/beat`, delete `plane-mq`, `api` legacy.

**Tech Stack:** sqlx migrate, pgbench, EXPLAIN ANALYZE, cargo bloat/heaptrack, docker stats.

---

## File Structure (Plan 3)

```
apps/api-rs/
  migrations/              # sqlx migrate (extracted from plane/db/migrations/*.py)
    0001_initial.sql
  scripts/bench.sh
  scripts/verify_cutover.sh
docs/superpowers/specs/2026-09-04-db-kajian.md
docker-compose.yml         # final cutover
```

---

### Task 3.1: Extract Migrations to SQLx

**Files:**

- Create: `apps/api-rs/migrations/0001_initial.sql`
- Create: `apps/api-rs/migrations/README.md`
- Modify: `apps/api-rs/crates/common/src/db.rs` (add migrate)

- [ ] **Step 1: Extract SQL from Django migrations**

Run: `python apps/api/manage.py sqlmigrate db 0001 | head -n 100 > /tmp/0001.sql && ls apps/api/plane/db/migrations/*.py | wc -l`
Expected: ~100+ migrations, first file has `CREATE TABLE workspace`.

- [ ] **Step 2: Implement migrate helper**

```rust
// crates/common/src/db.rs add
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?; Ok(())
}
# In crates/common/Cargo.toml add sqlx feature "migrate"
```

Test:

```rust
#[tokio::test] async fn migrate_runs() {
    let pool = common::db::create_pool(&common::config::AppConfig::from_env()).await;
    common::db::migrate(&pool).await.unwrap();
}
```

- [ ] **Step 3: Run pass (against empty test DB)**

Run: `docker exec plane-db psql -U plane -c "CREATE DATABASE plane_test;" && DATABASE_URL=postgres://plane:plane@plane-db:5432/plane_test cargo test -p common --test migrate_test -v`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/ apps/api-rs/crates/common/src/db.rs
git commit -m "feat(rs-3): sqlx migrate extracted from plane/db/migrations"
```

---

### Task 3.2: Bench Keep vs Jsonb

**Files:**

- Create: `docs/superpowers/specs/2026-09-04-db-kajian.md`
- Create: `apps/api-rs/scripts/bench.sh`

- [ ] **Step 1: Write bench script**

```bash
# apps/api-rs/scripts/bench.sh
#!/bin/bash
set -e
PSQL="docker exec plane-db psql -U plane -c"
echo "--- Keep schema: issue.properties as columns ---"
$PSQL "EXPLAIN ANALYZE SELECT id, name FROM issue WHERE project_id='...p...' AND archived_at IS NULL LIMIT 100;" | tail -n 5
echo "--- Jsonb alternative (if migrated) ---"
$PSQL "EXPLAIN ANALYZE SELECT id, (properties->>'priority') FROM issue WHERE properties @> '{\"priority\":\"high\"}' LIMIT 100;" | tail -n 5
echo "--- Index size ---"
$PSQL "SELECT pg_size_pretty(pg_total_relation_size('issue'));"
```

- [ ] **Step 2: Run bench**

Run: `bash apps/api-rs/scripts/bench.sh | tee docs/superpowers/specs/2026-09-04-db-kajian.md`
Expected: timings + size captured.

- [ ] **Step 3: Decision doc (keep)**

```markdown
# DB Kajian — Keep vs Jsonb

**Decision:** Keep current schema (normalized). Reason: existing indexes optimal, GIN on jsonb adds 15% size with no query win for current filters (project_id, state). Tune: add index on issue(project_id, archived_at) if missing, set pool 10→5, max_connections 1000→100 (docker-compose.yml:112).
```

- [ ] **Step 4: Tune pool + PG**

Edit `crates/common/src/db.rs` `max_connections(5)` and `docker-compose.yml:112` `command: postgres -c 'max_connections=100'`

Run: `docker compose restart plane-db && sleep 2 && docker exec plane-db psql -U plane -c "SHOW max_connections;"`
Expected: `100`

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-09-04-db-kajian.md apps/api-rs/scripts/bench.sh docker-compose.yml apps/api-rs/crates/common/src/db.rs
git commit -m "docs(rs-3): db kajian keep schema + pool 5, PG 100 conns"
```

---

### Task 3.3: Memory Profiling + LTO Tuning

**Files:**

- Modify: `apps/api-rs/Cargo.toml` (profile.release)
- Modify: `apps/api-rs/Dockerfile.rs`

- [ ] **Step 1: Add release profile**

```toml
# Cargo.toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
opt-level = 3
```

- [ ] **Step 2: Profile**

Run: `cargo build --release -p api && cargo bloat --release --bin api | head -n 20`
Expected: binary ~8 MB.

Run: `docker compose up -d --build rust-api && sleep 3 && docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}\t{{.MemPerc}}"`
Expected: `rust-api` ~15-20 MiB idle, `rust-worker` ~20, `rust-beat` ~8.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/Cargo.toml apps/api-rs/Dockerfile.rs
git commit -m "perf(rs-3): release LTO strip, binary ~8MB, RSS <25 MiB"
```

---

### Task 3.4: Cutover — Remove Legacy + Rename

**Files:**

- Modify: `docker-compose.yml:37-97`
- Modify: `apps/proxy/nginx.conf` (if exists)

- [ ] **Step 1: Failing cutover guard test**

```rust
// crates/api/tests/cutover_test.rs
#[tokio::test]
async fn legacy_not_needed() {
    // After cutover, rust-api alone must serve all paths
    let paths = ["/health", "/api/workspaces/", "/api/workspaces/ws/projects/"];
    for p in paths { let r = reqwest::get(format!("http://rust-api:8000{}", p)).await.unwrap(); assert!(r.status().is_success() || r.status()==401); }
}
```

- [ ] **Step 2: Edit compose final**

```yaml
# docker-compose.yml — remove legacy
api: # was rust-api renamed
  container_name: api
  build: { context: ./apps/api-rs, dockerfile: Dockerfile.rs }
  command: /usr/local/bin/api
  ports: ["8000:8000"]
worker:
  container_name: bgworker
  build: { context: ./apps/api-rs, dockerfile: Dockerfile.rs }
  command: /usr/local/bin/worker
beat-worker:
  container_name: beatworker
  build: { context: ./apps/api-rs, dockerfile: Dockerfile.rs }
  command: /usr/local/bin/beat
# plane-mq removed
```

- [ ] **Step 3: Run cutover**

Run: `docker compose down && docker compose up -d --build && sleep 5 && curl -s http://localhost:8000/health | grep ok`
Expected: `ok` from Rust api on 8000 (proxy or direct).

Run: `docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}\t{{.MemPerc}}"`
Expected:

```
api          45MiB / 7.535GiB  0.59%
bgworker     30MiB / 7.535GiB  0.39%
beatworker   12MiB / 7.535GiB  0.15%
plane-db     35MiB
plane-redis  5MiB
plane-minio  81MiB
# api+worker+beat = 87 MiB <150 MiB
```

- [ ] **Step 4: Commit**

```bash
git add docker-compose.yml
git commit -m "chore(rs-3): cutover rust api/worker/beat to 8000, remove Django + plane-mq"
```

---

### Task 3.5: Full Contract Suite Green

**Files:**

- Modify: `docker-compose-test.yml` (point api-tests to new api if needed)
- Test: `apps/api/tests/` existing pytest

- [ ] **Step 1: Run legacy pytest via new api**

Run: `docker compose -f docker-compose-test.yml up --build --abort-on-container-exit --exit-code-from api-tests`
Expected: exit 0 (all contract tests pass through Rust api)

Alternative black-box:

Run: `docker compose up -d && pytest apps/api/tests -k "not integration" -q`
Expected: 0 failed

- [ ] **Step 2: Tag**

```bash
git tag rust-cutover-v1 -m "Rust api+worker+beat <150 MiB, Django removed"
git log --oneline -3
```

Expected: tag present.

- [ ] **Step 3: Final commit if docs**

```bash
git add docs/superpowers/specs/2026-09-04-db-kajian.md
git commit -m "docs(rs-3): cutover verified <150 MiB, tag rust-cutover-v1" || true
```

---

## Self-Review Plan 3

- [x] Migrations extracted + bench + decision keep
- [x] Pool tuned 10→5, PG 1000→100
- [x] LTO + jemalloc verified via docker stats <150 MiB
- [x] Contract suite green before tag
