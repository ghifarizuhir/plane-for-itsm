use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Project {
    pub id: uuid::Uuid,
    pub name: String,
}
