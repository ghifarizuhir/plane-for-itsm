//! Track (module) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature, ProjectRow,
};
use crate::agent::{record, ToolTrace};

pub const TRACK_STATUSES: [&str; 6] = [
    "backlog",
    "planned",
    "in-progress",
    "paused",
    "completed",
    "cancelled",
];

pub fn track_status_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if TRACK_STATUSES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "status must be one of: {}",
            TRACK_STATUSES.join(", ")
        )))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct TrackRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub status: String,
    pub lead: String,
    pub members: i64,
    pub total: i64,
    pub completed: i64,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
}

pub const TRACKS_SQL: &str = "SELECT m.id, m.name, p.identifier AS project, m.status, \
     COALESCE(u.display_name, u.email, '') AS lead, \
     COALESCE((SELECT count(*) FROM module_members mm \
       WHERE mm.module_id = m.id AND mm.deleted_at IS NULL), 0)::int8 AS members, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.workspace_id = $1 AND m.deleted_at IS NULL AND m.archived_at IS NULL \
     AND p.module_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($3::uuid IS NULL OR p.id = $3) \
     AND ($4::text IS NULL OR m.name ILIKE '%' || $4 || '%') \
     AND ($5::text IS NULL OR m.status = $5) \
     ORDER BY m.created_at DESC LIMIT $6";

pub fn tracks_json(rows: &[TrackRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "project": row.project,
                "status": row.status,
                "lead": row.lead,
                "members": row.members,
                "total": row.total,
                "completed": row.completed,
                "start_date": row.start_date,
                "target_date": row.target_date,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

pub const TRACK_BY_ID_SQL: &str = "SELECT m.id, m.name, p.identifier AS project, m.status, \
     COALESCE(u.display_name, u.email, '') AS lead, \
     COALESCE((SELECT count(*) FROM module_members mm \
       WHERE mm.module_id = m.id AND mm.deleted_at IS NULL), 0)::int8 AS members, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.id = $1 AND m.workspace_id = $2 AND m.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

pub const TRACKS_BY_NAME_PROJECT_SQL: &str = "SELECT m.id, m.name, \
     p.identifier AS project, m.status, \
     COALESCE(u.display_name, u.email, '') AS lead, 0::int8 AS members, \
     0::int8 AS total, 0::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.project_id = $1 AND m.workspace_id = $2 AND m.deleted_at IS NULL \
     AND m.archived_at IS NULL \
     AND (lower(m.name) = lower($3) OR m.name ILIKE '%' || $3 || '%') \
     ORDER BY m.created_at DESC LIMIT 10";

pub const TRACKS_BY_NAME_SQL: &str = "SELECT m.id, m.name, p.identifier AS project, \
     m.status, COALESCE(u.display_name, u.email, '') AS lead, 0::int8 AS members, \
     0::int8 AS total, 0::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.workspace_id = $1 AND m.deleted_at IS NULL AND m.archived_at IS NULL \
     AND p.module_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(m.name) = lower($3) OR m.name ILIKE '%' || $3 || '%') \
     ORDER BY m.created_at DESC LIMIT 10";

pub fn pick_track(rows: Vec<TrackRow>, reference: &str) -> Result<TrackRow, ToolExecutionError> {
    let needle = reference.trim();
    if rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "track '{needle}' was not found or is not accessible"
        )));
    }
    let exact: Vec<&TrackRow> = rows
        .iter()
        .filter(|row| row.name.eq_ignore_ascii_case(needle))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }
    if exact.len() > 1 || rows.len() > 1 {
        let names = rows
            .iter()
            .map(|row| format!("{}/{}", row.project, row.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolExecutionError::invalid_args(format!(
            "track '{needle}' is ambiguous; use one of: {names}"
        )));
    }
    Ok(rows.into_iter().next().expect("one row"))
}

pub async fn resolve_track(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
    project: Option<&ProjectRow>,
) -> Result<TrackRow, ToolExecutionError> {
    if let Ok(id) = Uuid::parse_str(reference.trim()) {
        let row: Option<TrackRow> = sqlx::query_as(TRACK_BY_ID_SQL)
            .bind(id)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
        return row.ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "track '{reference}' was not found or is not accessible"
            ))
        });
    }
    let rows: Vec<TrackRow> = match project {
        Some(project) => sqlx::query_as(TRACKS_BY_NAME_PROJECT_SQL)
            .bind(project.id)
            .bind(workspace_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
        None => sqlx::query_as(TRACKS_BY_NAME_SQL)
            .bind(workspace_id)
            .bind(user_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
    };
    pick_track(rows, reference)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct TrackLinkRow {
    pub title: Option<String>,
    pub url: String,
}

pub const TRACK_MEMBERS_SQL: &str = "SELECT COALESCE(u.display_name, u.email, 'unknown') \
     FROM module_members mm JOIN users u ON u.id = mm.member_id \
     WHERE mm.module_id = $1 AND mm.deleted_at IS NULL ORDER BY 1";

pub const TRACK_LINKS_SQL: &str = "SELECT title, url FROM module_links \
     WHERE module_id = $1 AND deleted_at IS NULL ORDER BY title NULLS LAST, url";

pub const TRACK_STATE_COUNTS_SQL: &str = "SELECT s.\"group\" AS state_group, \
     count(*)::int8 AS count \
     FROM module_issues mi \
     JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE mi.module_id = $1 AND mi.deleted_at IS NULL \
     GROUP BY s.\"group\" ORDER BY s.\"group\"";

pub const TRACK_OPEN_ITEMS_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, COALESCE(s.\"group\", '') AS state_group, i.priority \
     FROM module_issues mi \
     JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE mi.module_id = $1 AND mi.deleted_at IS NULL \
     AND s.\"group\" NOT IN ('completed', 'cancelled') \
     ORDER BY i.updated_at DESC LIMIT 10";

pub fn track_detail_json(
    row: &TrackRow,
    members: &[String],
    links: &[TrackLinkRow],
    counts: &[crate::tools::work_items::GroupCountRow],
    open_items: &[crate::tools::work_items::WorkItemBrief],
) -> String {
    json!({
        "name": row.name,
        "project": row.project,
        "status": row.status,
        "lead": row.lead,
        "start_date": row.start_date,
        "target_date": row.target_date,
        "members": members,
        "links": links,
        "total": row.total,
        "completed": row.completed,
        "state_counts": counts,
        "open_work_items": open_items,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListTracksArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: backlog, planned, in-progress, paused, completed, cancelled.
    pub status: Option<String>,
    /// Case-insensitive substring to match against track names.
    pub query: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListTracks {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListTracks {
    const NAME: &'static str = "list_tracks";
    type Args = ListTracksArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List tracks (modules) the caller can access, newest first, optionally \
         filtered by project, status, and name substring. Returns name, project, \
         status, lead, member count, and progress (total and completed work \
         items). Only projects with the tracks feature enabled are included."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListTracksArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Modules).await?;
        }
        let status = track_status_arg(args.status.as_deref())?;
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<TrackRow> = sqlx::query_as(TRACKS_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(project.as_ref().map(|row| row.id))
            .bind(&query)
            .bind(&status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(tracks_json(&rows))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetTrackArgs {
    /// Track name, or a track id when known.
    pub track: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
}

pub struct GetTrack {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetTrack {
    const NAME: &'static str = "get_track";
    type Args = GetTrackArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one track by name or id: status, lead, dates, members, resource \
         links, progress (total and completed), the count of work items per \
         state group, and the 10 most recently updated unfinished items."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetTrackArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                ensure_feature(&self.pool, project.id, ProjectFeature::Modules).await?;
                Some(project)
            }
            None => None,
        };
        let row = resolve_track(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.track,
            project.as_ref(),
        )
        .await?;
        let members: Vec<String> = sqlx::query_scalar(TRACK_MEMBERS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let links: Vec<TrackLinkRow> = sqlx::query_as(TRACK_LINKS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let counts: Vec<crate::tools::work_items::GroupCountRow> =
            sqlx::query_as(TRACK_STATE_COUNTS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        let open_items: Vec<crate::tools::work_items::WorkItemBrief> =
            sqlx::query_as(TRACK_OPEN_ITEMS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        Ok(track_detail_json(
            &row, &members, &links, &counts, &open_items,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn track(name: &str, project: &str) -> TrackRow {
        TrackRow {
            id: Uuid::new_v4(),
            name: name.to_string(),
            project: project.to_string(),
            status: "planned".to_string(),
            lead: String::new(),
            members: 0,
            total: 0,
            completed: 0,
            start_date: None,
            target_date: None,
        }
    }

    #[test]
    fn track_status_allowlist() {
        assert_eq!(track_status_arg(None).unwrap(), None);
        assert_eq!(
            track_status_arg(Some("In-Progress")).unwrap(),
            Some("in-progress".to_string())
        );
        assert!(track_status_arg(Some("someday")).is_err());
    }

    #[test]
    fn pick_track_prefers_exact_then_unique_substring() {
        let rows = vec![track("Payments", "LTS"), track("Payments v2", "LTS")];
        assert_eq!(pick_track(rows.clone(), "payments").unwrap().name, "Payments");
        assert!(pick_track(rows, "pay").is_err());
        assert!(pick_track(vec![], "payments").is_err());
    }

    #[test]
    fn tracks_json_shape() {
        let parsed: serde_json::Value = serde_json::from_str(&tracks_json(&[track(
            "Payments",
            "LTS",
        )]))
        .unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["status"], json!("planned"));
    }

    #[test]
    fn track_detail_json_shape() {
        let row = track("Payments", "LTS");
        let links = vec![TrackLinkRow {
            title: Some("Runbook".to_string()),
            url: "https://example.com".to_string(),
        }];
        let counts = vec![crate::tools::work_items::GroupCountRow {
            state_group: "started".to_string(),
            count: 2,
        }];
        let items = vec![crate::tools::work_items::WorkItemBrief {
            identifier: "LTS-1".to_string(),
            name: "Fix".to_string(),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "none".to_string(),
        }];
        let parsed: serde_json::Value = serde_json::from_str(&track_detail_json(
            &row,
            &["Budi".to_string()],
            &links,
            &counts,
            &items,
        ))
        .unwrap();
        assert_eq!(parsed["members"][0], json!("Budi"));
        assert_eq!(parsed["links"][0]["title"], json!("Runbook"));
        assert_eq!(parsed["open_work_items"][0]["identifier"], json!("LTS-1"));
    }

    #[test]
    fn tracks_sql_is_scoped_guarded_and_gated() {
        assert!(TRACKS_SQL.contains("workspace_id = $1"));
        assert!(TRACKS_SQL.contains("project_members"));
        assert!(TRACKS_SQL.contains("p.module_view = true"));
    }
}
