use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Workspace {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
}
