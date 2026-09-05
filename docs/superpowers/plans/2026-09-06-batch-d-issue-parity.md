# Batch D Issue-Domain Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 13 issue-domain endpoint groups (all Django-backed, currently 404 in Rust) with byte-exact Django parity, plus a tech task (D0) that splits `issue.rs` (4987 lines) and adds Django's 404-fallback body.

**Architecture:** New-file-per-task (each task owns a disjoint file; no two tasks touch the same file): `routes/subscribe.rs` (D1), `routes/history.rs` (D2), append-only `state.rs`/`workspace.rs` (D3), `routes/userprops.rs` (D4), `routes/issue_dates.rs` (D5), `routes/versions.rs` (D6), `routes/issue_archive_one.rs` (D7), `routes/reactions.rs` (D8), append-only `work_item.rs` (D9), `routes/draft.rs` (D10), append-only `label.rs` (D11), `routes/issue_lists.rs` (D12), append-only `intake.rs` (D13); wire-up in `main.rs`; smoke in `scripts/smoke.sh`. D0 merges first (everything depends on the split); D1–D13 may then run in lanes (disjoint files) but `main.rs` edits apply serially.

**Tech Stack:** Rust, Axum, SQLx (Postgres), no new deps. Celery/activity side-effects SKIPPED everywhere (Batch C precedent). Reuse Batch C helpers: `project.rs::ws_role/project_role/deny()/missing()`, `FORBIDDEN_MSG`/`NOT_FOUND_MSG`, role ints 20/15/5, I2 paginator helpers (post-split location in `issue_common.rs` — see D0).

---

## Locked contracts (from Django source, verified 2026-09-06)

Byte-exact strings: `FORBIDDEN_MSG` / `NOT_FOUND_MSG` as Batch C. Plus Django's catch-all 404: `{"error": "Page not found."}` (`plane/urls.py:16` → `views/error_404.py:9-10`) — used when a URL doesn't resolve at all (incl. non-UUID path segments, since routes use `<uuid:...>` converters). Django NEVER emits 422 (grep 0 hits); in-view `ValidationError` → 400 `{"error":"Please provide valid detail"}` (`views/base.py:70-109,167-204`).

| # | Method+Path | Django source | Success | Errors |
|---|---|---|---|---|
| D0 | tech: split `issue.rs` + fallback 404 | — | `cargo test` green, zero behavior change except fallback body | — |
| D1a | `GET+POST+DELETE .../issues/:iid/subscribe/` (issues ONLY) | `urls/issue.py:185-189`, `views/issue/subscriber.py:16-104` | GET 200 `{"subscribed":bool}`; POST **201** `IssueSubscriberSerializer` (`serializers/issue.py:974-978`, `__all__`); DELETE **204** | POST dup → 400 `{"message":"User already subscribed to the issue."}` (`subscriber.py:76-79`); gate `ProjectLitePermission` = any ACTIVE project member incl. GUEST (`permissions/project.py:136-146`, no role filter) |
| D1b | `GET .../issue-subscribers/` + `DELETE .../issue-subscribers/:sid/` | `urls/issue.py:174-184`, `subscriber.py:52-67` | GET 200 **`ProjectMemberLite` list** (`serializers/project.py:237-244`: `member` (UserLite), `id`, `is_subscribed` — compute per-member `EXISTS(subscriber)`; Django passes no annotation so mirror the fields, note assumption in commit); DELETE **204** | DELETE miss → 404 via `.get()` (`subscriber.py:60-65`) |
| D2a | `GET .../issues/:iid/history/` | `urls/issue.py:149-153`, `views/issue/activity.py:24-86` | 200 merged list; `?activity_type=issue-property` → `IssueActivitySerializer[]` (model `__all__` + `actor_detail,issue_detail,project_detail,workspace_detail,source_data`, `serializers/issue.py:333-351`); `?activity_type=issue-comment` → `IssueCommentSerializer[]` (diff field list against the `work_item.rs` comment struct first: identical → reuse struct; delta → new struct mirroring Django, note delta in commit); `?created_at__gt` filter; ordering ASCENDING (`order_by("created_at")` both queries, merged `sorted()` ASC — mirror!); activities exclude `field IN (comment,vote,reaction,draft)`; only `activity_type=issue-property` prefetches `source_data` (skip prefetch otherwise) | ANY other `activity_type` value (incl. junk/missing) → merged default (`activity.py:81-86`); gate `ProjectEntityPermission` + ADMIN/MEMBER/GUEST |
| D2b | `GET .../issues/:iid/meta/` | `views/issue/base.py:1186-1198` | 200 `{"sequence_id","project_identifier"}` | miss → 404 `missing()`; gate ADMIN/MEMBER/GUEST |
| D3a | `GET .../projects/:pid/intake-state/` | `urls/state.py:22-26`, `views/state/base.py:136-...` | 200 `StateSerializer` (`serializers/state.py:12`: `id,project_id,workspace_id,name,color,group,default,description,sequence,order`) | miss → 404 `{"error":"Triage state not found"}` (verbatim!); gate ADMIN/MEMBER/GUEST |
| D3b | `GET /api/workspaces/:slug/states/` | `urls/workspace.py:167-171`, `views/workspace/state.py:17-...` | 200 `StateSerializer[]` | gate `WorkspaceEntityPermission`: GET (safe method) = any ACTIVE ws member incl. GUEST (`permissions/workspace.py:74-90`) |
| D4 | `GET+PATCH .../projects/:pid/user-properties/` + `cycles/:cid/user-properties/` + `modules/:mid/user-properties/` | `views/issue/base.py:743-770`, `views/cycle/base.py:625-655`, `views/module/base.py:825-855` | project: GET 200 / PATCH 200, row auto-created if missing (`get_or_create`, `base.py:747-755,768` — never 404); cycle: GET 200 / PATCH **201** (`cycle/base.py:644`), missing row → `.get()` 404 `missing()`; module: GET 200 / PATCH **201** (`module/base.py:844`), missing row → 404 `missing()`; cycle/module PATCH merges only 4 keys (`filters,rich_filters,display_filters,display_properties`, absent key keeps old value) | gates all ADMIN/MEMBER/GUEST; NO POST anywhere (FE's `issue-display-properties/` POST is FE-dead — do NOT implement that path) |
| D5 | `POST .../issue-dates/` `{updates:[{id,start_date,target_date}]}` | `urls/issue.py:251-255`, `views/issue/base.py:1106-1183` | 200 `{"message":"Issues updated successfully"}`; validation merges new-over-current (`new_start or current_start`, `base.py:1107-1124`, date strings `%Y-%m-%d`); unknown issue ids SKIPPED silently (`continue`, `base.py:1142-1143`); empty `updates` → 200 (loop no-op, `bulk_update([])`); effectively ATOMIC without explicit tx (single `bulk_update` at very end; 400 returns before it — `base.py:1181`) | 400 `{"message":"Start date cannot exceed target date"}`; missing `id` key → KeyError → 400 via base handler; gate ADMIN/MEMBER |
| D6a | `GET .../issues/:iid/versions[/:pk]/` | `urls/issue.py:256-265`, `views/issue/version.py:27-74` | 200 cursor-paginated 10-key list (`id,workspace,project,issue,last_saved_at,owned_by,created_at,updated_at,created_by,updated_by`, `version.py:48-59`; NO explicit `order_by` — check `IssueVersion` Meta ordering and mirror) / single 200 full snapshot (`serializers/issue.py:984-1016`: id,workspace,project,issue,parent,state,estimate_point,name,priority,start_date,target_date,assignees,sequence_id,labels,sort_order,completed_at,archived_at,is_draft,external_source,external_id,type,cycle,modules,meta,last_saved_at,owned_by,created_at,updated_at,created_by,updated_by — note `name` listed twice in Django, emit once) | gate ADMIN/MEMBER/GUEST; miss → 404 via `.get()` |
| D6b | `GET .../work-items/:wid/description-versions[/:pk]/` (work-items path ONLY) | `urls/issue.py:266-275`, `views/issue/version.py:77-144` | list cursor-`paginate()` same 10 keys; single 200 (`serializers/issue.py:1023-1038`: id,workspace,project,issue,description_binary,description_html,description_stripped,description_json,last_saved_at,owned_by,created_at,updated_at,created_by,updated_by) | guest blocked → 403 `{"error":"You are not allowed to view this issue"}` when (role==GUEST AND NOT `project.guest_view_all_features` AND NOT own issue) (`version.py:91-105`); NO `issues/:id/description-versions` route — do NOT add it |
| D6c | `GET .../intake-work-items/:id/description-versions[/:vid]/` | `urls/intake.py:56-65`, `views/intake/base.py:572-...` | 200 paginated `{id,workspace,project,issue,last_saved_at,owned_by,created_at,updated_at,created_by,updated_by}` | 403 guest gate (mirror) |
| D7 | `GET+POST+DELETE .../issues/:pk/archive/` (single issue) | `urls/issue.py:228-232`, `views/issue/archive.py:53,221-302` | GET 200 `IssueDetailSerializer` (scope = ARCHIVED ONLY: `get_queryset` filters `archived_at NOT NULL` + non-epic, `archive.py:97-102` — GET non-archived → 404); POST 200 `{"archived_at":…}`; DELETE **204** | POST non-completed/cancelled state → 400 `{"error":"Can only archive completed or cancelled state group issue"}` (`archive.py:259-263`); GET miss → 404 `{"error":"The required object does not exist."}`; gate ADMIN/MEMBER |
| D8a | `GET+POST .../issues/:iid/reactions/` + `DELETE .../reactions/:code/` | `urls/issue.py:191-201`, `views/issue/reaction.py:25-85` | POST **201** `IssueReactionSerializer` (`serializers/issue.py:649-655`, `__all__`+`actor_detail`); DELETE **204**; GET 200 = serializer list via `get_queryset` (no custom `list` — mirror queryset `reaction.py:29-43`) | POST dup → `IntegrityError` → 400 `{"error":"The payload is not valid"}` via base handler (NO explicit catch in `reaction.py` — differs from comments!); DELETE scoped `(slug,project,issue,reaction,actor=user)` miss → 404; gate ADMIN/MEMBER/GUEST |
| D8b | `GET+POST .../comments/:cid/reactions/` + `DELETE .../:code/` | `urls/issue.py:202-213`, `views/issue/comment.py:163-239` | POST **201** `CommentReactionSerializer` (`serializers/issue.py:666-685`: `id,actor,comment,reaction,display_name,…`) | POST dup → `IntegrityError` → 400 `{"error":"Reaction already exists for the user"}` (`comment.py:206-210`); DELETE scoped `(slug,project,comment,reaction,actor=user)` miss → 404; gate ADMIN/MEMBER/GUEST |
| D9 | `POST .../issues/:iid/remove-relation/` `{related_issue}` | `urls/issue.py:234-244`, `views/issue/relation.py:271-293` | **204** | gate `ProjectEntityPermission` (`relation.py:40`); MISS → Django does `.first()` → `None.delete()` → AttributeError (500) — Rust returns 404 `missing()` instead (documented intentional deviation, sane); same 404 when `related_issue` absent; (`DELETE issue-relation/:relId/` is FE-dead — do NOT implement) |
| D10 | `GET+POST /api/workspaces/:slug/draft-issues/` + `GET+PATCH+DELETE .../:did/` + `POST .../draft-to-issue/:did/` | `urls/workspace.py:202-216`, `views/workspace/draft.py:46-311` | list GET 200 paginated `DraftIssueSerializer` (own drafts only: `created_by=user`, `draft.py:101`); POST **201**/400; retrieve 200 `DraftIssueDetailSerializer`, miss → 404 standard msg; PATCH **204**/400, miss → 404 `{"error":"Issue not found"}` (verbatim, NON-standard — `draft.py:166`); DELETE **204**; draft-to-issue POST **201**, no-project → 400 `{"error":"Project is required to create an issue."}` (`draft.py:210-212`) | gates (WORKSPACE level): list/create AMG; PATCH ADMIN+MEMBER+creator; retrieve ADMIN+creator; destroy ADMIN+creator; draft-to-issue ADMIN+MEMBER (`draft.py:98,111,156,186,199,205`); read `DraftIssue` model columns + both serializers first, verify live with `\d draft_issues` |
| D11a | `GET /api/workspaces/:slug/labels/` | `urls/workspace.py:157`, `views/workspace/label.py:17-...` | 200 `[Label...]` | gate `WorkspaceViewerPermission` = any ACTIVE ws member (`permissions/workspace.py:93-100`) |
| D11b | `GET+POST .../projects/:pid/issue-labels/` (collection) | `urls/issue.py:71`, `views/issue/label.py:23-55` | GET list via `ModelViewSet` defaults (`LabelSerializer`, queryset scoped ws+project+member, `order_by sort_order`); POST **201** `Label` | POST dup → `IntegrityError` → 400 `{"error":"Label with the same name already exists in the project"}` (`label.py:51-55`); create gate ADMIN project-level (`@allow_permission([ROLE.ADMIN])`, `label.py:43`); read gate `ProjectBasePermission` (`permissions/project.py:13-53`: safe methods = any active ws member) |
| D12a | `GET /api/workspaces/:slug/issues/` | `urls/views.py:51-55`, `views/view/base.py:144-259` | 200 offset-paginated 26-key rows (`serializers/view.py:24-52`: id,name,state_id,sort_order,completed_at,estimate_point,priority,start_date,target_date,sequence_id,project_id,parent_id,cycle_id,sub_issues_count,created_at,updated_at,created_by,updated_by,attachment_count,link_count,is_draft,archived_at,state__group,assignee_ids,label_ids,module_ids) | gate WORKSPACE ADMIN/MEMBER/GUEST (`view/base.py:222`) |
| D12b | `GET .../projects/:pid/v2/issues/` | `urls/issue.py:54-58`, `views/issue/base.py:816-972` | 200 cursor-`paginate()` 27-key rows (`base.py:871-898`: id,name,state_id,state__group,sort_order,completed_at,estimate_point,priority,start_date,target_date,sequence_id,project_id,parent_id,cycle_id,created_at,updated_at,created_by,updated_by,is_draft,archived_at,module_ids,label_ids,assignee_ids,link_count,attachment_count,sub_issues_count — +`description_html` iff `?description=true`); `ORDER BY updated_at ASC` (not created_at! `base.py:906-907`); `?updated_at__gt` filter; guest (role==5 AND NOT `project.guest_view_all_features`) scoped to `created_by=user` (`base.py:910-920`) | gate ADMIN/MEMBER/GUEST; project miss → 404; NO `v2/work-items/` — do NOT add it |
| D12c | `GET /api/workspaces/:slug/user-issues/:uid/` | `urls/workspace.py:152-156`, `views/workspace/user.py:98-203` | 200 `issue_on_results` (scope: assignee∨creator∨subscriber `:uid`, requester must be ACTIVE project member, `user.py:139-147`; annotations: cycle_id subquery, link/attachment/sub_issues counts, `user.py:104-133`) | gate `WorkspaceViewerPermission`; `group_by==sub_group_by` → 400 `{"error":"Group by and sub group by cannot have same parameters"}` (`user.py:176-181`) |
| D13 | `PATCH .../inbox-issues/:pk/` (GET+DELETE already in Rust) | `urls/intake.py:51-55`, `views/intake/base.py:95,334-...` | 200 `IntakeIssueDetailSerializer` (compare existing Rust GET/DELETE handlers for shape reuse) | decorator ADMIN+creator(Issue); no membership AND no ws-admin → 403 `{"error":"Only admin or creator can update the intake work items"}` (`base.py:361-365`); guest-role member AND not creator AND not ws-admin → 400 `{"error":"You cannot edit intake issues"}` (`base.py:368-374`) |

Explicitly OUT (do NOT implement; FE-dead = 404/500 against Django prod too — document caller file in commit if nearby):
- `bulk-subscribe-issues/`, `bulk-operation-issues/` (grep 0 in Django; FE `issue.service.ts:340,417`)
- `my-issues/` (grep 0; FE `user.service.ts:46` — FE must migrate to `user-issues/:uid/`, D12c)
- `views/:viewId/issues/` (no route; FE `view.service.ts:59` — FE must migrate to workspace `issues/`, D12a)
- `issue-display-properties/` literal path + POST (Django path is `user-properties/` GET+PATCH, D4; FE `issue.service.ts:205,217`)
- `DELETE issue-relation/:relId/` (no route; FE `issue.service.ts:196` — FE must use `remove-relation/`, D9)
- `GET user-favorite-projects/` (wired but 500s in Django — no serializer; FE `project.service.ts:164`)
- `v2/work-items/`, workspace-level `issues-detail/`, `work-items/:id/subscribe/`, `epics/*` (no Django backend)
- `issues/:id/description-versions/` (only `work-items/` path exists, D6b)
- Batch E preview (separate plan): deploy-boards, pages ops (description/duplicate/archive/access/lock), `user-favorites/` infra + cycle/module/view favorites, cycles full (analytics/progress/cycle-issues/transfer/date-check/archived/archive), modules full (module-issues/links/archived/archive), workspace members/invites/leave, prefs (user-properties/sidebar/home/quick-links/recent-visits/workspace-views), ws estimates, slug-check/unsplash/last-visited, user-stats/profile/activity/export, assets full + attachments, ai-assistant, advance-analytics ×6, sign-out decision.
- Batch F tech-debt (deferred): grouped-pagination (verify FE caller first — `issues-detail` `group_by` client logic at `issue.service.ts:47`), `fields`/`expand` branches on `list/`, SMTP 501s (`auth_compat.rs:193,223`), Axum `Path<Uuid>` rejection mapping (verify actual behavior; D0 fallback covers unmatched routes only).

---

### Task D0: split issue.rs + 404 fallback (MERGES FIRST)

**Why:** `issue.rs` is 4987 lines (quality reviewers flagged twice); D adds ~1500 more. Pure-move split with test guard; plus Django's catch-all 404 body.

**Files:** `routes/issue.rs` → `routes/issue_query.rs` (list_by_ids, list_detail, deleted_list, archived_list + paginator/filter/cursor helpers), `routes/issue_write.rs` (create, bulk_delete, bulk_archive), `routes/issue_sub.rs` (sub_list, sub_add), `routes/issue_common.rs` (shared: scope fns, row structs, `parse_cursor`, envelope builder); `main.rs` (module decls + unchanged paths + fallback).

- [ ] **Step 1: Map the file** — list every `pub/async fn` + `#[cfg(test)]` mod in `issue.rs`; assign each to one of the 4 target files (tests move WITH the code they cover). No fn left behind.
- [ ] **Step 2: Baseline** — `cargo test -p api` green BEFORE (record counts); `git status` clean.
- [ ] **Step 3: Move code** — pure moves only: `pub(crate)` visibility for cross-module items, `use super::issue_common::...` imports. NO logic edits, NO renames (except module paths). `main.rs`: update `issue::x` → `issue_query::x` etc.; route paths UNCHANGED.
- [ ] **Step 4: Fallback** — Axum fallback returning 404 `{"error":"Page not found."}` (quote `error_404.py:9-10` in comment). Verify: unknown path → 404 body; existing routes unaffected.
- [ ] **Step 5: `cargo test -p api`** — counts identical to baseline, 0 failures. `cargo check -p api --tests` clean (no new warnings).
- [ ] **Step 6: Commit** `refactor(rs-api): split issue.rs jadi query/write/sub/common + fallback 404 Page not found`.

---

### Task D1: subscribe + subscribers (new file subscribe.rs)

**Files:** new `routes/subscribe.rs` (`subscribe_status`, `subscribe`, `unsubscribe`, `subscribers_list`, `subscriber_remove` + tests); `main.rs` wire 5 routes (issues path ONLY — no work-items/epics variants).

Contract: `subscriber.py:16-104`, URLs `issue.py:174-189`. Gate = `ProjectLitePermission` = any ACTIVE project member incl. GUEST (`permissions/project.py:136-146`, no role filter — differs from Entity!). GET subscribers returns `ProjectMemberLite` list (counterintuitive — comment citing `subscriber.py:52-57`).

- [ ] **Step 1: Failing tests** — pure: `DUP_SUBSCRIBE_MSG = "User already subscribed to the issue."` const (quoted from `subscriber.py:77`); `subscribed_body(true)` → `{"subscribed":true}`.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): issue subscribe/subscribers paritas Django`.

---

### Task D2: history + meta (new file history.rs)

Contract: `activity.py:24-86` (merged list, `activity_type` switch, `created_at__gt`), `base.py:1186-1198` (meta). Rust reads `issue_activities` (read-only precedent — VERIFY table/columns live: `docker exec plane-db psql … \d issue_activities` + comment table name/columns) and comments table (reuse `work_item.rs` comment shape — check first, reuse if identical).

- [ ] **Step 1: Verify live columns** for activities + comments (`docker exec plane-db psql -U plane -d plane -c "\d issue_activities"`); failing test for `activity_type` switch pure fn (`history_branch("issue-property"|"issue-comment"|missing|junk)` — junk/missing → merged default per `activity.py:81-86`).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): issue history + meta paritas Django`.

---

### Task D3: intake-state + workspace states (append-only state.rs + workspace.rs)

Contract: `state/base.py:136+` (404 `{"error":"Triage state not found"}` verbatim), `workspace/state.py:17+` (WorkspaceEntityPermission roles). Both return `StateSerializer` shape — reuse existing Rust state row struct (check `state.rs` list shape matches `id,project_id,workspace_id,name,color,group,default,description,sequence,order`; adapt if delta, note in commit).

- [ ] **Step 1: Failing tests** — triage-miss message const; `WorkspaceEntityPermission` role gate pure fn.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): intake-state + workspace states paritas Django`.

---

### Task D4: user-properties ×3 (new file userprops.rs)

Contract: `issue/base.py:743-770` (project PATCH 200 + auto-create, GET 200), `cycle/base.py:625-655` (PATCH **201**, missing row → 404), `module/base.py:825-855` (PATCH **201**, missing row → 404). Tables: `project_user_properties`, `cycle_user_properties`, `module_user_properties` — verify live columns (`\d` each); all serializers `__all__`. NO POST.

- [ ] **Step 1: Verify live columns** (3 tables); failing tests for gate pure fns + `missing_prop_patch("cycle")` → 404 vs `missing_prop_patch("project")` → create-then-200 distinction.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): project+cycle+module user-properties paritas Django`.

---

### Task D5: issue-dates POST (new file issue_dates.rs)

Contract: `base.py:1106-1183`. Body `{updates:[{id,start_date,target_date}]}`; per-row validation start≤target else 400 `{"message":"Start date cannot exceed target date"}`. NO explicit transaction — atomicity comes from a single `bulk_update` at the very end (`base.py:1181`); 400 returns before it so failed rows persist nothing. Gate ADMIN/MEMBER.

- [ ] **Step 1: Failing tests** — `validate_dates("2026-01-02","2026-01-01")` → Err("Start date cannot exceed target date"); ok-case passes; merge-new-over-current (`new_start or current_start`, `base.py:1113-1114`).
- [ ] **Step 2: Implement (NO explicit tx — single bulk_update at end, mirror) + wire + test + commit** `feat(rs-api): issue-dates bulk update paritas Django`.

---

### Task D6: versions trio (new file versions.rs)

Contract: `version.py:27-144`, serializers `serializers/issue.py:981-1017,1020-1039`. Cursor pagination — reuse I2 cursor helpers from `issue_common.rs` (same file family, no duplication). Guest-403 in desc-versions iff (role==GUEST AND NOT `project.guest_view_all_features` AND NOT own issue) → 403 `{"error":"You are not allowed to view this issue"}` (`version.py:91-105`, live-prove in T). Intake twin columns — verify live (`\d` the intake version table).

- [ ] **Step 1: Failing tests** — `desc_guest_gate(GUEST, false, false)` → Err("You are not allowed to view this issue"); `(GUEST, true, _)` / `(MEMBER, _, _)` → ok; cursor round-trip reuse (no new helper if existing fits).
- [ ] **Step 2: Implement + wire (3 route groups) + test + commit** `feat(rs-api): issue/snapshot + description versions paritas Django`.

---

### Task D7: single-issue archive (new file issue_archive_one.rs)

Contract: `archive.py:53,221-302`. Reuse `IssueDetailSerializer` shape from I2/D6 work (same struct if identical — verify, reuse, don't fork). POST state-group check: group ∉ {completed,cancelled} → 400 `{"error":"Can only archive completed or cancelled state group issue"}` (key `error`, NOT error_code — differs from I3b!).

- [ ] **Step 1: Failing tests** — group-gate pure fn (`completed|cancelled` ok, else Err("Can only archive completed or cancelled state group issue")).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): single-issue archive GET+POST+DELETE paritas Django`.

---

### Task D8: issue + comment reactions (new file reactions.rs)

**Files:** new `routes/reactions.rs` (issue list/create/destroy + comment list/create/destroy + tests); `main.rs` wire 6 routes (issue path `issues/:iid/reactions[/:code]`, comment path `comments/:cid/reactions[/:code]`).

Contract: `reaction.py:25-85`, `comment.py:163-239`, serializers `serializers/issue.py:649-655,666-685`. GET = default list via `get_queryset` (scope ws+project+issue/comment + active member + `archived_at IS NULL`, `ORDER BY created_at DESC`, `reaction.py:29-43`). Dup semantics DIFFER: issue POST dup → `IntegrityError` → 400 `{"error":"The payload is not valid"}` (no explicit catch!); comment POST dup → 400 `{"error":"Reaction already exists for the user"}` (`comment.py:206-210`). DELETE `:code` is `str` (e.g. `heart`), scoped to `actor=user`, miss → 404.

- [ ] **Step 1: Failing tests** — both dup-message consts above; `reaction_scope_ok` pure fn (actor scoping).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): issue+comment reactions paritas Django`.

---

### Task D9: remove-relation POST (append work_item.rs)

Contract: `relation.py:271-293`. Read existing `work_item.rs` relations handlers for table/shape reuse. 204 on success; relation found via bidirectional OR-filter (`issue_id=X&related=X OR swapped`, `relation.py:276-278`); MISS → Django 500s (AttributeError on None) — Rust returns 404 `missing()` (intentional, document in commit).

- [ ] **Step 1: Failing test** — `related_issue` missing/absent relation → 404 `missing()` (NOT 400 — Django has no such branch).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): remove-relation paritas Django (relation-DELETE tetap OUT — tak ada di Django)`.

---

### Task D10: drafts + draft-to-issue (new file draft.rs)

Contract: `workspace/draft.py:46-311`, URLs `workspace.py:202-216`. Read `DraftIssue` model + both serializers first; verify live columns. Draft-to-issue (`205-311`) is the complex one — read fully, mirror field mapping + 400 cases; Celery skipped.

- [ ] **Step 1: Verify live columns** (`\d draft_issues` + read `DraftIssue` model + both serializers); failing tests for `draft_to_issue` validation: no-project → Err("Project is required to create an issue.").
- [ ] **Step 2: Implement + wire (4 route groups) + test + commit** `feat(rs-api): draft-issues + draft-to-issue paritas Django`.

---

### Task D11: labels (append label.rs)

Contract: `workspace/label.py:17+` (viewer gate), `issue/label.py:23-55` (collection GET via `ModelViewSet.list` defaults over `LabelSerializer` with the scoped queryset `label.py:28-40`; POST 201). Reuse existing Rust label row struct (check `label.rs` shapes first; extend if Django `LabelSerializer` has extra keys).

- [ ] **Step 1: Failing tests** — `DUP_LABEL_MSG = "Label with the same name already exists in the project"` (quoted from `label.py:53`); gate pure fns (collection GET = any active ws member via `ProjectBasePermission` safe-methods, `permissions/project.py:18-22`; POST = ADMIN).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): workspace labels + issue-labels collection paritas Django`.

---

### Task D12: workspace issues + v2 issues + user-issues (new file issue_lists.rs)

Contract: `view/base.py:144-259` (26-key `ViewIssueListSerializer`, `serializers/view.py:24-52`), `issue/base.py:816-972` (v2: 27 keys `base.py:871-898`, `ORDER BY updated_at ASC`, guest scoping, `?description=true`/`?updated_at__gt` — do NOT reuse I1/I2 structs blindly), `workspace/user.py:98-203` (`issue_on_results` reuse pattern). Reuse `issue_common` paginator helpers where shapes allow.

- [ ] **Step 1: Failing tests** — `GROUPBY_SAME_MSG = "Group by and sub group by cannot have same parameters"` (quoted from `user.py:178`); v2 key-list test (27 keys incl. `state__group`, const array length + spot keys).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): workspace-issues + v2-issues + user-issues paritas Django`.

---

### Task D13: inbox-issues PATCH (append intake.rs)

Contract: `intake/base.py:334-...` (decorator ADMIN+creator(Issue); body gates below). Read existing Rust `intake.rs` GET/DELETE handlers — reuse scope + `IntakeIssueDetailSerializer` shape (verify against Django serializer, note delta).

- [ ] **Step 1: Failing tests** — gate matrix pure fn: (no membership, no ws-admin) → 403 "Only admin or creator can update the intake work items"; (guest-role member, not creator, not ws-admin) → 400 "You cannot edit intake issues"; (admin | creator | ws-admin) → ok.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): inbox-issues PATCH paritas Django`.

---

### Task T14: smoke + rebuild + live + push (Batch C pattern)

**Files:** modify `apps/api-rs/scripts/smoke.sh`.

Rules: all new paths are `/api/*` (unlimited) — confirm none land in `auth_router` IP-limited set. Validation-first checks where possible.

- [ ] **Step 1: Add checks** (order: D13 PATCH before any destructive use of the inbox fixture; nothing deactivates membership — no leave-last constraint this batch):
```bash
check sub-status-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check sub-add-201 201 -X POST "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check sub-dup-400 400 -X POST "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check sub-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check subscribers-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/issue-subscribers/"
check history-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/history/"
check meta-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/meta/"
check intake-state-200 200 "$BASE/api/workspaces/$WS/projects/$PID/intake-state/"
check ws-states-200 200 "$BASE/api/workspaces/$WS/states/"
check userprops-get-200 200 "$BASE/api/workspaces/$WS/projects/$PID/user-properties/"
check dates-400 400 -X POST -d '{"updates":[]}' "$BASE/api/workspaces/$WS/projects/$PID/issue-dates/"
check versions-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/versions/"
check descver-200 200 "$BASE/api/workspaces/$WS/projects/$PID/work-items/$IID/description-versions/"
DSID=$(curl -s -H "X-Api-Key: $TOKEN" -H "Origin: $FRONTEND" "$BASE/api/workspaces/$WS/projects/$PID/states/" | python3 -c "import json,sys; print([s['id'] for s in json.load(sys.stdin) if s['group']=='completed'][0])")
check arch1-tmp-create 201 -X POST -d "{\"name\":\"Arch1 tmp\",\"state_id\":\"$DSID\"}" "$BASE/api/workspaces/$WS/projects/$PID/issues/"
AID=$(jid id)
check arch1-post-200 200 -X POST "$BASE/api/workspaces/$WS/projects/$PID/issues/$AID/archive/"
check arch1-get-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$AID/archive/"
check arch1-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/issues/$AID/archive/"
check react-add-201 201 -X POST -d '{"reaction":"heart"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/reactions/"
check react-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/reactions/heart/"
check rel-a-create 201 -X POST -d '{"name":"Rel A"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
RA=$(jid id)
check rel-b-create 201 -X POST -d '{"name":"Rel B"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
RB=$(jid id)
check relate-201 201 -X POST -d "{\"related_issue\":\"$RB\",\"relation_type\":\"relates_to\"}" "$BASE/api/workspaces/$WS/projects/$PID/issues/$RA/relations/"
check remrel-204 204 -X POST -d "{\"related_issue\":\"$RB\"}" "$BASE/api/workspaces/$WS/projects/$PID/issues/$RA/remove-relation/"
check draft-create-201 201 -X POST -d "{\"name\":\"Draft tmp\",\"project_id\":\"$PID\"}" "$BASE/api/workspaces/$WS/draft-issues/"
DID=$(jid id)
check draft-get-200 200 "$BASE/api/workspaces/$WS/draft-issues/$DID/"
check draft-patch-204 204 -X PATCH -d '{"name":"Draft tmp2"}' "$BASE/api/workspaces/$WS/draft-issues/$DID/"
check draft-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/draft-issues/$DID/"
check draft-create2-201 201 -X POST -d "{\"name\":\"Draft conv\",\"project_id\":\"$PID\"}" "$BASE/api/workspaces/$WS/draft-issues/"
DID2=$(jid id)
check draft-to-issue-201 201 -X POST "$BASE/api/workspaces/$WS/draft-to-issue/$DID2/"
check ws-labels-200 200 "$BASE/api/workspaces/$WS/labels/"
check label-create-201 201 -X POST -d '{"name":"SmokeLbl","color":"#ff0000"}' "$BASE/api/workspaces/$WS/projects/$PID/issue-labels/"
check label-dup-400 400 -X POST -d '{"name":"SmokeLbl","color":"#ff0000"}' "$BASE/api/workspaces/$WS/projects/$PID/issue-labels/"
check ws-issues-200 200 "$BASE/api/workspaces/$WS/issues/"
check v2-issues-200 200 "$BASE/api/workspaces/$WS/projects/$PID/v2/issues/"
UID=$(curl -s -H "X-Api-Key: $TOKEN" -H "Origin: $FRONTEND" "$BASE/api/users/me/" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
check user-issues-200 200 "$BASE/api/workspaces/$WS/user-issues/$UID/"
check inbox-intake-create 201 -X POST -d '{"name":"Smoke intake"}' "$BASE/api/workspaces/$WS/projects/$PID/intakes/"
INKID=$(jid id)
check inbox-issue-create 201 -X POST -d "{\"name\":\"Smoke inbox issue\",\"intake_id\":\"$INKID\"}" "$BASE/api/workspaces/$WS/projects/$PID/intake-issues/"
INISSUE=$(jid id)
check inbox-patch-200 200 -X PATCH -d '{"name":"Smoke inbox renamed"}' "$BASE/api/workspaces/$WS/projects/$PID/inbox-issues/$INISSUE/"
check fallback-404 404 "$BASE/api/workspaces/$WS/no-such-path-here/"
grep -q 'Page not found' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   fallback-body -> Page not found"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED fallback-body"; echo "FAIL fallback-body: $(head -c 200 /tmp/smoke_body)"; }
```
Bodies for `relate-201` / `inbox-intake-create` / `inbox-issue-create` / `label-create` MUST match the in-repo Rust handlers (`work_item.rs::create_relations`, `intake.rs::create`, `intake.rs::create_issue`, `label.rs::create`) — read their expected keys first; adjust the `-d` payloads above (not the endpoints). Cleanup: prove zero leftovers post-run (`SELECT count(*) FROM draft_issues/issues/issue_relations/issue_reactions/issue_subscribers/labels WHERE workspace … smoke-%`); extend the cleanup block if any remain (drafts + converted issue + relations especially).
- [ ] **Step 2: Commit smoke** `test(rs-api): smoke batch-D (…)`.
- [ ] **Step 3: `cargo test --workspace`** — 0 failures.
- [ ] **Step 4: Rebuild** `docker compose up -d --build api` → `Started`.
- [ ] **Step 5: Live verify smoke-CAN'T** (temp fixtures, DELETE after): guest-403 desc-versions, D4 PATCH 200-vs-201 deltas, D7-400 non-completed archive, D10 draft-to-issue field mapping spot-check, D12 v2 key spot-check vs Django, D2 `activity_type` branches. Clean temp rows.
- [ ] **Step 6: Full smoke** → PASS=all FAIL=0 → `git push origin preview`.

---

## Self-review (author checklist, run before handoff)

1. **Spec coverage:** every contract-table row owns a task (D0–D13, T14; D8 has its own section after D7). Touched-but-unowned files: none (each task's file list is disjoint; `main.rs`/`smoke.sh` serial). ✔
2. **Placeholder scan:** all error strings quoted verbatim with Django file:line (re-verified 2026-09-06); every new table has a live-`\d` verify step (D2 activities/comments, D4 3 prop tables, D6c intake versions, D10 drafts); smoke payloads pinned to in-repo handler keys. ✔
3. **Type consistency:** Batch C helpers reused (`deny/missing/ws_role/project_role`, 20/15/5); D0 `issue_common.rs` is the single home for paginator/cursor helpers (D6/D12 reuse, no forks); `201` statuses called out explicitly wherever Django differs from 200 (D1a, D8, D10, D11b, D4-cycle/module PATCH). ✔
4. **Collision plan:** D0 merges before lanes; lanes touch disjoint files; `main.rs` applied serially. ✔
5. **FE-dead honesty:** OUT list names FE caller files so the FE team can migrate (`my-issues`→D12c, `views/:id/issues`→D12a, `issue-display-properties`→D4 path+method, relation-DELETE→D9, favorite-GET stays broken-vs-Django). ✔
