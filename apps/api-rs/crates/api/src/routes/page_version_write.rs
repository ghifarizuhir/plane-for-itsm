//! Page description-version writer — mirrors `track_page_version`
//! (`plane/bgtasks/page_version_task.py:22-81`): skip unchanged descriptions
//! (checked by the caller, `views/page/base.py:556-587`), merge into the
//! latest row when it is the same owner within 600 s, otherwise insert a
//! fresh snapshot; after both paths the page is trimmed to its 20 newest
//! live versions (the oldest is soft-deleted — `SoftDeletionManager`
//! excludes `deleted_at` rows, so the version endpoints drop it).
//!
//! Takes a plain `PgConnection` rather than a `Transaction`, so both a
//! caller-held transaction and a standalone connection work; callers with
//! a transaction pass `&mut tx`.

use serde_json::Value;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_page_version(
    conn: &mut sqlx::PgConnection,
    page_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    description_binary: Option<&[u8]>,
    description_html: &str,
    description_stripped: Option<&str>,
    description_json: &Value,
) -> Result<(), sqlx::Error> {
    // `PageVersion.objects` uses `SoftDeletionManager`
    // (`db/mixins.py:58-62`) → only live rows participate.
    let latest: Option<(Uuid, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, owned_by_id, last_saved_at FROM page_versions \
         WHERE page_id = $1 AND deleted_at IS NULL ORDER BY last_saved_at DESC LIMIT 1",
    )
    .bind(page_id)
    .fetch_optional(&mut *conn)
    .await?;

    if let Some((id, owned_by, last_saved_at)) = latest {
        // Django `total_seconds() <= 600` (`page_version_task.py:38-42`):
        // exact duration, not truncated seconds.
        if owned_by == actor && chrono::Utc::now() - last_saved_at <= chrono::Duration::seconds(600)
        {
            // `page_version_task.py:43-57`: the merge refreshes the snapshot
            // in place and keeps `last_saved_at` (not in `update_fields`);
            // `sub_pages_data` is reset to `{}`. `created_by_id` /
            // `updated_by_id` stay untouched (the task runs outside a
            // request, so `crum`'s current user is None and the audit
            // columns are never set).
            sqlx::query(
                "UPDATE page_versions SET description_binary = $1, description_html = $2, \
                 description_stripped = $3, description_json = $4, sub_pages_data = '{}'::jsonb, \
                 updated_at = now() WHERE id = $5",
            )
            .bind(description_binary)
            .bind(description_html)
            .bind(description_stripped)
            .bind(description_json.clone())
            .bind(id)
            .execute(&mut *conn)
            .await?;
            return trim_versions(conn, page_id).await;
        }
    }

    // `page_version_task.py:58-70`: fresh snapshot. `PageVersion.objects.create`
    // sets no audit columns — `BaseModel.save` only reads `crum`'s current
    // user when one is bound to the request (`db/models/base.py:20-38`).
    sqlx::query(
        "INSERT INTO page_versions (id, last_saved_at, description_binary, description_html, \
         description_stripped, description_json, sub_pages_data, owned_by_id, page_id, \
         workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), now(), $1, $2, $3, $4, '{}'::jsonb, $5, $6, $7, now(), now())",
    )
    .bind(description_binary)
    .bind(description_html)
    .bind(description_stripped)
    .bind(description_json.clone())
    .bind(actor)
    .bind(page_id)
    .bind(workspace_id)
    .execute(&mut *conn)
    .await?;
    trim_versions(conn, page_id).await
}

/// `page_version_task.py:71-74`: when a page holds more than 20 live
/// versions, soft-delete the oldest (`last_saved_at` asc). Django's
/// `SoftDeletionQuerySet.delete` (`db/mixins.py:41-44`) sets `deleted_at`
/// instead of removing the row, and `save()` bumps `updated_at` (`auto_now`).
async fn trim_versions(conn: &mut sqlx::PgConnection, page_id: Uuid) -> Result<(), sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM page_versions WHERE page_id = $1 AND deleted_at IS NULL",
    )
    .bind(page_id)
    .fetch_one(&mut *conn)
    .await?;
    if count > 20 {
        sqlx::query(
            "UPDATE page_versions SET deleted_at = now(), updated_at = now() WHERE id = \
             (SELECT id FROM page_versions WHERE page_id = $1 AND deleted_at IS NULL \
              ORDER BY last_saved_at ASC LIMIT 1)",
        )
        .bind(page_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
