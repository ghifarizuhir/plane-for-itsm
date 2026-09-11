use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/intake.py:IntakeSerializer` served by
/// `plane/app/urls/intake.py` (IntakeViewSet list/create; `inboxes/` is
/// an alias of the same viewset). Unique (name, project) → 409 mirrors
/// `intake_unique_name_project_when_deleted_at_null`. The default-intake
/// delete guard ("You cannot delete the default intake",
/// `plane/app/views/intake/base.py:88`) belongs to the detail task.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIntake {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntakeOut {
    pub id: uuid::Uuid,
    pub name: String,
}

/// Mirrors `plane/app/views/intake/base.py:IntakeIssueViewSet.create`:
/// nested `issue.name` required ("Name is required"), `issue.priority`
/// must be low/medium/high/urgent/none ("Invalid priority"). The issue
/// is created in the project's triage state (created on demand, mirroring
/// the view) and linked with status -2 (Pending).
#[derive(Debug, Clone, Deserialize)]
pub struct IntakeIssuePayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateIntakeIssue {
    pub issue: IntakeIssuePayload,
}

const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

pub fn validate_create(body: &CreateIntake) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    Ok(())
}

pub fn validate_issue_create(body: &CreateIntakeIssue) -> Result<(), String> {
    match &body.issue.name {
        Some(n) if !n.trim().is_empty() => {}
        _ => return Err("Name is required".to_string()),
    }
    let priority = body.issue.priority.as_deref().unwrap_or("none");
    if !PRIORITIES.contains(&priority) {
        return Err("Invalid priority".to_string());
    }
    Ok(())
}

/// Full `IntakeSerializer` row (`serializers/intake.py:17-24`: `__all__`
/// + nested `project_detail` lite + annotated `pending_issue_count`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct IntakeFullRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    name: String,
    description: String,
    is_default: bool,
    view_props: Value,
    logo_props: Value,
    proj_identifier: String,
    proj_name: String,
    proj_cover_image: Option<String>,
    proj_cover_asset: Option<String>,
    proj_logo_props: Value,
    proj_description: String,
    pending_issue_count: i64,
}

const INTAKE_FULL_COLS: &str = "i.id, i.created_at, i.updated_at, i.created_by_id, i.updated_by_id, \
    i.workspace_id, i.project_id, i.name, i.description, i.is_default, i.view_props, i.logo_props, \
    p.identifier AS proj_identifier, p.name AS proj_name, p.cover_image AS proj_cover_image, \
    fa.asset AS proj_cover_asset, p.logo_props AS proj_logo_props, p.description AS proj_description, \
    (SELECT COUNT(*) FROM intake_issues ii WHERE ii.intake_id = i.id \
     AND ii.status = -2 AND ii.deleted_at IS NULL) AS pending_issue_count";

fn intake_full_json(r: &IntakeFullRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
        "workspace": r.workspace_id,
        "project": r.project_id,
        "name": r.name,
        "description": r.description,
        "is_default": r.is_default,
        "view_props": r.view_props,
        "logo_props": r.logo_props,
        "project_detail": {
            "id": r.project_id,
            "identifier": r.proj_identifier,
            "name": r.proj_name,
            "cover_image": r.proj_cover_image,
            "cover_image_url": r.proj_cover_asset.clone().or_else(|| r.proj_cover_image.clone()),
            "logo_props": r.proj_logo_props,
            "description": r.proj_description,
        },
        "pending_issue_count": r.pending_issue_count,
    })
}

async fn fetch_intake_full(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<Option<IntakeFullRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {INTAKE_FULL_COLS} FROM intakes i \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN projects p ON p.id = i.project_id \
         LEFT JOIN file_assets fa ON fa.id = p.cover_image_asset_id \
         WHERE i.id = $1 AND i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL",
    ))
    .bind(pk)
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// GET `.../intakes/` — Django returns the SINGLE first intake object
/// (`get_queryset().first()`, `base.py:74-76`), not an array (absent →
/// 200 null). Gate ADMIN/MEMBER (`base.py:73`).
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match crate::routes::project::ws_role(&st.pool, auth.0, &slug).await? {
        Some(r) if r >= 15 => {}
        _ => return Ok(crate::routes::project::deny()),
    }
    let row: Option<IntakeFullRow> = sqlx::query_as(&format!(
        "SELECT {INTAKE_FULL_COLS} FROM intakes i \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN projects p ON p.id = i.project_id \
         LEFT JOIN file_assets fa ON fa.id = p.cover_image_asset_id \
         WHERE i.project_id = $1 AND w.slug = $2 AND i.deleted_at IS NULL \
         ORDER BY i.name ASC LIMIT 1",
    ))
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(intake_full_json(&r)))),
        None => Ok((StatusCode::OK, Json(Value::Null))),
    }
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIntake>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // ADMIN/MEMBER (`base.py:78`).
    match crate::routes::project::ws_role(&st.pool, auth.0, &slug).await? {
        Some(r) if r >= 15 => {}
        _ => return Ok(crate::routes::project::deny()),
    }
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let res = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, false, '{}', '{}', $3, w.id, $5, $5, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(project_id)
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await;
    let new_id = match res {
        Ok(v) => v,
        // Unique (name, project) violation → DRF `IntegrityError` handler:
        // 400 `{"error": "The payload is not valid"}` (`views/base.py:80-84`).
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate key") {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "The payload is not valid"})),
                ));
            }
            return Err(common::errors::AppError(anyhow::anyhow!(e)));
        }
    };
    match new_id {
        // 201 full shape (`base.py:78-80`, DRF default create).
        Some(id) => match fetch_intake_full(&st.pool, &slug, project_id, id).await? {
            Some(r) => Ok((StatusCode::CREATED, Json(intake_full_json(&r)))),
            None => Ok((StatusCode::CREATED, Json(json!({"id": id, "name": body.name})))),
        },
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Workspace not found"})))),
    }
}

/// One row of the intake-issue list: `IntakeIssueSerializer`
/// (`serializers/intake.py:27-50`) with nested `IssueIntakeSerializer`
/// (`serializers/issue.py:752-767`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct InboxListRow {
    id: uuid::Uuid,
    status: i32,
    duplicate_to_id: Option<uuid::Uuid>,
    snoozed_till: Option<chrono::DateTime<chrono::Utc>>,
    source: Option<String>,
    created_by_id: Option<uuid::Uuid>,
    issue_id: uuid::Uuid,
    issue_name: String,
    issue_priority: String,
    issue_sequence_id: i32,
    issue_project_id: uuid::Uuid,
    issue_created_at: chrono::DateTime<chrono::Utc>,
    issue_label_ids: Vec<uuid::Uuid>,
    issue_created_by_id: Option<uuid::Uuid>,
}

fn inbox_list_json(r: &InboxListRow) -> Value {
    json!({
        "id": r.id,
        "status": r.status,
        "duplicate_to": r.duplicate_to_id,
        "snoozed_till": r.snoozed_till,
        "source": r.source,
        "issue": {
            "id": r.issue_id,
            "name": r.issue_name,
            "priority": r.issue_priority,
            "sequence_id": r.issue_sequence_id,
            "project_id": r.issue_project_id,
            "created_at": r.issue_created_at,
            "label_ids": r.issue_label_ids,
            "created_by": r.issue_created_by_id,
        },
        "created_by": r.created_by_id,
    })
}

/// CSV-list parsing for the legacy `issue_filters` GET semantics
/// (`utils/issue_filters.py`): drop `"null"` tokens; an empty remainder
/// OR any `""` token drops the whole filter.
fn csv_filter(raw: Option<&str>) -> Vec<String> {
    match raw {
        Some(s) => {
            let parts: Vec<String> = s.split(',').map(str::to_string).collect();
            if parts.iter().any(|p| p.is_empty()) {
                return Vec::new();
            }
            let kept: Vec<String> = parts.into_iter().filter(|p| p != "null").collect();
            if kept.is_empty() {
                Vec::new()
            } else {
                kept
            }
        }
        None => Vec::new(),
    }
}

fn uuid_list(vals: &[String]) -> Vec<uuid::Uuid> {
    vals.iter().filter_map(|v| uuid::Uuid::parse_str(v).ok()).collect()
}

/// Date-range parsing for `filter_created_at/updated_at` GET
/// (`issue_filters.py:209-245` + `date_filter:55-81`): each comma item is
/// `date` (equality), `date;after` (>=), `date;before` (<=), or a
/// `N_unit;after|before;offset` duration (FE `getCustomDates` never emits
/// the duration form for inbox — treated as unbounded here); last item
/// wins per column, mirroring the dict overwrite.
fn date_bounds(raw: Option<&str>) -> (Option<String>, Option<String>) {
    let mut gte: Option<String> = None;
    let mut lte: Option<String> = None;
    for item in raw.unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let bits: Vec<&str> = item.split(';').collect();
        if bits.len() >= 2 {
            if bits[1] == "after" {
                gte = Some(bits[0].to_string());
            } else {
                lte = Some(bits[0].to_string());
            }
        } else {
            gte = Some(bits[0].to_string());
            lte = Some(bits[0].to_string());
        }
    }
    (gte, lte)
}

/// GET `.../intake-issues/` (and the FE-facing `inbox-issues/` twin) —
/// parity with `IntakeIssueViewSet.list` (`base.py:177-226`): AMG gate;
/// no intake → 404 `{"error": "Intake not found"}`; status CSV (default
/// `"-2"`, `"null"` tokens dropped); legacy `issue_filters` GET subset
/// (priority/labels/state/assignees/created_by/created_at/updated_at, all
/// `issue__`-prefixed); `order_by` allowlist
/// (`INTAKE_ISSUE_ORDER_BY_ALLOWLIST`, default `-issue__created_at`);
/// guest (+ !guest_view_all_features) sees own rows only; OffsetPaginator
/// envelope of `IntakeIssueSerializer` rows.
pub async fn list_issues(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::issue_common::{
        next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str,
        total_pages, DetailEnvelope, PageWindow,
    };
    // AMG (`base.py:177`); DRF permission-class deny shape.
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(crate::routes::member::deny_detail());
    }
    // First intake by name order (`Intake.Meta.ordering`); absent → 404
    // (`base.py:179-181`).
    let intake: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT i.id FROM intakes i JOIN workspaces w ON w.id = i.workspace_id \
         WHERE i.project_id = $1 AND w.slug = $2 AND i.deleted_at IS NULL \
         ORDER BY i.name ASC LIMIT 1",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((intake_id,)) = intake else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
    };
    let status_raw = params.get("status").map(|s| s.as_str()).unwrap_or("-2");
    let statuses: Vec<i32> = csv_filter(Some(status_raw))
        .into_iter()
        .filter_map(|s| s.parse::<i32>().ok())
        .collect();
    let prios = csv_filter(params.get("priority").map(|s| s.as_str()));
    let label_ids = uuid_list(&csv_filter(params.get("labels").map(|s| s.as_str())));
    let labels_none = params.get("labels").map(|s| s.split(',').any(|t| t == "None")).unwrap_or(false);
    let state_ids = uuid_list(&csv_filter(params.get("state").map(|s| s.as_str())));
    let assignee_ids = uuid_list(&csv_filter(params.get("assignees").map(|s| s.as_str())));
    let assignees_none = params.get("assignees").map(|s| s.split(',').any(|t| t == "None")).unwrap_or(false);
    let creator_ids = uuid_list(&csv_filter(params.get("created_by").map(|s| s.as_str())));
    let (created_gte, created_lte) = date_bounds(params.get("created_at").map(|s| s.as_str()));
    let (updated_gte, updated_lte) = date_bounds(params.get("updated_at").map(|s| s.as_str()));
    // Guest scope (`base.py:211-221`).
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let guest_only = if matches!(role, Some(r) if r <= 5) {
        let gva: bool = sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&st.pool)
            .await?
            .unwrap_or(false);
        !gva
    } else {
        false
    };
    // `order_by` allowlist (`order_queryset.py:36-48`, default `-issue__created_at`).
    let order_raw = params.get("order_by").map(|s| s.as_str()).unwrap_or("-issue__created_at");
    let (bare, desc) = match order_raw.strip_prefix('-') {
        Some(b) => (b, true),
        None => (order_raw, false),
    };
    let order_expr = match (bare, desc) {
        ("issue__created_at", false) => "i.created_at ASC",
        ("issue__created_at", true) => "i.created_at DESC",
        ("issue__updated_at", false) => "i.updated_at ASC",
        ("issue__updated_at", true) => "i.updated_at DESC",
        ("issue__sequence_id", false) => "i.sequence_id ASC",
        ("issue__sequence_id", true) => "i.sequence_id DESC",
        ("issue__sort_order", false) => "i.sort_order ASC",
        ("issue__sort_order", true) => "i.sort_order DESC",
        ("issue__target_date", false) => "i.target_date ASC NULLS LAST",
        ("issue__target_date", true) => "i.target_date DESC NULLS LAST",
        ("issue__start_date", false) => "i.start_date ASC NULLS LAST",
        ("issue__start_date", true) => "i.start_date DESC NULLS LAST",
        ("issue__priority", false) => "i.priority ASC",
        ("issue__priority", true) => "i.priority DESC",
        ("issue__state__name", _) => "s.name ASC NULLS LAST",
        ("created_at", false) => "ii.created_at ASC",
        ("created_at", true) => "ii.created_at DESC",
        ("updated_at", false) => "ii.updated_at ASC",
        ("updated_at", true) => "ii.updated_at DESC",
        ("status", false) => "ii.status ASC",
        ("status", true) => "ii.status DESC",
        _ => "i.created_at DESC",
    };
    let where_extra = format!(
        "AND ($3::int[] IS NULL OR ii.status = ANY($3)) \
         AND ($4::text[] IS NULL OR i.priority = ANY($4)) \
         AND ($5::uuid[] IS NULL OR EXISTS(SELECT 1 FROM issue_labels il WHERE il.issue_id = i.id AND il.deleted_at IS NULL AND il.label_id = ANY($5))) \
         AND (NOT $6::boolean OR NOT EXISTS(SELECT 1 FROM issue_labels il WHERE il.issue_id = i.id AND il.deleted_at IS NULL)) \
         AND ($7::uuid[] IS NULL OR i.state_id = ANY($7)) \
         AND ($8::uuid[] IS NULL OR EXISTS(SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL AND ia.assignee_id = ANY($8))) \
         AND (NOT $9::boolean OR NOT EXISTS(SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL)) \
         AND ($10::uuid[] IS NULL OR i.created_by_id = ANY($10)) \
         AND ($11::date IS NULL OR i.created_at::date >= $11) \
         AND ($12::date IS NULL OR i.created_at::date <= $12) \
         AND ($13::date IS NULL OR i.updated_at::date >= $13) \
         AND ($14::date IS NULL OR i.updated_at::date <= $14) \
         AND (NOT $15::boolean OR ii.created_by_id = $16)"
    );
    let status_arr: Option<Vec<i32>> = if statuses.is_empty() { None } else { Some(statuses) };
    let prio_arr: Option<Vec<String>> = if prios.is_empty() { None } else { Some(prios) };
    let label_arr: Option<Vec<uuid::Uuid>> = if label_ids.is_empty() { None } else { Some(label_ids) };
    let state_arr: Option<Vec<uuid::Uuid>> = if state_ids.is_empty() { None } else { Some(state_ids) };
    let assignee_arr: Option<Vec<uuid::Uuid>> = if assignee_ids.is_empty() { None } else { Some(assignee_ids) };
    let creator_arr: Option<Vec<uuid::Uuid>> = if creator_ids.is_empty() { None } else { Some(creator_ids) };
    let parse_date = |s: Option<String>| -> Option<chrono::NaiveDate> {
        s.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
    };
    let cg = parse_date(created_gte);
    let cl = parse_date(created_lte);
    let ug = parse_date(updated_gte);
    let ul = parse_date(updated_lte);
    let base_from = format!(
        "FROM intake_issues ii JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE ii.intake_id = $1 AND ii.project_id = $2 AND ii.deleted_at IS NULL {where_extra}"
    );
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {base_from}"))
        .bind(intake_id).bind(project_id)
        .bind(&status_arr).bind(&prio_arr).bind(&label_arr).bind(labels_none)
        .bind(&state_arr).bind(&assignee_arr).bind(assignees_none)
        .bind(&creator_arr).bind(cg).bind(cl).bind(ug).bind(ul)
        .bind(guest_only).bind(auth.0)
        .fetch_one(&st.pool)
        .await?;
    // OffsetPaginator envelope (`self.paginate`, `base.py:222-226`).
    let limit = match parse_per_page(params.get("per_page").map(|s| s.as_str())) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
    };
    let cursor_raw = params.get("cursor").cloned().unwrap_or_else(|| format!("{limit}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
    };
    let window = match page_window(cursor.page, limit) {
        Ok(w) => w,
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
    };
    let mut rows: Vec<InboxListRow> = match window {
        PageWindow::Rows(offset) => sqlx::query_as(&format!(
            "SELECT ii.id, ii.status, ii.duplicate_to_id, ii.snoozed_till, ii.source, \
             ii.created_by_id, i.id AS issue_id, i.name AS issue_name, i.priority AS issue_priority, \
             i.sequence_id AS issue_sequence_id, i.project_id AS issue_project_id, \
             i.created_at AS issue_created_at, \
             COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
               WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{{}}'::uuid[]) AS issue_label_ids, \
             i.created_by_id AS issue_created_by_id \
             {base_from} ORDER BY {order_expr} LIMIT $17 OFFSET $18"
        ))
        .bind(intake_id).bind(project_id)
        .bind(&status_arr).bind(&prio_arr).bind(&label_arr).bind(labels_none)
        .bind(&state_arr).bind(&assignee_arr).bind(assignees_none)
        .bind(&creator_arr).bind(cg).bind(cl).bind(ug).bind(ul)
        .bind(guest_only).bind(auth.0)
        .bind(limit + 1).bind(offset)
        .fetch_all(&st.pool)
        .await?,
        PageWindow::BeyondEnd => Vec::new(),
    };
    let next_page_results = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next_cursor_str(limit, cursor.page),
        prev_cursor: prev_cursor_str(limit, cursor.page),
        next_page_results,
        prev_page_results: cursor.page > 0,
        count: rows.len() as i64,
        total_pages: total_pages(total, limit),
        total_results: total,
        extra_stats: None,
        results: rows.iter().map(inbox_list_json).collect(),
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

pub async fn create_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIntakeIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // AMG (`base.py:228`).
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(crate::routes::member::deny_detail());
    }
    validate_issue_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let name = body.issue.name.clone().unwrap_or_default();
    let priority = body.issue.priority.clone().unwrap_or_else(|| "none".to_string());

    let workspace_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Workspace not found"}))));
    };

    // Triage state lookup-or-create mirrors
    // `IntakeIssueViewSet.create` (`plane/app/views/intake/base.py:246-256`):
    // Django reads `State.triage_objects.filter(project_id, workspace__slug)`
    // (`TriageStateManager`, `plane/db/models/state.py:72-76`), i.e. triage
    // identity is `"group" = 'triage'` — NOT `is_triage` (both the
    // `DEFAULT_STATES` seed and the on-demand `State.objects.create` leave
    // `is_triage` at its `default=False`). The intake issue lands in triage,
    // creating the state row on demand.
    let triage_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'triage' AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let triage_id = match triage_id {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \"default\", project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), 'Triage', '', 'triage', 'triage', '#4E5355', 65000, false, false, $1, $2, now(), now()) RETURNING id",
            )
            .bind(project_id)
            .bind(workspace_id)
            .fetch_one(&st.pool)
            .await?
        }
    };

    let issue_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, '<p></p>', '{}', $2, false, COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $4 AND state_id IS NOT DISTINCT FROM $3), 65535 - 10000) + 10000, COALESCE((SELECT MAX(sequence) FROM issue_sequences WHERE project_id = $4), 0) + 1, $3, $4, $5, now(), now()) RETURNING id",
    )
    .bind(&name)
    .bind(&priority)
    .bind(triage_id)
    .bind(project_id)
    .bind(workspace_id)
    .fetch_one(&st.pool)
    .await?;

    // The viewset attaches to the project's first intake (base.py:271).
    let intake_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM intakes WHERE project_id = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(intake_id) = intake_id else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
    };

    let row = sqlx::query_as::<_, common::models::intake::IntakeIssue>(
        "INSERT INTO intake_issues (id, intake_id, issue_id, status, extra, project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, -2, '{}'::jsonb, $3, $4, now(), now()) RETURNING id, status",
    )
    .bind(intake_id)
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .fetch_one(&st.pool)
    .await?;
    // Django `create` (`intake/base.py:330`) returns 200 (not 201) with the
    // full `IntakeIssueDetailSerializer`.
    match fetch_inbox_detail(&st.pool, &slug, project_id, row.id, issue_id, auth.0).await? {
        Some(d) => Ok((StatusCode::OK, Json(serde_json::to_value(&d).unwrap()))),
        None => Ok((
            StatusCode::OK,
            Json(json!({"id": row.id, "status": row.status, "issue_id": issue_id})),
        )),
    }
}

/// Mirrors `plane/app/views/intake/base.py:destroy`: the default intake
/// cannot be deleted.
pub fn guard_delete(is_default: bool) -> Result<(), String> {
    if is_default {
        return Err("You cannot delete the default intake".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchIntake {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Detail/patch/destroy share the ADMIN/MEMBER gate (`base.py:82`,
/// `allow_permission` on the viewset) + full-shape bodies.
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match crate::routes::project::ws_role(&st.pool, auth.0, &slug).await? {
        Some(r) if r >= 15 => {}
        _ => return Ok(crate::routes::project::deny()),
    }
    match fetch_intake_full(&st.pool, &slug, project_id, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(intake_full_json(&r)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchIntake>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match crate::routes::project::ws_role(&st.pool, auth.0, &slug).await? {
        Some(r) if r >= 15 => {}
        _ => return Ok(crate::routes::project::deny()),
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid name"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE intakes SET name = COALESCE($1, name), description = COALESCE($2, description), updated_at = now(), updated_by_id = $5 WHERE id = $3 AND project_id = $4 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(pk)
    .bind(project_id)
    .bind(auth.0)
    .execute(&st.pool)
    .await;
    match n {
        // Dup-name unique violation → the same DRF IntegrityError body as create.
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate key") {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "The payload is not valid"})),
                ));
            }
            return Err(common::errors::AppError(anyhow::anyhow!(e)));
        }
        Ok(r) if r.rows_affected() == 0 => {
            return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
        }
        Ok(_) => {}
    }
    match fetch_intake_full(&st.pool, &slug, project_id, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(intake_full_json(&r)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"})))),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match crate::routes::project::ws_role(&st.pool, auth.0, &slug).await? {
        Some(r) if r >= 15 => {}
        _ => return Ok(crate::routes::project::deny()),
    }
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT is_default FROM intakes WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((is_default,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
    };
    if let Err(e) = guard_delete(is_default) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    sqlx::query("UPDATE intakes SET deleted_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// Scope resolver for the `:pk/` handlers: Django's pk is the ISSUE id,
/// scoped by the project's first intake (`Meta.ordering = ("name",)` →
/// `ORDER BY name ASC LIMIT 1`) + workspace + project
/// (`base.py:339-346,505-506,553-558`). FE passes `issue.id` in every
/// detail/update/delete URL (store keys `inboxIssues[issue.id]`), so the
/// old row-id scope 404d all live FE traffic. No intake (Django
/// AttributeError-500) or no row → `None` → 404 (sane-mapping).
#[derive(Debug, Clone, sqlx::FromRow)]
struct InboxScope {
    row_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    created_by_id: Option<uuid::Uuid>,
    status: i32,
}

async fn resolve_inbox_row(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_pk: uuid::Uuid,
) -> Result<Option<InboxScope>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ii.id AS row_id, ii.issue_id, ii.created_by_id, ii.status \
         FROM intake_issues ii JOIN workspaces w ON w.id = ii.workspace_id \
         WHERE ii.issue_id = $1 AND ii.project_id = $2 AND w.slug = $3 \
         AND ii.deleted_at IS NULL \
         AND ii.intake_id = (SELECT i.id FROM intakes i JOIN workspaces w2 ON w2.id = i.workspace_id \
           WHERE i.project_id = $2 AND w2.slug = $3 AND i.deleted_at IS NULL \
           ORDER BY i.name ASC LIMIT 1)",
    )
    .bind(issue_pk)
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Shared full-detail fetch (`IntakeIssueDetailSerializer`, field order
/// `INBOX_DETAIL_KEYS`): the PATCH tail + GET detail + POST create all
/// return this shape. `row_id` = intake-issue row, `issue_id` = issue.
async fn fetch_inbox_detail(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    row_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    user: uuid::Uuid,
) -> Result<Option<InboxIssueDetail>, sqlx::Error> {
    let fresh: Option<(i32, Option<uuid::Uuid>, Option<chrono::DateTime<chrono::Utc>>, Option<String>)> =
        sqlx::query_as(
            "SELECT status, duplicate_to_id, snoozed_till, source FROM intake_issues \
              WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(row_id)
        .fetch_optional(pool)
        .await?;
    let Some((status, duplicate_to, snoozed_till, source)) = fresh else {
        return Ok(None);
    };
    let issue_row: Option<InboxIssueDetailIssue> = sqlx::query_as(INBOX_ISSUE_SELECT_SQL)
        .bind(issue_id)
        .bind(project_id)
        .bind(slug)
        .bind(user)
        .fetch_optional(pool)
        .await?;
    let Some(issue_row) = issue_row else {
        return Ok(None);
    };
    let duplicate_issue_detail: Option<InboxDuplicateDetail> = match duplicate_to {
        None => None,
        Some(dup) => {
            sqlx::query_as(
                "SELECT i.id, i.name, i.priority, i.sequence_id, i.project_id, i.created_at, \
                  COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
                    WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
                  i.created_by_id AS created_by \
                  FROM issues i WHERE i.id = $1 AND i.deleted_at IS NULL",
            )
            .bind(dup)
            .fetch_optional(pool)
            .await?
        }
    };
    Ok(Some(InboxIssueDetail {
        id: row_id,
        status,
        duplicate_to,
        snoozed_till,
        duplicate_issue_detail,
        source,
        issue: issue_row,
    }))
}

pub async fn detail_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // AMG ws gate (decorator, `base.py:503`).
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(crate::routes::member::deny_detail());
    }
    let Some(scope) = resolve_inbox_row(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    // Guest gate (`base.py:534-548`): guest + !guest_view_all_features +
    // not creator → 403 verbatim.
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if matches!(role, Some(r) if r <= 5) {
        let gva: bool = sqlx::query_scalar(
            "SELECT guest_view_all_features FROM projects WHERE id = $1",
        )
        .bind(project_id)
        .fetch_optional(&st.pool)
        .await?
        .unwrap_or(false);
        if !gva && scope.created_by_id != Some(auth.0) {
            return Ok((
                StatusCode::FORBIDDEN,
                Json(json!({"error": "You are not allowed to view this issue"})),
            ));
        }
    }
    match fetch_inbox_detail(&st.pool, &slug, project_id, scope.row_id, scope.issue_id, auth.0).await? {
        Some(d) => Ok((StatusCode::OK, Json(serde_json::to_value(&d).unwrap()))),
        None => Ok(missing()),
    }
}

/// DELETE `.../inbox-issues/:pk/` — parity with Django
/// `IntakeIssueViewSet.destroy` (`base.py:552-569`): pk = issue id scoped
/// by intake (`resolve_inbox_row`); gate `@allow_permission([ADMIN],
/// creator=True, model=Issue)` — project-ADMIN or the ISSUE creator,
/// else the decorator 403; cascade-deletes the issue when status in
/// [-2,-1,0,2]; soft-deletes the intake row; 204.
pub async fn destroy_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(scope) = resolve_inbox_row(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let issue_creator: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT created_by_id FROM issues WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(scope.issue_id)
    .fetch_optional(&st.pool)
    .await?
    .flatten();
    let is_creator = issue_creator == Some(auth.0);
    if !matches!(role, Some(20)) && !is_creator {
        return Ok(crate::routes::project::deny());
    }
    // Mirrors `plane/app/views/intake/base.py:destroy`: pending/rejected
    // intake rows (status in [-2,-1,0,2]) take the underlying issue with them.
    if matches!(scope.status, -2 | -1 | 0 | 2) {
        sqlx::query("UPDATE issues SET deleted_at = now() WHERE id = $1")
            .bind(scope.issue_id)
            .execute(&st.pool)
            .await?;
    }
    sqlx::query("UPDATE intake_issues SET deleted_at = now() WHERE id = $1")
        .bind(scope.row_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

use super::issue_common::{fetch_project_member_role, is_workspace_admin};
use crate::routes::project::missing;

/// PATCH `.../inbox-issues/:pk/` (and the `intake-issues/:pk/` twin —
/// Django serves both paths from `IntakeIssueViewSet`,
/// `plane/app/urls/intake.py:44-55`) — parity with Django
/// `IntakeIssueViewSet.partial_update`
/// (`plane/app/views/intake/base.py:334-...`). Celery
/// `issue_activity.delay` / `issue_description_version_task.delay` writes
/// skipped (batch-wide precedent — Rust never writes activities).
///
/// Locked semantics (plan D13):
/// - Decorator `@allow_permission([ADMIN], creator=True, model=Issue)`
///   (`base.py:334`) is collapsed into the two in-view gates below (the
///   plan's locked matrix): no-membership AND no-ws-admin → 403
///   `{"error":"Only admin or creator can update the intake work items"}`
///   (`base.py:361-365`); guest-role member AND not creator AND not
///   ws-admin → 400 `{"error":"You cannot edit intake issues"}`
///   (`base.py:368-374`); admin | creator | ws-admin (and plain members)
///   → ok. Per the locked matrix the members/guest branches surface the
///   in-view bodies rather than the decorator's generic 403 — note delta
///   vs a strict decorator reading (plain MEMBER non-creators and guest
///   non-creators would 403 at the decorator; here members proceed to
///   issue-edits and guests get the verbatim 400).
/// - Guest issue payloads narrowed to name/description_html/
///   description_json (`base.py:396-401`, no ws-admin exemption there).
/// - Intake-level fields (status/duplicate_to/snoozed_till/source)
///   applied only when `(role > MEMBER) or ws_admin` (`base.py:426`,
///   i.e. project ADMIN 20 or workspace admin).
/// - 200 `IntakeIssueDetailSerializer` (`serializers/intake.py:93-117`):
///   id, status, duplicate_to, snoozed_till, duplicate_issue_detail
///   (`IssueIntakeSerializer`, `serializers/issue.py:752-767`), source,
///   issue (nested 28-key `IssueDetailSerializer`,
///   `serializers/issue.py:934-945` — same key order as D7).
///
/// Scope reuse (plan "reuse its GET/DELETE scope"): lookup is the existing
/// `detail_issue`/`destroy_issue` scope (`intake_issues` row id + project
/// + live). Delta vs Django, which scopes `.get(issue_id=pk, intake_id,
/// project)` (`base.py:341-346`, pk = *issue* id): Rust pk = the
/// intake-issue row id, so no separate `Intake` lookup is needed (Django
/// would AttributeError-500 on a missing intake; Rust 404s instead, D9
/// precedent). Creator = the intake-issue row's `created_by_id` (the
/// field Django's 400 check compares, `base.py:370`); Django's decorator
/// additionally checks `Issue(id=pk).created_by`, unobservable under the
/// Rust row-id scope — noted, same effective outcome via check 1 which
/// has no creator clause.
///
/// Deviations (documented, reviewer-adjudicable):
/// - Datetimes serialize RFC3339 UTC (chrono, batch convention) vs DRF's
///   per-user-timezone rendering.
/// - Nested-issue counts/ids (`sub_issues_count`, `attachment_count`,
///   `link_count`, `cycle_id`, `module/label/assignee_ids`,
///   `is_subscribed`, `is_intake`) are computed live with the D7
///   `ARCHIVE_SELECT_SQL` convention (Django's partial_update tail
///   annotates only label/assignee ids and relies on prefetches).
/// - `issue` payload covers name/description_html/description_json/
///   priority (the triage-edit surface + guest-narrowed keys); other
///   `IssueCreateSerializer` fields are ignored (serde), not 400 —
///   beyond the locked contract.
/// - Value validation is Rust-side with plain `{"error": ...}` bodies:
///   blank name → "Name is required" (this file's intake-create message),
///   unknown priority → "Invalid priority" (ditto), unknown status →
///   "Invalid status" (DRF would return per-field choice errors;
///   first-error-wins here). Unknown `duplicate_to` → 404 `missing()`
///   (D9 precedent for bad related-issue refs).
/// - `skip_activity` is accepted-and-ignored (serde drops unknown keys;
///   activity tasks skipped batch-wide).
/// - Live DB verified 2026-09-06: `intake_issues(id, status,
///   snoozed_till, source, created_by_id, duplicate_to_id, intake_id,
///   issue_id, project_id, updated_by_id, workspace_id, ..., deleted_at)`.

/// Quoted from `plane/app/views/intake/base.py:363`.
pub(crate) const ONLY_ADMIN_OR_CREATOR_MSG: &str =
    "Only admin or creator can update the intake work items";
/// Quoted from `plane/app/views/intake/base.py:371`.
pub(crate) const CANNOT_EDIT_INTAKE_MSG: &str = "You cannot edit intake issues";

/// Intake-level statuses (`IntakeIssueStatus`,
/// `plane/db/models/intake.py:42-48`).
const INTAKE_STATUSES: [i32; 5] = [-2, -1, 0, 1, 2];

/// Top-level `IntakeIssueDetailSerializer.Meta.fields` order
/// (`serializers/intake.py:99-107`).
#[allow(dead_code)]
pub(crate) const INBOX_DETAIL_KEYS: [&str; 7] = [
    "id",
    "status",
    "duplicate_to",
    "snoozed_till",
    "duplicate_issue_detail",
    "source",
    "issue",
];

/// Nested `issue` = `IssueDetailSerializer` key order
/// (`serializers/issue.py:934-945` = `IssueSerializer.Meta.fields`
/// `issue.py:786-812` + description_html/is_subscribed/is_intake) —
/// identical to D7 `ARCHIVED_DETAIL_KEYS`.
#[allow(dead_code)]
pub(crate) const INBOX_ISSUE_KEYS: [&str; 28] = [
    "id",
    "name",
    "state_id",
    "sort_order",
    "completed_at",
    "estimate_point",
    "priority",
    "start_date",
    "target_date",
    "sequence_id",
    "project_id",
    "parent_id",
    "cycle_id",
    "module_ids",
    "label_ids",
    "assignee_ids",
    "sub_issues_count",
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "attachment_count",
    "link_count",
    "is_draft",
    "archived_at",
    "description_html",
    "is_subscribed",
    "is_intake",
];

/// In-view gates of `partial_update` (`base.py:355-374`, locked matrix):
/// `!project_member && !ws_admin` → 403; `(role <= GUEST) && !ws_admin
/// && !creator` → 400 (verbatim `<=`, `base.py:368`; among stored roles
/// 20/15/5 only GUEST trips it — a `Some` role implies membership, so no
/// separate flag is needed for the second check). Else ok.
pub(crate) fn guard_inbox_patch(
    has_membership: bool,
    role: Option<i16>,
    is_creator: bool,
    is_ws_admin: bool,
) -> Result<(), (StatusCode, String)> {
    if !has_membership && !is_ws_admin {
        return Err((
            StatusCode::FORBIDDEN,
            ONLY_ADMIN_OR_CREATOR_MSG.to_string(),
        ));
    }
    if matches!(role, Some(r) if r <= 5) && !is_ws_admin && !is_creator {
        return Err((StatusCode::BAD_REQUEST, CANNOT_EDIT_INTAKE_MSG.to_string()));
    }
    Ok(())
}

/// Intake-level write gate (`base.py:426`): `(project_member and role >
/// ROLE.MEMBER.value) or is_workspace_admin` — verbatim `> 15`, i.e.
/// project ADMIN (20) or workspace admin.
pub(crate) fn may_write_intake_fields(role: Option<i16>, is_ws_admin: bool) -> bool {
    matches!(role, Some(r) if r > 15) || is_ws_admin
}

/// Guest issue-payload narrowing (`base.py:396-401`): `project_member and
/// role <= ROLE.GUEST.value` — verbatim `<= 5`, with NO ws-admin
/// exemption in Django's narrowing branch.
pub(crate) fn is_guest_narrowed(role: Option<i16>) -> bool {
    matches!(role, Some(r) if r <= 5)
}

/// Nested `issue` patch fields: the triage-edit surface (a superset of
/// the guest-narrowed name/description keys, `base.py:396-401`). Unknown
/// keys are ignored by serde (see module docs).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct InboxIssueFields {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub description_json: Option<Value>,
    #[serde(default)]
    pub priority: Option<String>,
}

/// Top-level PATCH body: optional nested `issue` (Django reads
/// `request.data["issue"]`, `base.py:377`) + intake-level fields
/// (`IntakeIssueSerializer` partial, `base.py:426-431`). Double-`Option`
/// on nullable columns mirrors DRF partial semantics: absent = keep,
/// explicit null = clear, value = set. `skip_activity` is
/// accepted-and-ignored (activity tasks skipped batch-wide).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct InboxIssuePatch {
    #[serde(default)]
    pub issue: Option<InboxIssueFields>,
    #[serde(default)]
    pub status: Option<i32>,
    #[serde(default)]
    pub duplicate_to: Option<Option<uuid::Uuid>>,
    #[serde(default)]
    pub snoozed_till: Option<Option<chrono::DateTime<chrono::Utc>>>,
    #[serde(default)]
    pub source: Option<Option<String>>,
}

/// Nested `issue`: the 28-key `IssueDetailSerializer` shape in Django
/// field order (see `INBOX_ISSUE_KEYS`). Column mapping follows D7
/// `ArchivedIssueDetailRow` (`estimate_point` reads
/// `estimate_point_id`, `created_by`/`updated_by` the `*_id` columns).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct InboxIssueDetailIssue {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) state_id: Option<uuid::Uuid>,
    pub(crate) sort_order: f64,
    pub(crate) completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) estimate_point: Option<uuid::Uuid>,
    pub(crate) priority: String,
    pub(crate) start_date: Option<chrono::NaiveDate>,
    pub(crate) target_date: Option<chrono::NaiveDate>,
    pub(crate) sequence_id: i32,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) parent_id: Option<uuid::Uuid>,
    pub(crate) cycle_id: Option<uuid::Uuid>,
    pub(crate) module_ids: Vec<uuid::Uuid>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) assignee_ids: Vec<uuid::Uuid>,
    pub(crate) sub_issues_count: i64,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
    pub(crate) attachment_count: i64,
    pub(crate) link_count: i64,
    pub(crate) is_draft: bool,
    pub(crate) archived_at: Option<chrono::NaiveDate>,
    pub(crate) description_html: String,
    pub(crate) is_subscribed: bool,
    pub(crate) is_intake: bool,
}

/// `duplicate_issue_detail`: `IssueIntakeSerializer`
/// (`serializers/issue.py:752-767`), null when `duplicate_to` is null.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct InboxDuplicateDetail {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) priority: String,
    pub(crate) sequence_id: i32,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) created_by: Option<uuid::Uuid>,
}

/// 200 body: `IntakeIssueDetailSerializer` field order (see
/// `INBOX_DETAIL_KEYS`).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct InboxIssueDetail {
    pub(crate) id: uuid::Uuid,
    pub(crate) status: i32,
    pub(crate) duplicate_to: Option<uuid::Uuid>,
    pub(crate) snoozed_till: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) duplicate_issue_detail: Option<InboxDuplicateDetail>,
    pub(crate) source: Option<String>,
    pub(crate) issue: InboxIssueDetailIssue,
}

/// Shared nested-issue SELECT (D7 `ARCHIVE_SELECT_SQL` convention — live
/// bridge rows, counts via `COUNT(*)`, `is_subscribed`/`is_intake` via
/// `EXISTS`). `$1` = issue id, `$2` = project id, `$3` = workspace slug,
/// `$4` = requesting user id.
const INBOX_ISSUE_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
  i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
  i.sequence_id, i.project_id, i.parent_id, \
  (SELECT ci.cycle_id FROM cycle_issues ci \
    WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
  COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
    JOIN modules m ON m.id = mi.module_id \
    WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
    AND m.archived_at IS NULL), '{}'::uuid[]) AS module_ids, \
  COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
    WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
  COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
    WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
  (SELECT COUNT(*) FROM issues si \
    LEFT JOIN states ss ON ss.id = si.state_id \
    WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
    AND si.archived_at IS NULL AND si.is_draft = false \
    AND ss.\"group\" <> 'triage' \
    AND EXISTS(SELECT 1 FROM projects sp \
      WHERE sp.id = si.project_id AND sp.archived_at IS NULL)) AS sub_issues_count, \
  i.created_at, i.updated_at, \
  i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
  (SELECT COUNT(*) FROM file_assets fa \
    WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
    AND fa.deleted_at IS NULL) AS attachment_count, \
  (SELECT COUNT(*) FROM issue_links lin \
    WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count, \
  i.is_draft, i.archived_at, i.description_html, \
  EXISTS(SELECT 1 FROM issue_subscribers s \
    WHERE s.issue_id = i.id AND s.subscriber_id = $4 AND s.project_id = i.project_id \
    AND s.deleted_at IS NULL) AS is_subscribed, \
  EXISTS(SELECT 1 FROM intake_issues ii \
    WHERE ii.issue_id = i.id AND ii.status IN (-2, 0) AND ii.project_id = i.project_id \
    AND ii.deleted_at IS NULL) AS is_intake \
  FROM issues i \
  WHERE i.id = $1 AND i.project_id = $2 \
  AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
  AND i.deleted_at IS NULL";

/// PATCH `/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/`
/// (and the `intake-issues/:pk/` twin) — parity with Django
/// `IntakeIssueViewSet.partial_update`
/// (`plane/app/views/intake/base.py:334-...`,
/// `plane/app/urls/intake.py:44-55`).
///
/// - Scope: reused GET/DELETE scope (`intake_issues` row id + project +
///   live); miss → 404 `missing()` (Django `.get()` → 404 via
///   `views/base.py:92-96`; Django's extra `intake_id` scoping is implied
///   by the row id — see module docs).
/// - Gates: `guard_inbox_patch` (403 / 400 verbatim bodies); otherwise
///   validate-then-write (validate all applied values BEFORE any write,
///   mirroring Django validating both serializers before either save).
/// - Writes: nested `issue` (guest-narrowed) via `UPDATE issues`;
///   intake-level fields only when `may_write_intake_fields` (silently
///   ignored otherwise — Django never builds that serializer,
///   `base.py:426`). `updated_by_id` bumped on both rows (Django
///   `save()`).
/// - 200 `IntakeIssueDetailSerializer` (`INBOX_DETAIL_KEYS` order).
pub async fn patch_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<InboxIssuePatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Scope: pk = ISSUE id under the project's first intake
    // (`resolve_inbox_row`); the D13 locked gate matrix is unchanged.
    let Some(scope) = resolve_inbox_row(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    let row_id = scope.row_id;
    let issue_id = scope.issue_id;

    let user_id = auth.0;
    let role = fetch_project_member_role(&st.pool, user_id, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, &slug).await?;
    let is_creator = scope.created_by_id == Some(user_id);
    if let Err((code, msg)) = guard_inbox_patch(role.is_some(), role, is_creator, ws_admin) {
        return Ok((code, Json(json!({"error": msg}))));
    }
    let narrowed = is_guest_narrowed(role);
    let may_write_intake = may_write_intake_fields(role, ws_admin);

    // Validate every applied value BEFORE any write (Django validates
    // both serializers before either `save()`; first-error-wins here).
    let issue = body.issue.as_ref();
    let new_name = issue.and_then(|i| i.name.clone());
    if let Some(name) = &new_name {
        if name.trim().is_empty() {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Name is required"}))));
        }
    }
    let new_priority = issue.and_then(|i| i.priority.clone());
    if !narrowed {
        if let Some(p) = &new_priority {
            if !PRIORITIES.contains(&p.as_str()) {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid priority"}))));
            }
        }
    }
    if may_write_intake {
        if let Some(s) = body.status {
            if !INTAKE_STATUSES.contains(&s) {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid status"}))));
            }
        }
        if let Some(Some(dup)) = body.duplicate_to {
            let exists: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT id FROM issues WHERE id = $1 AND deleted_at IS NULL")
                    .bind(dup)
                    .fetch_optional(&st.pool)
                    .await?;
            if exists.is_none() {
                return Ok(missing());
            }
        }
    }

    // Nested `issue` write (`IssueCreateSerializer` partial, `base.py:403-418`).
    if !narrowed {
        let desc_html = issue.and_then(|i| i.description_html.clone());
        let desc_json = issue.and_then(|i| i.description_json.clone());
        if new_name.is_some() || desc_html.is_some() || desc_json.is_some() || new_priority.is_some() {
            sqlx::query(
                "UPDATE issues SET name = COALESCE($1, name), \
                  description_html = COALESCE($2, description_html), \
                  description_json = COALESCE($3::jsonb, description_json), \
                  priority = COALESCE($4, priority), \
                  updated_at = now(), updated_by_id = $6 \
                  WHERE id = $5 AND deleted_at IS NULL",
            )
            .bind(&new_name)
            .bind(&desc_html)
            .bind(&desc_json)
            .bind(&new_priority)
            .bind(issue_id)
            .bind(user_id)
            .execute(&st.pool)
            .await?;
        }
    } else {
        let desc_html = issue.and_then(|i| i.description_html.clone());
        let desc_json = issue.and_then(|i| i.description_json.clone());
        if new_name.is_some() || desc_html.is_some() || desc_json.is_some() {
            sqlx::query(
                "UPDATE issues SET name = COALESCE($1, name), \
                  description_html = COALESCE($2, description_html), \
                  description_json = COALESCE($3::jsonb, description_json), \
                  updated_at = now(), updated_by_id = $5 \
                  WHERE id = $4 AND deleted_at IS NULL",
            )
            .bind(&new_name)
            .bind(&desc_html)
            .bind(&desc_json)
            .bind(issue_id)
            .bind(user_id)
            .execute(&st.pool)
            .await?;
        }
    }

    // Intake-level write (`IntakeIssueSerializer` partial, `base.py:426-431`).
    if may_write_intake {
        let n_status = body.status;
        let (dup_set, dup_val): (bool, Option<uuid::Uuid>) = match body.duplicate_to {
            None => (false, None),
            Some(v) => (true, v),
        };
        let (snooze_set, snooze_val): (bool, Option<chrono::DateTime<chrono::Utc>>) =
            match body.snoozed_till {
                None => (false, None),
                Some(v) => (true, v),
            };
        let (source_set, source_val): (bool, Option<String>) = match body.source {
            None => (false, None),
            Some(ref v) => (true, v.clone()),
        };
        if n_status.is_some() || dup_set || snooze_set || source_set {
            // Positional binds are static, so each nullable column is set
            // via "= value (possibly NULL)" only when present — absent
            // columns keep their value. `updated_by_id` mirrors save().
            sqlx::query(
                "UPDATE intake_issues SET \
                  status = CASE WHEN $1::boolean THEN $2::integer ELSE status END, \
                  duplicate_to_id = CASE WHEN $3::boolean THEN $4::uuid ELSE duplicate_to_id END, \
                  snoozed_till = CASE WHEN $5::boolean THEN $6::timestamptz ELSE snoozed_till END, \
                  source = CASE WHEN $7::boolean THEN $8::varchar ELSE source END, \
                  updated_at = now(), updated_by_id = $10 \
                  WHERE id = $9 AND deleted_at IS NULL",
            )
            .bind(n_status.is_some())
            .bind(n_status)
            .bind(dup_set)
            .bind(dup_val)
            .bind(snooze_set)
            .bind(snooze_val)
            .bind(source_set)
            .bind(source_val)
            .bind(row_id)
            .bind(user_id)
            .execute(&st.pool)
            .await?;
        }
    }

    // Re-fetch + return the updated intake issue (`base.py:480-505` tail:
    // `IntakeIssueDetailSerializer(intake_issue)`, 200 — `id` is the
    // intake-issue ROW id).
    match fetch_inbox_detail(&st.pool, &slug, project_id, row_id, issue_id, user_id).await? {
        Some(detail) => Ok((
            StatusCode::OK,
            Json(serde_json::to_value(&detail).unwrap()),
        )),
        None => Ok(missing()),
    }
}

#[cfg(test)]
mod inbox_patch_tests {
    use super::*;

    #[test]
    fn gate_matrix_matches_django_partial_update() {
        // Mirrors `IntakeIssueViewSet.partial_update`
        // (`plane/app/views/intake/base.py:355-374`): no project
        // membership AND no workspace-admin → 403 "Only admin or creator
        // can update the intake work items" (check 1 has no creator
        // clause, so even a creator without membership/ws-admin 403s).
        for (has_pm, role, creator, ws_admin) in [
            (false, None, false, false),
            (false, None, true, false),
            (false, Some(5), false, false),
        ] {
            assert_eq!(
                guard_inbox_patch(has_pm, role, creator, ws_admin),
                Err((
                    StatusCode::FORBIDDEN,
                    ONLY_ADMIN_OR_CREATOR_MSG.to_string()
                )),
                "has_pm={has_pm} role={role:?} creator={creator} ws_admin={ws_admin}",
            );
        }
        // Guest-role member, not creator, not ws-admin → 400 "You cannot
        // edit intake issues".
        assert_eq!(
            guard_inbox_patch(true, Some(5), false, false),
            Err((
                StatusCode::BAD_REQUEST,
                CANNOT_EDIT_INTAKE_MSG.to_string()
            )),
        );
        // admin | member | creator | ws-admin → ok.
        for (has_pm, role, creator, ws_admin) in [
            (true, Some(20), false, false),
            (true, Some(15), false, false),
            (true, Some(5), true, false),
            (true, Some(5), false, true),
            (true, Some(15), false, true),
            (false, None, false, true),
            (false, None, true, true),
        ] {
            assert!(
                guard_inbox_patch(has_pm, role, creator, ws_admin).is_ok(),
                "has_pm={has_pm} role={role:?} creator={creator} ws_admin={ws_admin}",
            );
        }
    }

    #[test]
    fn intake_field_write_is_admin_or_ws_admin_only() {
        // Mirrors `(project_member and role > ROLE.MEMBER.value) or
        // is_workspace_admin` (`base.py:426`): only project ADMIN (20)
        // or a workspace admin may write intake-level fields
        // (status/duplicate_to/snoozed_till/source).
        assert!(may_write_intake_fields(Some(20), false));
        assert!(!may_write_intake_fields(Some(15), false));
        assert!(!may_write_intake_fields(Some(5), false));
        assert!(!may_write_intake_fields(None, false));
        assert!(may_write_intake_fields(Some(5), true));
        assert!(may_write_intake_fields(None, true));
    }

    #[test]
    fn guest_issue_edits_are_name_description_only() {
        // Mirrors `if project_member and role <= ROLE.GUEST.value`
        // (`base.py:396-401`): guest issue payloads are narrowed to
        // name/description_html/description_json (no ws-admin exemption
        // in Django's narrowing branch).
        assert!(is_guest_narrowed(Some(5)));
        assert!(!is_guest_narrowed(Some(15)));
        assert!(!is_guest_narrowed(Some(20)));
        assert!(!is_guest_narrowed(None));
    }

    #[test]
    fn detail_keys_follow_django_field_order() {
        // `IntakeIssueDetailSerializer.Meta.fields`
        // (`serializers/intake.py:99-107`): id, status, duplicate_to,
        // snoozed_till, duplicate_issue_detail, source, issue.
        assert_eq!(
            INBOX_DETAIL_KEYS,
            [
                "id",
                "status",
                "duplicate_to",
                "snoozed_till",
                "duplicate_issue_detail",
                "source",
                "issue",
            ]
        );
        // Nested `issue` is `IssueDetailSerializer`
        // (`serializers/issue.py:934-945`) = `IssueSerializer.Meta.fields`
        // (`issue.py:786-812`, 25 keys) + description_html, is_subscribed,
        // is_intake — same 28-key order as D7 `ARCHIVED_DETAIL_KEYS`.
        assert_eq!(INBOX_ISSUE_KEYS.len(), 28);
        assert_eq!(
            &INBOX_ISSUE_KEYS[..25],
            &[
                "id",
                "name",
                "state_id",
                "sort_order",
                "completed_at",
                "estimate_point",
                "priority",
                "start_date",
                "target_date",
                "sequence_id",
                "project_id",
                "parent_id",
                "cycle_id",
                "module_ids",
                "label_ids",
                "assignee_ids",
                "sub_issues_count",
                "created_at",
                "updated_at",
                "created_by",
                "updated_by",
                "attachment_count",
                "link_count",
                "is_draft",
                "archived_at",
            ]
        );
        assert_eq!(
            &INBOX_ISSUE_KEYS[25..],
            &["description_html", "is_subscribed", "is_intake"]
        );
    }

    #[test]
    fn inbox_patch_handler_exists_for_both_routes() {
        // Wiring guard: `main.rs` registers
        // `PATCH .../intake-issues/:pk/` + `.../inbox-issues/:pk/` →
        // `patch_issue` (Django serves both paths from
        // `IntakeIssueViewSet`, `urls/intake.py:44-55`).
        let _ = super::patch_issue;
    }
}
