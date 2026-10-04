//! Knowledge base (pages/articles) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature,
};
use crate::agent::{record, ToolTrace};
use crate::tools::work_items::truncate_chars;

pub const DESCRIPTION_MAX: usize = 8000;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ArticleSearchRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub updated_at: String,
    pub snippet: String,
}

pub const ARTICLE_SEARCH_SQL: &str = "SELECT pa.id, pa.name, \
     p.identifier AS project, pa.updated_at::text AS updated_at, \
     left(COALESCE(pa.description_stripped, ''), 200) AS snippet \
     FROM pages pa \
     JOIN project_pages pp ON pp.page_id = pa.id AND pp.deleted_at IS NULL \
     JOIN projects p ON p.id = pp.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     WHERE pa.workspace_id = $1 AND pa.deleted_at IS NULL AND pa.archived_at IS NULL \
     AND p.page_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (pa.access = 0 OR pa.owned_by_id = $2) \
     AND ($3::text IS NULL OR p.identifier ILIKE $3 OR p.name ILIKE '%' || $3 || '%') \
     AND (pa.name ILIKE '%' || $4 || '%' \
       OR COALESCE(pa.description_stripped, '') ILIKE '%' || $4 || '%') \
     AND ($5::uuid IS NULL OR p.id = $5) \
     ORDER BY pa.updated_at DESC LIMIT $6";

pub fn articles_json(rows: &[ArticleSearchRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "name": row.name,
                "project": row.project,
                "updated_at": row.updated_at,
                "snippet": row.snippet,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchArticlesArgs {
    /// Case-insensitive substring matched against article names and bodies. Required.
    pub query: String,
    /// Project identifier or name to scope the search.
    pub project: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct SearchArticles {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for SearchArticles {
    const NAME: &'static str = "search_articles";
    type Args = SearchArticlesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Search knowledge base articles (pages) the caller can access by a \
         substring in the title or body, optionally scoped to a project. \
         Returns id, name, project, updated_at, and a 200-character snippet. \
         Private articles are only visible to their owner. Use the returned id \
         with get_article."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<SearchArticlesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(Some(args.query.as_str()))
            .ok_or_else(|| ToolExecutionError::invalid_args("query is required"))?;
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                ensure_feature(&self.pool, project.id, ProjectFeature::Pages).await?;
                Some(project)
            }
            None => None,
        };
        let limit = clamp_limit(args.limit);
        let rows: Vec<ArticleSearchRow> = sqlx::query_as(ARTICLE_SEARCH_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(None::<String>)
            .bind(&query)
            .bind(project.as_ref().map(|row| row.id))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(articles_json(&rows))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ArticleDetailRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub access: i16,
    pub page_view: bool,
    pub description: String,
}

pub const ARTICLE_DETAIL_SQL: &str = "SELECT pa.id, pa.name, \
     p.identifier AS project, pa.access, p.page_view AS page_view, \
     COALESCE(pa.description_stripped, '') AS description \
     FROM pages pa \
     JOIN project_pages pp ON pp.page_id = pa.id AND pp.deleted_at IS NULL \
     JOIN projects p ON p.id = pp.project_id AND p.deleted_at IS NULL \
     WHERE pa.id = $1 AND pa.workspace_id = $2 AND pa.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (pa.access = 0 OR pa.owned_by_id = $3) \
     LIMIT 1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct LinkedIssueRow {
    pub identifier: String,
    pub name: String,
}

pub const ARTICLE_SUBPAGES_SQL: &str = "SELECT name FROM pages \
     WHERE parent_id = $1 AND deleted_at IS NULL ORDER BY name";

pub const ARTICLE_LABELS_SQL: &str = "SELECT l.name FROM page_labels pl \
     JOIN labels l ON l.id = pl.label_id AND l.deleted_at IS NULL \
     WHERE pl.page_id = $1 AND pl.deleted_at IS NULL ORDER BY l.name";

pub const ARTICLE_LINKED_ISSUES_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name \
     FROM page_logs l \
     JOIN issues i ON i.id = l.entity_identifier AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     WHERE l.page_id = $1 AND l.entity_name = 'issue' AND l.deleted_at IS NULL \
     ORDER BY identifier LIMIT 10";

pub fn article_detail_json(
    row: &ArticleDetailRow,
    sub_pages: &[String],
    labels: &[String],
    linked_issues: &[LinkedIssueRow],
) -> String {
    json!({
        "id": row.id,
        "name": row.name,
        "project": row.project,
        "description": truncate_chars(&row.description, DESCRIPTION_MAX),
        "sub_pages": sub_pages,
        "labels": labels,
        "linked_issues": linked_issues,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetArticleArgs {
    /// Article id (uuid) as returned by search_articles.
    pub article: String,
}

pub struct GetArticle {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetArticle {
    const NAME: &'static str = "get_article";
    type Args = GetArticleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one knowledge base article by id: body text (truncated), \
         sub-pages, labels, and the work items linked to it. Private articles \
         are only visible to their owner."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetArticleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let article = Uuid::parse_str(args.article.trim()).map_err(|_| {
            ToolExecutionError::invalid_args("article must be a uuid from search_articles")
        })?;
        let row: Option<ArticleDetailRow> = sqlx::query_as(ARTICLE_DETAIL_SQL)
            .bind(article)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Err(ToolExecutionError::invalid_args(format!(
                "article '{}' was not found or is not accessible",
                args.article
            )));
        };
        if !row.page_view {
            return Err(ToolExecutionError::invalid_args(
                "the knowledge base feature is disabled in this project",
            ));
        }
        let sub_pages: Vec<String> = sqlx::query_scalar(ARTICLE_SUBPAGES_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let labels: Vec<String> = sqlx::query_scalar(ARTICLE_LABELS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let linked_issues: Vec<LinkedIssueRow> = sqlx::query_as(ARTICLE_LINKED_ISSUES_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(article_detail_json(&row, &sub_pages, &labels, &linked_issues))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn article_search_json_shape() {
        let rows = vec![ArticleSearchRow {
            id: Uuid::nil(),
            name: "Runbook: restart API".to_string(),
            project: "LTS".to_string(),
            updated_at: "2026-10-01 00:00:00+00".to_string(),
            snippet: "How to restart the API".to_string(),
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&articles_json(&rows)).unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["name"], json!("Runbook: restart API"));
        assert_eq!(parsed["items"][0]["snippet"], json!("How to restart the API"));
    }

    #[test]
    fn article_detail_json_shape() {
        let row = ArticleDetailRow {
            id: Uuid::nil(),
            name: "Runbook: restart API".to_string(),
            project: "LTS".to_string(),
            access: 0,
            page_view: true,
            description: "x".repeat(9000),
        };
        let issues = vec![LinkedIssueRow {
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
        }];
        let parsed: serde_json::Value = serde_json::from_str(&article_detail_json(
            &row,
            &["Sub page".to_string()],
            &["runbook".to_string()],
            &issues,
        ))
        .unwrap();
        assert_eq!(parsed["name"], json!("Runbook: restart API"));
        assert_eq!(parsed["sub_pages"][0], json!("Sub page"));
        assert_eq!(parsed["labels"][0], json!("runbook"));
        assert_eq!(parsed["linked_issues"][0]["identifier"], json!("LTS-42"));
        assert_eq!(
            parsed["description"].as_str().unwrap().chars().count(),
            DESCRIPTION_MAX
        );
    }

    #[tokio::test]
    async fn search_articles_requires_a_query() {
        let tool = SearchArticles {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                SearchArticlesArgs {
                    query: "  ".to_string(),
                    project: None,
                    limit: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("query"));
    }

    #[tokio::test]
    async fn get_article_rejects_a_non_uuid_before_querying() {
        let tool = GetArticle {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                GetArticleArgs {
                    article: "Runbook".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("uuid"));
    }

    #[test]
    fn kb_sql_respects_private_access_and_membership() {
        for sql in [ARTICLE_SEARCH_SQL, ARTICLE_DETAIL_SQL] {
            assert!(sql.contains("workspace_id = $1") || sql.contains("workspace_id = $2"));
            assert!(sql.contains("project_members"));
            assert!(sql.contains("pa.access = 0 OR pa.owned_by_id"));
        }
        assert!(ARTICLE_SEARCH_SQL.contains("p.page_view = true"));
    }
}
