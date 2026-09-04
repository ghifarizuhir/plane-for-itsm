use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct FileAsset {
    pub id: uuid::Uuid,
    pub workspace_id: Option<uuid::Uuid>,
    pub project_id: Option<uuid::Uuid>,
    pub entity_type: Option<String>,
    pub is_uploaded: bool,
}
