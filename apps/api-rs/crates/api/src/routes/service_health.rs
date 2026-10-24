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
        issue_common::is_workspace_admin, project::FORBIDDEN_MSG, service::gate_member,
        service_access, war_room::severity_from_priority,
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
