use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Intake {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IntakeIssue {
    pub id: uuid::Uuid,
    pub status: i32,
}
