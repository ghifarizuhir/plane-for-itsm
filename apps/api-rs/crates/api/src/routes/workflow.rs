//! Workflows workspace-level (state + transisi) dan materialization state ke
//! project. Spec: docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md

use uuid::Uuid;

/// Group valid untuk workflow state; `triage` bukan bagian workflow
/// (`StateGroup` di `apps/api/plane/db/models/state.py:14-20`).
/// Alias ke `state::ALLOWED_GROUPS` agar daftarnya tidak pernah menyimpang.
pub const STATE_GROUPS: &[&str] = super::state::ALLOWED_GROUPS;

/// Validasi nama ala DRF (required, <=255). Mengembalikan nama ter-trim.
pub fn validate_name(name: &str, field: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.chars().count() > 255 {
        return Err(format!("Ensure {field} has no more than 255 characters."));
    }
    Ok(trimmed.to_string())
}

pub fn validate_state_group(group: &str) -> Result<(), String> {
    if STATE_GROUPS.contains(&group) {
        Ok(())
    } else {
        Err(format!("\"{group}\" is not a valid choice."))
    }
}

/// Target state (mirror) yang diizinkan dari `current_state`, dihitung murni
/// dari pasangan `(state_id, workflow_state_id)` dan daftar transisi
/// `(from_workflow_state_id, to_workflow_state_id)`. Urutan output mengikuti
/// urutan `mirror_pairs` dan tidak di-sort lebih lanjut.
pub fn allowed_target_state_ids(
    current_state: Uuid,
    mirror_pairs: &[(Uuid, Uuid)],
    transitions: &[(Uuid, Uuid)],
) -> Vec<Uuid> {
    let Some(current_workflow_state) = mirror_pairs
        .iter()
        .find(|(state_id, _)| *state_id == current_state)
        .map(|(_, workflow_state_id)| *workflow_state_id)
    else {
        return Vec::new();
    };
    let allowed_workflow_states: Vec<Uuid> = transitions
        .iter()
        .filter(|(from, _)| *from == current_workflow_state)
        .map(|(_, to)| *to)
        .collect();
    mirror_pairs
        .iter()
        .filter(|(_, workflow_state_id)| allowed_workflow_states.contains(workflow_state_id))
        .map(|(state_id, _)| *state_id)
        .collect()
}

/// Materialize seluruh state workflow milik `type_id` ke project (idempotent).
/// Return jumlah mirror yang diproses (update + insert), bukan jumlah baris
/// yang benar-benar berubah.
pub async fn materialize_type_states(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let workflow_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT workflow_id FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(type_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let Some(workflow_id) = workflow_id else {
        return Ok(0);
    };
    materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await
}

/// Inti materialization: turunkan default lama, soft-delete mirror usang,
/// revive + update mirror, lalu insert mirror baru. Satu transaksi. Return
/// jumlah mirror yang diproses (update + insert), bukan jumlah baris yang
/// benar-benar berubah.
pub(crate) async fn materialize_workflow_for_project(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
    workflow_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 1. Turunkan default lama lebih dulu agar partial-unique default
    //    (project_id, type_id) tidak bentrok saat mirror baru di-insert.
    sqlx::query(
        "UPDATE states SET \"default\" = false, updated_at = now() \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL AND \"default\" = true",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;

    // 2. Soft-delete mirror usang: workflow_state-nya sudah dihapus ATAU
    //    milik workflow lain (type pindah workflow). Harus sebelum insert,
    //    kalau tidak nama state lama menabrak partial-unique
    //    (project_id, type_id, name) saat nama state baru sama.
    sqlx::query(
        "UPDATE states s SET deleted_at = now(), updated_at = now() \
         WHERE s.project_id = $1 AND s.type_id = $2 AND s.deleted_at IS NULL \
         AND s.workflow_state_id IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM workflow_states ws \
                         WHERE ws.id = s.workflow_state_id AND ws.deleted_at IS NULL \
                           AND ws.workflow_id = $3)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?;

    // 3. Revive + update mirror (termasuk mirror workflow ini yang sempat
    //    soft-deleted).
    let updated = sqlx::query(
        "UPDATE states s SET name = ws.name, description = ws.description, color = ws.color, \
         slug = ws.slug, sequence = ws.sequence, \"group\" = ws.\"group\", \"default\" = ws.is_default, \
         deleted_at = NULL, updated_at = now() \
         FROM workflow_states ws \
         WHERE s.workflow_state_id = ws.id AND s.project_id = $1 AND s.type_id = $2 \
           AND ws.workflow_id = $3 AND ws.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // 4. Insert mirror baru untuk workflow_state yang belum punya mirror.
    let inserted = sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, workflow_state_id, created_at, updated_at) \
         SELECT gen_random_uuid(), ws.name, ws.description, ws.color, ws.slug, ws.sequence, ws.\"group\", \
         false, ws.is_default, $1, p.workspace_id, $2, ws.id, now(), now() \
         FROM workflow_states ws JOIN projects p ON p.id = $1 \
         WHERE ws.workflow_id = $3 AND ws.deleted_at IS NULL \
         AND NOT EXISTS (SELECT 1 FROM states s WHERE s.project_id = $1 AND s.workflow_state_id = ws.id)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(updated + inserted)
}

/// Sync satu workflow ke semua project hidup yang mengaktifkan type-nya.
pub(crate) async fn sync_workflow_to_projects(
    pool: &sqlx::PgPool,
    workflow_id: Uuid,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT DISTINCT pit.project_id, t.id FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         JOIN projects p ON p.id = pit.project_id \
         WHERE t.workflow_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND p.deleted_at IS NULL AND p.archived_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_all(pool)
    .await?;
    for (project_id, type_id) in rows {
        materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await?;
    }
    Ok(())
}

/// Pastikan semua type yang aktif di project sudah ter-materialize.
pub(crate) async fn ensure_project_workflows(
    pool: &sqlx::PgPool,
    project_id: Uuid,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT pit.issue_type_id, t.workflow_id FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND t.workflow_id IS NOT NULL",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    for (type_id, workflow_id) in rows {
        materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await?;
    }
    Ok(())
}
