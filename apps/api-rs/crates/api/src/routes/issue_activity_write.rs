//! Create- and update-path writers for `issue_activities` and
//! `issue_subscribers`, mirroring Django's `issue_activities_task` create and
//! update flows (`plane/bgtasks/issue_activities_task.py:357-638`).

use uuid::Uuid;

/// Context shared by the update-path activity rows.
pub(crate) struct ActivityCtx {
    pub issue_id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub actor: Uuid,
    pub epoch: f64,
}

/// One `verb`/`field` activity row for the update flow. `created_by_id` and
/// `updated_by_id` stay NULL (Django `bulk_create` skips `save()`);
/// `attachments` is `'{}'` (ArrayField default). Timestamps use
/// `clock_timestamp()` (per-statement) instead of `now()` (transaction
/// start) so batch rows get distinct, insertion-ordered `created_at` like
/// Django's per-instance defaults — `merge_last_description_activity` and
/// the activity feed depend on that ordering.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_activity_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &ActivityCtx,
    verb: &str,
    field: &str,
    comment: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
    old_identifier: Option<Uuid>,
    new_identifier: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_activities (id, verb, field, old_value, new_value, comment, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, updated_by_id, \
         old_identifier, new_identifier, epoch, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, '{}'::varchar[], $6, $7, $8, $9, NULL, NULL, \
         $10, $11, $12, clock_timestamp(), clock_timestamp())",
    )
    .bind(verb)
    .bind(field)
    .bind(old_value)
    .bind(new_value)
    .bind(comment)
    .bind(ctx.issue_id)
    .bind(ctx.project_id)
    .bind(ctx.workspace_id)
    .bind(ctx.actor)
    .bind(old_identifier)
    .bind(new_identifier)
    .bind(ctx.epoch)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

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
                '{}'::varchar[], $1, $2, $3, $4, NULL, NULL, u.id, $5, clock_timestamp(), clock_timestamp() \
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
