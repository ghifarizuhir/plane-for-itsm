# Real Service Health & Incidents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the mock service health/incident data with a read-time api-rs endpoint derived from open Incident work items and active war rooms, and update the service catalog UI accordingly.

**Architecture:** A new `service_health` axum module computes snapshots for visible services using three SQL queries (services, issue-based incidents, war-room-linked incidents), merges/dedupes in Rust, and returns the existing `IServiceHealthSnapshot` shape. The web app swaps its local mock for that endpoint and keeps the existing `healthMap`/filter flow. Deployment is postponed: `last_deployed_at` is always null and the UI shows "No deployment".

**Tech Stack:** Rust/axum/sqlx, PostgreSQL, React Router, MobX, Vitest, i18next.

---

### Task 1: Backend health endpoint

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/service_health.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs:705-830`
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Add failing integration tests**

Append these helpers and tests to `apps/api-rs/crates/api/tests/service_workspace_test.rs` (after the existing `insert_war_room` helper):

```rust
async fn insert_incident(
    pool: &PgPool,
    sc: &Scratch,
    project_id: Uuid,
    name: &str,
    priority: &str,
    group: &str,
) -> Uuid {
    let state_id = Uuid::new_v4();
    let type_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
         sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, $2, '', '#cccccc', $3, $4, $5, 65535, $6, false, false, now(), now())",
    )
    .bind(state_id)
    .bind(format!("health-{}", Uuid::new_v4().simple()))
    .bind(format!("health-{}", Uuid::new_v4().simple()))
    .bind(project_id)
    .bind(sc.workspace_id)
    .bind(group)
    .execute(pool)
    .await
    .expect("scratch incident state");
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
         is_default, level, is_epic, requires_service, created_at, updated_at) \
         VALUES ($1, 'Incident', '', '{}'::jsonb, $2, true, true, 0, false, true, now(), now())",
    )
    .bind(type_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch incident type");
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, name, description_html, description_json, description_stripped, \
         priority, is_draft, sort_order, sequence_id, state_id, type_id, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, $2, '<p></p>', '{}', '', $3, false, 65535, \
         (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $4), \
         $5, $6, $4, $7, now(), now())",
    )
    .bind(issue_id)
    .bind(name)
    .bind(priority)
    .bind(project_id)
    .bind(state_id)
    .bind(type_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch incident issue");
    issue_id
}

async fn link_issue_to_service(
    pool: &PgPool,
    sc: &Scratch,
    service_id: Uuid,
    project_id: Uuid,
    issue_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(project_id)
    .bind(service_id)
    .bind(issue_id)
    .execute(pool)
    .await
    .expect("scratch service issue");
}

async fn link_service_to_war_room(
    pool: &PgPool,
    sc: &Scratch,
    war_room_id: Uuid,
    project_id: Uuid,
    service_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO war_room_services (id, workspace_id, project_id, war_room_id, service_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(project_id)
    .bind(war_room_id)
    .bind(service_id)
    .execute(pool)
    .await
    .expect("scratch war room service");
}

async fn war_room_primary_issue(pool: &PgPool, war_room_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT primary_issue_id FROM war_rooms WHERE id = $1")
        .bind(war_room_id)
        .fetch_one(pool)
        .await
        .expect("scratch war room primary issue")
}

fn incident_snapshot<'a>(body: &'a serde_json::Value, service_id: Uuid) -> &'a serde_json::Value {
    let expected = service_id.to_string();
    body.as_array()
        .unwrap()
        .iter()
        .find(|row| row["service_id"].as_str() == Some(expected.as_str()))
        .expect("service snapshot")
}

#[tokio::test]
async fn health_derives_from_open_incidents_and_war_rooms() {
    use api::routes::service_health::workspace_health;
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let open_issue = insert_incident(&st.pool, &sc, sc.p1, "DB latency", "high", "unstarted").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, open_issue).await;

    let completed_issue =
        insert_incident(&st.pool, &sc, sc.p1, "Old outage", "none", "completed").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, completed_issue).await;

    let room_id = insert_war_room(&st.pool, &sc, sc.p1).await;
    let room_issue = war_room_primary_issue(&st.pool, room_id).await;
    link_service_to_war_room(&st.pool, &sc, room_id, sc.p1, sc.s1).await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, room_issue).await;

    let p2_issue = insert_incident(&st.pool, &sc, sc.p2, "P2 incident", "none", "unstarted").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p2, p2_issue).await;

    let (status, _, Json(body)) = workspace_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1, "owner member sees only S1");

    let s1 = incident_snapshot(&body, sc.s1);
    assert_eq!(s1["health"], "degraded");
    assert_eq!(s1["last_deployed_at"], serde_json::Value::Null);
    let incidents = s1["incidents"].as_array().unwrap();
    assert_eq!(
        incidents.len(),
        2,
        "completed + inaccessible P2 incidents excluded; war room deduped"
    );
    assert_eq!(incidents[0]["severity"], "sev2", "highest severity first");
    assert!(incidents[0]["identifier"].as_str().unwrap().starts_with("P1-"));

    let room_id_string = room_id.to_string();
    let room_incident = incidents
        .iter()
        .find(|incident| incident["war_room_id"].as_str() == Some(room_id_string.as_str()))
        .expect("war room incident");
    assert_eq!(room_incident["war_room_sequence_id"].as_i64(), Some(1));
    assert_eq!(room_incident["severity"], "sev3");
    assert_eq!(room_incident["name"], "War room primary");

    let (_, _, Json(admin_body)) =
        workspace_health(State(st.clone()), AuthUser(sc.admin), Path(sc.slug.clone()))
            .await
            .unwrap();
    let admin_s1 = incident_snapshot(&admin_body, sc.s1);
    assert_eq!(
        admin_s1["incidents"].as_array().unwrap().len(),
        3,
        "workspace admin sees the P2 incident too"
    );
}

#[tokio::test]
async fn health_respects_project_scope_and_lifecycle() {
    use api::routes::service_health::{project_health, workspace_health};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let issue = insert_incident(&st.pool, &sc, sc.p1, "API down", "none", "unstarted").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, issue).await;

    let denied = project_health(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.p2)),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    let (_, _, Json(body)) = project_health(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2)),
    )
    .await
    .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2, "S1 consumer + S2 owner");
    assert_eq!(incident_snapshot(&body, sc.s1)["health"], "healthy");
    assert_eq!(incident_snapshot(&body, sc.s2)["health"], "healthy");

    let (_, _, Json(owner_body)) = project_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
    )
    .await
    .unwrap();
    assert_eq!(incident_snapshot(&owner_body, sc.s1)["health"], "degraded");
    assert_eq!(
        incident_snapshot(&owner_body, sc.s1)["incidents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    sqlx::query("UPDATE services SET status = 'planned' WHERE id = $1")
        .bind(sc.s1)
        .execute(&st.pool)
        .await
        .unwrap();
    let (_, _, Json(planned_body)) =
        workspace_health(State(st.clone()), AuthUser(sc.owner_member), Path(sc.slug.clone()))
            .await
            .unwrap();
    let planned = incident_snapshot(&planned_body, sc.s1);
    assert_eq!(planned["health"], "unknown");
    assert_eq!(planned["incidents"].as_array().unwrap().len(), 1);

    sqlx::query("UPDATE issues SET archived_at = CURRENT_DATE WHERE id = $1")
        .bind(issue)
        .execute(&st.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE services SET status = 'active' WHERE id = $1")
        .bind(sc.s1)
        .execute(&st.pool)
        .await
        .unwrap();
    let (_, _, Json(archived_body)) =
        workspace_health(State(st.clone()), AuthUser(sc.owner_member), Path(sc.slug.clone()))
            .await
            .unwrap();
    assert_eq!(incident_snapshot(&archived_body, sc.s1)["health"], "healthy");

    sqlx::query("UPDATE issues SET archived_at = NULL, state_id = NULL WHERE id = $1")
        .bind(issue)
        .execute(&st.pool)
        .await
        .unwrap();
    let (_, _, Json(no_state_body)) =
        workspace_health(State(st.clone()), AuthUser(sc.owner_member), Path(sc.slug.clone()))
            .await
            .unwrap();
    assert_eq!(incident_snapshot(&no_state_body, sc.s1)["health"], "degraded");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
cargo test -p api --test service_workspace_test health_ -- --test-threads=1
```

Expected: compile error, unresolved import `api::routes::service_health`.

- [ ] **Step 3: Implement the health module**

Create `apps/api-rs/crates/api/src/routes/service_health.rs`:

```rust
//! Read-time service health derived from open Incident work items and active war rooms.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    Json,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::{
        issue_common::is_workspace_admin,
        project::FORBIDDEN_MSG,
        service::gate_member,
        service_access,
        war_room::severity_from_priority,
    },
    state::AppState,
};

pub type HealthResponse = (
    StatusCode,
    [(header::HeaderName, &'static str); 1],
    Json<Value>,
);

#[derive(Debug, sqlx::FromRow)]
struct ServiceRow {
    id: Uuid,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct IncidentRow {
    service_id: Uuid,
    issue_id: Uuid,
    issue_name: String,
    identifier: String,
    priority: String,
    issue_created_at: DateTime<Utc>,
    war_room_id: Option<Uuid>,
    war_room_sequence_id: Option<i64>,
    war_room_name: Option<String>,
    war_room_severity: Option<String>,
    war_room_started_at: Option<DateTime<Utc>>,
}

fn incident_severity(row: &IncidentRow) -> &str {
    row.war_room_severity
        .as_deref()
        .unwrap_or_else(|| severity_from_priority(&row.priority))
}

fn incident_opened_at(row: &IncidentRow) -> &DateTime<Utc> {
    row.war_room_started_at
        .as_ref()
        .unwrap_or(&row.issue_created_at)
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "sev1" => 0,
        "sev2" => 1,
        "sev3" => 2,
        "sev4" => 3,
        _ => 4,
    }
}

async fn accessible_project_ids(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    is_admin: bool,
) -> Result<Option<Vec<Uuid>>, sqlx::Error> {
    if is_admin {
        return Ok(None);
    }
    let ids = sqlx::query_scalar(
        "SELECT DISTINCT pm.project_id FROM project_members pm \
         JOIN projects p ON p.id = pm.project_id AND p.deleted_at IS NULL \
         JOIN workspace_members wm ON wm.workspace_id = p.workspace_id \
           AND wm.member_id = pm.member_id AND wm.is_active = true AND wm.deleted_at IS NULL \
         JOIN workspaces w ON w.id = p.workspace_id AND w.slug = $2 \
         WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(slug)
    .fetch_all(pool)
    .await?;
    Ok(Some(ids))
}

async fn build_health(
    st: &AppState,
    user_id: Uuid,
    slug: &str,
    project_id: Option<Uuid>,
) -> Result<Value, sqlx::Error> {
    let is_admin = is_workspace_admin(&st.pool, user_id, slug).await?;
    let visible = if is_admin {
        None
    } else {
        Some(service_access::visible_service_ids(&st.pool, user_id, slug).await?)
    };
    let accessible = accessible_project_ids(&st.pool, user_id, slug, is_admin).await?;

    let services: Vec<ServiceRow> = sqlx::query_as(
        "SELECT sv.id, sv.status FROM services sv \
         JOIN workspaces w ON w.id = sv.workspace_id AND w.slug = $1 \
         WHERE sv.deleted_at IS NULL \
           AND ($2::uuid[] IS NULL OR sv.id = ANY($2)) \
           AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM service_project_links spl \
                 WHERE spl.service_id = sv.id AND spl.project_id = $3 AND spl.deleted_at IS NULL)) \
         ORDER BY sv.sort_order, sv.name",
    )
    .bind(slug)
    .bind(&visible)
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;

    if services.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    let service_ids: Vec<Uuid> = services.iter().map(|service| service.id).collect();

    let issue_rows: Vec<IncidentRow> = sqlx::query_as(
        "SELECT si.service_id, i.id AS issue_id, i.name AS issue_name, \
                p.identifier || '-' || i.sequence_id::text AS identifier, \
                i.priority, i.created_at AS issue_created_at, \
                wr.id AS war_room_id, wr.sequence_id AS war_room_sequence_id, \
                wr.name AS war_room_name, wr.severity AS war_room_severity, \
                wr.started_at AS war_room_started_at \
         FROM service_issues si \
         JOIN issues i ON i.id = si.issue_id AND i.deleted_at IS NULL AND i.archived_at IS NULL \
         JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
         LEFT JOIN states st ON st.id = i.state_id \
         LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
         LEFT JOIN war_rooms wr ON wr.primary_issue_id = i.id \
           AND wr.status IN ('active', 'monitoring') AND wr.deleted_at IS NULL \
         WHERE si.deleted_at IS NULL AND si.service_id = ANY($1) \
           AND lower(btrim(t.name)) = 'incident' \
           AND (st.\"group\" IS NULL OR st.\"group\" NOT IN ('completed', 'cancelled')) \
           AND ($2::uuid[] IS NULL OR i.project_id = ANY($2))",
    )
    .bind(&service_ids)
    .bind(&accessible)
    .fetch_all(&st.pool)
    .await?;

    let room_rows: Vec<IncidentRow> = sqlx::query_as(
        "SELECT rs.service_id, i.id AS issue_id, i.name AS issue_name, \
                p.identifier || '-' || i.sequence_id::text AS identifier, \
                i.priority, i.created_at AS issue_created_at, \
                wr.id AS war_room_id, wr.sequence_id AS war_room_sequence_id, \
                wr.name AS war_room_name, wr.severity AS war_room_severity, \
                wr.started_at AS war_room_started_at \
         FROM war_room_services rs \
         JOIN war_rooms wr ON wr.id = rs.war_room_id AND wr.deleted_at IS NULL \
           AND wr.status IN ('active', 'monitoring') \
         JOIN issues i ON i.id = wr.primary_issue_id AND i.deleted_at IS NULL AND i.archived_at IS NULL \
         JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
         WHERE rs.deleted_at IS NULL AND rs.service_id = ANY($1) \
           AND ($2::uuid[] IS NULL OR wr.project_id = ANY($2))",
    )
    .bind(&service_ids)
    .bind(&accessible)
    .fetch_all(&st.pool)
    .await?;

    let mut by_service: HashMap<Uuid, HashMap<Uuid, IncidentRow>> = HashMap::new();
    for row in issue_rows.into_iter().chain(room_rows) {
        let service_incidents = by_service.entry(row.service_id).or_default();
        match service_incidents.entry(row.issue_id) {
            Entry::Vacant(entry) => {
                entry.insert(row);
            }
            Entry::Occupied(mut entry) => {
                if entry.get().war_room_id.is_none() && row.war_room_id.is_some() {
                    entry.insert(row);
                }
            }
        }
    }

    let payload = services
        .iter()
        .map(|service| {
            let mut incidents = by_service
                .remove(&service.id)
                .map(|rows| rows.into_values().collect::<Vec<_>>())
                .unwrap_or_default();
            incidents.sort_by(|a, b| {
                severity_rank(incident_severity(a))
                    .cmp(&severity_rank(incident_severity(b)))
                    .then_with(|| incident_opened_at(b).cmp(incident_opened_at(a)))
            });
            let health = if service.status == "planned" || service.status == "retired" {
                "unknown"
            } else if incidents.is_empty() {
                "healthy"
            } else {
                "degraded"
            };
            let incident_values: Vec<Value> = incidents
                .into_iter()
                .map(|row| {
                    json!({
                        "id": row.issue_id,
                        "service_id": row.service_id,
                        "severity": incident_severity(&row),
                        "opened_at": incident_opened_at(&row).to_rfc3339(),
                        "identifier": row.identifier,
                        "name": row.issue_name,
                        "war_room_id": row.war_room_id,
                        "war_room_sequence_id": row.war_room_sequence_id,
                        "war_room_name": row.war_room_name,
                    })
                })
                .collect();
            json!({
                "service_id": service.id,
                "health": health,
                "incidents": incident_values,
                "last_deployed_at": Value::Null,
            })
        })
        .collect::<Vec<_>>();

    Ok(Value::Array(payload))
}

pub async fn workspace_health(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<HealthResponse, common::errors::AppError> {
    let payload = build_health(&st, auth.0, &slug, None).await?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(payload),
    ))
}

pub async fn project_health(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<HealthResponse, common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok((
            StatusCode::FORBIDDEN,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({ "error": FORBIDDEN_MSG })),
        ));
    }
    let payload = build_health(&st, auth.0, &slug, Some(project_id)).await?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(payload),
    ))
}
```

- [ ] **Step 4: Register the module and routes**

In `apps/api-rs/crates/api/src/routes/mod.rs`, add after `pub mod service_access;`:

```rust
pub mod service_health;
```

In `apps/api-rs/crates/api/src/main.rs`, add these routes immediately before the `/api/workspaces/:slug/projects/:project_id/services/:pk/` route and before the `/api/workspaces/:slug/services/:pk/` route:

```rust
        .route(
            "/api/workspaces/:slug/projects/:project_id/services/health/",
            get(routes::service_health::project_health),
        )
```

```rust
        .route(
            "/api/workspaces/:slug/services/health/",
            get(routes::service_health::workspace_health),
        )
```

- [ ] **Step 5: Run the backend tests**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
cargo test -p api --test service_workspace_test health_ -- --test-threads=1
```

Expected: `2 passed`.

- [ ] **Step 6: Format and commit**

Run from `apps/api-rs`:

```bash
cargo fmt -p api
```

From the repo root:

```bash
git add apps/api-rs/crates/api/src/routes/service_health.rs \
  apps/api-rs/crates/api/src/routes/mod.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): derive service health from incidents and war rooms"
```

---

### Task 2: Frontend health service, helpers, and store

**Files:**

- Modify: `packages/types/src/service/core.ts:17-29`
- Modify: `apps/web/core/services/service-health.helpers.ts`
- Modify: `apps/web/core/services/service-health.service.ts`
- Modify: `apps/web/core/store/service.store.ts`
- Test: `apps/web/core/services/service-health.helpers.test.ts`
- Test: `apps/web/core/services/service-health.service.test.ts`

- [ ] **Step 1: Write the failing health service test**

Create `apps/web/core/services/service-health.service.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { ServiceHealthService } from "./service-health.service";

describe("ServiceHealthService", () => {
  it("uses the workspace endpoint without a project", async () => {
    const service = new ServiceHealthService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: [] } as never);

    await expect(service.getHealth("itsm", null)).resolves.toEqual([]);
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/services/health/");
  });

  it("uses the project compat endpoint with a project", async () => {
    const service = new ServiceHealthService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: [] } as never);

    await expect(service.getHealth("itsm", "p1")).resolves.toEqual([]);
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/projects/p1/services/health/");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
pnpm --filter=web test -- core/services/service-health.service.test.ts
```

Expected: FAIL because `getHealth` still takes four arguments and returns mock snapshots.

- [ ] **Step 3: Implement the HTTP health service**

Replace `apps/web/core/services/service-health.service.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type { IServiceHealthSnapshot } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class ServiceHealthService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getHealth(workspaceSlug: string, projectId: string | null): Promise<IServiceHealthSnapshot[]> {
    const url = projectId
      ? `/api/workspaces/${workspaceSlug}/projects/${projectId}/services/health/`
      : `/api/workspaces/${workspaceSlug}/services/health/`;
    return this.get(url).then((response) => response?.data);
  }
}
```

- [ ] **Step 4: Write the failing helper tests**

Create `apps/web/core/services/service-health.helpers.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { IServiceIncident } from "@plane/types";
import { getHighestSeverityIncident, getIncidentTooltipWindow } from "./service-health.helpers";

const incident = (
  id: string,
  severity: IServiceIncident["severity"],
  openedAt: string,
  overrides: Partial<IServiceIncident> = {}
): IServiceIncident => ({
  id,
  service_id: "service",
  severity,
  opened_at: openedAt,
  ...overrides,
});

describe("service health helpers", () => {
  it("sorts by severity, then newest first", () => {
    const incidents = [
      incident("sev4", "sev4", "2026-10-24T01:00:00Z"),
      incident("sev1-old", "sev1", "2026-10-24T01:00:00Z"),
      incident("sev1-new", "sev1", "2026-10-24T02:00:00Z"),
    ];

    expect(getHighestSeverityIncident(incidents)?.id).toBe("sev1-new");
    expect(getIncidentTooltipWindow(incidents).visible.map((item) => item.id)).toEqual([
      "sev1-new",
      "sev1-old",
      "sev4",
    ]);
  });

  it("caps the tooltip at five incidents and reports the remainder", () => {
    const incidents = Array.from({ length: 7 }, (_, index) =>
      incident(`incident-${index}`, "sev3", "2026-10-24T01:00:00Z")
    );

    const window = getIncidentTooltipWindow(incidents);
    expect(window.visible).toHaveLength(5);
    expect(window.remaining).toBe(2);
  });
});
```

- [ ] **Step 5: Run the helper test to verify it fails**

Run:

```bash
pnpm --filter=web test -- core/services/service-health.helpers.test.ts
```

Expected: FAIL because `getIncidentTooltipWindow` is not exported.

- [ ] **Step 6: Replace the mock helpers and extend the incident type**

Replace `apps/web/core/services/service-health.helpers.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IServiceIncident, TServiceHealth, TServiceIncidentSeverity } from "@plane/types";

export const HEALTH_WEIGHT: Record<TServiceHealth, number> = {
  down: 0,
  degraded: 1,
  unknown: 2,
  healthy: 3,
};

export const INCIDENT_SEVERITY_RANK: Record<TServiceIncidentSeverity, number> = {
  sev1: 0,
  sev2: 1,
  sev3: 2,
  sev4: 3,
};

/** Highest-severity incident in a list (`sev1` is highest; ties break newest first). */
export const getHighestSeverityIncident = (incidents: IServiceIncident[]): IServiceIncident | null =>
  incidents.reduce<IServiceIncident | null>(
    (highest, incident) =>
      !highest ||
      INCIDENT_SEVERITY_RANK[incident.severity] < INCIDENT_SEVERITY_RANK[highest.severity] ||
      (INCIDENT_SEVERITY_RANK[incident.severity] === INCIDENT_SEVERITY_RANK[highest.severity] &&
        new Date(incident.opened_at).getTime() > new Date(highest.opened_at).getTime())
        ? incident
        : highest,
    null
  );

/** Severity first (`sev1`), then most recently opened. */
export const sortIncidentsBySeverity = (incidents: IServiceIncident[]): IServiceIncident[] => {
  const sorted: IServiceIncident[] = [...incidents];
  sorted.sort((a, b) => {
    const rank = INCIDENT_SEVERITY_RANK[a.severity] - INCIDENT_SEVERITY_RANK[b.severity];
    if (rank !== 0) return rank;
    return new Date(b.opened_at).getTime() - new Date(a.opened_at).getTime();
  });
  return sorted;
};

export const getIncidentTooltipWindow = (incidents: IServiceIncident[], limit = 5) => {
  const sorted = sortIncidentsBySeverity(incidents);
  return {
    visible: sorted.slice(0, limit),
    remaining: Math.max(0, sorted.length - limit),
  };
};
```

In `packages/types/src/service/core.ts`, replace `IServiceIncident` with:

```ts
export interface IServiceIncident {
  id: string;
  service_id: string;
  severity: TServiceIncidentSeverity;
  opened_at: string;
  identifier?: string;
  name?: string;
  war_room_id?: string | null;
  war_room_sequence_id?: number | null;
  war_room_name?: string | null;
}
```

- [ ] **Step 7: Run both frontend tests**

Run:

```bash
pnpm --filter=web test -- core/services/service-health.helpers.test.ts core/services/service-health.service.test.ts
```

Expected: PASS (`4 passed`).

- [ ] **Step 8: Wire the store to the HTTP service**

In `apps/web/core/store/service.store.ts`:

1. Add to `IServiceStore` after `setPeekService`:

```ts
refreshHealth: (workspaceSlug: string, projectId: string | null) => Promise<void>;
```

2. Add `refreshHealth: action,` after `fetchWorkspaceServices: action,` in `makeObservable`.

3. In `fetchServices`, replace the health block with:

```ts
try {
  const health = await this.serviceHealthService.getHealth(workspaceSlug, projectId);
  runInAction(() => {
    health.forEach((snapshot) => {
      set(this.healthMap, [snapshot.service_id], snapshot);
    });
  });
} catch (error) {
  console.error("Failed to fetch service health", error);
}
```

4. In `fetchWorkspaceServices`, replace the health call with:

```ts
const health = await this.serviceHealthService.getHealth(workspaceSlug, projectId ?? null);
```

5. Add this method before `createService`:

```ts
refreshHealth = async (workspaceSlug: string, projectId: string | null) => {
  try {
    const health = await this.serviceHealthService.getHealth(workspaceSlug, projectId);
    runInAction(() => {
      health.forEach((snapshot) => {
        set(this.healthMap, [snapshot.service_id], snapshot);
      });
    });
  } catch (error) {
    console.error("Failed to refresh service health", error);
  }
};
```

6. In `removeLocalService`, add `delete this.healthMap[serviceId];` after the `workItemLinkMap` cleanup.

- [ ] **Step 9: Typecheck and commit**

Run:

```bash
pnpm turbo run check:types check:lint --filter=web
```

Expected: pass (pre-existing warnings allowed).

```bash
git add packages/types/src/service/core.ts \
  apps/web/core/services/service-health.helpers.ts \
  apps/web/core/services/service-health.service.ts \
  apps/web/core/services/service-health.helpers.test.ts \
  apps/web/core/services/service-health.service.test.ts \
  apps/web/core/store/service.store.ts
git commit -m "feat(web): consume real service health from api"
```

---

### Task 3: i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/service.json`
- Modify: all 18 other locale `service.json` files under `packages/i18n/src/locales/`

- [ ] **Step 1: Add the English keys**

In `packages/i18n/src/locales/en/service.json`, replace the `"incidents"` object and add a top-level `"deploy"` object after it:

```json
    "incidents": {
      "active": "Has active incidents",
      "tooltip_title": "Active incidents",
      "more": "{count, plural, one {# more} other {# more}}"
    },
    "deploy": {
      "no_deployment": "No deployment"
    },
```

- [ ] **Step 2: Translate the keys into every locale**

Use these exact values in each locale's `service.json` (same placement):

| Locale  | `deploy.no_deployment` | `incidents.tooltip_title` | `incidents.more`                                                                    |
| ------- | ---------------------- | ------------------------- | ----------------------------------------------------------------------------------- |
| `id`    | Belum ada deployment   | Insiden aktif             | `{count, plural, one {# lainnya} other {# lainnya}}`                                |
| `de`    | Keine Bereitstellung   | Aktive Vorfälle           | `{count, plural, one {noch #} other {noch #}}`                                      |
| `fr`    | Aucun déploiement      | Incidents actifs          | `{count, plural, one {# de plus} many {# de plus} other {# de plus}}`               |
| `es`    | Sin despliegue         | Incidentes activos        | `{count, plural, one {# más} other {# más}}`                                        |
| `it`    | Nessuna distribuzione  | Incidenti attivi          | `{count, plural, one {# altro} many {# altri} other {# altri}}`                     |
| `pt-BR` | Nenhuma implantação    | Incidentes ativos         | `{count, plural, one {mais #} many {mais #} other {mais #}}`                        |
| `pl`    | Brak wdrożenia         | Aktywne incydenty         | `{count, plural, one {# więcej} few {# więcej} many {# więcej} other {# więcej}}`   |
| `cs`    | Žádné nasazení         | Aktivní incidenty         | `{count, plural, one {# další} few {# další} many {# dalšího} other {# dalších}}`   |
| `sk`    | Žiadne nasadenie       | Aktívne incidenty         | `{count, plural, one {# ďalší} few {# ďalšie} many {# ďalšieho} other {# ďalších}}` |
| `ro`    | Nicio implementare     | Incidente active          | `{count, plural, one {încă #} few {încă #} other {încă #}}`                         |
| `ru`    | Нет развёртывания      | Активные инциденты        | `{count, plural, one {ещё #} few {ещё #} many {ещё #} other {ещё #}}`               |
| `ua`    | Немає розгортання      | Активні інциденти         | `{count, plural, one {ще #} few {ще #} many {ще #} other {ще #}}`                   |
| `tr-TR` | Dağıtım yok            | Etkin olaylar             | `{count, plural, one {# daha} other {# daha}}`                                      |
| `vi-VN` | Chưa có triển khai     | Sự cố đang hoạt động      | `{count, plural, one {thêm #} other {thêm #}}`                                      |
| `ja`    | デプロイなし           | アクティブなインシデント  | `{count, plural, one {他 # 件} other {他 # 件}}`                                    |
| `ko`    | 배포 없음              | 활성 인시던트             | `{count, plural, one {#개 더} other {#개 더}}`                                      |
| `zh-CN` | 无部署                 | 活跃事故                  | `{count, plural, one {另有 # 个} other {另有 # 个}}`                                |
| `zh-TW` | 無部署                 | 活躍事故                  | `{count, plural, one {另有 # 個} other {另有 # 個}}`                                |
| `ka-ge` | განლაგება არ არის      | აქტიური ინციდენტები       | `{count, plural, one {კიდევ #} other {კიდევ #}}`                                    |

- [ ] **Step 3: Regenerate types and verify sync**

Run:

```bash
pnpm --filter=@plane/i18n run generate:types
pnpm --filter=@plane/i18n run check:sync
```

Expected: the three new `service.*` keys present in every locale. Note: the full-repo
`check:sync` currently reports pre-existing drift in unrelated namespaces
(`auth`, `home`, `project_settings`); do not touch those files in this plan.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "feat(i18n): add service health and deployment copy"
```

---

### Task 4: Catalog UI, tooltip, deployment copy, and polling

**Files:**

- Modify: `apps/web/core/components/services/health/service-incident-cell.tsx`
- Modify: `apps/web/core/components/services/health/service-deploy-cell.tsx`
- Modify: `apps/web/core/components/services/graph/service-node.tsx`
- Modify: `apps/web/core/components/services/graph/service-graph-canvas.tsx`
- Create: `apps/web/core/hooks/use-service-health-polling.ts`
- Modify: `apps/web/core/components/services/services-list-view.tsx`

- [ ] **Step 1: Replace the incident cell with real data + tooltip**

Replace `apps/web/core/components/services/health/service-incident-cell.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { useTranslation } from "@plane/i18n";
import type { IServiceIncident } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident, getIncidentTooltipWindow } from "@/services/service-health.helpers";

type Props = {
  incidents: IServiceIncident[];
};

export function ServiceIncidentCell({ incidents }: Props) {
  const { t } = useTranslation();
  const highest = getHighestSeverityIncident(incidents);
  if (incidents.length === 0 || !highest) {
    return <span className="text-12 text-tertiary">—</span>;
  }
  const { visible, remaining } = getIncidentTooltipWindow(incidents);
  const tooltipLabel = [
    t("service.incidents.tooltip_title"),
    ...visible.map((incident) => {
      const parts = [incident.identifier, incident.name].filter((part): part is string => Boolean(part));
      if (incident.war_room_sequence_id) parts.push(`WR-${incident.war_room_sequence_id}`);
      parts.push(t(`service.incident_severity.${incident.severity}`));
      return parts.join(" · ");
    }),
    ...(remaining > 0 ? [t("service.incidents.more", { count: remaining })] : []),
  ].join(" · ");
  const isSevere = highest.severity === "sev1" || highest.severity === "sev2";
  return (
    <Tooltip label={tooltipLabel} layout="stacked">
      <span
        tabIndex={0}
        aria-label={tooltipLabel}
        className={cn(
          "flex items-center gap-1 text-12 font-medium focus-visible:outline-none",
          isSevere ? "text-danger-primary" : "text-warning-primary"
        )}
      >
        <WarningTriangleOutline className="h-3.5 w-3.5" />
        <span className="font-code tabular-nums">{incidents.length}</span>
        <span className="text-tertiary">·</span>
        <span>{t(`service.incident_severity.${highest.severity}`)}</span>
      </span>
    </Tooltip>
  );
}
```

- [ ] **Step 2: Show "No deployment" when the timestamp is null**

Replace `apps/web/core/components/services/health/service-deploy-cell.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ClockOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { calculateTimeAgo } from "@plane/utils";

type Props = {
  lastDeployedAt: string | null;
};

export function ServiceDeployCell({ lastDeployedAt }: Props) {
  const { t } = useTranslation();
  if (!lastDeployedAt) {
    return <span className="text-12 text-tertiary">{t("service.deploy.no_deployment")}</span>;
  }
  return (
    <span className="flex items-center gap-1 text-12 text-secondary">
      <ClockOutline className="h-3.5 w-3.5 text-tertiary" />
      <span className="font-code tabular-nums">{calculateTimeAgo(lastDeployedAt)}</span>
    </span>
  );
}
```

- [ ] **Step 3: Show the same copy on graph nodes**

In `apps/web/core/components/services/graph/service-node.tsx`:

1. Add `noDeploymentLabel: string;` after `lastDeployedAt?: string | null;` in `TServiceNodeData`.
2. Destructure `noDeploymentLabel` next to `lastDeployedAt`.
3. Replace the footer branch:

```tsx
          ) : (
            <span className="inline-flex max-w-[96px] shrink-0 items-center gap-1 font-code text-10 text-tertiary tabular-nums">
              <ClockOutline className="h-3 w-3 shrink-0 text-tertiary" />
              <span className="truncate">{noDeploymentLabel}</span>
            </span>
          )}
```

In `apps/web/core/components/services/graph/service-graph-canvas.tsx`:

1. Add `noDeploymentLabel: t("service.deploy.no_deployment"),` next to `lastDeployedAt`.
2. Add `noDeploymentLabel?: string;` to both inline comparator data types and compare `prevData?.noDeploymentLabel === nextData?.noDeploymentLabel`.

- [ ] **Step 4: Add health polling**

Create `apps/web/core/hooks/use-service-health-polling.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

const SERVICE_HEALTH_REFRESH_MS = 60_000;

export const useServiceHealthPolling = (workspaceSlug: string | undefined, projectId: string | null) => {
  const { refreshHealth } = useService();
  const { currentWorkspace } = useWorkspace();

  useEffect(() => {
    if (!workspaceSlug || !currentWorkspace?.id) return;

    const refresh = () => {
      void refreshHealth(workspaceSlug, projectId);
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") refresh();
    };

    const interval = window.setInterval(refresh, SERVICE_HEALTH_REFRESH_MS);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [currentWorkspace?.id, projectId, refreshHealth, workspaceSlug]);
};
```

- [ ] **Step 5: Wire polling into the catalog**

In `apps/web/core/components/services/services-list-view.tsx`:

1. Add near the other hook imports:

```ts
import { useServiceHealthPolling } from "@/hooks/use-service-health-polling";
```

2. Call it after `const workspaceId = currentWorkspace?.id;`:

```ts
useServiceHealthPolling(
  workspaceSlug?.toString(),
  workspaceMode ? (scopeProjectId ?? null) : (projectId?.toString() ?? null)
);
```

- [ ] **Step 6: Run tests, typecheck, and lint**

Run:

```bash
pnpm --filter=web test -- core/services/service-health.helpers.test.ts core/services/service-health.service.test.ts
pnpm turbo run check:types check:lint --filter=web
```

Expected: tests pass; typecheck/lint pass with only pre-existing warnings.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/services/health/service-incident-cell.tsx \
  apps/web/core/components/services/health/service-deploy-cell.tsx \
  apps/web/core/components/services/graph/service-node.tsx \
  apps/web/core/components/services/graph/service-graph-canvas.tsx \
  apps/web/core/hooks/use-service-health-polling.ts \
  apps/web/core/components/services/services-list-view.tsx
git commit -m "feat(web): show real incidents and no-deployment state"
```

---

### Task 5: Verification and rollout

**Files:** none.

- [ ] **Step 1: Run the full relevant checks**

```bash
pnpm turbo run check:types check:lint --filter=web
pnpm --filter=@plane/i18n run check:sync
```

Expected: both pass; i18n sync reports `0 missing, 0 stale, 0 collisions`.

- [ ] **Step 2: Rebuild api-rs detached**

Run from the repo root:

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker \
  > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll:

```bash
tail -n 20 /tmp/plane-api-build.log
```

Expected: build completes; Rust LTO link may take 10+ minutes without output. Then:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200`.

- [ ] **Step 3: Restart live and verify**

```bash
systemctl --user restart plane-live.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
```

Expected: `200`.

- [ ] **Step 4: Build and restart the web app**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000
```

Expected: `200`.

- [ ] **Step 5: Smoke test**

In the UI, open the workspace Services catalog and verify:

- Services with open Incident work items or active war rooms show `degraded` and a real incident count.
- Services with no active incidents show `healthy`; planned/retired show `unknown`.
- The incident cell tooltip lists the incident identifier, name, war room `WR-{n}`, and severity.
- The `Has active incidents` filter matches the displayed counts.
- Last deploy shows "No deployment" in board/list, sidebar, and graph node.
- A non-admin member does not see incidents from projects they cannot access.

- [ ] **Step 6: Commit any final docs**

If the spec was adjusted during implementation, commit it:

```bash
git add docs/superpowers/specs/2026-10-24-service-health-real-design.md
git commit -m "docs: align service health design with implementation"
```
