use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IssueComment {
    pub id: uuid::Uuid,
    pub comment_html: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IssueLink {
    pub id: uuid::Uuid,
    pub url: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IssueRelation {
    pub id: uuid::Uuid,
    pub related_issue_id: uuid::Uuid,
    pub relation_type: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IssueActivity {
    pub id: uuid::Uuid,
    pub verb: String,
}
