use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Estimate {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct EstimatePoint {
    pub id: uuid::Uuid,
    pub key: i32,
    pub value: String,
}
