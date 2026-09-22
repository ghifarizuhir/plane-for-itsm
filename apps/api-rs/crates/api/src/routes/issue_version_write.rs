//! Description-version writer — mirrors `issue_description_version_task`
//! (`plane/bgtasks/issue_description_version_task.py:44-80`): skip unchanged
//! descriptions, merge into the latest row when it is the same owner within
//! 600 s, otherwise insert a fresh snapshot.

use serde_json::Value;
use uuid::Uuid;

use super::page::strip_tags_text;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_description_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    issue_created_by_id: Option<Uuid>,
    issue_updated_by_id: Option<Uuid>,
    description_html: &str,
    description_json: &Value,
) -> Result<(), sqlx::Error> {
    let latest: Option<(Uuid, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, owned_by_id, last_saved_at FROM issue_description_versions \
         WHERE issue_id = $1 ORDER BY last_saved_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(&mut **tx)
    .await?;

    let stripped: Option<String> = if description_html.is_empty() {
        None
    } else {
        Some(strip_tags_text(description_html))
    };

    if let Some((id, owned_by, last_saved_at)) = latest {
        // Django `total_seconds() <= 600` (`issue_description_version_task.py:21-22`):
        // exact duration, not truncated seconds.
        if owned_by == actor && chrono::Utc::now() - last_saved_at <= chrono::Duration::seconds(600)
        {
            // Django `update_existing_version` copies `issue.description_binary`,
            // which is always NULL here (no Rust path writes that column) —
            // revisit if a binary writer is ever added.
            sqlx::query(
                "UPDATE issue_description_versions SET description_binary = NULL, description_html = $1, \
                 description_stripped = $2, description_json = $3, last_saved_at = now() WHERE id = $4",
            )
            .bind(description_html)
            .bind(stripped)
            .bind(description_json.clone())
            .bind(id)
            .execute(&mut **tx)
            .await?;
            return Ok(());
        }
    }

    sqlx::query(
        "INSERT INTO issue_description_versions (id, description_binary, description_html, \
         description_stripped, description_json, last_saved_at, owned_by_id, issue_id, project_id, \
         workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), NULL, $1, $2, $3, now(), $4, $5, $6, $7, $8, $9, now(), now())",
    )
    .bind(description_html)
    .bind(stripped)
    .bind(description_json.clone())
    .bind(actor)
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(issue_created_by_id)
    .bind(issue_updated_by_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
