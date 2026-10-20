//! Workspace-level service access (design 2026-10-17): visibility sets and
//! role gates shared by services, war rooms, intake, and AI tooling.

use uuid::Uuid;

use super::issue_common::{fetch_project_member_role, is_workspace_admin};

/// Full-visibility: ws admin, or active member of a project linked
/// (owner or consumer) to the service.
pub async fn full_visible(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return Ok(true);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_project_links spl \
          JOIN projects p ON p.id = spl.project_id AND p.deleted_at IS NULL \
          JOIN project_members pm ON pm.project_id = spl.project_id AND pm.member_id = $2 \
            AND pm.is_active = true AND pm.deleted_at IS NULL \
          JOIN workspace_members wm ON wm.workspace_id = spl.workspace_id AND wm.member_id = $2 \
            AND wm.is_active = true AND wm.deleted_at IS NULL \
          JOIN workspaces w ON w.id = spl.workspace_id AND w.slug = $3 \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.service_id = $1 AND spl.deleted_at IS NULL)",
    )
    .bind(service_id)
    .bind(user_id)
    .bind(slug)
    .fetch_one(pool)
    .await
}

/// All service ids the caller sees in full.
pub async fn visible_service_ids(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return sqlx::query_scalar(
            "SELECT sv.id FROM services sv \
              JOIN workspaces w ON w.id = sv.workspace_id \
              WHERE w.slug = $1 AND sv.deleted_at IS NULL ORDER BY sv.id",
        )
        .bind(slug)
        .fetch_all(pool)
        .await;
    }
    sqlx::query_scalar(
        "SELECT DISTINCT spl.service_id FROM service_project_links spl \
          JOIN projects p ON p.id = spl.project_id AND p.deleted_at IS NULL \
          JOIN project_members pm ON pm.project_id = spl.project_id AND pm.member_id = $1 \
            AND pm.is_active = true AND pm.deleted_at IS NULL \
          JOIN workspace_members wm ON wm.workspace_id = spl.workspace_id AND wm.member_id = $1 \
            AND wm.is_active = true AND wm.deleted_at IS NULL \
          JOIN workspaces w ON w.id = spl.workspace_id AND w.slug = $2 \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.deleted_at IS NULL ORDER BY spl.service_id",
    )
    .bind(user_id)
    .bind(slug)
    .fetch_all(pool)
    .await
}

async fn project_role(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<Option<i16>, sqlx::Error> {
    fetch_project_member_role(pool, user_id, slug, project_id).await
}

/// Writer: ws admin, or member+ (15/20) of the owner project.
pub async fn gate_writer(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return Ok(true);
    }
    let Some(owner) = owner_project(pool, service_id).await? else {
        return Ok(false);
    };
    Ok(matches!(
        project_role(pool, user_id, slug, owner).await?,
        Some(20) | Some(15)
    ))
}

/// Admin: ws admin, or project admin (20) of the owner project.
pub async fn gate_admin(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return Ok(true);
    }
    let Some(owner) = owner_project(pool, service_id).await? else {
        return Ok(false);
    };
    Ok(matches!(
        project_role(pool, user_id, slug, owner).await?,
        Some(20)
    ))
}

pub async fn owner_project(
    pool: &sqlx::PgPool,
    service_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT spl.project_id FROM service_project_links spl \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.service_id = $1 AND spl.link_type = 'owner' AND spl.deleted_at IS NULL",
    )
    .bind(service_id)
    .fetch_optional(pool)
    .await
}

pub async fn linked_to_project(
    pool: &sqlx::PgPool,
    service_id: Uuid,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_project_links spl \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.service_id = $1 AND spl.project_id = $2 AND spl.deleted_at IS NULL)",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
}

/// Pure project-admin check (does not include the ws-admin fallback).
pub async fn is_project_admin(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(matches!(
        project_role(pool, user_id, slug, project_id).await?,
        Some(20)
    ))
}
