//! Endpoint ingest publik untuk webhook Alertmanager.
//! Spec: `docs/superpowers/specs/2026-10-04-intake-webhook-source-design.md`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    routes::intake::{accept_intake_issue, PRIORITIES},
    state::AppState,
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Source {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub type_id: Option<Uuid>,
    pub is_active: bool,
    pub auto_accept: bool,
    pub config: Value,
    pub created_by_id: Option<Uuid>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Series {
    pub issue_id: Uuid,
    pub row_id: Uuid,
    pub status: i32,
    pub state_group: Option<String>,
}

#[derive(Default)]
struct Counts {
    created: u64,
    updated: u64,
    reopened: u64,
    accepted: u64,
    declined: u64,
    resolved: u64,
    ignored: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Created,
    Updated,
    Reopened,
    Declined,
    Resolved,
    Ignored,
}

impl Counts {
    fn bump(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Created => self.created += 1,
            Outcome::Updated => self.updated += 1,
            Outcome::Reopened => self.reopened += 1,
            Outcome::Declined => self.declined += 1,
            Outcome::Resolved => self.resolved += 1,
            Outcome::Ignored => self.ignored += 1,
        }
    }

    fn bump_accepted(&mut self) {
        self.accepted += 1;
    }
}

pub async fn alertmanager(
    State(st): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let source = sqlx::query_as::<_, Source>(
        "SELECT s.id, s.project_id, s.name, s.type_id, s.is_active, s.auto_accept, s.config, s.created_by_id \
         FROM intake_sources s \
         JOIN projects p ON p.id = s.project_id AND p.deleted_at IS NULL \
         WHERE s.token = $1 AND s.deleted_at IS NULL",
    )
    .bind(&token)
    .fetch_optional(&st.pool)
    .await?;
    let Some(source) = source else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Unknown token"})),
        ));
    };
    if !source.is_active {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Intake source is inactive"})),
        ));
    }
    let Some(alerts) = body.get("alerts").and_then(Value::as_array) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "alerts must be an array"})),
        ));
    };
    let common_labels = body.get("commonLabels").cloned().unwrap_or_else(|| json!({}));
    let common_annotations = body
        .get("commonAnnotations")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let mut counts = Counts::default();
    for alert in alerts {
        let status = alert
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("firing");
        let resolved = status == "resolved";
        let fingerprint = fingerprint_of(alert);
        let existing = find_series(&st.pool, source.id, &fingerprint).await?;
        let Some(actor) = source.created_by_id else {
            return Err(common::errors::AppError(anyhow::anyhow!(
                "intake source without created_by_id"
            )));
        };

        let outcome = if resolved {
            match &existing {
                Some(series) => resolve_series(&st, &source, series, actor).await?,
                None => Outcome::Ignored,
            }
        } else {
            match &existing {
                Some(series) => fire_series(&st, &source, series, actor).await?,
                None => {
                    create_series(&st, &source, alert, &common_labels, &common_annotations).await?
                }
            }
        };

        // Auto-accept: hanya untuk firing yang berakhir pending.
        let mut counted = false;
        if !resolved
            && source.auto_accept
            && matches!(
                outcome,
                Outcome::Created | Outcome::Updated | Outcome::Reopened
            )
        {
            let pending = match &existing {
                Some(series) => series.status == -1 || series.status == -2,
                None => true,
            };
            if pending {
                let (row_id, issue_id) = match &existing {
                    Some(series) => (series.row_id, series.issue_id),
                    None => sqlx::query_as::<_, (Uuid, Uuid)>(
                        "SELECT ii.id, ii.issue_id FROM issues i \
                         JOIN intake_issues ii ON ii.issue_id = i.id \
                         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = $2 \
                         AND i.deleted_at IS NULL",
                    )
                    .bind(source.id)
                    .bind(&fingerprint)
                    .fetch_one(&st.pool)
                    .await?,
                };
                match accept_intake_issue(
                    &st.pool,
                    source.project_id,
                    row_id,
                    issue_id,
                    Some(actor),
                )
                .await?
                {
                    Ok(()) => {
                        counts.bump_accepted();
                        counted = true;
                    }
                    Err(message) => tracing::warn!(
                        source_id = %source.id,
                        fingerprint = %fingerprint,
                        message,
                        "auto-accept skipped"
                    ),
                }
            }
        }
        if !counted {
            counts.bump(outcome);
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "created": counts.created,
            "updated": counts.updated,
            "reopened": counts.reopened,
            "accepted": counts.accepted,
            "declined": counts.declined,
            "resolved": counts.resolved,
            "ignored": counts.ignored,
        })),
    ))
}

/// Baca label dari label alert, fallback ke `commonLabels`.
fn label_value<'a>(
    alert_labels: &'a Value,
    common_labels: &'a Value,
    key: &str,
) -> Option<&'a str> {
    alert_labels
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| common_labels.get(key).and_then(Value::as_str))
}

fn annotation(alert: &Value, common: &Value, key: &str) -> Option<String> {
    alert
        .get("annotations")
        .and_then(|a| a.get(key))
        .and_then(Value::as_str)
        .or_else(|| common.get(key).and_then(Value::as_str))
        .map(str::to_string)
}

fn map_service(config: &Value, alert_labels: &Value, common_labels: &Value) -> Option<Uuid> {
    let key = config
        .get("service_label_key")
        .and_then(Value::as_str)
        .unwrap_or("service");
    if let Some(label) = label_value(alert_labels, common_labels, key) {
        if let Some(id) = config
            .get("service_map")
            .and_then(|m| m.get(label))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            return Some(id);
        }
    }
    config
        .get("fallback_service_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn map_priority(config: &Value, alert_labels: &Value, common_labels: &Value) -> String {
    let key = config
        .get("severity_label_key")
        .and_then(Value::as_str)
        .unwrap_or("severity");
    let mapped = label_value(alert_labels, common_labels, key)
        .and_then(|severity| config.get("severity_map").and_then(|m| m.get(severity)))
        .and_then(Value::as_str)
        .filter(|p| PRIORITIES.contains(p))
        .map(str::to_string);
    mapped
        .or_else(|| {
            config
                .get("default_priority")
                .and_then(Value::as_str)
                .filter(|p| PRIORITIES.contains(p))
                .map(str::to_string)
        })
        .unwrap_or_else(|| "none".to_string())
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fingerprint_of(alert: &Value) -> String {
    if let Some(fp) = alert
        .get("fingerprint")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return fp.to_string();
    }
    let mut labels: Vec<(String, String)> = alert
        .get("labels")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    labels.sort();
    let joined = labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(joined.as_bytes()))
}

async fn resolve_or_create_triage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project_id: Uuid,
    workspace_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'triage' AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await?;
    match existing {
        Some(id) => Ok(id),
        None => sqlx::query_scalar(
            "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \"default\", project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Triage', '', 'triage', 'triage', '#4E5355', 65000, false, false, $1, $2, now(), now()) RETURNING id",
        )
        .bind(project_id)
        .bind(workspace_id)
        .fetch_one(&mut **tx)
        .await,
    }
}

async fn resolve_or_create_intake(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM intakes WHERE project_id = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await?;
    match existing {
        Some(id) => Ok(id),
        None => sqlx::query_scalar(
            "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Intake', '', true, '{}'::jsonb, '{}'::jsonb, $1, $2, $3, $3, now(), now()) RETURNING id",
        )
        .bind(project_id)
        .bind(workspace_id)
        .bind(actor)
        .fetch_one(&mut **tx)
        .await,
    }
}

async fn find_series(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    fingerprint: &str,
) -> Result<Option<Series>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id AS issue_id, ii.id AS row_id, ii.status, st.\"group\" AS state_group \
         FROM issues i \
         JOIN intake_issues ii ON ii.issue_id = i.id AND ii.deleted_at IS NULL \
         LEFT JOIN states st ON st.id = i.state_id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = $2 \
         AND i.deleted_at IS NULL LIMIT 1",
    )
    .bind(source_id)
    .bind(fingerprint)
    .fetch_optional(pool)
    .await
}

async fn system_comment(
    st: &AppState,
    issue_id: Uuid,
    project_id: Uuid,
    actor: Uuid,
    html: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_comments (id, comment_html, comment_json, comment_stripped, access, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, '{}', '', 'INTERNAL', '{}', $2, $3, i.workspace_id, $4, $4, now(), now() \
         FROM issues i WHERE i.id = $2",
    )
    .bind(html)
    .bind(issue_id)
    .bind(project_id)
    .bind(actor)
    .execute(&st.pool)
    .await?;
    Ok(())
}

async fn first_completed_state(
    pool: &sqlx::PgPool,
    project_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" = 'completed' ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

async fn resolve_series(
    st: &AppState,
    source: &Source,
    series: &Series,
    actor: Uuid,
) -> Result<Outcome, common::errors::AppError> {
    match series.status {
        -2 => {
            sqlx::query(
                "UPDATE intake_issues SET status = -1, updated_at = now() \
                 WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(series.row_id)
            .execute(&st.pool)
            .await?;
            Ok(Outcome::Declined)
        }
        1 => {
            if matches!(
                series.state_group.as_deref(),
                Some("completed") | Some("cancelled")
            ) {
                return Ok(Outcome::Ignored);
            }
            let Some(target) = first_completed_state(&st.pool, source.project_id).await? else {
                tracing::warn!(
                    issue_id = %series.issue_id,
                    "alert resolved but project has no completed state"
                );
                return Ok(Outcome::Ignored);
            };
            sqlx::query(
                "UPDATE issues SET state_id = $2, completed_at = now(), updated_at = now() WHERE id = $1",
            )
            .bind(series.issue_id)
            .bind(target)
            .execute(&st.pool)
            .await?;
            system_comment(
                st,
                series.issue_id,
                source.project_id,
                actor,
                "<p>Auto-resolved by Alertmanager</p>",
            )
            .await?;
            Ok(Outcome::Resolved)
        }
        // snoozed (0), declined (-1), duplicate (2)
        _ => Ok(Outcome::Ignored),
    }
}

async fn fire_series(
    st: &AppState,
    source: &Source,
    series: &Series,
    actor: Uuid,
) -> Result<Outcome, common::errors::AppError> {
    sqlx::query(
        "UPDATE issues SET intake_occurrence_count = intake_occurrence_count + 1, \
         intake_last_seen_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(series.issue_id)
    .execute(&st.pool)
    .await?;

    if series.status == -2 || series.status == 0 || series.status == 2 {
        return Ok(Outcome::Updated);
    }
    if series.status == -1 {
        sqlx::query(
            "UPDATE intake_issues SET status = -2, updated_at = now() \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(series.row_id)
        .execute(&st.pool)
        .await?;
        return Ok(Outcome::Reopened);
    }
    // status == 1: hanya reopen bila issue sudah selesai/dibatalkan.
    let closed = matches!(
        series.state_group.as_deref(),
        Some("completed") | Some("cancelled")
    );
    if !closed {
        return Ok(Outcome::Updated);
    }
    let Some(target) =
        super::issue_common::resolve_issue_state(&st.pool, source.project_id, None).await?
    else {
        tracing::warn!(issue_id = %series.issue_id, "alert refire: no default state to reopen");
        return Ok(Outcome::Updated);
    };
    sqlx::query(
        "UPDATE issues SET state_id = $2, completed_at = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(series.issue_id)
    .bind(target)
    .execute(&st.pool)
    .await?;
    system_comment(
        st,
        series.issue_id,
        source.project_id,
        actor,
        "<p>Reopened by alert refire</p>",
    )
    .await?;
    Ok(Outcome::Reopened)
}

#[allow(clippy::too_many_arguments)]
async fn create_series(
    st: &AppState,
    source: &Source,
    alert: &Value,
    common_labels: &Value,
    common_annotations: &Value,
) -> Result<Outcome, common::errors::AppError> {
    let Some(actor) = source.created_by_id else {
        return Err(common::errors::AppError(anyhow::anyhow!(
            "intake source without created_by_id"
        )));
    };
    let labels = alert.get("labels").cloned().unwrap_or_else(|| json!({}));
    let fingerprint = fingerprint_of(alert);
    let (workspace_id, workspace_slug): (Uuid, String) = sqlx::query_as(
        "SELECT w.id, w.slug FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND p.deleted_at IS NULL",
    )
    .bind(source.project_id)
    .fetch_one(&st.pool)
    .await?;

    let title = annotation(alert, common_annotations, "summary")
        .or_else(|| label_value(&labels, common_labels, "alertname").map(str::to_string))
        .unwrap_or_else(|| "Alert".to_string());
    let mut description = String::new();
    if let Some(text) = annotation(alert, common_annotations, "description") {
        description.push_str(&format!("<p>{}</p>", escape_html(&text)));
    }
    if let Some(url) = alert.get("generatorURL").and_then(Value::as_str) {
        description.push_str(&format!(
            "<p><a href=\"{}\">{}</a></p>",
            escape_html(url),
            escape_html(url)
        ));
    }
    if description.is_empty() {
        description.push_str("<p></p>");
    }
    let priority = map_priority(&source.config, &labels, common_labels);
    let service_id = map_service(&source.config, &labels, common_labels);

    let mut tx = st.pool.begin().await?;
    let triage_id = resolve_or_create_triage(&mut tx, source.project_id, workspace_id).await?;
    let intake_id = resolve_or_create_intake(&mut tx, source.project_id, workspace_id, actor).await?;
    let issue = super::issue_write::insert_issue(
        &mut tx,
        super::issue_write::NewIssue {
            slug: &workspace_slug,
            project_id: source.project_id,
            state_id: Some(triage_id),
            name: &title,
            description_html: &description,
            priority: &priority,
            start_date: None,
            target_date: None,
            parent_id: None,
            type_id: source.type_id,
            estimate_point_id: None,
            created_by: actor,
        },
    )
    .await?;
    super::issue_version_write::record_description_version(
        &mut tx,
        issue.id,
        source.project_id,
        workspace_id,
        actor,
        Some(actor),
        None,
        &description,
        &json!({}),
    )
    .await?;
    sqlx::query(
        "UPDATE issues SET intake_source_id = $1, intake_fingerprint = $2, \
         intake_occurrence_count = 1, intake_last_seen_at = now() WHERE id = $3",
    )
    .bind(source.id)
    .bind(&fingerprint)
    .bind(issue.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO intake_issues (id, intake_id, issue_id, status, extra, source, project_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, -2, '{}'::jsonb, 'WEBHOOK', $3, $4, now(), now())",
    )
    .bind(intake_id)
    .bind(issue.id)
    .bind(source.project_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;
    if let Some(service_id) = service_id {
        sqlx::query(
            "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
        )
        .bind(workspace_id)
        .bind(source.project_id)
        .bind(service_id)
        .bind(issue.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Outcome::Created)
}
