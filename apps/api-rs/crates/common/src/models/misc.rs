use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ApiToken {
    pub id: uuid::Uuid,
    pub label: String,
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
