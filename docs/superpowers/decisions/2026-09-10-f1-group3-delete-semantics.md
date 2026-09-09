# ADR F1-grup3: DELETE semantics (hard-vs-soft, destroy miss)

Date: 2026-09-10
Status: accepted
Format: docs/superpowers/decisions/2026-09-10-f0-parity-decision-format.md

## 1. V2 attachments DELETE stays HARD (`deviation_accepted`, no code change)

- Route `/api/assets/v2/.../issues/:issue_id/attachments/:pk/` DELETE serves V2 URL
  with V1 HARD semantics (`DELETE FROM file_assets`, `asset.rs:1521-1522`) per E9
  contract (v1 `IssueAttachmentEndpoint.delete`, `attachment.py:62-86`: HARD +
  miss 404 `Issue attachment not found.`), NOT the V2 soft-delete
  (`attachment.py:149-170`: `is_deleted=True`).
- DECISION: keep Rust HARD. Observable deltas vs Django V2 (DELETE-twice,
  GET-after-DELETE, restore-after-delete) are accepted contract deviations.
  GET/PATCH branches already match Django V2 byte-exact, so the whole entry flips
  to `deviation_accepted`.
- Lock: no DB test harness exists in the `api` crate (pure unit tests only);
  locked via this ADR + inventory entry + code comment at `asset.rs:1486-1493`.

## 2. Estimate destroy: check-before-delete (code fix)

- Django `BulkEstimatePointEndpoint.destroy` (`estimate/base.py:147-150`): `.get()`
  miss → 404 with NO side effects. Old Rust order deleted `estimate_points`
  BEFORE the estimate existence check → a miss wiped points then returned 404.
- FIX (`estimate.rs:destroy`): `SELECT EXISTS` first → `missing()` (generic 404,
  `views/base.py:92-96`) with zero side effects; then delete points + estimate → 204.
  Miss string unified from custom `Estimate not found` to generic (F1 rule).
- Entry stays `shape_mismatch` (KEYS list-shape + gate still TODO).

## 3. Estimate-point destroy: miss check (code fix, partial)

- Django `EstimatePointEndpoint.destroy` (`estimate/base.py:196-268`): miss →
  404 `{"error": "Estimate point not found"}` (`:248-252`, SPECIFIC string, kept
  verbatim — not generic `missing()`).
- FIX (`estimate.rs:destroy_point`): `SELECT EXISTS` first → 404 specific string.
  Check runs BEFORE the issue remap (Django remaps first then 404s); Rust
  intentionally avoids destructive side effects on miss (sane-mapping precedent).
- Deferred to F2/F6: 200 updated-points array (vs 204) + key rearrangement
  (`base.py:254-261` bulk key decrement). Entry stays `shape_mismatch`.
