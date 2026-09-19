use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::issue_common::{parse_cursor, parse_per_page, DetailCursor, PageWindow, page_window};
use crate::routes::issue_query::build_ungrouped_envelope;

/// `?cursor=&per_page=` params shared by v1 list endpoints.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PageParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
}

impl PageParams {
    /// Byte-exact DRF parse: per_page default/max 1000, cursor `value:page:is_prev`.
    pub(crate) fn resolve(&self) -> Result<(i64, DetailCursor), String> {
        let per_page = parse_per_page(self.per_page.as_deref())?;
        let raw = self.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
        let cursor = parse_cursor(&raw)?;
        Ok((per_page, cursor))
    }
}

/// Offset window for a page (wrapper so handlers don't import `issue_common`
/// directly). `BeyondEnd` renders an empty page. Takes `page` by value because
/// `DetailCursor` is not `Clone`/`Copy` and the caller still needs it afterwards.
pub(crate) fn window_for(page: i128, limit: i64) -> Result<PageWindow, ()> {
    page_window(page, limit)
}

pub(crate) fn page_rows(rows: Vec<Value>, per_page: Option<&str>, cursor: Option<&str>) -> Result<Value, String> {
    let params = PageParams { cursor: cursor.map(str::to_string), per_page: per_page.map(str::to_string) };
    let (limit, cur) = params.resolve()?;
    if limit <= 0 { return Err("Invalid per_page value. Cannot exceed 1000.".to_string()); }
    let page = cur.page.max(0);
    let total = rows.len() as i64;
    let offset = page.saturating_mul(i128::from(limit));
    let slice: Vec<Value> = if offset >= i128::from(total) { Vec::new() } else { let start = offset as usize; let end = (start + limit as usize).min(rows.len()); rows[start..end].to_vec() };
    Ok(build_ungrouped_envelope(total, limit, page, slice))
}

pub(crate) fn bad_request(msg: String) -> (StatusCode, serde_json::Value) { (StatusCode::BAD_REQUEST, json!({"detail": msg})) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_params_default_matches_django() {
        let (per_page, cursor) = PageParams::default().resolve().unwrap();
        assert_eq!(per_page, 1000);
        assert_eq!(cursor.page, 0);
        assert!(!cursor.is_prev);
    }

    #[test]
    fn page_rows_wraps_array_in_twelve_key_envelope() {
        let rows = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 3}),
        ];
        let out = page_rows(rows, None, None).unwrap();
        assert_eq!(out["total_count"], 3);
        assert_eq!(out["count"], 3);
        assert_eq!(out["total_pages"], 1);
        assert_eq!(out["results"].as_array().unwrap().len(), 3);
        assert_eq!(out["next_cursor"], "1000:1:0");
        assert_eq!(out["prev_cursor"], "1000:-1:1");
        assert!(out["grouped_by"].is_null());
        assert!(out["extra_stats"].is_null());
    }

    #[test]
    fn page_rows_slices_forward_pages() {
        let rows: Vec<serde_json::Value> = (0..5).map(|i| serde_json::json!({"id": i})).collect();
        let out = page_rows(rows, Some("2"), Some("2:1:0")).unwrap();
        assert_eq!(out["count"], 2);
        assert_eq!(out["total_count"], 5);
        assert_eq!(out["total_pages"], 3);
        assert_eq!(out["next_page_results"], true);
        assert_eq!(out["results"][0]["id"], 2);
        assert_eq!(out["results"][1]["id"], 3);
    }

    #[test]
    fn page_rows_beyond_end_is_empty_not_panic() {
        let rows = vec![serde_json::json!({"id": 1})];
        let out = page_rows(rows, Some("2"), Some("2:9:0")).unwrap();
        assert_eq!(out["count"], 0);
        assert_eq!(out["next_page_results"], false);
    }

    #[test]
    fn page_rows_rejects_zero_per_page() {
        assert!(page_rows(vec![], Some("0"), None).is_err());
    }
}
