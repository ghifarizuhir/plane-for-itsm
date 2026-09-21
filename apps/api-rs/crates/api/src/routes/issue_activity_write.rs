//! Create-path writers for `issue_activities` and `issue_subscribers`,
//! mirroring Django's `issue_activities_task` create flow
//! (`plane/bgtasks/issue_activities_task.py:357-591`).

use uuid::Uuid;

/// One `verb="created"` row. `actor_id` is the issue creator;
/// `created_by_id` follows Django's `objects.create()` (crum user),
/// `updated_by_id` stays NULL (BaseModel create semantics).
pub(crate) async fn insert_created_activity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    epoch: f64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_activities (id, verb, field, old_value, new_value, comment, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, updated_by_id, epoch, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), 'created', NULL, NULL, NULL, 'created the issue', '{}'::varchar[], \
                $1, $2, $3, $4, $4, NULL, $5, i.created_at, now() \
         FROM issues i WHERE i.id = $1",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(actor)
    .bind(epoch)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One `verb="updated"`, `field="assignees"` "added" row per requested
/// assignee. `created_by_id`/`updated_by_id` stay NULL, matching Django's
/// `bulk_create` path which skips `save()`.
pub(crate) async fn insert_assignee_activities(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    added: &[Uuid],
    epoch: f64,
) -> Result<(), sqlx::Error> {
    if added.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO issue_activities (id, verb, field, old_value, new_value, comment, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, updated_by_id, new_identifier, \
         epoch, created_at, updated_at) \
         SELECT gen_random_uuid(), 'updated', 'assignees', '', u.display_name, 'added assignee ', \
                '{}'::varchar[], $1, $2, $3, $4, NULL, NULL, u.id, $5, now(), now() \
         FROM users u WHERE u.id = ANY($6)",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(actor)
    .bind(epoch)
    .bind(added)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Subscriber rows for added assignees (`ignore_conflicts=True` parity).
/// Django authors each row as the assignee, not the actor.
pub(crate) async fn insert_subscribers(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    subscriber_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if subscriber_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO issue_subscribers (id, issue_id, subscriber_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, u.id, $2, $3, u.id, u.id, now(), now() \
         FROM users u WHERE u.id = ANY($4) \
         ON CONFLICT DO NOTHING",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(subscriber_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
