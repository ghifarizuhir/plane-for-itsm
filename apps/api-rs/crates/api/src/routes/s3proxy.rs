/// Bucket yang dilayani proxy. Default HARUS "uploads" mengikuti
/// Django `plane/settings/common.py:307`, bukan "" seperti `s3_conf()`.
pub fn configured_bucket() -> String {
    std::env::var("AWS_S3_BUCKET_NAME").unwrap_or_else(|_| "uploads".to_string())
}

/// Host MinIO INTERNAL (antar-container), bukan request-Host.
/// Presign memakai request-Host untuk URL publik; proxy memakai ini untuk upstream.
pub fn minio_base() -> String {
    std::env::var("AWS_S3_ENDPOINT_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("MINIO_ENDPOINT_URL")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "http://plane-minio:9000".to_string())
}

/// Bangun URL upstream MinIO. `None` = bukan tanggung jawab proxy
/// (panggil fallback 404). Menolak `..` anti path-traversal.
pub fn proxy_target(bucket: &str, rest: &str, raw_query: Option<&str>) -> Option<String> {
    if bucket != configured_bucket() {
        return None;
    }
    if rest.split('/').any(|seg| seg == "..") {
        return None;
    }
    let mut url = format!(
        "{}/{}/{}",
        minio_base().trim_end_matches('/'),
        bucket,
        rest.trim_start_matches('/')
    );
    if let Some(q) = raw_query {
        if !q.is_empty() {
            url.push('?');
            url.push_str(q);
        }
    }
    Some(url)
}

use axum::{
    body::{Body, Bytes},
    extract::{Path, RawQuery},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("s3proxy client")
    })
}

/// Forward mentah ke MinIO. Tanpa auth gate: otorisasi dibawa signature
/// SigV4 di query/form (persis seperti Caddy `Caddyfile.ce:20-21` yang juga
/// tidak meng-gate). Body sudah dibatasi 5MB oleh RequestBodyLimitLayer.
pub async fn proxy_to_minio(
    Path((bucket, rest)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let Some(url) = proxy_target(&bucket, &rest, query.as_deref()) else {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"error": "Page not found."})),
        )
            .into_response();
    };
    let mut req = http().request(method, url).body(body);
    if let Some(ct) = headers.get(axum::http::header::CONTENT_TYPE).cloned() {
        req = req.header(axum::http::header::CONTENT_TYPE, ct);
    }
    let upstream = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "s3proxy upstream unreachable");
            return (
                StatusCode::BAD_GATEWAY,
                axum::Json(json!({"error": "Object storage unreachable."})),
            )
                .into_response();
        }
    };
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut resp = Response::builder().status(status);
    for h in [
        axum::http::header::CONTENT_TYPE,
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::header::ETAG,
    ] {
        if let Some(v) = upstream.headers().get(h.clone()).cloned() {
            resp = resp.header(h, v);
        }
    }
    let bytes = match upstream.bytes().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "s3proxy upstream body failed");
            return (
                StatusCode::BAD_GATEWAY,
                axum::Json(json!({"error": "Object storage unreachable."})),
            )
                .into_response();
        }
    };
    resp.body(Body::from(bytes)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scoped process-env snapshot: restores every touched var on drop
    /// (including assertion panics), so the mutation never leaks into
    /// parallel unit tests. Same pattern as `EnvGuard` in `asset.rs` tests
    /// (kept local — that one lives inside asset's test module and is not
    /// importable here).
    struct EnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvGuard {
        fn snapshot(keys: &[&'static str]) -> Self {
            Self {
                saved: keys.iter().map(|k| (*k, std::env::var_os(k))).collect(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in self.saved.drain(..) {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    /// Pin the S3 env to known-good defaults so tests are hermetic even when
    /// the outer environment sets e.g. `AWS_S3_ENDPOINT_URL`
    /// (`docker-compose-test.yml` points it at `http://test-minio:9000`).
    /// The returned guard restores the previous env on drop.
    fn pin_default_env() -> EnvGuard {
        let guard = EnvGuard::snapshot(&[
            "AWS_S3_BUCKET_NAME",
            "AWS_S3_ENDPOINT_URL",
            "MINIO_ENDPOINT_URL",
        ]);
        std::env::set_var("AWS_S3_BUCKET_NAME", "uploads");
        std::env::set_var("AWS_S3_ENDPOINT_URL", "http://plane-minio:9000");
        std::env::remove_var("MINIO_ENDPOINT_URL");
        guard
    }

    #[test]
    fn target_keeps_bucket_root_for_presigned_post() {
        let _env = pin_default_env();
        assert_eq!(
            proxy_target("uploads", "", None).as_deref(),
            Some("http://plane-minio:9000/uploads/")
        );
    }

    #[test]
    fn target_preserves_key_and_query_verbatim() {
        let _env = pin_default_env();
        assert_eq!(
            proxy_target("uploads", "abc/file.png", Some("X-Amz-Algorithm=AWS4-HMAC-SHA256")).as_deref(),
            Some("http://plane-minio:9000/uploads/abc/file.png?X-Amz-Algorithm=AWS4-HMAC-SHA256")
        );
    }

    #[test]
    fn rejects_wrong_bucket_and_traversal() {
        let _env = pin_default_env();
        assert!(proxy_target("other", "", None).is_none());
        assert!(proxy_target("uploads", "../etc/passwd", None).is_none());
    }

    #[test]
    fn empty_endpoint_env_falls_through_to_default() {
        let _env = pin_default_env();
        std::env::set_var("AWS_S3_ENDPOINT_URL", "");
        std::env::set_var("MINIO_ENDPOINT_URL", "");
        assert_eq!(minio_base(), "http://plane-minio:9000");
    }
}
