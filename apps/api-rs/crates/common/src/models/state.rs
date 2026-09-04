use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct State {
    pub id: uuid::Uuid,
    pub name: String,
    pub group: String,
}
