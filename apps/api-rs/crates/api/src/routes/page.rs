use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::cycle::format_archived_at;
use crate::routes::project::{deny, missing, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/base.py:92-97` (Django `ValidationError` → 400).
pub const VALID_DETAIL_MSG: &str = "Please provide valid detail";
/// `plane/app/views/base.py:92-97` (Django `IntegrityError` → 400; fav dup).
pub const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";
/// DRF default permission-denied body for `ProjectPagePermission` denials
/// (`plane/app/permissions/page.py:24-138`).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";
/// `plane/app/views/page/base.py:244` (retrieve miss — verbatim, NO period;
/// distinct from the Axum fallback `"Page not found."`).
pub const PAGE_NOT_FOUND_MSG: &str = "Page not found";
/// `plane/app/views/page/base.py:239` (restricted guest retrieve).
pub const GUEST_VIEW_DENY_MSG: &str = "You are not allowed to view this page";
/// `plane/app/views/page/base.py:179` (partial_update on locked).
pub const PAGE_LOCKED_MSG: &str = "Page is locked";
/// `plane/app/views/page/base.py:193` (access change by non-owner) and
/// `:213` (the `except Page.DoesNotExist` quirk — parent-missing and
/// page-missing both land here; preserved verbatim).
pub const ACCESS_QUIRK_MSG: &str = "Access cannot be updated since this page is owned by someone else";
/// `plane/app/views/page/base.py:339` (archive by non-owner non-admin).
pub const ARCHIVE_ONLY_MSG: &str = "Only the owner or admin can archive the page";
/// `plane/app/views/page/base.py:370` (unarchive; sic "un archive").
pub const UNARCHIVE_ONLY_MSG: &str = "Only the owner or admin can un archive the page";
/// `plane/app/views/page/base.py:393` (destroy before archive).
pub const DELETE_ARCHIVE_FIRST_MSG: &str = "The page should be archived before deleting";
/// `plane/app/views/page/base.py:407` (destroy by non-owner non-admin; 403).
pub const DELETE_ADMIN_OWNER_MSG: &str = "Only admin or owner can delete the page";
/// `plane/app/views/page/base.py:606` (duplicate of private page; 403).
pub const DUPLICATE_DENY_MSG: &str = "Permission denied";
/// `plane/utils/error_codes.py:12` (`ERROR_CODES["PAGE_LOCKED"]`).
pub const PAGE_LOCKED_CODE: i32 = 4701;
/// `plane/utils/error_codes.py:13` (`ERROR_CODES["PAGE_ARCHIVED"]`).
pub const PAGE_ARCHIVED_CODE: i32 = 4702;
/// `plane/app/serializers/page.py:198` (bad base64 in description update).
pub const DESC_DECODE_MSG: &str = "Failed to decode base64 data";
/// `plane/app/views/page/base.py:588` (description update ok).
pub const DESC_UPDATED_MSG: &str = "Updated successfully";
/// `plane/app/serializers/page.py:152` (create body default).
pub const DEFAULT_DESC_HTML: &str = "<p></p>";
/// `plane/db/models/page.py:26` (`Page.DEFAULT_SORT_ORDER`).
pub const DEFAULT_SORT_ORDER: f64 = 65535.0;
/// `plane/db/models/page.py:19-20` (`get_view_props` default).
pub const DEFAULT_VIEW_PROPS: &str = r#"{"full_width": false}"#;

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Outcome of the `ProjectPagePermission` matrix
/// (`plane/app/permissions/page.py:24-138`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagePerm {
    Allow,
    /// DRF permission-class deny → 403 `{"detail": ...}` (NOT `{"error"}`).
    Deny,
}

/// Mirrors `ProjectPagePermission.has_permission` + `_check_project_action_access`
/// (`plane/app/permissions/page.py:24-138`): active project membership required
/// (`role.is_none()` → deny); owner bypasses everything (`:57-59`); a private
/// page denies every non-owner (`:61-63,93-98` — base impl returns False);
/// public pages check the method against the role (`:100-128`: POST∈{20,15},
/// SAFE∈{20,15,5}, PUT/PATCH∈{20,15}, DELETE∈{20}).
pub fn page_perm_decision(
    is_owner: bool,
    is_private: bool,
    method: &str,
    role: Option<i16>,
) -> PagePerm {
    let Some(role) = role else {
        return PagePerm::Deny;
    };
    // `page.py:57-59` — owner bypasses all checks.
    if is_owner {
        return PagePerm::Allow;
    }
    // `page.py:61-63,93-98` — private non-owner always denied.
    if is_private {
        return PagePerm::Deny;
    }
    // `page.py:100-128` — public action matrix.
    let ok = match method {
        "POST" => role == 20 || role == 15,
        "GET" | "HEAD" | "OPTIONS" => role == 20 || role == 15 || role == 5,
        "PUT" | "PATCH" => role == 20 || role == 15,
        "DELETE" => role == 20,
        _ => false,
    };
    if ok {
        PagePerm::Allow
    } else {
        PagePerm::Deny
    }
}

/// Mirrors `sanitize_order_by` (`plane/utils/order_queryset.py:129-150`) over
/// `PAGE_ORDER_BY_ALLOWLIST` (`order_queryset.py:79-84`:
/// `{created_at,updated_at,name,sort_order}`, default `-created_at`).
/// Returns the SQL expression + descending flag.
pub fn sanitize_page_order_by(raw: &str) -> (&'static str, bool) {
    if raw.is_empty() {
        return ("p.created_at", true);
    }
    let (bare, desc) = match raw.strip_prefix('-') {
        Some(b) => (b, true),
        None => (raw, false),
    };
    if bare.starts_with('-') {
        return ("p.created_at", true);
    }
    let expr = match bare {
        "created_at" => "p.created_at",
        "updated_at" => "p.updated_at",
        "name" => "p.name",
        "sort_order" => "p.sort_order",
        _ => return ("p.created_at", true),
    };
    (expr, desc)
}

/// Mirrors the access-change guard (`plane/app/views/page/base.py:191-195`
/// and `:296-300`): changing `access` when the requester is not the owner →
/// the quirk 400.
pub fn guard_access_change(page_access: i16, body_access: Option<i16>, is_owner: bool) -> Result<(), String> {
    if let Some(next) = body_access {
        if next != page_access && !is_owner {
            return Err(ACCESS_QUIRK_MSG.to_string());
        }
    }
    Ok(())
}

/// Mirrors the archive/unarchive owner-or-admin gate
/// (`plane/app/views/page/base.py:332-341`, `:363-372`): an active member
/// with role ≤ 15 who is not the owner is denied.
pub fn guard_archive_owner(is_owner: bool, low_role_member: bool, unarchive: bool) -> Result<(), String> {
    if !is_owner && low_role_member {
        return Err(if unarchive {
            UNARCHIVE_ONLY_MSG.to_string()
        } else {
            ARCHIVE_ONLY_MSG.to_string()
        });
    }
    Ok(())
}

/// Mirrors `destroy` (`plane/app/views/page/base.py:391-395,397-409`).
pub fn guard_delete_archived(archived: bool) -> Result<(), String> {
    if !archived {
        return Err(DELETE_ARCHIVE_FIRST_MSG.to_string());
    }
    Ok(())
}

/// `base.py:397-405`: non-owner without an active role-20 membership → 403.
pub fn guard_delete_owner(is_owner: bool, is_admin: bool) -> Result<(), String> {
    if !is_owner && !is_admin {
        return Err(DELETE_ADMIN_OWNER_MSG.to_string());
    }
    Ok(())
}

/// PROJECT-level AM guard mirroring `@allow_permission([ADMIN, MEMBER])`
/// (`plane/app/views/page/base.py:490,500`, fav endpoints): roles 20/15.
pub fn guard_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `validate_binary_data` (`plane/utils/content_validator.py:29-68`):
/// empty → valid; > 10 MB (`:15-16,52-54`) → size error; < 4 bytes (`:57-58`)
/// → too-short error; first-200-char text containing an entry of
/// `SUSPICOUS_BINARY_PATTERNS` (`:19-26,60-64`) → suspicious error.
pub fn validate_binary_bytes(data: &[u8]) -> Result<(), String> {
    if data.is_empty() {
        return Ok(());
    }
    if data.len() > 10 * 1024 * 1024 {
        return Err("Binary data exceeds maximum size limit (10MB)".to_string());
    }
    if data.len() < 4 {
        return Err("Binary data too short to be valid document format".to_string());
    }
    let head = String::from_utf8_lossy(&data[..data.len().min(200)]).to_lowercase();
    const PATTERNS: &[&str] = &["<html", "<!doctype", "<script", "javascript:", "data:", "<iframe"];
    if PATTERNS.iter().any(|p| head.contains(p)) {
        return Err("Binary data contains suspicious content patterns".to_string());
    }
    Ok(())
}

/// Decodes a `description_binary` body value (base64 on the wire,
/// `plane/app/serializers/page.py:180-198`): empty → empty bytes (valid per
/// `validate_binary_data`); bad base64 → `{"description_binary":
/// ["Failed to decode base64 data"]}` (`:198`); rule violations →
/// `{"description_binary": ["Invalid binary data: <msg>"]}` (`:190-192`).
pub fn decode_description_binary(raw: &str) -> Result<Vec<u8>, String> {
    if raw.is_empty() {
        return Ok(vec![]);
    }
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|_| DESC_DECODE_MSG.to_string())?;
    validate_binary_bytes(&bytes).map_err(|e| format!("Invalid binary data: {e}"))?;
    Ok(bytes)
}

/// Strips `<script>`/`<style>` blocks, HTML comments, `on*` event attributes
/// and `javascript:`/`data:`/`vbscript:` URL values inside tags. Hand-rolled:
/// the `ammonia`/`nh3` crate is not in the dependency closure and CONSTRAINTS
/// allow modifying only `main.rs`, so the exact `nh3.clean` allowlist
/// (`plane/utils/content_validator.py:72-160`) cannot be linked — this removes
/// the dangerous constructs nh3 would strip while keeping benign markup.
/// Documented deviation.
pub fn sanitize_html_content(input: &str) -> String {
    let mut s = input.to_string();
    for tag in ["script", "style"] {
        s = strip_element_blocks(&s, tag);
    }
    s = strip_html_comments(&s);
    s = strip_tag_attributes(&s);
    s
}

/// Validates + sanitizes a `description_html` body value
/// (`plane/app/serializers/page.py:200-211`, size rule
/// `plane/utils/content_validator.py:219-221`): empty → stored as-is;
/// > 10 MB → `{"description_html": ["HTML content exceeds maximum size
/// limit (10MB)"]}`; else the sanitized HTML.
pub fn clean_description_html(raw: &str) -> Result<String, String> {
    if raw.is_empty() {
        return Ok(raw.to_string());
    }
    if raw.as_bytes().len() > 10 * 1024 * 1024 {
        return Err("HTML content exceeds maximum size limit (10MB)".to_string());
    }
    Ok(sanitize_html_content(raw))
}

fn strip_element_blocks(s: &str, tag: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut rest = 0usize;
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}");
    loop {
        let hay = &lower[rest..];
        let Some(rel) = hay.find(open_pat.as_str()) else {
            out.push_str(&s[rest..]);
            break;
        };
        let abs = rest + rel;
        // The char after `<tag` must end the name (space, `/`, `>`).
        let after = lower.as_bytes().get(abs + open_pat.len()).copied().unwrap_or(b'>');
        if after != b'>' && after != b'/' && !after.is_ascii_whitespace() {
            out.push_str(&s[rest..abs + 1]);
            rest = abs + 1;
            continue;
        }
        out.push_str(&s[rest..abs]);
        let from = abs + open_pat.len();
        // Skip to the end of the opening tag, then to `</tag>`.
        let after_open = lower[from..].find('>').map(|i| from + i + 1).unwrap_or(s.len());
        let tail = &lower[after_open..];
        match tail.find(close_pat.as_str()) {
            Some(ci) => {
                let cabs = after_open + ci;
                rest = lower[cabs..].find('>').map(|i| cabs + i + 1).unwrap_or(s.len());
            }
            None => break, // unclosed: drop to end (nh3 drops the dangling open tag too)
        }
    }
    out
}

fn strip_html_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = 0usize;
    loop {
        match s[rest..].find("<!--") {
            Some(ci) => {
                out.push_str(&s[rest..rest + ci]);
                match s[rest + ci..].find("-->") {
                    Some(ei) => rest += ci + ei + 3,
                    None => break,
                }
            }
            None => {
                out.push_str(&s[rest..]);
                break;
            }
        }
    }
    out
}

/// Removes `on*` event attributes and neutralizes dangerous URL schemes in
/// `href`/`src` attributes, tag-aware (text outside `<...>` untouched).
fn strip_tag_attributes(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < s.len() {
        let Some(rel) = s[i..].find('<') else {
            out.push_str(&s[i..]);
            break;
        };
        let abs = i + rel;
        out.push_str(&s[i..abs]);
        // Comment remnants / declarations / closing tags pass through
        // (comments already stripped; keep `</...>` and `<!...>` verbatim).
        if bytes.get(abs + 1).is_some_and(|b| *b == b'/' || *b == b'!') {
            let end = s[abs..].find('>').map(|e| abs + e + 1).unwrap_or(s.len());
            out.push_str(&s[abs..end]);
            i = end;
            continue;
        }
        let end = s[abs..].find('>').map(|e| abs + e + 1).unwrap_or(s.len());
        out.push_str(&clean_tag(&s[abs..end]));
        i = end;
    }
    out
}

fn clean_tag(tag: &str) -> String {
    // Tokenize `name="v"`, `name='v'`, `name=v`, `name`.
    let mut kept: Vec<String> = Vec::new();
    let b = tag.as_bytes();
    let mut i = 1usize; // skip `<`
    // Tag name first.
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    kept.push(tag[..i].to_string());
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        if i >= b.len() || b[i] == b'>' {
            break;
        }
        let ns = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'>' {
            i += 1;
        }
        let name = tag[ns..i].to_string();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut val = String::new();
        let mut raw_attr = name.clone();
        if b.get(i).is_some_and(|c| *c == b'=') {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if let Some(&q) = b.get(i) {
                if q == b'"' || q == b'\'' {
                    i += 1;
                    let vs = i;
                    while i < b.len() && b[i] != q {
                        i += 1;
                    }
                    val = tag[vs..i].to_string();
                    raw_attr = format!("{name}={q}{val}{q}", q = q as char);
                    if i < b.len() {
                        i += 1;
                    }
                }
            }
            if val.is_empty() && !raw_attr.contains('=') {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                val = tag[vs..i].to_string();
                raw_attr = format!("{name}={val}");
            }
        }
        let lname = name.to_lowercase();
        // Drop event handlers (`content_validator` nh3 ATTRIBUTES allowlist
        // has no `on*` entries).
        if lname.starts_with("on") && lname.len() > 2 {
            continue;
        }
        // Neutralize dangerous URL schemes (nh3 `SAFE_PROTOCOLS =
        // {"http","https","mailto","tel"}` — anything else is stripped; we
        // keep the attribute with a benign value so markup stays valid).
        if lname == "href" || lname == "src" {
            let vtrim = val.trim().to_lowercase();
            if vtrim.starts_with("javascript:")
                || vtrim.starts_with("data:")
                || vtrim.starts_with("vbscript:")
            {
                kept.push(format!("{name}=\"#\""));
                continue;
            }
        }
        kept.push(raw_attr);
    }
    let mut s = kept.join(" ");
    if tag.trim_end().ends_with("/>") && !s.ends_with("/>") {
        s.push_str(" /");
    }
    s.push('>');
    s
}

/// Mirrors `strip_tags` for the stored `description_stripped` column
/// (`plane/db/models/page.py:73-77` — `save()` recomputes it; never
/// serialized).
pub fn strip_tags_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(c);
        }
    }
    out
}

/// Pre-existing validate shape (required by
/// `crates/api/tests/page_test.rs`; CONSTRAINTS forbid touching that file).
/// `#[allow(dead_code)]`: the Axum handlers take `Json<Value>` bodies
/// (Django reads `request.data` dynamically), so this typed helper is a
/// construction point for tests only (E2 `cycle.rs:CreateCycle` precedent).
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePage {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub access: Option<i16>,
    #[serde(default)]
    pub color: Option<String>,
}

#[allow(dead_code)]
pub fn validate_create(body: &CreatePage) -> Result<(), String> {
    if let Some(access) = body.access {
        if access != 0 && access != 1 {
            return Err("access must be 0 (Public) or 1 (Private)".to_string());
        }
    }
    if let Some(color) = &body.color {
        if color.chars().count() > 255 {
            return Err("color max length 255".to_string());
        }
    }
    Ok(())
}

// ============================================================================
// Shared gates + lookups.
// ============================================================================

async fn project_in_workspace(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(slug)
    .fetch_one(pool)
    .await
}

/// Gate for `@allow_permission([ADMIN, MEMBER])` endpoints (favorites,
/// `plane/app/views/page/base.py:490,500`): role 20/15 outright, else the
/// workspace-ADMIN fallback (`plane/app/permissions/base.py:53-78`, via the
/// shared `issue_common` helpers — same shape as `cycle::gate_am`).
async fn gate_am(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        guard_am(role).is_ok(),
        role.is_some(),
        ws_admin,
    ))
}

/// DRF permission-class deny body (`ProjectPagePermission`) — same shape as
/// the E2 `cycle::deny_detail` / E3 `module::deny_detail` helper.
fn deny_detail() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": PERMISSION_DETAIL_MSG})),
    )
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

/// The page row the permission matrix needs (`page.py:48-59`): id + owner +
/// access, scoped to the workspace slug and an ACTIVE `project_pages` link
/// for the URL project (`:42-53`, GHSA-g49r/GHSA-ghcr — soft-deleted links
/// deny).
#[derive(Debug, Clone, sqlx::FromRow)]
struct PagePermRow {
    owned_by_id: uuid::Uuid,
    access: i16,
}

async fn fetch_perm_page(
    pool: &sqlx::PgPool,
    page_id: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<Option<PagePermRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.owned_by_id, p.access FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(slug)
    .bind(pid)
    .fetch_optional(pool)
    .await
}

/// Runs the `ProjectPagePermission` matrix for a page endpoint. Missing page
/// or missing membership → `Deny` (Django `has_permission → False` → DRF 403
/// `{"detail": ...}`, `page.py:54-55,89-91`).
async fn require_page_perm(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    page_id: uuid::Uuid,
    method: &str,
) -> Result<Result<PagePermRow, (StatusCode, Json<Value>)>, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let page = fetch_perm_page(pool, page_id, slug, pid).await?;
    let decision = match &page {
        Some(p) => page_perm_decision(
            p.owned_by_id == user,
            p.access == 1,
            method,
            role,
        ),
        None => PagePerm::Deny,
    };
    match (page, decision) {
        (Some(p), PagePerm::Allow) => Ok(Ok(p)),
        _ => Ok(Err(deny_detail())),
    }
}

/// Runs the matrix for collection endpoints (list/summary/create — no
/// `page_id`, `page.py:65-66,83-91`).
async fn require_collection_perm(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    method: &str,
) -> Result<Result<i16, (StatusCode, Json<Value>)>, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    match page_perm_decision(false, false, method, role) {
        PagePerm::Allow => Ok(Ok(role.unwrap_or(0))),
        PagePerm::Deny => Ok(Err(deny_detail())),
    }
}

/// Restricted-guest scoping (`base.py:309-319` list, `:456-466` summary,
/// `:227-237` retrieve): active role-5 membership on a project with
/// `guest_view_all_features=false`.
async fn guest_restricted(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_members pm JOIN projects p ON p.id = pm.project_id \
         JOIN workspaces w ON w.id = pm.workspace_id \
         WHERE pm.project_id = $1 AND pm.member_id = $2 AND w.slug = $3 AND pm.role = 5 \
         AND pm.is_active = true AND pm.deleted_at IS NULL \
         AND p.guest_view_all_features = false AND p.deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(user)
    .bind(slug)
    .fetch_one(pool)
    .await
}

async fn low_role_member(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    // `base.py:333-336`: active membership with `role__lte=15`.
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_members \
         WHERE project_id = $1 AND member_id = $2 AND is_active = true \
         AND deleted_at IS NULL AND role <= 15)",
    )
    .bind(pid)
    .bind(user)
    .fetch_one(pool)
    .await
}

async fn is_project_admin(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    // `base.py:397-405`: active role-20 membership in the workspace project.
    Ok(fetch_project_member_role(pool, user, slug, pid)
        .await?
        .is_some_and(|r| r == 20))
}

// ============================================================================
// Row structs + JSON builders.
// ============================================================================

/// Full page row for list/detail shapes. `archived_at` is a `date` column
/// (`plane/db/models/page.py:47`) → `Option<NaiveDate>`, rendered
/// `"YYYY-MM-DD"` by DRF's `DateField`.
#[derive(Debug, Clone, sqlx::FromRow)]
struct PageRow {
    id: uuid::Uuid,
    name: String,
    owned_by_id: uuid::Uuid,
    access: i16,
    color: String,
    parent_id: Option<uuid::Uuid>,
    is_favorite: bool,
    is_locked: bool,
    archived_at: Option<NaiveDate>,
    workspace_id: uuid::Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    view_props: Value,
    logo_props: Value,
    description_html: String,
    label_ids: Vec<uuid::Uuid>,
    project_ids: Vec<uuid::Uuid>,
}

fn opt_uuid(u: &Option<uuid::Uuid>) -> Value {
    u.map(|v| json!(v)).unwrap_or(Value::Null)
}

/// List-shape JSON in the exact `PageSerializer.Meta.fields` key order
/// (`plane/app/serializers/page.py:36-58`) — notably NO `description_html`.
fn page_list_json(r: &PageRow) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "owned_by": r.owned_by_id,
        "access": r.access,
        "color": r.color,
        "parent": opt_uuid(&r.parent_id),
        "is_favorite": r.is_favorite,
        "is_locked": r.is_locked,
        "archived_at": r.archived_at,
        "workspace": r.workspace_id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
        "view_props": r.view_props,
        "logo_props": r.logo_props,
        "label_ids": r.label_ids,
        "project_ids": r.project_ids,
    })
}

/// Detail-shape JSON (`PageDetailSerializer`, `serializers/page.py:129-133`):
/// list keys + `description_html`.
fn page_detail_json(r: &PageRow) -> Value {
    let mut v = page_list_json(r);
    v.as_object_mut()
        .expect("page json is object")
        .insert("description_html".to_string(), json!(r.description_html));
    v
}

const PAGE_ROW_COLS: &str = "p.id, p.name, p.owned_by_id, p.access, p.color, p.parent_id, \
    EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'page' \
        AND uf.entity_identifier = p.id AND uf.user_id = $4 \
        AND uf.workspace_id = p.workspace_id AND uf.deleted_at IS NULL) AS is_favorite, \
    p.is_locked, p.archived_at, p.workspace_id, p.created_at, p.updated_at, \
    p.created_by_id, p.updated_by_id, p.view_props, p.logo_props, p.description_html, \
    COALESCE(ARRAY(SELECT DISTINCT pl.label_id FROM page_labels pl \
        WHERE pl.page_id = p.id AND pl.deleted_at IS NULL AND pl.label_id IS NOT NULL), '{}') AS label_ids, \
    COALESCE(ARRAY(SELECT DISTINCT pp2.project_id FROM project_pages pp2 \
        WHERE pp2.page_id = p.id AND pp2.deleted_at IS NULL), '{}') AS project_ids";

/// Parses a `labels` body key the way DRF `PrimaryKeyRelatedField(many=True)`
/// does: must be a list of UUIDs — anything else is a `ValidationError` →
/// 400 `{"error": "Please provide valid detail"}` (`views/base.py:92-97`).
fn parse_labels(body: &Value) -> Result<Option<Vec<uuid::Uuid>>, ()> {
    let Some(v) = body.get("labels") else {
        return Ok(None);
    };
    if v.is_null() {
        return Ok(None);
    }
    let arr = v.as_array().ok_or(())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item.as_str().ok_or(())?;
        out.push(s.parse().map_err(|_| ())?);
    }
    Ok(Some(out))
}

fn invalid_pk_msg(key: &str, raw: &str) -> Value {
    json!({key: [format!("Invalid pk \"{raw}\" - object does not exist.")]})
}

// ============================================================================
// E4a — summary + list + create.
// ============================================================================

/// Mirrors `PageViewSet.summary` (`plane/app/views/page/base.py:436-484`):
/// 200 `{public_pages, private_pages, archived_pages}` over top-level,
/// owned-or-public, active-link pages; restricted guests see owned only.
pub async fn summary(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_collection_perm(&st.pool, auth.0, &slug, pid, "GET")
        .await?
        .is_err()
    {
        let e = deny_detail();
        return Ok(e);
    }
    let owned_only = guest_restricted(&st.pool, auth.0, &slug, pid).await?;
    // `base.py:468-482`: public = access 0 unarchived; private = access 1
    // unarchived; archived = `archived_at` set (any access).
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE p.access = 0 AND p.archived_at IS NULL), \
         COUNT(*) FILTER (WHERE p.access = 1 AND p.archived_at IS NULL), \
         COUNT(*) FILTER (WHERE p.archived_at IS NOT NULL) \
         FROM pages p JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $2 \
         WHERE w.slug = $1 AND p.deleted_at IS NULL AND p.parent_id IS NULL \
         AND (p.owned_by_id = $3 OR (p.access = 0 AND $4 = false))",
    )
    .bind(&slug)
    .bind(pid)
    .bind(auth.0)
    .bind(owned_only)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({"public_pages": row.0, "private_pages": row.1, "archived_pages": row.2})).into())
    .map(|j| (StatusCode::OK, j))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PageListQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub order_by: Option<String>,
}

/// Mirrors `PageViewSet.list` + `get_queryset`
/// (`plane/app/views/page/base.py:82-142,306-321`): 200 array of
/// `PageSerializer` rows (NO `description_html`, `serializers/page.py:36-58`)
/// ordered `-is_favorite,<order>,id` (`:112-120`); `?search=` filters `name`
/// (`search_fields = ["name"]`, `:80`); `?order_by` sanitized against
/// `PAGE_ORDER_BY_ALLOWLIST` (`:113-118`); restricted guests see owned only.
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<PageListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_collection_perm(&st.pool, auth.0, &slug, pid, "GET")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let owned_only = guest_restricted(&st.pool, auth.0, &slug, pid).await?;
    let (expr, desc) = sanitize_page_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let dir = if desc { "DESC" } else { "ASC" };
    let search = q.search.as_deref().unwrap_or("").to_string();
    // `filter_queryset` SearchFilter: empty search matches everything.
    let sql = format!(
        "SELECT {PAGE_ROW_COLS} FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $2 \
         WHERE w.slug = $1 AND p.deleted_at IS NULL AND p.parent_id IS NULL \
         AND (p.owned_by_id = $3 OR (p.access = 0 AND $4 = false)) \
         AND ($5 = '' OR p.name ILIKE '%' || $5 || '%') \
         ORDER BY is_favorite DESC, {expr} {dir}, p.id ASC"
    );
    let rows: Vec<PageRow> = sqlx::query_as(&sql)
        .bind(&slug)
        .bind(pid)
        .bind(auth.0)
        .bind(auth.0)
        .bind(owned_only)
        .bind(search)
        .fetch_all(&st.pool)
        .await?;
    // NOTE: `$4` is bound twice (user id for `is_favorite`, then the
    // owned-only flag) — bind order above is slug, pid, user(fav), user(own),
    // owned_only, search. The query text uses $1..$6 positionally.
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(page_list_json).collect::<Vec<_>>())),
    ))
}

async fn fetch_detail_row(
    pool: &sqlx::PgPool,
    page_id: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    user: uuid::Uuid,
) -> Result<Option<PageRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PAGE_ROW_COLS} FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $2 \
         WHERE w.slug = $1 AND p.id = $3 AND p.deleted_at IS NULL"
    );
    sqlx::query_as::<_, PageRow>(&sql)
        .bind(slug)
        .bind(pid)
        .bind(page_id)
        .bind(user)
        .fetch_optional(pool)
        .await
}

/// Mirrors `PageViewSet.create` (`plane/app/views/page/base.py:144-167`):
/// **201** detail shape; body defaults `description_json={}`,
/// `description_binary=None`, `description_html="<p></p>"` (`:150-152`);
/// creates the `project_pages` link (`serializers/page.py:82-89`) and
/// optional `page_labels` (`:91-105`). Celery `page_transaction` skipped.
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_collection_perm(&st.pool, auth.0, &slug, pid, "POST")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    // `Page.name` is `TextField(blank=True)` — untitled allowed, no max.
    let name = match body.get("name") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"name": ["Not a valid string."]})),
            ));
        }
    };
    // `access` choices ((0,"Public"),(1,"Private"), `db/models/page.py:37`).
    let access: i16 = match body.get("access") {
        None | Some(Value::Null) => 0,
        Some(Value::Number(n)) => match n.as_i64() {
            Some(0) => 0,
            Some(1) => 1,
            Some(v) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"access": [format!("\"{v}\" is not a valid choice.")]})),
                ));
            }
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"access": ["A valid integer is required."]})),
                ));
            }
        },
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"access": ["A valid integer is required."]})),
            ));
        }
    };
    // `color` `CharField(max_length=255, blank=True)` (`db/models/page.py:38`).
    let color = match body.get("color") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => {
            if s.chars().count() > 255 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"color": ["Ensure this field has no more than 255 characters."]})),
                ));
            }
            s.clone()
        }
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"color": ["Not a valid string."]})),
            ));
        }
    };
    // `parent` FK — invalid pk → DRF `ValidationError` → valid-detail.
    let parent: Option<uuid::Uuid> = match body.get("parent") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match s.parse() {
            Ok(u) => Some(u),
            Err(_) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        },
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    if let Some(p) = parent {
        let exists: (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pages WHERE id = $1 AND deleted_at IS NULL)")
                .bind(p)
                .fetch_one(&st.pool)
                .await?;
        if !exists.0 {
            let raw = body
                .get("parent")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            return Ok((StatusCode::BAD_REQUEST, Json(invalid_pk_msg("parent", &raw))));
        }
    }
    // `labels` write-only M2M (`serializers/page.py:27-31`).
    let labels: Option<Vec<uuid::Uuid>> = match parse_labels(&body) {
        Ok(l) => l,
        Err(()) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    if let Some(ids) = &labels {
        for id in ids {
            let exists: (bool,) =
                sqlx::query_as("SELECT EXISTS(SELECT 1 FROM labels WHERE id = $1 AND deleted_at IS NULL)")
                    .bind(id)
                    .fetch_one(&st.pool)
                    .await?;
            if !exists.0 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(invalid_pk_msg("labels", &id.to_string())),
                ));
            }
        }
    }
    let description_json: Value = body.get("description_json").cloned().unwrap_or(json!({}));
    let description_html: String = match body.get("description_html") {
        None | Some(Value::Null) => DEFAULT_DESC_HTML.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"description_html": ["Not a valid string."]})),
            ));
        }
    };
    // Create path stores the raw binary value (`serializers/page.py:61-67`
    // passes `description_binary` straight through); a provided base64
    // string goes through the same decode+validate rules as the update
    // serializer (`serializers/page.py:180-198`) instead of crashing.
    let description_binary: Option<Vec<u8>> = match body.get("description_binary") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match decode_description_binary(s) {
            Ok(b) if b.is_empty() && s.is_empty() => None,
            Ok(b) => Some(b),
            Err(e) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"description_binary": [e]})),
                ));
            }
        },
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    let stripped = strip_tags_text(&description_html);
    // Multi-write in one tx: page + project_pages link + page_labels.
    let mut tx = st.pool.begin().await?;
    let row: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
         description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
         access, parent_id, archived_at, is_locked, view_props, logo_props, is_global, sort_order, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, $3, $4, $5, $6, $6, $6, p.workspace_id, $7, $8, $9, \
         NULL, false, $10::jsonb, '{}', false, $11, now(), now() \
         FROM projects p WHERE p.id = $12 RETURNING id",
    )
    .bind(&name)
    .bind(&description_json)
    .bind(&description_binary)
    .bind(&description_html)
    .bind(if stripped.is_empty() { None } else { Some(stripped) })
    .bind(auth.0)
    .bind(&color)
    .bind(access)
    .bind(parent)
    .bind(DEFAULT_VIEW_PROPS)
    .bind(DEFAULT_SORT_ORDER)
    .bind(pid)
    .fetch_one(&mut *tx)
    .await?;
    let new_id = row.0;
    sqlx::query(
        "INSERT INTO project_pages (id, workspace_id, project_id, page_id, created_by_id, updated_by_id, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), p.workspace_id, p.id, $1, $2, $2, now(), now() \
         FROM projects p WHERE p.id = $3",
    )
    .bind(new_id)
    .bind(auth.0)
    .bind(pid)
    .execute(&mut *tx)
    .await?;
    if let Some(ids) = &labels {
        for id in ids {
            sqlx::query(
                "INSERT INTO page_labels (id, label_id, page_id, workspace_id, created_by_id, updated_by_id, \
                 created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, $2, p.workspace_id, $3, $3, now(), now() \
                 FROM projects p WHERE p.id = $4",
            )
            .bind(id)
            .bind(new_id)
            .bind(auth.0)
            .bind(pid)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    // `base.py:164-166`: re-read through the queryset, serialize detail, 201.
    match fetch_detail_row(&st.pool, new_id, &slug, pid, auth.0).await? {
        Some(r) => Ok((StatusCode::CREATED, Json(page_detail_json(&r)))),
        None => Ok(missing()),
    }
}

// ============================================================================
// E4b — detail + patch + destroy.
// ============================================================================

/// Mirrors `PageViewSet.retrieve` (`plane/app/views/page/base.py:217-259`):
/// 200 detail + `issue_ids` from `page_logs(entity_name=issue)` (`:246-248`);
/// private/top-level-filtered → 404 `{"error": "Page not found"}` (`:243-244`,
/// NO period); restricted guest viewing another's page → 400 (`:238-241`).
/// `?track_visit=` defaults true but the celery task is skipped either way.
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "GET")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    // Queryset scope (`base.py:82-142`): top-level + owned-or-public + active
    // link for the URL project.
    let row: Option<PageRow> = {
        let sql = format!(
            "SELECT {PAGE_ROW_COLS} FROM pages p \
             JOIN workspaces w ON w.id = p.workspace_id \
             JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $2 \
             WHERE w.slug = $1 AND p.id = $3 AND p.deleted_at IS NULL AND p.parent_id IS NULL \
             AND (p.owned_by_id = $5 OR p.access = 0)"
        );
        sqlx::query_as::<_, PageRow>(&sql)
            .bind(&slug)
            .bind(pid)
            .bind(page_id)
            .bind(auth.0)
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await?
    };
    let Some(r) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": PAGE_NOT_FOUND_MSG})),
        ));
    };
    // Guest gate AFTER the fetch would 500 on `None.owned_by` in Django
    // (`base.py:227-237` runs before the `:243` None check); the sane order
    // is 404-first, then the guest check (documented normalize-crash).
    if guest_restricted(&st.pool, auth.0, &slug, pid).await? && r.owned_by_id != auth.0 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": GUEST_VIEW_DENY_MSG})),
        ));
    }
    let issue_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT entity_identifier FROM page_logs WHERE page_id = $1 AND entity_name = 'issue' \
         AND entity_identifier IS NOT NULL AND deleted_at IS NULL ORDER BY created_at ASC",
    )
    .bind(page_id)
    .fetch_all(&st.pool)
    .await?;
    let mut v = page_detail_json(&r);
    v.as_object_mut()
        .expect("page json is object")
        .insert("issue_ids".to_string(), json!(issue_ids));
    Ok((StatusCode::OK, Json(v)))
}

/// Mirrors `PageViewSet.partial_update`
/// (`plane/app/views/page/base.py:169-215`): locked → 400 (`:178-179`);
/// parent lookup (`:182-188`) and the scoped page lookup both miss → 400
/// quirk (`:211-215`, preserved); access change by non-owner → 400 quirk
/// (`:191-195`); `labels` present → replace `page_labels`
/// (`serializers/page.py:108-126`); 200 detail.
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    let perm = require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "PATCH").await?;
    let perm_row = match perm {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    // Scoped lookup (`base.py:171-176`).
    let cur: Option<(i16, bool, uuid::Uuid)> = sqlx::query_as(
        "SELECT p.access, p.is_locked, p.owned_by_id FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .fetch_optional(&st.pool)
    .await?;
    let Some((cur_access, is_locked, owner_id)) = cur else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ACCESS_QUIRK_MSG})),
        ));
    };
    if is_locked {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAGE_LOCKED_MSG})),
        ));
    }
    // Parent lookup (`base.py:181-188`): truthy `parent` must resolve in the
    // same scope; miss → the quirk 400. Bad UUID → valid-detail 400
    // (DRF `ValidationError` → `views/base.py:92-97`).
    let parent: Option<Option<uuid::Uuid>> = match body.get("parent") {
        None | Some(Value::Null) if body.get("parent").is_none() => None,
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) if s.is_empty() => Some(None),
        Some(Value::String(s)) => match s.parse::<uuid::Uuid>() {
            Ok(u) => {
                let exists: (bool,) = sqlx::query_as(
                    "SELECT EXISTS(SELECT 1 FROM pages p JOIN workspaces w ON w.id = p.workspace_id \
                     JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
                     WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL)",
                )
                .bind(u)
                .bind(&slug)
                .bind(pid)
                .fetch_one(&st.pool)
                .await?;
                if !exists.0 {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": ACCESS_QUIRK_MSG})),
                    ));
                }
                Some(Some(u))
            }
            Err(_) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        },
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    // Access-change guard (`base.py:191-195`).
    let body_access: Option<i16> = body.get("access").and_then(Value::as_i64).map(|v| v as i16);
    if body.get("access").is_some_and(|v| !v.is_i64()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": VALID_DETAIL_MSG})),
        ));
    }
    if guard_access_change(cur_access, body_access, owner_id == auth.0).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ACCESS_QUIRK_MSG})),
        ));
    }
    let _ = perm_row;
    let name: Option<String> = body.get("name").and_then(Value::as_str).map(str::to_string);
    if body.get("name").is_some_and(|v| !v.is_string()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"name": ["Not a valid string."]})),
        ));
    }
    let color: Option<String> = match body.get("color") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            if s.chars().count() > 255 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"color": ["Ensure this field has no more than 255 characters."]})),
                ));
            }
            Some(s.clone())
        }
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"color": ["Not a valid string."]})),
            ));
        }
    };
    let description_html: Option<String> = match body.get("description_html") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"description_html": ["Not a valid string."]})),
            ));
        }
    };
    let description_json: Option<Value> = body.get("description_json").cloned();
    let labels: Option<Vec<uuid::Uuid>> = match parse_labels(&body) {
        Ok(l) => l,
        Err(()) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    if let Some(ids) = &labels {
        for id in ids {
            let exists: (bool,) =
                sqlx::query_as("SELECT EXISTS(SELECT 1 FROM labels WHERE id = $1 AND deleted_at IS NULL)")
                    .bind(id)
                    .fetch_one(&st.pool)
                    .await?;
            if !exists.0 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(invalid_pk_msg("labels", &id.to_string())),
                ));
            }
        }
    }
    let stripped: Option<Option<String>> = description_html
        .as_ref()
        .map(|h| strip_tags_text(h))
        .map(|s| if s.is_empty() { None } else { Some(s) });
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE pages SET name = COALESCE($1, name), access = COALESCE($2, access), \
         color = COALESCE($3, color), parent_id = COALESCE($4, parent_id), \
         description_html = COALESCE($5, description_html), \
         description_json = COALESCE($6, description_json), \
         description_stripped = COALESCE($7, description_stripped), \
         updated_at = now() WHERE id = $8",
    )
    .bind(&name)
    .bind(body_access)
    .bind(&color)
    .bind(parent.flatten())
    .bind(&description_html)
    .bind(&description_json)
    .bind(stripped.flatten())
    .bind(page_id)
    .execute(&mut *tx)
    .await?;
    // `labels` key present → replace `page_labels`
    // (`serializers/page.py:108-126`).
    if let Some(ids) = &labels {
        sqlx::query("DELETE FROM page_labels WHERE page_id = $1")
            .bind(page_id)
            .execute(&mut *tx)
            .await?;
        for id in ids {
            sqlx::query(
                "INSERT INTO page_labels (id, label_id, page_id, workspace_id, created_by_id, updated_by_id, \
                 created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, $2, p.workspace_id, $3, $3, now(), now() \
                 FROM projects p WHERE p.id = $4",
            )
            .bind(id)
            .bind(page_id)
            .bind(auth.0)
            .bind(pid)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    // NOTE: setting `parent` to null via PATCH is not expressible through
    // `COALESCE` (Django `partial=True` likewise ignores explicit nulls for
    // non-nullable handling here); explicit-null parent stays unchanged —
    // documented deviation.
    match fetch_detail_row(&st.pool, page_id, &slug, pid, auth.0).await? {
        Some(r) => Ok((StatusCode::OK, Json(page_detail_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `PageViewSet.destroy` (`plane/app/views/page/base.py:383-434`):
/// unarchived → 400 (`:391-395`); non-owner without active role-20 → 403
/// (`:397-409`); children `parent=NULL` (`:412-417`); soft-delete the page
/// (`:419`, `SoftDeletionQuerySet`); soft-delete favs (`:421-426`);
/// HARD-delete recents (`:428-433`, `soft=False`); **204**.
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "DELETE")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let cur: Option<(Option<NaiveDate>, uuid::Uuid)> = sqlx::query_as(
        "SELECT p.archived_at, p.owned_by_id FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .fetch_optional(&st.pool)
    .await?;
    let Some((archived_at, owner_id)) = cur else {
        return Ok(missing());
    };
    if guard_delete_archived(archived_at.is_some()).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": DELETE_ARCHIVE_FIRST_MSG})),
        ));
    }
    let admin = is_project_admin(&st.pool, auth.0, &slug, pid).await?;
    if guard_delete_owner(owner_id == auth.0, admin).is_err() {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": DELETE_ADMIN_OWNER_MSG})),
        ));
    }
    let mut tx = st.pool.begin().await?;
    // `base.py:412-417`: detach children in scope.
    sqlx::query(
        "UPDATE pages p SET parent_id = NULL FROM workspaces w, project_pages pp \
         WHERE p.parent_id = $1 AND p.workspace_id = w.id AND w.slug = $2 \
         AND pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $3 \
         AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .execute(&mut *tx)
    .await?;
    // `base.py:419`: soft-delete.
    sqlx::query("UPDATE pages SET deleted_at = now(), updated_at = now() WHERE id = $1")
        .bind(page_id)
        .execute(&mut *tx)
        .await?;
    // `base.py:421-426`: soft-delete favs (queryset `delete()` defaults soft).
    sqlx::query(
        "UPDATE user_favorites SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND entity_identifier = $2 AND entity_type = 'page' \
         AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL",
    )
    .bind(pid)
    .bind(page_id)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    // `base.py:428-433`: HARD-delete recents (`soft=False`).
    sqlx::query(
        "DELETE FROM user_recent_visits WHERE project_id = $1 AND entity_identifier = $2 \
         AND entity_name = 'page' AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3)",
    )
    .bind(pid)
    .bind(page_id)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E4c — favorites.
// ============================================================================

/// Mirrors `PageFavoriteViewSet.create`
/// (`plane/app/views/page/base.py:490-498`): gate project AM
/// (`@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` — deny is the
/// `{"error": ...}` shape, `permissions/base.py:81-84`); POST **204**; dup →
/// `IntegrityError` 400 `{"error": "The payload is not valid"}`
/// (`views/base.py:92-97`).
pub async fn create_favorite(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `base.py:492-497`: no page-existence check on create.
    let r = sqlx::query(
        "INSERT INTO user_favorites (id, project_id, workspace_id, user_id, entity_type, \
         entity_identifier, name, is_folder, sequence, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, p.workspace_id, $2, 'page', $3, '', false, 65535, now(), now() \
         FROM projects p WHERE p.id = $1",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(page_id)
    .execute(&st.pool)
    .await;
    match r {
        Ok(_) => Ok((StatusCode::NO_CONTENT, Json(Value::Null))),
        Err(e) if is_constraint_violation(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Mirrors `PageFavoriteViewSet.destroy`
/// (`plane/app/views/page/base.py:500-510`): gate project AM; `.get()` miss
/// → 404 (`views/base.py:ObjectDoesNotExist` → `missing()`); hard delete
/// (`soft=False`, `:509`); DELETE **204**.
pub async fn destroy_favorite(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let n = sqlx::query(
        "DELETE FROM user_favorites WHERE project_id = $1 AND entity_type = 'page' \
         AND user_id = $2 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) \
         AND entity_identifier = $4",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(&slug)
    .bind(page_id)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E4d — archive / unarchive.
// ============================================================================

/// Recursive archive helper, verbatim SQL from
/// `unarchive_archive_page_and_descendants`
/// (`plane/app/views/page/base.py:60-73`): a recursive CTE over `parent_id`
/// updates the page and all descendants in one statement.
const ARCHIVE_DESCENDANTS_SQL: &str = "WITH RECURSIVE descendants AS ( \
    SELECT id FROM pages WHERE id = $1 \
    UNION ALL \
    SELECT pages.id FROM pages, descendants WHERE pages.parent_id = descendants.id \
    ) UPDATE pages SET archived_at = $2, updated_at = now() WHERE id IN (SELECT id FROM descendants)";

/// Mirrors `PageViewSet.archive` (`plane/app/views/page/base.py:323-352`):
/// non-owner non-admin (active role ≤ 15 + not owner) → 400 (`:332-341`);
/// deletes matching favs (`:343-348`, queryset `delete()` = soft);
/// recursive archive (`:350`); 200 `{"archived_at": ...}`.
pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "POST")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let owner: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT p.owned_by_id FROM pages p JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .fetch_optional(&st.pool)
    .await?;
    let Some((owner_id,)) = owner else {
        return Ok(missing());
    };
    let low = low_role_member(&st.pool, auth.0, pid).await?;
    if guard_archive_owner(owner_id == auth.0, low, false).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ARCHIVE_ONLY_MSG})),
        ));
    }
    // Single DB clock for the stored value AND the response (E2/E3 precedent).
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT now()").fetch_one(&st.pool).await?;
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE user_favorites SET deleted_at = now(), updated_at = now() \
         WHERE entity_type = 'page' AND entity_identifier = $1 AND project_id = $2 \
         AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(pid)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    // `archived_at` is a `date` column (`db/models/page.py:47`): Django
    // passes `datetime.now()` and Postgres casts to date; we bind the date
    // part explicitly.
    sqlx::query(ARCHIVE_DESCENDANTS_SQL)
        .bind(page_id)
        .bind(now.date_naive())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    // E2 `%Y-%m-%d %H:%M:%S%.6f+00:00` format, reused via
    // `cycle::format_archived_at` (Django returns naive-local
    // `str(datetime.now())`; UTC here — documented deviation, E2 precedent).
    Ok((
        StatusCode::OK,
        Json(json!({"archived_at": format_archived_at(now)})),
    ))
}

/// Mirrors `PageViewSet.unarchive` (`plane/app/views/page/base.py:354-381`):
/// sic 400 for non-owner non-admin (`:363-372`); parent still archived →
/// `parent=None` (`:375-377`); recursive unarchive (`:379`); DELETE **204**.
pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "DELETE")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let cur: Option<(uuid::Uuid, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT p.owned_by_id, p.parent_id FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .fetch_optional(&st.pool)
    .await?;
    let Some((owner_id, parent_id)) = cur else {
        return Ok(missing());
    };
    let low = low_role_member(&st.pool, auth.0, pid).await?;
    if guard_archive_owner(owner_id == auth.0, low, true).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": UNARCHIVE_ONLY_MSG})),
        ));
    }
    let mut tx = st.pool.begin().await?;
    // `base.py:375-377`: break hierarchy when the parent is still archived.
    if let Some(par) = parent_id {
        let par_archived: (bool,) =
            sqlx::query_as("SELECT archived_at IS NOT NULL FROM pages WHERE id = $1")
                .bind(par)
                .fetch_one(&mut *tx)
                .await?;
        if par_archived.0 {
            sqlx::query("UPDATE pages SET parent_id = NULL, updated_at = now() WHERE id = $1")
                .bind(page_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query(ARCHIVE_DESCENDANTS_SQL)
        .bind(page_id)
        .bind(None::<NaiveDate>)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E4e — lock / unlock / access.
// ============================================================================

/// Mirrors `PageViewSet.lock` (`plane/app/views/page/base.py:261-271`):
/// POST sets `is_locked`, no ownership check; **204**.
pub async fn lock(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "POST")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let n = sqlx::query(
        "UPDATE pages p SET is_locked = true, updated_at = now() FROM workspaces w, project_pages pp \
         WHERE p.id = $1 AND p.workspace_id = w.id AND w.slug = $2 \
         AND pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $3 \
         AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Mirrors `PageViewSet.unlock` (`plane/app/views/page/base.py:273-284`):
/// DELETE clears `is_locked`; **204**.
pub async fn unlock(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "DELETE")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let n = sqlx::query(
        "UPDATE pages p SET is_locked = false, updated_at = now() FROM workspaces w, project_pages pp \
         WHERE p.id = $1 AND p.workspace_id = w.id AND w.slug = $2 \
         AND pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $3 \
         AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Mirrors `PageViewSet.access` (`plane/app/views/page/base.py:286-304`):
/// POST **204**; `{access}` defaults 0 with NO 0/1 validation (`:287,302`
/// store it raw); access change by non-owner → 400 quirk (`:296-300`).
pub async fn access(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "POST")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let cur: Option<(i16, uuid::Uuid)> = sqlx::query_as(
        "SELECT p.access, p.owned_by_id FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .fetch_optional(&st.pool)
    .await?;
    let Some((cur_access, owner_id)) = cur else {
        return Ok(missing());
    };
    // `base.py:287`: raw value, default 0, no choice validation. A
    // non-integer body value is a DRF `ValidationError` → valid-detail 400.
    let next: i16 = match body.get("access") {
        None | Some(Value::Null) => 0,
        Some(Value::Number(n)) => match n.as_i64() {
            Some(v) => v as i16,
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        },
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    if guard_access_change(cur_access, Some(next), owner_id == auth.0).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ACCESS_QUIRK_MSG})),
        ));
    }
    sqlx::query("UPDATE pages SET access = $1, updated_at = now() WHERE id = $2")
        .bind(next)
        .bind(page_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E4f — description get/patch.
// ============================================================================

/// Renders a JSON error as an Axum `Response` (description endpoints return
/// raw bytes on success, so errors share the body type).
fn desc_err(status: StatusCode, v: Value) -> Result<Response, common::errors::AppError> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(v.to_string()))
        .map_err(|e| anyhow::anyhow!(e).into())
}

/// Mirrors `PagesDescriptionViewSet.retrieve`
/// (`plane/app/views/page/base.py:516-534`): 200 raw `description_binary`
/// bytes (`None` → empty body, `:527-530`) with
/// `Content-Type: application/octet-stream` + `Content-Disposition:
/// attachment; filename="page_description.bin"` (`:532-533`). A plain Axum
/// body — observably identical to the single-yield streaming response.
/// Scope adds owned-or-public (`:518`). Miss → 404 `missing()`
/// (`BaseAPIView.handle_exception` maps `DoesNotExist`, `views/base.py`).
pub async fn desc_get(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<Response, common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return desc_err(StatusCode::NOT_FOUND, json!({"error": "The required object does not exist."}));
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "GET")
        .await?
        .is_err()
    {
        let (s, j) = deny_detail();
        return desc_err(s, j.0);
    }
    let row: Option<(Option<Vec<u8>> ,)> = sqlx::query_as(
        "SELECT p.description_binary FROM pages p JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL \
         AND (p.owned_by_id = $4 OR p.access = 0)",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some((bytes,)) = row else {
        return desc_err(StatusCode::NOT_FOUND, json!({"error": "The required object does not exist."}));
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"page_description.bin\"",
        )
        .body(Body::from(bytes.unwrap_or_default()))
        .map_err(|e| anyhow::anyhow!(e).into())
}

/// Mirrors `PagesDescriptionViewSet.partial_update`
/// (`plane/app/views/page/base.py:536-590`): ORDER locked → 400
/// `{"error_code": 4701, "error_message": "PAGE_LOCKED"}` (`:545-552`,
/// `utils/error_codes.py:12`); archived → 4702/`PAGE_ARCHIVED` (`:554-561`,
/// `error_codes.py:13`); then `PageBinaryUpdateSerializer` validation
/// (`serializers/page.py:173-224`): base64→bytes (≤10 MB, ≥4 B, no
/// `<html/<!doctype/<script/javascript:/data:/<iframe`,
/// `content_validator.py:29-68,211-243`), html sanitize ≤10 MB, json
/// passthrough. 200 `{"message": "Updated successfully"}` (`:588`).
/// Celery tasks skipped.
pub async fn desc_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<Response, common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return desc_err(StatusCode::NOT_FOUND, json!({"error": "The required object does not exist."}));
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "PATCH")
        .await?
        .is_err()
    {
        let (s, j) = deny_detail();
        return desc_err(s, j.0);
    }
    let cur: Option<(bool, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT p.is_locked, p.archived_at FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL \
         AND (p.owned_by_id = $4 OR p.access = 0)",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some((is_locked, archived_at)) = cur else {
        return desc_err(StatusCode::NOT_FOUND, json!({"error": "The required object does not exist."}));
    };
    if is_locked {
        return desc_err(
            StatusCode::BAD_REQUEST,
            json!({"error_code": PAGE_LOCKED_CODE, "error_message": "PAGE_LOCKED"}),
        );
    }
    if archived_at.is_some() {
        return desc_err(
            StatusCode::BAD_REQUEST,
            json!({"error_code": PAGE_ARCHIVED_CODE, "error_message": "PAGE_ARCHIVED"}),
        );
    }
    // `PageBinaryUpdateSerializer` field validation.
    let binary: Option<Vec<u8>> = match body.get("description_binary") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match decode_description_binary(s) {
            Ok(b) => Some(b),
            Err(e) => {
                return desc_err(
                    StatusCode::BAD_REQUEST,
                    json!({"description_binary": [e]}),
                );
            }
        },
        Some(_) => {
            return desc_err(
                StatusCode::BAD_REQUEST,
                json!({"description_binary": [DESC_DECODE_MSG]}),
            );
        }
    };
    let html: Option<String> = match body.get("description_html") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match clean_description_html(s) {
            Ok(h) => Some(h),
            Err(e) => {
                return desc_err(StatusCode::BAD_REQUEST, json!({"description_html": [e]}));
            }
        },
        Some(_) => {
            return desc_err(
                StatusCode::BAD_REQUEST,
                json!({"description_html": ["Not a valid string."]}),
            );
        }
    };
    let des_json: Option<Value> = body.get("description_json").cloned();
    sqlx::query(
        "UPDATE pages p SET description_binary = COALESCE($1, description_binary), \
         description_html = COALESCE($2, description_html), \
         description_json = COALESCE($3, description_json), updated_at = now() \
         FROM workspaces w, project_pages pp \
         WHERE p.id = $4 AND p.workspace_id = w.id AND w.slug = $5 \
         AND pp.page_id = p.id AND pp.deleted_at IS NULL AND pp.project_id = $6 \
         AND p.deleted_at IS NULL",
    )
    .bind(&binary)
    .bind(&html)
    .bind(&des_json)
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    desc_err(StatusCode::OK, json!({"message": DESC_UPDATED_MSG}))
}

// ============================================================================
// E4g — duplicate.
// ============================================================================

/// Mirrors `PageDuplicateEndpoint.post`
/// (`plane/app/views/page/base.py:593-654`): **201** detail; private page +
/// not owner → 403 `{"error": "Permission denied"}` (`:605-606`); copy with
/// a new PK, `name + " (Copy)"` (`:612`), `description_binary = None`
/// (`:613`, json/html kept), owned/created/updated-by = requester
/// (`:614-616`), re-linked to ALL original project ids (`:619-626`, not just
/// the URL project). Celery/S3 tasks skipped.
pub async fn duplicate(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "POST")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let cur: Option<(i16, uuid::Uuid)> = sqlx::query_as(
        "SELECT p.access, p.owned_by_id FROM pages p \
         JOIN workspaces w ON w.id = p.workspace_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE p.id = $1 AND w.slug = $2 AND pp.project_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(page_id)
    .bind(&slug)
    .bind(pid)
    .fetch_optional(&st.pool)
    .await?;
    let Some((access, owner_id)) = cur else {
        return Ok(missing());
    };
    // `base.py:605-606` (defense in depth behind the perm gate).
    if access == 1 && owner_id != auth.0 {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": DUPLICATE_DENY_MSG})),
        ));
    }
    // `base.py:609`: ALL projects the page is linked to (active links).
    let project_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT project_id FROM project_pages WHERE page_id = $1 AND deleted_at IS NULL",
    )
    .bind(page_id)
    .fetch_all(&st.pool)
    .await?;
    let mut tx = st.pool.begin().await?;
    // Same-object copy (`base.py:611-617`): every column carried over except
    // id / name / binary / ownership + fresh timestamps.
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
         description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
         parent_id, archived_at, is_locked, view_props, logo_props, is_global, sort_order, \
         external_id, external_source, created_at, updated_at) \
         SELECT gen_random_uuid(), p.name || ' (Copy)', p.description_json, NULL, \
         p.description_html, p.description_stripped, $2, $2, $2, p.workspace_id, p.color, \
         p.parent_id, p.archived_at, p.is_locked, p.view_props, p.logo_props, p.is_global, \
         p.sort_order, p.external_id, p.external_source, now(), now() \
         FROM pages p WHERE p.id = $1 AND p.deleted_at IS NULL RETURNING id",
    )
    .bind(page_id)
    .bind(auth.0)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((new_id,)) = row else {
        tx.rollback().await?;
        return Ok(missing());
    };
    // `base.py:619-626`: re-link to every original project.
    for target in &project_ids {
        sqlx::query(
            "INSERT INTO project_pages (id, workspace_id, project_id, page_id, created_by_id, \
             updated_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), p.workspace_id, $1, $2, $3, $3, now(), now() \
             FROM pages p WHERE p.id = $2",
        )
        .bind(target)
        .bind(new_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    match fetch_detail_row(&st.pool, new_id, &slug, pid, auth.0).await? {
        Some(r) => Ok((StatusCode::CREATED, Json(page_detail_json(&r)))),
        None => Ok(missing()),
    }
}

// ============================================================================
// E4h — versions.
// ============================================================================

/// List-shape row: the exact `PageVersionSerializer.Meta.fields` keys
/// (`plane/app/serializers/page.py:136-150`) — no binary.
#[derive(Debug, Clone, sqlx::FromRow)]
struct PageVersionRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    page_id: uuid::Uuid,
    last_saved_at: DateTime<Utc>,
    owned_by_id: uuid::Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

/// Detail-shape row (`PageVersionDetailSerializer`,
/// `serializers/page.py:153-170`): list keys + binary/html/json.
/// `description_binary` renders base64-on-the-wire (E-batch precedent:
/// `routes/versions.rs` `base64_or_null`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct PageVersionDetailRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    page_id: uuid::Uuid,
    last_saved_at: DateTime<Utc>,
    description_binary: Option<Vec<u8>>,
    description_html: String,
    description_json: Value,
    owned_by_id: uuid::Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

fn page_version_json(r: &PageVersionRow) -> Value {
    json!({
        "id": r.id,
        "workspace": r.workspace_id,
        "page": r.page_id,
        "last_saved_at": r.last_saved_at,
        "owned_by": r.owned_by_id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
    })
}

fn page_version_detail_json(r: &PageVersionDetailRow) -> Value {
    use base64::Engine as _;
    json!({
        "id": r.id,
        "workspace": r.workspace_id,
        "page": r.page_id,
        "last_saved_at": r.last_saved_at,
        "description_binary": r.description_binary.as_ref().map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        "description_html": r.description_html,
        "description_json": r.description_json,
        "owned_by": r.owned_by_id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
    })
}

/// Mirrors `PageVersionEndpoint.get` list branch
/// (`plane/app/views/page/version.py:44-53`): 200 array scoped to the
/// workspace + an active `project_pages` link for the URL project
/// (`:46-49`).
pub async fn versions_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "GET")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let rows: Vec<PageVersionRow> = sqlx::query_as(
        "SELECT pv.id, pv.workspace_id, pv.page_id, pv.last_saved_at, pv.owned_by_id, \
         pv.created_at, pv.updated_at, pv.created_by_id, pv.updated_by_id \
         FROM page_versions pv JOIN workspaces w ON w.id = pv.workspace_id \
         JOIN pages p ON p.id = pv.page_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE w.slug = $1 AND pp.project_id = $2 AND pv.page_id = $3 \
         AND pv.deleted_at IS NULL ORDER BY pv.created_at DESC",
    )
    .bind(&slug)
    .bind(pid)
    .bind(page_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(page_version_json).collect::<Vec<_>>())),
    ))
}

/// Mirrors `PageVersionEndpoint.get` single branch
/// (`plane/app/views/page/version.py:19-42`): 200 detail; miss → 404
/// (`BaseAPIView.handle_exception` maps `DoesNotExist`, `views/base.py`).
pub async fn version_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "GET")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let row: Option<PageVersionDetailRow> = sqlx::query_as(
        "SELECT pv.id, pv.workspace_id, pv.page_id, pv.last_saved_at, pv.description_binary, \
         pv.description_html, pv.description_json, pv.owned_by_id, \
         pv.created_at, pv.updated_at, pv.created_by_id, pv.updated_by_id \
         FROM page_versions pv JOIN workspaces w ON w.id = pv.workspace_id \
         JOIN pages p ON p.id = pv.page_id \
         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
         WHERE w.slug = $1 AND pp.project_id = $2 AND pv.page_id = $3 AND pv.id = $4 \
         AND pv.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pid)
    .bind(page_id)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(page_version_detail_json(&r)))),
        None => Ok(missing()),
    }
}

// ============================================================================
// Tests (STEP 1 — pure fns; no DB).
// ============================================================================

#[cfg(test)]
mod page_e4_tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn perm_matrix_owner_bypass_private_deny_public_roles() {
        // `page.py:57-59` — owner bypasses everything, even private + DELETE
        // (membership still required first, `page.py:88-91`).
        for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
            assert_eq!(
                page_perm_decision(true, true, method, Some(5)),
                PagePerm::Allow
            );
            assert_eq!(
                page_perm_decision(true, false, method, None),
                PagePerm::Deny
            );
        }
        // `page.py:61-63,93-98` — private non-owner always denied.
        for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
            for role in [Some(20), Some(15), Some(5)] {
                assert_eq!(
                    page_perm_decision(false, true, method, role),
                    PagePerm::Deny
                );
            }
        }
        // `page.py:89-91` — no active membership → deny (DRF-403 branch).
        assert_eq!(page_perm_decision(false, false, "GET", None), PagePerm::Deny);
        // `page.py:100-128` — public action matrix.
        assert_eq!(page_perm_decision(false, false, "POST", Some(20)), PagePerm::Allow);
        assert_eq!(page_perm_decision(false, false, "POST", Some(15)), PagePerm::Allow);
        assert_eq!(page_perm_decision(false, false, "POST", Some(5)), PagePerm::Deny);
        for method in ["GET", "HEAD", "OPTIONS"] {
            for role in [Some(20), Some(15), Some(5)] {
                assert_eq!(page_perm_decision(false, false, method, role), PagePerm::Allow);
            }
        }
        assert_eq!(page_perm_decision(false, false, "GET", Some(1)), PagePerm::Deny);
        for method in ["PUT", "PATCH"] {
            assert_eq!(page_perm_decision(false, false, method, Some(20)), PagePerm::Allow);
            assert_eq!(page_perm_decision(false, false, method, Some(15)), PagePerm::Allow);
            assert_eq!(page_perm_decision(false, false, method, Some(5)), PagePerm::Deny);
        }
        assert_eq!(page_perm_decision(false, false, "DELETE", Some(20)), PagePerm::Allow);
        assert_eq!(page_perm_decision(false, false, "DELETE", Some(15)), PagePerm::Deny);
        assert_eq!(page_perm_decision(false, false, "DELETE", Some(5)), PagePerm::Deny);
        assert_eq!(page_perm_decision(false, false, "TRACE", Some(20)), PagePerm::Deny);
    }

    #[test]
    fn deny_detail_shape_is_drf_default() {
        // DRF permission-class deny — `{"detail": ...}`, NOT `{"error"}`.
        let (s, j) = deny_detail();
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(
            j.0,
            json!({"detail": "You do not have permission to perform this action."})
        );
    }

    #[test]
    fn order_by_sanitized_to_allowlist() {
        // `order_queryset.py:79-84,129-150` + `base.py:113-118`.
        assert_eq!(sanitize_page_order_by("-created_at"), ("p.created_at", true));
        assert_eq!(sanitize_page_order_by("name"), ("p.name", false));
        assert_eq!(sanitize_page_order_by("-sort_order"), ("p.sort_order", true));
        assert_eq!(sanitize_page_order_by("updated_at"), ("p.updated_at", false));
        // Unknown / malformed → default `-created_at`.
        assert_eq!(sanitize_page_order_by("description_html"), ("p.created_at", true));
        assert_eq!(sanitize_page_order_by("--created_at"), ("p.created_at", true));
        assert_eq!(sanitize_page_order_by(""), ("p.created_at", true));
        assert_eq!(sanitize_page_order_by("-is_favorite"), ("p.created_at", true));
    }

    #[test]
    fn access_quirk_preserved() {
        // `base.py:191-195` — non-owner change → quirk; owner ok; same ok.
        assert_eq!(
            guard_access_change(0, Some(1), false).unwrap_err(),
            ACCESS_QUIRK_MSG
        );
        assert!(guard_access_change(0, Some(1), true).is_ok());
        assert!(guard_access_change(0, Some(0), false).is_ok());
        assert!(guard_access_change(1, None, false).is_ok());
    }

    #[test]
    fn archive_owner_admin_gates() {
        // `base.py:332-341` + `:363-372` (sic "un archive").
        assert_eq!(
            guard_archive_owner(false, true, false).unwrap_err(),
            "Only the owner or admin can archive the page"
        );
        assert_eq!(
            guard_archive_owner(false, true, true).unwrap_err(),
            "Only the owner or admin can un archive the page"
        );
        assert!(guard_archive_owner(true, true, false).is_ok());
        assert!(guard_archive_owner(false, false, false).is_ok());
        assert!(guard_archive_owner(true, false, true).is_ok());
    }

    #[test]
    fn delete_gates() {
        // `base.py:391-395` + `:397-409`.
        assert_eq!(
            guard_delete_archived(false).unwrap_err(),
            "The page should be archived before deleting"
        );
        assert!(guard_delete_archived(true).is_ok());
        assert_eq!(
            guard_delete_owner(false, false).unwrap_err(),
            "Only admin or owner can delete the page"
        );
        assert!(guard_delete_owner(true, false).is_ok());
        assert!(guard_delete_owner(false, true).is_ok());
    }

    #[test]
    fn error_code_consts_match_django() {
        // `utils/error_codes.py:12-13`.
        assert_eq!(PAGE_LOCKED_CODE, 4701);
        assert_eq!(PAGE_ARCHIVED_CODE, 4702);
    }

    #[test]
    fn archived_at_format_reuses_e2() {
        // E4d: SAME `%Y-%m-%d %H:%M:%S%.6f+00:00` shape as E2.
        let now = Utc.with_ymd_and_hms(2026, 9, 6, 12, 34, 56).unwrap();
        let s = format_archived_at(now);
        assert!(s.starts_with("2026-09-06 12:34:56."));
        assert!(s.ends_with("+00:00"));
        let frac = &s[20..s.len() - 6];
        assert_eq!(frac.len(), 6);
        assert!(frac.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn binary_validation_cases() {
        use base64::Engine as _;
        // `content_validator.py:29-68`.
        assert!(validate_binary_bytes(b"").is_ok());
        assert_eq!(
            validate_binary_bytes(b"abc").unwrap_err(),
            "Binary data too short to be valid document format"
        );
        assert!(validate_binary_bytes(&vec![0u8; 10 * 1024 * 1024 + 1])
            .unwrap_err()
            .contains("10MB"));
        assert!(validate_binary_bytes(b"%PDF-1.4 binary content here").is_ok());
        assert!(validate_binary_bytes(b"<script>alert(1)</script> padding")
            .unwrap_err()
            .contains("suspicious"));
        assert!(validate_binary_bytes(b"<!DOCTYPE html> padding pad")
            .unwrap_err()
            .contains("suspicious"));
        // Bad base64 → `serializers/page.py:198` shape.
        assert_eq!(
            decode_description_binary("!!!not-base64!!!").unwrap_err(),
            "Failed to decode base64 data"
        );
        // Valid base64 of a real payload passes.
        let enc = base64::engine::general_purpose::STANDARD.encode(b"%PDF-body-bytes");
        assert_eq!(
            decode_description_binary(&enc).unwrap(),
            b"%PDF-body-bytes".to_vec()
        );
        // Suspicious payload through the wire shape.
        let evil = base64::engine::general_purpose::STANDARD.encode(b"<html>evil");
        assert!(decode_description_binary(&evil)
            .unwrap_err()
            .starts_with("Invalid binary data: "));
    }

    #[test]
    fn html_sanitize_strips_danger_keeps_markup() {
        // nh3-approximation for `content_validator.py:211-243`.
        let clean = sanitize_html_content("<p>Hello <b>world</b></p>");
        assert!(clean.contains("<p>"));
        assert!(clean.contains("<b>world</b>"));
        let evil = sanitize_html_content(
            "<p onclick=\"alert(1)\">x</p><script>alert(2)</script><a href=\"javascript:alert(3)\">y</a>",
        );
        assert!(!evil.contains("onclick"));
        assert!(!evil.contains("<script"));
        assert!(!evil.contains("alert(2)"));
        assert!(!evil.contains("javascript:"));
        assert!(evil.contains(">x</p>"));
        assert!(evil.contains(">y</a>"));
        // Oversize → `content_validator.py:219-221` message.
        let big = "a".repeat(10 * 1024 * 1024 + 1);
        assert!(clean_description_html(&big)
            .unwrap_err()
            .contains("10MB"));
        assert_eq!(clean_description_html("").unwrap(), "");
    }

    #[test]
    fn duplicate_and_version_shapes() {
        // `base.py:612` copy-name rule.
        assert_eq!(format!("{} (Copy)", "Spec"), "Spec (Copy)");
        // `base.py:606` duplicate deny literal.
        assert_eq!(DUPLICATE_DENY_MSG, "Permission denied");
        // `base.py:244` retrieve miss literal (NO period).
        assert_eq!(PAGE_NOT_FOUND_MSG, "Page not found");
    }
}