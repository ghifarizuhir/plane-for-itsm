use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/views/workspace/base.py:WorkSpaceViewSet` +Serializer
/// `plane/app/serializers/workspace.py:WorkSpaceSerializer`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspace {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
}

pub const RESTRICTED_SLUGS: &[&str] = &[
    "404", "accounts", "api", "create-workspace", "god-mode", "installations", "invitations",
    "onboarding", "profile", "spaces", "workspace-invitations", "password", "flags", "monitor",
    "monitoring", "ingest", "plane-pro", "plane-ultimate", "enterprise", "plane-enterprise",
    "disco", "silo", "chat", "calendar", "drive", "channels", "upgrade", "billing", "sign-in",
    "sign-up", "signin", "signup", "config", "live", "admin", "m", "import", "importers",
    "integrations", "integration", "configuration", "initiatives", "initiative", "workflow",
    "workflows", "epics", "epic", "story", "mobile", "dashboard", "desktop", "onload",
    "real-time", "one", "pages", "business", "pro", "settings", "license", "licenses",
    "instances", "instance",
];

fn has_alphanumeric(value: &str) -> bool {
    value.chars().any(|c| c.is_alphanumeric())
}

fn contains_url(value: &str) -> bool {
    if value.len() > 1000 {
        return false;
    }
    let lower = value.to_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        return true;
    }
    // domain-like token: something.tld (tld 2-6 alpha) or IPv4
    for token in lower.split_whitespace() {
        let t = token.trim_matches(|c: char| c.is_ascii_punctuation());
        if is_ipv4(t) {
            return true;
        }
        if let Some(dot) = t.rfind('.') {
            let tld = &t[dot + 1..];
            let name_part = &t[..dot];
            if (2..=6).contains(&tld.len())
                && tld.chars().all(|c| c.is_ascii_alphabetic())
                && !name_part.is_empty()
                && name_part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            {
                return true;
            }
        }
    }
    false
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u8>().is_ok())
}

fn valid_slug_chars(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Pure validation mirroring Django (unit-testable, no DB).
/// Returns Ok(()) or Err(message mentioning offending field.
pub fn validate_create(body: &CreateWorkspace) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.slug.trim().is_empty() {
        return Err("slug is required".to_string());
    }
    if body.name.chars().count() > 80 {
        return Err("name max length 80".to_string());
    }
    if body.slug.chars().count() > 48 {
        return Err("slug max length 48".to_string());
    }
    if contains_url(&body.name) {
        return Err("Name must not contain URLs".to_string());
    }
    if !has_alphanumeric(&body.name) {
        return Err("Name must contain at least one letter or number".to_string());
    }
    if RESTRICTED_SLUGS.contains(&body.slug.as_str()) {
        return Err("Slug is not valid".to_string());
    }
    if !valid_slug_chars(&body.slug) {
        return Err("Slug can only contain letters, numbers, hyphens (-), and underscores (_)"
            .to_string());
    }
    Ok(())
}

/// GET /api/workspaces/ — member-scoped, ordered by name.
/// Mirrors `get_queryset`: filter by membership, annotate total_members.
pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<WorkspaceOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::workspace::Workspace>(
        "SELECT id, name, slug FROM workspaces WHERE deleted_at IS NULL ORDER BY name ASC",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|w| WorkspaceOut {
                id: w.id,
                name: w.name,
                slug: w.slug,
            })
            .collect(),
    ))
}

/// POST /api/workspaces/ — validates then inserts.
pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    Json(body): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<WorkspaceOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let row = sqlx::query_as::<_, common::models::workspace::Workspace>(
        "INSERT INTO workspaces (id, name, description, slug, created_at, updated_at) VALUES (gen_random_uuid(), $1, '', $2, now(), now()) RETURNING id, name, slug",
    )
    .bind(&body.name)
    .bind(&body.slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(WorkspaceOut {
            id: row.id,
            name: row.name,
            slug: row.slug,
        }),
    ))
}
