//! Paritas Django `/api/users/me/*` (`apps/api/plane/app/urls/user.py`,
//! `workspace.py`, `project.py`).

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::state::AppState;

pub fn email_code_throttle_key(uid: &uuid::Uuid) -> String {
    format!("emailcode:throttle:{uid}")
}

pub fn email_code_key(uid: &uuid::Uuid, email: &str) -> String {
    format!("emailcode:{uid}:{email}")
}

/// 6 digit seperti Django `secrets.randbelow(900000) + 100000`.
pub fn new_email_code() -> String {
    let n: u32 = rand::random_range(100000..1000000);
    format!("{n:06}")
}

#[derive(Deserialize)]
pub struct EmailBody {
    pub email: Option<String>,
}

fn plain_error(message: &str) -> Json<Value> {
    Json(json!({"error": message}))
}

/// Validasi email-baru, pesan byte-exact dari `_validate_new_email`.
async fn validate_new_email(
    pool: &sqlx::PgPool,
    uid: uuid::Uuid,
    current: &str,
    raw: Option<String>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let email = raw.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() {
        return Err((StatusCode::BAD_REQUEST, plain_error("Email is required")));
    }
    if !crate::routes::auth::email_valid(&email) {
        return Err((StatusCode::BAD_REQUEST, plain_error("Invalid email format")));
    }
    if email == current {
        return Err((StatusCode::BAD_REQUEST, plain_error("New email must be different from current email")));
    }
    // NOTE: tabel `users` tidak punya `deleted_at` (skema aktual) — filter email saja.
    // Galat DB di sini jangan fail-open (bisa melewatkan duplikat) — balas 500.
    let taken: Option<bool> = match sqlx::query_scalar("SELECT true FROM users WHERE email = $1 AND id <> $2")
        .bind(&email)
        .bind(uid)
        .fetch_optional(pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "generate-code: duplicate-email check failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
    };
    if taken == Some(true) {
        return Err((StatusCode::BAD_REQUEST, plain_error("An account with this email already exists")));
    }
    Ok(email)
}

/// POST /api/users/me/email/generate-code/ — paritas
/// `UserEndpoint.generate_email_verification_code` (3/jam via Redis).
pub async fn generate_email_code(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<EmailBody>,
) -> (StatusCode, Json<Value>) {
    // Galat DB = 500; hanya user yang benar-benar hilang yang jadi 401.
    let email_row: Option<(String,)> = match sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "generate-code: current-email lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let Some((current,)) = email_row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let email = match validate_new_email(&st.pool, auth.0, &current.to_lowercase(), body.email).await {
        Ok(e) => e,
        Err(err) => return err,
    };
    let mut conn = match st.redis_client().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))),
    };
    // INCR gagal = fail-closed (anggap budget habis) agar throttle tak bisa
    // dilewati saat Redis bermasalah.
    let count: i64 = redis::cmd("INCR")
        .arg(email_code_throttle_key(&auth.0))
        .query_async(&mut conn)
        .await
        .unwrap_or(4);
    if count == 1 {
        // Tanpa TTL, kunci throttle mengunci user selamanya — kalau EXPIRE
        // gagal, hapus kuncinya agar hitungan mulai bersih di percobaan berikut.
        let expire: redis::RedisResult<()> = redis::cmd("EXPIRE")
            .arg(email_code_throttle_key(&auth.0))
            .arg(3600)
            .query_async(&mut conn)
            .await;
        if let Err(e) = expire {
            tracing::warn!(error = %e, "generate-code: EXPIRE throttle failed, resetting key");
            let _: () = redis::cmd("DEL")
                .arg(email_code_throttle_key(&auth.0))
                .query_async(&mut conn)
                .await
                .unwrap_or(());
        }
    }
    if count > 3 {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error_code": 5900, "error_message": "RATE_LIMIT_EXCEEDED"})));
    }
    let code = new_email_code();
    let stored: redis::RedisResult<()> = redis::cmd("SET")
        .arg(email_code_key(&auth.0, &email))
        .arg(json!({"token": code}).to_string())
        .arg("EX")
        .arg(600)
        .query_async(&mut conn)
        .await;
    if stored.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    // SMTP belum ada: kode tersimpan di Redis 10 mnt; pengiriman email = follow-up.
    tracing::info!(user_id = %auth.0, "email verification code stored (delivery pending SMTP)");
    (StatusCode::OK, Json(json!({"message": "Verification code sent to email"})))
}

#[derive(Deserialize)]
pub struct UpdateEmailBody {
    pub email: Option<String>,
    pub code: Option<String>,
}

/// PATCH /api/users/me/email/ — paritas `UserEndpoint.update_email`.
pub async fn update_email(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateEmailBody>,
) -> (StatusCode, Json<Value>) {
    // Galat DB = 500; hanya user yang benar-benar hilang yang jadi 401.
    let row: Option<(String,)> = match sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "update-email: current-email lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let Some((current,)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let email = match validate_new_email(&st.pool, auth.0, &current.to_lowercase(), body.email).await {
        Ok(e) => e,
        Err(err) => return err,
    };
    let code = body.code.unwrap_or_default().trim().to_string();
    if code.is_empty() {
        return (StatusCode::BAD_REQUEST, plain_error("Verification code is required"));
    }
    let mut conn = match st.redis_client().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))),
    };
    let key = email_code_key(&auth.0, &email);
    let cached: Option<String> = redis::cmd("GET").arg(&key).query_async(&mut conn).await.unwrap_or(None);
    let Some(raw) = cached else {
        return (StatusCode::BAD_REQUEST, plain_error("Verification code has expired or is invalid"));
    };
    let stored: String = serde_json::from_str::<Value>(&raw).ok().and_then(|v| v.get("token").and_then(|t| t.as_str()).map(str::to_string)).unwrap_or_default();
    if stored != code {
        return (StatusCode::BAD_REQUEST, plain_error("Invalid verification code"));
    }
    // Cek ulang duplikat (bisa diambil user lain antara generate dan update);
    // galat DB = 500 agar tak fail-open menimpa email duplikat.
    let taken: Option<bool> = match sqlx::query_scalar("SELECT true FROM users WHERE email = $1 AND id <> $2")
        .bind(&email).bind(auth.0).fetch_optional(&st.pool).await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "update-email: duplicate-email recheck failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    if taken == Some(true) {
        return (StatusCode::BAD_REQUEST, plain_error("An account with this email already exists"));
    }
    let upd = sqlx::query("UPDATE users SET email = $1, is_email_verified = false, updated_at = now() WHERE id = $2")
        .bind(&email).bind(auth.0).execute(&st.pool).await;
    if upd.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap_or(());
    // Tanpa logout server-side: sesi Rust stateless (JWT ≤15 mnt, refresh
    // me-resolve uid → email baru berlaku otomatis); frontend sign-out sendiri.
    let back: Option<(String, String, String)> =
        match sqlx::query_as("SELECT email, first_name, last_name FROM users WHERE id = $1")
            .bind(auth.0).fetch_optional(&st.pool).await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "update-email: re-read after update failed");
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
            }
        };
    match back {
        Some((email, first, last)) => (StatusCode::OK, Json(json!({"id": auth.0, "email": email, "first_name": first, "last_name": last}))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "User not found"}))),
    }
}

/// Logo workspace: aset file diutamakan, fallback ke kolom `logo` lama.
pub fn pick_logo_url(asset: Option<&str>, logo: Option<&str>) -> Option<String> {
    asset.map(str::to_string).or_else(|| logo.map(str::to_string))
}

#[derive(sqlx::FromRow)]
struct MyWorkspaceRow {
    id: uuid::Uuid,
    name: String,
    slug: String,
    timezone: String,
    organization_size: Option<String>,
    logo: Option<String>,
    logo_asset_url: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    owner_id: uuid::Uuid,
    owner_email: String,
    owner_first: String,
    owner_last: String,
    role: i16,
    total_members: i64,
}

/// GET /api/users/me/workspaces/ — paritas `UserWorkSpacesEndpoint.get`
/// (workspace ber-membership aktif + anotasi role & total_members).
pub async fn my_workspaces(
    State(st): State<AppState>,
    auth: AuthUser,
) -> (StatusCode, Json<Value>) {
    // `file_assets` tidak punya kolom `asset_url` (skema aktual: `asset`,
    // varchar 800) — logo diambil dari `fa.asset`, fallback ke `w.logo`.
    // Field `url` DIHILANGKAN: tidak ada kolomnya di `workspaces` dan tidak
    // ada kode FE yang membaca properti `workspace.url`.
    let rows: Vec<MyWorkspaceRow> = match sqlx::query_as(
        "SELECT w.id, w.name, w.slug, w.timezone, w.organization_size, w.logo, \
                fa.asset AS logo_asset_url, \
                w.created_at, w.updated_at, w.created_by_id, w.updated_by_id, \
                o.id AS owner_id, o.email AS owner_email, \
                o.first_name AS owner_first, o.last_name AS owner_last, \
                wm.role AS role, \
                (SELECT COUNT(*) FROM workspace_members m JOIN users u ON u.id = m.member_id \
                  WHERE m.workspace_id = w.id AND m.is_active = true AND m.deleted_at IS NULL \
                    AND u.is_bot = false) AS total_members \
         FROM workspaces w \
         JOIN workspace_members wm ON wm.workspace_id = w.id \
           AND wm.member_id = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
         JOIN users o ON o.id = w.owner_id \
         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
         WHERE w.deleted_at IS NULL \
         ORDER BY w.created_at DESC",
    )
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "my-workspaces: lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "slug": r.slug,
                "timezone": r.timezone,
                "organization_size": r.organization_size.unwrap_or_default(),
                "logo_url": pick_logo_url(r.logo_asset_url.as_deref(), r.logo.as_deref()),
                "created_at": r.created_at,
                "updated_at": r.updated_at,
                "created_by": r.created_by_id,
                "updated_by": r.updated_by_id,
                "owner": {
                    "id": r.owner_id,
                    "email": r.owner_email,
                    "first_name": r.owner_first,
                    "last_name": r.owner_last,
                },
                "role": r.role,
                "total_members": r.total_members,
            })
        })
        .collect();
    (StatusCode::OK, Json(Value::Array(out)))
}

/// Link terima-undangan, byte-exact dari
/// `WorkSpaceMemberInviteSerializer.get_invite_link`.
pub fn workspace_invite_link(id: &str, slug: &str, token: &str) -> String {
    format!("/workspace-invitations/?invitation_id={id}&slug={slug}&token={token}")
}

#[derive(sqlx::FromRow)]
struct MyWorkspaceInviteRow {
    id: uuid::Uuid,
    email: String,
    accepted: bool,
    token: String,
    message: Option<String>,
    responded_at: Option<chrono::DateTime<chrono::Utc>>,
    role: i16,
    workspace_id: uuid::Uuid,
    workspace_name: String,
    workspace_slug: String,
    workspace_logo: Option<String>,
}

/// GET /api/users/me/workspaces/invitations/ — paritas
/// `UserWorkspaceInvitationsViewSet.list` (filter email user saat ini).
pub async fn my_workspace_invitations(
    State(st): State<AppState>,
    auth: AuthUser,
) -> (StatusCode, Json<Value>) {
    // Galat DB = 500; hanya user yang benar-benar hilang yang jadi 401.
    let email_row: Option<(String,)> = match sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "ws-invitations: current-email lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let Some((email,)) = email_row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let rows: Vec<MyWorkspaceInviteRow> = match sqlx::query_as(
        "SELECT i.id, i.email, i.accepted, i.token, i.message, i.responded_at, i.role, \
                w.id AS workspace_id, w.name AS workspace_name, w.slug AS workspace_slug, \
                w.logo AS workspace_logo \
         FROM workspace_member_invites i \
         JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL \
         WHERE i.email = $1 AND i.deleted_at IS NULL \
         ORDER BY i.created_at DESC",
    )
    .bind(&email)
    .fetch_all(&st.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "ws-invitations: lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let id = r.id.to_string();
            json!({
                "id": id,
                "email": r.email,
                "accepted": r.accepted,
                "token": r.token,
                "message": r.message,
                "responded_at": r.responded_at,
                "role": r.role,
                "workspace": {
                    "id": r.workspace_id,
                    "name": r.workspace_name,
                    "slug": r.workspace_slug,
                    "logo_url": pick_logo_url(None, r.workspace_logo.as_deref()),
                },
                "invite_link": workspace_invite_link(&id, &r.workspace_slug, &r.token),
            })
        })
        .collect();
    (StatusCode::OK, Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct JoinWorkspacesBody {
    pub invitations: Vec<uuid::Uuid>,
}

/// Default `view_props`/`default_props` ala Django `get_default_props()`.
fn default_view_props() -> Value {
    json!({
        "filters": {
            "priority": null, "state": null, "state_group": null,
            "assignees": null, "created_by": null, "labels": null,
            "start_date": null, "target_date": null, "subscriber": null,
        },
        "display_filters": {
            "group_by": null, "order_by": "-created_at", "type": null,
            "sub_issue": true, "show_empty_groups": true,
            "layout": "list", "calendar_date_range": "",
        },
        "display_properties": {
            "assignee": true, "attachment_count": true, "created_on": true,
            "due_date": true, "estimate": true, "key": true, "labels": true,
            "link": true, "priority": true, "start_date": true, "state": true,
            "sub_issue_count": true, "updated_on": true,
        },
    })
}

/// Default `issue_props` ala Django `get_issue_props()`.
fn default_issue_props() -> Value {
    json!({"subscribed": true, "assigned": true, "created": true, "all_issues": true})
}

/// POST /api/users/me/workspaces/invitations/ — paritas
/// `UserWorkspaceInvitationsViewSet.create`: aktifkan member nonaktif,
/// insert member baru (abaikan konflik), hapus invite — semua dalam SATU
/// transaksi. Sukses = 204 tanpa body.
pub async fn join_workspaces(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<JoinWorkspacesBody>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    // Galat DB = 500; hanya user yang benar-benar hilang yang jadi 401.
    let email_row: Option<(String,)> = match sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "ws-join: current-email lookup failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
    };
    let Some((email,)) = email_row else {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"}))));
    };
    if body.invitations.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!(error = %e, "ws-join: begin transaction failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
    };
    // Hanya invite milik email user ini (cermin `filter(pk__in=..., email=...)`).
    let invites: Vec<(uuid::Uuid, uuid::Uuid, i16)> = match sqlx::query_as(
        "SELECT i.id, i.workspace_id, i.role FROM workspace_member_invites i \
         WHERE i.id = ANY($1) AND i.email = $2 AND i.deleted_at IS NULL \
         ORDER BY i.created_at DESC",
    )
    .bind(&body.invitations)
    .bind(&email)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "ws-join: invite lookup failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
    };
    let view_props = default_view_props();
    let issue_props = default_issue_props();
    for (_, workspace_id, role) in &invites {
        // Aktifkan keanggotaan nonaktif (cermin `update(is_active=True, role=...)`).
        if sqlx::query(
            "UPDATE workspace_members SET is_active = true, role = $1, updated_at = now() \
             WHERE workspace_id = $2 AND member_id = $3",
        )
        .bind(role)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            tracing::warn!("ws-join: member activation failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
        // `ON CONFLICT DO NOTHING` tanpa target: cermin
        // `bulk_create(ignore_conflicts=True)` — kena constraint mana pun
        // ((workspace_id, member_id) parsial maupun (workspace_id,
        // member_id, deleted_at)) tetap diabaikan, tak perlu arbiter.
        if sqlx::query(
            "INSERT INTO workspace_members \
             (id, workspace_id, member_id, role, created_by_id, view_props, \
              default_props, issue_props, is_active, getting_started_checklist, \
              tips, explored_features, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $3, $5, $5, $6, true, '{}', '{}', '{}', now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(workspace_id)
        .bind(auth.0)
        .bind(role)
        .bind(&view_props)
        .bind(&issue_props)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            tracing::warn!("ws-join: member insert failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
    }
    let joined_ids: Vec<uuid::Uuid> = invites.into_iter().map(|(id, _, _)| id).collect();
    if sqlx::query("DELETE FROM workspace_member_invites WHERE id = ANY($1)")
        .bind(&joined_ids)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        tracing::warn!("ws-join: invite delete failed");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
    }
    if tx.commit().await.is_err() {
        tracing::warn!("ws-join: commit failed");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_code_throttle_key_format() {
        let uid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(email_code_throttle_key(&uid), "emailcode:throttle:11111111-1111-1111-1111-111111111111");
        assert_eq!(email_code_key(&uid, "a@b.co"), "emailcode:11111111-1111-1111-1111-111111111111:a@b.co");
    }

    #[test]
    fn six_digit_code_range() {
        for _ in 0..200 {
            let c = new_email_code();
            assert!(c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit()));
            let n: u32 = c.parse().expect("kode harus numerik");
            assert!((100000..=999999).contains(&n));
        }
    }

    #[test]
    fn email_update_clears_key_format() {
        // key yang dihapus setelah sukses harus sama dengan key generate
        let uid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(email_code_key(&uid, "n@x.io"), "emailcode:11111111-1111-1111-1111-111111111111:n@x.io");
    }

    #[test]
    fn logo_fallback_order() {
        assert_eq!(pick_logo_url(Some("a"), Some("b")).as_deref(), Some("a"));
        assert_eq!(pick_logo_url(None, Some("b")).as_deref(), Some("b"));
        assert_eq!(pick_logo_url(None, None), None);
    }

    #[test]
    fn invite_link_shape() {
        assert_eq!(workspace_invite_link("1", "s", "t"), "/workspace-invitations/?invitation_id=1&slug=s&token=t");
    }
}
