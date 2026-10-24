# External Service Metrics Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let external monitoring agents push pre-aggregated metric buckets (1m/5m/10m/30m/1h) into the service catalog, then show them in a Monitoring tab on the service detail pane.

**Architecture:** Three additive Postgres tables (`metric_sources`, `service_metrics`, `service_metric_buckets`) owned by api-rs. A public token endpoint validates an all-or-nothing batch, resolves services via workspace-level mapping, auto-registers metric definitions, and upserts buckets. Read endpoints reuse existing service visibility gates. A daily worker job deletes buckets per resolution. Web adds a pane tab and an admin settings page.

**Tech Stack:** Rust/Axum/sqlx, Postgres, React/Next.js, MobX permissions, SWR, Recharts via `@plane/propel/charts/line-chart`, i18n JSON.

**Spec:** `docs/superpowers/specs/2026-10-24-service-metrics-ingest-design.md`

---

## File Structure

Backend:

- Create `apps/api-rs/migrations/0024_service_metrics.sql` — schema.
- Create `apps/api-rs/crates/api/src/routes/service_metrics.rs` — validation, ingest, source CRUD, read API.
- Modify `apps/api-rs/crates/api/src/routes/mod.rs` — register module.
- Modify `apps/api-rs/crates/api/src/main.rs` — register routes.
- Create `apps/api-rs/crates/api/tests/service_metrics_test.rs` — DB-backed integration tests.
- Modify `apps/api-rs/crates/worker/src/handlers/cleanup.rs` — retention handler.
- Modify `apps/api-rs/crates/worker/src/handlers/mod.rs` — enable/route cleanup job.
- Modify `apps/api-rs/crates/beat/src/main.rs` — daily schedule.

Frontend:

- Create `packages/types/src/service/metrics.ts`; modify `packages/types/src/service/index.ts`.
- Create `apps/web/core/services/service-metrics.service.ts` + test.
- Create `apps/web/core/components/services/metrics/helpers.ts` + test.
- Create `apps/web/core/hooks/use-service-metrics.ts`.
- Create `apps/web/core/components/services/metrics/{monitoring-panel,metric-selector,range-selector,metric-chart,metric-summary}.tsx`.
- Modify `apps/web/core/components/services/detail/pane/helpers.ts`, `tab-panels/root.tsx`; create `tab-panels/monitoring.tsx`.
- Create settings page under `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/metric-sources/`.
- Create `apps/web/core/components/services/metrics/sources/{root,source-form,index}.tsx`.
- Modify `packages/types/src/settings.ts`, `packages/constants/src/settings/workspace.ts`, `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`.
- Modify `packages/i18n/src/locales/*/service.json` and `workspace-settings.json`.

---

## Task 1: Migration

**Files:**

- Create: `apps/api-rs/migrations/0024_service_metrics.sql`

- [ ] **Step 1: Write the migration**

```sql
-- External service metric buckets (design 2026-10-24-service-metrics-ingest).
-- Additive: workspace-level sources, per-service metric definitions, bucket samples.

CREATE TABLE IF NOT EXISTS public.metric_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    name character varying(255) NOT NULL,
    token character varying(64) NOT NULL,
    is_active boolean NOT NULL DEFAULT true,
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS metric_sources_token_uniq
    ON public.metric_sources (token) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS metric_sources_name_uniq
    ON public.metric_sources (workspace_id, lower(btrim(name))) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS metric_sources_workspace_idx
    ON public.metric_sources (workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.service_metrics (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    metric_key character varying(200) NOT NULL,
    display_name character varying(255) NOT NULL DEFAULT '',
    unit character varying(50) NOT NULL DEFAULT '',
    description text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS service_metrics_service_key_uniq
    ON public.service_metrics (service_id, metric_key);
CREATE INDEX IF NOT EXISTS service_metrics_workspace_idx
    ON public.service_metrics (workspace_id);

CREATE TABLE IF NOT EXISTS public.service_metric_buckets (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    metric_id uuid NOT NULL REFERENCES public.service_metrics(id) ON DELETE CASCADE,
    source_id uuid REFERENCES public.metric_sources(id) ON DELETE SET NULL,
    resolution smallint NOT NULL CHECK (resolution IN (60, 300, 600, 1800, 3600)),
    bucket_start timestamptz NOT NULL,
    count bigint CHECK (count IS NULL OR count >= 0),
    sum double precision,
    min double precision,
    max double precision,
    avg double precision,
    last_value double precision,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT service_metric_buckets_stats_present CHECK (
        count IS NOT NULL OR sum IS NOT NULL OR min IS NOT NULL OR max IS NOT NULL
        OR avg IS NOT NULL OR last_value IS NOT NULL
    ),
    CONSTRAINT service_metric_buckets_min_max CHECK (
        min IS NULL OR max IS NULL OR min <= max
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS service_metric_buckets_series_uniq
    ON public.service_metric_buckets (metric_id, resolution, bucket_start);
CREATE INDEX IF NOT EXISTS service_metric_buckets_service_time_idx
    ON public.service_metric_buckets (service_id, bucket_start DESC);
CREATE INDEX IF NOT EXISTS service_metric_buckets_retention_idx
    ON public.service_metric_buckets (resolution, bucket_start);
```

- [ ] **Step 2: Apply it to the dev database**

```bash
psql "postgres://plane:plane@localhost:5432/plane" \
  -f apps/api-rs/migrations/0024_service_metrics.sql
```

Expected: `CREATE TABLE`, `CREATE INDEX`, no errors. The API also applies it at boot via `common::db::migrate`.

- [ ] **Step 3: Verify**

```bash
psql "postgres://plane:plane@localhost:5432/plane" -c "\d service_metric_buckets"
```

Expected: columns `resolution`, `bucket_start`, `last_value`, and the three indexes.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0024_service_metrics.sql
git commit -m "feat(api): add service metric tables"
```

---

## Task 2: Validation Primitives

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/service_metrics.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `service_metrics.rs` with the types/constants and this test module at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn valid_item() -> IngestItem {
        IngestItem {
            service_id: None,
            service: Some("payment-api".into()),
            key: "http.latency".into(),
            display_name: None,
            unit: Some("ms".into()),
            resolution: 60,
            bucket_start: Utc.with_ymd_and_hms(2026, 10, 24, 10, 0, 0).unwrap(),
            count: Some(60),
            sum: Some(7200.0),
            min: Some(80.0),
            max: Some(250.0),
            avg: Some(120.0),
            last: Some(130.0),
        }
    }

    #[test]
    fn retention_windows_are_tiered() {
        assert_eq!(retention_seconds(60), Some(48 * 3600));
        assert_eq!(retention_seconds(300), Some(14 * 24 * 3600));
        assert_eq!(retention_seconds(600), Some(30 * 24 * 3600));
        assert_eq!(retention_seconds(1800), Some(90 * 24 * 3600));
        assert_eq!(retention_seconds(3600), Some(365 * 24 * 3600));
        assert_eq!(retention_seconds(120), None);
    }

    #[test]
    fn auto_resolution_matches_range() {
        assert_eq!(auto_resolution(3600), Some(60));
        assert_eq!(auto_resolution(24 * 3600), Some(300));
        assert_eq!(auto_resolution(7 * 24 * 3600), Some(600));
        assert_eq!(auto_resolution(30 * 24 * 3600), Some(1800));
        assert_eq!(auto_resolution(90 * 24 * 3600), Some(3600));
        assert_eq!(auto_resolution(91 * 24 * 3600), None);
    }

    #[test]
    fn accepts_aligned_valid_bucket() {
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
        assert!(validate_item(&valid_item(), now).is_empty());
    }

    #[test]
    fn rejects_unaligned_and_out_of_window_buckets() {
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
        let mut item = valid_item();
        item.bucket_start = Utc.with_ymd_and_hms(2026, 10, 24, 10, 0, 30).unwrap();
        assert!(validate_item(&item, now).iter().any(|(f, _)| f == "bucket_start"));

        let mut old = valid_item();
        old.bucket_start = Utc.with_ymd_and_hms(2026, 10, 22, 10, 0, 0).unwrap();
        assert!(validate_item(&old, now).iter().any(|(f, _)| f == "bucket_start"));

        let mut future = valid_item();
        future.bucket_start = Utc.with_ymd_and_hms(2026, 10, 24, 12, 10, 0).unwrap();
        assert!(validate_item(&future, now).iter().any(|(f, _)| f == "bucket_start"));
    }

    #[test]
    fn rejects_invalid_stats() {
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
        let mut item = valid_item();
        item.min = Some(300.0);
        item.max = Some(200.0);
        assert!(validate_item(&item, now).iter().any(|(f, _)| f == "min"));

        let mut empty = valid_item();
        empty.count = None;
        empty.sum = None;
        empty.min = None;
        empty.max = None;
        empty.avg = None;
        empty.last = None;
        assert!(validate_item(&empty, now).iter().any(|(f, _)| f == "stats"));
    }

    #[test]
    fn rejects_invalid_key() {
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
        let mut item = valid_item();
        item.key = "bad key!".into();
        assert!(validate_item(&item, now).iter().any(|(f, _)| f == "key"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd apps/api-rs
cargo test -p api routes::service_metrics::tests -- --nocapture
```

Expected: compile errors for missing `IngestItem`, `Utc`, `retention_seconds`, `auto_resolution`, `validate_item`.

- [ ] **Step 3: Implement the primitives**

At the top of `service_metrics.rs`, add:

```rust
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::{issue_common::is_workspace_admin, project::{deny, missing}, service_access},
    state::AppState,
};

pub const RESOLUTIONS: [i32; 5] = [60, 300, 600, 1800, 3600];
const MAX_BATCH: usize = 1000;
const MAX_ERRORS: usize = 50;
const FUTURE_SKEW_SECS: i64 = 300;
const MAX_RANGE_SECONDS: i64 = 90 * 24 * 3600;

pub fn retention_seconds(resolution: i32) -> Option<i64> {
    match resolution {
        60 => Some(48 * 3600),
        300 => Some(14 * 24 * 3600),
        600 => Some(30 * 24 * 3600),
        1800 => Some(90 * 24 * 3600),
        3600 => Some(365 * 24 * 3600),
        _ => None,
    }
}

pub fn auto_resolution(range_seconds: i64) -> Option<i32> {
    match range_seconds {
        s if s <= 6 * 3600 => Some(60),
        s if s <= 24 * 3600 => Some(300),
        s if s <= 7 * 24 * 3600 => Some(600),
        s if s <= 30 * 24 * 3600 => Some(1800),
        s if s <= 90 * 24 * 3600 => Some(3600),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestItem {
    #[serde(default)]
    pub service_id: Option<Uuid>,
    #[serde(default)]
    pub service: Option<String>,
    pub key: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    pub resolution: i32,
    pub bucket_start: DateTime<Utc>,
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub sum: Option<f64>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub avg: Option<f64>,
    #[serde(default)]
    pub last: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct IngestBody {
    pub metrics: Vec<Value>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetricSourceRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub token: String,
    pub is_active: bool,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_id: Option<Uuid>,
}

pub fn valid_metric_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    key.chars().count() <= 200
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

pub fn validate_item(item: &IngestItem, now: DateTime<Utc>) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    if !valid_metric_key(&item.key) {
        errors.push(("key".into(), "Metric key is invalid".into()));
    }
    if item.display_name.as_deref().map(|s| s.chars().count() > 255).unwrap_or(false) {
        errors.push(("display_name".into(), "display_name must be at most 255 characters".into()));
    }
    if item.unit.as_deref().map(|s| s.chars().count() > 50).unwrap_or(false) {
        errors.push(("unit".into(), "unit must be at most 50 characters".into()));
    }
    let Some(retention) = retention_seconds(item.resolution) else {
        errors.push((
            "resolution".into(),
            "resolution must be one of 60, 300, 600, 1800, 3600".into(),
        ));
        return errors;
    };
    let resolution = i64::from(item.resolution);
    if item.bucket_start.timestamp().rem_euclid(resolution) != 0 {
        errors.push(("bucket_start".into(), format!("bucket_start must align to {resolution}s")));
    }
    if item.bucket_start > now + Duration::seconds(FUTURE_SKEW_SECS) {
        errors.push(("bucket_start".into(), "bucket_start is too far in the future".into()));
    }
    if item.bucket_start < now - Duration::seconds(retention) {
        errors.push(("bucket_start".into(), "bucket_start is older than the retention window".into()));
    }
    for (field, value) in [
        ("sum", item.sum),
        ("min", item.min),
        ("max", item.max),
        ("avg", item.avg),
        ("last", item.last),
    ] {
        if value.is_some_and(|v| !v.is_finite()) {
            errors.push((field.into(), format!("{field} must be a finite number")));
        }
    }
    if let (Some(min), Some(max)) = (item.min, item.max) {
        if min > max {
            errors.push(("min".into(), "min must be less than or equal to max".into()));
        }
    }
    if let (Some(avg), Some(min)) = (item.avg, item.min) {
        if avg < min - 1e-9 {
            errors.push(("avg".into(), "avg must be within [min, max]".into()));
        }
    }
    if let (Some(avg), Some(max)) = (item.avg, item.max) {
        if avg > max + 1e-9 {
            errors.push(("avg".into(), "avg must be within [min, max]".into()));
        }
    }
    if item.count.is_none()
        && item.sum.is_none()
        && item.min.is_none()
        && item.max.is_none()
        && item.avg.is_none()
        && item.last.is_none()
    {
        errors.push(("stats".into(), "At least one statistic is required".into()));
    }
    errors
}
```

- [ ] **Step 4: Register the module**

In `apps/api-rs/crates/api/src/routes/mod.rs`, add `pub mod service_metrics;` next to `pub mod service_health;`.

- [ ] **Step 5: Run the tests**

```bash
cd apps/api-rs
cargo test -p api routes::service_metrics::tests -- --nocapture
```

Expected: 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service_metrics.rs apps/api-rs/crates/api/src/routes/mod.rs
git commit -m "feat(api): add service metric validation"
```

---

## Task 3: Ingest Endpoint

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service_metrics.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Create: `apps/api-rs/crates/api/tests/service_metrics_test.rs`

- [ ] **Step 1: Write the failing integration tests**

Create `apps/api-rs/crates/api/tests/service_metrics_test.rs` with the harness and ingest tests:

```rust
//! DB-backed service metrics tests. Run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test service_metrics_test -- --test-threads=1`

use api::routes::service_metrics::ingest;
use api::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use common::config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
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

async fn insert_workspace_member(pool: &PgPool, user_id: Uuid, workspace_id: Uuid, role: i16) {
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', '{}', true)",
    )
    .bind(role)
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project(pool: &PgPool, project_id: Uuid, workspace_id: Uuid, name: &str, identifier: &str) {
    sqlx::query(
        "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
         identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
         intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
         is_issue_type_enabled, guest_view_all_features, timezone) \
         VALUES ($1, now(), now(), $2, '', 2, $3, $4, false, false, false, false, false, \
         30, 30, '{}'::jsonb, false, false, false, 'UTC')",
    )
    .bind(project_id)
    .bind(name)
    .bind(identifier)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project");
}

async fn insert_project_member(pool: &PgPool, user_id: Uuid, project_id: Uuid, workspace_id: Uuid, role: i16) {
    sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
         view_props, default_props, sort_order, preferences, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, true, '{}', '{}', 65535, '{}', now(), now())",
    )
    .bind(user_id)
    .bind(role)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project member");
}

async fn insert_service(pool: &PgPool, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, workspace_id, name, description, description_html, status, \
         criticality, \"type\", sort_order, created_at, updated_at) \
         VALUES ($1, $2, $3, '', '', 'active', 'medium', 'internal', 65535, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("scratch service");
    id
}

async fn link_service(pool: &PgPool, service_id: Uuid, project_id: Uuid, workspace_id: Uuid) {
    sqlx::query(
        "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, link_type, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 'owner', now(), now())",
    )
    .bind(workspace_id)
    .bind(service_id)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("scratch service link");
}

async fn insert_source(pool: &PgPool, workspace_id: Uuid, token: &str, config: Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO metric_sources (id, workspace_id, name, token, is_active, config, \
         created_at, updated_at) VALUES ($1, $2, $3, $4, true, $5, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(format!("source-{id}"))
    .bind(token)
    .bind(config)
    .execute(pool)
    .await
    .expect("scratch metric source");
    id
}

struct Scratch {
    workspace_id: Uuid,
    admin: Uuid,
    member: Uuid,
    project_id: Uuid,
    service_id: Uuid,
    slug: String,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let workspace_id = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let member = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let slug = format!("metrics-{}", Uuid::new_v4().simple());
        insert_user(pool, admin, &format!("{slug}-admin")).await;
        insert_user(pool, member, &format!("{slug}-member")).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Metrics Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin)
        .execute(pool)
        .await
        .expect("scratch workspace");
        insert_workspace_member(pool, admin, workspace_id, 20).await;
        insert_workspace_member(pool, member, workspace_id, 15).await;
        insert_project(pool, project_id, workspace_id, "Metrics Project", "MTR").await;
        insert_project_member(pool, member, project_id, workspace_id, 15).await;
        let service_id = insert_service(pool, workspace_id, "Payment API").await;
        link_service(pool, service_id, project_id, workspace_id).await;
        Self { workspace_id, admin, member, project_id, service_id, slug }
    }
}

fn aligned_now(resolution: i64) -> String {
    let now = Utc::now().timestamp();
    let ts = now - now.rem_euclid(resolution);
    DateTime::<Utc>::from_timestamp(ts, 0).unwrap().to_rfc3339()
}

#[tokio::test]
async fn ingest_creates_metric_and_bucket_from_service_map() {
    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;

    let (status, Json(body)) = ingest(
        State(state().await),
        Path(token),
        Json(json!({"metrics": [{
            "service": "payment-api",
            "key": "http.latency",
            "unit": "ms",
            "resolution": 60,
            "bucket_start": aligned_now(60),
            "count": 60, "sum": 7200, "min": 80, "max": 250, "avg": 120, "last": 130
        }]})),
    )
    .await
    .unwrap();

    assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    assert_eq!(body["accepted"], 1);
    assert_eq!(body["metrics_created"], 1);
    let avg: Option<f64> =
        sqlx::query_scalar("SELECT \"avg\" FROM service_metric_buckets WHERE service_id = $1")
            .bind(sc.service_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(avg, Some(120.0));
}

#[tokio::test]
async fn ingest_upserts_same_bucket_last_write_wins() {
    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;
    let bucket = aligned_now(60);
    for avg in [100.0, 200.0] {
        let (status, _) = ingest(
            State(state().await),
            Path(token.clone()),
            Json(json!({"metrics": [{
                "service": "payment-api", "key": "http.latency", "unit": "ms",
                "resolution": 60, "bucket_start": bucket,
                "count": 60, "avg": avg
            }]})),
        )
        .await
        .unwrap();
        assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    }
    let (count, avg): (i64, f64) = sqlx::query_as(
        "SELECT count(*), max(\"avg\") FROM service_metric_buckets WHERE service_id = $1",
    )
    .bind(sc.service_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(avg, 200.0);
}

#[tokio::test]
async fn ingest_batch_is_atomic_on_invalid_item() {
    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;

    let (status, Json(body)) = ingest(
        State(state().await),
        Path(token),
        Json(json!({"metrics": [
            {"service": "payment-api", "key": "ok.metric", "resolution": 60,
             "bucket_start": aligned_now(60), "avg": 1.0},
            {"service": "payment-api", "key": "bad key!", "resolution": 60,
             "bucket_start": aligned_now(60), "avg": 1.0}
        ]})),
    )
    .await
    .unwrap();

    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(body["errors"][0]["index"], 1);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM service_metric_buckets WHERE service_id = $1")
        .bind(sc.service_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
```

Expected: compile errors for missing `ingest`.

- [ ] **Step 3: Implement the ingest handler**

Append to `service_metrics.rs`:

```rust
const SOURCE_COLS: &str = "s.id, s.workspace_id, s.name, s.token, s.is_active, s.config, \
    s.created_at, s.updated_at, s.created_by_id";

fn resolve_service_id(source: &MetricSourceRow, item: &IngestItem) -> Result<Uuid, String> {
    if let Some(id) = item.service_id {
        return Ok(id);
    }
    let Some(key) = item.service.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return Err("service or service_id is required".into());
    };
    if key.chars().count() > 200 {
        return Err("service must be at most 200 characters".into());
    }
    if let Some(id) = source
        .config
        .get("service_map")
        .and_then(Value::as_object)
        .and_then(|map| map.get(key))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        return Ok(id);
    }
    if let Some(id) = source
        .config
        .get("fallback_service_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        return Ok(id);
    }
    Err(format!("Unknown service '{key}'"))
}

fn error_response(mut errors: Vec<Value>) -> (StatusCode, Json<Value>) {
    let truncated = errors.len() > MAX_ERRORS;
    errors.truncate(MAX_ERRORS);
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"errors": errors, "truncated": truncated})),
    )
}

pub async fn ingest(
    State(st): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let source: Option<MetricSourceRow> = sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM metric_sources s \
         JOIN workspaces w ON w.id = s.workspace_id \
         WHERE s.token = $1 AND s.deleted_at IS NULL"
    ))
    .bind(&token)
    .fetch_optional(&st.pool)
    .await?;
    let Some(source) = source else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Unknown token"}))));
    };
    if !source.is_active {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": "Metric source is inactive"}))));
    }

    let parsed: IngestBody = match serde_json::from_value(body) {
        Ok(value) => value,
        Err(error) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid payload: {error}")})),
            ))
        }
    };
    if parsed.metrics.is_empty() || parsed.metrics.len() > MAX_BATCH {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("metrics must contain 1..{MAX_BATCH} items")})),
        ));
    }

    let now = Utc::now();
    let mut errors: Vec<Value> = Vec::new();
    let mut items: Vec<IngestItem> = Vec::with_capacity(parsed.metrics.len());
    for (index, raw) in parsed.metrics.into_iter().enumerate() {
        match serde_json::from_value::<IngestItem>(raw) {
            Ok(item) => {
                for (field, message) in validate_item(&item, now) {
                    errors.push(json!({"index": index, "field": field, "message": message}));
                }
                items.push(item);
            }
            Err(error) => {
                errors.push(json!({"index": index, "field": "", "message": error.to_string()}));
            }
        }
    }
    if !errors.is_empty() {
        return Ok(error_response(errors));
    }

    let mut requested: Vec<Uuid> = Vec::with_capacity(items.len());
    let mut resolved: Vec<(Uuid, &IngestItem)> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match resolve_service_id(&source, item) {
            Ok(id) => {
                requested.push(id);
                resolved.push((id, item));
            }
            Err(message) => errors.push(json!({"index": index, "field": "service", "message": message})),
        }
    }
    if !errors.is_empty() {
        return Ok(error_response(errors));
    }
    requested.sort();
    requested.dedup();

    let live: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM services WHERE workspace_id = $1 AND deleted_at IS NULL AND id = ANY($2)",
    )
    .bind(source.workspace_id)
    .bind(&requested)
    .fetch_all(&st.pool)
    .await?;
    let live: HashSet<Uuid> = live.into_iter().collect();
    for (index, (service_id, _)) in resolved.iter().enumerate() {
        if !live.contains(service_id) {
            errors.push(json!({
                "index": index,
                "field": "service",
                "message": "Service not found in this workspace"
            }));
        }
    }
    if !errors.is_empty() {
        return Ok(error_response(errors));
    }

    let mut tx = st.pool.begin().await?;
    let mut accepted = 0usize;
    let mut metrics_created = 0usize;
    for (service_id, item) in resolved {
        let display_name = item.display_name.as_deref().map(str::trim).unwrap_or("");
        let unit = item.unit.as_deref().map(str::trim).unwrap_or("");
        let (metric_id, created): (Uuid, bool) = sqlx::query_as(
            "INSERT INTO service_metrics (id, workspace_id, service_id, metric_key, display_name, unit) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5) \
             ON CONFLICT (service_id, metric_key) DO UPDATE SET \
               display_name = CASE WHEN service_metrics.display_name = '' THEN EXCLUDED.display_name \
                                   ELSE service_metrics.display_name END, \
               unit = CASE WHEN service_metrics.unit = '' THEN EXCLUDED.unit \
                           ELSE service_metrics.unit END, \
               updated_at = now() \
             RETURNING id, (xmax = 0) AS inserted",
        )
        .bind(source.workspace_id)
        .bind(service_id)
        .bind(&item.key)
        .bind(display_name)
        .bind(unit)
        .fetch_one(&mut *tx)
        .await?;
        if created {
            metrics_created += 1;
        }
        sqlx::query(
            "INSERT INTO service_metric_buckets \
             (service_id, metric_id, source_id, resolution, bucket_start, \"count\", \"sum\", \
              \"min\", \"max\", \"avg\", last_value) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
             ON CONFLICT (metric_id, resolution, bucket_start) DO UPDATE SET \
               \"count\" = EXCLUDED.\"count\", \"sum\" = EXCLUDED.\"sum\", \
               \"min\" = EXCLUDED.\"min\", \"max\" = EXCLUDED.\"max\", \
               \"avg\" = EXCLUDED.\"avg\", last_value = EXCLUDED.last_value, \
               source_id = EXCLUDED.source_id, ingested_at = now()",
        )
        .bind(service_id)
        .bind(metric_id)
        .bind(source.id)
        .bind(i16::try_from(item.resolution).unwrap_or(60))
        .bind(item.bucket_start)
        .bind(item.count)
        .bind(item.sum)
        .bind(item.min)
        .bind(item.max)
        .bind(item.avg)
        .bind(item.last)
        .execute(&mut *tx)
        .await?;
        accepted += 1;
    }
    tx.commit().await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"accepted": accepted, "metrics_created": metrics_created})),
    ))
}
```

- [ ] **Step 4: Register the ingest route**

In `apps/api-rs/crates/api/src/main.rs`, near the existing `/api/inbound/alertmanager/:token/` route, add:

```rust
.route(
    "/api/inbound/metrics/:token/",
    post(routes::service_metrics::ingest),
)
```

- [ ] **Step 5: Run the tests**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service_metrics.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/service_metrics_test.rs
git commit -m "feat(api): ingest external service metric buckets"
```

---

## Task 4: Source Management API

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service_metrics.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/service_metrics_test.rs`

- [ ] **Step 1: Add failing tests**

Append to `service_metrics_test.rs`:

```rust
#[tokio::test]
async fn source_crud_requires_workspace_admin_and_rotate_invalidates_old_token() {
    use api::routes::service_metrics::{create_source, list_sources, rotate_source, CreateSource};
    use api::middleware::auth::AuthUser;

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;

    let (status, Json(_)) = list_sources(
        State(state().await),
        AuthUser(sc.member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

    let (status, Json(created)) = create_source(
        State(state().await),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Json(CreateSource {
            name: "Prometheus".into(),
            is_active: None,
            config: Some(json!({"service_map": {"payment-api": sc.service_id}})),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let old_token = created["token"].as_str().unwrap().to_string();
    assert!(old_token.starts_with("plane_ms_"));

    let source_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let (status, Json(rotated)) = rotate_source(
        State(state().await),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), source_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    let new_token = rotated["token"].as_str().unwrap().to_string();
    assert_ne!(old_token, new_token);

    let body = json!({"metrics": [{"service": "payment-api", "key": "x", "resolution": 60,
        "bucket_start": aligned_now(60), "avg": 1.0}]});
    let (status, _) = ingest(State(state().await), Path(old_token), Json(body.clone())).await.unwrap();
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    let (status, _) = ingest(State(state().await), Path(new_token), Json(body)).await.unwrap();
    assert_eq!(status, axum::http::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn source_config_rejects_service_from_another_workspace() {
    use api::routes::service_metrics::{create_source, CreateSource};
    use api::middleware::auth::AuthUser;

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let other = Scratch::new(&pool).await;

    let (status, Json(body)) = create_source(
        State(state().await),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Json(CreateSource {
            name: "Bad mapping".into(),
            is_active: None,
            config: Some(json!({"service_map": {"other": other.service_id}})),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Service does not belong to this workspace");
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
```

Expected: compile errors for missing source handlers.

- [ ] **Step 3: Implement source handlers**

Append to `service_metrics.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct CreateSource {
    pub name: String,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PatchSource {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub config: Option<Value>,
}

fn source_json(row: &MetricSourceRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "name": row.name,
        "token": row.token,
        "is_active": row.is_active,
        "config": row.config,
        "created_by": row.created_by_id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

pub async fn validate_source_config(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    config: &Value,
) -> Result<(), String> {
    let Some(object) = config.as_object() else {
        return Err("config must be an object".into());
    };
    for key in object.keys() {
        if key != "service_map" && key != "fallback_service_id" {
            return Err(format!("Unknown config key '{key}'"));
        }
    }
    let mut ids: Vec<Uuid> = Vec::new();
    if let Some(map) = object.get("service_map") {
        let Some(map) = map.as_object() else {
            return Err("service_map must be an object".into());
        };
        if map.len() > 500 {
            return Err("service_map must have at most 500 entries".into());
        }
        for (key, value) in map {
            let key = key.trim();
            if key.is_empty() || key.chars().count() > 200 {
                return Err("service_map keys must be 1..200 characters".into());
            }
            let Some(id) = value.as_str().and_then(|s| Uuid::parse_str(s).ok()) else {
                return Err("service_map values must be service ids".into());
            };
            ids.push(id);
        }
    }
    if let Some(value) = object.get("fallback_service_id") {
        if !value.is_null() {
            let Some(id) = value.as_str().and_then(|s| Uuid::parse_str(s).ok()) else {
                return Err("fallback_service_id must be a service id or null".into());
            };
            ids.push(id);
        }
    }
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(());
    }
    let live: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM services WHERE workspace_id = $1 AND deleted_at IS NULL AND id = ANY($2)",
    )
    .bind(workspace_id)
    .bind(&ids)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if live.len() != ids.len() {
        return Err("Service does not belong to this workspace".into());
    }
    Ok(())
}

async fn fetch_source(
    pool: &sqlx::PgPool,
    slug: &str,
    pk: Uuid,
) -> Result<Option<MetricSourceRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM metric_sources s \
         JOIN workspaces w ON w.id = s.workspace_id \
         WHERE s.id = $1 AND w.slug = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub async fn list_sources(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let rows: Vec<MetricSourceRow> = sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM metric_sources s \
         JOIN workspaces w ON w.id = s.workspace_id \
         WHERE w.slug = $1 AND s.deleted_at IS NULL ORDER BY s.created_at ASC"
    ))
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(source_json).collect())),
    ))
}

pub async fn create_source(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateSource>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 255 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Name must be 1..255 characters"})),
        ));
    }
    let workspace_id: Uuid = sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await?;
    let config = body.config.unwrap_or_else(|| json!({}));
    if let Err(message) = validate_source_config(&st.pool, workspace_id, &config).await {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
    }
    let id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO metric_sources (id, workspace_id, name, token, is_active, config, \
         created_by_id, updated_by_id, created_at, updated_at) \
         VALUES ($1, $2, $3, 'plane_ms_' || replace(gen_random_uuid()::text, '-', ''), $4, $5, \
         $6, $6, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(&name)
    .bind(body.is_active.unwrap_or(true))
    .bind(&config)
    .bind(auth.0)
    .execute(&st.pool)
    .await;
    if let Err(error) = inserted {
        if error.as_database_error().and_then(|e| e.constraint()).is_some() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "A metric source with this name already exists"})),
            ));
        }
        return Err(error.into());
    }
    let row = fetch_source(&st.pool, &slug, id).await?.unwrap();
    Ok((StatusCode::CREATED, Json(source_json(&row))))
}

pub async fn patch_source(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<PatchSource>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(current) = fetch_source(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    let name = body.name.as_ref().map(|name| name.trim().to_string());
    if matches!(&name, Some(name) if name.is_empty() || name.chars().count() > 255) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Name must be 1..255 characters"})),
        ));
    }
    if let Some(config) = &body.config {
        if let Err(message) = validate_source_config(&st.pool, current.workspace_id, config).await {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    let config = body.config.unwrap_or(current.config);
    let updated = sqlx::query(
        "UPDATE metric_sources SET name = COALESCE($1, name), is_active = COALESCE($2, is_active), \
         config = $3, updated_by_id = $4, updated_at = now() \
         WHERE id = $5 AND deleted_at IS NULL",
    )
    .bind(name)
    .bind(body.is_active)
    .bind(&config)
    .bind(auth.0)
    .bind(pk)
    .execute(&st.pool)
    .await;
    if let Err(error) = updated {
        if error.as_database_error().and_then(|e| e.constraint()).is_some() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "A metric source with this name already exists"})),
            ));
        }
        return Err(error.into());
    }
    let row = fetch_source(&st.pool, &slug, pk).await?.unwrap();
    Ok((StatusCode::OK, Json(source_json(&row))))
}

pub async fn destroy_source(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let result = sqlx::query(
        "UPDATE metric_sources s SET deleted_at = now(), updated_by_id = $1, updated_at = now() \
         FROM workspaces w WHERE w.id = s.workspace_id AND w.slug = $2 \
         AND s.id = $3 AND s.deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::OK, Json(json!({"ok": true}))))
}

pub async fn rotate_source(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let token: Option<String> = sqlx::query_scalar(
        "UPDATE metric_sources s \
         SET token = 'plane_ms_' || replace(gen_random_uuid()::text, '-', ''), \
         updated_by_id = $1, updated_at = now() \
         FROM workspaces w WHERE w.id = s.workspace_id AND w.slug = $2 \
         AND s.id = $3 AND s.deleted_at IS NULL RETURNING s.token",
    )
    .bind(auth.0)
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match token {
        Some(token) => Ok((StatusCode::OK, Json(json!({"id": pk, "token": token})))),
        None => Ok(missing()),
    }
}
```

- [ ] **Step 4: Register source routes**

In `apps/api-rs/crates/api/src/main.rs`, near the workspace services routes, add:

```rust
.route(
    "/api/workspaces/:slug/metric-sources/",
    get(routes::service_metrics::list_sources).post(routes::service_metrics::create_source),
)
.route(
    "/api/workspaces/:slug/metric-sources/:pk/",
    patch(routes::service_metrics::patch_source).delete(routes::service_metrics::destroy_source),
)
.route(
    "/api/workspaces/:slug/metric-sources/:pk/rotate/",
    post(routes::service_metrics::rotate_source),
)
```

- [ ] **Step 5: Run the tests**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
```

Expected: 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service_metrics.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/service_metrics_test.rs
git commit -m "feat(api): manage workspace metric sources"
```

---

## Task 5: Read API

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service_metrics.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/service_metrics_test.rs`

- [ ] **Step 1: Add failing tests**

Append to `service_metrics_test.rs`:

```rust
#[tokio::test]
async fn read_endpoints_scope_by_service_visibility_and_auto_resolve() {
    use api::middleware::auth::AuthUser;
    use api::routes::service_metrics::{buckets, list_metrics, BucketQuery};

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;
    ingest(
        State(state().await),
        Path(token),
        Json(json!({"metrics": [{
            "service": "payment-api", "key": "http.latency", "unit": "ms",
            "resolution": 300, "bucket_start": aligned_now(300),
            "count": 60, "avg": 120
        }]})),
    )
    .await
    .unwrap();

    let (status, Json(defs)) = list_metrics(
        State(state().await),
        AuthUser(sc.member),
        Path((sc.slug.clone(), sc.service_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    let metric_id = Uuid::parse_str(defs["metrics"][0]["id"].as_str().unwrap()).unwrap();
    assert_eq!(defs["metrics"][0]["key"], "http.latency");

    let (status, _, Json(series)) = buckets(
        State(state().await),
        AuthUser(sc.member),
        Path((sc.slug.clone(), sc.service_id, metric_id)),
        Query(BucketQuery { from: None, to: None, resolution: None }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(series["resolution"], 300);
    assert_eq!(series["buckets"][0]["avg"], 120.0);

    let outsider = Uuid::new_v4();
    insert_user(&pool, outsider, &format!("outsider-{}", Uuid::new_v4().simple())).await;
    let (status, Json(_)) = list_metrics(
        State(state().await),
        AuthUser(outsider),
        Path((sc.slug.clone(), sc.service_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

    let other_service = insert_service(&pool, sc.workspace_id, "Other").await;
    let (status, _, Json(_)) = buckets(
        State(state().await),
        AuthUser(sc.admin),
        Path((sc.slug, other_service, metric_id)),
        Query(BucketQuery { from: None, to: None, resolution: None }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
}
```

Add `Query` to the test imports:

```rust
use axum::extract::{Path, Query, State};
```

- [ ] **Step 2: Run to verify failure**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
```

Expected: compile errors for missing `buckets`, `list_metrics`, `BucketQuery`.

- [ ] **Step 3: Implement read handlers**

Append to `service_metrics.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct BucketQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub resolution: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
struct MetricWithLast {
    id: Uuid,
    service_id: Uuid,
    metric_key: String,
    display_name: String,
    unit: String,
    description: String,
    bucket_start: Option<DateTime<Utc>>,
    resolution: Option<i16>,
    count: Option<i64>,
    sum: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    avg: Option<f64>,
    last_value: Option<f64>,
}

#[derive(Debug, sqlx::FromRow)]
struct MetricRow {
    id: Uuid,
    service_id: Uuid,
    metric_key: String,
    display_name: String,
    unit: String,
    description: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BucketRow {
    bucket_start: DateTime<Utc>,
    count: Option<i64>,
    sum: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    avg: Option<f64>,
    last_value: Option<f64>,
}

fn metric_json(row: &MetricRow) -> Value {
    json!({
        "id": row.id,
        "service_id": row.service_id,
        "key": row.metric_key,
        "display_name": row.display_name,
        "unit": row.unit,
        "description": row.description,
    })
}

pub async fn list_metrics(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::full_visible(&st.pool, auth.0, &slug, pk).await? {
        return Ok(deny());
    }
    let rows: Vec<MetricWithLast> = sqlx::query_as(
        "SELECT m.id, m.service_id, m.metric_key, m.display_name, m.unit, m.description, \
                b.bucket_start, b.resolution, b.\"count\", b.\"sum\", b.\"min\", b.\"max\", \
                b.\"avg\", b.last_value \
         FROM service_metrics m \
         LEFT JOIN LATERAL ( \
           SELECT bucket_start, resolution, \"count\", \"sum\", \"min\", \"max\", \"avg\", last_value \
           FROM service_metric_buckets b WHERE b.metric_id = m.id \
           ORDER BY b.bucket_start DESC LIMIT 1 \
         ) b ON true \
         WHERE m.service_id = $1 ORDER BY m.metric_key",
    )
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;

    let metrics: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let last_bucket = row.bucket_start.map(|start| {
                json!({
                    "bucket_start": start,
                    "resolution": row.resolution,
                    "count": row.count,
                    "sum": row.sum,
                    "min": row.min,
                    "max": row.max,
                    "avg": row.avg,
                    "last": row.last_value,
                })
            });
            json!({
                "id": row.id,
                "service_id": row.service_id,
                "key": row.metric_key,
                "display_name": row.display_name,
                "unit": row.unit,
                "description": row.description,
                "last_bucket": last_bucket,
            })
        })
        .collect();
    Ok((StatusCode::OK, Json(json!({"metrics": metrics}))))
}

type BucketsResponse = (
    StatusCode,
    [(header::HeaderName, &'static str); 1],
    Json<Value>,
);

fn parse_timestamp(value: Option<&str>, default: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    match value {
        Some(raw) => DateTime::parse_from_rfc3339(raw)
            .map(|parsed| parsed.with_timezone(&Utc))
            .map_err(|_| format!("Invalid timestamp '{raw}'")),
        None => Ok(default),
    }
}

pub async fn buckets(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk, metric_id)): Path<(String, Uuid, Uuid)>,
    Query(params): Query<BucketQuery>,
) -> Result<BucketsResponse, common::errors::AppError> {
    if !service_access::full_visible(&st.pool, auth.0, &slug, pk).await? {
        return Ok((
            StatusCode::FORBIDDEN,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"error": "You don't have the required permissions."})),
        ));
    }
    let metric: Option<MetricRow> = sqlx::query_as(
        "SELECT id, service_id, metric_key, display_name, unit, description \
         FROM service_metrics WHERE id = $1 AND service_id = $2",
    )
    .bind(metric_id)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(metric) = metric else {
        return Ok((
            StatusCode::NOT_FOUND,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"error": "The required object does not exist."})),
        ));
    };

    let now = Utc::now();
    let to = match parse_timestamp(params.to.as_deref(), now) {
        Ok(value) => value,
        Err(message) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                [(header::CACHE_CONTROL, "no-store")],
                Json(json!({"error": message})),
            ))
        }
    };
    let from = match parse_timestamp(params.from.as_deref(), to - Duration::hours(24)) {
        Ok(value) => value,
        Err(message) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                [(header::CACHE_CONTROL, "no-store")],
                Json(json!({"error": message})),
            ))
        }
    };
    let range_seconds = (to - from).num_seconds();
    if range_seconds <= 0 || range_seconds > MAX_RANGE_SECONDS {
        return Ok((
            StatusCode::BAD_REQUEST,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"error": "Range must be between 0 and 90 days"})),
        ));
    }
    let resolution = match params.resolution {
        Some(resolution) => {
            let Some(retention) = retention_seconds(resolution) else {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    [(header::CACHE_CONTROL, "no-store")],
                    Json(json!({"error": "Invalid resolution"})),
                ));
            };
            if from < now - Duration::seconds(retention) {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    [(header::CACHE_CONTROL, "no-store")],
                    Json(json!({"error": "Resolution is not retained for this range"})),
                ));
            }
            resolution
        }
        None => auto_resolution(range_seconds).unwrap_or(3600),
    };

    let rows: Vec<BucketRow> = sqlx::query_as(
        "SELECT bucket_start, \"count\", \"sum\", \"min\", \"max\", \"avg\", last_value \
         FROM service_metric_buckets \
         WHERE metric_id = $1 AND resolution = $2 AND bucket_start >= $3 AND bucket_start < $4 \
         ORDER BY bucket_start",
    )
    .bind(metric_id)
    .bind(i16::try_from(resolution).unwrap_or(60))
    .bind(from)
    .bind(to)
    .fetch_all(&st.pool)
    .await?;

    let buckets: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            json!({
                "bucket_start": row.bucket_start,
                "count": row.count,
                "sum": row.sum,
                "min": row.min,
                "max": row.max,
                "avg": row.avg,
                "last": row.last_value,
            })
        })
        .collect();
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "metric": metric_json(&metric),
            "resolution": resolution,
            "from": from,
            "to": to,
            "buckets": buckets,
        })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct PatchMetric {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn patch_metric(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk, metric_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchMetric>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let in_workspace: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services s JOIN workspaces w ON w.id = s.workspace_id \
         WHERE s.id = $1 AND w.slug = $2 AND s.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !in_workspace {
        return Ok(missing());
    }
    let display_name = body.display_name.map(|value| value.trim().to_string());
    if matches!(&display_name, Some(value) if value.chars().count() > 255) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "display_name must be at most 255 characters"})),
        ));
    }
    let unit = body.unit.map(|value| value.trim().to_string());
    if matches!(&unit, Some(value) if value.chars().count() > 50) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "unit must be at most 50 characters"})),
        ));
    }
    let updated: Option<MetricRow> = sqlx::query_as(
        "UPDATE service_metrics SET \
           display_name = COALESCE($1, display_name), \
           unit = COALESCE($2, unit), \
           description = COALESCE($3, description), \
           updated_at = now() \
         WHERE id = $4 AND service_id = $5 \
         RETURNING id, service_id, metric_key, display_name, unit, description",
    )
    .bind(display_name)
    .bind(unit)
    .bind(body.description)
    .bind(metric_id)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match updated {
        Some(row) => Ok((StatusCode::OK, Json(metric_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn destroy_metric(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk, metric_id)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let in_workspace: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services s JOIN workspaces w ON w.id = s.workspace_id \
         WHERE s.id = $1 AND w.slug = $2 AND s.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !in_workspace {
        return Ok(missing());
    }
    let deleted: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM service_metrics WHERE id = $1 AND service_id = $2 RETURNING id",
    )
    .bind(metric_id)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match deleted {
        Some(_) => Ok((StatusCode::OK, Json(json!({"ok": true})))),
        None => Ok(missing()),
    }
}
```

- [ ] **Step 4: Register read routes**

In `apps/api-rs/crates/api/src/main.rs`, add:

```rust
.route(
    "/api/workspaces/:slug/services/:pk/metrics/",
    get(routes::service_metrics::list_metrics),
)
.route(
    "/api/workspaces/:slug/services/:pk/metrics/:metric_id/",
    patch(routes::service_metrics::patch_metric).delete(routes::service_metrics::destroy_metric),
)
.route(
    "/api/workspaces/:slug/services/:pk/metrics/:metric_id/buckets/",
    get(routes::service_metrics::buckets),
)
```

- [ ] **Step 5: Run the tests**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
```

Expected: 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service_metrics.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/service_metrics_test.rs
git commit -m "feat(api): read service metric definitions and buckets"
```

---

## Task 6: Retention Job

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/cleanup.rs`
- Modify: `apps/api-rs/crates/worker/src/handlers/mod.rs`
- Modify: `apps/api-rs/crates/beat/src/main.rs`

- [ ] **Step 1: Add the failing unit test**

Append to `apps/api-rs/crates/worker/src/handlers/cleanup.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::retention_interval;

    #[test]
    fn retention_intervals_cover_every_resolution() {
        assert_eq!(retention_interval(60), Some("48 hours"));
        assert_eq!(retention_interval(300), Some("14 days"));
        assert_eq!(retention_interval(600), Some("30 days"));
        assert_eq!(retention_interval(1800), Some("90 days"));
        assert_eq!(retention_interval(3600), Some("365 days"));
        assert_eq!(retention_interval(120), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd apps/api-rs
cargo test -p worker retention_intervals -- --nocapture
```

Expected: compile error for missing `retention_interval`.

- [ ] **Step 3: Implement the handler**

Append to `cleanup.rs`:

```rust
pub fn retention_interval(resolution: i32) -> Option<&'static str> {
    match resolution {
        60 => Some("48 hours"),
        300 => Some("14 days"),
        600 => Some("30 days"),
        1800 => Some("90 days"),
        3600 => Some("365 days"),
        _ => None,
    }
}

pub async fn service_metric_buckets(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for resolution in [60_i32, 300, 600, 1800, 3600] {
        let Some(interval) = retention_interval(resolution) else {
            continue;
        };
        let mut total = 0_u64;
        loop {
            let result = sqlx::query(
                "DELETE FROM service_metric_buckets WHERE id IN ( \
                   SELECT id FROM service_metric_buckets \
                   WHERE resolution = $1 AND bucket_start < now() - $2::interval \
                   LIMIT 10000)",
            )
            .bind(i16::try_from(resolution).unwrap_or(60))
            .bind(interval)
            .execute(pool)
            .await?;
            let deleted = result.rows_affected();
            total += deleted;
            if deleted < 10_000 {
                break;
            }
        }
        tracing::info!(resolution, deleted = total, "service metric buckets cleaned");
    }
    Ok(())
}
```

- [ ] **Step 4: Wire worker and beat**

In `apps/api-rs/crates/worker/src/handlers/mod.rs`, add to `dispatch_with_pool`:

```rust
"cleanup.service_metric_buckets" => cleanup::service_metric_buckets(pool).await,
```

Add to `is_enabled_job`:

```rust
"cleanup.service_metric_buckets"
```

Add to `handle_by_id`:

```rust
"cleanup.service_metric_buckets" => cleanup::service_metric_buckets(pool).await,
```

In `apps/api-rs/crates/beat/src/main.rs`, add to the daily job array:

```rust
("0 45 3 * * *", "cleanup.service_metric_buckets"),
```

- [ ] **Step 5: Run the tests and check**

```bash
cd apps/api-rs
cargo test -p worker retention_intervals -- --nocapture
cargo check -p worker -p beat
```

Expected: test passes and both crates compile.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/cleanup.rs \
  apps/api-rs/crates/worker/src/handlers/mod.rs \
  apps/api-rs/crates/beat/src/main.rs
git commit -m "feat(worker): expire service metric buckets"
```

---

## Task 7: Types and HTTP Client

**Files:**

- Create: `packages/types/src/service/metrics.ts`
- Modify: `packages/types/src/service/index.ts`
- Create: `apps/web/core/services/service-metrics.service.ts`
- Create: `apps/web/core/services/service-metrics.service.test.ts`

- [ ] **Step 1: Add the types**

Create `packages/types/src/service/metrics.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceMetricResolution = 60 | 300 | 600 | 1800 | 3600;

export type TMetricRange = "1h" | "6h" | "24h" | "7d" | "30d" | "90d";

export interface IServiceMetricBucket {
  bucket_start: string;
  count: number | null;
  sum: number | null;
  min: number | null;
  max: number | null;
  avg: number | null;
  last: number | null;
}

export interface IServiceMetricLastBucket extends IServiceMetricBucket {
  resolution: TServiceMetricResolution;
}

export interface IServiceMetric {
  id: string;
  service_id: string;
  key: string;
  display_name: string;
  unit: string;
  description: string;
  last_bucket: IServiceMetricLastBucket | null;
}

export interface IServiceMetricBucketsResponse {
  metric: Pick<IServiceMetric, "id" | "key" | "display_name" | "unit" | "description">;
  resolution: TServiceMetricResolution;
  from: string;
  to: string;
  buckets: IServiceMetricBucket[];
}

export interface IMetricSourceConfig {
  service_map?: Record<string, string>;
  fallback_service_id?: string | null;
}

export interface IMetricSource {
  id: string;
  workspace_id: string;
  name: string;
  token: string;
  is_active: boolean;
  config: IMetricSourceConfig;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface IMetricSourcePayload {
  name: string;
  is_active?: boolean;
  config?: IMetricSourceConfig;
}
```

Add to `packages/types/src/service/index.ts`:

```ts
export * from "./metrics";
```

- [ ] **Step 2: Write the failing service test**

Create `apps/web/core/services/service-metrics.service.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import { ServiceMetricsService } from "./service-metrics.service";

describe("ServiceMetricsService", () => {
  it("fetches metric definitions", async () => {
    const service = new ServiceMetricsService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: { metrics: [] } } as never);
    await expect(service.getMetrics("itsm", "s1")).resolves.toEqual([]);
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/services/s1/metrics/");
  });

  it("passes range params to buckets", async () => {
    const service = new ServiceMetricsService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: {} } as never);
    await service.getBuckets("itsm", "s1", "m1", {
      from: "2026-10-24T09:00:00Z",
      to: "2026-10-24T10:00:00Z",
      resolution: 300,
    });
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/services/s1/metrics/m1/buckets/", {
      params: {
        from: "2026-10-24T09:00:00Z",
        to: "2026-10-24T10:00:00Z",
        resolution: 300,
      },
    });
  });

  it("creates, updates, rotates, and deletes sources", async () => {
    const service = new ServiceMetricsService();
    const post = vi.spyOn(service, "post").mockResolvedValue({ data: { id: "s" } } as never);
    const patch = vi.spyOn(service, "patch").mockResolvedValue({ data: { id: "s" } } as never);
    const del = vi.spyOn(service, "delete").mockResolvedValue({ data: {} } as never);

    await service.createSource("itsm", { name: "Prometheus" });
    await service.updateSource("itsm", "s", { name: "Prom" });
    await service.rotateSource("itsm", "s");
    await service.deleteSource("itsm", "s");

    expect(post).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/", { name: "Prometheus" });
    expect(patch).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/s/", { name: "Prom" });
    expect(post).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/s/rotate/", {});
    expect(del).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/s/");
  });
});
```

- [ ] **Step 3: Run to verify failure**

```bash
pnpm --filter=web test -- core/services/service-metrics.service.test.ts
```

Expected: module not found.

- [ ] **Step 4: Implement the service**

Create `apps/web/core/services/service-metrics.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IMetricSource,
  IMetricSourcePayload,
  IServiceMetric,
  IServiceMetricBucketsResponse,
  TServiceMetricResolution,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class ServiceMetricsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getMetrics(workspaceSlug: string, serviceId: string): Promise<IServiceMetric[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/`).then(
      (response) => response?.data?.metrics ?? []
    );
  }

  async getBuckets(
    workspaceSlug: string,
    serviceId: string,
    metricId: string,
    params: { from: string; to: string; resolution: TServiceMetricResolution }
  ): Promise<IServiceMetricBucketsResponse> {
    return this.get(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/${metricId}/buckets/`, {
      params,
    }).then((response) => response?.data);
  }

  async getSources(workspaceSlug: string): Promise<IMetricSource[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/metric-sources/`).then((response) => response?.data ?? []);
  }

  async createSource(workspaceSlug: string, data: IMetricSourcePayload): Promise<IMetricSource> {
    return this.post(`/api/workspaces/${workspaceSlug}/metric-sources/`, data).then((response) => response?.data);
  }

  async updateSource(
    workspaceSlug: string,
    sourceId: string,
    data: Partial<IMetricSourcePayload>
  ): Promise<IMetricSource> {
    return this.patch(`/api/workspaces/${workspaceSlug}/metric-sources/${sourceId}/`, data).then(
      (response) => response?.data
    );
  }

  async deleteSource(workspaceSlug: string, sourceId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/metric-sources/${sourceId}/`).then(
      (response) => response?.data
    );
  }

  async rotateSource(workspaceSlug: string, sourceId: string): Promise<{ id: string; token: string }> {
    return this.post(`/api/workspaces/${workspaceSlug}/metric-sources/${sourceId}/rotate/`, {}).then(
      (response) => response?.data
    );
  }

  async updateMetric(
    workspaceSlug: string,
    serviceId: string,
    metricId: string,
    data: { display_name?: string; unit?: string; description?: string }
  ): Promise<IServiceMetric> {
    return this.patch(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/${metricId}/`, data).then(
      (response) => response?.data
    );
  }

  async deleteMetric(workspaceSlug: string, serviceId: string, metricId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/${metricId}/`).then(
      (response) => response?.data
    );
  }
}
```

- [ ] **Step 5: Run the tests**

```bash
pnpm --filter=web test -- core/services/service-metrics.service.test.ts
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add packages/types/src/service/metrics.ts packages/types/src/service/index.ts \
  apps/web/core/services/service-metrics.service.ts \
  apps/web/core/services/service-metrics.service.test.ts
git commit -m "feat(web): add service metrics types and client"
```

---

## Task 8: Helpers and Hook

**Files:**

- Create: `apps/web/core/components/services/metrics/helpers.ts`
- Create: `apps/web/core/components/services/metrics/helpers.test.ts`
- Create: `apps/web/core/hooks/use-service-metrics.ts`

- [ ] **Step 1: Write the failing helper tests**

Create `apps/web/core/components/services/metrics/helpers.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { autoResolutionForRange, buildTimeline, formatMetricValue, latestBucket, rangeToFromTo } from "./helpers";

describe("service metrics helpers", () => {
  it("maps ranges to resolutions", () => {
    expect(autoResolutionForRange("1h")).toBe(60);
    expect(autoResolutionForRange("6h")).toBe(60);
    expect(autoResolutionForRange("24h")).toBe(300);
    expect(autoResolutionForRange("7d")).toBe(600);
    expect(autoResolutionForRange("30d")).toBe(1800);
    expect(autoResolutionForRange("90d")).toBe(3600);
  });

  it("builds from/to from the range", () => {
    const now = new Date("2026-10-24T10:00:00.000Z");
    expect(rangeToFromTo("1h", now)).toEqual({
      from: "2026-10-24T09:00:00.000Z",
      to: "2026-10-24T10:00:00.000Z",
    });
  });

  it("formats values with units", () => {
    expect(formatMetricValue(120, "ms")).toBe("120 ms");
    expect(formatMetricValue(99.5, "%")).toBe("99.5%");
    expect(formatMetricValue(null, "ms")).toBe("—");
  });

  it("fills gaps with null points", () => {
    const buckets = [
      {
        bucket_start: "2026-10-24T10:00:00.000Z",
        count: 60,
        sum: 7200,
        min: 80,
        max: 250,
        avg: 120,
        last: 130,
      },
      {
        bucket_start: "2026-10-24T10:05:00.000Z",
        count: 60,
        sum: 6000,
        min: 70,
        max: 200,
        avg: 100,
        last: 90,
      },
    ];
    const points = buildTimeline(buckets, 300, "2026-10-24T10:00:00.000Z", "2026-10-24T10:10:00.000Z", "1h");
    expect(points).toHaveLength(2);
    expect(points[0].avg).toBe(120);
    expect(points[1].avg).toBe(100);
    expect(points[0].domainValue).toBe(250);
  });

  it("returns the latest non-empty bucket", () => {
    expect(latestBucket([])).toBeNull();
    expect(
      latestBucket([
        { bucket_start: "a", count: null, sum: null, min: null, max: null, avg: null, last: null },
        { bucket_start: "b", count: 1, sum: 2, min: 1, max: 3, avg: 2, last: 2 },
      ])?.bucket_start
    ).toBe("b");
  });
});
```

- [ ] **Step 2: Run to verify failure**

```bash
pnpm --filter=web test -- core/components/services/metrics/helpers.test.ts
```

Expected: module not found.

- [ ] **Step 3: Implement helpers**

Create `helpers.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { format } from "date-fns";
import type { IServiceMetricBucket, TServiceMetricResolution, TMetricRange } from "@plane/types";

export const MAX_CHART_POINTS = 3000;

export const RANGE_HOURS: Record<TMetricRange, number> = {
  "1h": 1,
  "6h": 6,
  "24h": 24,
  "7d": 7 * 24,
  "30d": 30 * 24,
  "90d": 90 * 24,
};

export const RANGE_OPTIONS: { key: TMetricRange; label: string }[] = [
  { key: "1h", label: "service.monitoring.range_1h" },
  { key: "6h", label: "service.monitoring.range_6h" },
  { key: "24h", label: "service.monitoring.range_24h" },
  { key: "7d", label: "service.monitoring.range_7d" },
  { key: "30d", label: "service.monitoring.range_30d" },
  { key: "90d", label: "service.monitoring.range_90d" },
];

export type TMetricChartPoint = {
  timestamp: number;
  label: string;
  domainValue: number | null;
  avg: number | null;
  max: number | null;
  min: number | null;
  last: number | null;
  count: number | null;
  sum: number | null;
};

export function autoResolutionForRange(range: TMetricRange): TServiceMetricResolution {
  if (range === "1h" || range === "6h") return 60;
  if (range === "24h") return 300;
  if (range === "7d") return 600;
  if (range === "30d") return 1800;
  return 3600;
}

export function rangeToFromTo(range: TMetricRange, now = new Date()): { from: string; to: string } {
  const to = now.toISOString();
  const from = new Date(now.getTime() - RANGE_HOURS[range] * 60 * 60 * 1000).toISOString();
  return { from, to };
}

export function formatMetricValue(value: number | null | undefined, unit: string): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  const formatted = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value);
  if (!unit) return formatted;
  return unit === "%" ? `${formatted}%` : `${formatted} ${unit}`;
}

export function formatBucketLabel(date: Date, range: TMetricRange): string {
  if (range === "1h" || range === "6h" || range === "24h") return format(date, "HH:mm");
  if (range === "7d" || range === "30d") return format(date, "MMM d, HH:mm");
  return format(date, "MMM d");
}

export function latestBucket(buckets: IServiceMetricBucket[]): IServiceMetricBucket | null {
  for (let index = buckets.length - 1; index >= 0; index -= 1) {
    const bucket = buckets[index];
    if (
      bucket.count !== null ||
      bucket.sum !== null ||
      bucket.min !== null ||
      bucket.max !== null ||
      bucket.avg !== null ||
      bucket.last !== null
    ) {
      return bucket;
    }
  }
  return null;
}

export function buildTimeline(
  buckets: IServiceMetricBucket[],
  resolution: TServiceMetricResolution,
  from: string,
  to: string,
  range: TMetricRange
): TMetricChartPoint[] {
  const stepMs = resolution * 1000;
  const startMs = Math.floor(new Date(from).getTime() / stepMs) * stepMs;
  const endMs = new Date(to).getTime();
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) return [];
  const byTime = new Map(buckets.map((bucket) => [new Date(bucket.bucket_start).getTime(), bucket] as const));
  const points: TMetricChartPoint[] = [];
  for (let timestamp = startMs; timestamp < endMs && points.length < MAX_CHART_POINTS; timestamp += stepMs) {
    const bucket = byTime.get(timestamp);
    points.push({
      timestamp,
      label: formatBucketLabel(new Date(timestamp), range),
      domainValue: bucket?.max ?? bucket?.avg ?? bucket?.min ?? bucket?.last ?? null,
      avg: bucket?.avg ?? null,
      max: bucket?.max ?? null,
      min: bucket?.min ?? null,
      last: bucket?.last ?? null,
      count: bucket?.count ?? null,
      sum: bucket?.sum ?? null,
    });
  }
  return points;
}
```

- [ ] **Step 4: Add the hooks**

Create `apps/web/core/hooks/use-service-metrics.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import useSWR from "swr";
import type { IServiceMetric, IServiceMetricBucketsResponse, TMetricRange } from "@plane/types";
// components
import { autoResolutionForRange, rangeToFromTo } from "@/components/services/metrics/helpers";
// services
import { ServiceMetricsService } from "@/services/service-metrics.service";

const service = new ServiceMetricsService();

export function useServiceMetricDefinitions(workspaceSlug: string, serviceId: string) {
  return useSWR<IServiceMetric[]>(
    workspaceSlug && serviceId ? `SERVICE_METRIC_DEFS_${workspaceSlug}_${serviceId}` : null,
    () => service.getMetrics(workspaceSlug, serviceId),
    { refreshInterval: 60_000, revalidateOnFocus: true }
  );
}

export function useServiceMetricBuckets(
  workspaceSlug: string,
  serviceId: string,
  metricId: string | null,
  range: TMetricRange
) {
  return useSWR<IServiceMetricBucketsResponse>(
    metricId ? ["SERVICE_METRIC_BUCKETS", workspaceSlug, serviceId, metricId, range] : null,
    () => {
      const { from, to } = rangeToFromTo(range);
      return service.getBuckets(workspaceSlug, serviceId, metricId!, {
        from,
        to,
        resolution: autoResolutionForRange(range),
      });
    },
    { refreshInterval: 60_000, revalidateOnFocus: true, keepPreviousData: true }
  );
}
```

- [ ] **Step 5: Run the tests**

```bash
pnpm --filter=web test -- core/components/services/metrics/helpers.test.ts
```

Expected: 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/metrics/helpers.ts \
  apps/web/core/components/services/metrics/helpers.test.ts \
  apps/web/core/hooks/use-service-metrics.ts
git commit -m "feat(web): add service metrics helpers and hooks"
```

---

## Task 9: Monitoring Tab UI

**Files:**

- Modify: `apps/web/core/components/services/detail/pane/helpers.ts`
- Modify: `apps/web/core/components/services/detail/pane/tab-panels/root.tsx`
- Create: `apps/web/core/components/services/detail/pane/tab-panels/monitoring.tsx`
- Create: `apps/web/core/components/services/metrics/monitoring-panel.tsx`
- Create: `apps/web/core/components/services/metrics/metric-selector.tsx`
- Create: `apps/web/core/components/services/metrics/range-selector.tsx`
- Create: `apps/web/core/components/services/metrics/metric-chart.tsx`
- Create: `apps/web/core/components/services/metrics/metric-summary.tsx`

- [ ] **Step 1: Extend the pane tab metadata**

In `helpers.ts`, replace the union and `SERVICE_DETAIL_PANE_TABS`:

```ts
export type TServiceDetailPaneTab = "properties" | "relationships" | "tickets" | "monitoring";

export const SERVICE_DETAIL_PANE_TABS: {
  key: TServiceDetailPaneTab;
  i18n_label: string;
}[] = [
  { key: "properties", i18n_label: "common.properties" },
  { key: "relationships", i18n_label: "service.relationships.title" },
  { key: "tickets", i18n_label: "service.tabs.work_items" },
  { key: "monitoring", i18n_label: "service.tabs.monitoring" },
];
```

- [ ] **Step 2: Create the tab panel**

Create `monitoring.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ServiceMonitoringPanel } from "@/components/services/metrics/monitoring-panel";

type Props = {
  serviceId: string;
};

export function ServiceMonitoringTabPanel({ serviceId }: Props) {
  return <ServiceMonitoringPanel serviceId={serviceId} />;
}
```

In `tab-panels/root.tsx`, import and add:

```tsx
import { ServiceMonitoringTabPanel } from "./monitoring";
```

```tsx
{
  tab.key === "monitoring" && <ServiceMonitoringTabPanel serviceId={serviceId} />;
}
```

- [ ] **Step 3: Create the selectors**

Create `metric-selector.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { IServiceMetric } from "@plane/types";
import { CustomSelect } from "@plane/ui";

type Props = {
  metrics: IServiceMetric[];
  value: string | null;
  onChange: (metricId: string) => void;
};

export function MetricSelector({ metrics, value, onChange }: Props) {
  const { t } = useTranslation();
  const selected = metrics.find((metric) => metric.id === value);
  const label = selected
    ? `${selected.display_name || selected.key}${selected.unit ? ` (${selected.unit})` : ""}`
    : t("service.monitoring.select_metric");

  return (
    <CustomSelect
      value={value}
      label={label}
      onChange={(next: string) => onChange(next)}
      noChevron
      className="w-full"
      buttonClassName="w-full"
    >
      {metrics.map((metric) => (
        <CustomSelect.Option key={metric.id} value={metric.id}>
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate">{metric.display_name || metric.key}</span>
            {metric.unit && <span className="text-tertiary">{metric.unit}</span>}
          </span>
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
}
```

Create `range-selector.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TMetricRange } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { RANGE_OPTIONS } from "./helpers";

type Props = {
  value: TMetricRange;
  onChange: (range: TMetricRange) => void;
};

export function RangeSelector({ value, onChange }: Props) {
  const { t } = useTranslation();
  const selected = RANGE_OPTIONS.find((option) => option.key === value);

  return (
    <CustomSelect
      value={value}
      label={selected ? t(selected.label) : value}
      onChange={(next: TMetricRange) => onChange(next)}
      noChevron
      className="w-full"
      buttonClassName="w-full"
    >
      {RANGE_OPTIONS.map((option) => (
        <CustomSelect.Option key={option.key} value={option.key}>
          {t(option.label)}
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
}
```

- [ ] **Step 4: Create the chart and summary**

Create `metric-chart.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import { LineChart } from "@plane/propel/charts/line-chart";
import type { TChartData } from "@plane/types";
import { formatMetricValue, type TMetricChartPoint } from "./helpers";

type Props = {
  data: TMetricChartPoint[];
  unit: string;
};

export function MetricChart({ data, unit }: Props) {
  const { t } = useTranslation();
  const hasData = data.some((point) => point.avg !== null || point.max !== null);
  if (!hasData) {
    return (
      <div className="flex h-60 items-center justify-center text-13 text-tertiary">
        {t("service.monitoring.empty_range")}
      </div>
    );
  }

  return (
    <LineChart
      className="h-60 w-full"
      data={data as unknown as TChartData<string, string>[]}
      xAxis={{ key: "label" }}
      yAxis={{ key: "domainValue", label: unit }}
      lines={[
        {
          key: "avg",
          label: t("service.monitoring.avg"),
          dashedLine: false,
          fill: "#1192E8",
          showDot: false,
          smoothCurves: false,
          stroke: "#1192E8",
        },
        {
          key: "max",
          label: t("service.monitoring.max"),
          dashedLine: true,
          fill: "#F0A000",
          showDot: false,
          smoothCurves: false,
          stroke: "#F0A000",
        },
      ]}
      customTooltipContent={({ active, payload }) => {
        if (!active || !payload?.length) return null;
        const point = payload[0].payload as TMetricChartPoint;
        const rows: [string, number | null][] = [
          ["last", point.last],
          ["avg", point.avg],
          ["min", point.min],
          ["max", point.max],
        ];
        return (
          <div className="rounded-md border border-subtle bg-surface-1 p-2 text-11 shadow-lg">
            <p className="mb-1 font-medium text-primary">{point.label}</p>
            {rows.map(([key, value]) => (
              <p key={key} className="flex justify-between gap-3 text-tertiary">
                <span>{t(`service.monitoring.${key}`)}</span>
                <span className="text-secondary">{formatMetricValue(value, unit)}</span>
              </p>
            ))}
            <p className="flex justify-between gap-3 text-tertiary">
              <span>{t("service.monitoring.count")}</span>
              <span className="text-secondary">{point.count ?? "—"}</span>
            </p>
          </div>
        );
      }}
    />
  );
}
```

Create `metric-summary.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { IServiceMetricBucket } from "@plane/types";
import { formatMetricValue, latestBucket } from "./helpers";

type Props = {
  buckets: IServiceMetricBucket[];
  unit: string;
};

export function MetricSummary({ buckets, unit }: Props) {
  const { t } = useTranslation();
  const latest = latestBucket(buckets);
  const items: { key: string; value: number | null; raw?: boolean }[] = [
    { key: "last", value: latest?.last ?? null },
    { key: "avg", value: latest?.avg ?? null },
    { key: "min", value: latest?.min ?? null },
    { key: "max", value: latest?.max ?? null },
    { key: "count", value: latest?.count ?? null, raw: true },
  ];

  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
      {items.map((item) => (
        <div key={item.key} className="rounded-md border border-subtle p-2">
          <p className="text-10 font-medium uppercase tracking-wide text-tertiary">
            {t(`service.monitoring.${item.key}`)}
          </p>
          <p className="truncate text-13 font-medium text-primary">
            {item.raw ? (item.value ?? "—") : formatMetricValue(item.value, unit)}
          </p>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 5: Create the panel**

Create `monitoring-panel.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { observer } from "mobx-react";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EmptyStateCompact } from "@plane/propel/empty-state";
import type { TMetricRange } from "@plane/types";
import { Loader } from "@plane/ui";
import { useUserPermissions } from "@/hooks/store/user";
import { useServiceMetricBuckets, useServiceMetricDefinitions } from "@/hooks/use-service-metrics";
import { buildTimeline } from "./helpers";
import { MetricChart } from "./metric-chart";
import { MetricSelector } from "./metric-selector";
import { MetricSummary } from "./metric-summary";
import { RangeSelector } from "./range-selector";

type Props = {
  serviceId: string;
};

export const ServiceMonitoringPanel = observer(function ServiceMonitoringPanel({ serviceId }: Props) {
  const { t } = useTranslation();
  const params = useParams();
  const workspaceSlug = params.workspaceSlug?.toString() ?? "";
  const { allowPermissions } = useUserPermissions();
  const isAdmin = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const [selectedMetricId, setSelectedMetricId] = useState<string | null>(null);
  const [range, setRange] = useState<TMetricRange>("24h");

  const { data: metrics, isLoading } = useServiceMetricDefinitions(workspaceSlug, serviceId);

  useEffect(() => {
    if (!selectedMetricId && metrics?.length) {
      setSelectedMetricId((metrics.find((metric) => metric.last_bucket) ?? metrics[0]).id);
    }
    if (selectedMetricId && metrics && !metrics.some((metric) => metric.id === selectedMetricId)) {
      setSelectedMetricId(null);
    }
  }, [metrics, selectedMetricId]);

  const metric = metrics?.find((item) => item.id === selectedMetricId) ?? null;
  const {
    data: bucketData,
    isLoading: bucketsLoading,
    error,
    mutate,
  } = useServiceMetricBuckets(workspaceSlug, serviceId, metric?.id ?? null, range);

  const timeline = useMemo(
    () =>
      bucketData ? buildTimeline(bucketData.buckets, bucketData.resolution, bucketData.from, bucketData.to, range) : [],
    [bucketData, range]
  );

  if (isLoading) {
    return (
      <Loader className="space-y-3">
        <Loader.Item height="32px" width="100%" />
        <Loader.Item height="120px" width="100%" />
      </Loader>
    );
  }

  if (!metrics?.length) {
    return (
      <div className="flex flex-col items-center justify-center gap-4 py-10">
        <EmptyStateCompact
          assetKey="unknown"
          title={t("service.monitoring.empty_title")}
          description={t("service.monitoring.empty_description")}
        />
        {isAdmin && (
          <Link href={`/${workspaceSlug}/settings/metric-sources`}>
            <Button variant="primary" size="sm">
              {t("service.monitoring.configure_source")}
            </Button>
          </Link>
        )}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 py-10">
        <p className="text-13 text-tertiary">{t("service.monitoring.error")}</p>
        <Button variant="secondary" size="sm" onClick={() => mutate()}>
          {t("common.retry")}
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-col gap-2">
        <MetricSelector metrics={metrics} value={metric?.id ?? null} onChange={setSelectedMetricId} />
        <RangeSelector value={range} onChange={setRange} />
      </div>
      <p className="text-11 text-tertiary">
        {t("service.monitoring.resolution")}: {bucketData?.resolution ? `${bucketData.resolution / 60}m` : "—"}
      </p>
      {bucketData && <MetricSummary buckets={bucketData.buckets} unit={metric?.unit ?? ""} />}
      {bucketsLoading && !bucketData ? (
        <Loader className="space-y-2">
          <Loader.Item height="240px" width="100%" />
        </Loader>
      ) : (
        <MetricChart data={timeline} unit={metric?.unit ?? ""} />
      )}
    </div>
  );
});
```

- [ ] **Step 6: Typecheck**

```bash
pnpm --filter=web check:types
```

Expected: no errors. Fix prop/type mismatches (`CustomSelect`, `LineChart`, `EmptyStateCompact`) before continuing.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/services/detail/pane/helpers.ts \
  apps/web/core/components/services/detail/pane/tab-panels/root.tsx \
  apps/web/core/components/services/detail/pane/tab-panels/monitoring.tsx \
  apps/web/core/components/services/metrics
git commit -m "feat(web): add monitoring tab to service detail"
```

---

## Task 10: Workspace Settings Page

**Files:**

- Modify: `packages/types/src/settings.ts`
- Modify: `packages/constants/src/settings/workspace.ts`
- Modify: `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/metric-sources/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/metric-sources/header.tsx`
- Create: `apps/web/core/components/services/metrics/sources/root.tsx`
- Create: `apps/web/core/components/services/metrics/sources/source-form.tsx`
- Create: `apps/web/core/components/services/metrics/sources/index.ts`

- [ ] **Step 1: Add the settings tab type and item**

In `packages/types/src/settings.ts`, add `"metric_sources"` to `TWorkspaceSettingsTabs`.

In `packages/constants/src/settings/workspace.ts`, add to `WORKSPACE_SETTINGS`:

```ts
metric_sources: {
  key: "metric_sources",
  i18n_label: "workspace_settings.settings.metric_sources.heading",
  href: `/settings/metric-sources`,
  access: [EUserWorkspaceRoles.ADMIN],
  highlight: (pathname: string, baseUrl: string) =>
    pathname.startsWith(`${baseUrl}/settings/metric-sources`),
},
```

Add `WORKSPACE_SETTINGS["metric_sources"]` to `GROUPED_WORKSPACE_SETTINGS[WORKSPACE_SETTINGS_CATEGORY.SERVICE_MANAGEMENT]`.

- [ ] **Step 2: Add the icon**

In `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`, import `AnalyticsIcon` from `@makeplane/propel/icons` and add:

```ts
metric_sources: AnalyticsIcon,
```

- [ ] **Step 3: Create the header and page**

Create `header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { WORKSPACE_SETTINGS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs } from "@plane/ui";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { SettingsPageHeader } from "@/components/settings/page-header";
import { WORKSPACE_SETTINGS_ICONS } from "@/components/settings/workspace/sidebar/item-icon";

export const MetricSourcesSettingsHeader = observer(function MetricSourcesSettingsHeader() {
  const { t } = useTranslation();
  const settingsDetails = WORKSPACE_SETTINGS.metric_sources;
  const Icon = WORKSPACE_SETTINGS_ICONS.metric_sources;

  return (
    <SettingsPageHeader
      leftItem={
        <div className="flex items-center gap-2">
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t(settingsDetails.i18n_label)}
                  icon={<Icon className="size-4 text-tertiary" />}
                />
              }
            />
          </Breadcrumbs>
        </div>
      }
    />
  );
});
```

Create `page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
import { observer } from "mobx-react";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { NotAuthorizedView } from "@/components/auth-screens/not-authorized-view";
import { PageHead } from "@/components/core/page-title";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
import { MetricSourcesRoot } from "@/components/services/metrics/sources";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { MetricSourcesSettingsHeader } from "./header";

function MetricSourcesPage() {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const { currentWorkspace } = useWorkspace();
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const canManage = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.metric_sources.heading")}`
    : undefined;

  if (workspaceUserInfo && !canManage) {
    return <NotAuthorizedView section="settings" className="h-auto" />;
  }

  return (
    <SettingsContentWrapper header={<MetricSourcesSettingsHeader />}>
      <PageHead title={pageTitle} />
      <MetricSourcesRoot workspaceSlug={workspaceSlug?.toString() ?? ""} />
    </SettingsContentWrapper>
  );
}

export default observer(MetricSourcesPage);
```

- [ ] **Step 4: Create the source form**

Create `source-form.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IMetricSource, IService } from "@plane/types";
import { CustomSelect, EModalPosition, EModalWidth, Input as UIKitInput, ModalCore } from "@plane/ui";
import { ServiceMetricsService } from "@/services/service-metrics.service";

const service = new ServiceMetricsService();

type Mapping = { key: string; serviceId: string };

type Props = {
  isOpen: boolean;
  workspaceSlug: string;
  source: IMetricSource | null;
  services: IService[];
  onClose: () => void;
  onSaved: () => void | Promise<void>;
};

export function MetricSourceForm({ isOpen, workspaceSlug, source, services, onClose, onSaved }: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [mappings, setMappings] = useState<Mapping[]>([{ key: "", serviceId: "" }]);
  const [fallback, setFallback] = useState("");
  const [isActive, setIsActive] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setName(source?.name ?? "");
    const entries = Object.entries(source?.config.service_map ?? {});
    setMappings(
      entries.length ? entries.map(([key, serviceId]) => ({ key, serviceId })) : [{ key: "", serviceId: "" }]
    );
    setFallback(source?.config.fallback_service_id ?? "");
    setIsActive(source?.is_active ?? true);
    setError(null);
  }, [isOpen, source]);

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError(t("workspace_settings.settings.metric_sources.name_required"));
      return;
    }
    const serviceMap: Record<string, string> = {};
    for (const mapping of mappings) {
      if (!mapping.key.trim() || !mapping.serviceId) continue;
      serviceMap[mapping.key.trim()] = mapping.serviceId;
    }
    const payload = {
      name: trimmed,
      is_active: isActive,
      config: { service_map: serviceMap, fallback_service_id: fallback || null },
    };
    setSaving(true);
    setError(null);
    try {
      if (source) {
        await service.updateSource(workspaceSlug, source.id, payload);
      } else {
        await service.createSource(workspaceSlug, payload);
      }
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t(
          source
            ? "workspace_settings.settings.metric_sources.updated"
            : "workspace_settings.settings.metric_sources.created"
        ),
      });
      await onSaved();
    } catch (err) {
      setError((err as { response?: { data?: { error?: string } } })?.response?.data?.error ?? t("common.error"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} position={EModalPosition.TOP} width={EModalWidth.XXL} className="p-4">
      <h3 className="text-h4-medium text-primary">
        {t(
          source
            ? "workspace_settings.settings.metric_sources.edit_title"
            : "workspace_settings.settings.metric_sources.create_title"
        )}
      </h3>
      <div className="mt-4 space-y-4">
        <div>
          <label className="mb-1 block text-11 text-tertiary">
            {t("workspace_settings.settings.metric_sources.name")}
          </label>
          <UIKitInput
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Prometheus production"
          />
        </div>

        <div>
          <p className="mb-2 text-11 text-tertiary">{t("workspace_settings.settings.metric_sources.mapping_title")}</p>
          <div className="space-y-2">
            {mappings.map((mapping, index) => (
              <div key={index} className="flex gap-2">
                <UIKitInput
                  value={mapping.key}
                  placeholder={t("workspace_settings.settings.metric_sources.mapping_key")}
                  onChange={(event) => {
                    const next = [...mappings];
                    next[index] = { ...next[index], key: event.target.value };
                    setMappings(next);
                  }}
                />
                <CustomSelect
                  value={mapping.serviceId}
                  label={
                    services.find((item) => item.id === mapping.serviceId)?.name ??
                    t("workspace_settings.settings.metric_sources.mapping_service")
                  }
                  onChange={(value: string) => {
                    const next = [...mappings];
                    next[index] = { ...next[index], serviceId: value };
                    setMappings(next);
                  }}
                  noChevron
                  className="w-full"
                >
                  {services.map((item) => (
                    <CustomSelect.Option key={item.id} value={item.id}>
                      {item.name}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setMappings(mappings.filter((_, i) => i !== index))}
                >
                  {t("common.delete")}
                </Button>
              </div>
            ))}
          </div>
          <Button
            variant="secondary"
            size="sm"
            className="mt-2"
            onClick={() => setMappings([...mappings, { key: "", serviceId: "" }])}
          >
            {t("workspace_settings.settings.metric_sources.add_mapping")}
          </Button>
        </div>

        <div>
          <label className="mb-1 block text-11 text-tertiary">
            {t("workspace_settings.settings.metric_sources.fallback")}
          </label>
          <CustomSelect
            value={fallback}
            label={
              services.find((item) => item.id === fallback)?.name ??
              t("workspace_settings.settings.metric_sources.fallback_none")
            }
            onChange={(value: string) => setFallback(value)}
            noChevron
            className="w-full"
          >
            <CustomSelect.Option value="">
              {t("workspace_settings.settings.metric_sources.fallback_none")}
            </CustomSelect.Option>
            {services.map((item) => (
              <CustomSelect.Option key={item.id} value={item.id}>
                {item.name}
              </CustomSelect.Option>
            ))}
          </CustomSelect>
        </div>

        <label className="flex items-center gap-2 text-13 text-secondary">
          <input type="checkbox" checked={isActive} onChange={(event) => setIsActive(event.target.checked)} />
          {t("workspace_settings.settings.metric_sources.active")}
        </label>

        {error && <p className="text-12 text-danger-primary">{error}</p>}

        <div className="flex justify-end gap-2">
          <Button variant="secondary" size="sm" onClick={onClose} disabled={saving}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" onClick={submit} loading={saving}>
            {t("workspace_settings.settings.metric_sources.save")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
}
```

- [ ] **Step 5: Create the source list**

Create `root.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import useSWR from "swr";
import { API_BASE_URL } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EmptyStateCompact } from "@plane/propel/empty-state";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IMetricSource } from "@plane/types";
import { copyTextToClipboard } from "@plane/utils";
import { AlertModalCore } from "@plane/ui";
import { SettingsHeading } from "@/components/settings/heading";
import { ServiceMetricsService } from "@/services/service-metrics.service";
import { ServiceService } from "@/services/service.service";
import { MetricSourceForm } from "./source-form";

const metricsService = new ServiceMetricsService();
const serviceService = new ServiceService();

type Props = {
  workspaceSlug: string;
};

export function MetricSourcesRoot({ workspaceSlug }: Props) {
  const { t } = useTranslation();
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<IMetricSource | null>(null);
  const [deleting, setDeleting] = useState<IMetricSource | null>(null);
  const [rotating, setRotating] = useState<IMetricSource | null>(null);
  const [revealed, setRevealed] = useState<Record<string, boolean>>({});
  const [submitting, setSubmitting] = useState(false);

  const {
    data: sources,
    isLoading,
    mutate,
  } = useSWR(workspaceSlug ? `METRIC_SOURCES_${workspaceSlug}` : null, () => metricsService.getSources(workspaceSlug));
  const { data: services } = useSWR(workspaceSlug ? `METRIC_SOURCE_SERVICES_${workspaceSlug}` : null, () =>
    serviceService.getWorkspaceServices(workspaceSlug)
  );

  const handleDelete = async () => {
    if (!deleting) return;
    setSubmitting(true);
    try {
      await metricsService.deleteSource(workspaceSlug, deleting.id);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("workspace_settings.settings.metric_sources.deleted"),
      });
      setDeleting(null);
      await mutate();
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("common.error") });
    } finally {
      setSubmitting(false);
    }
  };

  const handleRotate = async () => {
    if (!rotating) return;
    setSubmitting(true);
    try {
      await metricsService.rotateSource(workspaceSlug, rotating.id);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("workspace_settings.settings.metric_sources.rotated"),
      });
      setRotating(null);
      await mutate();
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("common.error") });
    } finally {
      setSubmitting(false);
    }
  };

  const copyToken = async (token: string) => {
    await copyTextToClipboard(token);
    setToast({
      type: TOAST_TYPE.SUCCESS,
      title: t("workspace_settings.settings.metric_sources.token_copied"),
    });
  };

  return (
    <div className="w-full">
      <MetricSourceForm
        isOpen={showForm}
        workspaceSlug={workspaceSlug}
        source={editing}
        services={services ?? []}
        onClose={() => {
          setShowForm(false);
          setEditing(null);
        }}
        onSaved={async () => {
          await mutate();
          setShowForm(false);
          setEditing(null);
        }}
      />
      <AlertModalCore
        isOpen={Boolean(deleting)}
        handleClose={() => setDeleting(null)}
        handleSubmit={handleDelete}
        isSubmitting={submitting}
        title={t("workspace_settings.settings.metric_sources.delete")}
        content={t("workspace_settings.settings.metric_sources.delete_confirm")}
        variant="danger"
        primaryButtonText={{
          loading: t("common.delete"),
          default: t("common.delete"),
        }}
      />
      <AlertModalCore
        isOpen={Boolean(rotating)}
        handleClose={() => setRotating(null)}
        handleSubmit={handleRotate}
        isSubmitting={submitting}
        title={t("workspace_settings.settings.metric_sources.rotate")}
        content={t("workspace_settings.settings.metric_sources.rotate_confirm")}
        primaryButtonText={{
          loading: t("workspace_settings.settings.metric_sources.rotate"),
          default: t("workspace_settings.settings.metric_sources.rotate"),
        }}
      />

      <SettingsHeading
        title={t("workspace_settings.settings.metric_sources.heading")}
        description={t("workspace_settings.settings.metric_sources.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditing(null);
              setShowForm(true);
            }}
          >
            {t("workspace_settings.settings.metric_sources.add")}
          </Button>
        }
      />

      {isLoading ? (
        <p className="mt-4 text-13 text-tertiary">{t("common.loading")}</p>
      ) : !sources?.length ? (
        <div className="mt-4 flex justify-center py-20">
          <EmptyStateCompact
            assetKey="unknown"
            title={t("workspace_settings.settings.metric_sources.empty_title")}
            description={t("workspace_settings.settings.metric_sources.empty_description")}
          />
        </div>
      ) : (
        <div className="mt-4 divide-y divide-subtle rounded-md border border-subtle">
          {sources.map((source) => {
            const mapped = Object.keys(source.config.service_map ?? {}).length;
            return (
              <div key={source.id} className="flex flex-col gap-2 p-3 md:flex-row md:items-center">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-13 font-medium text-primary">{source.name}</p>
                  <p className="text-11 text-tertiary">
                    {mapped} {t("workspace_settings.settings.metric_sources.mapped_services")} ·{" "}
                    {source.is_active
                      ? t("workspace_settings.settings.metric_sources.active")
                      : t("workspace_settings.settings.metric_sources.inactive")}
                  </p>
                </div>
                <div className="flex min-w-0 items-center gap-2">
                  <code className="max-w-55 truncate rounded bg-surface-2 px-2 py-1 text-11 text-secondary">
                    {revealed[source.id] ? source.token : `${source.token.slice(0, 12)}…`}
                  </code>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setRevealed({ ...revealed, [source.id]: !revealed[source.id] })}
                  >
                    {revealed[source.id]
                      ? t("workspace_settings.settings.metric_sources.hide")
                      : t("workspace_settings.settings.metric_sources.show")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => copyToken(source.token)}>
                    {t("workspace_settings.settings.metric_sources.copy_token")}
                  </Button>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      setEditing(source);
                      setShowForm(true);
                    }}
                  >
                    {t("common.edit")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => setRotating(source)}>
                    {t("workspace_settings.settings.metric_sources.rotate")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => setDeleting(source)}>
                    {t("common.delete")}
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="mt-6 rounded-md border border-subtle p-3">
        <p className="text-12 font-medium text-primary">{t("workspace_settings.settings.metric_sources.snippet")}</p>
        <pre className="mt-2 overflow-x-auto rounded bg-surface-2 p-2 text-11 text-secondary">
          {`curl -X POST "${API_BASE_URL}/api/inbound/metrics/<token>/" \\
  -H 'Content-Type: application/json' \\
  -d '{"metrics":[{"service":"payment-api","key":"http.latency","unit":"ms","resolution":60,"bucket_start":"2026-10-24T10:00:00Z","count":60,"avg":120}]}'`}
        </pre>
      </div>
    </div>
  );
}
```

Create `index.ts`:

```ts
export { MetricSourcesRoot } from "./root";
```

- [ ] **Step 6: Typecheck and lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: no errors. Fix `common.edit`/`common.loading` if those keys are missing by adding them to the metric-sources i18n object and referencing them explicitly.

- [ ] **Step 7: Commit**

  apps/web/app/'(all)'/'(workspaceSlug)'/'(settings)'/settings/'(workspace)'/metric-sources \
   apps/web/core/components/services/metrics/sources
  git commit -m "feat(web): add metric sources settings page"

````

---

## Task 11: i18n

**Files:**
- Modify: `packages/i18n/src/locales/en/service.json`
- Modify: `packages/i18n/src/locales/en/workspace-settings.json`
- Modify: `packages/i18n/src/locales/*/service.json`
- Modify: `packages/i18n/src/locales/*/workspace-settings.json`

- [ ] **Step 1: Add the English service keys**

In `packages/i18n/src/locales/en/service.json`, add `"monitoring": "Monitoring"` to `service.tabs`, then add this object under `service`:

```json
"monitoring": {
  "select_metric": "Metric",
  "range_1h": "Last 1 hour",
  "range_6h": "Last 6 hours",
  "range_24h": "Last 24 hours",
  "range_7d": "Last 7 days",
  "range_30d": "Last 30 days",
  "range_90d": "Last 90 days",
  "empty_title": "No metrics received yet",
  "empty_description": "Connect a monitoring source to start ingesting metric buckets for this service.",
  "empty_range": "No data in this range",
  "error": "Could not load metrics.",
  "configure_source": "Configure metric source",
  "last": "Last",
  "avg": "Average",
  "min": "Min",
  "max": "Max",
  "count": "Count",
  "resolution": "Resolution"
}
````

- [ ] **Step 2: Add the English settings keys**

In `packages/i18n/src/locales/en/workspace-settings.json`, add under `workspace_settings.settings`:

```json
"metric_sources": {
  "heading": "Metric sources",
  "description": "Receive pre-aggregated monitoring metrics for services in this workspace.",
  "add": "Add source",
  "name": "Source name",
  "name_required": "Source name is required",
  "mapping_title": "Service mapping",
  "mapping_key": "External key",
  "mapping_service": "Service",
  "add_mapping": "Add mapping",
  "fallback": "Fallback service",
  "fallback_none": "No fallback",
  "active": "Active",
  "inactive": "Inactive",
  "mapped_services": "services mapped",
  "copy_token": "Copy token",
  "token_copied": "Token copied",
  "show": "Show",
  "hide": "Hide",
  "create_title": "Add metric source",
  "edit_title": "Edit metric source",
  "delete": "Delete source",
  "delete_confirm": "Delete this source? Existing metric buckets are kept.",
  "deleted": "Metric source deleted",
  "rotate": "Rotate token",
  "rotate_confirm": "Rotate this token? The old token will stop working immediately.",
  "rotated": "Token rotated",
  "save": "Save",
  "created": "Metric source created",
  "updated": "Metric source updated",
  "snippet": "Ingest example",
  "empty_title": "No metric sources yet",
  "empty_description": "Add a source to get an ingest token and map external service keys to services."
}
```

- [ ] **Step 3: Translate all locales**

Use the `translate` skill. Required reading before touching locale files: `.claude/skills/translate/SKILL.md`.

Translate only the new `service.tabs.monitoring`, `service.monitoring.*`, and `workspace_settings.settings.metric_sources.*` keys into all 19 locales. Do not change existing keys (including `service.layout` / board labels). Preserve placeholders and punctuation rules from the skill.

- [ ] **Step 4: Regenerate types**

```bash
pnpm --filter=@plane/i18n run generate:types
```

Expected: `packages/i18n/src/types/keys.generated.ts` includes the new keys.

- [ ] **Step 5: Verify no old service board keys were removed accidentally**

```bash
rg -n "service\\.monitoring\\.|metric_sources" packages/i18n/src/types/keys.generated.ts | head
```

Expected: new keys present.

- [ ] **Step 6: Typecheck**

```bash
pnpm --filter=web check:types
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add packages/i18n/src/locales packages/i18n/src/types/keys.generated.ts
git commit -m "feat(i18n): add service metrics copy"
```

---

## Task 12: Verification and Rollout

**Files:** none (verification only; commit fixes if needed)

- [ ] **Step 1: Run backend tests**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1
cargo test -p api routes::service_metrics::tests
cargo test -p worker retention_intervals
cargo check -p api -p worker -p beat
```

Expected: all pass/compile.

- [ ] **Step 2: Run web checks**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web test -- core/services/service-metrics.service.test.ts core/components/services/metrics/helpers.test.ts
pnpm --filter=web build
```

Expected: no errors; build completes.

- [ ] **Step 3: Rebuild backend**

Follow `AGENTS.md`: detached build to a log so the Rust LTO link is not mistaken for a hang.

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker \
  > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll `/tmp/plane-api-build.log` until it finishes, then:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
systemctl --user restart plane-live.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
```

Expected: both `200`.

- [ ] **Step 4: Rebuild web and restart prod**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 5
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000
```

Expected: `200`.

- [ ] **Step 5: Smoke test the API**

Create a source through the settings page (or via curl with a session), copy its token, then:

```bash
BUCKET=$(date -u +%Y-%m-%dT%H:%M:00Z)
curl -s -X POST "http://localhost:8000/api/inbound/metrics/<token>/" \
  -H 'Content-Type: application/json' \
  -d "{\"metrics\":[{\"service\":\"<external-key>\",\"key\":\"http.latency\",\"unit\":\"ms\",\"resolution\":60,\"bucket_start\":\"$BUCKET\",\"count\":60,\"avg\":120,\"max\":250}]}"
```

Expected: `202 {"accepted":1,"metrics_created":1}`. Re-send the same body: `metrics_created` becomes `0`, `accepted` stays `1`.

- [ ] **Step 6: Smoke test the UI**

1. Open a service detail → pane → Monitoring tab.
2. Select the metric and ranges `1h`, `24h`, `7d`; chart and summary render.
3. Rotate the token in settings; the old token returns `404`.
4. Open the settings page as a non-admin; `NotAuthorizedView` appears.
5. Confirm the service list/board/graph and health cells are unchanged.

- [ ] **Step 7: Commit any fixes**

```bash
git status --short
git add -A
git commit -m "fix: address service metrics verification findings"
```

Only commit if Step 1-6 required changes.

---

## Plan Self-Review

**Spec coverage:**

| Spec section                                       | Task      |
| -------------------------------------------------- | --------- |
| Data model / migration                             | Task 1    |
| Ingest contract, validation, upsert, auto-register | Tasks 2-3 |
| Source management API                              | Task 4    |
| Read API, auto resolution, scopes                  | Task 5    |
| Retention job                                      | Task 6    |
| Types + HTTP client                                | Task 7    |
| Helpers, hooks                                     | Task 8    |
| Monitoring tab                                     | Task 9    |
| Settings page                                      | Task 10   |
| i18n                                               | Task 11   |
| Verification / rollout                             | Task 12   |

**Placeholder scan:** no TBD/TODO; every code step includes the code. The only manual steps are the `translate` skill (required by repo rules) and the Docker log polling (required by AGENTS.md).

**Type consistency:** `IngestItem`, `BucketQuery`, `CreateSource`, `PatchMetric`, `IServiceMetric`, `IServiceMetricBucket`, `TMetricRange`, `TServiceMetricResolution` are defined once and reused. API field `last` maps to DB column `last_value`. The read response returns `last`; the ingest payload accepts `last`.

**Known follow-ups (not in this plan):** threshold/health integration, metric labels/dimensions, per-source rate limits, server-side rollups, partitions, and list/board sparklines.
