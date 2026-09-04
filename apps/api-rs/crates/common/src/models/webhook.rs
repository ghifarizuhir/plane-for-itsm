use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Webhook {
    pub id: uuid::Uuid,
    pub url: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WebhookLog {
    pub id: uuid::Uuid,
    pub event_type: Option<String>,
}
