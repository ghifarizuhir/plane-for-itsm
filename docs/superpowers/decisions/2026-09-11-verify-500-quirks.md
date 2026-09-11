# ADR Verify-A: stock-Django 500-quirks behind full-shape Rust responses

Date: 2026-09-11
Status: accepted
Format: docs/superpowers/decisions/2026-09-10-f0-parity-decision-format.md

Rule (F0-2): entries whose ONLY deltas are Django bugs / sane mappings may
flip to `implemented` with an ADR pointer. All three entries below return
the serializer shape Django INTENDED (verified against the serializer
declarations + the annotated twin endpoints) with a sane 2xx where stock
Django raises `AttributeError` → 500 via `views/base.py:101-109`.

## 1. `GET issues/:pk/` — `is_intake` never annotated

- `IssueDetailSerializer` declares `description_html / is_subscribed /
is_intake` (`serializers/issue.py:934-945`).
- The pk `retrieve` (`views/issue/base.py:493-624`) annotates
  `is_subscribed` (`:578-586`) but NOT `is_intake`; the only `is_intake`
  annotation in the file serves the `:ident/` twin (`:1318`). Serializing an
  unannotated row raises `AttributeError` → 500.
- Rust `work_item.rs:get_issue` returns the intended 28-key shape
  (`description_html` model column + both `EXISTS` annotations) with 200,
  shared with the `:ident/` path (which Django 200s — same wire shape both
  routes). Guest-view 403 + decorator-first gate order mirrored exactly.

## 2. Comment `POST / PATCH / DELETE` — `is_member` never annotated on writes

- `IssueCommentSerializer` declares `is_member` (`serializers/issue.py:703`);
  the only annotation lives on the list/retrieve queryset
  (`views/issue/comment.py:51-57`).
- `create` (`:63-81`) returns `serializer.data` on the fresh instance,
  `partial_update` (`:109-141`) serializes `current_instance`, `destroy`
  (`:144-160`) serializes `current_instance` — all without the queryset
  annotation → `AttributeError` → 500 on every success path. (List + single
  GET are annotated and 200 — Rust matches those shapes key-for-key,
  including the four nested details.)
- Rust `work_item.rs` returns the intended full rows (scalars + `actor_detail`
  - `issue_detail`/`project_detail`/`workspace_detail`/`comment_reactions` +
    `is_member`) with 201/200/204. Gate order mirrors the
    `@allow_permission([ADMIN], creator=True)` decorator (deny before fetch).

## 3. `GET/PUT/PATCH cycle-issues/:issue_id/` — nested issue counts never annotated

- `CycleIssueSerializer` nests `issue_detail` (`IssueStateSerializer`,
  `serializers/cycle.py:92-99`, `serializers/issue.py:738-749`), whose
  `sub_issues_count / attachment_count / link_count` are plain
  `IntegerField(read_only=True)`.
- `CycleIssueViewSet.get_queryset` (`views/cycle/issue.py:53-72`) annotates
  `sub_issues_count` on the CYCLE row only; the nested issue carries none of
  the three → `AttributeError` → 500 on GET/PUT/PATCH (all three serialize
  the row). No FE callers (`issue.service.ts` only DELETEs).
- Rust `cycle.rs:cycle_issue_full_json` computes all three counts and returns
  the full intended shape (`__all__` + `issue_detail` + `sub_issues_count`)
  with 200. Gates mirror the DRF-default member-filtered queryset (any
  project member incl. guests; non-members/archived 404, not 403).
