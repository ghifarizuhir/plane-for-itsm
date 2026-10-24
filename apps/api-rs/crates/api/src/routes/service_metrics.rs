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
