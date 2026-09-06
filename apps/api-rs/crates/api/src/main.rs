#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod middleware;
mod routes;
mod state;

use axum::{
    http::StatusCode,
    middleware as axum_middleware,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde_json::{json, Value};

use crate::middleware::rate_limit::{
    ip_rate_limit_middleware, rate_limit_middleware, IpRateLimiter, RateLimiter,
};
use tracing_subscriber::EnvFilter;

// Django catch-all 404 body, byte-exact from
// `apps/api/plane/app/views/error_404.py:9-10`:
// ```python
// def custom_404_view(request, exception=None):
//     return JsonResponse({"error": "Page not found."}, status=404)
// ```
// Axum fallback for unmatched routes (incl. non-UUID segments); existing
// routes unaffected.
async fn fallback_404() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "Page not found."})),
    )
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    if let Err(e) = common::db::migrate(&pool).await {
        tracing::warn!(error=%e, "migrate failed");
    }
    let redis = redis::Client::open(cfg.redis_url.as_str()).expect("redis client open failed");

    let app = Router::new()
        .route("/health", get(routes::health::health))
        .route(
            "/api/workspaces/",
            get(routes::workspace::list).post(routes::workspace::create),
        )
        .route(
            "/api/workspaces/:slug/",
            get(routes::workspace::detail)
                .patch(routes::workspace::patch)
                .delete(routes::workspace::destroy),
        )
        // Parity with `WorkspaceStatesEndpoint.get`
        // (`views/workspace/state.py:17-...`, `urls/workspace.py:167-171`):
        // GET `StateSerializer[]` with per-group `order`; gate = any
        // ACTIVE ws member incl. GUEST (WorkspaceEntityPermission safe
        // branch, `permissions/workspace.py:74-82`).
        .route("/api/workspaces/:slug/states/", get(routes::workspace::ws_states))
        // Parity with `WorkspaceViewIssuesViewSet.list`
        // (`views/view/base.py:222-259`, `urls/views.py:51-55`):
        // GET 200 offset-paginated 26-key `ViewIssueListSerializer` rows,
        // workspace-scoped with the per-row project-permission predicate
        // (`_get_project_permission_filters`, `view/base.py:155-171`).
        // Gate WORKSPACE ADMIN/MEMBER/GUEST (any active ws member 20/15/5).
        .route("/api/workspaces/:slug/issues/", get(routes::issue_lists::workspace_issues))
        // Parity with `WorkspaceUserProfileIssuesEndpoint.get`
        // (`views/workspace/user.py:98-203`,
        // `urls/workspace.py:152-156`): GET 200 flat `issue_on_results`
        // (assignee∨creator∨subscriber `:uid`, requester ACTIVE project
        // member per row). Gate `WorkspaceViewerPermission` (any active ws
        // member). `group_by==sub_group_by` → 400
        // `{"error": "Group by and sub group by cannot have same parameters"}`
        // (`user.py:176-181`); truthy `group_by` grouped shapes are OUT
        // (Batch F — 400, `archived_list` precedent).
        .route(
            "/api/workspaces/:slug/user-issues/:user_id/",
            get(routes::issue_lists::user_issues),
        )
        .route(
            "/api/workspaces/:slug/projects/",
            get(routes::project::list).post(routes::project::create),
        )
        .route(
            "/api/workspaces/:slug/projects/details/",
            get(routes::project::project_details),
        )
        .route(
            "/api/workspaces/:slug/project-identifiers/",
            get(routes::project::check_identifier),
        )
        // Parity with `ProjectFavoritesViewSet` (`views/project/base.py:498-532`,
        // `urls/project.py:102-111`): POST-only collection + DELETE-only detail.
        // NO GET — Django defines no `serializer_class` for this viewset.
        .route(
            "/api/workspaces/:slug/user-favorite-projects/",
            post(routes::project::fav_add),
        )
        .route(
            "/api/workspaces/:slug/user-favorite-projects/:project_id/",
            delete(routes::project::fav_remove),
        )
        // Parity with `WorkspaceFavoriteEndpoint` list/create
        // (`views/workspace/favorite.py:23-67`,
        // `urls/workspace.py:187-191`): GET 200 `UserFavoriteSerializer[]`
        // (`parent__isnull` + project-member gate; `?all` ignored) + POST
        // **200** (dup entity → 200 existing; race → 400
        // `{"error":"Favorite already exists"}`). Gate WORKSPACE
        // ADMIN/MEMBER (`deny()` 403; Guest → 403); bad slug → 200 `[]`.
        .route(
            "/api/workspaces/:slug/user-favorites/",
            get(routes::favorite::list).post(routes::favorite::create),
        )
        // Parity with `WorkspaceFavoriteEndpoint` patch/delete
        // (`favorite.py:69-82`, `urls/workspace.py:192-196`): PATCH 200 +
        // DELETE **204** HARD (`soft=False`). Lookup user+slug+pk, miss →
        // 404 `missing()` (Django 500s — sane mapping, see `favorite.rs`).
        .route(
            "/api/workspaces/:slug/user-favorites/:fid/",
            patch(routes::favorite::patch).delete(routes::favorite::destroy),
        )
        // Parity with `WorkspaceFavoriteGroupEndpoint.get`
        // (`favorite.py:85-97`, `urls/workspace.py:197-201`): GET 200
        // children of the folder + member gate (no page exclusion, unlike
        // the list twin). Same WORKSPACE ADMIN/MEMBER gate.
        .route(
            "/api/workspaces/:slug/user-favorites/:fid/group/",
            get(routes::favorite::group),
        )
        .route(
            "/api/workspaces/:slug/projects/:pk/",
            get(routes::project::detail)
                .patch(routes::project::patch)
                .delete(routes::project::destroy),
        )
        // Parity with `ProjectArchiveUnarchiveEndpoint`
        // (`views/project/base.py:427-441`, `urls/project.py:122-126`):
        // POST archives (200 `{"archived_at"}`), DELETE restores (204).
        .route(
            "/api/workspaces/:slug/projects/:project_id/archive/",
            post(routes::project::archive).delete(routes::project::unarchive),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/",
            get(routes::issue_query::list).post(routes::issue_write::create),
        )
        // Parity with `IssueListEndpoint.get`
        // (`views/issue/base.py:84-205`): static `list` wins over `:pk` in
        // Axum (no conflict; same as P6/P7 `members/leave/`,
        // `project-members/me/` precedent).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/list/",
            get(routes::issue_query::list_by_ids),
        )
        // Parity with `BulkDeleteIssuesEndpoint.delete`
        // (`views/issue/base.py:773-797`, `urls/issue.py:94-96`): DELETE
        // with JSON body (Axum `Json` reads DELETE bodies; live-curl proof
        // deferred to T13).
        .route(
            "/api/workspaces/:slug/projects/:project_id/bulk-delete-issues/",
            delete(routes::issue_write::bulk_delete),
        )
        // Parity with `BulkArchiveIssuesEndpoint.post`
        // (`views/issue/archive.py:305-343`, `urls/issue.py:99-101`).
        .route(
            "/api/workspaces/:slug/projects/:project_id/bulk-archive-issues/",
            post(routes::issue_write::bulk_archive),
        )
        // Parity with `DeletedIssuesListViewSet.get`
        // (`views/issue/base.py:800-813`, `urls/issue.py:247-249`): bare
        // UUID array, unpaginated.
        .route(
            "/api/workspaces/:slug/projects/:project_id/deleted-issues/",
            get(routes::issue_query::deleted_list),
        )
        // Parity with `IssueArchiveViewSet.list`
        // (`views/issue/archive.py:97-218`, `urls/issue.py:224-225`):
        // flat-list paginated envelope (grouped paginators out of scope).
        .route(
            "/api/workspaces/:slug/projects/:project_id/archived-issues/",
            get(routes::issue_query::archived_list),
        )
        // Parity with `IssueDetailEndpoint.get`
        // (`views/issue/base.py:1027-1103`, `urls/issue.py:48-50`): static
        // `issues-detail` wins over `:pk`-style routes in Axum (same as the
        // `issues/list/` precedent above). FE `getIssuesFromServer`
        // (`issue.service.ts:40-61`) branches here iff `queries.expand`
        // includes `issue_relation` && `!group_by`.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues-detail/",
            get(routes::issue_query::list_detail),
        )
        // Parity with `IssuePaginatedViewSet.list`
        // (`views/issue/base.py:863-972`, `urls/issue.py:54-58`): GET 200
        // cursor-`paginate()` 26-key rows (27 with `?description=true`),
        // fixed `ORDER BY updated_at ASC`, `?updated_at__gt` filter, GUEST
        // scoping to own rows when NOT `guest_view_all_features`
        // (`base.py:910-920`). Gate ADMIN/MEMBER/GUEST (+ ws-admin
        // fallback); project miss → 404. NO `v2/work-items/` — Django
        // defines none.
        .route(
            "/api/workspaces/:slug/projects/:project_id/v2/issues/",
            get(routes::issue_lists::v2_issues),
        )
        // Parity with `SubIssuesEndpoint`
        // (`views/issue/sub_issue.py:37-275`, `urls/issue.py:104-108`):
        // GET lists + POST attaches. The extra `sub-issues` segment keeps
        // it distinct from `.../issues/:pk/` (segment-count distinct, no
        // conflict — same static-vs-dynamic precedent as `issues/list/`).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/sub-issues/",
            get(routes::issue_sub::sub_list).post(routes::issue_sub::sub_add),
        )
        // Parity with `IssueSubscriberViewSet.subscription_status/subscribe/
        // unsubscribe` (`views/issue/subscriber.py:69-104`,
        // `urls/issue.py:185-189`): GET 200 `{"subscribed"}` + POST 201 +
        // DELETE 204 on the issues path ONLY (no work-items/epics variants —
        // Django defines none).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/subscribe/",
            get(routes::subscribe::subscription_status)
                .post(routes::subscribe::subscribe)
                .delete(routes::subscribe::unsubscribe),
        )
        // Parity with `IssueSubscriberViewSet.list`
        // (`views/issue/subscriber.py:52-57`, `urls/issue.py:174-179`): GET
        // returns the `ProjectMemberLite` list (issues path ONLY). Django's
        // URL also maps POST→create, but the Batch D plan scopes D1b to
        // GET + DELETE only, so no POST handler is wired here.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/",
            get(routes::subscribe::subscribers_list),
        )
        // Parity with `IssueSubscriberViewSet.destroy`
        // (`views/issue/subscriber.py:59-67`, `urls/issue.py:180-184`):
        // DELETE 204 (issues path ONLY).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/:subscriber_id/",
            delete(routes::subscribe::subscriber_remove),
        )
        // Parity with `IssueActivityEndpoint.get`
        // (`views/issue/activity.py:30-86`, `urls/issue.py:149-153`):
        // `?activity_type=issue-property` → activities, `=issue-comment`
        // → comments, else merged ASC (issues path ONLY).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/history/",
            get(routes::history::history),
        )
        // Parity with `IssueMetaEndpoint.get`
        // (`views/issue/base.py:1186-1198`, `urls/issue.py:277-279`):
        // 200 `{"sequence_id", "project_identifier"}` (issues path ONLY).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/meta/",
            get(routes::history::meta),
        )
        // Parity with `ProjectUserDisplayPropertyEndpoint`
        // (`views/issue/base.py:743-770`, `urls/issue.py:217-221`):
        // GET 200 + PATCH 200, row auto-created if missing
        // (`get_or_create` — never 404); gate ADMIN/MEMBER/GUEST. NO POST
        // (Django defines none; the FE `issue-display-properties/` POST is
        // FE-dead).
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-properties/",
            get(routes::userprops::project_props_get).patch(routes::userprops::project_props_patch),
        )
        // Parity with `CycleUserPropertiesEndpoint`
        // (`views/cycle/base.py:625-655`, `urls/cycle.py:77-81`):
        // GET 200 (`get_or_create`) + PATCH **201** (missing row → 404);
        // PATCH merges only the 4 filter keys. Gate ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/user-properties/",
            get(routes::userprops::cycle_props_get).patch(routes::userprops::cycle_props_patch),
        )
        // Parity with `ModuleUserPropertiesEndpoint`
        // (`views/module/base.py:825-855`, `urls/module.py:86-90`):
        // same 200/201/404 semantics as the cycle twin. Gate
        // ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:module_id/user-properties/",
            get(routes::userprops::module_props_get).patch(routes::userprops::module_props_patch),
        )
        // Parity with `IssueBulkUpdateDateEndpoint.post`
        // (`views/issue/base.py:1106-1183`, `urls/issue.py:251-255`):
        // POST bulk start/target update; validation merges new-over-current
        // (`%Y-%m-%d`); unknown ids skipped silently; empty `updates` → 200;
        // single bulk UPDATE at the end (NO explicit tx — one statement =
        // atomic); 400 `{"message": ...}` on start>target; gate
        // ADMIN/MEMBER (issues path ONLY — Django defines no work-items
        // variant).
        .route(
            "/api/workspaces/:slug/projects/:project_id/issue-dates/",
            post(routes::issue_dates::bulk_update_dates),
        )
        // Parity with `IssueVersionEndpoint`
        // (`views/issue/version.py:27-74`, `urls/issue.py:256-265`): GET
        // cursor-paginated 10-key list + GET single full snapshot (issues
        // path ONLY). List ordering is the model `Meta.ordering =
        // ("-created_at",)` (`db/models/issue.py:731`); gate
        // ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/",
            get(routes::versions::issue_versions_list),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/:pk/",
            get(routes::versions::issue_version_detail),
        )
        // Parity with `WorkItemDescriptionVersionEndpoint`
        // (`views/issue/version.py:77-144`, `urls/issue.py:266-275`): GET
        // list (explicit `.order_by("-created_at")`) + GET single 14-key
        // detail, with the guest-403 gate. Work-items path ONLY — Django
        // defines NO `issues/:id/description-versions/` route.
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/",
            get(routes::versions::desc_versions_list),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/:pk/",
            get(routes::versions::desc_version_detail),
        )
        // Parity with `IntakeWorkItemDescriptionVersionEndpoint`
        // (`views/intake/base.py:572-640`, `urls/intake.py:56-65`): same
        // 10-key list (NO explicit ordering — mirrored literally) + same
        // 14-key single + same guest-403 gate, over the same
        // `issue_description_versions` table.
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/",
            get(routes::versions::intake_desc_versions_list),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/:pk/",
            get(routes::versions::intake_desc_version_detail),
        )
        // Parity with `IssueArchiveViewSet.retrieve/archive/unarchive`
        // (`views/issue/archive.py:221-302`, `urls/issue.py:228-232`):
        // GET 200 `IssueDetailSerializer` (ARCHIVED ONLY — non-archived →
        // 404) + POST 200 `{"archived_at"}` (state-group check → 400
        // `{"error": ...}`, key `error` NOT `error_code`) + DELETE 204.
        // Gate ADMIN/MEMBER (+ ws-admin fallback). Issues path ONLY.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:pk/archive/",
            get(routes::issue_archive_one::retrieve)
                .post(routes::issue_archive_one::archive)
                .delete(routes::issue_archive_one::unarchive),
        )
        // Parity with `IssueReactionViewSet`
        // (`views/issue/reaction.py:25-85`, `urls/issue.py:191-201`): GET 200
        // serializer list via `get_queryset` (no custom `list` — scope
        // ws+project+issue + active member + archived-null, created_at DESC)
        // + POST **201** (`IssueReactionSerializer`, `serializers/issue.py:
        // 649-655`, `__all__`+`actor_detail`) + DELETE **204** scoped
        // `(slug,project,issue,reaction,actor=user)`. Dup POST → NO explicit
        // catch → 400 `{"error":"The payload is not valid"}` via the base
        // handler. `:reaction_code` is `str` (`urls/issue.py:198`). Gate
        // ADMIN/MEMBER/GUEST (+ ws-admin fallback). Issues path ONLY.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/reactions/",
            get(routes::reactions::issue_reactions_list)
                .post(routes::reactions::issue_reaction_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/reactions/:reaction_code/",
            delete(routes::reactions::issue_reaction_destroy),
        )
        // Parity with `CommentReactionViewSet`
        // (`views/issue/comment.py:163-239`, `urls/issue.py:203-213`): same
        // GET/POST/DELETE shape over `comment_reactions`, scoped to the
        // comment instead of the issue (`CommentReactionSerializer`,
        // `serializers/issue.py:666-685`, 12 keys incl. `display_name`).
        // Dup POST → explicit catch → 400
        // `{"error":"Reaction already exists for the user"}`
        // (`comment.py:206-210` — differs from the issue twin). Gate
        // ADMIN/MEMBER/GUEST (+ ws-admin fallback). Project-level comments
        // path ONLY (Django nests under project, not under issue).
        .route(
            "/api/workspaces/:slug/projects/:project_id/comments/:comment_id/reactions/",
            get(routes::reactions::comment_reactions_list)
                .post(routes::reactions::comment_reaction_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/comments/:comment_id/reactions/:reaction_code/",
            delete(routes::reactions::comment_reaction_destroy),
        )
        // Parity with `WorkspaceDraftIssueViewSet.list/create`
        // (`views/workspace/draft.py:97-154`,
        // `urls/workspace.py:202-206`): GET 200 paginated own-drafts-only
        // `DraftIssueSerializer` + POST **201** (same 21 keys). Gate
        // WORKSPACE ADMIN/MEMBER/GUEST (list/create AMG).
        .route(
            "/api/workspaces/:slug/draft-issues/",
            get(routes::draft::list).post(routes::draft::create),
        )
        // Parity with `WorkspaceDraftIssueViewSet.retrieve/partial_update/
        // destroy` (`draft.py:156-203`, `urls/workspace.py:207-211`):
        // GET 200 detail (miss → 404 standard) + PATCH **204** (miss →
        // 404 `{"error":"Issue not found"}` verbatim, NON-standard) +
        // DELETE **204**. Gates WORKSPACE level: PATCH ADMIN+MEMBER+creator,
        // retrieve ADMIN+creator, destroy ADMIN-or-creator.
        .route(
            "/api/workspaces/:slug/draft-issues/:pk/",
            get(routes::draft::retrieve)
                .patch(routes::draft::partial_update)
                .delete(routes::draft::destroy),
        )
        // Parity with `WorkspaceDraftIssueViewSet.create_draft_to_issue`
        // (`draft.py:205-311`, `urls/workspace.py:212-216`): POST **201**;
        // no-project → 400
        // `{"error":"Project is required to create an issue."}`. Gate
        // WORKSPACE ADMIN+MEMBER (no creator requirement). Celery skipped.
        .route(
            "/api/workspaces/:slug/draft-to-issue/:draft_id/",
            post(routes::draft::create_draft_to_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/",
            get(routes::cycle::list).post(routes::cycle::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:pk/",
            get(routes::cycle::detail)
                .patch(routes::cycle::patch)
                .delete(routes::cycle::destroy),
        )
        // Parity with `CycleIssueViewSet.list/create`
        // (`views/cycle/issue.py:108-221`, `urls/cycle.py:39-43`): GET 200
        // paginated envelope (cursor/per_page default/max 1000, order_by,
        // cycle/link/attachment/sub-issues annotations; grouped shapes OUT —
        // flat envelope) + POST **201** `{"message":"success"}`. Gate
        // ADMIN/MEMBER. `group_by==sub_group_by` → 400 verbatim.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/cycle-issues/",
            get(routes::cycle::cycle_issues_list).post(routes::cycle::cycle_issues_create),
        )
        // Parity with `CycleIssueViewSet.destroy`
        // (`views/cycle/issue.py:319-343`, `urls/cycle.py:44-55`): DELETE
        // **204** always, even with 0 rows (soft-delete). Gate ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/cycle-issues/:issue_id/",
            delete(routes::cycle::cycle_issue_destroy),
        )
        // Parity with `CycleDateCheckEndpoint.post`
        // (`views/cycle/base.py:520-556`, `urls/cycle.py:56-60`): POST 200
        // `{"status":true}`; overlap → **200** (NOT 4xx) verbatim error +
        // `status:false`. Gate ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/date-check/",
            post(routes::cycle::date_check),
        )
        // Parity with `CycleFavoriteViewSet.create/destroy`
        // (`views/cycle/base.py:559-591`, `urls/cycle.py:61-70`): POST 204
        // (dup → 400 `{"error":"The payload is not valid"}`, no existence
        // check) + DELETE 204 (miss → 404). Gate ADMIN/MEMBER. NO GET —
        // the E2 contract wires POST+DELETE only.
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-cycles/",
            post(routes::cycle::fav_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-cycles/:cycle_id/",
            delete(routes::cycle::fav_destroy),
        )
        // Parity with `TransferCycleIssueEndpoint.post`
        // (`views/cycle/base.py:594-622`, `urls/cycle.py:71-75`,
        // `utils/cycle_transfer_issues.py`): POST 200 `{"message":"Success"}`
        // + progress-snapshot write on the SOURCE cycle (verified Django
        // behavior) + backlog/unstarted/started-only move. Gate ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/transfer-issues/",
            post(routes::cycle::transfer),
        )
        // Parity with `CycleArchiveUnarchiveEndpoint.get`
        // (`views/cycle/archive.py:271-304,305-584`, `urls/cycle.py:86-95`):
        // archived-only list/detail 200 (list shape + started/unstarted/
        // backlog + archived_at, NO logo_props/version/created_by; detail +
        // distribution/estimate_distribution). Gate ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/archived-cycles/",
            get(routes::cycle::archived_list),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/archived-cycles/:pk/",
            get(routes::cycle::archived_detail),
        )
        // Parity with `CycleArchiveUnarchiveEndpoint.post/delete`
        // (`views/cycle/archive.py:586-611`, `urls/cycle.py:81-85`): POST
        // 200 `{"archived_at"}` (non-completed → 400 verbatim) + DELETE 204.
        // Gate ADMIN/MEMBER. NO GET on this path (Django defines none).
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/archive/",
            post(routes::cycle::archive).delete(routes::cycle::unarchive),
        )
        // Parity with `CycleProgressEndpoint.get`
        // (`views/cycle/base.py:658-783`, `urls/cycle.py:96-100`): GET 200,
        // 12 keys (snapshot counts win when present; points-estimate only;
        // total may be null). Gate ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/progress/",
            get(routes::cycle::progress),
        )
        // Parity with `CycleAnalyticsEndpoint.get`
        // (`views/cycle/base.py:786-1048`, `urls/cycle.py:101-105`): GET 200
        // `{assignees,labels,completion_chart}` (snapshot-or-live; points
        // branch only with a points estimate). Gate ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/analytics/",
            get(routes::cycle::analytics),
        )
        // Parity with `WorkspaceCyclesEndpoint.get`
        // (`views/workspace/cycle.py:19-132`, `urls/workspace.py:183-185`):
        // GET 200 cross-project `CycleSerializer[]` (member-projects only,
        // archived excluded; NO favorite/status/assignee annotations; WITH
        // started/unstarted/backlog counts). Gate = any ACTIVE ws member;
        // deny is the DRF permission-class 403 `{"detail": ...}`.
        .route("/api/workspaces/:slug/cycles/", get(routes::cycle::workspace_cycles))
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/",
            get(routes::module::list).post(routes::module::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:pk/",
            get(routes::module::detail)
                .put(routes::module::update)
                .patch(routes::module::patch)
                .delete(routes::module::destroy),
        )
        // Parity with `ModuleIssueViewSet.list/create_module_issues`
        // (`views/module/issue.py:94-254`, `urls/module.py:41-45`): GET 200
        // cursor-paginated envelope (order default `created_at` ASC,
        // module/link/attachment/sub-issues annotations; grouped shapes OUT
        // — flat envelope) + POST **201** `{"message":"success"}` (issues
        // re-scoped to ws+project, unknown silently dropped). Gate
        // ADMIN/MEMBER. `group_by==sub_group_by` → 400 verbatim.
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:module_id/issues/",
            get(routes::module::issues_list).post(routes::module::issues_create),
        )
        // Parity with `ModuleIssueViewSet.create_issue_modules`
        // (`views/module/issue.py:256-323`, `urls/module.py:36-40`): POST
        // **201** `{"message":"success"}` always, even with empty lists;
        // added modules NOT scoped (replicated as-is). Gate ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/modules/",
            post(routes::module::issue_modules_create),
        )
        // Parity with `ModuleIssueViewSet.destroy`
        // (`views/module/issue.py:325-345`, `urls/module.py:46-57`): DELETE
        // **204** always, even with 0 rows (soft-delete; missing link → 204
        // idempotent, Django `.first().module` crash normalized). Gate
        // ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:module_id/issues/:issue_id/",
            delete(routes::module::issue_destroy),
        )
        // Parity with `ModuleLinkViewSet.list/create`
        // (`views/module/base.py:762-788`, `urls/module.py:58-62`): GET 200
        // (order `-created_at`) + POST **201** (prepend `http://` when the
        // scheme is missing). Gate SAFE = any active member incl GUEST,
        // unsafe = ADMIN/MEMBER (DRF `{"detail": ...}` deny).
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:module_id/module-links/",
            get(routes::module::links_list).post(routes::module::links_create),
        )
        // Parity with `ModuleLinkViewSet.retrieve/update/partial_update/
        // destroy` (`urls/module.py:63-74`): GET 200 + PUT/PATCH 200 (dup
        // url on update → 400 sic `"URL already exists for this Issue"`) +
        // DELETE **204**. Bad url → 400 field errors. Same entity gate.
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:module_id/module-links/:pk/",
            get(routes::module::link_detail)
                .put(routes::module::link_put)
                .patch(routes::module::link_patch)
                .delete(routes::module::link_destroy),
        )
        // Parity with `ModuleFavoriteViewSet.create/destroy`
        // (`views/module/base.py:791-822`, `urls/module.py:75-84`): POST 204
        // (dup → 400 `{"error":"The payload is not valid"}`, NO
        // module-existence check) + DELETE 204 (miss → 404). Gate Lite (any
        // active member; DRF `{"detail": ...}` deny). NO GET — the E3
        // contract wires POST+DELETE only.
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-modules/",
            post(routes::module::fav_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-modules/:module_id/",
            delete(routes::module::fav_destroy),
        )
        // Parity with `ModuleArchiveUnarchiveEndpoint.post/delete`
        // (`views/module/archive.py:544-565`, `urls/module.py:90-94`): POST
        // 200 `{"archived_at"}` (wrong status → 400 verbatim) + DELETE 204
        // (no status check). Gate ADMIN/MEMBER (DRF `{"detail": ...}` deny).
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:module_id/archive/",
            post(routes::module::archive).delete(routes::module::unarchive),
        )
        // Parity with `ModuleArchiveUnarchiveEndpoint.get`
        // (`views/module/archive.py:258-309,310-542`, `urls/module.py:
        // 95-104`): archived-only list/detail 200 (list shape OMITS
        // `logo_props,estimate_points`; detail + link/sub-issues/
        // distribution/estimate_distribution). Gate SAFE (any active
        // member; DRF `{"detail": ...}` deny).
        .route(
            "/api/workspaces/:slug/projects/:project_id/archived-modules/",
            get(routes::module::archived_list),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/archived-modules/:pk/",
            get(routes::module::archived_detail),
        )
        // Parity with `WorkspaceModulesEndpoint.get`
        // (`views/workspace/module.py:25-132`): GET 200 cross-project
        // `ModuleSerializer[]` (member-projects only, archived excluded).
        // Gate = any ACTIVE ws member; deny is the DRF permission-class 403
        // `{"detail": ...}`.
        .route("/api/workspaces/:slug/modules/", get(routes::module::workspace_modules))
        .route(
            "/api/workspaces/:slug/projects/:project_id/states/",
            get(routes::state::list).post(routes::state::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/states/:pk/",
            get(routes::state::detail)
                .patch(routes::state::patch)
                .delete(routes::state::destroy),
        )
        // Parity with `StateViewSet.mark_as_default`
        // (`views/state/base.py:104-110`, `urls/state.py:27-31`):
        // POST blind clear+set → 204 unconditional.
        .route(
            "/api/workspaces/:slug/projects/:project_id/states/:pk/mark-default/",
            post(routes::state::mark_default),
        )
        // Parity with `IntakeStateEndpoint.get`
        // (`views/state/base.py:136-...`, `urls/state.py:22-26`): 200
        // `StateSerializer` (no `order` key); miss → 404
        // `{"error":"Triage state not found"}` verbatim; gate PROJECT
        // ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-state/",
            get(routes::state::intake_state),
        )
        // Parity with `WorkspaceLabelsEndpoint.get`
        // (`views/workspace/label.py:17-30`, `urls/workspace.py:157-161`):
        // GET 200 `LabelSerializer[]` scoped to the caller's active
        // projects (archived excluded); gate `WorkspaceViewerPermission` =
        // any ACTIVE ws member incl. GUEST.
        .route("/api/workspaces/:slug/labels/", get(routes::label::ws_labels))
        // Parity with `LabelViewSet.list/create`
        // (`views/issue/label.py:23-55`, `urls/issue.py:71-75`): GET 200
        // `LabelSerializer[]` (`ORDER BY sort_order`, scoped ws+project+
        // caller membership; read gate `ProjectBasePermission` safe-methods
        // = any active ws member) + POST **201** (create gate ADMIN
        // project-level + ws-admin fallback; dup → 400 `{"error": ...}`
        // via the IntegrityError branch, `label.py:51-55`). Detail
        // `issue-labels/:pk/` below stays on the pre-existing handlers.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issue-labels/",
            get(routes::label::issue_labels_list).post(routes::label::issue_labels_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/labels/",
            get(routes::label::list).post(routes::label::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/labels/:pk/",
            get(routes::label::detail)
                .patch(routes::label::patch)
                .delete(routes::label::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issue-labels/:pk/",
            get(routes::label::detail)
                .patch(routes::label::patch)
                .delete(routes::label::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/",
            get(routes::estimate::list).post(routes::estimate::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/",
            get(routes::estimate::detail)
                .patch(routes::estimate::patch)
                .delete(routes::estimate::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/estimate-points/",
            post(routes::estimate::create_point),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/estimate-points/:point_id/",
            get(routes::estimate::detail)
                .patch(routes::estimate::patch_point)
                .delete(routes::estimate::destroy_point),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intakes/",
            get(routes::intake::list).post(routes::intake::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intakes/:pk/",
            get(routes::intake::detail)
                .patch(routes::intake::patch)
                .delete(routes::intake::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inboxes/",
            get(routes::intake::list).post(routes::intake::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inboxes/:pk/",
            get(routes::intake::detail)
                .patch(routes::intake::patch)
                .delete(routes::intake::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/",
            get(routes::intake::detail_issue)
                .patch(routes::intake::patch_issue)
                .delete(routes::intake::destroy_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/",
            get(routes::intake::detail_issue)
                .patch(routes::intake::patch_issue)
                .delete(routes::intake::destroy_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/members/",
            get(routes::member::list).post(routes::member::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members/",
            get(routes::member::list).post(routes::member::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members-lite/",
            get(routes::member::list_lite),
        )
        .route("/api/workspaces/:slug/members/", get(routes::member::list_workspace_members))
        .route("/api/workspaces/:slug/members-lite/", get(routes::member::list_workspace_members))
        // Parity with `WorkSpaceMemberViewSet.leave`
        // (`views/workspace/member.py:160-205`, `urls/workspace.py:103-105`
        // `{"post": "leave"}` — POST only): static `leave` wins over `:pk`
        // in Axum (no conflict; `members/leave/` project precedent above).
        .route("/api/workspaces/:slug/members/leave/", post(routes::member::ws_leave))
        // Parity with `WorkSpaceMemberViewSet.retrieve/partial_update/
        // destroy` (`views/workspace/member.py:57-150`,
        // `urls/workspace.py:98-100`): GET 200 (miss → 404 verbatim) +
        // PATCH 200 (role==5 cascades project roles) + DELETE **204**
        // (soft-deactivate). Gate ADMIN for write paths.
        .route(
            "/api/workspaces/:slug/members/:pk/",
            get(routes::member::ws_member_detail)
                .patch(routes::member::ws_member_patch)
                .delete(routes::member::ws_member_destroy),
        )
        // Parity with `WorkspaceMemberUserEndpoint.get`
        // (`views/workspace/member.py:217-234`,
        // `urls/workspace.py:113-115`): GET 200 (non-member → 200 null).
        .route(
            "/api/workspaces/:slug/workspace-members/me/",
            get(routes::member::ws_me),
        )
        // Parity with `WorkspaceProjectMemberEndpoint.get`
        // (`views/workspace/member.py:237-266`,
        // `urls/workspace.py:93-95`): GET 200 `{project_id: [...]}`;
        // non-member → DRF 403 `{"detail": ...}`.
        .route("/api/workspaces/:slug/project-members/", get(routes::member::ws_project_members))
        // Parity with `WorkspaceInvitationsViewset` list/create
        // (`views/workspace/invite.py:52-128`,
        // `urls/workspace.py:66-68`): GET 200 (ADMIN/MEMBER, guest → DRF
        // 403) + POST **200** `{"message":"Emails sent successfully"}`.
        // Celery sends skipped.
        .route(
            "/api/workspaces/:slug/invitations/",
            get(routes::invite::ws_list).post(routes::invite::ws_create),
        )
        // Parity with `WorkspaceInvitationsViewset.retrieve/partial_update/
        // destroy` (`urls/workspace.py:71-73`): GET 200 + PATCH 200
        // (role/accepted writable) + DELETE **204** (HARD delete,
        // `invite.py:130-133` — verified, no soft override).
        .route(
            "/api/workspaces/:slug/invitations/:pk/",
            get(routes::invite::ws_detail)
                .patch(routes::invite::ws_patch)
                .delete(routes::invite::ws_destroy),
        )
        // Parity with `WorkspaceJoinEndpoint`
        // (`views/workspace/invite.py:149-233`,
        // `urls/workspace.py:82-84`): GET 200 public shape (no auth, no
        // token/email needed; token+invite_link omitted) + POST 200
        // accept/reject (token equality → 403s; anon → generic 401).
        .route(
            "/api/workspaces/:slug/invitations/:pk/join/",
            get(routes::invite::ws_join_get).post(routes::invite::ws_join_post),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/views/",
            get(routes::view::list).post(routes::view::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/views/:pk/",
            get(routes::view::detail)
                .patch(routes::view::patch)
                .delete(routes::view::destroy),
        )
        .route(
            "/api/workspaces/:slug/views/",
            get(routes::view::list_global).post(routes::view::create_global),
        )
        .route(
            "/api/workspaces/:slug/views/:pk/",
            get(routes::view::detail_global),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/members/:pk/",
            get(routes::member::detail)
                .patch(routes::member::patch)
                .delete(routes::member::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members/:pk/",
            get(routes::member::detail)
                .patch(routes::member::patch)
                .delete(routes::member::destroy),
        )
        // Parity with `ProjectMemberViewSet.leave`
        // (`views/project/member.py:323-349`, `urls/project.py:88-89`
        // `{"post": "leave"}` — POST only): static `leave` wins over `:pk`
        // in Axum (no conflict; live-curl proof deferred to T13). No
        // `project-members/leave/` alias — Django defines none.
        .route(
            "/api/workspaces/:slug/projects/:project_id/members/leave/",
            post(routes::member::leave_project),
        )
        // Parity with `ProjectMemberPreferenceEndpoint`
        // (`views/project/member.py:382-408`, `urls/project.py:128-130`):
        // GET 200 + PATCH 200 (body IS the preferences object, merged
        // shallow); miss → 404. Gate ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/preferences/member/:member_id/",
            get(routes::member::pref_get).patch(routes::member::pref_patch),
        )
        // Parity with `ProjectInvitationsViewset` list/create
        // (`views/project/invite.py:56-116`, `urls/project.py:53-55`): GET
        // 200 (IsAuthenticated-only) + POST **200**
        // `{"message":"Email sent successfully"}` (gate ADMIN; role-vs-ws
        // check → intended 400 — Django returns 200 by omitting `status=`,
        // plus two crash bugs fixed; see `invite.rs` E5f header).
        .route(
            "/api/workspaces/:slug/projects/:project_id/invitations/",
            get(routes::invite::proj_list).post(routes::invite::proj_create),
        )
        // Parity with `ProjectInvitationsViewset.retrieve/destroy`
        // (`urls/project.py:58-61`): GET 200 + DELETE **204** (HARD delete
        // — default DRF destroy, no soft override). IsAuthenticated-only.
        .route(
            "/api/workspaces/:slug/projects/:project_id/invitations/:pk/",
            get(routes::invite::proj_detail).delete(routes::invite::proj_destroy),
        )
        // Parity with `ProjectJoinEndpoint`
        // (`views/project/invite.py:195-286`, `urls/project.py:73-75`):
        // GET 200 public shape (NO email/token keys) + POST 200
        // accept/reject (non-bool `accepted` → verbatim 400; ws role cap +
        // role-keeping reactivation quirks replicated).
        .route(
            "/api/workspaces/:slug/projects/:project_id/join/:pk/",
            get(routes::invite::proj_join_get).post(routes::invite::proj_join_post),
        )
        // Parity with `ProjectMemberUserEndpoint`
        // (`views/project/member.py:352-362`, `urls/project.py:97-101`):
        // static `me` wins over `:pk` in Axum (no conflict; live-curl
        // proof deferred to T13).
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members/me/",
            get(routes::project::my_membership),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-views/",
            get(routes::view::list_favorites).post(routes::view::create_favorite),
        )
        // Parity with `IssueViewFavoriteViewSet.destroy`
        // (`views/view/base.py:435-444`, `urls/views.py:61-64`): DELETE
        // **204** HARD (`soft=False`), miss → 404. Gate project
        // ADMIN/MEMBER (+ ws-admin fallback). The POST twin stays on the
        // pre-existing `view.rs` handler; NO GET is added here (locked §4
        // broken-list rule — the pre-existing collection GET is untouched).
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-views/:view_id/",
            delete(routes::favorite::view_fav_destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/",
            get(routes::page::list).post(routes::page::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/",
            get(routes::page::detail)
                .patch(routes::page::patch)
                .delete(routes::page::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages-summary/",
            get(routes::page::summary),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/favorite-pages/:page_id/",
            post(routes::page::create_favorite).delete(routes::page::destroy_favorite),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/archive/",
            post(routes::page::archive).delete(routes::page::unarchive),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/lock/",
            post(routes::page::lock).delete(routes::page::unlock),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/access/",
            post(routes::page::access),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/description/",
            get(routes::page::desc_get).patch(routes::page::desc_patch),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/versions/",
            get(routes::page::versions_list),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/versions/:pk/",
            get(routes::page::version_detail),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/duplicate/",
            post(routes::page::duplicate),
        )
        // Parity with the v2 asset surface (`app/views/asset/v2.py`,
        // `app/urls/asset.py:48-113`): presign POSTs (200
        // `{upload_data,asset_id,asset_url}`) for workspace / user / project /
        // issue scopes, 204 completes with entity FK binding, 204 deletes,
        // 302 presigned-GET singles/downloads, 200 `{exists}` check (+ legacy
        // file-assets GET-check/DELETE/restore with the 200-miss quirk).
        // Gate ADMIN/MEMBER/GUEST per row (WORKSPACE_LOGO presign needs
        // workspace ADMIN); static serves AllowAny. See `routes/asset.rs`.
        .route("/api/assets/v2/workspaces/:slug/", post(routes::asset::ws_presign))
        .route(
            "/api/assets/v2/workspaces/:slug/:asset_id/",
            get(routes::asset::ws_get)
                .patch(routes::asset::mark_uploaded)
                .delete(routes::asset::soft_delete),
        )
        .route("/api/assets/v2/user-assets/", post(routes::asset::user_presign))
        .route(
            "/api/assets/v2/user-assets/:asset_id/",
            patch(routes::asset::user_complete).delete(routes::asset::user_delete),
        )
        .route("/api/assets/v2/static/:asset_id/", get(routes::asset::static_get))
        .route(
            "/api/assets/v2/workspaces/:slug/projects/:project_id/",
            post(routes::asset::project_presign),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/",
            get(routes::asset::project_get)
                .patch(routes::asset::project_complete)
                .delete(routes::asset::project_delete),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/bulk/",
            post(routes::asset::bulk),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/duplicate-assets/:asset_id/",
            post(routes::asset::duplicate),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/download/:asset_id/",
            get(routes::asset::ws_download),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/projects/:project_id/download/:asset_id/",
            get(routes::asset::project_download),
        )
        // Parity with the issue-attachment V2 surface
        // (`app/views/issue/attachment.py` `IssueAttachmentV2Endpoint`,
        // `app/urls/issue.py:137-146`): byte-identical `assets/v2` paths —
        // POST presign + GET list on the collection, 302 GET / PATCH
        // complete / DELETE on the member.
        .route(
            "/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/",
            post(routes::asset::issue_presign).get(routes::asset::issue_list),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/:pk/",
            get(routes::asset::issue_get)
                .patch(routes::asset::issue_complete)
                .delete(routes::asset::issue_delete),
        )
        // Parity with the legacy file-assets (`app/views/asset/base.py`,
        // `app/urls/asset.py:27-47`): GET-check (200 + `{error,status:False}`
        // miss quirk), DELETE 204 soft, workspace restore 204. NO
        // POST-create (no FE caller) and NO `user-assets/server/*`.
        .route(
            "/api/workspaces/file-assets/:workspace_id/:key/",
            get(routes::asset::legacy_ws_get).delete(routes::asset::legacy_ws_delete),
        )
        .route(
            "/api/workspaces/file-assets/:workspace_id/:key/restore/",
            post(routes::asset::legacy_ws_restore),
        )
        .route(
            "/api/users/file-assets/:key/",
            get(routes::asset::legacy_user_get).delete(routes::asset::legacy_user_delete),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/check/:asset_id/",
            get(routes::asset::check),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/restore/:asset_id/",
            post(routes::asset::restore),
        )
        .route(
            "/api/workspaces/:slug/webhooks/",
            get(routes::webhook::list).post(routes::webhook::create),
        )
        .route(
            "/api/workspaces/:slug/webhooks/:pk/",
            get(routes::webhook::detail)
                .patch(routes::webhook::patch)
                .delete(routes::webhook::destroy),
        )
        .route(
            "/api/workspaces/:slug/webhooks/:pk/regenerate/",
            post(routes::webhook::regenerate),
        )
        .route("/api/workspaces/:slug/webhook-logs/:webhook_id/", get(routes::webhook::list_logs))
        .route("/api/workspaces/:slug/users/notifications/", get(routes::notification::list))
        .route("/api/workspaces/:slug/users/notifications/unread/", get(routes::notification::unread))
        .route(
            "/api/workspaces/:slug/users/notifications/mark-all-read/",
            post(routes::notification::mark_all_read),
        )
        .route(
            "/api/workspaces/:slug/users/notifications/:pk/read/",
            post(routes::notification::mark_read).delete(routes::notification::mark_unread),
        )
        .route(
            "/api/workspaces/:slug/users/notifications/:pk/archive/",
            post(routes::notification::archive).delete(routes::notification::unarchive),
        )
        .route(
            "/api/users/me/notification-preferences/",
            get(routes::notification::get_preferences).patch(routes::notification::patch_preferences),
        )
        .route("/api/workspaces/:slug/search/", get(routes::search::global_search))
        .route(
            "/api/workspaces/:slug/projects/:project_id/search-issues/",
            get(routes::search::issue_search),
        )
        .route("/api/workspaces/:slug/entity-search/", get(routes::search::entity_search))
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:pk/",
            get(routes::work_item::get_issue)
                .patch(routes::work_item::patch_issue)
                .delete(routes::work_item::delete_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/",
            get(routes::work_item::list_comments).post(routes::work_item::create_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/:pk/",
            get(routes::work_item::get_comment)
                .patch(routes::work_item::patch_comment)
                .delete(routes::work_item::delete_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/links/",
            get(routes::work_item::list_links).post(routes::work_item::create_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/links/:pk/",
            get(routes::work_item::get_link)
                .patch(routes::work_item::patch_link)
                .delete(routes::work_item::delete_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/relations/",
            get(routes::work_item::list_relations).post(routes::work_item::create_relations),
        )
        // Parity with `IssueRelationViewSet.remove_relation`
        // (`views/issue/relation.py:271-293`, `urls/issue.py:240-244`):
        // POST **204**; relation found via the bidirectional OR-filter
        // (`issue=X&related=Y OR swapped`, `relation.py:276-278`); miss —
        // or `related_issue` absent (Django has no such branch; it 500s on
        // `None.delete()`) — → 404 `missing()` (intentional deviation).
        // Gate `ProjectEntityPermission` (`relation.py:40`, POST =
        // non-safe → ADMIN/MEMBER + ws-admin fallback). Issues path ONLY;
        // `DELETE issue-relation/:relId/` is FE-dead (no Django route) and
        // stays OUT.
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/remove-relation/",
            post(routes::work_item::remove_relation),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/activities/",
            get(routes::work_item::list_activities),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/activities/:pk/",
            get(routes::work_item::get_activity),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/",
            get(routes::issue_query::list).post(routes::issue_write::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:pk/",
            get(routes::work_item::get_issue)
                .patch(routes::work_item::patch_issue)
                .delete(routes::work_item::delete_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/",
            get(routes::work_item::list_comments).post(routes::work_item::create_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/:pk/",
            get(routes::work_item::get_comment)
                .patch(routes::work_item::patch_comment)
                .delete(routes::work_item::delete_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/",
            get(routes::work_item::list_links).post(routes::work_item::create_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/:pk/",
            get(routes::work_item::get_link)
                .patch(routes::work_item::patch_link)
                .delete(routes::work_item::delete_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/relations/",
            get(routes::work_item::list_relations).post(routes::work_item::create_relations),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/",
            get(routes::work_item::list_activities),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/:pk/",
            get(routes::work_item::get_activity),
        )
        .route("/api/workspaces/:slug/work-items/search/", get(routes::work_item::workspace_issue_search))
        .route("/api/workspaces/:slug/work-items/:ident/", get(routes::work_item::get_by_identifier))
        .route("/api/timezones/", get(routes::misc::timezones))
        .route(
            "/api/instances/",
            get(routes::instance::get).patch(routes::instance_admin::instance_patch),
        )
        // E1 — instance-admin god-mode (JWT), paritas Django
        // (`license/api/views/admin.py|instance.py|configuration.py|workspace.py`).
        // JSON auth (sign-in/sign-up 200 + cookies, sign-out 200 + clears —
        // NO 302); gate `InstanceAdmin(role>=15)` with the DRF
        // `{"detail": ...}` deny. OUT (no Django route wired):
        // `admins/session/`, `sign-up-screen-visited/`, `changelog/`.
        // NO GET on `admins/:pk/` — the explicit GET below answers the
        // 404 fallback body (Axum alone would 405 the DELETE-only route).
        .route(
            "/api/instances/admins/sign-in/",
            post(routes::instance_admin::sign_in),
        )
        .route(
            "/api/instances/admins/sign-up/",
            post(routes::instance_admin::sign_up),
        )
        .route(
            "/api/instances/admins/sign-out/",
            post(routes::instance_admin::sign_out),
        )
        .route(
            "/api/instances/admins/",
            get(routes::instance_admin::admins_list).post(routes::instance_admin::admins_create),
        )
        .route(
            "/api/instances/admins/:pk/",
            delete(routes::instance_admin::admins_delete)
                .get(routes::instance_admin::admin_pk_get_404),
        )
        .route(
            "/api/instances/admins/me/",
            get(routes::instance_admin::admins_me),
        )
        .route(
            "/api/instances/configurations/",
            get(routes::instance_admin::configs_list).patch(routes::instance_admin::configs_patch),
        )
        .route(
            "/api/instances/configurations/disable-email-feature/",
            delete(routes::instance_admin::disable_email_with_body),
        )
        .route(
            "/api/instances/email-credentials-check/",
            post(routes::instance_admin::email_check),
        )
        .route(
            "/api/instances/workspaces/",
            get(routes::instance_admin::workspaces_list)
                .post(routes::instance_admin::workspaces_create),
        )
        .route(
            "/api/instances/workspace-slug-check/",
            get(routes::instance_admin::slug_check),
        )
        .route("/api/auth/refresh/", post(routes::auth::refresh))
        .route("/api/auth/logout/", post(routes::auth::logout))
        // Tanpa throttle selaras Django (`ChangePasswordEndpoint` tanpa throttle_classes).
        .route("/auth/change-password/", post(routes::auth_compat::change_password))
        // Parity `SignOutAuthEndpoint.post` (`authentication/views/app/signout.py:16-28`,
        // mounted `auth/` tanpa `/api` per `plane/urls.py:23`): POST-only → 405
        // `Allow: POST` untuk metode lain (default Axum). Selalu 302, tanpa body.
        .route("/auth/sign-out/", post(routes::auth::sign_out))
        .route("/auth/set-password/", post(routes::auth_compat::set_password))
        .route("/auth/get-csrf-token/", get(routes::auth_compat::csrf_token))
        .route("/api/auth/oauth/:provider/start/", get(routes::auth::oauth_start))
        .route(
            "/api/workspaces/:slug/export-issues/",
            post(routes::misc::create_export).get(routes::misc::export_history),
        )
        .route(
            "/api/users/me/",
            get(routes::user::me)
                .patch(routes::user::patch_me)
                .delete(routes::user::deactivate),
        )
        // Parity `UserSessionEndpoint` (`user/base.py:351-362`): AllowAny,
        // never 401 (tanpa kredensial → 200 `{"is_authenticated": false}`).
        .route("/api/users/session/", get(routes::user::session_allow_any))
        .route("/api/users/me/settings/", get(routes::user::settings))
        .route("/api/users/me/instance-admin/", get(routes::user::instance_admin))
        .route("/api/users/me/onboard/", patch(routes::user::onboard))
        .route("/api/users/me/tour-completed/", patch(routes::user::tour_completed))
        .route("/api/users/me/activities/", get(routes::user::activities))
        .route(
            "/api/users/me/email/generate-code/",
            post(routes::users_me::generate_email_code),
        )
        .route(
            "/api/users/me/email/",
            patch(routes::users_me::update_email),
        )
        .route(
            "/api/users/me/workspaces/",
            get(routes::users_me::my_workspaces),
        )
        .route(
            "/api/users/me/workspaces/invitations/",
            get(routes::users_me::my_workspace_invitations).post(routes::users_me::join_workspaces),
        )
        .route(
            "/api/users/me/workspaces/:slug/projects/invitations/",
            get(routes::users_me::my_project_invitations).post(routes::users_me::join_projects),
        )
        .route(
            "/api/users/me/workspaces/:slug/project-roles/",
            get(routes::users_me::my_project_roles),
        )
        .route(
            "/api/users/me/profile/",
            get(routes::user::profile).patch(routes::user::patch_profile),
        )
        .route(
            "/api/users/me/accounts/",
            get(routes::user::list_accounts),
        )
        .route(
            "/api/users/me/accounts/:pk/",
            get(routes::user::get_account).delete(routes::user::delete_account),
        )
        // Parity E8d workspace-scoped user routes (`workspace/user.py:98-546`,
        // `workspace/base.py:175-391`): stats (OPEN gate, unknown slug 200
        // kosong), profile (404 non-member), activity GET (403 DRF detail),
        // export POST (text/csv), graphs + dashboard.
        .route(
            "/api/workspaces/:slug/user-stats/:user_id/",
            get(routes::user::user_stats),
        )
        .route(
            "/api/workspaces/:slug/user-profile/:user_id/",
            get(routes::user::user_profile),
        )
        .route(
            "/api/workspaces/:slug/user-activity/:user_id/",
            get(routes::user::user_activity),
        )
        .route(
            "/api/workspaces/:slug/user-activity/:user_id/export/",
            post(routes::user::export_activity),
        )
        .route(
            "/api/users/me/workspaces/:slug/activity-graph/",
            get(routes::user::activity_graph),
        )
        .route(
            "/api/users/me/workspaces/:slug/issues-completed-graph/",
            get(routes::user::issues_completed_graph),
        )
        .route(
            "/api/users/me/workspaces/:slug/dashboard/",
            get(routes::user::dashboard),
        )
        .route(
            "/api/users/api-tokens/",
            get(routes::misc::list_tokens).post(routes::misc::create_token),
        )
        .route(
            "/api/users/api-tokens/:pk/",
            get(routes::misc::get_token).delete(routes::misc::delete_token),
        )
        .route(
            "/api/workspaces/:slug/stickies/",
            get(routes::misc::list_stickies).post(routes::misc::create_sticky),
        )
        .route(
            "/api/workspaces/:slug/stickies/:pk/",
            get(routes::misc::get_sticky)
                .patch(routes::misc::patch_sticky)
                .delete(routes::misc::delete_sticky),
        )
        .route("/api/workspaces/:slug/default-analytics/", get(routes::analytic::default_analytics))
        .route("/api/workspaces/:slug/project-stats/", get(routes::analytic::project_stats))
        .route(
            "/api/workspaces/:slug/analytic-view/",
            get(routes::analytic::list_views).post(routes::analytic::create_view),
        )
        // Parity with `AdvanceAnalyticsEndpoint.get`
        // (`views/analytic/advance.py:104-119`, `urls/analytic.py:61-63`):
        // GET 200 `?tab=overview|work-items` (default overview; invalid →
        // 400 `{"message": "Invalid tab"}`). Gate WORKSPACE ADMIN/MEMBER
        // (`deny()` 403).
        .route("/api/workspaces/:slug/advance-analytics/", get(routes::analytic::advance_overview))
        // Parity with `AdvanceAnalyticsStatsEndpoint.get`
        // (`advance.py:158-169`, `urls/analytic.py:66-68`): GET 200
        // per-project state-group counts (only `?type=work-items`, else 400
        // `{"message": "Invalid type"}`). Gate WORKSPACE ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/advance-analytics-stats/",
            get(routes::analytic::advance_stats),
        )
        // Parity with `AdvanceAnalyticsChartEndpoint.get`
        // (`advance.py:285-318`, `urls/analytic.py:71-73`): GET 200
        // `?type=projects|custom-work-items|work-items` (default projects;
        // invalid → 400 Invalid type). Gate WORKSPACE ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/advance-analytics-charts/",
            get(routes::analytic::advance_charts),
        )
        // Parity with `ProjectAdvanceAnalyticsEndpoint.get`
        // (`views/analytic/project_analytics.py:84-94`,
        // `urls/analytic.py:76-78`): GET 200 work-item stats scoped to the
        // project (+ optional `?cycle_id|module_id` id__in scoping; unknown
        // → zero counts, no 404). Gate project ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/advance-analytics/",
            get(routes::analytic::project_advance),
        )
        // Parity with `ProjectAdvanceAnalyticsStatsEndpoint.get`
        // (`project_analytics.py:165-179`, `urls/analytic.py:81-83`): GET 200
        // per-assignee counts (only `?type=work-items`). Gate project
        // ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/projects/:project_id/advance-analytics-stats/",
            get(routes::analytic::project_advance_stats),
        )
        // Parity with `ProjectAdvanceAnalyticsChartEndpoint.get`
        // (`project_analytics.py:317-367`, `urls/analytic.py:86-88`): GET 200
        // `?type=custom-work-items|work-items` (default `projects` → 400
        // Invalid type). Gate project ADMIN/MEMBER/GUEST.
        .route(
            "/api/workspaces/:slug/projects/:project_id/advance-analytics-charts/",
            get(routes::analytic::project_advance_charts),
        )
        // Parity with `DeployBoardViewSet` (`views/project/base.py:535-576`,
        // `urls/project.py:113-118`): list GET 200 (no board → 200 null) +
        // create POST upsert **200**; retrieve GET 200 + PATCH 200 + DELETE
        // **204** soft. Gates: SAFE reads = any active member, POST =
        // workspace ADMIN/MEMBER, PATCH/DELETE = project ADMIN/MEMBER
        // (DRF 403 `{"detail": ...}` denies).
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-deploy-boards/",
            get(routes::analytic::deploy_list).post(routes::analytic::deploy_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-deploy-boards/:pk/",
            get(routes::analytic::deploy_retrieve)
                .patch(routes::analytic::deploy_patch)
                .delete(routes::analytic::deploy_destroy),
        )
        // Parity with `WorkspaceUserPropertiesEndpoint`
        // (`views/workspace/user.py:252-269`,
        // `serializers/workspace.py:174-178`): GET 200 + PATCH 200 full
        // `__all__` object, `get_or_create` (never 404 on the row).
        // Gate = any ACTIVE ws member (`WorkspaceViewerPermission`,
        // DRF 403 `{"detail": ...}` deny).
        .route(
            "/api/workspaces/:slug/user-properties/",
            get(routes::prefs::user_props_get).patch(routes::prefs::user_props_patch),
        )
        // Parity with `WorkspaceUserPreferenceViewSet`
        // (`views/workspace/user_preference.py:18-103`): GET 200 dict
        // (auto-creates missing keys) + PATCH 200 `{"message": ...}`
        // (skips unknown keys). Gate AMG (`deny()` 403). NO `:key/`
        // route — Django defines none.
        .route(
            "/api/workspaces/:slug/sidebar-preferences/",
            get(routes::prefs::sidebar_get).patch(routes::prefs::sidebar_patch),
        )
        // Parity with `WorkspaceHomePreferenceViewSet`
        // (`views/workspace/home.py:24-79`): GET 200 list (auto-creates
        // the 3 keys) + PATCH 200 per key (partial, `config` read-only;
        // miss → 400 `{"detail": "Preference not found"}`). Gate AMG.
        .route("/api/workspaces/:slug/home-preferences/", get(routes::prefs::home_list))
        .route("/api/workspaces/:slug/home-preferences/:key/", patch(routes::prefs::home_patch))
        // Parity with `QuickLinkViewSet`
        // (`views/workspace/quick_link.py:24-61`): POST **201** + LIST/GET/
        // PATCH 200 + DELETE **204**; PATCH miss → 404 `detail` twin,
        // GET miss → 404 `error` twin; owner-scoped. Gate AMG.
        .route(
            "/api/workspaces/:slug/quick-links/",
            get(routes::prefs::quick_list).post(routes::prefs::quick_create),
        )
        .route(
            "/api/workspaces/:slug/quick-links/:pk/",
            get(routes::prefs::quick_detail)
                .patch(routes::prefs::quick_patch)
                .delete(routes::prefs::quick_destroy),
        )
        // Parity with `UserRecentVisitViewSet.list`
        // (`views/workspace/recent_visit.py:25-33`): GET 200 (forced
        // allowlist, cap 20, no pagination; reads only — NO POST/DELETE).
        // Gate AMG.
        .route("/api/workspaces/:slug/recent-visits/", get(routes::prefs::recent_list))
        // Parity with `WorkspaceMemberUserViewsEndpoint.post`
        // (`views/workspace/member.py:208-212`): POST **204** overwriting
        // the member's `view_props`; non-member → 404. POST-only.
        .route("/api/workspaces/:slug/workspace-views/", post(routes::prefs::views_post))
        // Parity with `WorkspaceEstimatesEndpoint.get`
        // (`views/workspace/estimate.py:22`): GET 200
        // `WorkspaceEstimateSerializer[]` (project-estimate ids for the
        // slug + points). Gate safe-branch = any ACTIVE ws member (DRF
        // 403 `detail` deny). NO 2h cache (documented in `prefs.rs`).
        .route("/api/workspaces/:slug/estimates/", get(routes::prefs::ws_estimates))
        // Parity with `WorkSpaceAvailabilityCheckEndpoint`
        // (`views/workspace/base.py:215-224`): GET 200 `{"status"}`;
        // missing slug → 400. IsAuthenticated only, no ws gate.
        .route("/api/workspace-slug-check/", get(routes::prefs::slug_check))
        // Parity with `UnsplashEndpoint`
        // (`views/external/base.py:215-243`): GET 200 `[]` without a key,
        // else upstream passthrough. IsAuthenticated only.
        .route("/api/unsplash/", get(routes::prefs::unsplash))
        // Parity with `UserLastProjectWithWorkspaceEndpoint`
        // (`views/workspace/user.py:68-95`): GET 200 (null shape when no
        // workspace). GET-only. IsAuthenticated only.
        .route("/api/users/last-visited-workspace/", get(routes::prefs::last_visited));

    // Login + OAuth callback + email-check di-limit per-IP (5/mnt); refresh/logout/start bebas.
    let auth_router = Router::new()
        .route("/api/auth/login/", post(routes::auth::login))
        .route(
            "/api/auth/oauth/:provider/callback/",
            get(routes::auth::oauth_callback),
        )
        .route("/auth/email-check/", post(routes::auth::email_check))
        .route(
            "/auth/forgot-password/",
            post(routes::auth_compat::forgot_password),
        )
        .route(
            "/auth/magic-generate/",
            post(routes::auth_compat::magic_generate),
        )
        .route_layer(axum_middleware::from_fn_with_state(
            IpRateLimiter::new(5, std::time::Duration::from_secs(60)),
            ip_rate_limit_middleware,
        ));

    let app = Router::new()
        .merge(auth_router)
        .merge(app)
        .fallback(fallback_404)
        .with_state(state::AppState {
            pool,
            redis,
            config: cfg.clone(),
        })
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            5 * 1024 * 1024,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // Process-level burst backstop. Mirrors DRF throttle intent
    // (`plane/settings/common.py`: anon 30/min, API-key 60/min) with
    // immediate 429 (unlike tower's delay-based limiter) — true per-key
    // accounting stays a follow-up (Redis-backed); this bucket is shared
    // process-wide.
    let app = app.route_layer(axum_middleware::from_fn_with_state(
        RateLimiter::new(600, std::time::Duration::from_secs(60)),
        rate_limit_middleware,
    ));

    let app = app.route_layer(axum_middleware::from_fn_with_state(
        crate::middleware::origin::allowed_origins_from_env(&cfg.frontend_url),
        crate::middleware::origin::origin_middleware,
    ));

    // CORS paling luar: preflight dijawab sebelum origin/rate logic.
    // allow-credentials + origin eksplisit agar cookie auth lintas-port menempel.
    let cors = crate::middleware::cors::cors_layer_from_env(&cfg.frontend_url);
    let app = app.layer(cors);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", cfg.port))
        .await
        .unwrap();
    tracing::info!("rust-api listening on {}", cfg.port);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}
