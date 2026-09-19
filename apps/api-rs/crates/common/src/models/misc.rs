use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// Full `api_tokens` row minus the secret `token` column, mirroring Django's
/// `APITokenReadSerializer` (`exclude = ("token",)`). The `token` column is only
/// ever returned by the create endpoint.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub label: String,
    pub description: String,
    pub last_used: Option<DateTime<Utc>>,
    pub user_type: i16,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
    pub user_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub expired_at: Option<DateTime<Utc>>,
    pub is_service: bool,
    pub allowed_rate_limit: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Sticky {
    pub id: uuid::Uuid,
    pub name: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExporterHistory {
    pub id: uuid::Uuid,
    pub provider: String,
}
