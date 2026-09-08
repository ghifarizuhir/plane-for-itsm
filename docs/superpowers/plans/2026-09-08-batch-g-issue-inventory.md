# Batch G: Issue-Domain Inventory Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inventory all 40 Django `plane/app/urls/issue.py` paths in `parity-inventory.json` (33 new `issue`-domain entries + 1 evidence update), so the issue domain — the highest FE-traffic surface — is fully tracked with status and FE evidence.

**Architecture:** JSON-only expansion, no handler changes. Audit (2026-09-08) shows all 40 issue.py paths are already registered in `main.rs`, so this batch is documentation + verification: each entry records the real methods/handler (from `main.rs` wiring), `rust_status` from a Django-view-vs-handler comparison (recipe below), and `fe_evidence` mined from `apps/web/core/services/issue/*.ts`. The two gate tests stay green throughout; any `shape_mismatch` found becomes a Batch G-fix task in a follow-up plan, NOT in this batch.

**Tech Stack:** JSON (`apps/api-rs/crates/api/parity-inventory.json`), existing gate tests (`route_inventory_test`, `fe_tripwire_test`), Python one-liners for verification. No new deps.

---

## Audit reference (2026-09-08, static)

All 40 `issue.py` paths are `IN-RS` (registered in `main.rs`). 6 already inventoried (Batch F: issue-links×2, issue-relation, legacy issue-attachments×2, work-items `:ident`). `bulk-create-labels/` lives under the `label` domain. Remaining: **33 entries** across Tasks G1–G10, plus 1 evidence update (G11).

Domain rule: inventory is organized by Django urls file (audit source), so all `issue.py` paths go under the `issue` domain even when the Rust module differs (e.g. `asset.rs`, `userprops.rs`) — the `rust_module` field records the real module.

## Status recipe (used by every task)

For each path, determine `rust_status`:

1. Path not in `main.rs` → `missing`. (Does not occur for issue.py, but the rule stands.)
2. Else read the Django view (class shown in the task table; find it with `grep -rn "class <View>" apps/api/plane/views/`) and the Rust handler(s) in `apps/api-rs/crates/api/src/routes/<module>.rs`.
3. Compare, timeboxed to: (a) response status codes, (b) response JSON keys (DRF serializer `fields` vs Rust `json!` keys / row structs), (c) error message strings, (d) permission gates.
4. Any delta → `shape_mismatch` (`constraint_mismatch` if validation-only). `notes` MUST describe the delta with `file:line` on both sides, e.g. `"GET list keys differ: Django IssueSerializer has X (serializers/issue.py:123), Rust list omits X (issue_query.rs:45)"`.
5. No delta found → `implemented`, `notes` = `"verified <DjangoView> vs <rust_handler>"`.
6. Known precedent: `issues/` GET returns the paginated envelope (commits `43736b5`, skeleton fix `2026-09-08`); if the envelope contract holds, status is `implemented` with that note.

`fe_pages` rule: default `[]`. Fill ONLY if `grep -rn "<method>" apps/web/app --include=*.tsx` shows callers from exactly one page; otherwise leave `[]` (the method name is the stable key, never guess a page).

## Worked example (table row → JSON object)

Table row:

| path                                                      | methods | rust_handler                       | django_source                              | fe_evidence                                                        |
| --------------------------------------------------------- | ------- | ---------------------------------- | ------------------------------------------ | ------------------------------------------------------------------ |
| `/api/workspaces/:slug/projects/:project_id/issues/list/` | GET     | `routes::issue_query::list_by_ids` | `app/urls/issue.py:37` (IssueListEndpoint) | `retrieveIssues` @ `apps/web/core/services/issue/issue.service.ts` |

becomes (appended to `domains.issue.endpoints`):

```json
{
  "methods": ["GET"],
  "path": "/api/workspaces/:slug/projects/:project_id/issues/list/",
  "django_source": "app/urls/issue.py:37",
  "rust_status": "implemented",
  "rust_handler": "routes::issue_query::list_by_ids",
  "fe_evidence": [{ "service": "apps/web/core/services/issue/issue.service.ts", "method": "retrieveIssues" }],
  "fe_pages": [],
  "batch_task": "Batch G T1",
  "out_scope": false,
  "notes": "verified IssueListEndpoint vs list_by_ids"
}
```

`rust_module` for every G-task entry: the module owning the primary handler (e.g. `"routes/issue_query.rs"`); the `issue` domain object keeps its existing `"rust_module": "routes/work_item.rs"` (domain-level label, unchanged).

---

### Task 1: G1 issues core list/detail (5 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append 5 objects to `domains.issue.endpoints`)
- Test: `apps/api-rs/crates/api/tests/route_inventory_test.rs` + `fe_tripwire_test.rs` (existing, must stay green)

Entries (all `out_scope: false`, `fe_pages: []` per rule, `batch_task: "Batch G T1"`):

| path                                                        | methods            | rust_handler                       | django_source (view)                           | fe_evidence (all @ `apps/web/core/services/issue/issue.service.ts`) |
| ----------------------------------------------------------- | ------------------ | ---------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issues/list/`   | GET                | `routes::issue_query::list_by_ids` | `app/urls/issue.py:37` (IssueListEndpoint)     | `retrieveIssues`                                                    |
| `/api/workspaces/:slug/projects/:project_id/issues/`        | GET, POST          | `routes::issue_query::list`        | `app/urls/issue.py:42` (IssueViewSet)          | `createIssue`, `getIssuesFromServer`, `getIssuesWithParams`         |
| `/api/workspaces/:slug/projects/:project_id/issues-detail/` | GET                | `routes::issue_query::list_detail` | `app/urls/issue.py:47` (IssueDetailEndpoint)   | `getIssuesFromServer`                                               |
| `/api/workspaces/:slug/projects/:project_id/v2/issues/`     | GET                | `routes::issue_lists::v2_issues`   | `app/urls/issue.py:54` (IssuePaginatedViewSet) | `getIssuesForSync`                                                  |
| `/api/workspaces/:slug/projects/:project_id/issues/:pk/`    | GET, PATCH, DELETE | `routes::work_item::get_issue`     | `app/urls/issue.py:59` (IssueViewSet)          | `retrieve`, `patchIssue`, `deleteIssue`                             |

- [ ] **Step 1: Append the 5 entries**

Shape follows the worked example above. Anchor for the edit: the tail of the current `issue` domain array is the `work-items/:ident/` object (Batch F T10); append after it, preserving JSON validity. Determine `rust_status` + `notes` per entry via the status recipe (compare each Django view against its handler: `issue_query.rs` `list_by_ids`/`list`/`list_detail`, `issue_lists.rs` `v2_issues`, `work_item.rs` `get_issue`/`patch_issue`/`delete_issue`).

- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3 tests). If `fe_tripwire` fails on a new evidence entry, the evidence is wrong — re-check the FE method's URL template, do not weaken the test.

- [ ] **Step 4: Presence check for this task's paths**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issues/list/',
  '/api/workspaces/:slug/projects/:project_id/issues/',
  '/api/workspaces/:slug/projects/:project_id/issues-detail/',
  '/api/workspaces/:slug/projects/:project_id/v2/issues/',
  '/api/workspaces/:slug/projects/:project_id/issues/:pk/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G1 present: 5/5')
"
```

Expected: `G1 present: 5/5`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory issue core list/detail (Batch G T1)"
```

---

### Task 2: G2 labels (2 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G1 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T2"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                           | methods            | rust_handler                       | django_source (view)                  | fe_evidence (all @ `apps/web/core/services/issue/issue_label.service.ts`) |
| -------------------------------------------------------------- | ------------------ | ---------------------------------- | ------------------------------------- | ------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issue-labels/`     | GET, POST          | `routes::label::issue_labels_list` | `app/urls/issue.py:71` (LabelViewSet) | `getProjectLabels`, `createIssueLabel`                                    |
| `/api/workspaces/:slug/projects/:project_id/issue-labels/:pk/` | GET, PATCH, DELETE | `routes::label::detail`            | `app/urls/issue.py:76` (LabelViewSet) | `patchIssueLabel`, `deleteIssueLabel`                                     |

- [ ] **Step 1: Append the 2 entries** (shape = worked example; status/notes via recipe comparing `LabelViewSet` against `label.rs` `issue_labels_list`/`issue_labels_create`/`detail`/`patch`/`destroy`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issue-labels/',
  '/api/workspaces/:slug/projects/:project_id/issue-labels/:pk/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G2 present: 2/2')
"
```

Expected: `G2 present: 2/2`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory issue labels (Batch G T2)"
```

---

### Task 3: G3 bulk ops + sub-issues (3 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G2 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T3"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                      | methods   | rust_handler                        | django_source (view)                               | fe_evidence (all @ `apps/web/core/services/issue/issue.service.ts`) |
| ------------------------------------------------------------------------- | --------- | ----------------------------------- | -------------------------------------------------- | ------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/bulk-delete-issues/`          | DELETE    | `routes::issue_write::bulk_delete`  | `app/urls/issue.py:93` (BulkDeleteIssuesEndpoint)  | `bulkDeleteIssues`                                                  |
| `/api/workspaces/:slug/projects/:project_id/bulk-archive-issues/`         | POST      | `routes::issue_write::bulk_archive` | `app/urls/issue.py:98` (BulkArchiveIssuesEndpoint) | `bulkArchiveIssues`                                                 |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/sub-issues/` | GET, POST | `routes::issue_sub::sub_list`       | `app/urls/issue.py:104` (SubIssuesEndpoint)        | `subIssues`, `addSubIssues`                                         |

- [ ] **Step 1: Append the 3 entries** (status/notes via recipe; compare against `issue_write.rs` `bulk_delete`/`bulk_archive` and `issue_sub.rs` `sub_list`/`sub_add`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/bulk-delete-issues/',
  '/api/workspaces/:slug/projects/:project_id/bulk-archive-issues/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/sub-issues/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G3 present: 3/3')
"
```

Expected: `G3 present: 3/3`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory bulk ops + sub-issues (Batch G T3)"
```

---

### Task 4: G4 comments (2 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G3 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T4"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                        | methods            | rust_handler                       | django_source (view)                          | fe_evidence (all @ `apps/web/core/services/issue/issue_comment.service.ts`)                                                        |
| --------------------------------------------------------------------------- | ------------------ | ---------------------------------- | --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/`     | GET, POST          | `routes::work_item::list_comments` | `app/urls/issue.py:156` (IssueCommentViewSet) | `createIssueComment` (GET list has no direct FE list caller — `getIssueComments` hits `history/`; record only the verified caller) |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/:pk/` | GET, PATCH, DELETE | `routes::work_item::get_comment`   | `app/urls/issue.py:161` (IssueCommentViewSet) | `patchIssueComment`, `deleteIssueComment`                                                                                          |

- [ ] **Step 1: Append the 2 entries** (status/notes via recipe; compare against `work_item.rs` `list_comments`/`create_comment`/`get_comment`/`patch_comment`/`delete_comment`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/:pk/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G4 present: 2/2')
"
```

Expected: `G4 present: 2/2`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory issue comments (Batch G T4)"
```

---

### Task 5: G5 subscribers (3 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G4 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T5"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                                            | methods           | rust_handler                             | django_source (view)                             | fe_evidence                                                                                                                                                      |
| ----------------------------------------------------------------------------------------------- | ----------------- | ---------------------------------------- | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/`                | GET               | `routes::subscribe::subscribers_list`    | `app/urls/issue.py:175` (IssueSubscriberViewSet) | none found in issue services — confirm with `grep -rn "issue-subscribers" apps/web/core/services packages/services/src`; `[]` if confirmed absent                |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/:subscriber_id/` | DELETE            | `routes::subscribe::subscriber_remove`   | `app/urls/issue.py:180` (IssueSubscriberViewSet) | same grep; `[]` if confirmed absent                                                                                                                              |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/subscribe/`                        | GET, POST, DELETE | `routes::subscribe::subscription_status` | `app/urls/issue.py:185` (IssueSubscriberViewSet) | `getIssueNotificationSubscriptionStatus`, `subscribeToIssueNotifications`, `unsubscribeFromIssueNotifications` @ `apps/web/core/services/issue/issue.service.ts` |

- [ ] **Step 1: Append the 3 entries** (status/notes via recipe; compare against `subscribe.rs` `subscribers_list`/`subscriber_remove`/`subscribe`/`subscription_status`/`unsubscribe`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/:subscriber_id/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/subscribe/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G5 present: 3/3')
"
```

Expected: `G5 present: 3/3`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory issue subscribers (Batch G T5)"
```

---

### Task 6: G6 reactions (4 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G5 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T6"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                                        | methods   | rust_handler                                  | django_source (view)                             | fe_evidence (all @ `apps/web/core/services/issue/issue_reaction.service.ts`) |
| ------------------------------------------------------------------------------------------- | --------- | --------------------------------------------- | ------------------------------------------------ | ---------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/reactions/`                    | GET, POST | `routes::reactions::issue_reactions_list`     | `app/urls/issue.py:192` (IssueReactionViewSet)   | `createIssueReaction`, `listIssueReactions`                                  |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/reactions/:reaction_code/`     | DELETE    | `routes::reactions::issue_reaction_destroy`   | `app/urls/issue.py:197` (IssueReactionViewSet)   | `deleteIssueReaction`                                                        |
| `/api/workspaces/:slug/projects/:project_id/comments/:comment_id/reactions/`                | GET, POST | `routes::reactions::comment_reactions_list`   | `app/urls/issue.py:204` (CommentReactionViewSet) | `createIssueCommentReaction`, `listIssueCommentReactions`                    |
| `/api/workspaces/:slug/projects/:project_id/comments/:comment_id/reactions/:reaction_code/` | DELETE    | `routes::reactions::comment_reaction_destroy` | `app/urls/issue.py:209` (CommentReactionViewSet) | `deleteIssueCommentReaction`                                                 |

- [ ] **Step 1: Append the 4 entries** (status/notes via recipe; compare against `reactions.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/reactions/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/reactions/:reaction_code/',
  '/api/workspaces/:slug/projects/:project_id/comments/:comment_id/reactions/',
  '/api/workspaces/:slug/projects/:project_id/comments/:comment_id/reactions/:reaction_code/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G6 present: 4/4')
"
```

Expected: `G6 present: 4/4`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory issue reactions (Batch G T6)"
```

---

### Task 7: G7 history/meta/props/dates (4 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G6 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T7"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                   | methods    | rust_handler                             | django_source (view)                                         | fe_evidence                                                                                                                                                                                                                            |
| ---------------------------------------------------------------------- | ---------- | ---------------------------------------- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/history/` | GET        | `routes::history::history`               | `app/urls/issue.py:149` (IssueActivityEndpoint)              | `getIssueActivities` @ `apps/web/core/services/issue/issue.service.ts` + `apps/web/core/services/issue/issue_activity.service.ts`; `getIssueComments` @ `apps/web/core/services/issue/issue_comment.service.ts` (also hits `history/`) |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/meta/`    | GET        | `routes::history::meta`                  | `app/urls/issue.py:276` (IssueMetaEndpoint)                  | `getIssueMetaFromURL` @ `apps/web/core/services/issue/issue.service.ts`                                                                                                                                                                |
| `/api/workspaces/:slug/projects/:project_id/user-properties/`          | GET, PATCH | `routes::userprops::project_props_get`   | `app/urls/issue.py:216` (ProjectUserDisplayPropertyEndpoint) | none found in issue services — confirm with `grep -rn "user-properties" apps/web/core/services packages/services/src`; `[]` if confirmed absent                                                                                        |
| `/api/workspaces/:slug/projects/:project_id/issue-dates/`              | POST       | `routes::issue_dates::bulk_update_dates` | `app/urls/issue.py:251` (IssueBulkUpdateDateEndpoint)        | `updateIssueDates` @ `apps/web/core/services/issue/issue.service.ts`                                                                                                                                                                   |

- [ ] **Step 1: Append the 4 entries** (status/notes via recipe; compare against `history.rs` `history`/`meta`, `userprops.rs` `project_props_get`/`project_props_patch`, `issue_dates.rs` `bulk_update_dates`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/history/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/meta/',
  '/api/workspaces/:slug/projects/:project_id/user-properties/',
  '/api/workspaces/:slug/projects/:project_id/issue-dates/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G7 present: 4/4')
"
```

Expected: `G7 present: 4/4`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory history/meta/props/dates (Batch G T7)"
```

---

### Task 8: G8 archive/trash (3 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G7 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T8"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                             | methods           | rust_handler                          | django_source (view)                               | fe_evidence                                                                                                       |
| ---------------------------------------------------------------- | ----------------- | ------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/archived-issues/`    | GET               | `routes::issue_query::archived_list`  | `app/urls/issue.py:223` (IssueArchiveViewSet)      | `getArchivedIssues` @ `apps/web/core/services/issue/issue_archive.service.ts`                                     |
| `/api/workspaces/:slug/projects/:project_id/issues/:pk/archive/` | GET, POST, DELETE | `routes::issue_archive_one::retrieve` | `app/urls/issue.py:228` (IssueArchiveViewSet)      | `archiveIssue`, `restoreIssue`, `retrieveArchivedIssue` @ `apps/web/core/services/issue/issue_archive.service.ts` |
| `/api/workspaces/:slug/projects/:project_id/deleted-issues/`     | GET               | `routes::issue_query::deleted_list`   | `app/urls/issue.py:246` (DeletedIssuesListViewSet) | `getDeletedIssues` @ `apps/web/core/services/issue/issue.service.ts`                                              |

- [ ] **Step 1: Append the 3 entries** (status/notes via recipe; compare against `issue_query.rs` `archived_list`/`deleted_list` and `issue_archive_one.rs` `archive`/`retrieve`/`unarchive`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/archived-issues/',
  '/api/workspaces/:slug/projects/:project_id/issues/:pk/archive/',
  '/api/workspaces/:slug/projects/:project_id/deleted-issues/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G8 present: 3/3')
"
```

Expected: `G8 present: 3/3`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory archive/trash (Batch G T8)"
```

---

### Task 9: G9 versions (4 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G8 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T9"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                                            | methods | rust_handler                             | django_source (view)                                         | fe_evidence                                                                                                                                       |
| ----------------------------------------------------------------------------------------------- | ------- | ---------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/`                         | GET     | `routes::versions::issue_versions_list`  | `app/urls/issue.py:256` (IssueVersionEndpoint)               | none found in issue services — confirm with `grep -rn "issues/.*versions" apps/web/core/services packages/services/src`; `[]` if confirmed absent |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/:pk/`                     | GET     | `routes::versions::issue_version_detail` | `app/urls/issue.py:261` (IssueVersionEndpoint)               | same grep; `[]` if confirmed absent                                                                                                               |
| `/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/`     | GET     | `routes::versions::desc_versions_list`   | `app/urls/issue.py:266` (WorkItemDescriptionVersionEndpoint) | `listDescriptionVersions` @ `apps/web/core/services/issue/work_item_version.service.ts`                                                           |
| `/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/:pk/` | GET     | `routes::versions::desc_version_detail`  | `app/urls/issue.py:271` (WorkItemDescriptionVersionEndpoint) | `retrieveDescriptionVersion` @ `apps/web/core/services/issue/work_item_version.service.ts`                                                        |

- [ ] **Step 1: Append the 4 entries** (status/notes via recipe; compare against `versions.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/:pk/',
  '/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/',
  '/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/:pk/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G9 present: 4/4')
"
```

Expected: `G9 present: 4/4`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory versions (Batch G T9)"
```

---

### Task 10: G10 attachments v2 + remove-relation (3 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the G9 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch G T10"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                                                     | methods            | rust_handler                         | django_source (view)                                | fe_evidence                                                                                                               |
| ---------------------------------------------------------------------------------------- | ------------------ | ------------------------------------ | --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/`     | GET, POST          | `routes::asset::issue_list`          | `app/urls/issue.py:137` (IssueAttachmentV2Endpoint) | `uploadIssueAttachment`, `getIssueAttachments` @ `apps/web/core/services/issue/issue_attachment.service.ts`               |
| `/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/:pk/` | GET, PATCH, DELETE | `routes::asset::issue_get`           | `app/urls/issue.py:142` (IssueAttachmentV2Endpoint) | `updateIssueAttachmentUploadStatus`, `deleteIssueAttachment` @ `apps/web/core/services/issue/issue_attachment.service.ts` |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/remove-relation/`           | POST               | `routes::work_item::remove_relation` | `app/urls/issue.py:240` (IssueRelationViewSet)      | `deleteIssueRelation` @ `apps/web/core/services/issue/issue_relation.service.ts`                                          |

- [ ] **Step 1: Append the 3 entries** (status/notes via recipe; compare against `asset.rs` `issue_list`/`issue_presign`/`issue_get`/`issue_complete`/`issue_delete` and `work_item.rs` `remove_relation`)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/',
  '/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/:pk/',
  '/api/workspaces/:slug/projects/:project_id/issues/:issue_id/remove-relation/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('G10 present: 3/3')
"
```

Expected: `G10 present: 3/3`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory attachments v2 + remove-relation (Batch G T10)"
```

---

### Task 11: G11 identifier evidence + final verification

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (add 1 `fe_evidence` entry to the existing `work-items/:ident/` object; no other JSON changes)

- [ ] **Step 1: Add identifier FE evidence**

In `domains.issue.endpoints`, find the object with `"path": "/api/workspaces/:slug/work-items/:ident/"` and append to its `fe_evidence` array:

```json
{ "service": "apps/web/core/services/issue/issue.service.ts", "method": "retrieveWithIdentifier" }
```

(`retrieveWithIdentifier` calls `/api/workspaces/${workspaceSlug}/work-items/${project_identifier}-${issue_sequence}/` — verified in `issue.service.ts`. `batch_task` stays `"Batch F T10"`.)

- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Full domain audit — every issue.py path inventoried**

Run:

```bash
python3 -c "
import json, re
src = open('apps/api/plane/app/urls/issue.py').read()
def canon(s):
    return '/api/' + re.sub(r'<(?:str|uuid|int):(\w+)>', r':\1', s)
dpaths = set()
for m in re.finditer(r'path\(\s*(?:[ru]?\"([^\"]+)\"|ru?\'([^\']+)\')', src):
    dpaths.add(canon(m.group(1) or m.group(2)))
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
have = set(ep['path'] for d in inv['domains'].values() for ep in d['endpoints'])
# bulk-create-labels lives under label domain (Batch F); :ident form covers the identifier route
have.add('/api/workspaces/:slug/projects/:project_id/bulk-create-labels/')
missing = sorted(dpaths - have)
extra_note = 'work-items/:ident/ covers :project_identifier-:issue_identifier (T10 constraint in handler)'
print('issue.py paths:', len(dpaths), '| inventoried:', len(dpaths & have))
assert not missing, missing
issue_eps = inv['domains']['issue']['endpoints']
print('issue domain entries:', len(issue_eps))
assert len(issue_eps) == 39, len(issue_eps)
print('G11 audit: OK -', extra_note)
"
```

Expected: `issue.py paths: 40 | inventoried: 40`, `issue domain entries: 39`, `G11 audit: OK`.

- [ ] **Step 4: Run the full api test suite**

Run: `cargo test -p api`
Expected: 0 failed (includes `helper_test` 7, `route_inventory_test` 3, `fe_tripwire_test` 3, `parity_gate_test` 2 + 1 ignored).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): identifier FE evidence + issue domain audit (Batch G T11)"
```

---

## Final state after Batch G

- `issue` domain: 39 entries (6 Batch F + 33 Batch G), every `issue.py` path tracked.
- Any `shape_mismatch`/`constraint_mismatch` found during G1–G10 is RECORDED in the entry (`rust_status` + `notes`) but NOT fixed here — fixes go to a follow-up "Batch G-fix" plan, one task per mismatch.
- Gates green; `cargo test -p api` 0 failed.

## Out of scope

- Fixing handlers for any `shape_mismatch` found (follow-up plan).
- Expanding other domains (`workspace`, `project`, `cycle`, `module`, …) — Batch H+.
- Changing gate tests, shadow.sh, or FE code.
- `bulk-create-labels/` (already under `label` domain).

## Self-review notes

- Spec coverage: matrix design spec §3 (schema per entry) → every task's table + worked example; §5 workflow (one domain per batch, T-tasks per gap) → G1–G11; §6 rollout (expand per domain, FE-impact priority) → issue domain chosen on FE call-site + Django surface data.
- Placeholder scan: no TBD/TODO; every task has exact paths, methods (from `main.rs` wiring), handlers (from `main.rs` wiring), Django sources with line numbers, FE evidence with file + method, exact commands with expected output.
- Type consistency: `batch_task` values `"Batch G T1"`–`"Batch G T11"` unique per task; `rust_status` values restricted to the 4 spec enum values; canonical `:param` path form matches the gate's exact-string comparison; `fe_evidence` service paths are repo-root-relative as consumed by `fe_tripwire_test`.
- Task ordering: G1–G10 append sequentially to the same `domains.issue.endpoints` array — must run sequentially, not in parallel. G11 runs last (audit over the complete domain).
