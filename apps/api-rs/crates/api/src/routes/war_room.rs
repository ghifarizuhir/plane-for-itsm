use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::issue_common::{fetch_project_member_role, is_workspace_admin};
use super::service::{bad_request, gate_member, gate_writer, validate_enum};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarRoomRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub sequence_id: i64,
    pub name: String,
    pub description_html: String,
    pub notes_html: String,
    pub severity: String,
    pub status: String,
    pub primary_issue_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const WAR_ROOM_SELECT: &str = "SELECT r.id, r.workspace_id, r.project_id, r.sequence_id, \
    r.name, r.description_html, r.notes_html, r.severity, r.status, r.primary_issue_id, \
    r.started_at, r.resolved_at, r.created_at, r.updated_at, r.created_by_id, r.updated_by_id \
    FROM war_rooms r";

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomServiceRow {
    id: Uuid,
    name: String,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LinkedIssueRow {
    id: Uuid,
    identifier: String,
    name: String,
    priority: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ParticipantRow {
    pub id: Uuid,
    pub member_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunbookItemRow {
    pub id: Uuid,
    pub title: String,
    pub sort_order: f64,
    pub is_done: bool,
    pub done_by_id: Option<Uuid>,
    pub done_at: Option<chrono::DateTime<chrono::Utc>>,
    pub template_key: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarRoomEventRow {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub fn room_base_json(r: &WarRoomRow) -> Value {
    serde_json::json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "sequence_id": r.sequence_id,
        "name": r.name,
        "description_html": r.description_html,
        "notes_html": r.notes_html,
        "severity": r.severity,
        "status": r.status,
        "primary_issue_id": r.primary_issue_id,
        "started_at": r.started_at,
        "resolved_at": r.resolved_at,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
    })
}

fn service_json(r: &RoomServiceRow) -> Value {
    serde_json::json!({ "id": r.id, "name": r.name, "status": r.status })
}

fn linked_issue_json(r: &LinkedIssueRow) -> Value {
    serde_json::json!({
        "id": r.id, "identifier": r.identifier, "name": r.name, "priority": r.priority
    })
}

pub fn participant_json(r: &ParticipantRow) -> Value {
    serde_json::json!({
        "id": r.id, "member_id": r.member_id, "role": r.role, "joined_at": r.joined_at,
        "display_name": r.display_name, "avatar_url": r.avatar_url,
    })
}

pub fn runbook_item_json(r: &RunbookItemRow) -> Value {
    serde_json::json!({
        "id": r.id, "title": r.title, "sort_order": r.sort_order, "is_done": r.is_done,
        "done_by_id": r.done_by_id, "done_at": r.done_at, "template_key": r.template_key,
    })
}

pub fn event_json(r: &WarRoomEventRow) -> Value {
    serde_json::json!({
        "id": r.id, "actor_id": r.actor_id, "event_type": r.event_type,
        "payload": r.payload, "created_at": r.created_at,
    })
}

async fn fetch_room(
    pool: &PgPool,
    project_id: Uuid,
    pk: Uuid,
) -> Result<Option<WarRoomRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{WAR_ROOM_SELECT} WHERE r.id = $1 AND r.project_id = $2 AND r.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

async fn room_services(pool: &PgPool, room_id: Uuid) -> Result<Vec<RoomServiceRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.id, s.name, s.status FROM war_room_services l \
         JOIN services s ON s.id = l.service_id \
         WHERE l.war_room_id = $1 AND l.deleted_at IS NULL AND s.deleted_at IS NULL \
         ORDER BY s.name ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

async fn room_issues(pool: &PgPool, room_id: Uuid) -> Result<Vec<LinkedIssueRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id, p.identifier || '-' || i.sequence_id AS identifier, i.name, i.priority \
         FROM war_room_issues l JOIN issues i ON i.id = l.issue_id \
         JOIN projects p ON p.id = i.project_id \
         WHERE l.war_room_id = $1 AND l.deleted_at IS NULL AND i.deleted_at IS NULL \
         ORDER BY l.created_at ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

pub async fn room_participants(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.member_id, p.role, p.joined_at, u.display_name, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
           THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
         FROM war_room_participants p JOIN users u ON u.id = p.member_id \
         WHERE p.war_room_id = $1 AND p.deleted_at IS NULL ORDER BY p.joined_at ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

pub async fn room_runbook(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Vec<RunbookItemRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, sort_order, is_done, done_by_id, done_at, template_key \
         FROM war_room_runbook_items WHERE war_room_id = $1 AND deleted_at IS NULL \
         ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

async fn message_count(pool: &PgPool, room_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM war_room_messages WHERE war_room_id = $1 AND deleted_at IS NULL",
    )
    .bind(room_id)
    .fetch_one(pool)
    .await
}

async fn primary_issue_json(
    pool: &PgPool,
    project_id: Uuid,
    issue_id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Uuid, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT i.id, p.identifier || '-' || i.sequence_id, i.name, i.priority, s.\"group\" \
         FROM issues i JOIN projects p ON p.id = i.project_id \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, identifier, name, priority, state_group)| {
        serde_json::json!({
            "id": id, "identifier": identifier, "name": name,
            "priority": priority, "state_group": state_group,
        })
    }))
}

pub async fn room_detail_json(pool: &PgPool, room: &WarRoomRow) -> Result<Value, sqlx::Error> {
    let services = room_services(pool, room.id).await?;
    let issues = room_issues(pool, room.id).await?;
    let participants = room_participants(pool, room.id).await?;
    let runbook = room_runbook(pool, room.id).await?;
    let messages = message_count(pool, room.id).await?;
    let primary_issue = primary_issue_json(pool, room.project_id, room.primary_issue_id).await?;
    let mut json = room_base_json(room);
    json["primary_issue"] = primary_issue.unwrap_or(Value::Null);
    json["services"] = Value::Array(services.iter().map(service_json).collect());
    json["issues"] = Value::Array(issues.iter().map(linked_issue_json).collect());
    json["participants"] = Value::Array(participants.iter().map(participant_json).collect());
    json["runbook_items"] = Value::Array(runbook.iter().map(runbook_item_json).collect());
    json["counts"] = serde_json::json!({ "messages": messages });
    Ok(json)
}

// ---------------------------------------------------------------------------
// List + summary
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub q: Option<String>,
}

fn parse_csv(value: &Option<String>) -> Vec<String> {
    value
        .as_deref()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn rooms_by_room_id<T>(rows: Vec<(Uuid, T)>) -> HashMap<Uuid, Vec<T>> {
    let mut map: HashMap<Uuid, Vec<T>> = HashMap::new();
    for (room_id, value) in rows {
        map.entry(room_id).or_default().push(value);
    }
    map
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Query(params): Query<ListParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let statuses = parse_csv(&params.status);
    for s in &statuses {
        if let Err(e) = validate_enum("status", s, WAR_ROOM_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    let severities = parse_csv(&params.severity);
    for s in &severities {
        if let Err(e) = validate_enum("severity", s, WAR_ROOM_SEVERITIES) {
            return Ok(bad_request(e));
        }
    }
    let q = params
        .q
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let status_filter = if statuses.is_empty() {
        None
    } else {
        Some(statuses)
    };
    let severity_filter = if severities.is_empty() {
        None
    } else {
        Some(severities)
    };

    let rooms: Vec<WarRoomRow> = sqlx::query_as(&format!(
        "{WAR_ROOM_SELECT} WHERE r.project_id = $1 AND r.deleted_at IS NULL \
         AND ($2::text[] IS NULL OR r.status = ANY($2)) \
         AND ($3::text[] IS NULL OR r.severity = ANY($3)) \
         AND ($4::text IS NULL OR r.name ILIKE '%' || $4 || '%' OR EXISTS ( \
            SELECT 1 FROM issues i JOIN projects p ON p.id = i.project_id \
            WHERE i.id = r.primary_issue_id \
              AND (i.name ILIKE '%' || $4 || '%' OR (p.identifier || '-' || i.sequence_id) ILIKE '%' || $4 || '%') \
         )) \
         ORDER BY array_position(ARRAY['active','monitoring','resolved','archived'], r.status), \
                  r.severity ASC, r.started_at DESC"
    ))
    .bind(project_id)
    .bind(&status_filter)
    .bind(&severity_filter)
    .bind(&q)
    .fetch_all(&st.pool)
    .await?;

    let room_ids: Vec<Uuid> = rooms.iter().map(|r| r.id).collect();
    let services: HashMap<Uuid, Vec<RoomServiceRow>> = rooms_by_room_id(
        sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
            "SELECT l.war_room_id, s.id, s.name, s.status FROM war_room_services l \
             JOIN services s ON s.id = l.service_id \
             WHERE l.war_room_id = ANY($1) AND l.deleted_at IS NULL AND s.deleted_at IS NULL \
             ORDER BY s.name ASC",
        )
        .bind(&room_ids)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|(room_id, id, name, status)| (room_id, RoomServiceRow { id, name, status }))
        .collect::<Vec<_>>(),
    );
    let primary_issues: HashMap<Uuid, Value> = sqlx::query_as::<
        _,
        (Uuid, Uuid, String, String, String, Option<String>),
    >(
        "SELECT r.id, i.id, p.identifier || '-' || i.sequence_id, i.name, i.priority, s.\"group\" \
         FROM war_rooms r JOIN issues i ON i.id = r.primary_issue_id \
         JOIN projects p ON p.id = i.project_id \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE r.id = ANY($1) AND i.deleted_at IS NULL",
    )
    .bind(&room_ids)
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .map(
        |(room_id, issue_id, identifier, name, priority, state_group)| {
            (
                room_id,
                serde_json::json!({
                    "id": issue_id, "identifier": identifier, "name": name,
                    "priority": priority, "state_group": state_group,
                }),
            )
        },
    )
    .collect();
    let participants: HashMap<Uuid, Vec<ParticipantRow>> = rooms_by_room_id(
        sqlx::query_as(
            "SELECT p.war_room_id, p.id, p.member_id, p.role, p.joined_at, u.display_name, \
             CASE WHEN u.avatar_asset_id IS NOT NULL \
               THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
             FROM war_room_participants p JOIN users u ON u.id = p.member_id \
             WHERE p.war_room_id = ANY($1) AND p.deleted_at IS NULL ORDER BY p.joined_at ASC",
        )
        .bind(&room_ids)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|row: ParticipantListRow| {
            let room_id = row.war_room_id;
            (
                room_id,
                ParticipantRow {
                    id: row.id,
                    member_id: row.member_id,
                    role: row.role,
                    joined_at: row.joined_at,
                    display_name: row.display_name,
                    avatar_url: row.avatar_url,
                },
            )
        })
        .collect::<Vec<_>>(),
    );
    let message_counts: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT war_room_id, COUNT(*) FROM war_room_messages \
         WHERE war_room_id = ANY($1) AND deleted_at IS NULL GROUP BY war_room_id",
    )
    .bind(&room_ids)
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .collect();
    let last_activity: HashMap<Uuid, chrono::DateTime<chrono::Utc>> =
        sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
            "SELECT room_id, MAX(ts) FROM ( \
            SELECT war_room_id AS room_id, MAX(created_at) AS ts FROM war_room_messages \
            WHERE war_room_id = ANY($1) AND deleted_at IS NULL GROUP BY war_room_id \
            UNION ALL \
            SELECT war_room_id, MAX(created_at) FROM war_room_events \
            WHERE war_room_id = ANY($1) GROUP BY war_room_id \
         ) x GROUP BY room_id",
        )
        .bind(&room_ids)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .filter_map(|(room_id, ts)| ts.map(|t| (room_id, t)))
        .collect();

    let items: Vec<Value> = rooms
        .iter()
        .map(|r| {
            let svc = services.get(&r.id).cloned().unwrap_or_default();
            let parts = participants.get(&r.id).cloned().unwrap_or_default();
            let mut json = room_base_json(r);
            json["primary_issue"] = primary_issues.get(&r.id).cloned().unwrap_or(Value::Null);
            json["services"] = Value::Array(svc.iter().map(service_json).collect());
            json["participants"] = Value::Array(parts.iter().map(participant_json).collect());
            json["service_count"] = serde_json::json!(svc.len());
            json["participant_count"] = serde_json::json!(parts.len());
            json["message_count"] =
                serde_json::json!(message_counts.get(&r.id).copied().unwrap_or(0));
            json["last_activity_at"] = serde_json::json!(last_activity.get(&r.id));
            json
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(items))))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ParticipantListRow {
    war_room_id: Uuid,
    id: Uuid,
    member_id: Uuid,
    role: String,
    joined_at: chrono::DateTime<chrono::Utc>,
    display_name: Option<String>,
    avatar_url: Option<String>,
}

pub async fn summary(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           COUNT(*) FILTER (WHERE status IN ('active','monitoring')), \
           COUNT(*) FILTER (WHERE status IN ('active','monitoring') AND severity IN ('sev1','sev2')), \
           COUNT(*) FILTER (WHERE status = 'resolved' AND resolved_at >= now() - interval '7 days') \
         FROM war_rooms WHERE project_id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "active": row.0, "sev1_2": row.1, "resolved_7d": row.2
        })),
    ))
}

/// Allowed `status` values (`packages/types/src/war-room/core.ts`).
pub const WAR_ROOM_STATUSES: &[&str] = &["active", "monitoring", "resolved", "archived"];
/// Allowed `severity` values.
pub const WAR_ROOM_SEVERITIES: &[&str] = &["sev1", "sev2", "sev3", "sev4"];
/// Allowed participant roles (coordination labels, not permission gates).
pub const PARTICIPANT_ROLES: &[&str] = &["commander", "comms", "scribe", "responder"];

/// Default severity derived from the incident priority.
pub fn severity_from_priority(priority: &str) -> &'static str {
    match priority {
        "urgent" => "sev1",
        "high" => "sev2",
        "medium" => "sev3",
        _ => "sev4",
    }
}

pub fn is_active_status(status: &str) -> bool {
    status == "active" || status == "monitoring"
}

/// Allowed status transitions; `archived` is terminal.
pub fn transitions_allowed(status: &str) -> &'static [&'static str] {
    match status {
        "active" => &["monitoring", "resolved", "archived"],
        "monitoring" => &["active", "resolved", "archived"],
        "resolved" => &["active", "archived"],
        _ => &[],
    }
}

pub fn status_transition_allowed(from: &str, to: &str) -> bool {
    transitions_allowed(from).contains(&to)
}

/// Extract unique user ids from `@{uuid}` tokens. Invalid tokens are ignored.
/// Mentions are stored raw here; the handler filters them to workspace members.
pub fn parse_mentions(body: &str) -> Vec<Uuid> {
    let mut mentions: Vec<Uuid> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("@{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        if let Ok(user_id) = Uuid::parse_str(&after[..end]) {
            if !mentions.contains(&user_id) {
                mentions.push(user_id);
            }
        }
        rest = &after[end + 1..];
    }
    mentions
}

/// Built-in runbook per work item type name (lowercased, trimmed).
/// Returns `(template_key, title)` pairs.
pub fn runbook_template(type_name: Option<&str>) -> Vec<(&'static str, &'static str)> {
    match type_name.map(|n| n.trim().to_lowercase()).as_deref() {
        Some("incident") => vec![
            ("triage", "Triage & assess impact"),
            ("mitigate", "Mitigate (rollback/redeploy)"),
            ("communicate", "Communicate status update"),
            ("verify", "Verify recovery & monitor"),
            ("postmortem", "Schedule postmortem"),
        ],
        Some("problem") => vec![
            ("confirm_cause", "Confirm root cause hypothesis"),
            ("evidence", "Collect evidence & timeline"),
            ("fix", "Identify permanent fix"),
            ("change_plan", "Create change plan"),
            ("knowledge", "Update knowledge base"),
        ],
        Some("change") => vec![
            ("pre_verify", "Pre-change verification"),
            ("execute", "Execute change steps"),
            ("validate", "Validate service health"),
            ("rollback", "Rollback if needed"),
            ("close", "Close change record"),
        ],
        Some("request") => vec![
            ("requester", "Confirm requester details"),
            ("steps", "Check fulfilment steps"),
            ("fulfil", "Execute fulfilment"),
            ("notify", "Notify requester"),
        ],
        _ => vec![],
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateWarRoom {
    pub name: Option<String>,
    pub primary_issue_id: Uuid,
    pub severity: Option<String>,
    pub description_html: Option<String>,
    pub service_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IssueSummaryRow {
    pub id: Uuid,
    pub name: String,
    pub priority: String,
    pub type_name: Option<String>,
}

async fn fetch_issue_summary(
    pool: &PgPool,
    project_id: Uuid,
    issue_id: Uuid,
) -> Result<Option<IssueSummaryRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id, i.name, i.priority, t.name AS type_name \
         FROM issues i LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
         WHERE i.id = $1 AND i.project_id = $2 AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

async fn issue_assignees(pool: &PgPool, issue_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT assignee_id FROM issue_assignees \
         WHERE issue_id = $1 AND deleted_at IS NULL ORDER BY created_at ASC",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
}

pub async fn record_event(
    pool: &PgPool,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<WarRoomEventRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO war_room_events (id, workspace_id, project_id, war_room_id, actor_id, \
         event_type, payload, created_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, now()) \
         RETURNING id, actor_id, event_type, payload, created_at",
    )
    .bind(room.workspace_id)
    .bind(room.project_id)
    .bind(room.id)
    .bind(actor)
    .bind(event_type)
    .bind(payload)
    .fetch_one(pool)
    .await
}

/// Realtime channel consumed by `apps/live` (`war-room-relay.service.ts`).
pub const WAR_ROOM_CHANNEL: &str = "war-room:events";

/// Best-effort realtime publish; a Redis hiccup must never fail the write.
pub async fn publish_war_room_event(st: &AppState, room_id: Uuid, kind: &str, data: Value) {
    let payload = serde_json::json!({ "room_id": room_id, "kind": kind, "data": data });
    match st.redis_client().await {
        Ok(mut conn) => {
            let result: Result<i64, redis::RedisError> =
                conn.publish(WAR_ROOM_CHANNEL, payload.to_string()).await;
            if let Err(error) = result {
                tracing::warn!(room_id=%room_id, kind, error=%error, "war room publish failed");
            }
        }
        Err(error) => {
            tracing::warn!(room_id=%room_id, kind, error=%error, "war room redis unavailable");
        }
    }
}

/// Insert an activity row, then publish `activity.created` plus a
/// `room.changed` hint with the affected sections (`reasons` may be empty).
pub async fn record_and_publish(
    st: &AppState,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
    reasons: &[&str],
) -> Result<WarRoomEventRow, sqlx::Error> {
    let row = record_event(&st.pool, room, actor, event_type, payload).await?;
    publish_war_room_event(st, room.id, "activity.created", event_json(&row)).await;
    if !reasons.is_empty() {
        publish_war_room_event(
            st,
            room.id,
            "room.changed",
            serde_json::json!({ "reasons": reasons }),
        )
        .await;
    }
    Ok(row)
}

async fn record_event_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO war_room_events (id, workspace_id, project_id, war_room_id, actor_id, \
         event_type, payload, created_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, now())",
    )
    .bind(room.workspace_id)
    .bind(room.project_id)
    .bind(room.id)
    .bind(actor)
    .bind(event_type)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateWarRoom>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(issue) = fetch_issue_summary(&st.pool, project_id, body.primary_issue_id).await?
    else {
        return Ok(bad_request(
            "Invalid primary_issue_id - object does not exist.",
        ));
    };
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM war_rooms WHERE project_id = $1 AND primary_issue_id = $2 \
         AND status IN ('active','monitoring') AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .bind(body.primary_issue_id)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(existing_id) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "active_war_room_exists", "war_room_id": existing_id
            })),
        ));
    }
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| issue.name.clone());
    let severity = body
        .severity
        .clone()
        .unwrap_or_else(|| severity_from_priority(&issue.priority).to_string());
    if let Err(e) = validate_enum("severity", &severity, WAR_ROOM_SEVERITIES) {
        return Ok(bad_request(e));
    }
    let service_ids = body.service_ids.clone().unwrap_or_default();
    if !service_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM services WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(&service_ids)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != service_ids.len() {
            return Ok(bad_request("Invalid service_ids - object does not exist."));
        }
    }
    let assignees = issue_assignees(&st.pool, body.primary_issue_id).await?;
    let room_id = Uuid::new_v4();

    let mut tx = st.pool.begin().await?;
    sqlx::query("SELECT id FROM projects WHERE id = $1 FOR UPDATE")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM war_rooms WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;
    let workspace_id: Uuid = sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO war_rooms (id, workspace_id, project_id, sequence_id, name, description_html, \
         notes_html, severity, status, primary_issue_id, started_at, created_at, updated_at, \
         created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, '', $7, 'active', $8, now(), now(), now(), $9, $9)",
    )
    .bind(room_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(sequence)
    .bind(&name)
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&severity)
    .bind(body.primary_issue_id)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;

    let room = WarRoomRow {
        id: room_id,
        workspace_id,
        project_id,
        sequence_id: sequence,
        name: name.clone(),
        description_html: body.description_html.clone().unwrap_or_default(),
        notes_html: String::new(),
        severity: severity.clone(),
        status: "active".to_string(),
        primary_issue_id: body.primary_issue_id,
        started_at: chrono::Utc::now(),
        resolved_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        created_by_id: Some(auth.0),
        updated_by_id: Some(auth.0),
    };

    let mut initial_participants: Vec<(Uuid, &str)> = vec![(auth.0, "commander")];
    for assignee in assignees {
        if assignee != auth.0 {
            initial_participants.push((assignee, "responder"));
        }
    }
    for (member_id, role) in &initial_participants {
        sqlx::query(
            "INSERT INTO war_room_participants (id, workspace_id, project_id, war_room_id, \
             member_id, role, joined_at, created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now(), now(), $6, $6) \
             ON CONFLICT DO NOTHING",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(room_id)
        .bind(*member_id)
        .bind(*role)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    for service_id in &service_ids {
        sqlx::query(
            "INSERT INTO war_room_services (id, workspace_id, project_id, war_room_id, service_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5) \
             ON CONFLICT DO NOTHING",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(room_id)
        .bind(service_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    let mut sort_order = 0.0_f64;
    for (template_key, title) in runbook_template(issue.type_name.as_deref()) {
        sort_order += 65535.0;
        sqlx::query(
            "INSERT INTO war_room_runbook_items (id, workspace_id, project_id, war_room_id, title, \
             sort_order, is_done, template_key, created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, $6, now(), now(), $7, $7)",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(room_id)
        .bind(title)
        .bind(sort_order)
        .bind(template_key)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    record_event_tx(
        &mut tx,
        &room,
        auth.0,
        "room.created",
        serde_json::json!({
            "name": name, "severity": severity, "primary_issue_id": body.primary_issue_id
        }),
    )
    .await?;
    for (member_id, role) in &initial_participants {
        record_event_tx(
            &mut tx,
            &room,
            auth.0,
            "participant.joined",
            serde_json::json!({ "member_id": member_id, "role": role }),
        )
        .await?;
    }
    tx.commit().await?;
    publish_war_room_event(
        &st,
        room_id,
        "room.changed",
        serde_json::json!({ "reasons": ["created"] }),
    )
    .await;

    let row = fetch_room(&st.pool, project_id, room_id)
        .await?
        .expect("room just inserted");
    let detail = room_detail_json(&st.pool, &row).await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

#[derive(Debug, Deserialize)]
pub struct PatchWarRoom {
    pub name: Option<String>,
    pub severity: Option<String>,
    pub status: Option<String>,
    pub description_html: Option<String>,
    pub notes_html: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        ));
    };
    let detail = room_detail_json(&st.pool, &room).await?;
    Ok((StatusCode::OK, Json(detail)))
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchWarRoom>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(current) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        ));
    };
    if current.status == "archived" {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "room_archived"})),
        ));
    }
    let name = body.name.clone().unwrap_or_else(|| current.name.clone());
    if name.trim().is_empty() {
        return Ok(bad_request("Invalid name"));
    }
    let severity = body
        .severity
        .clone()
        .unwrap_or_else(|| current.severity.clone());
    if let Err(e) = validate_enum("severity", &severity, WAR_ROOM_SEVERITIES) {
        return Ok(bad_request(e));
    }
    let status = body
        .status
        .clone()
        .unwrap_or_else(|| current.status.clone());
    if let Err(e) = validate_enum("status", &status, WAR_ROOM_STATUSES) {
        return Ok(bad_request(e));
    }
    if status != current.status && !status_transition_allowed(&current.status, &status) {
        return Ok(bad_request("invalid_status_transition"));
    }
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    let notes_html = body
        .notes_html
        .clone()
        .unwrap_or_else(|| current.notes_html.clone());
    let resolved_at = match status.as_str() {
        "resolved" => current.resolved_at.or(Some(chrono::Utc::now())),
        "active" | "monitoring" => None,
        _ => current.resolved_at,
    };
    sqlx::query(
        "UPDATE war_rooms SET name = $1, severity = $2, status = $3, description_html = $4, \
         notes_html = $5, resolved_at = $6, updated_at = now(), updated_by_id = $7 WHERE id = $8",
    )
    .bind(&name)
    .bind(&severity)
    .bind(&status)
    .bind(&description_html)
    .bind(&notes_html)
    .bind(resolved_at)
    .bind(auth.0)
    .bind(pk)
    .execute(&st.pool)
    .await?;

    if severity != current.severity {
        record_and_publish(
            &st,
            &current,
            auth.0,
            "room.severity_changed",
            serde_json::json!({ "from": current.severity, "to": severity }),
            &["severity"],
        )
        .await?;
    }
    if status != current.status {
        record_and_publish(
            &st,
            &current,
            auth.0,
            "room.status_changed",
            serde_json::json!({ "from": current.status, "to": status }),
            &["status"],
        )
        .await?;
        let specific = match status.as_str() {
            "resolved" => Some("room.resolved"),
            "active" if current.status == "resolved" => Some("room.reopened"),
            "archived" => Some("room.archived"),
            _ => None,
        };
        if let Some(event_type) = specific {
            record_and_publish(&st, &current, auth.0, event_type, serde_json::json!({}), &[])
                .await?;
        }
    }
    let mut detail_reasons: Vec<&str> = Vec::new();
    if name != current.name || description_html != current.description_html {
        detail_reasons.push("details");
    }
    if notes_html != current.notes_html {
        detail_reasons.push("notes");
    }
    if !detail_reasons.is_empty() {
        publish_war_room_event(
            &st,
            current.id,
            "room.changed",
            serde_json::json!({ "reasons": detail_reasons }),
        )
        .await;
    }
    let row = fetch_room(&st.pool, project_id, pk)
        .await?
        .expect("room just updated");
    let detail = room_detail_json(&st.pool, &row).await?;
    Ok((StatusCode::OK, Json(detail)))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE war_room_messages SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_runbook_items SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_participants SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_services SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_issues SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_rooms SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    publish_war_room_event(
        &st,
        pk,
        "room.changed",
        serde_json::json!({ "reasons": ["deleted"] }),
    )
    .await;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[derive(Debug, Deserialize)]
pub struct LinkServices {
    pub service_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct LinkIssues {
    pub issue_ids: Vec<Uuid>,
}

async fn room_for_write(
    st: &AppState,
    slug: &str,
    project_id: Uuid,
    pk: Uuid,
    user: Uuid,
) -> Result<Result<WarRoomRow, (StatusCode, Json<Value>)>, common::errors::AppError> {
    if !gate_writer(&st.pool, user, slug, project_id).await? {
        return Ok(Err(deny()));
    }
    let Some(room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok(Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        )));
    };
    if room.status == "archived" {
        return Ok(Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "room_archived"})),
        )));
    }
    Ok(Ok(room))
}

pub async fn services_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<LinkServices>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    if !body.service_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM services WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(&body.service_ids)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != body.service_ids.len() {
            return Ok(bad_request("Invalid service_ids - object does not exist."));
        }
    }
    let mut linked = 0_u64;
    for service_id in &body.service_ids {
        let result = sqlx::query(
            "INSERT INTO war_room_services (id, workspace_id, project_id, war_room_id, service_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5) \
             ON CONFLICT DO NOTHING",
        )
        .bind(room.workspace_id)
        .bind(project_id)
        .bind(room.id)
        .bind(service_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
        linked += result.rows_affected();
    }
    if linked > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "service.linked",
            serde_json::json!({ "service_ids": body.service_ids }),
            &["links"],
        )
        .await?;
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "linked": linked })),
    ))
}

pub async fn services_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, service_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let result = sqlx::query(
        "UPDATE war_room_services SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND service_id = $3 AND deleted_at IS NULL",
    )
    .bind(room.id)
    .bind(project_id)
    .bind(service_id)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "service.unlinked",
            serde_json::json!({ "service_id": service_id }),
            &["links"],
        )
        .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

pub async fn issues_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<LinkIssues>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    if body.issue_ids.contains(&room.primary_issue_id) {
        return Ok(bad_request("primary_issue_not_linkable"));
    }
    if !body.issue_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issues WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(&body.issue_ids)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != body.issue_ids.len() {
            return Ok(bad_request("Invalid issue_ids - object does not exist."));
        }
    }
    let mut linked = 0_u64;
    for issue_id in &body.issue_ids {
        let result = sqlx::query(
            "INSERT INTO war_room_issues (id, workspace_id, project_id, war_room_id, issue_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5) \
             ON CONFLICT DO NOTHING",
        )
        .bind(room.workspace_id)
        .bind(project_id)
        .bind(room.id)
        .bind(issue_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
        linked += result.rows_affected();
    }
    if linked > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "issue.linked",
            serde_json::json!({ "issue_ids": body.issue_ids }),
            &["links"],
        )
        .await?;
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "linked": linked })),
    ))
}

pub async fn issues_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, issue_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let result = sqlx::query(
        "UPDATE war_room_issues SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(room.id)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "issue.unlinked",
            serde_json::json!({ "issue_id": issue_id }),
            &["links"],
        )
        .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[derive(Debug, Deserialize)]
pub struct ParticipantCreate {
    pub member_id: Uuid,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ParticipantPatch {
    pub role: String,
}

async fn participant_by_id(
    pool: &PgPool,
    room_id: Uuid,
    participant_id: Uuid,
) -> Result<Option<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.member_id, p.role, p.joined_at, u.display_name, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
           THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
         FROM war_room_participants p JOIN users u ON u.id = p.member_id \
         WHERE p.id = $1 AND p.war_room_id = $2 AND p.deleted_at IS NULL",
    )
    .bind(participant_id)
    .bind(room_id)
    .fetch_optional(pool)
    .await
}

pub async fn participants_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<ParticipantCreate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let role = body.role.clone().unwrap_or_else(|| "responder".to_string());
    if let Err(e) = validate_enum("role", &role, PARTICIPANT_ROLES) {
        return Ok(bad_request(e));
    }
    let project_role =
        fetch_project_member_role(&st.pool, body.member_id, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, body.member_id, &slug).await?;
    if project_role.is_none() && !ws_admin {
        return Ok(bad_request("Invalid member_id - not a project member."));
    }
    let mut tx = st.pool.begin().await?;
    if role == "commander" {
        sqlx::query(
            "UPDATE war_room_participants SET role = 'responder', updated_at = now(), updated_by_id = $1 \
             WHERE war_room_id = $2 AND role = 'commander' AND member_id != $3 AND deleted_at IS NULL",
        )
        .bind(auth.0)
        .bind(room.id)
        .bind(body.member_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO war_room_participants (id, workspace_id, project_id, war_room_id, member_id, \
         role, joined_at, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now(), now(), $6, $6) \
         ON CONFLICT (war_room_id, member_id) WHERE deleted_at IS NULL \
         DO UPDATE SET role = EXCLUDED.role, updated_at = now(), updated_by_id = $6",
    )
    .bind(room.workspace_id)
    .bind(project_id)
    .bind(room.id)
    .bind(body.member_id)
    .bind(&role)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    record_and_publish(
        &st,
        &room,
        auth.0,
        "participant.joined",
        serde_json::json!({ "member_id": body.member_id, "role": role }),
        &["participants"],
    )
    .await?;
    let row = sqlx::query_as::<_, ParticipantRow>(
        "SELECT p.id, p.member_id, p.role, p.joined_at, u.display_name, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
           THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
         FROM war_room_participants p JOIN users u ON u.id = p.member_id \
         WHERE p.war_room_id = $1 AND p.member_id = $2 AND p.deleted_at IS NULL",
    )
    .bind(room.id)
    .bind(body.member_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(participant_json(&row))))
}

pub async fn participants_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, participant_id)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(body): Json<ParticipantPatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    if let Err(e) = validate_enum("role", &body.role, PARTICIPANT_ROLES) {
        return Ok(bad_request(e));
    }
    let current = participant_by_id(&st.pool, room.id, participant_id).await?;
    let Some(current) = current else {
        return Ok(missing());
    };
    let mut tx = st.pool.begin().await?;
    if body.role == "commander" {
        sqlx::query(
            "UPDATE war_room_participants SET role = 'responder', updated_at = now(), updated_by_id = $1 \
             WHERE war_room_id = $2 AND role = 'commander' AND id != $3 AND deleted_at IS NULL",
        )
        .bind(auth.0)
        .bind(room.id)
        .bind(participant_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE war_room_participants SET role = $1, updated_at = now(), updated_by_id = $2 \
         WHERE id = $3 AND war_room_id = $4 AND deleted_at IS NULL",
    )
    .bind(&body.role)
    .bind(auth.0)
    .bind(participant_id)
    .bind(room.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    record_and_publish(
        &st,
        &room,
        auth.0,
        "participant.role_changed",
        serde_json::json!({
            "member_id": current.member_id, "from": current.role, "to": body.role
        }),
        &["participants"],
    )
    .await?;
    let row = participant_by_id(&st.pool, room.id, participant_id)
        .await?
        .expect("participant just updated");
    Ok((StatusCode::OK, Json(participant_json(&row))))
}

pub async fn participants_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, participant_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let current = participant_by_id(&st.pool, room.id, participant_id).await?;
    let Some(current) = current else {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    };
    sqlx::query(
        "UPDATE war_room_participants SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(participant_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    record_and_publish(
        &st,
        &room,
        auth.0,
        "participant.left",
        serde_json::json!({ "member_id": current.member_id }),
        &["participants"],
    )
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[derive(Debug, Deserialize)]
pub struct RunbookCreate {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct RunbookPatch {
    pub title: Option<String>,
    pub is_done: Option<bool>,
}

async fn runbook_item_by_id(
    pool: &PgPool,
    room_id: Uuid,
    item_id: Uuid,
) -> Result<Option<RunbookItemRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, sort_order, is_done, done_by_id, done_at, template_key \
         FROM war_room_runbook_items WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(item_id)
    .bind(room_id)
    .fetch_optional(pool)
    .await
}

pub async fn runbook_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<RunbookCreate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let title = body.title.trim();
    if title.is_empty() {
        return Ok(bad_request("Invalid title"));
    }
    let max_order: f64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), 0) FROM war_room_runbook_items \
         WHERE war_room_id = $1 AND deleted_at IS NULL",
    )
    .bind(room.id)
    .fetch_one(&st.pool)
    .await?;
    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO war_room_runbook_items (id, workspace_id, project_id, war_room_id, title, \
         sort_order, is_done, template_key, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, false, NULL, now(), now(), $7, $7)",
    )
    .bind(item_id)
    .bind(room.workspace_id)
    .bind(project_id)
    .bind(room.id)
    .bind(title)
    .bind(max_order + 65535.0)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    publish_war_room_event(
        &st,
        room.id,
        "room.changed",
        serde_json::json!({ "reasons": ["runbook"] }),
    )
    .await;
    let row = runbook_item_by_id(&st.pool, room.id, item_id)
        .await?
        .expect("runbook item just inserted");
    Ok((StatusCode::CREATED, Json(runbook_item_json(&row))))
}

pub async fn runbook_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, item_id)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(body): Json<RunbookPatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let Some(current) = runbook_item_by_id(&st.pool, room.id, item_id).await? else {
        return Ok(missing());
    };
    let title = match body.title.clone() {
        Some(t) if t.trim().is_empty() => return Ok(bad_request("Invalid title")),
        Some(t) => t,
        None => current.title.clone(),
    };
    let is_done = body.is_done.unwrap_or(current.is_done);
    let (done_by_id, done_at) = if is_done {
        (Some(auth.0), Some(chrono::Utc::now()))
    } else {
        (None, None)
    };
    sqlx::query(
        "UPDATE war_room_runbook_items SET title = $1, is_done = $2, done_by_id = $3, \
         done_at = $4, updated_at = now(), updated_by_id = $5 WHERE id = $6 AND war_room_id = $7",
    )
    .bind(&title)
    .bind(is_done)
    .bind(done_by_id)
    .bind(done_at)
    .bind(auth.0)
    .bind(item_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    if is_done != current.is_done {
        let event_type = if is_done {
            "runbook.item_done"
        } else {
            "runbook.item_reopened"
        };
        record_and_publish(
            &st,
            &room,
            auth.0,
            event_type,
            serde_json::json!({ "item_id": item_id, "title": title }),
            &["runbook"],
        )
        .await?;
    }
    let row = runbook_item_by_id(&st.pool, room.id, item_id)
        .await?
        .expect("runbook item just updated");
    Ok((StatusCode::OK, Json(runbook_item_json(&row))))
}

pub async fn runbook_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, item_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    sqlx::query(
        "UPDATE war_room_runbook_items SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(item_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    publish_war_room_event(
        &st,
        room.id,
        "room.changed",
        serde_json::json!({ "reasons": ["runbook"] }),
    )
    .await;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[derive(Debug, Deserialize)]
pub struct EventsParams {
    pub before_id: Option<Uuid>,
    pub limit: Option<i64>,
}

pub async fn events_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Query(params): Query<EventsParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(_room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        ));
    };
    let cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)> = match params.before_id {
        Some(before_id) => {
            sqlx::query_as(
                "SELECT created_at, id FROM war_room_events WHERE id = $1 AND war_room_id = $2",
            )
            .bind(before_id)
            .bind(pk)
            .fetch_optional(&st.pool)
            .await?
        }
        None => None,
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let rows: Vec<WarRoomEventRow> = sqlx::query_as(
        "SELECT e.id, e.actor_id, e.event_type, e.payload, e.created_at FROM war_room_events e \
         WHERE e.war_room_id = $1 \
         AND ($2::timestamptz IS NULL OR (e.created_at, e.id) < ($2::timestamptz, $3::uuid)) \
         ORDER BY e.created_at DESC, e.id DESC LIMIT $4",
    )
    .bind(pk)
    .bind(cursor.as_ref().map(|c| c.0))
    .bind(cursor.as_ref().map(|c| c.1))
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(event_json).collect())),
    ))
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarRoomMessageRow {
    pub id: Uuid,
    pub war_room_id: Uuid,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub mentions: Value,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

const MESSAGE_SELECT: &str = "SELECT m.id, m.war_room_id, m.author_id, m.body, m.mentions, \
    m.edited_at, m.created_at, u.display_name, \
    CASE WHEN u.avatar_asset_id IS NOT NULL \
      THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
    FROM war_room_messages m LEFT JOIN users u ON u.id = m.author_id";

pub fn message_json(r: &WarRoomMessageRow) -> Value {
    serde_json::json!({
        "id": r.id,
        "war_room_id": r.war_room_id,
        "author_id": r.author_id,
        "author": r.author_id.map(|id| serde_json::json!({
            "id": id, "display_name": r.display_name, "avatar_url": r.avatar_url,
        })),
        "body": r.body,
        "mentions": r.mentions,
        "edited_at": r.edited_at,
        "created_at": r.created_at,
    })
}

async fn fetch_message(
    pool: &PgPool,
    room_id: Uuid,
    message_id: Uuid,
) -> Result<Option<WarRoomMessageRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{MESSAGE_SELECT} WHERE m.id = $1 AND m.war_room_id = $2 AND m.deleted_at IS NULL"
    ))
    .bind(message_id)
    .bind(room_id)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Deserialize)]
pub struct MessagesParams {
    pub before_id: Option<Uuid>,
    pub limit: Option<i64>,
}

/// Chat pages are returned oldest → newest inside the page (client renders
/// directly); `before_id` walks backwards with the first id of the page.
pub async fn messages_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Query(params): Query<MessagesParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(_room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        ));
    };
    let cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)> = match params.before_id {
        Some(before_id) => {
            sqlx::query_as(
                "SELECT created_at, id FROM war_room_messages \
                 WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
            )
            .bind(before_id)
            .bind(pk)
            .fetch_optional(&st.pool)
            .await?
        }
        None => None,
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let mut rows: Vec<WarRoomMessageRow> = sqlx::query_as(&format!(
        "{MESSAGE_SELECT} WHERE m.war_room_id = $1 \
         AND ($2::timestamptz IS NULL OR (m.created_at, m.id) < ($2::timestamptz, $3::uuid)) \
         AND m.deleted_at IS NULL \
         ORDER BY m.created_at DESC, m.id DESC LIMIT $4"
    ))
    .bind(pk)
    .bind(cursor.as_ref().map(|c| c.0))
    .bind(cursor.as_ref().map(|c| c.1))
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    rows.reverse();
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(message_json).collect())),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_mapping_follows_priority() {
        assert_eq!(severity_from_priority("urgent"), "sev1");
        assert_eq!(severity_from_priority("high"), "sev2");
        assert_eq!(severity_from_priority("medium"), "sev3");
        assert_eq!(severity_from_priority("low"), "sev4");
        assert_eq!(severity_from_priority("none"), "sev4");
        assert_eq!(severity_from_priority("bogus"), "sev4");
    }

    #[test]
    fn transition_map_is_terminal_on_archived() {
        assert!(status_transition_allowed("active", "monitoring"));
        assert!(status_transition_allowed("monitoring", "active"));
        assert!(status_transition_allowed("active", "resolved"));
        assert!(status_transition_allowed("monitoring", "resolved"));
        assert!(status_transition_allowed("resolved", "active"));
        assert!(status_transition_allowed("resolved", "archived"));
        assert!(status_transition_allowed("active", "archived"));
        assert!(!status_transition_allowed("monitoring", "monitoring"));
        assert!(!status_transition_allowed("archived", "active"));
        assert!(!status_transition_allowed("archived", "resolved"));
        assert!(!status_transition_allowed("resolved", "monitoring"));
    }

    #[test]
    fn runbook_template_matches_type_name_case_insensitively() {
        let incident = runbook_template(Some("Incident"));
        assert_eq!(incident.len(), 5);
        assert_eq!(incident[0].0, "triage");
        assert_eq!(incident[4].0, "postmortem");
        assert_eq!(runbook_template(Some(" problem ")).len(), 5);
        assert_eq!(runbook_template(Some("Change")).len(), 5);
        assert_eq!(runbook_template(Some("Request")).len(), 4);
        assert!(runbook_template(Some("Custom type")).is_empty());
        assert!(runbook_template(None).is_empty());
    }

    #[test]
    fn active_status_check() {
        assert!(is_active_status("active"));
        assert!(is_active_status("monitoring"));
        assert!(!is_active_status("resolved"));
        assert!(!is_active_status("archived"));
    }

    #[test]
    fn mentions_parse_valid_uuids_only_once() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let body = format!(
            "hey @{{{a}}} and @{{{b}}} and @{{{a}}} bad @{{nope}} tail @{{"
        );
        assert_eq!(parse_mentions(&body), vec![a, b]);
    }

    #[test]
    fn mentions_parse_empty_when_none() {
        assert!(parse_mentions("no mentions here").is_empty());
        assert!(parse_mentions("").is_empty());
    }
}
