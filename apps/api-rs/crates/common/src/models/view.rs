use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IssueView {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UserFavorite {
    pub id: uuid::Uuid,
    pub entity_identifier: Option<uuid::Uuid>,
}
