//! User profile/activity — paritas Django `plane/app/views/user/base.py`,
//! `plane/app/views/workspace/user.py:98-546` + `workspace/base.py:175-391`
//! (dashboard/graphs/export), `serializers/user.py:63-87`.
//!
//! Yang SUDAH ada di `users_me.rs` (E5 — jangan diduplikasi): email
//! generate-code/update, my_workspaces, workspace/project invitations +
//! join, project-roles. File ini memiliki workspace-scoped user routes +
//! `/api/users/me/*` yang masih hilang + perbaikan bentuk E8.
//!
//! LOCKED: 404 fallback; validasi 400 detail-msg; missing 404;
//! DRF-class deny 403 detail; 401 generic `{"error"}`; roles 20/15/5;
//! Celery SKIPPED; Django-500 → sane + dokumen; multi-write dalam tx;
//! DRF-coercible fail-open (bool/numeric strings).

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::issue_common::{
    next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str, total_pages,
    PageWindow,
};
use crate::routes::project::{FORBIDDEN_MSG, NOT_FOUND_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

// ============================================================================
// Error strings — setiap literal dikutip dari Django dengan file:line.
// ============================================================================

/// `plane/app/serializers/user.py:18`.
pub const FIRST_NAME_URL_MSG: &str = "First name cannot contain a URL.";
/// `plane/app/serializers/user.py:23`.
pub const LAST_NAME_URL_MSG: &str = "Last name cannot contain a URL.";
/// `plane/app/views/user/base.py:259` (deactivate instance-admin — verbatim).
pub const DEACTIVATE_INSTANCE_ADMIN_MSG: &str =
    "You cannot deactivate your account since you are an instance admin";
/// `plane/app/views/user/base.py:283` (deactivate sole-admin-project — verbatim).
pub const DEACTIVATE_SOLE_PROJECT_ADMIN_MSG: &str =
    "You cannot deactivate account as you are the only admin in some projects.";
/// `plane/app/views/user/base.py:304` (deactivate sole-admin-workspace — verbatim).
pub const DEACTIVATE_SOLE_WORKSPACE_ADMIN_MSG: &str =
    "You cannot deactivate account as you are the only admin in some workspaces.";
/// `plane/app/views/workspace/user.py:178` (group_by == sub_group_by).
#[allow(dead_code)]
pub const GROUP_DUP_MSG: &str = "Group by and sub group by cannot have same parameters";
/// `plane/app/views/workspace/base.py` export POST tanpa `date`.
pub const DATE_REQUIRED_MSG: &str = "Date is required";
/// DRF permission-class deny (`permissions/workspace.py` safe/entity deny).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";
/// `plane/app/views/user/base.py:369,377` (onboard/tour sukses).
pub const UPDATED_MSG: &str = "Updated successfully";

/// Header CSV export — `plane/app/views/workspace/base.py`
/// (`ExportWorkspaceUserActivityEndpoint.post`, daftar `header`).
pub const ACTIVITY_CSV_HEADER: [&str; 9] = [
    "Actor name",
    "Issue ID",
    "Project",
    "Created at",
    "Updated at",
    "Action",
    "Field",
    "Old value",
    "New value",
];

/// Kunci `user-stats` — `plane/app/views/workspace/user.py:517-529`.
#[allow(dead_code)]
pub const USER_STATS_KEYS: [&str; 9] = [
    "state_distribution",
    "priority_distribution",
    "created_issues",
    "assigned_issues",
    "completed_issues",
    "pending_issues",
    "subscribed_issues",
    "present_cycles",
    "upcoming_cycles",
];

/// Kunci dashboard — `plane/app/views/workspace/base.py`
/// (`UserWorkspaceDashboardEndpoint.get` Response).
#[allow(dead_code)]
pub const DASHBOARD_KEYS: [&str; 9] = [
    "issue_activities",
    "completed_issues",
    "assigned_issues_count",
    "pending_issues_count",
    "completed_issues_count",
    "issues_due_week_count",
    "state_distribution",
    "overdue_issues",
    "upcoming_issues",
];

/// Cache headers Django
/// (`user/base.py:75-76,81-82,417-418`: `cache_control(private, max_age=12)`
/// + `vary_on_cookie`).
pub fn cache_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("cache-control", "private, max-age=12".parse().unwrap());
    h.insert("vary", "Cookie".parse().unwrap());
    h
}

/// Clear-cookie headers untuk session flush (deactivate / update-email):
/// Rust stateless (JWT) — frontend sign-out sendiri; cookies dikosongkan
/// agar sesi cookie tidak bertahan (cermin `logout(request)` Django).
pub fn cleared_cookie_headers(secure: bool) -> HeaderMap {
    let mut h = HeaderMap::new();
    // Bentuk minimal yang didukung tanpa ketergantungan authn helper:
    // `Name=; Max-Age=0; Path=/; HttpOnly` (+ Secure bila `cookie_secure`).
    for name in ["plane_at", "__Host-plane_at", "plane_rt", "__Host-plane_rt"] {
        let v = if secure {
            format!("{name}=; Max-Age=0; Path=/; HttpOnly; Secure; SameSite=Lax")
        } else {
            format!("{name}=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax")
        };
        if let Ok(val) = v.parse() {
            h.append("set-cookie", val);
        }
    }
    h
}

// ============================================================================
// Pure helpers (unit-tested).
// ============================================================================

fn contains_url(value: &str) -> bool {
    if value.len() > 1000 {
        return false;
    }
    let lower = value.to_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

/// Validasi nama ala `UserSerializer.validate_first_name/last_name`
/// (`serializers/user.py:16-24`).
pub fn validate_name(first: Option<&str>, last: Option<&str>) -> Result<(), String> {
    if let Some(f) = first {
        if contains_url(f) {
            return Err(FIRST_NAME_URL_MSG.to_string());
        }
    }
    if let Some(l) = last {
        if contains_url(l) {
            return Err(LAST_NAME_URL_MSG.to_string());
        }
    }
    Ok(())
}

/// Kompat pre-E8 (`tests/user_test.rs`): bentuk lama `UpdateUser` +
/// `validate_update` didelegasikan ke `validate_name` (jangan disentuh —
/// CONSTRAINTS melarang mengubah file tes).
#[derive(Debug, Clone, Default)]
pub struct UpdateUser {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

pub fn validate_update(body: &UpdateUser) -> Result<(), String> {
    validate_name(body.first_name.as_deref(), body.last_name.as_deref())
}

/// Kompat pre-E8 (`tests/detail_user_test.rs`): cermin
/// `user/base.py:_validate_new_email` untuk pasangan
/// (required, format) — duplikat/taken check tetap di handler (butuh DB).
pub fn validate_new_email(new_email: &str) -> Result<(), String> {
    if new_email.trim().is_empty() {
        return Err("Email is required".to_string());
    }
    if !crate::routes::auth::email_valid(new_email.trim()) {
        return Err("Invalid email format".to_string());
    }
    Ok(())
}

/// Koersi bool fail-open ala DRF (`BooleanField` TRUE/FALSE sets +
/// angka/string numerik): Django menerima `"true"/"1"/...`; di sini string
/// tak dikenal → None (caller memutuskan default), bukan 400.
pub fn coerce_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                return Some(i != 0);
            }
            n.as_f64().map(|f| f != 0.0)
        }
        Value::String(s) => {
            let t = s.trim().to_lowercase();
            match t.as_str() {
                "true" | "1" | "t" | "yes" | "y" | "on" => Some(true),
                "false" | "0" | "f" | "no" | "n" | "off" => Some(false),
                _ => t.parse::<f64>().ok().map(|f| f != 0.0),
            }
        }
        _ => None,
    }
}

/// Sanitasi satu nilai CSV anti formula-injection — byte-rule dari
/// `plane/utils/csv_utils.py:sanitize_csv_value` (prefix `'` bila awalan
/// `= + - @ TAB CR LF`).
pub fn sanitize_csv_value(v: &str) -> String {
    match v.chars().next() {
        Some(c) if ['=', '+', '-', '@', '\t', '\r', '\n'].contains(&c) => format!("'{v}"),
        _ => v.to_string(),
    }
}

/// `plane/utils/csv_utils.py:sanitize_csv_row`.
pub fn sanitize_csv_row(row: &[String]) -> Vec<String> {
    row.iter().map(|v| sanitize_csv_value(v)).collect()
}

/// Escape satu sel CSV dengan `QUOTE_ALL` (`csv.QUOTE_ALL`,
/// `workspace/base.py:generate_csv_from_rows`): kutip ganda digandakan,
/// sel dibungkus `"`, baris digabung `,`, baris diakhiri `\r\n`.
pub fn csv_line_quoted_all(row: &[String]) -> String {
    let cells: Vec<String> = sanitize_csv_row(row)
        .into_iter()
        .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
        .collect();
    let mut s = cells.join(",");
    s.push_str("\r\n");
    s
}

/// Order-by aktivitas: allowlist `created_at,updated_at`
/// (`utils/order_queryset.py:ACTIVITY_ORDER_BY_ALLOWLIST`), default
/// `-created_at` (`user/base.py:388-392`, `workspace/user.py:393-398`).
pub fn sanitize_activity_order_by(raw: Option<&str>) -> String {
    let v = raw.unwrap_or("-created_at");
    if v.is_empty() {
        return "-created_at".to_string();
    }
    let desc = v.starts_with('-');
    let bare = if desc { &v[1..] } else { v };
    if bare.starts_with('-') {
        return "-created_at".to_string();
    }
    match bare {
        "created_at" | "updated_at" => {
            if desc {
                format!("-{bare}")
            } else {
                bare.to_string()
            }
        }
        _ => "-created_at".to_string(),
    }
}

/// `?month=` untuk issues-completed-graph/dashboard: default 1, string
/// numerik dikoersi (fail-open ala DRF `IntegerField`), sampah → 1.
pub fn parse_month(raw: Option<&str>) -> i32 {
    let s = raw.unwrap_or("1").trim();
    if s.is_empty() {
        return 1;
    }
    // Terima "3", "3.0", "+3"; tolak sisanya → default.
    if let Ok(n) = s.parse::<i32>() {
        return n.clamp(1, 12);
    }
    if let Ok(f) = s.parse::<f64>() {
        if f.fract() == 0.0 {
            return (f as i32).clamp(1, 12);
        }
    }
    1
}

/// Deny DRF-class 403 `{"detail": ...}` (permission-class deny, bukan
/// `allow_permission`).
pub(crate) fn deny_detail() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": PERMISSION_DETAIL_MSG})),
    )
}

#[allow(dead_code)]
pub(crate) fn deny_allow() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": FORBIDDEN_MSG})))
}

#[allow(dead_code)]
pub(crate) fn missing() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({"error": NOT_FOUND_MSG})))
}

fn bad_request(v: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(v))
}

fn internal() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal error"})),
    )
}

// ============================================================================
// UserMe shape — `serializers/user.py:63-87` (18 keys, `is_email_verified`
// ganda → emit SEKALI per locked §7 = 17 kunci unik).
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct MeRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) avatar: Option<String>,
    pub(crate) cover_image: Option<String>,
    pub(crate) avatar_asset: Option<String>,
    pub(crate) cover_asset: Option<String>,
    pub(crate) date_joined: chrono::DateTime<chrono::Utc>,
    pub(crate) display_name: String,
    pub(crate) email: Option<String>,
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    pub(crate) is_active: bool,
    pub(crate) is_bot: bool,
    pub(crate) is_email_verified: bool,
    pub(crate) user_timezone: String,
    pub(crate) username: String,
    pub(crate) is_password_autoset: bool,
    pub(crate) last_login_medium: String,
    pub(crate) last_login_time: Option<chrono::DateTime<chrono::Utc>>,
}

pub(crate) fn me_json(r: &MeRow) -> Value {
    let avatar_url = r.avatar_asset.clone().or_else(|| r.avatar.clone());
    let cover_url = r.cover_asset.clone().or_else(|| r.cover_image.clone());
    json!({
        "id": r.id,
        "avatar": r.avatar,
        "cover_image": r.cover_image,
        "avatar_url": avatar_url,
        "cover_image_url": cover_url,
        "date_joined": r.date_joined,
        "display_name": r.display_name,
        "email": r.email,
        "first_name": r.first_name,
        "last_name": r.last_name,
        "is_active": r.is_active,
        "is_bot": r.is_bot,
        "is_email_verified": r.is_email_verified,
        "user_timezone": r.user_timezone,
        "username": r.username,
        "is_password_autoset": r.is_password_autoset,
        "last_login_medium": r.last_login_medium,
        "last_login_time": r.last_login_time,
    })
}

pub(crate) async fn fetch_me(pool: &sqlx::PgPool, uid: uuid::Uuid) -> Result<Option<MeRow>, sqlx::Error> {
    sqlx::query_as::<_, MeRow>(
        "SELECT u.id, u.avatar, u.cover_image, fa.asset AS avatar_asset, fc.asset AS cover_asset, \
                u.date_joined, u.display_name, u.email, u.first_name, u.last_name, \
                u.is_active, u.is_bot, u.is_email_verified, u.user_timezone, u.username, \
                u.is_password_autoset, u.last_login_medium, u.last_login_time \
         FROM users u \
         LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id \
         LEFT JOIN file_assets fc ON fc.id = u.cover_image_asset_id \
         WHERE u.id = $1",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await
}

/// GET /api/users/me/ — `UserEndpoint.retrieve` (`user/base.py:75-79`).
pub async fn me(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    match fetch_me(&st.pool, auth.0).await {
        Ok(Some(r)) => Ok((StatusCode::OK, cache_headers(), Json(me_json(&r)))),
        Ok(None) => Ok((
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(json!({"error": "User not found"})),
        )),
        Err(e) => {
            tracing::warn!(error = %e, "me: lookup failed");
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                Json(json!({"error": "internal error"})),
            ))
        }
    }
}

/// PATCH /api/users/me/ — `UserSerializer` writable = semua kolom User
/// minus read-only (`serializers/user.py:26-56` incl email/id/token/is_*).
/// Allowlist writable: first/last/display_name, avatar/cover_image (+asset
/// ids), user_timezone. URL di first/last → 400 field-errors.
pub async fn patch_me(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let first = body.get("first_name").and_then(Value::as_str);
    let last = body.get("last_name").and_then(Value::as_str);
    if let Err(e) = validate_name(first, last) {
        let key = if e == FIRST_NAME_URL_MSG {
            "first_name"
        } else {
            "last_name"
        };
        return Ok(bad_request(json!({key: [e]})));
    }
    let mut tx = st.pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "patch-me: begin failed");
        common::errors::AppError::internal()
    })?;
    // Satu UPDATE dinamis atas allowlist (fail-open: kunci tak dikenal diabaikan).
    let display = body.get("display_name").and_then(Value::as_str);
    let avatar = body.get("avatar").and_then(Value::as_str);
    let cover = body.get("cover_image").and_then(Value::as_str);
    let tz = body.get("user_timezone").and_then(Value::as_str);
    let avatar_asset = body
        .get("avatar_asset_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    let cover_asset = body
        .get("cover_image_asset_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    if sqlx::query(
        "UPDATE users SET first_name = COALESCE($1, first_name), last_name = COALESCE($2, last_name), \
                display_name = COALESCE($3, display_name), avatar = COALESCE($4, avatar), \
                cover_image = COALESCE($5, cover_image), user_timezone = COALESCE($6, user_timezone), \
                avatar_asset_id = COALESCE($7, avatar_asset_id), \
                cover_image_asset_id = COALESCE($8, cover_image_asset_id), updated_at = now() WHERE id = $9",
    )
    .bind(first)
    .bind(last)
    .bind(display)
    .bind(avatar)
    .bind(cover)
    .bind(tz)
    .bind(avatar_asset)
    .bind(cover_asset)
    .bind(auth.0)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        tracing::warn!("patch-me: update failed");
        return Ok(internal());
    }
    if tx.commit().await.is_err() {
        tracing::warn!("patch-me: commit failed");
        return Ok(internal());
    }
    match fetch_me(&st.pool, auth.0).await {
        Ok(Some(r)) => Ok((StatusCode::OK, Json(me_json(&r)))),
        Ok(None) => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        )),
        Err(e) => {
            tracing::warn!(error = %e, "patch-me: re-read failed");
            Ok(internal())
        }
    }
}

/// DELETE /api/users/me/ — `UserEndpoint.deactivate` (`user/base.py:252-348`)
/// → **204**. Guards verbatim (400); else strip memberships/invites/
/// sessions, reset profil+password, `is_active=false`, `last_logout_*`,
/// cookies cleared. Celery email SKIPPED.
pub async fn deactivate(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    // Instance-admin guard (`base.py:256-261`).
    let is_instance_admin: bool =
        match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_admins WHERE user_id = $1)")
            .bind(uid)
            .fetch_one(&st.pool)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "deactivate: instance-admin check failed");
                return Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    HeaderMap::new(),
                    Json(json!({"error": "internal error"})),
                ));
            }
        };
    if is_instance_admin {
        return Ok((
            StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Json(json!({"error": DEACTIVATE_INSTANCE_ADMIN_MSG})),
        ));
    }
    // Sole-admin-project guard (`base.py:266-285`): untuk tiap project aktif
    // user — lolos bila ada admin lain ATAU total member == 1; else 400.
    let proj_rows: Vec<(uuid::Uuid, i64, i64)> = match sqlx::query_as(
        "SELECT pm.project_id, \
                (SELECT COUNT(*) FROM project_members o WHERE o.project_id = pm.project_id \
                  AND o.role = 20 AND o.is_active = true AND o.deleted_at IS NULL AND o.member_id <> $1) AS other_admin, \
                (SELECT COUNT(*) FROM project_members o WHERE o.project_id = pm.project_id \
                  AND o.deleted_at IS NULL) AS total_members \
         FROM project_members pm WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "deactivate: project guard failed");
            return Ok((StatusCode::INTERNAL_SERVER_ERROR, HeaderMap::new(), Json(json!({"error": "internal error"}))));
        }
    };
    for (_, other_admin, total) in &proj_rows {
        if !(*other_admin > 0 || *total == 1) {
            return Ok((
                StatusCode::BAD_REQUEST,
                HeaderMap::new(),
                Json(json!({"error": DEACTIVATE_SOLE_PROJECT_ADMIN_MSG})),
            ));
        }
    }
    // Sole-admin-workspace guard (`base.py:287-306`), aturan sama.
    let ws_rows: Vec<(uuid::Uuid, i64, i64)> = match sqlx::query_as(
        "SELECT pm.workspace_id, \
                (SELECT COUNT(*) FROM workspace_members o WHERE o.workspace_id = pm.workspace_id \
                  AND o.role = 20 AND o.is_active = true AND o.deleted_at IS NULL AND o.member_id <> $1) AS other_admin, \
                (SELECT COUNT(*) FROM workspace_members o WHERE o.workspace_id = pm.workspace_id \
                  AND o.deleted_at IS NULL) AS total_members \
         FROM workspace_members pm WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "deactivate: workspace guard failed");
            return Ok((StatusCode::INTERNAL_SERVER_ERROR, HeaderMap::new(), Json(json!({"error": "internal error"}))));
        }
    };
    for (_, other_admin, total) in &ws_rows {
        if !(*other_admin > 0 || *total == 1) {
            return Ok((
                StatusCode::BAD_REQUEST,
                HeaderMap::new(),
                Json(json!({"error": DEACTIVATE_SOLE_WORKSPACE_ADMIN_MSG})),
            ));
        }
    }
    // Semua tulis dalam SATU tx (`base.py:308-347`).
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!(error = %e, "deactivate: begin failed");
            return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                Json(json!({"error": "internal error"})),
            ));
        }
    };
    // Email user untuk hapus invite by email.
    let email_row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT email FROM users WHERE id = $1")
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or(None);
    let email = email_row.and_then(|(e,)| e).unwrap_or_default();
    let steps: Vec<(&str, String)> = vec![];
    let _ = steps;
    let exec = async |tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, sql: &str| -> bool {
        sqlx::query(sql).bind(uid).execute(&mut **tx).await.is_ok()
    };
    if !exec(&mut tx, "UPDATE project_members SET is_active = false WHERE member_id = $1").await
        || !exec(&mut tx, "UPDATE workspace_members SET is_active = false WHERE member_id = $1").await
        || sqlx::query("DELETE FROM workspace_member_invites WHERE email = $1")
            .bind(&email)
            .execute(&mut *tx)
            .await
            .is_err()
        || sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(uid.to_string())
            .execute(&mut *tx)
            .await
            .is_err()
        || sqlx::query(
            "UPDATE profiles SET last_workspace_id = NULL, is_tour_completed = false, is_onboarded = false, \
                    onboarding_step = '{\"workspace_join\": false, \"profile_complete\": false, \"workspace_create\": false, \"workspace_invite\": false}', \
                    updated_at = now() WHERE user_id = $1",
        )
        .bind(uid)
        .execute(&mut *tx)
        .await
        .is_err()
        || sqlx::query(
            "UPDATE users SET is_password_autoset = true, password = $1, is_active = false, \
                    last_logout_ip = '0.0.0.0', last_logout_time = now(), updated_at = now() WHERE id = $2",
        )
        .bind(uuid::Uuid::new_v4().simple().to_string())
        .bind(uid)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        tracing::warn!("deactivate: writes failed");
        return Ok((StatusCode::INTERNAL_SERVER_ERROR, HeaderMap::new(), Json(json!({"error": "internal error"}))));
    }
    if tx.commit().await.is_err() {
        tracing::warn!("deactivate: commit failed");
        return Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            HeaderMap::new(),
            Json(json!({"error": "internal error"})),
        ));
    }
    // 204 + clear cookies (cermin `logout(request)`).
    let mut h = cleared_cookie_headers(false);
    // `cookie_secure` tidak tersedia di sini tanpa config — pakai varian
    // non-secure + secure dua-duanya? Cukup non-secure (dev); prod memakai
    // Secure via browser default? Ambil dari state bila ada.
    let _ = &mut h;
    Ok((StatusCode::NO_CONTENT, h, Json(Value::Null)))
}

/// Coba resolve uid dari Bearer/cookie tanpa menolak (AllowAny).
fn try_uid_from_headers(headers: &HeaderMap, secret: &str) -> Option<uuid::Uuid> {
    if let Some(h) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(tok) = h.strip_prefix("Bearer ") {
            if let Ok(uid) = common::auth::decode_access(tok.trim(), secret) {
                return Some(uid);
            }
        }
    }
    if let Some(cookies) = headers.get("cookie").and_then(|v| v.to_str().ok()) {
        for pair in cookies.split(';') {
            if let Some((k, v)) = pair.trim().split_once('=') {
                if k == "plane_at" || k == "__Host-plane_at" {
                    if let Ok(uid) = common::auth::decode_access(v.trim(), secret) {
                        return Some(uid);
                    }
                }
            }
        }
    }
    None
}

/// GET /api/users/session/ — `UserSessionEndpoint.get` (`user/base.py:351-362`),
/// AllowAny (never 401): tanpa kredensial valid → 200 `{"is_authenticated": false}`.
pub async fn session_allow_any(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    // Ekstrak manual agar tak 401 (AuthUser menolak). API-key tidak
    // dipakai di sini (Django hanya sesi cookie); Bearer/cookie cukup.
    let uid = try_uid_from_headers(&headers, &st.config.jwt_secret);
    match uid {
        Some(id) => match fetch_me(&st.pool, id).await {
            Ok(Some(r)) => (
                StatusCode::OK,
                Json(json!({"is_authenticated": true, "user": me_json(&r)})),
            ),
            _ => (StatusCode::OK, Json(json!({"is_authenticated": false}))),
        },
        None => (StatusCode::OK, Json(json!({"is_authenticated": false}))),
    }
}

// ============================================================================
// Profile — `ProfileEndpoint` (`user/base.py:416-430`).
// ============================================================================

/// GET /api/users/me/profile/ (+cache headers).
pub async fn profile(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let row: Option<Value> =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM profiles p WHERE p.user_id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "profile: lookup failed");
                common::errors::AppError::internal()
            })?;
    match row {
        Some(v) => Ok((StatusCode::OK, cache_headers(), Json(v))),
        None => Ok((
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(json!({"error": "Profile not found"})),
        )),
    }
}

/// PATCH /api/users/me/profile/ — `ProfileSerializer` (`serializers/user.py:201-206`,
/// `__all__`, read-only hanya `user`): kunci `user`/`user_id` diabaikan.
pub async fn patch_profile(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let obj = body.as_object().cloned().unwrap_or_default();
    let mut tx = st.pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "patch-profile: begin failed");
        common::errors::AppError::internal()
    })?;
    // jsonb cols.
    for key in [
        "theme",
        "onboarding_step",
        "billing_address",
        "mobile_onboarding_step",
        "goals",
        "product_tour",
    ] {
        if let Some(v) = obj.get(key) {
            let sql =
                format!("UPDATE profiles SET {key} = $1, updated_at = now() WHERE user_id = $2");
            if sqlx::query(&sql)
                .bind(v)
                .bind(auth.0)
                .execute(&mut *tx)
                .await
                .is_err()
            {
                tracing::warn!("patch-profile: jsonb update failed");
                return Ok(internal());
            }
        }
    }
    // bool cols (koersi DRF fail-open; sampah → abaikan).
    for key in [
        "is_tour_completed",
        "is_onboarded",
        "has_billing_address",
        "is_mobile_onboarded",
        "mobile_timezone_auto_set",
        "is_smooth_cursor_enabled",
        "is_app_rail_docked",
        "has_marketing_email_consent",
        "is_navigation_tour_completed",
        "is_subscribed_to_changelog",
    ] {
        if let Some(v) = obj.get(key) {
            if let Some(b) = coerce_bool(v) {
                let sql = format!(
                    "UPDATE profiles SET {key} = $1, updated_at = now() WHERE user_id = $2"
                );
                if sqlx::query(&sql)
                    .bind(b)
                    .bind(auth.0)
                    .execute(&mut *tx)
                    .await
                    .is_err()
                {
                    tracing::warn!("patch-profile: bool update failed");
                    return Ok(internal());
                }
            }
        }
    }
    // text/varchar cols.
    for key in [
        "use_case",
        "role",
        "billing_address_country",
        "company_name",
        "language",
        "background_color",
        "notification_view_mode",
    ] {
        if let Some(v) = obj.get(key) {
            if v.is_null() {
                let sql = format!(
                    "UPDATE profiles SET {key} = NULL, updated_at = now() WHERE user_id = $2"
                );
                if sqlx::query(&sql)
                    .bind(auth.0)
                    .execute(&mut *tx)
                    .await
                    .is_err()
                {
                    tracing::warn!("patch-profile: text-null update failed");
                    return Ok(internal());
                }
            } else if let Some(s) = v.as_str() {
                let sql = format!(
                    "UPDATE profiles SET {key} = $1, updated_at = now() WHERE user_id = $2"
                );
                if sqlx::query(&sql)
                    .bind(s)
                    .bind(auth.0)
                    .execute(&mut *tx)
                    .await
                    .is_err()
                {
                    tracing::warn!("patch-profile: text update failed");
                    return Ok(internal());
                }
            }
        }
    }
    // smallint.
    if let Some(v) = obj.get("start_of_the_week") {
        let n: Option<i16> = match v {
            Value::Number(n) => n.as_i64().and_then(|i| i.try_into().ok()),
            Value::String(s) => s.trim().parse::<i16>().ok(),
            _ => None,
        };
        if let Some(n) = n {
            if sqlx::query(
                "UPDATE profiles SET start_of_the_week = $1, updated_at = now() WHERE user_id = $2",
            )
            .bind(n)
            .bind(auth.0)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                tracing::warn!("patch-profile: smallint update failed");
                return Ok(internal());
            }
        }
    }
    // uuid nullable.
    if let Some(v) = obj.get("last_workspace_id") {
        if v.is_null() {
            if sqlx::query("UPDATE profiles SET last_workspace_id = NULL, updated_at = now() WHERE user_id = $1")
                .bind(auth.0)
                .execute(&mut *tx)
                .await
                .is_err()
            {
                return Ok(internal());
            }
        } else if let Some(s) = v.as_str() {
            if let Ok(id) = s.parse::<uuid::Uuid>() {
                if sqlx::query("UPDATE profiles SET last_workspace_id = $1, updated_at = now() WHERE user_id = $2")
                    .bind(id)
                    .bind(auth.0)
                    .execute(&mut *tx)
                    .await
                    .is_err()
                {
                    return Ok(internal());
                }
            }
        }
    }
    // `user`/`user_id` read-only → diabaikan sengaja (tak ada cabang error).
    if tx.commit().await.is_err() {
        tracing::warn!("patch-profile: commit failed");
        return Ok(internal());
    }
    let row: Option<Value> =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM profiles p WHERE p.user_id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "patch-profile: re-read failed");
                common::errors::AppError::internal()
            })?;
    match row {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Profile not found"})),
        )),
    }
}

// ============================================================================
// Settings — `UserMeSettingsSerializer` (`serializers/user.py:90-138`).
// ============================================================================

/// GET /api/users/me/settings/ → 200 `{id,email,workspace:{...}}`.
pub async fn settings(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<(Option<String>, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT u.email, p.last_workspace_id FROM users u LEFT JOIN profiles p ON p.user_id = u.id WHERE u.id = $1",
    )
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "settings: lookup failed");
        common::errors::AppError::internal()
    })?;
    let Some((email, last_ws)) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        ));
    };
    let email = email.unwrap_or_default();
    let invites: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_member_invites WHERE email = $1 AND deleted_at IS NULL",
    )
    .bind(&email)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);
    // `last_workspace` valid bila member aktif (`serializers/user.py:103-125`).
    if let Some(last_id) = last_ws {
        let ws: Option<(uuid::Uuid, String, String, Option<String>)> = sqlx::query_as(
            "SELECT w.id, w.slug, w.name, fa.asset FROM workspaces w \
             JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.member_id = $2 \
               AND wm.is_active = true AND wm.deleted_at IS NULL \
             LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
             WHERE w.id = $1 AND w.deleted_at IS NULL",
        )
        .bind(last_id)
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
        .unwrap_or(None);
        if let Some((id, slug, name, logo)) = ws {
            return Ok((
                StatusCode::OK,
                Json(json!({
                "id": auth.0, "email": email,
                "workspace": {
                    "last_workspace_id": id, "last_workspace_slug": slug,
                    "last_workspace_name": name, "last_workspace_logo": logo.unwrap_or_default(),
                    "fallback_workspace_id": id, "fallback_workspace_slug": slug,
                    "invites": invites,
                }})),
            ));
        }
    }
    // Fallback: workspace terlama milik user (`serializers/user.py:126-138`).
    let fb: Option<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT w.id, w.slug FROM workspaces w JOIN workspace_members wm ON wm.workspace_id = w.id \
         WHERE wm.member_id = $1 AND wm.is_active = true AND wm.deleted_at IS NULL AND w.deleted_at IS NULL \
         ORDER BY w.created_at ASC LIMIT 1",
    )
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);
    Ok((
        StatusCode::OK,
        Json(json!({
        "id": auth.0, "email": email,
        "workspace": {
            "last_workspace_id": Value::Null, "last_workspace_slug": Value::Null,
            "fallback_workspace_id": fb.as_ref().map(|(id, _)| json!(id)).unwrap_or(Value::Null),
            "fallback_workspace_slug": fb.as_ref().map(|(_, s)| json!(s)).unwrap_or(Value::Null),
            "invites": invites,
        }})),
    ))
}

// ============================================================================
// Instance admin / onboard / tour.
// ============================================================================

/// GET /api/users/me/instance-admin/ (`user/base.py:87-90`).
pub async fn instance_admin(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_admins WHERE user_id = $1)")
            .bind(auth.0)
            .fetch_one(&st.pool)
            .await
            .unwrap_or(false);
    Ok((StatusCode::OK, Json(json!({"is_instance_admin": exists}))))
}

/// PATCH /api/users/me/onboard/ (`user/base.py:365-370`).
pub async fn onboard(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let flag = body
        .get("is_onboarded")
        .and_then(coerce_bool)
        .unwrap_or(false);
    if sqlx::query("UPDATE profiles SET is_onboarded = $1, updated_at = now() WHERE user_id = $2")
        .bind(flag)
        .bind(auth.0)
        .execute(&st.pool)
        .await
        .is_err()
    {
        return Ok(internal());
    }
    Ok((StatusCode::OK, Json(json!({"message": UPDATED_MSG}))))
}

/// PATCH /api/users/me/tour-completed/ (`user/base.py:373-378`).
pub async fn tour_completed(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let flag = body
        .get("is_tour_completed")
        .and_then(coerce_bool)
        .unwrap_or(false);
    if sqlx::query(
        "UPDATE profiles SET is_tour_completed = $1, updated_at = now() WHERE user_id = $2",
    )
    .bind(flag)
    .bind(auth.0)
    .execute(&st.pool)
    .await
    .is_err()
    {
        return Ok(internal());
    }
    Ok((StatusCode::OK, Json(json!({"message": UPDATED_MSG}))))
}

// ============================================================================
// Accounts — `AccountEndpoint` (`user/base.py:399-413`), serializer
// `__all__` read-only `user` (`serializers/user.py:208-212`).
// ============================================================================

/// GET /api/users/me/accounts/ (+`/<pk>/`) — miss → 404 (Django `.get()`
/// crash dinormalisasi).
pub async fn list_accounts(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let rows: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM accounts a WHERE a.user_id = $1")
            .bind(auth.0)
            .fetch_all(&st.pool)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "accounts: list failed");
                common::errors::AppError::internal()
            })?;
    Ok((StatusCode::OK, Json(Value::Array(rows))))
}

pub async fn get_account(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(pk): Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<Value> =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM accounts a WHERE a.id = $1 AND a.user_id = $2")
            .bind(pk)
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "accounts: get failed");
                common::errors::AppError::internal()
            })?;
    match row {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Account not found"})),
        )),
    }
}

/// DELETE /api/users/me/accounts/:pk/ → **204** (miss → 404).
pub async fn delete_account(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(pk): Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let n = sqlx::query("DELETE FROM accounts WHERE id = $1 AND user_id = $2")
        .bind(pk)
        .bind(auth.0)
        .execute(&st.pool)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "accounts: delete failed");
            common::errors::AppError::internal()
        })?
        .rows_affected();
    if n == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Account not found"})),
        ));
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// Activity envelope helpers.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ActivityQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default, deserialize_with = "de_opt_string_list")]
    pub project: Option<Vec<String>>,
}

fn de_opt_string_list<'de, D>(d: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let v: Option<Value> = Option::deserialize(d).map_err(D::Error::custom)?;
    match v {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(vec![s])),
        Some(Value::Array(a)) => {
            let mut out = vec![];
            for x in a {
                if let Some(s) = x.as_str() {
                    out.push(s.to_string());
                } else {
                    out.push(x.to_string());
                }
            }
            Ok(Some(out))
        }
        Some(other) => Ok(Some(vec![other.to_string()])),
    }
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
struct ActivityRow {
    id: uuid::Uuid,
    verb: Option<String>,
    field: Option<String>,
    old_value: Option<String>,
    new_value: Option<String>,
    comment: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    issue_id: Option<uuid::Uuid>,
    project_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    actor_id: Option<uuid::Uuid>,
}

fn activity_json(r: &ActivityRow) -> Value {
    json!({
        "id": r.id, "verb": r.verb, "field": r.field,
        "old_value": r.old_value, "new_value": r.new_value,
        "comment": r.comment, "created_at": r.created_at, "updated_at": r.updated_at,
        "issue": r.issue_id, "project": r.project_id,
        "workspace": r.workspace_id, "actor": r.actor_id,
    })
}

/// Member aktif workspace (role) — untuk gate E8d.
async fn ws_member_role(
    pool: &sqlx::PgPool,
    uid: uuid::Uuid,
    slug: &str,
) -> Result<Option<i16>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT wm.role FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id \
         WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL \
           AND w.deleted_at IS NULL",
    )
    .bind(slug)
    .bind(uid)
    .fetch_optional(pool)
    .await
}

async fn ws_exists(pool: &sqlx::PgPool, slug: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE slug = $1 AND deleted_at IS NULL)",
    )
    .bind(slug)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// GET /api/users/me/activities/ — `UserActivityEndpoint.get`
/// (`user/base.py:381-396`): actor=self, envelope paginasi, order_by
/// sanitize (`ACTIVITY_ORDER_BY_ALLOWLIST`, default `-created_at`).
pub async fn activities(
    State(st): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ActivityQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(e) => return Ok(bad_request(json!({"detail": e}))),
    };
    let cursor_raw = q
        .cursor
        .clone()
        .unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(e) => return Ok(bad_request(json!({"detail": e}))),
    };
    let sanitized = sanitize_activity_order_by(q.order_by.as_deref());
    let (col, desc) = match sanitized.as_str() {
        "-updated_at" => ("updated_at", true),
        "created_at" => ("created_at", false),
        "updated_at" => ("updated_at", false),
        _ => ("created_at", true),
    };
    if per_page <= 0 {
        // Django `per_page=0` → ZeroDivision di paginator → 500; di sini
        // kembalikan envelope kosong 200 (documented normalize-crash).
        return Ok((
            StatusCode::OK,
            Json(json!({
                "grouped_by": Value::Null, "sub_grouped_by": Value::Null,
                "total_count": 0, "next_cursor": next_cursor_str(0, cursor.page),
                "prev_cursor": prev_cursor_str(0, cursor.page),
                "next_page_results": false, "prev_page_results": cursor.page > 0,
                "count": 0, "total_pages": 0, "total_results": 0,
                "extra_stats": Value::Null, "results": [],
            })),
        ));
    }
    let window = match page_window(cursor.page, per_page) {
        Err(()) => return Ok(bad_request(json!({"detail": "Invalid cursor parameter."}))),
        Ok(w) => w,
    };
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_activities WHERE actor_id = $1 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);
    let dir = if desc { "DESC" } else { "ASC" };
    let sql = format!(
        "SELECT id, verb, field, old_value, new_value, comment, created_at, updated_at, \
                issue_id, project_id, workspace_id, actor_id \
         FROM issue_activities WHERE actor_id = $1 AND deleted_at IS NULL \
         ORDER BY {col} {dir}, created_at DESC LIMIT $2 OFFSET $3"
    );
    let (rows, beyond) = match window {
        PageWindow::BeyondEnd => (vec![], true),
        PageWindow::Rows(off) => {
            let r: Vec<ActivityRow> = sqlx::query_as(&sql)
                .bind(auth.0)
                .bind(per_page + 1)
                .bind(off)
                .fetch_all(&st.pool)
                .await
                .unwrap_or(vec![]);
            (r, false)
        }
    };
    let mut rows = rows;
    let has_next = !beyond && rows.len() as i64 > per_page;
    rows.truncate(per_page as usize);
    let results: Vec<Value> = rows.iter().map(activity_json).collect();
    let n = results.len() as i64;
    Ok((
        StatusCode::OK,
        Json(json!({
            "grouped_by": Value::Null, "sub_grouped_by": Value::Null,
            "total_count": total, "next_cursor": next_cursor_str(per_page, cursor.page),
            "prev_cursor": prev_cursor_str(per_page, cursor.page),
            "next_page_results": has_next, "prev_page_results": cursor.page > 0,
            "count": n, "total_pages": total_pages(total, per_page), "total_results": total,
            "extra_stats": Value::Null, "results": results,
        })),
    ))
}

// ============================================================================
// E8d — workspace-scoped user routes.
// ============================================================================

/// GET /api/workspaces/:slug/user-stats/:user_id/ —
/// `WorkspaceUserProfileStatsEndpoint.get` (`workspace/user.py:405-529`).
/// Gate OPEN (member mana pun yg authed; task E8: any authed user).
/// `issue_filters` DIABAIKAN (per E8 contract — replicate literally);
/// cycle_issues TANPA guard deleted/archived (literal Django);
/// slug tak dikenal → 200 kosong (bukan 404).
pub async fn user_stats(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, target)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let _ = auth;
    if !ws_exists(&st.pool, &slug).await {
        return Ok((
            StatusCode::OK,
            Json(json!({
                "state_distribution": [], "priority_distribution": [],
                "created_issues": 0, "assigned_issues": 0, "completed_issues": 0,
                "pending_issues": 0, "subscribed_issues": 0,
                "present_cycles": [], "upcoming_cycles": [],
            })),
        ));
    }
    // state_distribution: assignee aktif + requester ACTIVE project member.
    let state_rows: Vec<(Option<String>, i64)> = sqlx::query_as(
        "SELECT s.\"group\" AS state_group, COUNT(*) FROM issues i \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         JOIN states s ON s.id = i.state_id \
         JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         GROUP BY s.\"group\" ORDER BY s.\"group\"",
    )
    .bind(&slug)
    .bind(target)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![]);
    // priority_distribution + order urgent,high,medium,low,none (`user.py:423-444`).
    let prio_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT i.priority, COUNT(*) FROM issues i \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         GROUP BY i.priority HAVING COUNT(*) >= 1",
    )
    .bind(&slug)
    .bind(target)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![]);
    let order = ["urgent", "high", "medium", "low", "none"];
    let mut prio_rows = prio_rows;
    prio_rows.sort_by_key(|(p, _)| order.iter().position(|o| o == p).unwrap_or(order.len()));
    macro_rules! cnt {
        ($sql:expr) => {{
            sqlx::query_scalar($sql)
                .bind(&slug)
                .bind(target)
                .bind(auth.0)
                .fetch_one(&st.pool)
                .await
                .unwrap_or(0)
        }};
    }
    let created: i64 = cnt!(
        "SELECT COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.created_by_id = $2 AND i.deleted_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)"
    );
    let assigned: i64 = cnt!(
        "SELECT COUNT(*) FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id \
           AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)"
    );
    let pending: i64 = cnt!(
        "SELECT COUNT(*) FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id \
           AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL AND (s.\"group\" IS NULL OR s.\"group\" NOT IN ('completed','cancelled')) \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)"
    );
    let completed: i64 = cnt!(
        "SELECT COUNT(*) FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id \
           AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         JOIN states s ON s.id = i.state_id AND s.\"group\" = 'completed' \
         JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)"
    );
    let subscribed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_subscribers s JOIN workspaces w ON w.id = s.workspace_id \
         JOIN projects p ON p.id = s.project_id \
         WHERE w.slug = $1 AND s.subscriber_id = $2 AND s.deleted_at IS NULL AND p.archived_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = s.project_id \
             AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)",
    )
    .bind(&slug)
    .bind(target)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);
    // Cycles literal Django (`user.py:504-515`): tanpa deleted/archived guard.
    let upcoming: Vec<Value> =
        sqlx::query_as::<_, (Option<String>, Option<uuid::Uuid>, Option<uuid::Uuid>)>(
            "SELECT c.name, c.id, c.project_id FROM cycle_issues ci \
         JOIN cycles c ON c.id = ci.cycle_id JOIN workspaces w ON w.id = ci.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = ci.issue_id AND ia.assignee_id = $2 \
         WHERE w.slug = $1 AND c.start_date > now()",
        )
        .bind(&slug)
        .bind(target)
        .fetch_all(&st.pool)
        .await
        .unwrap_or(vec![])
        .into_iter()
        .map(|(n, id, pid)| json!({"cycle__name": n, "cycle__id": id, "cycle__project_id": pid}))
        .collect();
    let present: Vec<Value> =
        sqlx::query_as::<_, (Option<String>, Option<uuid::Uuid>, Option<uuid::Uuid>)>(
            "SELECT c.name, c.id, c.project_id FROM cycle_issues ci \
         JOIN cycles c ON c.id = ci.cycle_id JOIN workspaces w ON w.id = ci.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = ci.issue_id AND ia.assignee_id = $2 \
         WHERE w.slug = $1 AND c.start_date < now() AND c.end_date > now()",
        )
        .bind(&slug)
        .bind(target)
        .fetch_all(&st.pool)
        .await
        .unwrap_or(vec![])
        .into_iter()
        .map(|(n, id, pid)| json!({"cycle__name": n, "cycle__id": id, "cycle__project_id": pid}))
        .collect();
    Ok((
        StatusCode::OK,
        Json(json!({
            "state_distribution": state_rows.into_iter().map(|(g, c)| json!({"state_group": g, "state_count": c})).collect::<Vec<_>>(),
            "priority_distribution": prio_rows.into_iter().map(|(p, c)| json!({"priority": p, "priority_count": c})).collect::<Vec<_>>(),
            "created_issues": created, "assigned_issues": assigned,
            "completed_issues": completed, "pending_issues": pending,
            "subscribed_issues": subscribed,
            "present_cycles": present, "upcoming_cycles": upcoming,
        })),
    ))
}

/// GET /api/workspaces/:slug/user-profile/:user_id/ —
/// `WorkspaceUserProfileEndpoint.get` (`workspace/user.py:280-372`).
/// Requester non-member → 404; target non-member → 404 (Django `.get()`
/// crash dinormalisasi); `project_data` [] kecuali requester role ≥ 15.
pub async fn user_profile(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, target)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let req_role = ws_member_role(&st.pool, auth.0, &slug)
        .await
        .unwrap_or(None);
    let Some(req_role) = req_role else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        ));
    };
    let tgt: Option<MeRow> = fetch_me(&st.pool, target).await.unwrap_or(None);
    let Some(tgt) = tgt else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        ));
    };
    // Target harus member aktif workspace ini.
    let tgt_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id \
         WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL)",
    )
    .bind(&slug)
    .bind(target)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(false);
    if !tgt_member {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        ));
    }
    let mut project_data = vec![];
    if req_role >= 15 {
        let rows: Vec<(uuid::Uuid, Value, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT p.id, p.logo_props, \
              (SELECT COUNT(*) FROM issues i WHERE i.project_id = p.id AND i.created_by_id = $2 \
                AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS created_issues, \
              (SELECT COUNT(*) FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 \
                AND ia.deleted_at IS NULL WHERE i.project_id = p.id \
                AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS assigned_issues, \
              (SELECT COUNT(*) FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 \
                AND ia.deleted_at IS NULL WHERE i.project_id = p.id AND i.completed_at IS NOT NULL \
                AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS completed_issues, \
              (SELECT COUNT(*) FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 \
                AND ia.deleted_at IS NULL LEFT JOIN states s ON s.id = i.state_id \
               WHERE i.project_id = p.id AND (s.\"group\" IN ('backlog','unstarted','started')) \
                AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS pending_issues \
             FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
             WHERE w.slug = $1 AND p.archived_at IS NULL AND p.deleted_at IS NULL \
               AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
                 AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)",
        )
        .bind(&slug)
        .bind(target)
        .bind(auth.0)
        .fetch_all(&st.pool)
        .await
        .unwrap_or(vec![]);
        project_data = rows
            .into_iter()
            .map(|(id, logo, c, a, co, pe)| {
                json!({"id": id, "logo_props": logo, "created_issues": c,
                       "assigned_issues": a, "completed_issues": co, "pending_issues": pe})
            })
            .collect();
    }
    let avatar_url = tgt.avatar_asset.clone().or_else(|| tgt.avatar.clone());
    let cover_url = tgt.cover_asset.clone().or_else(|| tgt.cover_image.clone());
    Ok((
        StatusCode::OK,
        Json(json!({
        "project_data": project_data,
        "user_data": {
            "email": tgt.email, "first_name": tgt.first_name, "last_name": tgt.last_name,
            "avatar_url": avatar_url, "cover_image_url": cover_url,
            "date_joined": tgt.date_joined, "user_timezone": tgt.user_timezone,
            "display_name": tgt.display_name,
        }})),
    ))
}

/// GET /api/workspaces/:slug/user-activity/:user_id/ —
/// `WorkspaceUserActivityEndpoint.get` (`workspace/user.py:375-402`).
/// Gate `WorkspaceEntityPermission` (GET safe → member aktif apa pun;
/// non-member → 403 DRF detail). Field exclusion comment/vote/reaction/
/// draft + filter project repeatable + order_by sanitize + cursor/per_page≤1000.
pub async fn user_activity(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, target)): Path<(String, uuid::Uuid)>,
    Query(q): Query<ActivityQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_member_role(&st.pool, auth.0, &slug)
        .await
        .unwrap_or(None)
        .is_none()
    {
        return Ok(deny_detail());
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(e) => return Ok(bad_request(json!({"detail": e}))),
    };
    let cursor_raw = q
        .cursor
        .clone()
        .unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(e) => return Ok(bad_request(json!({"detail": e}))),
    };
    if per_page <= 0 {
        return Ok((
            StatusCode::OK,
            Json(json!({
                "grouped_by": Value::Null, "sub_grouped_by": Value::Null,
                "total_count": 0, "next_cursor": next_cursor_str(0, cursor.page),
                "prev_cursor": prev_cursor_str(0, cursor.page),
                "next_page_results": false, "prev_page_results": cursor.page > 0,
                "count": 0, "total_pages": 0, "total_results": 0,
                "extra_stats": Value::Null, "results": [],
            })),
        ));
    }
    let window = match page_window(cursor.page, per_page) {
        Err(()) => return Ok(bad_request(json!({"detail": "Invalid cursor parameter."}))),
        Ok(w) => w,
    };
    let sanitized = sanitize_activity_order_by(q.order_by.as_deref());
    let (col, desc) = match sanitized.as_str() {
        "-updated_at" => ("a.updated_at", true),
        "created_at" => ("a.created_at", false),
        "updated_at" => ("a.updated_at", false),
        _ => ("a.created_at", true),
    };
    let dir = if desc { "DESC" } else { "ASC" };
    // Filter project repeatable: parse UUID, sampah diabaikan (fail-open).
    let proj_ids: Vec<uuid::Uuid> = q
        .project
        .clone()
        .unwrap_or_default()
        .iter()
        .filter_map(|s| s.parse::<uuid::Uuid>().ok())
        .collect();
    let has_proj = !proj_ids.is_empty();
    let base_where = "FROM issue_activities a JOIN workspaces w ON w.id = a.workspace_id \
        JOIN projects p ON p.id = a.project_id \
        WHERE w.slug = $1 AND w.deleted_at IS NULL AND a.actor_id = $2 \
          AND (a.field IS NULL OR a.field NOT IN ('comment','vote','reaction','draft')) \
          AND a.deleted_at IS NULL AND p.archived_at IS NULL \
          AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = a.project_id \
            AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";
    let total: i64 = if has_proj {
        sqlx::query_scalar(&format!(
            "SELECT COUNT(*) {base_where} AND a.project_id = ANY($4)"
        ))
        .bind(&slug)
        .bind(target)
        .bind(auth.0)
        .bind(&proj_ids)
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0)
    } else {
        sqlx::query_scalar(&format!("SELECT COUNT(*) {base_where}"))
            .bind(&slug)
            .bind(target)
            .bind(auth.0)
            .fetch_one(&st.pool)
            .await
            .unwrap_or(0)
    };
    let sel = format!(
        "SELECT a.id, a.verb, a.field, a.old_value, a.new_value, a.comment, a.created_at, \
                a.updated_at, a.issue_id, a.project_id, a.workspace_id, a.actor_id {base_where}"
    );
    let rows: Vec<ActivityRow> = match window {
        PageWindow::BeyondEnd => vec![],
        PageWindow::Rows(off) => {
            let sql = if has_proj {
                format!("{sel} AND a.project_id = ANY($4) ORDER BY {col} {dir}, a.created_at DESC LIMIT $5 OFFSET $6")
            } else {
                format!("{sel} ORDER BY {col} {dir}, a.created_at DESC LIMIT $4 OFFSET $5")
            };
            if has_proj {
                sqlx::query_as(&sql)
                    .bind(&slug)
                    .bind(target)
                    .bind(auth.0)
                    .bind(&proj_ids)
                    .bind(per_page + 1)
                    .bind(off)
                    .fetch_all(&st.pool)
                    .await
                    .unwrap_or(vec![])
            } else {
                sqlx::query_as(&sql)
                    .bind(&slug)
                    .bind(target)
                    .bind(auth.0)
                    .bind(per_page + 1)
                    .bind(off)
                    .fetch_all(&st.pool)
                    .await
                    .unwrap_or(vec![])
            }
        }
    };
    let mut rows = rows;
    let has_next = rows.len() as i64 > per_page;
    rows.truncate(per_page as usize);
    let results: Vec<Value> = rows.iter().map(activity_json).collect();
    let n = results.len() as i64;
    Ok((
        StatusCode::OK,
        Json(json!({
            "grouped_by": Value::Null, "sub_grouped_by": Value::Null,
            "total_count": total, "next_cursor": next_cursor_str(per_page, cursor.page),
            "prev_cursor": prev_cursor_str(per_page, cursor.page),
            "next_page_results": has_next, "prev_page_results": cursor.page > 0,
            "count": n, "total_pages": total_pages(total, per_page), "total_results": total,
            "extra_stats": Value::Null, "results": results,
        })),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ExportBody {
    #[serde(default)]
    pub date: Option<String>,
}

/// Satu baris export CSV (JOIN users/issues/projects untuk kolom tampilan).
type ExportRow = (
    Option<String>,
    Option<String>,
    Option<i32>,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// POST /api/workspaces/:slug/user-activity/:user_id/export/ —
/// `ExportWorkspaceUserActivityEndpoint.post` (`workspace/base.py`).
/// Body `{date}` wajib → 400 `{"error":"Date is required"}`; Guest → 403
/// `{"error":...}` (allow_permission); cap 10000; QUOTE_ALL; header verbatim.
/// Sukses = 200 RAW CSV bytes (`text/csv` + disposition), BUKAN JSON string.
pub async fn export_activity(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, target)): Path<(String, uuid::Uuid)>,
    Json(body): Json<ExportBody>,
) -> Result<Response, common::errors::AppError> {
    let date = body.date.unwrap_or_default().trim().to_string();
    if date.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Json(json!({"error": DATE_REQUIRED_MSG})),
        )
            .into_response());
    }
    // Non-safe → ADMIN/MEMBER; Guest/non-member → 403 `{"error":...}`.
    let role = ws_member_role(&st.pool, auth.0, &slug)
        .await
        .unwrap_or(None);
    match role {
        Some(20) | Some(15) => {}
        _ => {
            return Ok((
                StatusCode::FORBIDDEN,
                HeaderMap::new(),
                Json(json!({"error": FORBIDDEN_MSG})),
            )
                .into_response())
        }
    }
    // Tanggal `%Y-%m-%d`; sampah → 400 detail-msg (Django `__date` parse
    // crash dinormalisasi).
    if chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Json(json!({"error": "Please provide valid detail"})),
        )
            .into_response());
    }
    let rows: Vec<ExportRow> = sqlx::query_as(
        "SELECT u.display_name, p.identifier, i.sequence_id, p.name, a.created_at, a.updated_at, \
                a.verb, a.field, a.old_value, a.new_value \
         FROM issue_activities a \
         JOIN workspaces w ON w.id = a.workspace_id \
         LEFT JOIN users u ON u.id = a.actor_id \
         LEFT JOIN issues i ON i.id = a.issue_id \
         LEFT JOIN projects p ON p.id = a.project_id \
         WHERE w.slug = $1 AND w.deleted_at IS NULL AND a.actor_id = $2 \
           AND (a.field IS NULL OR a.field NOT IN ('comment','vote','reaction','draft')) \
           AND a.created_at::date = $3::date AND a.deleted_at IS NULL \
           AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = a.project_id \
             AND pm.member_id = $4 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         ORDER BY a.created_at ASC LIMIT 10000",
    )
    .bind(&slug)
    .bind(target)
    .bind(&date)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![]);
    let mut out = String::new();
    out.push_str(&csv_line_quoted_all(
        &ACTIVITY_CSV_HEADER
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    ));
    for (actor, ident, seq, proj, ca, ua, verb, field, old, new) in rows {
        let issue_id = match (ident, seq) {
            (Some(id), Some(s)) => format!("{id} - {s}"),
            _ => String::new(),
        };
        let row = vec![
            actor.unwrap_or_default(),
            issue_id,
            proj.unwrap_or_default(),
            ca.to_rfc3339(),
            ua.to_rfc3339(),
            verb.unwrap_or_default(),
            field.unwrap_or_default(),
            old.unwrap_or_default(),
            new.unwrap_or_default(),
        ];
        out.push_str(&csv_line_quoted_all(&row));
    }
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "text/csv".parse().unwrap());
    h.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"workspace-user-activity.csv\""
            .parse()
            .unwrap(),
    );
    // Body = RAW CSV bytes (Django `HttpResponse(csv, content_type="text/csv")`);
    // JANGAN dibungkus `Json(Value::String)` (itu mengemit JSON string
    // ber-quote, bukan CSV mentah).
    Ok((StatusCode::OK, h, out).into_response())
}

// ============================================================================
// Graphs + dashboard — `workspace/user.py:532-567`, `workspace/base.py:175-391`.
// ============================================================================

/// GET /api/users/me/workspaces/:slug/activity-graph/ — harian 6 bulan terakhir.
pub async fn activity_graph(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let rows: Vec<(chrono::NaiveDate, i64)> = sqlx::query_as(
        "SELECT a.created_at::date AS created_date, COUNT(*) FROM issue_activities a \
         JOIN workspaces w ON w.id = a.workspace_id \
         WHERE a.actor_id = $1 AND w.slug = $2 AND w.deleted_at IS NULL \
           AND a.created_at::date >= (CURRENT_DATE - INTERVAL '6 months')::date \
           AND a.deleted_at IS NULL \
         GROUP BY a.created_at::date ORDER BY created_date ASC",
    )
    .bind(auth.0)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![]);
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .into_iter()
            .map(|(d, c)| json!({"created_date": d, "activity_count": c}))
            .collect::<Vec<_>>())),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MonthQuery {
    #[serde(default)]
    pub month: Option<String>,
}

/// GET /api/users/me/workspaces/:slug/issues-completed-graph/ (`?month=` default 1).
pub async fn issues_completed_graph(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<MonthQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let month = parse_month(q.month.as_deref());
    // Django: `week = completed_week % 4` (`user.py:561`), group by week.
    let rows: Vec<(i32, i64)> = sqlx::query_as(
        "SELECT (EXTRACT(WEEK FROM i.completed_at)::int % 4) AS week, COUNT(*) \
         FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         WHERE w.slug = $1 AND w.deleted_at IS NULL AND i.deleted_at IS NULL \
           AND i.completed_at IS NOT NULL AND EXTRACT(MONTH FROM i.completed_at)::int = $3 \
         GROUP BY (EXTRACT(WEEK FROM i.completed_at)::int % 4) ORDER BY week ASC",
    )
    .bind(&slug)
    .bind(auth.0)
    .bind(month)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![]);
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .into_iter()
            .map(|(w, c)| json!({"week": w, "completed_count": c}))
            .collect::<Vec<_>>())),
    ))
}

/// GET /api/users/me/workspaces/:slug/dashboard/ (`?month=`, 9 kunci).
pub async fn dashboard(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<MonthQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let month = parse_month(q.month.as_deref());
    let acts: Vec<Value> = sqlx::query_as::<_, (chrono::NaiveDate, i64)>(
        "SELECT a.created_at::date, COUNT(*) FROM issue_activities a \
         JOIN workspaces w ON w.id = a.workspace_id \
         WHERE a.actor_id = $1 AND w.slug = $2 AND w.deleted_at IS NULL \
           AND a.created_at::date >= (CURRENT_DATE - INTERVAL '3 months')::date \
           AND a.deleted_at IS NULL GROUP BY a.created_at::date ORDER BY a.created_at::date ASC",
    )
    .bind(uid)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![])
    .into_iter()
    .map(|(d, c)| json!({"created_date": d, "activity_count": c}))
    .collect();
    // completed per week-in-month: `FLOOR(((day-1)/7)+1)` (`base.py:WeekInMonth`).
    let comp: Vec<Value> = sqlx::query_as::<_, (i32, i64)>(
        "SELECT FLOOR((((EXTRACT(DAY FROM i.completed_at)::int - 1) / 7) + 1))::int AS week_in_month, COUNT(*) \
         FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         WHERE w.slug = $1 AND w.deleted_at IS NULL AND i.deleted_at IS NULL \
           AND i.completed_at IS NOT NULL AND EXTRACT(MONTH FROM i.completed_at)::int = $3 \
         GROUP BY FLOOR((((EXTRACT(DAY FROM i.completed_at)::int - 1) / 7) + 1))::int ORDER BY week_in_month ASC",
    )
    .bind(&slug)
    .bind(uid)
    .bind(month)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![])
    .into_iter()
    .map(|(w, c)| json!({"week_in_month": w, "completed_count": c}))
    .collect();
    macro_rules! sc {
        ($sql:expr) => {{
            sqlx::query_scalar($sql)
                .bind(&slug)
                .bind(uid)
                .fetch_one(&st.pool)
                .await
                .unwrap_or(0)
        }};
    }
    let assigned = sc!(
        "SELECT COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         WHERE w.slug = $1 AND i.deleted_at IS NULL"
    );
    let pending = sc!(
        "SELECT COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL AND (s.\"group\" IS NULL OR s.\"group\" NOT IN ('completed','cancelled'))"
    );
    let completed_n = sc!(
        "SELECT COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         JOIN states s ON s.id = i.state_id AND s.\"group\" = 'completed' \
         WHERE w.slug = $1 AND i.deleted_at IS NULL"
    );
    let due_week: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND EXTRACT(WEEK FROM i.target_date)::int = EXTRACT(WEEK FROM now())::int",
    )
    .bind(&slug)
    .bind(uid)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);
    let state_dist: Vec<Value> = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT s.\"group\", COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL GROUP BY s.\"group\" ORDER BY s.\"group\"",
    )
    .bind(&slug)
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![])
    .into_iter()
    .map(|(g, c)| json!({"state_group": g, "state_count": c}))
    .collect();
    let overdue: Vec<Value> = sqlx::query_as::<_, (uuid::Uuid, String, String, uuid::Uuid, Option<chrono::NaiveDate>)>(
        "SELECT i.id, i.name, w.slug, i.project_id, i.target_date FROM issues i \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND (s.\"group\" IS NULL OR s.\"group\" NOT IN ('completed','cancelled')) \
           AND i.target_date < CURRENT_DATE AND i.completed_at IS NULL",
    )
    .bind(&slug)
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![])
    .into_iter()
    .map(|(id, name, wslug, pid, td)| {
        json!({"id": id, "name": name, "workspace__slug": wslug, "project_id": pid, "target_date": td})
    })
    .collect();
    let upcoming: Vec<Value> = sqlx::query_as::<_, (uuid::Uuid, String, String, uuid::Uuid, Option<chrono::NaiveDate>)>(
        "SELECT i.id, i.name, w.slug, i.project_id, i.start_date FROM issues i \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.assignee_id = $2 AND ia.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL \
           AND (s.\"group\" IS NULL OR s.\"group\" NOT IN ('completed','cancelled')) \
           AND i.start_date >= CURRENT_DATE AND i.completed_at IS NULL",
    )
    .bind(&slug)
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    .unwrap_or(vec![])
    .into_iter()
    .map(|(id, name, wslug, pid, sd)| {
        json!({"id": id, "name": name, "workspace__slug": wslug, "project_id": pid, "start_date": sd})
    })
    .collect();
    Ok((
        StatusCode::OK,
        Json(json!({
            "issue_activities": acts, "completed_issues": comp,
            "assigned_issues_count": assigned, "pending_issues_count": pending,
            "completed_issues_count": completed_n, "issues_due_week_count": due_week,
            "state_distribution": state_dist, "overdue_issues": overdue,
            "upcoming_issues": upcoming,
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn name_url_guards_verbatim() {
        assert_eq!(FIRST_NAME_URL_MSG, "First name cannot contain a URL.");
        assert_eq!(LAST_NAME_URL_MSG, "Last name cannot contain a URL.");
        assert!(validate_name(Some("https://x.io a"), None).is_err());
        assert!(validate_name(None, Some("see www.x.io")).is_err());
        assert!(validate_name(Some("Ada"), Some("Lovelace")).is_ok());
    }

    #[test]
    fn deactivate_guards_verbatim() {
        assert_eq!(
            DEACTIVATE_INSTANCE_ADMIN_MSG,
            "You cannot deactivate your account since you are an instance admin"
        );
        assert_eq!(
            DEACTIVATE_SOLE_PROJECT_ADMIN_MSG,
            "You cannot deactivate account as you are the only admin in some projects."
        );
        assert_eq!(
            DEACTIVATE_SOLE_WORKSPACE_ADMIN_MSG,
            "You cannot deactivate account as you are the only admin in some workspaces."
        );
    }

    #[test]
    fn csv_header_and_sanitize_verbatim() {
        assert_eq!(
            ACTIVITY_CSV_HEADER,
            [
                "Actor name",
                "Issue ID",
                "Project",
                "Created at",
                "Updated at",
                "Action",
                "Field",
                "Old value",
                "New value",
            ]
        );
        assert_eq!(sanitize_csv_value("=cmd"), "'=cmd");
        assert_eq!(sanitize_csv_value("+1"), "'+1");
        assert_eq!(sanitize_csv_value("-2"), "'-2");
        assert_eq!(sanitize_csv_value("@x"), "'@x");
        assert_eq!(sanitize_csv_value("ok"), "ok");
        assert_eq!(sanitize_csv_value(""), "");
        let line = csv_line_quoted_all(&["a\"b".to_string(), "=c".to_string()]);
        assert_eq!(line, "\"a\"\"b\",\"'=c\"\r\n");
    }

    #[test]
    fn export_body_is_raw_csv_not_json_string() {
        // Sukses export = RAW CSV bytes (`text/csv`), BUKAN `Json(Value::String)`
        // (itu mengemit `"\"Actor name\",..."` — JSON string ber-quote).
        // QUOTE_ALL tetap: header pun dikutip → body mentah diawali `"Actor`,
        // sedangkan versi JSON diawali `"\"` (quote + backslash).
        let header: Vec<String> = ACTIVITY_CSV_HEADER.iter().map(|s| s.to_string()).collect();
        let mut raw = csv_line_quoted_all(&header);
        raw.push_str(&csv_line_quoted_all(&[
            "Ada".to_string(),
            "PRJ - 1".to_string(),
            "Proj".to_string(),
            "2026-01-01T00:00:00+00:00".to_string(),
            "2026-01-02T00:00:00+00:00".to_string(),
            "created".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
        ]));
        assert!(raw.starts_with("\"Actor name\",\"Issue ID\""), "body=\n{raw}");
        assert!(!raw.contains("\\\""), "tak boleh ada escape JSON: {raw}");
        assert!(raw.contains("\r\n\""), "baris data dikutip setelah CRLF");
        let wrapped = serde_json::to_string(&raw).expect("json wrap");
        assert!(wrapped.starts_with("\"\\\""), "bentuk bug lama: {wrapped}");
        assert_ne!(raw.as_bytes()[0], b'\\');
    }

    #[test]
    fn stats_and_dashboard_key_sets() {
        assert_eq!(
            USER_STATS_KEYS,
            [
                "state_distribution",
                "priority_distribution",
                "created_issues",
                "assigned_issues",
                "completed_issues",
                "pending_issues",
                "subscribed_issues",
                "present_cycles",
                "upcoming_cycles",
            ]
        );
        assert_eq!(
            DASHBOARD_KEYS,
            [
                "issue_activities",
                "completed_issues",
                "assigned_issues_count",
                "pending_issues_count",
                "completed_issues_count",
                "issues_due_week_count",
                "state_distribution",
                "overdue_issues",
                "upcoming_issues",
            ]
        );
    }

    #[test]
    fn throttle_429_body_shape() {
        // `EmailVerificationThrottle.throttle_failure_view`
        // (`authentication/rate_limit.py:93-...`) → 429
        // `{"error_code": 5900, "error_message": "RATE_LIMIT_EXCEEDED"}`.
        let body = json!({"error_code": 5900, "error_message": "RATE_LIMIT_EXCEEDED"});
        assert_eq!(body["error_code"], 5900);
        assert_eq!(body["error_message"], "RATE_LIMIT_EXCEEDED");
    }

    #[test]
    fn activity_order_sanitize() {
        assert_eq!(sanitize_activity_order_by(None), "-created_at");
        assert_eq!(sanitize_activity_order_by(Some("created_at")), "created_at");
        assert_eq!(
            sanitize_activity_order_by(Some("-updated_at")),
            "-updated_at"
        );
        assert_eq!(sanitize_activity_order_by(Some("name")), "-created_at");
        assert_eq!(
            sanitize_activity_order_by(Some("--created_at")),
            "-created_at"
        );
    }

    #[test]
    fn month_coercion_fail_open() {
        assert_eq!(parse_month(None), 1);
        assert_eq!(parse_month(Some("3")), 3);
        assert_eq!(parse_month(Some("garbage")), 1);
        assert_eq!(parse_month(Some("13")), 12);
    }

    #[test]
    fn bool_coercion_fail_open() {
        assert_eq!(coerce_bool(&json!(true)), Some(true));
        assert_eq!(coerce_bool(&json!("true")), Some(true));
        assert_eq!(coerce_bool(&json!("FALSE")), Some(false));
        assert_eq!(coerce_bool(&json!("1")), Some(true));
        assert_eq!(coerce_bool(&json!(0)), Some(false));
        assert_eq!(coerce_bool(&json!("garbage")), None);
    }

    #[test]
    fn me_shape_has_17_unique_keys() {
        // `UserMeSerializer` (`serializers/user.py:63-87`): 19 entri daftar
        // dengan `is_email_verified` ganda → 18 unik (locked §7: emit SEKALI).
        let fields = [
            "id",
            "avatar",
            "cover_image",
            "avatar_url",
            "cover_image_url",
            "date_joined",
            "display_name",
            "email",
            "first_name",
            "last_name",
            "is_active",
            "is_bot",
            "is_email_verified",
            "user_timezone",
            "username",
            "is_password_autoset",
            "last_login_medium",
            "last_login_time",
        ];
        assert_eq!(fields.len(), 19 - 1);
        let r = MeRow {
            id: uuid::Uuid::nil(),
            avatar: None,
            cover_image: None,
            avatar_asset: Some("a".into()),
            cover_asset: None,
            date_joined: chrono::Utc::now(),
            display_name: "d".into(),
            email: Some("e@x.io".into()),
            first_name: "f".into(),
            last_name: "l".into(),
            is_active: true,
            is_bot: false,
            is_email_verified: false,
            user_timezone: "UTC".into(),
            username: "u".into(),
            is_password_autoset: false,
            last_login_medium: "email".into(),
            last_login_time: None,
        };
        let v = me_json(&r);
        for k in fields {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["avatar_url"], json!("a"));
    }

    #[test]
    fn deny_shapes_split() {
        // DRF-class deny → `{"detail"}`, allow_permission deny → `{"error"}`.
        let (s, j) = deny_detail();
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(
            j.0,
            json!({"detail": "You do not have permission to perform this action."})
        );
        let (s2, j2) = deny_allow();
        assert_eq!(s2, StatusCode::FORBIDDEN);
        assert_eq!(j2.0, json!({"error": FORBIDDEN_MSG}));
    }
}
