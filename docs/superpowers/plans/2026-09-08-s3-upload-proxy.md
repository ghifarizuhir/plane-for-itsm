# S3 Upload Proxy (A+B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upload file sembuh lewat dua jalur: langsung `:8000` (proxy S3 di api-rs) dan via Caddy `:8080` (service proxy di compose-local).

**Architecture:** Jalur A: route Axum `/:bucket/*rest` di api-rs meneruskan byte mentah ke MinIO internal (`AWS_S3_ENDPOINT_URL`), tanpa auth gate (auth dibawa SigV4 presigned, sama seperti Caddy). Jalur B: service `proxy` di `docker-compose-local.yml` memakai `apps/proxy` yang sudah ada, di port host `8080` agar tidak butuh root. Tidak ada dependensi baru, tidak ada perubahan kontrak presign.

**Tech Stack:** Rust Axum 0.7 + reqwest 0.12 (sudah ada, tanpa tambah feature — body ≤5MB sudah dibatasi `RequestBodyLimitLayer`), Caddy 2.11, Django `requests==2.33.0` (sudah ada, hanya task opsional).

**Konteks masalah (jangan implementasi ulang diagnosis ini):** Dengan `USE_MINIO=1`, backend mengembalikan `upload_data.url = http://<Host-header>/uploads/` (`apps/api/plane/settings/storage.py:39-52`, `apps/api-rs/crates/api/src/routes/asset.rs:225-238,445`). Path `/uploads/*` hanya dilayani Caddy (`apps/proxy/Caddyfile.ce:20-21`). `docker-compose-local.yml` tidak punya service proxy dan frontend menembak `:8000` langsung → 404 `{"error":"Page not found."}` dari backend, bukan dari MinIO. Ini bukan regresi migrasi: Django pun tidak pernah punya route `/uploads/`.

**Fakta kunci yang mengikat desain (hasil riset, jangan dilanggar):**

- Body limit global 5MB (`main.rs:1638-1640`) sudah mencakup route baru → buffer `Bytes` aman, tidak perlu streaming.
- Axum mengutamakan route statik (`/health`, `/api/*`) di atas `/:bucket` apa pun urutan registrasi — route baru tidak bisa membajak API (terverifikasi: tidak ada route dinamis satu-segmen di root).
- Test Rust: unit di-file (precedent `asset.rs`, test di dalam modul) + integrasi `crates/api/tests/*` via `tower::ServiceExt::oneshot` (precedent `cors_test.rs:1-50`). Tanpa DB untuk plan ini.
- Default bucket proxy HARUS `"uploads"` mengikuti Django `plane/settings/common.py:307`, bukan `""` seperti `s3_conf()` di `asset.rs:367`.
- Body 404 parity = `{"error":"Page not found."}` (`plane/app/views/error_404.py:10`, `main.rs:29-34`) — bucket salah HARUS balas body identik.

---

## File Structure

- Modify: `apps/api-rs/crates/api/src/routes/mod.rs:41` — tambah `pub mod s3proxy;`
- Create: `apps/api-rs/crates/api/src/routes/s3proxy.rs` — helper murni + handler proxy (tanggung jawab tunggal)
- Modify: `apps/api-rs/crates/api/src/main.rs:11` (import `head`) + 1 `.route` sebelum blok E9 (~baris 975-983)
- Create: `apps/api-rs/crates/api/tests/s3proxy_test.rs` — test integrasi oneshot
- Modify: `apps/api-rs/scripts/smoke.sh` (sisip setelah baris 459 `AIDS=$(jid asset_id)`)
- Modify: `docker-compose-local.yml` — tambah service `proxy`
- Optional: `apps/api/plane/app/views/asset/s3proxy.py` + `apps/api/plane/app/urls/asset.py` — fallback Django, HANYA bila tim memakai `--profile legacy`

---

### Task 1: Helper murni proxy + unit test (Rust)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/s3proxy.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs:41`

- [ ] **Step 1: Daftarkan modul**

```rust
// apps/api-rs/crates/api/src/routes/mod.rs, setelah baris 41 `pub mod prefs;`
pub mod s3proxy;
```

- [ ] **Step 2: Tulis helper + unit test (file baru, lengkap)**

```rust
// apps/api-rs/crates/api/src/routes/s3proxy.rs (bagian 1: helper murni)
/// Bucket yang dilayani proxy. Default HARUS "uploads" mengikuti
/// Django `plane/settings/common.py:307`, bukan "" seperti `s3_conf()`.
pub fn configured_bucket() -> String {
    std::env::var("AWS_S3_BUCKET_NAME").unwrap_or_else(|_| "uploads".to_string())
}

/// Host MinIO INTERNAL (antar-container), bukan request-Host.
/// Presign memakai request-Host untuk URL publik; proxy memakai ini untuk upstream.
pub fn minio_base() -> String {
    std::env::var("AWS_S3_ENDPOINT_URL")
        .or_else(|_| std::env::var("MINIO_ENDPOINT_URL"))
        .unwrap_or_else(|_| "http://plane-minio:9000".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_keeps_bucket_root_for_presigned_post() {
        std::env::remove_var("AWS_S3_BUCKET_NAME");
        std::env::remove_var("AWS_S3_ENDPOINT_URL");
        std::env::remove_var("MINIO_ENDPOINT_URL");
        assert_eq!(
            proxy_target("uploads", "", None).as_deref(),
            Some("http://plane-minio:9000/uploads/")
        );
    }

    #[test]
    fn target_preserves_key_and_query_verbatim() {
        assert_eq!(
            proxy_target("uploads", "abc/file.png", Some("X-Amz-Algorithm=AWS4-HMAC-SHA256")).as_deref(),
            Some("http://plane-minio:9000/uploads/abc/file.png?X-Amz-Algorithm=AWS4-HMAC-SHA256")
        );
    }

    #[test]
    fn rejects_wrong_bucket_and_traversal() {
        assert!(proxy_target("other", "", None).is_none());
        assert!(proxy_target("uploads", "../etc/passwd", None).is_none());
    }
}
```

- [ ] **Step 3: Jalankan test**

Run: `cargo test -p api --lib routes::s3proxy` (workdir `apps/api-rs`)
Expected: PASS 3 test. Jika FAIL karena env bocor antar-test (`AWS_S3_BUCKET_NAME` di-set test lain yang jalan paralel), set env eksplisit di tiap test via `std::env::set_var` sebelum assert, jangan mengandalkan `remove_var` saja.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/s3proxy.rs apps/api-rs/crates/api/src/routes/mod.rs
git commit -m "feat(rs-api): pure S3 proxy URL helpers with unit tests"
```

---

### Task 2: Handler proxy + registrasi route + test 404-parity

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/s3proxy.rs` (tambah handler)
- Modify: `apps/api-rs/crates/api/src/main.rs:11` + 1 `.route` sebelum blok E9
- Create: `apps/api-rs/crates/api/tests/s3proxy_test.rs`

- [ ] **Step 1: Tulis test integrasi dulu (gagal: handler belum ada)**

```rust
// apps/api-rs/crates/api/tests/s3proxy_test.rs
use api::routes::s3proxy::proxy_to_minio;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Router,
};
use tower::ServiceExt as _;

fn app() -> Router {
    Router::new().route("/:bucket/*rest", get(proxy_to_minio).post(proxy_to_minio))
}

#[tokio::test]
async fn wrong_bucket_returns_django_identical_404() {
    let req = Request::builder()
        .uri("/not-a-bucket/x")
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"error": "Page not found."})
    );
}
```

- [ ] **Step 2: Jalankan, harapkan FAIL (`proxy_to_minio` belum ada)**

Run: `cargo test -p api --test s3proxy_test` (workdir `apps/api-rs`)
Expected: FAIL kompilasi `unresolved import / no function`.

- [ ] **Step 3: Implementasi handler minimal (tambah ke `s3proxy.rs`)**

```rust
// Tambahan di apps/api-rs/crates/api/src/routes/s3proxy.rs
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
        if let Some(v) = upstream.headers().get(h).cloned() {
            resp = resp.header(h, v);
        }
    }
    let bytes = upstream.bytes().await.unwrap_or_default();
    resp.body(Body::from(bytes)).unwrap()
}
```

- [ ] **Step 4: Registrasi route di `main.rs` (2 edit kecil)**

```rust
// main.rs baris 11, ubah import routing:
routing::{delete, get, head, patch, post, put},
```

```rust
// main.rs, tepat sebelum blok komentar E9 asset
// (sebelum baris .route("/api/assets/v2/workspaces/:slug/", ...) ~baris 983):
.route("/:bucket/*rest", get(routes::s3proxy::proxy_to_minio).post(routes::s3proxy::proxy_to_minio).put(routes::s3proxy::proxy_to_minio).delete(routes::s3proxy::proxy_to_minio).head(routes::s3proxy::proxy_to_minio))
```

- [ ] **Step 5: Jalankan test, harapkan PASS**

Run: `cargo test -p api --test s3proxy_test && cargo test -p api --lib routes::s3proxy && cargo test -p api --test route_parity_test` (workdir `apps/api-rs`)
Expected: semua PASS (route baru tidak menggeser route statik — parity gate membuktikannya).

- [ ] **Step 6: fmt + commit**

Run: `cargo fmt --check` (workdir `apps/api-rs`; jika gagal, `cargo fmt` lalu ulangi check)

```bash
git add apps/api-rs/crates/api/src/routes/s3proxy.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/s3proxy_test.rs
git commit -m "feat(rs-api): S3 bucket proxy to MinIO for direct :8000 uploads"
```

---

### Task 3: Round-trip hidup presign → proxy-POST → PATCH → GET (lokal)

**Files:**

- Modify: `apps/api-rs/scripts/smoke.sh` (sisip setelah baris 459 `AIDS=$(jid asset_id)`)

- [ ] **Step 1: Tambah cek round-trip ke smoke.sh**

```bash
UPURL=$(jid upload_data.url)
echo "$UPURL" | grep -q '/uploads/$' && { PASS=$((PASS+1)); echo "ok   e9-proxy-url -> bucket path"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e9-proxy-url"; echo "FAIL e9-proxy-url: $UPURL"; }
printf 'smoke-bytes' > /tmp/smoke_up.bin
FIELDS=$(jq -r '.upload_data.fields | to_entries[] | "-F\n\(.key)=\(.value)"' /tmp/smoke_body)
# shellcheck disable=SC2086
UPCODE=$(curl -s -o /tmp/smoke_up_resp -w "%{http_code}" -X POST $FIELDS -F "file=@/tmp/smoke_up.bin;type=image/png" "$UPURL")
[ "$UPCODE" = "204" ] && { PASS=$((PASS+1)); echo "ok   e9-proxy-post -> 204"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e9-proxy-post"; echo "FAIL e9-proxy-post: $UPCODE $(head -c 200 /tmp/smoke_up_resp)"; }
check e9-ws-complete 204 -X PATCH "$BASE/api/assets/v2/workspaces/$WS/$AIDS/"
check e9-ws-get-redirect 302 "$BASE/api/assets/v2/workspaces/$WS/$AIDS/"
```

- [ ] **Step 2: Rebuild + jalankan stack lokal + smoke**

Run:

```bash
docker compose -f docker-compose-local.yml build api
docker compose -f docker-compose-local.yml up -d api plane-db plane-redis plane-minio
BASE=http://192.168.1.11:8000 bash apps/api-rs/scripts/smoke.sh
```

Expected: `e9-proxy-post -> 204`, `e9-ws-complete 204`, `e9-ws-get-redirect 302`. Jika `e9-proxy-post` 400 dari MinIO = signature invalid → JANGAN ubah proxy; baca body MinIO (`/tmp/smoke_up_resp`) — 99% penyebabnya field `-F` tidak verbatim (urutan/encoding), bukan bug proxy.

- [ ] **Step 3: Verifikasi manual persis skenario pelapor (browser)**

Run:

```bash
curl -s -X POST -H 'Content-Type: application/json' -d '{"entity_type":"USER_AVATAR","name":"cek.png","type":"image/png","size":21099}' http://192.168.1.11:8000/api/assets/v2/user-assets/ -H "Authorization: Bearer <API_KEY>" | tee /tmp/cek.json
```

Expected: `upload_data.url` = `http://192.168.1.11:8000/uploads/`, lalu ulangi POST multipart ke URL itu → 204 (bukan lagi 404 `{"error":"Page not found."}`).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/scripts/smoke.sh
git commit -m "test(rs-api): e9 presign-proxy-POST round trip in smoke"
```

---

### Task 4: Service proxy di compose-local (Jalur B)

**Files:**

- Modify: `docker-compose-local.yml` (tambah 1 service)

- [ ] **Step 1: Tambah service (port host 8080 agar tidak butuh root; SITE_ADDRESS=:80 internal)**

```yaml
proxy:
  container_name: plane-proxy
  build:
    context: ./apps/proxy
    dockerfile: Dockerfile.ce
  restart: unless-stopped
  networks:
    - dev_env
  ports:
    - "8080:80"
  environment:
    FILE_SIZE_LIMIT: ${FILE_SIZE_LIMIT:-5242880}
    BUCKET_NAME: ${AWS_S3_BUCKET_NAME:-uploads}
    SITE_ADDRESS: ":80"
    CERT_EMAIL: ""
    TRUSTED_PROXIES: 0.0.0.0/0
  depends_on:
    - api
    - plane-minio
```

Catatan: `CERT_EMAIL: ""` membuat baris `{$CERT_EMAIL}` kosong (diabaikan Caddy); `SITE_ADDRESS: ":80"` berarti tanpa ACME. `reverse_proxy /* web:3000` akan 502 di lokal karena tidak ada container web (frontend via pnpm) — itu ekspektasi; path `/api/*`, `/auth/*`, `/uploads/*` tetap jalan karena service `api` dan `plane-minio` ada di network `dev_env` dengan nama host persis itu.

- [ ] **Step 2: Verifikasi (tanpa mengubah kode apa pun)**

Run:

```bash
docker compose -f docker-compose-local.yml up -d --build proxy
curl -s -o /dev/null -w "%{http_code}\n" http://192.168.1.11:8080/uploads/ -X POST
```

Expected: BUKAN 404 `{"error":"Page not found."}` dari backend — MinIO menjawab 400/403 (request tidak signed, tapi rutenya sampai). Pembanding: `curl -s http://192.168.1.11:8000/uploads/ -X POST` → kini respons via proxy Rust (Jalur A), bukan 404.

- [ ] **Step 3: Commit + catat manual env frontend (TANPA commit file env lokal)**

```bash
git add docker-compose-local.yml
git commit -m "chore(compose-local): add Caddy proxy on :8080 for api/uploads path"
```

Manual (tidak di-commit): `VITE_API_BASE_URL=http://192.168.1.11:8080` di env dev `apps/web` (pnpm) lalu restart; untuk image prod, build ulang dengan `--build-arg VITE_API_BASE_URL=http://192.168.1.11:8080` karena nilai di-bake (`Dockerfile.web:46-47`).

---

### Task 5 (OPSIONAL, hanya bila tim memakai `--profile legacy`): fallback Django

**Files:**

- Create: `apps/api/plane/app/views/asset/s3proxy.py`
- Modify: `apps/api/plane/app/urls/asset.py` (tambah PALING AKHIR agar tidak menelan route V2)

Lewati task ini bila `api-legacy` tidak pernah dinyalakan (`docker-compose-local.yml` tidak menjalankannya; hanya `docker-compose.yml --profile legacy` yang memakai). Isi `s3proxy.py` — forward streaming via `requests` (sudah ada `requests==2.33.0` di `requirements/base.txt:59`):

```python
import os
import requests
from django.http import StreamingHttpResponse, JsonResponse
from rest_framework.permissions import AllowAny
from plane.app.views.base import BaseAPIView

class S3ProxyEndpoint(BaseAPIView):
    permission_classes = [AllowAny]

    def _upstream(self, bucket, key):
        if bucket != os.environ.get("AWS_S3_BUCKET_NAME", "uploads") or ".." in key.split("/"):
            return None
        base = (
            os.environ.get("AWS_S3_ENDPOINT_URL")
            or os.environ.get("MINIO_ENDPOINT_URL")
            or "http://plane-minio:9000"
        ).rstrip("/")
        return f"{base}/{bucket}/{key}"

    def _proxy(self, request, bucket, key):
        url = self._upstream(bucket, key)
        if url is None:
            return JsonResponse({"error": "Page not found."}, status=404)
        qs = request.META.get("QUERY_STRING", "")
        if qs:
            url += "?" + qs
        headers = {}
        if request.META.get("CONTENT_TYPE"):
            headers["Content-Type"] = request.META["CONTENT_TYPE"]
        try:
            r = requests.request(
                request.method, url, data=request.body, headers=headers, timeout=60, stream=True
            )
        except requests.RequestException:
            return JsonResponse({"error": "Object storage unreachable."}, status=502)
        resp = StreamingHttpResponse(r.iter_content(chunk_size=65536), status=r.status_code)
        for h in ("Content-Type", "Content-Disposition", "ETag"):
            if h in r.headers:
                resp[h] = r.headers[h]
        return resp

    def get(self, request, bucket, key=""):
        return self._proxy(request, bucket, key)

    def post(self, request, bucket, key=""):
        return self._proxy(request, bucket, key)
```

Route (paling akhir `urlpatterns` di `asset.py`):

```python
path("<str:bucket>/", S3ProxyEndpoint.as_view(), name="s3-proxy-root"),
path("<str:bucket>/<path:key>", S3ProxyEndpoint.as_view(), name="s3-proxy"),
```

Verifikasi: ulangi Task 3 Step 3 ke `api-legacy:8000` → harapkan 204. Perlu import `S3ProxyEndpoint` di `plane/app/urls/asset.py` dan export di `plane/app/views/__init__.py` mengikuti pola view lain.

- [ ] **Step: Commit terpisah**

```bash
git add apps/api/plane/app/views/asset/s3proxy.py apps/api/plane/app/urls/asset.py
git commit -m "feat(api-legacy): S3 bucket proxy to MinIO for direct uploads"
```

---

## Self-Review

1. **Cakupan:** upload langsung `:8000` → Task 1+2+3; jalur proxy `:8080` → Task 4; profil legacy → Task 5 opsional. Tampil/download gambar ikut sembuh karena 302 mengarah ke host yang kini dilayani proxy (A) atau Caddy (B). Tidak ada perubahan kontrak presign/frontend.
2. **Placeholder scan:** semua langkah berisi kode/perintah/ekspektasi konkret; tidak ada TBD. Satu-satunya manual-step adalah env frontend lokal (disengaja, tidak di-commit).
3. **Konsistensi tipe:** `proxy_target` dipakai identik oleh handler dan test; body 404 identik Django; default bucket `"uploads"` konsisten dengan Django; nama file/route/modul konsisten di semua task. Nomor baris acuan (`main.rs:11`, blok E9 ~983, `smoke.sh:459`, `mod.rs:41`) diverifikasi ulang terhadap HEAD per 2026-09-08 — commit Batch F T5–T9 hanya menggeser baris, tidak mengubah struktur yang disentuh plan.
