use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ProjectMember {
    pub id: uuid::Uuid,
    pub member_id: Option<uuid::Uuid>,
    pub role: i16,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkspaceInvite {
    pub id: uuid::Uuid,
    pub email: String,
    pub role: i16,
}
