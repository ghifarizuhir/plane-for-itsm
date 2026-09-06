use axum::{
    extract::{Path, State},
    http::{header::LOCATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    middleware::auth::AuthUser,
    routes::{
        issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows},
        project::{deny, missing, ws_role},
    },
    state::AppState,
};

// ============================================================================
// E9 — assets/attachments + MinIO presign (App scope: `plane/app/*` only).
// Django sources: `plane/app/views/asset/v2.py`, `plane/app/views/asset/base.py`
// (legacy file-assets), `plane/app/views/issue/attachment.py`,
// `plane/settings/storage.py`, `plane/db/models/asset.py:19-77`,
// `plane/settings/common.py:353,453-556`.
// Every user-visible string below quotes its Django file:line.
// ============================================================================

/// `plane/settings/common.py:353` (`FILE_SIZE_LIMIT`, default 5 MiB).
pub const FILE_SIZE_LIMIT: i64 = 5_242_880;

/// `plane/db/models/asset.py:36-46` (`EntityTypeContext.values`).
pub const ENTITY_TYPES: [&str; 10] = [
    "ISSUE_ATTACHMENT",
    "ISSUE_DESCRIPTION",
    "COMMENT_DESCRIPTION",
    "PAGE_DESCRIPTION",
    "USER_COVER",
    "USER_AVATAR",
    "WORKSPACE_LOGO",
    "PROJECT_COVER",
    "DRAFT_ISSUE_ATTACHMENT",
    "DRAFT_ISSUE_DESCRIPTION",
];

/// Images-only allowlist for user/ws/project presigns
/// (`plane/app/views/asset/v2.py:367-373`, also `:596-602`).
pub const ALLOWED_FILE_TYPES: [&str; 5] = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/jpg",
    "image/gif",
];

/// Full attachment allowlist (`plane/settings/common.py:453-541`).
/// Duplicates from the Django list (`text/markdown` twice) are collapsed —
/// membership is the only contract.
pub const ATTACHMENT_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/svg+xml",
    "image/webp",
    "image/tiff",
    "image/bmp",
    "application/pdf",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.ms-excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "text/plain",
    "text/markdown",
    "application/rtf",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.oasis.opendocument.presentation",
    "application/vnd.oasis.opendocument.graphics",
    "application/vnd.visio",
    "image/x-portable-graymap",
    "image/x-portable-bitmap",
    "image/x-portable-pixmap",
    "application/vnd.oasis.opendocument.database",
    "audio/mpeg",
    "audio/wav",
    "audio/ogg",
    "audio/midi",
    "audio/x-midi",
    "audio/aac",
    "audio/flac",
    "audio/x-m4a",
    "video/mp4",
    "video/mpeg",
    "video/ogg",
    "video/webm",
    "video/quicktime",
    "video/x-msvideo",
    "video/x-ms-wmv",
    "application/zip",
    "application/x-rar",
    "application/x-rar-compressed",
    "application/x-tar",
    "application/gzip",
    "application/x-zip",
    "application/x-zip-compressed",
    "application/x-7z-compressed",
    "application/x-compressed",
    "application/x-compressed-tar",
    "application/x-compressed-tar-gz",
    "application/x-compressed-tar-bz2",
    "application/x-compressed-tar-zip",
    "application/x-compressed-tar-7z",
    "application/x-compressed-tar-rar",
    "model/gltf-binary",
    "model/gltf+json",
    "application/octet-stream",
    "font/ttf",
    "font/otf",
    "font/woff",
    "font/woff2",
    "text/css",
    "text/javascript",
    "application/json",
    "text/xml",
    "text/csv",
    "application/xml",
    "application/x-sql",
    "application/x-gzip",
];

/// Script-capable MIME types forced to `attachment` disposition
/// (`plane/settings/common.py:546-556`).
const SCRIPT_MIME_TYPES: &[&str] = &[
    "image/svg+xml",
    "text/javascript",
    "application/javascript",
    "text/html",
    "application/xhtml+xml",
    "text/xml",
    "application/xml",
];

// --- Verbatim Django strings ------------------------------------------------

/// `v2.py:122-126,349-353,589-593` + static `v2.py:514-517`.
const INVALID_ENTITY_MSG: &str = "Invalid entity type.";
/// `v2.py:821-825` (duplicate endpoint only).
const INVALID_ENTITY_DUP_MSG: &str = "Invalid entity type or entity id";
/// `v2.py:374-381,603-610` + user `v2.py:136-143`.
const INVALID_IMAGE_TYPE_MSG: &str =
    "Invalid file type. Only JPEG, PNG, WebP, JPG and GIF files are allowed.";
/// `attachment.py:105-109` (issue attachments take the full allowlist).
const INVALID_FILE_TYPE_MSG: &str = "Invalid file type.";
/// `v2.py:362` (WORKSPACE_LOGO needs workspace ADMIN).
const WS_LOGO_ADMIN_MSG: &str = "Only workspace admins can upload a workspace logo.";
/// `v2.py:424,451,468` (project-bound asset, workspace endpoint).
const NO_ASSET_ACCESS_MSG: &str = "You don't have access to this asset.";
/// `v2.py:475,503,682,731,881,909` (uploaded-assets miss / not-uploaded).
const ASSET_MISSING_MSG: &str = "The requested asset could not be found.";
/// `attachment.py:180-183` (issue single GET on a non-uploaded asset).
const ASSET_NOT_UPLOADED_MSG: &str = "The asset is not uploaded.";
/// `attachment.py:68-71` (issue-attachment single miss).
const ISSUE_ATTACHMENT_MISSING_MSG: &str = "Issue attachment not found.";
/// `v2.py:710` (bulk without ids).
const NO_ASSET_IDS_MSG: &str = "No asset ids provided.";
/// `v2.py:831` (duplicate into an unknown project).
const PROJECT_NOT_FOUND_MSG: &str = "Project not found";
/// `v2.py:842` (duplicate with unknown/non-uploaded source).
const ASSET_NOT_FOUND_MSG: &str = "Asset not found";
/// `base.py:33-36,76-79` (legacy GET-check miss quirk: HTTP 200, NOT 404).
const LEGACY_KEY_MISSING_MSG: &str = "Asset key does not exist";

// ============================================================================
// Pure helpers (unit-tested in `e9_tests` below).
// ============================================================================

/// Mirrors `plane/utils/path_validator.py:14-51` (`sanitize_filename`):
/// strip controls, backslash→slash, basename, drop `..`, trim, strip leading
/// dots, `None` when empty/missing.
pub fn sanitize_filename(raw: Option<&str>) -> Option<String> {
    let s = raw?;
    if s.is_empty() {
        return None;
    }
    let no_ctrl: String = s
        .chars()
        .filter(|c| !(*c < '\u{20}' || *c == '\u{7f}'))
        .collect();
    let slashed = no_ctrl.replace('\\', "/");
    let base = slashed.rsplit('/').next().unwrap_or("");
    let no_traversal = base.replace("..", "");
    let t = no_traversal.trim().trim_start_matches('.').trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Mirrors `size_limit = min(settings.FILE_SIZE_LIMIT, size)` (`v2.py:119,384,613`,
/// `attachment.py:118`): clamp, never reject.
pub fn clamp_size(size: i64) -> i64 {
    size.min(FILE_SIZE_LIMIT)
}

/// Key layout for workspace/project/issue assets: `{wsid}/{hex}-{name}`
/// (`v2.py:390,619`, `attachment.py:115`).
pub fn ws_asset_key(ws_id: &str, hex: &str, name: &str) -> String {
    format!("{ws_id}/{hex}-{name}")
}

/// Key layout for user assets: `{hex}-{name}` (no workspace prefix)
/// (`v2.py:146`, cf. `models/asset.py:19-23`).
pub fn user_asset_key(hex: &str, name: &str) -> String {
    format!("{hex}-{name}")
}

/// `plane/settings/storage.py:39-53` HOST RULE: `USE_MINIO==1` → endpoint =
/// request `Host` header (scheme `https` iff `MINIO_ENDPOINT_SSL==1`, else the
/// request scheme); otherwise the raw `AWS_S3_ENDPOINT_URL` verbatim.
pub fn s3_endpoint_for(
    use_minio: bool,
    minio_ssl: bool,
    host: &str,
    scheme: &str,
    raw_endpoint: &str,
) -> String {
    if use_minio {
        let proto = if minio_ssl { "https" } else { scheme };
        format!("{proto}://{host}")
    } else {
        raw_endpoint.to_string()
    }
}

/// `v2.py:523-526`: `attachment` for script-capable MIME types (parameters
/// like `; charset=…` ignored, case-insensitive), else `inline`.
pub fn disposition_for(mime: &str) -> &'static str {
    let base = mime.split(';').next().unwrap_or("").trim().to_lowercase();
    if SCRIPT_MIME_TYPES.contains(&base.as_str()) {
        "attachment"
    } else {
        "inline"
    }
}

pub fn is_image_mime(mime: &str) -> bool {
    ALLOWED_FILE_TYPES.contains(&mime)
}

pub fn is_attachment_mime(mime: &str) -> bool {
    ATTACHMENT_MIME_TYPES.contains(&mime)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAssetInit {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub file_type: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub entity_identifier: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchAsset {
    #[serde(default)]
    pub attributes: Option<Value>,
}

pub fn validate_upload_init(body: &CreateAssetInit) -> Result<(), String> {
    if !ENTITY_TYPES.contains(&body.entity_type.as_str()) {
        return Err("Invalid entity type.".to_string());
    }
    let file_type = body.file_type.as_deref().unwrap_or("image/jpeg");
    if !ALLOWED_FILE_TYPES.contains(&file_type) {
        return Err("Invalid file type. Only JPEG, PNG, WebP, JPG and GIF files are allowed.".to_string());
    }
    Ok(())
}

// ============================================================================
// SigV4 (hand-rolled over `hmac`+`sha2`, mirroring botocore so MinIO accepts).
// ============================================================================

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// SigV4 signing-key derivation (`AWS4` + date → region → service → request).
/// The `e9_tests` vector pins this against the AWS-documented example.
pub fn derive_signing_key(secret: &str, datestamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_secret = format!("AWS4{secret}");
    let k_date = hmac_sha256(k_secret.as_bytes(), datestamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// SigV4 percent-encoding (unreserved `A-Za-z0-9-_.~` pass through).
fn pct_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b as char;
        if c.is_ascii_alphanumeric()
            || c == '-'
            || c == '_'
            || c == '.'
            || c == '~'
            || (keep_slash && c == '/')
        {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn amz_dates(now: chrono::DateTime<Utc>) -> (String, String) {
    (
        now.format("%Y%m%dT%H%M%SZ").to_string(),
        now.format("%Y%m%d").to_string(),
    )
}

// --- S3 config (env names EXACT per `storage.py:27-37` + `common.py:301-316,353) ---

#[derive(Debug, Clone)]
struct S3Conf {
    access: String,
    secret: String,
    bucket: String,
    region: String,
    raw_endpoint: String,
    expiration: i64,
    use_minio: bool,
    minio_ssl: bool,
    file_limit: i64,
}

fn env_s(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn s3_conf() -> S3Conf {
    let raw = std::env::var("AWS_S3_ENDPOINT_URL")
        .or_else(|_| std::env::var("MINIO_ENDPOINT_URL"))
        .unwrap_or_default();
    S3Conf {
        access: env_s("AWS_ACCESS_KEY_ID", ""),
        secret: env_s("AWS_SECRET_ACCESS_KEY", ""),
        bucket: env_s("AWS_S3_BUCKET_NAME", ""),
        region: env_s("AWS_REGION", ""),
        raw_endpoint: raw,
        expiration: std::env::var("SIGNED_URL_EXPIRATION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600),
        use_minio: std::env::var("USE_MINIO").as_deref() == Ok("1"),
        minio_ssl: std::env::var("MINIO_ENDPOINT_SSL").as_deref() == Ok("1"),
        file_limit: std::env::var("FILE_SIZE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(FILE_SIZE_LIMIT),
    }
}

fn endpoint_authority(endpoint: &str) -> &str {
    endpoint
        .split("://")
        .nth(1)
        .unwrap_or(endpoint)
        .trim_end_matches('/')
}

fn request_host_scheme(headers: &HeaderMap) -> (String, String) {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "http".to_string());
    (host, scheme)
}

/// Replicates `S3Storage.generate_presigned_post` (`storage.py:65-99`):
/// boto `generate_presigned_post` semantics — `fields={key,Content-Type,
/// x-amz-*}`, `conditions=[{bucket},[content-length-range,1,size],
/// {Content-Type},{key}]`, policy + SigV4-of-policy signature.
fn presign_post(
    conf: &S3Conf,
    endpoint: &str,
    key: &str,
    content_type: &str,
    size_limit: i64,
    now: chrono::DateTime<Utc>,
) -> Value {
    let (amzdate, datestamp) = amz_dates(now);
    let credential = format!("{}/{datestamp}/{}/s3/aws4_request", conf.access, conf.region);
    let expiration = (now + chrono::Duration::seconds(conf.expiration))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let policy = json!({
        "expiration": expiration,
        // Django's base conditions (`storage.py:71-82`) PLUS the `x-amz-*`
        // field conditions boto injects (`generate_presigned_post` adds
        // `x-amz-algorithm/credential/date` to both fields AND conditions —
        // S3/MinIO reject any form field missing from the policy, proven by
        // the live round-trip test below).
        "conditions": [
            {"bucket": conf.bucket},
            ["content-length-range", 1, size_limit],
            {"Content-Type": content_type},
            {"key": key},
            {"x-amz-algorithm": "AWS4-HMAC-SHA256"},
            {"x-amz-credential": credential},
            {"x-amz-date": amzdate},
        ],
    });
    let policy_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, policy.to_string());
    let signing = derive_signing_key(&conf.secret, &datestamp, &conf.region, "s3");
    let signature = hex::encode(hmac_sha256(&signing, policy_b64.as_bytes()));
    let url = format!("{}/{}/", endpoint.trim_end_matches('/'), conf.bucket);
    json!({
        "url": url,
        "fields": {
            "key": key,
            "Content-Type": content_type,
            "x-amz-algorithm": "AWS4-HMAC-SHA256",
            "x-amz-credential": credential,
            "x-amz-date": amzdate,
            "policy": policy_b64,
            "x-amz-signature": signature,
        },
    })
}

/// `storage.py:101-110` (`_get_content_disposition`): `None` filename →
/// random hex; else `{disposition}; filename*=UTF-8''{quoted}`.
fn content_disposition(disposition: &str, filename: Option<&str>) -> String {
    let name = filename
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    format!("{disposition}; filename*=UTF-8''{}", pct_encode(&name, true))
}

/// Replicates `S3Storage.generate_presigned_url` (`storage.py:112-140`) for
/// `get_object`: SigV4 query auth (`UNSIGNED-PAYLOAD`, path-style
/// `/{bucket}/{key}`).
fn presign_get(
    conf: &S3Conf,
    endpoint: &str,
    key: &str,
    disposition: &str,
    filename: Option<&str>,
    now: chrono::DateTime<Utc>,
) -> String {
    let (amzdate, datestamp) = amz_dates(now);
    let scope = format!("{datestamp}/{}/s3/aws4_request", conf.region);
    let credential = format!("{}/{}", conf.access, scope);
    let host = endpoint_authority(endpoint).to_string();
    let disp = content_disposition(disposition, filename);
    let mut params: Vec<(String, String)> = vec![
        ("X-Amz-Algorithm".to_string(), "AWS4-HMAC-SHA256".to_string()),
        ("X-Amz-Credential".to_string(), credential),
        ("X-Amz-Date".to_string(), amzdate.clone()),
        ("X-Amz-Expires".to_string(), conf.expiration.to_string()),
        ("X-Amz-SignedHeaders".to_string(), "host".to_string()),
        ("response-content-disposition".to_string(), disp),
    ];
    params.sort_by_key(|(k, _)| k.clone());
    let canonical_qs: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", pct_encode(k, false), pct_encode(v, false)))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_uri = format!(
        "/{}/{}",
        pct_encode(&conf.bucket, false),
        pct_encode(key, true)
    );
    let canonical_headers = format!("host:{host}\n");
    let canonical_req = format!(
        "GET\n{canonical_uri}\n{canonical_qs}\n{canonical_headers}\nhost\nUNSIGNED-PAYLOAD"
    );
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}", sha256_hex(canonical_req.as_bytes()));
    let signing = derive_signing_key(&conf.secret, &datestamp, &conf.region, "s3");
    let signature = hex::encode(hmac_sha256(&signing, string_to_sign.as_bytes()));
    format!(
        "{}{canonical_uri}?{canonical_qs}&X-Amz-Signature={signature}",
        endpoint.trim_end_matches('/')
    )
}

/// Server-side MinIO→MinIO copy for the duplicate endpoint, mirroring
/// `S3Storage.copy_object` (`storage.py:158-170`, `CopySource={Bucket,Key}`)
/// via a SigV4-signed S3 `PUT` with `x-amz-copy-source` (reqwest is already a
/// dependency). Copy failures are logged and ignored — Django's duplicate
/// view ignores `copy_object`'s `None` the same way (`v2.py:861-863` still
/// mark the row uploaded and return 200).
async fn s3_copy_object(conf: &S3Conf, endpoint: &str, src_key: &str, dst_key: &str) {
    let now = Utc::now();
    let (amzdate, datestamp) = amz_dates(now);
    let scope = format!("{datestamp}/{}/s3/aws4_request", conf.region);
    let host = endpoint_authority(endpoint).to_string();
    let copy_source = format!("/{}/{}", pct_encode(&conf.bucket, false), pct_encode(src_key, true));
    let payload_hash = sha256_hex(b"");
    let canonical_uri = format!("/{}/{}", pct_encode(&conf.bucket, false), pct_encode(dst_key, true));
    let canon_headers = format!(
        "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-copy-source:{copy_source}\nx-amz-date:{amzdate}\n"
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-copy-source;x-amz-date";
    let canonical_req =
        format!("PUT\n{canonical_uri}\n\n{canon_headers}\n{signed_headers}\n{payload_hash}");
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}", sha256_hex(canonical_req.as_bytes()));
    let signing = derive_signing_key(&conf.secret, &datestamp, &conf.region, "s3");
    let signature = hex::encode(hmac_sha256(&signing, string_to_sign.as_bytes()));
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        conf.access, scope, signed_headers, signature
    );
    let url = format!("{}{canonical_uri}", endpoint.trim_end_matches('/'));
    let res = reqwest::Client::new()
        .put(&url)
        .header("host", &host)
        .header("x-amz-date", &amzdate)
        .header("x-amz-content-sha256", &payload_hash)
        .header("x-amz-copy-source", &copy_source)
        .header("authorization", &auth)
        .body(vec![])
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => {}
        Ok(r) => tracing::warn!(status = %r.status(), "s3 copy_object failed"),
        Err(e) => tracing::warn!(error = %e, "s3 copy_object failed"),
    }
}

// ============================================================================
// Shared gates + lookups (reuse `project`/`issue_common` helpers, not forks).
// ============================================================================

/// WORKSPACE-level gate mirroring `@allow_permission(..., level="WORKSPACE")`
/// (`permissions/base.py:44-50`): any ACTIVE workspace membership whose role
/// is in the allowed list passes; deny body is `deny()` (`base.py:81-84`).
async fn gate_ws_roles(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    allowed: &[i16],
) -> Result<bool, sqlx::Error> {
    Ok(ws_role(pool, user, slug)
        .await?
        .map(|r| allowed.contains(&r))
        .unwrap_or(false))
}

const AMG: &[i16] = &[20, 15, 5];

/// PROJECT-level gate mirroring `@allow_permission(...)` (`base.py:53-78`):
/// allowed project role outright, else any-membership + workspace-ADMIN
/// fallback (same shape as `issue_common::project_gate_allows`, reused).
async fn gate_project_roles(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    allowed: &[i16],
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        role.map(|r| allowed.contains(&r)).unwrap_or(false),
        role.is_some(),
        ws_admin,
    ))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsRow {
    id: uuid::Uuid,
}

async fn workspace_by_slug(
    pool: &sqlx::PgPool,
    slug: &str,
) -> Result<Option<WsRow>, sqlx::Error> {
    sqlx::query_as::<_, WsRow>("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

async fn project_in_workspace(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    wsid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(wsid)
    .fetch_one(pool)
    .await
}

/// Full `file_assets` row (live schema verified via `\d file_assets`:
/// `entity_type, entity_identifier, is_uploaded, attributes, size,
/// storage_metadata`, FK ids, audit cols).
#[derive(Debug, Clone, sqlx::FromRow)]
struct AssetRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    attributes: Value,
    asset: String,
    user_id: Option<uuid::Uuid>,
    workspace_id: Option<uuid::Uuid>,
    draft_issue_id: Option<uuid::Uuid>,
    project_id: Option<uuid::Uuid>,
    issue_id: Option<uuid::Uuid>,
    comment_id: Option<uuid::Uuid>,
    page_id: Option<uuid::Uuid>,
    entity_type: Option<String>,
    entity_identifier: Option<String>,
    is_deleted: bool,
    is_archived: bool,
    external_id: Option<String>,
    external_source: Option<String>,
    size: f64,
    is_uploaded: bool,
    storage_metadata: Option<Value>,
}

const ASSET_COLS: &str = "id, created_at, updated_at, created_by_id, updated_by_id, attributes, asset, \
    user_id, workspace_id, draft_issue_id, project_id, issue_id, comment_id, page_id, entity_type, \
    entity_identifier, is_deleted, is_archived, external_id, external_source, size, is_uploaded, \
    storage_metadata";

/// `FileAsset.asset_url` (`models/asset.py:82-103`).
fn asset_url_for(
    entity_type: Option<&str>,
    id: uuid::Uuid,
    ws_slug: Option<&str>,
    project_id: Option<uuid::Uuid>,
    issue_id: Option<uuid::Uuid>,
) -> Value {
    match entity_type {
        Some("WORKSPACE_LOGO" | "USER_AVATAR" | "USER_COVER" | "PROJECT_COVER") => {
            json!(format!("/api/assets/v2/static/{id}/"))
        }
        Some("ISSUE_ATTACHMENT") => match (ws_slug, project_id, issue_id) {
            (Some(s), Some(p), Some(i)) => json!(format!(
                "/api/assets/v2/workspaces/{s}/projects/{p}/issues/{i}/attachments/{id}/"
            )),
            _ => Value::Null,
        },
        Some(
            "ISSUE_DESCRIPTION" | "COMMENT_DESCRIPTION" | "PAGE_DESCRIPTION"
            | "DRAFT_ISSUE_DESCRIPTION",
        ) => match (ws_slug, project_id) {
            (Some(s), Some(p)) => {
                json!(format!("/api/assets/v2/workspaces/{s}/projects/{p}/{id}/"))
            }
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn opt_uuid(u: Option<uuid::Uuid>) -> Value {
    u.map(|v| json!(v)).unwrap_or(Value::Null)
}

fn opt_str(s: Option<&str>) -> Value {
    s.map(|v| json!(v)).unwrap_or(Value::Null)
}

/// `FileAssetSerializer` (`serializers/asset.py:9-13`, `Meta.fields="__all__"`)
/// plus the read-only `asset_url` (`serializers/issue.py:616-630`).
///
/// All model fields (FKs as ids) plus `asset_url`. Shared by the legacy
/// GET-check, the issue list, and the issue-presign `attachment` key.
fn full_asset_json(r: &AssetRow, ws_slug: Option<&str>) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": opt_uuid(r.created_by_id),
        "updated_by": opt_uuid(r.updated_by_id),
        "attributes": r.attributes,
        "asset": r.asset,
        "user": opt_uuid(r.user_id),
        "workspace": opt_uuid(r.workspace_id),
        "draft_issue": opt_uuid(r.draft_issue_id),
        "project": opt_uuid(r.project_id),
        "issue": opt_uuid(r.issue_id),
        "comment": opt_uuid(r.comment_id),
        "page": opt_uuid(r.page_id),
        "entity_type": opt_str(r.entity_type.as_deref()),
        "entity_identifier": opt_str(r.entity_identifier.as_deref()),
        "is_deleted": r.is_deleted,
        "is_archived": r.is_archived,
        "external_id": opt_str(r.external_id.as_deref()),
        "external_source": opt_str(r.external_source.as_deref()),
        "size": r.size,
        "is_uploaded": r.is_uploaded,
        "storage_metadata": r.storage_metadata.clone().unwrap_or(json!({})),
        "asset_url": asset_url_for(r.entity_type.as_deref(), r.id, ws_slug, r.project_id, r.issue_id),
    })
}

fn redirect_302(url: String) -> Result<Response, common::errors::AppError> {
    let mut headers = HeaderMap::new();
    let v: axum::http::HeaderValue = url
        .parse()
        .map_err(|_| common::errors::AppError::internal())?;
    headers.insert(LOCATION, v);
    Ok((StatusCode::FOUND, headers, "").into_response())
}

fn parse_size(v: Option<&Value>, default: i64) -> Result<i64, ()> {
    match v {
        None => Ok(default),
        Some(Value::Number(n)) => n.as_i64().ok_or(()),
        Some(Value::String(s)) => s.trim().parse::<i64>().map_err(|_| ()),
        _ => Err(()),
    }
}

async fn find_asset(
    st: &AppState,
    slug: &str,
    asset_id: uuid::Uuid,
) -> Result<Option<common::models::asset::FileAsset>, common::errors::AppError> {
    Ok(sqlx::query_as::<_, common::models::asset::FileAsset>(
        "SELECT a.id, a.workspace_id, a.project_id, a.entity_type, a.is_uploaded FROM file_assets a JOIN workspaces w ON w.id = a.workspace_id WHERE w.slug = $1 AND a.id = $2 AND a.deleted_at IS NULL",
    )
    .bind(slug)
    .bind(asset_id)
    .fetch_optional(&st.pool)
    .await?)
}

/// Mirrors `has_project_asset_access` (`v2.py:316-338`): workspace-level entity
/// types (`project_id IS NULL`) always pass; project-bound assets require an
/// active `ProjectMember` row scoped to the asset's workspace + project.
async fn check_project_access(
    st: &AppState,
    auth: &AuthUser,
    asset: &common::models::asset::FileAsset,
) -> Result<bool, common::errors::AppError> {
    let Some(project_id) = asset.project_id else {
        return Ok(true);
    };
    let uid = auth.0;
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_members WHERE member_id = $1 AND workspace_id = $2 AND project_id = $3 AND is_active = true AND deleted_at IS NULL)",
    )
    .bind(uid)
    .bind(asset.workspace_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok(allowed)
}

/// `get_entity_id_field` (`v2.py:206-236,551-578,782-813`): map
/// `(entity_type, entity_identifier)` to the FK column. Identifiers that are
/// not UUIDs are skipped (Django would crash with a 500 here — sane +
/// documented). `DRAFT_*` selects NO column: Django's workspace-presign
/// (`v2.py:206-236`) and duplicate (`v2.py:782-813`) `get_entity_id_field`
/// have no DRAFT branch → `{}` → FK stays NULL (only the project-presign
/// `v2.py:576-577` `DRAFT_ISSUE_DESCRIPTION` branch binds `draft_issue_id`,
/// and the shared presign/duplicate inserts below intentionally leave it
/// NULL for uniform DRAFT handling).
fn entity_fk_fragment(entity_type: &str, entity_id: Option<&str>) -> (&'static str, Option<uuid::Uuid>) {
    let id = entity_id.and_then(|s| s.parse::<uuid::Uuid>().ok());
    let col = match entity_type {
        "WORKSPACE_LOGO" => "workspace_id",
        "PROJECT_COVER" => "project_id",
        "USER_AVATAR" | "USER_COVER" => "user_id",
        "ISSUE_ATTACHMENT" | "ISSUE_DESCRIPTION" => "issue_id",
        "PAGE_DESCRIPTION" => "page_id",
        "COMMENT_DESCRIPTION" => "comment_id",
        _ => "",
    };
    (col, id)
}

// ============================================================================
// E9a — presign POSTs (all 200 `{upload_data, asset_id, asset_url}`).
// ============================================================================

/// `WorkspaceFileAssetEndpoint.post` (`v2.py:340-415`): WORKSPACE-level gate
/// ADMIN/MEMBER/GUEST; `WORKSPACE_LOGO` additionally needs workspace ADMIN
/// (`v2.py:356-364`); images-only MIME; size clamp; key `{wsid}/{hex}-{name}`.
pub async fn ws_presign(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny());
    }
    let entity_type = body.get("entity_type").and_then(Value::as_str).unwrap_or("");
    if !ENTITY_TYPES.contains(&entity_type) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_ENTITY_MSG, "status": false})),
        ));
    }
    if entity_type == "WORKSPACE_LOGO"
        && !gate_ws_roles(&st.pool, auth.0, &slug, &[20]).await?
    {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": WS_LOGO_ADMIN_MSG}))));
    }
    let mime = body.get("type").and_then(Value::as_str).unwrap_or("image/jpeg");
    if !is_image_mime(mime) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_IMAGE_TYPE_MSG, "status": false})),
        ));
    }
    let conf = s3_conf();
    let size = match parse_size(body.get("size"), conf.file_limit) {
        Ok(v) => v,
        // Django `int(...)` (`v2.py:344`) raises → 500; map to a 400
        // (documented normalize-crash).
        Err(_) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid size."}))));
        }
    };
    let size_limit = clamp_size(size).min(conf.file_limit);
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let raw_name = body.get("name").and_then(Value::as_str);
    let name = sanitize_filename(raw_name).unwrap_or_else(|| "unnamed".to_string());
    let key = ws_asset_key(&ws.id.to_string(), &uuid::Uuid::new_v4().simple().to_string(), &name);
    let entity_id = body.get("entity_identifier").and_then(Value::as_str);
    let (fk_col, fk_id) = entity_fk_fragment(entity_type, entity_id);
    let now = Utc::now();
    let row: (uuid::Uuid,) = match fk_col {
        "workspace_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "project_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, project_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(fk_id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "user_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, user_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(fk_id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "issue_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, issue_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(fk_id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "page_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, page_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(fk_id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "comment_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, comment_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(fk_id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        // No FK column (covers `DRAFT_*`): Django's workspace-presign
        // `get_entity_id_field` (`v2.py:206-236`) has no DRAFT branch → `{}`
        // → FK stays NULL.
        _ => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit}))
        .bind(&key).bind(size_limit as f64).bind(ws.id).bind(auth.0)
        .bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
    };
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let upload_data = presign_post(&conf, &endpoint, &key, mime, size_limit, now);
    Ok((
        StatusCode::OK,
        Json(json!({
            "upload_data": upload_data,
            "asset_id": row.0,
            "asset_url": asset_url_for(Some(entity_type), row.0, Some(&slug), None, fk_id.filter(|_| entity_type == "ISSUE_ATTACHMENT")),
        })),
    ))
}

/// `UserAssetsV2Endpoint.post` (`v2.py:111-170`): authenticated-only (no
/// workspace); entity must be `USER_AVATAR`/`USER_COVER`; images-only; key
/// `{hex}-{name}` (`v2.py:146`); row carries `user=request.user`.
pub async fn user_presign(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let entity_type = body.get("entity_type").and_then(Value::as_str).unwrap_or("");
    if !["USER_AVATAR", "USER_COVER"].contains(&entity_type) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_ENTITY_MSG, "status": false})),
        ));
    }
    let mime = body.get("type").and_then(Value::as_str).unwrap_or("image/jpeg");
    if !is_image_mime(mime) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_IMAGE_TYPE_MSG, "status": false})),
        ));
    }
    let conf = s3_conf();
    let size = match parse_size(body.get("size"), conf.file_limit) {
        Ok(v) => v,
        Err(_) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid size."}))));
        }
    };
    let size_limit = clamp_size(size).min(conf.file_limit);
    let raw_name = body.get("name").and_then(Value::as_str);
    let name = sanitize_filename(raw_name).unwrap_or_else(|| "unnamed".to_string());
    let key = user_asset_key(&uuid::Uuid::new_v4().simple().to_string(), &name);
    let now = Utc::now();
    let row: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO file_assets (id, attributes, asset, size, user_id, created_by_id, entity_type, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, $5, false, false, false, '{}', now(), now()) RETURNING id",
    )
    .bind(json!({"name": name, "type": mime, "size": size_limit}))
    .bind(&key)
    .bind(size_limit as f64)
    .bind(auth.0)
    .bind(entity_type)
    .fetch_one(&st.pool)
    .await?;
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let upload_data = presign_post(&conf, &endpoint, &key, mime, size_limit, now);
    Ok((
        StatusCode::OK,
        Json(json!({
            "upload_data": upload_data,
            "asset_id": row.0,
            "asset_url": asset_url_for(Some(entity_type), row.0, None, None, None),
        })),
    ))
}

/// `ProjectAssetEndpoint.post` (`v2.py:580-645`): PROJECT-level
/// ADMIN/MEMBER/GUEST gate; images-only; key `{wsid}/{hex}-{name}`;
/// row carries `project_id` + the entity FK (`v2.py:551-578`).
pub async fn project_presign(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let entity_type = body.get("entity_type").and_then(Value::as_str).unwrap_or("");
    if !ENTITY_TYPES.contains(&entity_type) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_ENTITY_MSG, "status": false})),
        ));
    }
    let mime = body.get("type").and_then(Value::as_str).unwrap_or("image/jpeg");
    if !is_image_mime(mime) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_IMAGE_TYPE_MSG, "status": false})),
        ));
    }
    let conf = s3_conf();
    let size = match parse_size(body.get("size"), conf.file_limit) {
        Ok(v) => v,
        Err(_) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid size."}))));
        }
    };
    let size_limit = clamp_size(size).min(conf.file_limit);
    let raw_name = body.get("name").and_then(Value::as_str);
    let name = sanitize_filename(raw_name).unwrap_or_else(|| "unnamed".to_string());
    let key = ws_asset_key(&ws.id.to_string(), &uuid::Uuid::new_v4().simple().to_string(), &name);
    let entity_id = body.get("entity_identifier").and_then(Value::as_str);
    let (fk_col, fk_id) = entity_fk_fragment(entity_type, entity_id);
    let now = Utc::now();
    // Django passes the entity FK alongside `project_id` (`v2.py:622-631`).
    let row: (uuid::Uuid,) = match fk_col {
        "issue_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, project_id, issue_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit})).bind(&key).bind(size_limit as f64)
        .bind(ws.id).bind(project_id).bind(fk_id).bind(auth.0).bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "page_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, project_id, page_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit})).bind(&key).bind(size_limit as f64)
        .bind(ws.id).bind(project_id).bind(fk_id).bind(auth.0).bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        "comment_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, project_id, comment_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit})).bind(&key).bind(size_limit as f64)
        .bind(ws.id).bind(project_id).bind(fk_id).bind(auth.0).bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
        // No FK column (covers `DRAFT_*` → FK stays NULL, matching the
        // workspace-presign / duplicate Django behavior).
        _ => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, project_id, created_by_id, entity_type, entity_identifier, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, '{}', now(), now()) RETURNING id",
        )
        .bind(json!({"name": name, "type": mime, "size": size_limit})).bind(&key).bind(size_limit as f64)
        .bind(ws.id).bind(project_id).bind(auth.0).bind(entity_type).bind(entity_id)
        .fetch_one(&st.pool).await?,
    };
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let upload_data = presign_post(&conf, &endpoint, &key, mime, size_limit, now);
    Ok((
        StatusCode::OK,
        Json(json!({
            "upload_data": upload_data,
            "asset_id": row.0,
            "asset_url": asset_url_for(Some(entity_type), row.0, Some(&slug), Some(project_id), fk_id.filter(|_| entity_type == "ISSUE_ATTACHMENT")),
        })),
    ))
}

/// `IssueAttachmentV2Endpoint.post` (`attachment.py:99-147`): PROJECT-level
/// gate; `type` required and in the FULL `ATTACHMENT_MIME_TYPES`
/// (`attachment.py:105-109`); key `{wsid}/{hex}-{name}`; row carries
/// `issue_id` + `project_id` with `ISSUE_ATTACHMENT`.
pub async fn issue_presign(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let mime = body.get("type").and_then(Value::as_str).unwrap_or("");
    if mime.is_empty() || !is_attachment_mime(mime) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_FILE_TYPE_MSG, "status": false})),
        ));
    }
    // Sane + documented: Django never checks the issue (FK violation → 500);
    // return the standard 404 instead.
    let issue_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues i JOIN projects p ON p.id = i.project_id \
         WHERE i.id = $1 AND i.project_id = $2 AND p.workspace_id = $3 AND i.deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(ws.id)
    .fetch_one(&st.pool)
    .await?;
    if !issue_ok {
        return Ok(missing());
    }
    let conf = s3_conf();
    let size = match parse_size(body.get("size"), conf.file_limit) {
        Ok(v) => v,
        Err(_) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid size."}))));
        }
    };
    let size_limit = clamp_size(size).min(conf.file_limit);
    let raw_name = body.get("name").and_then(Value::as_str);
    let name = sanitize_filename(raw_name).unwrap_or_else(|| "unnamed".to_string());
    let key = ws_asset_key(&ws.id.to_string(), &uuid::Uuid::new_v4().simple().to_string(), &name);
    let now = Utc::now();
    let row: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, project_id, issue_id, created_by_id, entity_type, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, 'ISSUE_ATTACHMENT', false, false, false, '{}', now(), now()) RETURNING id",
    )
    .bind(json!({"name": name, "type": mime, "size": size_limit}))
    .bind(&key)
    .bind(size_limit as f64)
    .bind(ws.id)
    .bind(project_id)
    .bind(issue_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let full: Option<AssetRow> =
        sqlx::query_as(&format!("SELECT {ASSET_COLS} FROM file_assets WHERE id = $1"))
            .bind(row.0)
            .fetch_optional(&st.pool)
            .await?;
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let upload_data = presign_post(&conf, &endpoint, &key, mime, size_limit, now);
    Ok((
        StatusCode::OK,
        Json(json!({
            "upload_data": upload_data,
            "asset_id": row.0,
            "attachment": full.map(|r| full_asset_json(&r, Some(&slug))).unwrap_or(Value::Null),
            "asset_url": asset_url_for(Some("ISSUE_ATTACHMENT"), row.0, Some(&slug), Some(project_id), Some(issue_id)),
        })),
    ))
}

// ============================================================================
// Completes: PATCH → 204 (`is_uploaded=true` + entity FK binding).
// ============================================================================

/// `WorkspaceFileAssetEndpoint.patch` (`v2.py:417-443`) + `entity_asset_save`
/// (`v2.py:249-286`): `WORKSPACE_LOGO` binds `workspaces.logo_asset_id`
/// (previous logo soft-deleted), `PROJECT_COVER` binds
/// `projects.cover_image_asset_id`. Celery `get_asset_object_metadata` skipped.
pub async fn mark_uploaded(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<PatchAsset>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny());
    }
    let Some(asset) = find_asset(&st, &slug, asset_id).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    };
    if !check_project_access(&st, &auth, &asset).await? {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": NO_ASSET_ACCESS_MSG}))));
    }
    let full: Option<AssetRow> =
        sqlx::query_as(&format!("SELECT {ASSET_COLS} FROM file_assets WHERE id = $1"))
            .bind(asset_id)
            .fetch_optional(&st.pool)
            .await?;
    if let Some(r) = full {
        match r.entity_type.as_deref() {
            Some("WORKSPACE_LOGO") => {
                if let Some(wsid) = r.workspace_id {
                    let prev: Option<uuid::Uuid> =
                        sqlx::query_scalar("SELECT logo_asset_id FROM workspaces WHERE id = $1")
                            .bind(wsid)
                            .fetch_optional(&st.pool)
                            .await?
                            .flatten();
                    if let Some(p) = prev.filter(|p| *p != asset_id) {
                        sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
                            .bind(p).execute(&st.pool).await?;
                    }
                    sqlx::query("UPDATE workspaces SET logo = '', logo_asset_id = $1 WHERE id = $2")
                        .bind(asset_id).bind(wsid).execute(&st.pool).await?;
                }
            }
            Some("PROJECT_COVER") => {
                if let Some(pid) = r.project_id {
                    let prev: Option<uuid::Uuid> = sqlx::query_scalar(
                        "SELECT cover_image_asset_id FROM projects WHERE id = $1",
                    )
                    .bind(pid)
                    .fetch_optional(&st.pool)
                    .await?
                    .flatten();
                    if let Some(p) = prev.filter(|p| *p != asset_id) {
                        sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
                            .bind(p).execute(&st.pool).await?;
                    }
                    sqlx::query("UPDATE projects SET cover_image = '', cover_image_asset_id = $1 WHERE id = $2")
                        .bind(asset_id).bind(pid).execute(&st.pool).await?;
                }
            }
            _ => {}
        }
    }
    sqlx::query("UPDATE file_assets SET is_uploaded = true, attributes = COALESCE($1, attributes) WHERE id = $2")
        .bind(&body.attributes)
        .bind(asset_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `UserAssetsV2Endpoint.patch` (`v2.py:172-191`) + `entity_asset_save`
/// (`v2.py:43-80`): binds `users.avatar_asset_id` / `cover_image_asset_id`
/// (previous asset soft-deleted).
pub async fn user_complete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(asset_id): Path<uuid::Uuid>,
    Json(body): Json<PatchAsset>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let full: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL"
    ))
    .bind(asset_id)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some(r) = full else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    };
    match r.entity_type.as_deref() {
        Some("USER_AVATAR") => {
            let prev: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT avatar_asset_id FROM users WHERE id = $1")
                    .bind(auth.0)
                    .fetch_optional(&st.pool)
                    .await?
                    .flatten();
            if let Some(p) = prev.filter(|p| *p != asset_id) {
                sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
                    .bind(p).execute(&st.pool).await?;
            }
            sqlx::query("UPDATE users SET avatar = '', avatar_asset_id = $1 WHERE id = $2")
                .bind(asset_id).bind(auth.0).execute(&st.pool).await?;
        }
        Some("USER_COVER") => {
            let prev: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT cover_image_asset_id FROM users WHERE id = $1")
                    .bind(auth.0)
                    .fetch_optional(&st.pool)
                    .await?
                    .flatten();
            if let Some(p) = prev.filter(|p| *p != asset_id) {
                sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
                    .bind(p).execute(&st.pool).await?;
            }
            sqlx::query("UPDATE users SET cover_image = NULL, cover_image_asset_id = $1 WHERE id = $2")
                .bind(asset_id).bind(auth.0).execute(&st.pool).await?;
        }
        _ => {}
    }
    sqlx::query("UPDATE file_assets SET is_uploaded = true, attributes = COALESCE($1, attributes) WHERE id = $2")
        .bind(&body.attributes)
        .bind(asset_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `ProjectAssetEndpoint.patch` (`v2.py:647-661`): no entity binding —
/// just `is_uploaded` + attributes.
pub async fn project_complete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchAsset>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM file_assets WHERE id = $1 AND workspace_id = $2 AND project_id = $3 AND deleted_at IS NULL)",
    )
    .bind(pk).bind(ws.id).bind(project_id)
    .fetch_one(&st.pool).await?;
    if !exists {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    }
    sqlx::query("UPDATE file_assets SET is_uploaded = true, attributes = COALESCE($1, attributes) WHERE id = $2")
        .bind(&body.attributes).bind(pk).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `IssueAttachmentV2Endpoint.patch` (`attachment.py:205-234`): marks
/// `is_uploaded` (activity + metadata Celery tasks skipped).
pub async fn issue_complete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let updated = sqlx::query(
        "UPDATE file_assets SET is_uploaded = true WHERE id = $1 AND workspace_id = $2 \
         AND project_id = $3 AND issue_id = $4 AND deleted_at IS NULL",
    )
    .bind(pk).bind(ws.id).bind(project_id).bind(issue_id)
    .execute(&st.pool).await?.rows_affected();
    if updated == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ISSUE_ATTACHMENT_MISSING_MSG}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ============================================================================
// Deletes.
// ============================================================================

/// `WorkspaceFileAssetEndpoint.delete` (`v2.py:445-459`) + `entity_asset_delete`
/// (`v2.py:288-314`): soft-delete + FK-clear for LOGO/COVER, with the
/// project-level access check (`v2.py:448-453`).
pub async fn soft_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny());
    }
    // Django's asset_delete is a silent no-op when the row is missing.
    if let Some(asset) = find_asset(&st, &slug, asset_id).await? {
        if !check_project_access(&st, &auth, &asset).await? {
            return Ok((StatusCode::FORBIDDEN, Json(json!({"error": NO_ASSET_ACCESS_MSG}))));
        }
        let full: Option<AssetRow> =
            sqlx::query_as(&format!("SELECT {ASSET_COLS} FROM file_assets WHERE id = $1"))
                .bind(asset_id)
                .fetch_optional(&st.pool)
                .await?;
        if let Some(r) = full {
            match r.entity_type.as_deref() {
                Some("WORKSPACE_LOGO") => {
                    if let Some(wsid) = r.workspace_id {
                        sqlx::query("UPDATE workspaces SET logo_asset_id = NULL WHERE id = $1")
                            .bind(wsid).execute(&st.pool).await?;
                    }
                }
                Some("PROJECT_COVER") => {
                    if let Some(pid) = r.project_id {
                        sqlx::query("UPDATE projects SET cover_image_asset_id = NULL WHERE id = $1")
                            .bind(pid).execute(&st.pool).await?;
                    }
                }
                _ => {}
            }
        }
        sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
            .bind(asset_id)
            .execute(&st.pool)
            .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `UserAssetsV2Endpoint.delete` (`v2.py:193-200`) + `entity_asset_delete`
/// (`v2.py:82-109`): soft-delete + avatar/cover FK-clear.
pub async fn user_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(asset_id): Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let full: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL"
    ))
    .bind(asset_id)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some(r) = full else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    };
    match r.entity_type.as_deref() {
        Some("USER_AVATAR") => {
            sqlx::query("UPDATE users SET avatar_asset_id = NULL WHERE id = $1")
                .bind(auth.0).execute(&st.pool).await?;
        }
        Some("USER_COVER") => {
            sqlx::query("UPDATE users SET cover_image_asset_id = NULL WHERE id = $1")
                .bind(auth.0).execute(&st.pool).await?;
        }
        _ => {}
    }
    sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
        .bind(asset_id).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `ProjectAssetEndpoint.delete` (`v2.py:663-672`): plain soft-delete.
pub async fn project_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let updated = sqlx::query(
        "UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1 \
         AND workspace_id = $2 AND project_id = $3 AND deleted_at IS NULL",
    )
    .bind(pk).bind(ws.id).bind(project_id)
    .execute(&st.pool).await?.rows_affected();
    if updated == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// Issue-attachment DELETE (`attachment.py:149-170` shape, contract: **HARD**
/// delete + 404 `Issue attachment not found.`): gate is ADMIN-or-creator
/// (`@allow_permission([ROLE.ADMIN], creator=True, model=FileAsset)`), same
/// creator-bypass shape as `cycle::destroy`.
///
/// DEVIATION: Django's v2 endpoint soft-deletes (`is_deleted=True`,
/// `attachment.py:154-156`); the E9 contract mandates HARD delete (the v1
/// `IssueAttachmentEndpoint.delete`, `attachment.py:72-73`, semantics).
pub async fn issue_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT created_by_id FROM file_assets WHERE id = $1 AND workspace_id = $2 \
         AND project_id = $3 AND issue_id = $4 AND deleted_at IS NULL",
    )
    .bind(pk).bind(ws.id).bind(project_id).bind(issue_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((created_by,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ISSUE_ATTACHMENT_MISSING_MSG}))));
    };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let admin_ok = project_gate_allows(role == Some(20), role.is_some(), ws_admin);
    if !admin_ok && created_by != Some(auth.0) {
        return Ok(deny());
    }
    sqlx::query("DELETE FROM file_assets WHERE id = $1")
        .bind(pk).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ============================================================================
// GET singles → 302 presigned GET.
// ============================================================================

/// `WorkspaceFileAssetEndpoint.get` (`v2.py:461-488`): project-access 403;
/// not-uploaded → 404 `The requested asset could not be found.`; else 302
/// with `attachment` disposition.
pub async fn ws_get(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
) -> Result<Response, common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny().into_response());
    }
    let Some(asset) = find_asset(&st, &slug, asset_id).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))).into_response());
    };
    if !check_project_access(&st, &auth, &asset).await? {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": NO_ASSET_ACCESS_MSG}))).into_response());
    }
    let full: Option<AssetRow> =
        sqlx::query_as(&format!("SELECT {ASSET_COLS} FROM file_assets WHERE id = $1"))
            .bind(asset_id)
            .fetch_optional(&st.pool)
            .await?;
    let Some(r) = full.filter(|r| r.is_uploaded) else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_MISSING_MSG}))).into_response());
    };
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let name = r.attributes.get("name").and_then(Value::as_str);
    redirect_302(presign_get(&conf, &endpoint, &r.asset, "attachment", name, Utc::now()))
}

/// `ProjectAssetEndpoint.get` (`v2.py:674-695`): same 302 shape scoped to the
/// project (`workspace__slug, project_id, pk`).
pub async fn project_get(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<Response, common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing().into_response());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing().into_response());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny().into_response());
    }
    let full: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND workspace_id = $2 AND project_id = $3 AND deleted_at IS NULL"
    ))
    .bind(pk).bind(ws.id).bind(project_id)
    .fetch_optional(&st.pool).await?;
    let Some(r) = full.filter(|r| r.is_uploaded) else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_MISSING_MSG}))).into_response());
    };
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let name = r.attributes.get("name").and_then(Value::as_str);
    redirect_302(presign_get(&conf, &endpoint, &r.asset, "attachment", name, Utc::now()))
}

/// `IssueAttachmentV2Endpoint.get` (single, `attachment.py:173-191`):
/// not-uploaded → **400** `The asset is not uploaded.`; else 302.
pub async fn issue_get(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, project_id, issue_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<Response, common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing().into_response());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing().into_response());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny().into_response());
    }
    let full: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND workspace_id = $2 AND project_id = $3 AND issue_id = $4 AND deleted_at IS NULL"
    ))
    .bind(pk).bind(ws.id).bind(project_id).bind(issue_id)
    .fetch_optional(&st.pool).await?;
    let Some(r) = full else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ISSUE_ATTACHMENT_MISSING_MSG}))).into_response());
    };
    if !r.is_uploaded {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ASSET_NOT_UPLOADED_MSG, "status": false})),
        ).into_response());
    }
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let name = r.attributes.get("name").and_then(Value::as_str);
    redirect_302(presign_get(&conf, &endpoint, &r.asset, "attachment", name, Utc::now()))
}

/// `IssueAttachmentV2Endpoint.get` (list, `attachment.py:193-203`): uploaded
/// `ISSUE_ATTACHMENT` rows for the issue, serialized full-shape.
pub async fn issue_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let rows: Vec<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE issue_id = $1 AND entity_type = 'ISSUE_ATTACHMENT' \
         AND workspace_id = $2 AND project_id = $3 AND is_uploaded = true AND deleted_at IS NULL \
         ORDER BY created_at DESC"
    ))
    .bind(issue_id).bind(ws.id).bind(project_id)
    .fetch_all(&st.pool).await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(|r| full_asset_json(r, Some(&slug))).collect::<Vec<_>>())),
    ))
}

/// `StaticFileAssetEndpoint.get` (`v2.py:491-533`): **AllowAny** (no auth);
/// not-uploaded → 404; non-static entity → 400; script MIME → `attachment`
/// else `inline`; 302.
pub async fn static_get(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(asset_id): Path<uuid::Uuid>,
) -> Result<Response, common::errors::AppError> {
    let full: Option<AssetRow> =
        sqlx::query_as(&format!("SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND deleted_at IS NULL"))
            .bind(asset_id)
            .fetch_optional(&st.pool)
            .await?;
    let Some(r) = full else {
        // Django `.get` (`v2.py:498`) raises → 500; sane 404 (documented).
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))).into_response());
    };
    if !r.is_uploaded {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_MISSING_MSG}))).into_response());
    }
    match r.entity_type.as_deref() {
        Some("USER_AVATAR" | "USER_COVER" | "WORKSPACE_LOGO" | "PROJECT_COVER") => {}
        _ => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": INVALID_ENTITY_MSG, "status": false})),
            ).into_response());
        }
    }
    let mime = r.attributes.get("type").and_then(Value::as_str).unwrap_or("");
    let disposition = disposition_for(mime);
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    redirect_302(presign_get(&conf, &endpoint, &r.asset, disposition, None, Utc::now()))
}

// ============================================================================
// check / restore / duplicate / downloads.
// ============================================================================

/// `AssetCheckEndpoint.get` (`v2.py:770-776`): 200 `{exists}` over
/// `all_objects` + `deleted_at IS NULL` (soft-deleted rows excluded, but
/// `is_deleted`-only rows without timestamp still count — replicated
/// literally).
pub async fn check(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM file_assets a JOIN workspaces w ON w.id = a.workspace_id WHERE w.slug = $1 AND a.id = $2 AND a.deleted_at IS NULL)",
    )
    .bind(&slug)
    .bind(asset_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"exists": exists}))))
}

/// `AssetRestoreEndpoint.post` (`v2.py:536-545`): clears `is_deleted` /
/// `deleted_at` → 204. Miss → sane 404 (Django `all_objects.get` raises →
/// 500; documented).
pub async fn restore(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny());
    }
    let updated = sqlx::query(
        "UPDATE file_assets a SET is_deleted = false, deleted_at = NULL FROM workspaces w WHERE w.id = a.workspace_id AND w.slug = $1 AND a.id = $2",
    )
    .bind(&slug)
    .bind(asset_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `DuplicateAssetEndpoint.post` (`v2.py:815-865`): validates entity/project,
/// scopes the source to the same workspace + uploaded, creates the duplicate
/// row, copies the object server-side (MinIO→MinIO), marks uploaded → 200
/// `{asset_id}`. Throttle (`AssetRateThrottle`) skipped per contract.
pub async fn duplicate(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny());
    }
    let entity_type = body.get("entity_type").and_then(Value::as_str).unwrap_or("");
    if entity_type.is_empty() || !ENTITY_TYPES.contains(&entity_type) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": INVALID_ENTITY_DUP_MSG}))));
    }
    let project_id = body.get("project_id").and_then(Value::as_str).and_then(|s| s.parse::<uuid::Uuid>().ok());
    let entity_id = body.get("entity_id").and_then(Value::as_str);
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if let Some(pid) = project_id {
        if !project_in_workspace(&st.pool, pid, ws.id).await? {
            return Ok((StatusCode::NOT_FOUND, Json(json!({"error": PROJECT_NOT_FOUND_MSG}))));
        }
    }
    let src: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND is_uploaded = true AND workspace_id = $2 AND deleted_at IS NULL"
    ))
    .bind(asset_id).bind(ws.id)
    .fetch_optional(&st.pool).await?;
    let Some(src) = src else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_NOT_FOUND_MSG}))));
    };
    let orig_name = src.attributes.get("name").and_then(Value::as_str);
    let clean = sanitize_filename(orig_name).unwrap_or_else(|| "unnamed".to_string());
    let dest_key = ws_asset_key(&ws.id.to_string(), &uuid::Uuid::new_v4().simple().to_string(), &clean);
    let (fk_col, fk_id) = entity_fk_fragment(entity_type, entity_id);
    let attrs = json!({
        "name": src.attributes.get("name").cloned().unwrap_or(Value::Null),
        "type": src.attributes.get("type").cloned().unwrap_or(Value::Null),
        "size": src.attributes.get("size").cloned().unwrap_or(Value::Null),
    });
    let dup: (uuid::Uuid,) = match fk_col {
        "workspace_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(&src.storage_metadata)
        .fetch_one(&st.pool).await?,
        "project_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, user_id, issue_id, page_id, comment_id, draft_issue_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, COALESCE($7, $9), NULL, NULL, NULL, NULL, NULL, $8, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(&src.storage_metadata).bind(fk_id)
        .fetch_one(&st.pool).await?,
        "user_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, user_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(fk_id).bind(&src.storage_metadata)
        .fetch_one(&st.pool).await?,
        "issue_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, issue_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(fk_id).bind(&src.storage_metadata)
        .fetch_one(&st.pool).await?,
        "page_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, page_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(fk_id).bind(&src.storage_metadata)
        .fetch_one(&st.pool).await?,
        "comment_id" => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, comment_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(fk_id).bind(&src.storage_metadata)
        .fetch_one(&st.pool).await?,
        // No FK column (covers `DRAFT_*`): Django's duplicate
        // `get_entity_id_field` (`v2.py:782-813`) has no DRAFT branch → `{}`
        // → FK stays NULL.
        _ => sqlx::query_as(
            "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, entity_type, project_id, storage_metadata, is_uploaded, is_deleted, is_archived, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, false, false, false, now(), now()) RETURNING id",
        ).bind(&attrs).bind(&dest_key).bind(src.size).bind(ws.id).bind(auth.0).bind(entity_type).bind(project_id).bind(&src.storage_metadata)
        .fetch_one(&st.pool).await?,
    };
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    s3_copy_object(&conf, &endpoint, &src.asset, &dest_key).await;
    sqlx::query("UPDATE file_assets SET is_uploaded = true WHERE id = $1")
        .bind(dup.0).execute(&st.pool).await?;
    Ok((StatusCode::OK, Json(json!({"asset_id": dup.0}))))
}

/// `WorkspaceAssetDownloadEndpoint.get` (`v2.py:868-892`): uploaded-only
/// lookup; miss → 404 `The requested asset could not be found.`; 302 with
/// `attachment` disposition.
pub async fn ws_download(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, asset_id)): Path<(String, uuid::Uuid)>,
) -> Result<Response, common::errors::AppError> {
    if !gate_ws_roles(&st.pool, auth.0, &slug, AMG).await? {
        return Ok(deny().into_response());
    }
    let full: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets a WHERE a.id = $1 AND a.is_uploaded = true AND a.deleted_at IS NULL \
         AND a.workspace_id = (SELECT id FROM workspaces WHERE slug = $2 AND deleted_at IS NULL)"
    ))
    .bind(asset_id).bind(&slug)
    .fetch_optional(&st.pool).await?;
    let Some(r) = full else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_MISSING_MSG}))).into_response());
    };
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let name = r.attributes.get("name").and_then(Value::as_str);
    redirect_302(presign_get(&conf, &endpoint, &r.asset, "attachment", name, Utc::now()))
}

/// `ProjectAssetDownloadEndpoint.get` (`v2.py:895-919`): PROJECT-level gate;
/// same 302 shape scoped to `(slug, project_id)`.
pub async fn project_download(
    State(st): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((slug, project_id, asset_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<Response, common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing().into_response());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing().into_response());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny().into_response());
    }
    let full: Option<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = $1 AND workspace_id = $2 AND project_id = $3 \
         AND is_uploaded = true AND deleted_at IS NULL"
    ))
    .bind(asset_id).bind(ws.id).bind(project_id)
    .fetch_optional(&st.pool).await?;
    let Some(r) = full else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_MISSING_MSG}))).into_response());
    };
    let conf = s3_conf();
    let (host, scheme) = request_host_scheme(&headers);
    let endpoint = s3_endpoint_for(conf.use_minio, conf.minio_ssl, &host, &scheme, &conf.raw_endpoint);
    let name = r.attributes.get("name").and_then(Value::as_str);
    redirect_302(presign_get(&conf, &endpoint, &r.asset, "attachment", name, Utc::now()))
}

/// `ProjectBulkAssetEndpoint.post` (`v2.py:698-767`): PROJECT-level gate;
/// empty ids → 400; scope = own uploads in this workspace that are
/// unassociated or already in this project (`v2.py:720-724`); the first
/// asset's entity drives the binding (`v2.py:736-765`, `ISSUE_ATTACHMENT`
/// no-op); → 204.
pub async fn bulk(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, entity_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(ws) = workspace_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if !project_in_workspace(&st.pool, project_id, ws.id).await? {
        return Ok(missing());
    }
    if !gate_project_roles(&st.pool, auth.0, &slug, project_id, AMG).await? {
        return Ok(deny());
    }
    let ids: Vec<uuid::Uuid> = body
        .get("asset_ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|s| s.parse::<uuid::Uuid>().ok()))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": NO_ASSET_IDS_MSG}))));
    }
    let rows: Vec<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE id = ANY($1) AND workspace_id = $2 \
         AND created_by_id = $3 AND (project_id = $4 OR project_id IS NULL) AND deleted_at IS NULL \
         ORDER BY created_at DESC"
    ))
    .bind(&ids).bind(ws.id).bind(auth.0).bind(project_id)
    .fetch_all(&st.pool).await?;
    let Some(first) = rows.first() else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": ASSET_MISSING_MSG}))));
    };
    let ids_scoped: Vec<uuid::Uuid> = rows.iter().map(|r| r.id).collect();
    match first.entity_type.as_deref() {
        Some("PROJECT_COVER") => {
            sqlx::query("UPDATE file_assets SET project_id = $1 WHERE id = ANY($2)")
                .bind(project_id).bind(&ids_scoped).execute(&st.pool).await?;
            for r in &rows {
                sqlx::query("UPDATE projects SET cover_image_asset_id = $1 WHERE id = $2")
                    .bind(r.id).bind(project_id).execute(&st.pool).await?;
            }
        }
        Some("ISSUE_DESCRIPTION") => {
            let r = sqlx::query(
                "UPDATE file_assets SET issue_id = $1, project_id = $2 WHERE id = ANY($3)",
            )
            .bind(entity_id).bind(project_id).bind(&ids_scoped)
            .execute(&st.pool).await;
            // Django swallows post-delete integrity races (`v2.py:743-746`).
            if let Err(e) = r {
                if !is_fk_violation(&e) {
                    return Err(e.into());
                }
            }
        }
        Some("COMMENT_DESCRIPTION") => {
            let r = sqlx::query("UPDATE file_assets SET comment_id = $1 WHERE id = ANY($2)")
                .bind(entity_id).bind(&ids_scoped)
                .execute(&st.pool).await;
            if let Err(e) = r {
                if !is_fk_violation(&e) {
                    return Err(e.into());
                }
            }
        }
        Some("PAGE_DESCRIPTION") => {
            sqlx::query("UPDATE file_assets SET page_id = $1 WHERE id = ANY($2)")
                .bind(entity_id).bind(&ids_scoped).execute(&st.pool).await?;
        }
        Some("DRAFT_ISSUE_DESCRIPTION") => {
            let r = sqlx::query("UPDATE file_assets SET draft_issue_id = $1 WHERE id = ANY($2)")
                .bind(entity_id).bind(&ids_scoped)
                .execute(&st.pool).await;
            if let Err(e) = r {
                if !is_fk_violation(&e) {
                    return Err(e.into());
                }
            }
        }
        _ => {} // ISSUE_ATTACHMENT and the rest: no-op (`v2.py` has no branch).
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

fn is_fk_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c == "23503"))
}

// ============================================================================
// E9b — legacy file-assets (NO POST-create: no FE caller).
// ============================================================================

async fn legacy_ws_member(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    wsid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members WHERE workspace_id = $1 AND member_id = $2 \
         AND is_active = true AND deleted_at IS NULL)",
    )
    .bind(wsid)
    .bind(user)
    .fetch_one(pool)
    .await
}

/// `FileAssetEndpoint.get` (`base.py:26-36`): `asset_key = wsid/key`; hit →
/// 200 `{data, status: True}`; MISS QUIRK → **200**
/// `{error: "Asset key does not exist", status: False}` (preserved!).
pub async fn legacy_ws_get(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((workspace_id, key)): Path<(uuid::Uuid, String)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !legacy_ws_member(&st.pool, auth.0, workspace_id).await? {
        return Ok(deny());
    }
    let asset_key = format!("{workspace_id}/{key}");
    let rows: Vec<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE asset = $1 AND deleted_at IS NULL"
    ))
    .bind(&asset_key)
    .fetch_all(&st.pool)
    .await?;
    if rows.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(json!({"error": LEGACY_KEY_MISSING_MSG, "status": false})),
        ));
    }
    let ws_slug: Option<String> = sqlx::query_scalar(
        "SELECT slug FROM workspaces WHERE id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "data": rows.iter().map(|r| full_asset_json(r, ws_slug.as_deref())).collect::<Vec<_>>(),
            "status": true,
        })),
    ))
}

/// `FileAssetEndpoint.delete` (`base.py:48-53`): sets `is_deleted` ONLY (no
/// `deleted_at` — replicated literally) → 204. Miss → sane 404 (Django
/// `.get` raises → 500; documented).
pub async fn legacy_ws_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((workspace_id, key)): Path<(uuid::Uuid, String)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !legacy_ws_member(&st.pool, auth.0, workspace_id).await? {
        return Ok(deny());
    }
    let asset_key = format!("{workspace_id}/{key}");
    let updated = sqlx::query("UPDATE file_assets SET is_deleted = true WHERE asset = $1 AND deleted_at IS NULL")
        .bind(&asset_key)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if updated == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `FileAssetViewSet.restore` (`base.py:59-64`): sets `is_deleted = False`
/// ONLY → 204. Miss → sane 404 (documented).
pub async fn legacy_ws_restore(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((workspace_id, key)): Path<(uuid::Uuid, String)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !legacy_ws_member(&st.pool, auth.0, workspace_id).await? {
        return Ok(deny());
    }
    let asset_key = format!("{workspace_id}/{key}");
    let updated = sqlx::query("UPDATE file_assets SET is_deleted = false WHERE asset = $1 AND deleted_at IS NULL")
        .bind(&asset_key)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if updated == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `UserAssetsEndpoint.get` (`base.py:70-79`): own-asset lookup by raw key;
/// hit → 200 `{data, status: True}`; miss quirk → 200
/// `{error: "Asset key does not exist", status: False}`.
pub async fn legacy_user_get(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let rows: Vec<AssetRow> = sqlx::query_as(&format!(
        "SELECT {ASSET_COLS} FROM file_assets WHERE asset = $1 AND created_by_id = $2 AND deleted_at IS NULL"
    ))
    .bind(&key)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    if rows.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(json!({"error": LEGACY_KEY_MISSING_MSG, "status": false})),
        ));
    }
    Ok((
        StatusCode::OK,
        Json(json!({
            "data": full_asset_json(&rows[0], None),
            "status": true,
        })),
    ))
}

/// `UserAssetsEndpoint.delete` (`base.py:88-92`): own-asset soft-delete
/// (`is_deleted` only) → 204. Miss → sane 404 (documented).
pub async fn legacy_user_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let updated = sqlx::query(
        "UPDATE file_assets SET is_deleted = true WHERE asset = $1 AND created_by_id = $2 AND deleted_at IS NULL",
    )
    .bind(&key)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ============================================================================
// E9 pure-function contract tests (SigV4 vector + key/MIME/host/disposition
// rules). Written first (RED), now pinning the GREEN implementation.
// ============================================================================
#[cfg(test)]
mod e9_tests {
    use super::*;

    #[test]
    fn sigv4_matches_aws_documented_presigned_vector() {
        // AWS docs (`sigv4-query-string-auth`) presigned-URL example:
        // secret `.../bPxRfi...`, date `20130524`, region `us-east-1`,
        // service `s3`, string-to-sign hash `3bfa2928...` → signature
        // `aeeed9bb...f604d404`. This pins key-derivation + final HMAC.
        //
        // NOTE: the E9 brief quotes `...4c4c1bd34a0d02d6f2` as the expected
        // value for the `+`-variant key; that matches NEITHER the signing
        // key (`f11749...`, cross-checked with Python `hmac`) NOR the
        // signature under standard derivation — it shares a 48-hex prefix
        // with the documented signature below and looks mis-transcribed
        // (signing key vs final signature conflated). True documented values
        // are pinned instead; see the task report.
        let sk = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "20130524",
            "us-east-1",
            "s3",
        );
        let sts = "AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/aws4_request\n3bfa292879f6447bbcda7001decf97f4a54dc650c8942174ae0a9121cf58ad04";
        assert_eq!(
            hex::encode(hmac_sha256(&sk, sts.as_bytes())),
            "aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        );
    }

    #[test]
    fn host_rule_minio_uses_request_host_and_scheme() {
        // `plane/settings/storage.py:39-53`: USE_MINIO==1 → endpoint =
        // request Host (+ scheme, https iff MINIO_ENDPOINT_SSL==1).
        assert_eq!(
            s3_endpoint_for(true, false, "api:8000", "http", "http://plane-minio:9000"),
            "http://api:8000"
        );
        assert_eq!(
            s3_endpoint_for(true, true, "api:8000", "http", "http://plane-minio:9000"),
            "https://api:8000"
        );
        // USE_MINIO==0 → raw AWS_S3_ENDPOINT_URL verbatim.
        assert_eq!(
            s3_endpoint_for(false, false, "api:8000", "http", "https://s3.example.com"),
            "https://s3.example.com"
        );
    }

    #[test]
    fn disposition_forces_attachment_for_script_mime() {
        // `plane/app/views/asset/v2.py:523-526` + `settings/common.py:546-556`.
        assert_eq!(disposition_for("image/svg+xml"), "attachment");
        assert_eq!(disposition_for("text/html; charset=utf-8"), "attachment");
        assert_eq!(disposition_for("image/png"), "inline");
        assert_eq!(disposition_for("application/pdf"), "inline");
    }

    #[test]
    fn key_layout_matches_django() {
        // `v2.py:390` ws/project/issue vs `v2.py:146` user (no ws prefix).
        assert_eq!(ws_asset_key("wsid", "hex", "a.png"), "wsid/hex-a.png");
        assert_eq!(user_asset_key("hex", "a.png"), "hex-a.png");
    }

    #[test]
    fn sanitize_matches_path_validator() {
        // `plane/utils/path_validator.py:14-51`.
        assert_eq!(sanitize_filename(Some("../x.png")), Some("x.png".to_string()));
        assert_eq!(sanitize_filename(Some("  ")), None);
        assert_eq!(sanitize_filename(None), None);
    }

    #[test]
    fn mime_gates_match_django_allowlists() {
        // Issue attachments: full `ATTACHMENT_MIME_TYPES`
        // (`settings/common.py:453-541`); user/ws/project: images-only
        // (`v2.py:367-373`).
        assert!(is_attachment_mime("application/pdf"));
        assert!(is_attachment_mime("image/jpeg"));
        assert!(!is_attachment_mime("application/x-evil"));
        assert!(is_image_mime("image/jpeg"));
        assert!(!is_image_mime("application/pdf"));
    }

    #[test]
    fn issue_attachment_asset_url_matches_served_v2_route() {
        // `plane/db/models/asset.py:92-93`: ISSUE_ATTACHMENT →
        // `/api/assets/v2/workspaces/{slug}/projects/{project_id}/issues/{issue_id}/attachments/{id}/`.
        // This must equal a SERVED route (`main.rs` issue-attachment routes,
        // Django `app/urls/issue.py:137-146` `IssueAttachmentV2Endpoint`).
        let id = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let pid = uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let iid = uuid::Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let url = asset_url_for(
            Some("ISSUE_ATTACHMENT"),
            id,
            Some("ws-slug"),
            Some(pid),
            Some(iid),
        );
        let s = url.as_str().expect("asset_url is a string");
        assert!(s.starts_with("/api/assets/v2/workspaces/"), "v2 prefix: {s}");
        assert_eq!(
            s,
            format!("/api/assets/v2/workspaces/ws-slug/projects/{pid}/issues/{iid}/attachments/{id}/")
        );
    }

    #[test]
    fn draft_entity_binds_no_fk() {
        // Django `get_entity_id_field` (`v2.py:206-236` workspace presign,
        // `v2.py:782-813` duplicate) has NO DRAFT branch → `{}` → FK stays
        // NULL. `DRAFT_*` must therefore select no FK column.
        let eid = "44444444-4444-4444-4444-444444444444";
        let (col_a, _) = entity_fk_fragment("DRAFT_ISSUE_ATTACHMENT", Some(eid));
        let (col_d, _) = entity_fk_fragment("DRAFT_ISSUE_DESCRIPTION", Some(eid));
        assert_eq!(col_a, "", "DRAFT_ISSUE_ATTACHMENT binds no FK");
        assert_eq!(col_d, "", "DRAFT_ISSUE_DESCRIPTION binds no FK");
        // Regression pin: the non-DRAFT mapping is unchanged.
        let (col_i, id_i) = entity_fk_fragment("ISSUE_ATTACHMENT", Some(eid));
        assert_eq!(col_i, "issue_id");
        assert_eq!(id_i.unwrap().to_string(), eid);
    }

    /// Test-only SigV4-signed empty-body PUT (bucket creation for the live
    /// test below): `PUT {path}` with `host`, `x-amz-content-sha256`,
    /// `x-amz-date` signed headers.
    async fn s3_signed_put(
        conf: &S3Conf,
        endpoint: &str,
        path: &str,
    ) -> Result<reqwest::StatusCode, String> {
        let now = Utc::now();
        let (amzdate, datestamp) = amz_dates(now);
        let scope = format!("{datestamp}/{}/s3/aws4_request", conf.region);
        let host = endpoint_authority(endpoint).to_string();
        let payload_hash = sha256_hex(b"");
        let canon_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amzdate}\n");
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_req =
            format!("PUT\n{path}\n\n{canon_headers}\n{signed_headers}\n{payload_hash}");
        let sts = format!(
            "AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}",
            sha256_hex(canonical_req.as_bytes())
        );
        let signing = derive_signing_key(&conf.secret, &datestamp, &conf.region, "s3");
        let signature = hex::encode(hmac_sha256(&signing, sts.as_bytes()));
        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            conf.access, scope, signed_headers, signature
        );
        let url = format!("{}{path}", endpoint.trim_end_matches('/'));
        reqwest::Client::new()
            .put(&url)
            .header("host", &host)
            .header("x-amz-date", &amzdate)
            .header("x-amz-content-sha256", &payload_hash)
            .header("authorization", &auth)
            .body(vec![])
            .send()
            .await
            .map(|r| r.status())
            .map_err(|e| e.to_string())
    }

    /// Scoped process-env snapshot for the live-MinIO test below: restores
    /// every touched var on drop (including assertion panics), so the
    /// mutation never leaks into parallel unit tests.
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

    /// LIVE PROOF — MinIO round-trip through the real signing code.
    /// Gated on `E9_LIVE_MINIO_URL` (e.g. `http://172.27.0.5:9000`); returns
    /// immediately when unset (CI-safe). Uses the repo's MinIO credentials
    /// with the region deliberately EMPTY (production parity — Django passes
    /// `AWS_REGION=""` straight to boto). Mints a presigned POST via
    /// `presign_post`, uploads multipart FormData, then fetches the object
    /// back through `presign_get` and asserts byte equality.
    #[tokio::test]
    async fn live_minio_round_trip() {
        let endpoint = match std::env::var("E9_LIVE_MINIO_URL") {
            Ok(v) => v,
            Err(_) => return,
        };
        // Snapshot + restore every var this test mutates (Drop restores even
        // on assertion panic).
        let _env = EnvGuard::snapshot(&[
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_S3_BUCKET_NAME",
            "USE_MINIO",
            "AWS_S3_ENDPOINT_URL",
            "AWS_REGION",
            "MINIO_ENDPOINT_URL",
        ]);
        std::env::set_var("AWS_ACCESS_KEY_ID", "access-key");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "secret-key");
        std::env::set_var("AWS_S3_BUCKET_NAME", "uploads");
        std::env::set_var("USE_MINIO", "0");
        std::env::set_var("AWS_S3_ENDPOINT_URL", &endpoint);
        std::env::remove_var("AWS_REGION");
        std::env::remove_var("MINIO_ENDPOINT_URL");
        let conf = s3_conf();
        // `uploads` bucket (idempotent — 200 new, 409 exists).
        let st = s3_signed_put(&conf, &endpoint, "/uploads")
            .await
            .expect("bucket PUT transport");
        assert!(st.is_success() || st.as_u16() == 409, "bucket PUT: {st}");

        let key = format!("e9-live/{}-proof.txt", uuid::Uuid::new_v4().simple());
        let body_bytes = b"e9-live-proof-bytes".to_vec();
        let now = Utc::now();
        let post = presign_post(&conf, &endpoint, &key, "text/plain", 64, now);
        let url = post.get("url").and_then(Value::as_str).unwrap().to_string();
        let fields = post.get("fields").and_then(Value::as_object).unwrap().clone();
        // Hand-built multipart FormData (reqwest has no `multipart` feature
        // in this workspace — same bytes curl would send).
        let boundary = "e9liveboundary123";
        let mut form: Vec<u8> = Vec::new();
        for (k, v) in &fields {
            form.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{}\r\n",
                    v.as_str().unwrap()
                )
                .as_bytes(),
            );
        }
        form.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"proof.txt\"\r\nContent-Type: text/plain\r\n\r\n"
            )
            .as_bytes(),
        );
        form.extend_from_slice(&body_bytes);
        form.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let res = reqwest::Client::new()
            .post(&url)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(form)
            .send()
            .await
            .expect("presigned POST transport");
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        assert!(status.is_success(), "presigned POST: {status} {text}");

        let get_url = presign_get(&conf, &endpoint, &key, "attachment", Some("proof.txt"), Utc::now());
        let res = reqwest::Client::new()
            .get(&get_url)
            .send()
            .await
            .expect("presigned GET transport");
        assert_eq!(res.status(), 200, "presigned GET status");
        let back = res.bytes().await.expect("read body").to_vec();
        assert_eq!(back, body_bytes, "round-trip bytes");

        // Best-effort cleanup: DELETE the proof object so no `e9-live/`
        // objects remain in the bucket (errors ignored — bucket cleanliness
        // is verified out-of-band).
        let _ = s3_signed_delete(&conf, &endpoint, &format!("/uploads/{key}")).await;
    }

    /// Test-only SigV4-signed empty-body DELETE (proof-object cleanup for
    /// the live test above): `DELETE {path}` with `host`,
    /// `x-amz-content-sha256`, `x-amz-date` signed headers.
    async fn s3_signed_delete(
        conf: &S3Conf,
        endpoint: &str,
        path: &str,
    ) -> Result<reqwest::StatusCode, String> {
        let now = Utc::now();
        let (amzdate, datestamp) = amz_dates(now);
        let scope = format!("{datestamp}/{}/s3/aws4_request", conf.region);
        let host = endpoint_authority(endpoint).to_string();
        let payload_hash = sha256_hex(b"");
        let canon_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amzdate}\n");
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_req =
            format!("DELETE\n{path}\n\n{canon_headers}\n{signed_headers}\n{payload_hash}");
        let sts = format!(
            "AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}",
            sha256_hex(canonical_req.as_bytes())
        );
        let signing = derive_signing_key(&conf.secret, &datestamp, &conf.region, "s3");
        let signature = hex::encode(hmac_sha256(&signing, sts.as_bytes()));
        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            conf.access, scope, signed_headers, signature
        );
        let url = format!("{}{path}", endpoint.trim_end_matches('/'));
        reqwest::Client::new()
            .delete(&url)
            .header("host", &host)
            .header("x-amz-date", &amzdate)
            .header("x-amz-content-sha256", &payload_hash)
            .header("authorization", &auth)
            .send()
            .await
            .map(|r| r.status())
            .map_err(|e| e.to_string())
    }
}
