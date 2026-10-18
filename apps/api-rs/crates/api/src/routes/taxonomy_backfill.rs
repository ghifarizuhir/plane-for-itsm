//! Program P1/S3: workspace-level taxonomy backfill helpers.
//!
//! Per-workspace, idempotent versions of the `0022_taxonomy_workspace.sql`
//! union logic. Used by tests; S6/S7/S8 call `backfill_labels` at their
//! cutovers to absorb interim project-level edits (no dual-write).
//! The migration file keeps its own self-contained full-DB copy —
//! migrations must never import crate code. Duplication is intentional.

use sqlx::PgPool;
use uuid::Uuid;

/// Union duplicate labels per (workspace, lower(name)): remap issue_labels
/// and hierarchical parents to the canonical (oldest) row, soft-delete
/// dupes, and null surviving `project_id`s.
pub async fn backfill_labels(pool: &PgPool, ws: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE issue_labels il SET label_id = m.canonical_id, updated_at = now() \
         FROM (SELECT id AS old_id, FIRST_VALUE(id) OVER w AS canonical_id \
               FROM labels WHERE deleted_at IS NULL AND workspace_id = $1 \
               WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)) m \
         WHERE il.label_id = m.old_id AND il.label_id <> m.canonical_id AND il.deleted_at IS NULL",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE labels ch SET parent_id = m.canonical_id, updated_at = now() \
         FROM (SELECT id AS old_id, FIRST_VALUE(id) OVER w AS canonical_id \
               FROM labels WHERE deleted_at IS NULL AND workspace_id = $1 \
               WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)) m \
         WHERE ch.parent_id = m.old_id AND ch.parent_id <> m.canonical_id AND ch.deleted_at IS NULL",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE labels l SET deleted_at = now(), updated_at = now() \
         WHERE l.deleted_at IS NULL AND l.workspace_id = $1 AND l.id NOT IN ( \
             SELECT DISTINCT FIRST_VALUE(id) OVER w FROM labels \
             WHERE deleted_at IS NULL AND workspace_id = $1 \
             WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id))",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE labels SET project_id = NULL, updated_at = now() \
         WHERE deleted_at IS NULL AND workspace_id = $1 AND project_id IS NOT NULL",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    Ok(())
}

/// Merge duplicate estimates per (workspace, name, type): union points by
/// value, remap issue references, soft-delete dupes, enforce a single
/// `last_used` per workspace, and point `workspace_settings.estimate_id`
/// at the canonical estimate (NULL when the workspace has none).
pub async fn backfill_estimates(pool: &PgPool, ws: Uuid) -> Result<(), sqlx::Error> {
    // Ensure a settings row exists (scratch workspaces created after the
    // migrations have none yet; GET auto-ensure covers this in production).
    sqlx::query(
        "INSERT INTO workspace_settings (id, workspace_id) VALUES (gen_random_uuid(), $1) \
         ON CONFLICT (workspace_id) DO NOTHING",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    let ranked_cte = "WITH ranked AS (SELECT e.id, e.workspace_id, lower(btrim(e.name)) AS norm_name, \
         e.type, COUNT(i.id) AS use_count, e.created_at, \
         ROW_NUMBER() OVER (PARTITION BY e.workspace_id, lower(btrim(e.name)), e.type \
         ORDER BY COUNT(i.id) DESC, MIN(e.created_at), MIN(e.id::text)) AS rn \
         FROM estimates e \
         LEFT JOIN estimate_points ep ON ep.estimate_id = e.id AND ep.deleted_at IS NULL \
         LEFT JOIN issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL \
         WHERE e.deleted_at IS NULL AND e.workspace_id = $1 \
         GROUP BY e.id, e.workspace_id, e.name, e.type, e.created_at), \
         canon AS (SELECT id AS canonical_id, workspace_id, norm_name, type FROM ranked WHERE rn = 1), \
         dupes AS (SELECT id AS dupe_id, workspace_id, norm_name, type FROM ranked WHERE rn > 1)";
    sqlx::query(&format!(
        "{ranked_cte} UPDATE estimate_points ep SET estimate_id = c.canonical_id, updated_at = now() \
         FROM dupes d JOIN canon c \
           ON c.workspace_id = d.workspace_id AND c.norm_name = d.norm_name AND c.type = d.type \
         WHERE ep.estimate_id = d.dupe_id AND ep.deleted_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM estimate_points cp \
             WHERE cp.estimate_id = c.canonical_id AND cp.deleted_at IS NULL AND cp.value = ep.value)"
    ))
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        "{ranked_cte} UPDATE issues i SET estimate_point_id = cp.id, updated_at = now() \
         FROM estimate_points ep JOIN dupes d ON d.dupe_id = ep.estimate_id \
         JOIN canon c ON c.workspace_id = d.workspace_id AND c.norm_name = d.norm_name AND c.type = d.type \
         JOIN estimate_points cp \
           ON cp.estimate_id = c.canonical_id AND cp.deleted_at IS NULL AND cp.value = ep.value \
         WHERE i.estimate_point_id = ep.id AND i.deleted_at IS NULL AND ep.deleted_at IS NULL"
    ))
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        "{ranked_cte} UPDATE estimate_points ep SET deleted_at = now(), updated_at = now() \
         WHERE ep.deleted_at IS NULL AND ep.estimate_id IN (SELECT dupe_id FROM dupes) \
           AND NOT EXISTS (SELECT 1 FROM issues i WHERE i.estimate_point_id = ep.id AND i.deleted_at IS NULL)"
    ))
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "WITH ranked AS (SELECT e.id, ROW_NUMBER() OVER (PARTITION BY e.workspace_id, \
         lower(btrim(e.name)), e.type ORDER BY (SELECT COUNT(*) FROM estimate_points ep \
         JOIN issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL \
         WHERE ep.estimate_id = e.id AND ep.deleted_at IS NULL) DESC, e.created_at, e.id::text) AS rn \
         FROM estimates e WHERE e.deleted_at IS NULL AND e.workspace_id = $1) \
         UPDATE estimates e SET deleted_at = now(), updated_at = now() \
         WHERE e.id IN (SELECT id FROM ranked WHERE rn > 1) AND e.deleted_at IS NULL",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE estimates SET last_used = false, updated_at = now() \
         WHERE deleted_at IS NULL AND workspace_id = $1 AND last_used IS DISTINCT FROM false",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "WITH canon AS (SELECT e.id, ROW_NUMBER() OVER (ORDER BY (SELECT COUNT(*) FROM estimate_points ep \
         JOIN issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL \
         WHERE ep.estimate_id = e.id AND ep.deleted_at IS NULL) DESC, e.created_at, e.id::text) AS rn \
         FROM estimates e WHERE e.deleted_at IS NULL AND e.workspace_id = $1) \
         UPDATE estimates e SET last_used = true, updated_at = now() \
         FROM canon c WHERE c.id = e.id AND c.rn = 1",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE workspace_settings ws SET estimate_id = sub.canonical_id, updated_at = now() \
         FROM (SELECT DISTINCT ON (e.workspace_id) e.workspace_id, e.id AS canonical_id \
         FROM estimates e WHERE e.deleted_at IS NULL AND e.last_used = true AND e.workspace_id = $1 \
         ORDER BY e.workspace_id) sub WHERE sub.workspace_id = ws.workspace_id",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE workspace_settings SET estimate_id = NULL, updated_at = now() \
         WHERE workspace_id = $1 AND estimate_id IS NOT NULL AND NOT EXISTS ( \
         SELECT 1 FROM estimates e WHERE e.id = workspace_settings.estimate_id AND e.deleted_at IS NULL)",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE estimates SET project_id = NULL, updated_at = now() \
         WHERE deleted_at IS NULL AND workspace_id = $1 AND project_id IS NOT NULL",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE estimate_points SET project_id = NULL, updated_at = now() \
         WHERE deleted_at IS NULL AND workspace_id = $1 AND project_id IS NOT NULL",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    Ok(())
}
