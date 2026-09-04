use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Label {
    pub id: uuid::Uuid,
    pub name: String,
}
