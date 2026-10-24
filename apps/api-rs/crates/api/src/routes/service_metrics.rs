//! External service metrics ingest + read API.
//! Spec: docs/superpowers/specs/2026-10-24-service-metrics-ingest-design.md

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
    routes::{
        issue_common::is_workspace_admin,
        project::{deny, missing},
        service_access,
    },
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

// ---------------------------------------------------------------------------
// Ingest
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Source management (workspace admin)
// ---------------------------------------------------------------------------

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
