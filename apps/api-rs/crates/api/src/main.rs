#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod middleware;
mod routes;
mod state;

use axum::{
    http::StatusCode,
    middleware as axum_middleware,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::middleware::rate_limit::{ip_rate_limit_middleware, rate_limit_middleware, IpRateLimiter, RateLimiter};
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
    (StatusCode::NOT_FOUND, Json(json!({"error": "Page not found."})))
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
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/",
            get(routes::module::list).post(routes::module::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:pk/",
            get(routes::module::detail)
                .patch(routes::module::patch)
                .delete(routes::module::destroy),
        )
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
            get(routes::intake::detail_issue).delete(routes::intake::destroy_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/",
            get(routes::intake::detail_issue).delete(routes::intake::destroy_issue),
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
        .route(
            "/api/workspaces/:slug/invitations/",
            get(routes::member::list_invites).post(routes::member::create_invite),
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
            post(routes::page::create_favorite),
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
            "/api/assets/v2/workspaces/:slug/:asset_id/",
            axum::routing::patch(routes::asset::mark_uploaded).delete(routes::asset::soft_delete),
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
        .route("/api/instances/", get(routes::instance::get))
        .route("/api/auth/refresh/", post(routes::auth::refresh))
        .route("/api/auth/logout/", post(routes::auth::logout))
        // Tanpa throttle selaras Django (`ChangePasswordEndpoint` tanpa throttle_classes).
        .route("/auth/change-password/", post(routes::auth_compat::change_password))
        .route("/auth/set-password/", post(routes::auth_compat::set_password))
        .route("/auth/get-csrf-token/", get(routes::auth_compat::csrf_token))
        .route("/api/auth/oauth/:provider/start/", get(routes::auth::oauth_start))
        .route(
            "/api/workspaces/:slug/export-issues/",
            post(routes::misc::create_export).get(routes::misc::export_history),
        )
        .route(
            "/api/users/me/",
            get(routes::user::me).patch(routes::user::patch_me),
        )
        .route("/api/users/session/", get(routes::user::session))
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
        );

    // Login + OAuth callback + email-check di-limit per-IP (5/mnt); refresh/logout/start bebas.
    let auth_router = Router::new()
        .route("/api/auth/login/", post(routes::auth::login))
        .route("/api/auth/oauth/:provider/callback/", get(routes::auth::oauth_callback))
        .route("/auth/email-check/", post(routes::auth::email_check))
        .route("/auth/forgot-password/", post(routes::auth_compat::forgot_password))
        .route("/auth/magic-generate/", post(routes::auth_compat::magic_generate))
        .route_layer(axum_middleware::from_fn_with_state(
            IpRateLimiter::new(5, std::time::Duration::from_secs(60)),
            ip_rate_limit_middleware,
        ));

    let app = Router::new()
        .merge(auth_router)
        .merge(app)
        .fallback(fallback_404)
        .with_state(state::AppState { pool, redis, config: cfg.clone() })
        .layer(tower_http::limit::RequestBodyLimitLayer::new(5 * 1024 * 1024))
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
        cfg.frontend_url.clone(),
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
    ).await.unwrap();
}
