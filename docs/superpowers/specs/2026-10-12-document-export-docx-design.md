# Document Export (DOCX) for Work Items & KB Articles — Design

Date: 2026-10-12
Status: Approved in brainstorming (2026-10-12); awaiting written-spec review.
Scope: web (`apps/web`, `packages/types`, `packages/constants`, `packages/i18n`), API (`apps/api-rs` + sqlx migration), Django parity model/migration (`apps/api`). No changes to `apps/live`.
Related: issue export pipeline (`apps/api/plane/bgtasks/export_task.py`, `apps/web/core/components/exporter/`), page PDF export (`apps/web/core/components/pages/modals/export-page-modal.tsx`, `apps/web/core/components/editor/pdf/document.tsx`), settings registry (`packages/constants/src/settings/workspace.ts`), Work Item Types settings as the end-to-end workspace-config precedent (`apps/web/core/components/work-item-types/root.tsx`, `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`).

## Background

ITSM users need to hand formal, letterhead-style documents to external parties (auditors, regulators, customers): a work item as a formal case/incident report, and a KB article as a controlled document. Existing exports do not fit: work item export produces CSV/XLSX/JSON data dumps; page export produces PDF/Markdown but no editable office document and no workspace-configurable letterhead.

Best practice note: a formal export with a configurable header/footer, a properties table, and a milestone-only activity appendix is an established ITSM pattern. Full activity logs are included only as milestones because raw history is noisy and not auditor-friendly.

## Locked decisions (brainstorming 2026-10-12)

1. **Output format:** DOCX (Word), editable office document.
2. **Generation location:** client-side in `apps/web` using the `docx` npm library (MIT). Instant download; no server compute, no async job.
3. **Work item document contains:** properties table, description body, linked work items, history appendix. No comment thread section, no attachments list. Comment events appear only as (filtered-out) history; not rendered.
4. **KB article document contains:** title + body, properties block, sub-pages (recursive), version info.
5. **Header/footer config:** workspace-level, contains header logo (workspace logo or custom upload), header text lines, footer text lines, page numbers, generated-by line, and header placeholders.
6. **History scope:** milestones only — created, state change, priority change, assignee added/removed, start/target date change, archive. Excluded: label edits, description edits, comments/votes/reactions, link edits, attachment edits, estimate changes.
7. **Entry points:** single work item (detail header + kebab menu) and single KB article (existing page export modal). No bulk/paged/tree export.
8. **Approach:** client-side generation (approach A). Document model is defined separately from the renderer so generation can move server-side later if needed.

## Non-goals (this phase)

- Bulk export (multiple work items, whole project, whole pages tree).
- Other formats: PDF generation for these documents (PDF already exists separately for pages), ODT, HTML file.
- Server-side generation, async export jobs, export audit history (`ExporterHistory`), email delivery.
- Live in-app preview of the header/footer before export (settings form only; can be added later).
- Per-project or per-document-type template overrides (one config per workspace).
- Comment bodies, attachment binaries, or embedded file lists in the work item document.
- Translating document section headings beyond the app's current locale (document uses app locale strings; new i18n keys added to `en`, other locales via the standard translate workflow).

## Design

### 1. Architecture & module layout

All new web code lives under `apps/web/core/components/document-export/`:

| File                                  | Responsibility                                                                                                                                                                |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `build-document-shell.ts`             | Assembles the DOCX document: header (logo + text lines), footer (text lines, `PageNumber`/`NumberOfTotalPages`, generated-by), core document metadata (title/author/created). |
| `html-to-docx.ts`                     | `DOMParser` walker: TipTap `description_html` → docx elements. Shared by both templates.                                                                                      |
| `build-work-item-doc.ts`              | Work item template: title block, properties table, description, linked items, history appendix.                                                                               |
| `build-page-doc.ts`                   | KB article template: title, properties block, body, recursive sub-pages, version info.                                                                                        |
| `resolve-placeholders.ts`             | Replaces `{workspace_name}`, `{document_title}`, `{document_id}`, `{export_date}`, `{exported_by}` in configured text lines.                                                  |
| `milestones.ts`                       | Filters `IssueActivity` rows to milestone events; maps `field` → label.                                                                                                       |
| `packages/types/src/document-export/` | `TDocumentExportSettings`, text-line/alignment types, builder input types.                                                                                                    |

Download uses the existing blob pattern from `ExportPageModal.initiateDownload` (`apps/web/core/components/pages/modals/export-page-modal.tsx:136-145`). Filenames: `WORK-123-title-slug.docx` and `article-title-slug.docx` (reuse existing slug helpers where present).

`docx` is added to the pnpm catalog (`pnpm-workspace.yaml`) and referenced as `catalog:` in `apps/web/package.json`, per repo dependency convention.

### 2. Data model & migrations

New table `workspace_document_settings` (one row per workspace, soft-deletable), following the `intake_sources` migration style (`apps/api-rs/migrations/0015_intake_sources.sql`):

```sql
CREATE TABLE IF NOT EXISTS public.workspace_document_settings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL,
    header_logo_mode varchar(16) NOT NULL DEFAULT 'none',  -- none | workspace | custom
    header_logo_asset_id uuid,
    header_text_lines jsonb NOT NULL DEFAULT '[]'::jsonb,
    footer_text_lines jsonb NOT NULL DEFAULT '[]'::jsonb,
    show_page_numbers boolean NOT NULL DEFAULT true,
    show_generated_by boolean NOT NULL DEFAULT true,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);
CREATE UNIQUE INDEX IF NOT EXISTS workspace_document_settings_ws_uniq
    ON public.workspace_document_settings (workspace_id) WHERE deleted_at IS NULL;
```

- Migration file: `apps/api-rs/migrations/0019_workspace_document_settings.sql` (next free number, never reuse per migrations README).
- Text-line shape: `[{ "text": "PT Example — IT Services", "align": "left|center|right" }]`; `text` may contain placeholders.
- `header_logo_asset_id` references `file_assets` as a plain uuid column without an SQL FK constraint, following the `intake_sources` convention.
- Row absence = defaults: `header_logo_mode='none'`, empty lines, `show_page_numbers=true`, `show_generated_by=true`. No row is created until first save.
- Django parity: model in `apps/api/plane/db/models/` + migration (latest number + 1) + serializer registration, mirroring the `IssueType` parity pattern. Django remains migrator/tests only.

### 3. API endpoints (Rust api-rs)

New handlers in `apps/api-rs/crates/api/src/routes/document_export.rs` (registered in `main.rs` next to the workspace/work-item-type routes):

- `GET /api/workspaces/:slug/document-export-settings/` — any workspace member; returns defaults when no row exists (no write on read).
- `PATCH /api/workspaces/:slug/document-export-settings/` — workspace admin only; validates logo mode, line arrays (length caps: 5 header lines, 5 footer lines, 200 chars/line, align enum), upserts row.
- Asset upload for `custom` logo reuses the generic workspace file-asset flow: add entity type `DOC_HEADER_LOGO` to the workspace asset upload and a `mark_uploaded` binding arm in `apps/api-rs/crates/api/src/routes/asset.rs` that sets `workspace_document_settings.header_logo_asset_id` (soft-deleting the previous asset), mirroring the `WORKSPACE_LOGO` arm (`asset.rs:1325-1345`).

Response includes `header_logo_url` resolved from the asset (same pattern as `workspace_logo_url`). Permission gate: read = workspace member, write = workspace admin (`ADMIN` role), same gate helpers used by work-item-type routes.

### 4. Workspace settings UI

- New registry entry in `packages/constants/src/settings/workspace.ts` ("Document export") under the Workspace category.
- New page `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/document-export/page.tsx` with a form component under `apps/web/core/components/settings/` (or `document-export/settings/`):
  - Logo mode radio: None / Use workspace logo / Custom upload (upload uses existing asset upload flow).
  - Header text lines editor: ordered rows (text input + align select), add/remove, max 5, placeholder cheat-sheet shown inline.
  - Footer text lines editor: same, max 5.
  - Toggles: page numbers, generated-by line.
  - Admin-only edit; non-admin members see read-only state.
- New service `apps/web/core/services/document-export-settings.service.ts` + SWR key in `packages/constants/src/fetch-keys.ts`.
- Types in `packages/types` (workspace or new `document-export` domain).
- New i18n keys in `packages/i18n/src/locales/en/` (settings labels, doc section headings, export modal strings); propagate other locales via the translate workflow, not in this phase.

### 5. Document shell (header/footer)

- **Header**: logo image (if mode resolves to an asset) on the left; text lines below/right per their align. `ImageRun` supports PNG/JPEG/GIF; workspace logos that are SVG are rasterized client-side via canvas when possible, otherwise the logo is omitted (never fail the export). Logo max height 48pt equivalent, width auto.
- **Header placeholders**: resolved per text line via `resolve-placeholders.ts`; unresolved placeholders render literally so misconfig is visible.
- **Footer**: configured footer lines (shared align), then a line with `Page X of Y` when enabled (`PageNumber.CURRENT` / `PageNumber.TOTAL_PAGES`), then `Generated by {user} on {date}` when enabled. Date format follows app locale; user = exporting user's display name.
- **Core metadata**: docx `title` = document title, `creator` = exporting user, `created` = export timestamp.
- Same shell is used by both builders.

### 6. Work item document

Inputs: issue detail data already in the web store (title, sequence id, type, state, priority, assignees, labels, dates, project), `description_html`, linked items (issue relations), `IssueActivity` rows (existing `/history/` endpoint via `IssueActivityStore`).

1. **Title block**: `WORK-123 — {title}` as Heading 1; work item type as a subtitle line.
2. **Properties table**: two-column grid, empty fields omitted — State, Priority, Type, Assignees, Labels, Start date, Target date, Project, Created (by + at), Last updated (by + at).
3. **Description**: `html-to-docx.ts` output; omitted when empty.
4. **Linked work items**: table — ID, Title, Relation (blocks / blocked by / relates to), State.
5. **Appendix — History (milestones)**: chronological table — Timestamp, Actor, Event, From → To. Filter in `milestones.ts` on `IssueActivity.field`:
   - Included: `created` (verb `created`), `state`, `priority`, `assignees`, `target_date`, `start_date`, `archived_at`.
   - Excluded: everything else (`labels`, `description`, `comment`, `vote`, `reaction`, `link`, `attachment`, `estimate_point`, `cycle`, `module`, etc.).
   - Value resolution: `old_value`/`new_value` for `state`/`assignees` are UUIDs — resolve to state names / member display names via existing store lookups (same resolution the activity feed uses); fallback to raw value when unresolved. Actors use `actor_detail.display_name`. Order: chronological ascending, oldest first (audit-document convention, differs from the in-app feed).
6. **Footer** per shell.

Raw history rows come from the same SWR-backed activity store used by the issue detail UI; if history is not yet loaded the builder fetches it first (reuse `IssueActivityStore.fetch` semantics).

### 7. KB article document

Inputs: page detail (`name`, `description_html`, owner, created/updated + by, labels, project), sub-page tree, versions endpoint (`/pages/:pageId/versions/` via `ProjectPageVersionService`).

1. **Title**: article name as Heading 1.
2. **Properties block**: Owner, Created (by + at), Last updated (by + at), Labels, Project association, Version count (length from versions endpoint; omit section if the call fails).
3. **Body**: `html-to-docx.ts` output.
4. **Sub-pages**: recursive numbered sections (`1`, `1.1`, `1.1.1`), each titled with the sub-page name and its body. Archived sub-pages excluded. Fetch children via the same pages listing the navigation pane uses (parent-scoped list), depth-limited recursively with a safety cap (e.g. 10 levels) and a note when truncated.
5. **Footer** per shell.

### 8. HTML → DOCX conversion

`html-to-docx.ts` walks `description_html` with `DOMParser`:

| HTML                    | DOCX mapping                                                                                                                                 |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `h1`–`h6`               | Heading 1–6                                                                                                                                  |
| `p`                     | Paragraph                                                                                                                                    |
| `strong`/`em`/`u`/`s`   | Bold / italics / underline / strike runs                                                                                                     |
| `ul`/`ol`/`li` (nested) | Bullet / numbered lists with levels                                                                                                          |
| `blockquote`            | Indented quote paragraph                                                                                                                     |
| `pre`/`code`            | Monospace block (light shading for `pre`)                                                                                                    |
| `a`                     | External hyperlink, styled                                                                                                                   |
| `img`                   | Fetch → `Uint8Array` → `ImageRun`, max width 600px, aspect ratio preserved from natural size; fetch failure → alt-text placeholder paragraph |
| `table`                 | DOCX table; first row header-styled when `<th>` present                                                                                      |
| `hr` / `br`             | Border rule / line break                                                                                                                     |
| Mention spans (TipTap)  | Plain `@Display Name` text                                                                                                                   |
| Unknown tags            | Unwrapped to plain text (children preserved); `script`/`style` dropped                                                                       |

Image fetching reuses the existing base64/data-URL approach from `use-parse-editor-content.ts` where possible; cross-origin asset fetch failures degrade to the alt-text placeholder and never abort the export.

### 9. Entry points & UX

- **Work item**: add `"export"` to `useWorkItemDetailMenuItems` (`apps/web/core/components/issues/issue-layouts/quick-action-dropdowns/helper.tsx:293-307`) and an export IconButton in the issue detail header actions (`issue-detail-quick-actions.tsx`). Clicking opens a small confirmation modal (new `document-export-modal`) with a single "Include history appendix" checkbox (default checked) and an "Export" button. Download starts immediately; button shows a spinner while generating.
- **KB article**: extend `ExportPageModal` format list with "DOCX (Formal document)" mapped to `build-page-doc.ts`; existing options (PDF/Markdown/page size) untouched.
- No export-history records, no background jobs, no new permissions beyond normal read access to the item/article.

### 10. Error handling

- Settings fetch failure → export proceeds with defaults; no blocking dialog.
- No settings row → defaults (page numbers on, generated-by on, no logo/footer lines).
- Image fetch failure → alt-text placeholder; export continues.
- Empty description/body → section omitted, properties still render.
- Missing logo asset (deleted file) → header without logo.
- Version endpoint failure on KB export → version line omitted.
- Any builder exception → user-facing toast with retry; no partial/corrupt file download.

### 11. Testing & verification

- **Web unit tests** (Vitest, `apps/web`): placeholder resolution (including unresolved literals), milestone filter table (one case per included/excluded `field`), UUID→name resolution fallbacks, HTML→DOCX element mapping (headings, nested lists, table/th, image failure path, mentions, unknown tags), filename slugging, sub-page recursion cap.
- **Rust tests**: GET returns defaults without row; PATCH admin-only (member denied); PATCH validation (align enum, line caps); asset binding sets `header_logo_asset_id` and soft-deletes previous; DB-backed integration tests follow the scratch-workspace serial-run convention (`-- --test-threads=1` where applicable).
- **Manual verification**: generate both documents from the running stack and open in Word and LibreOffice — header/footer on every page, "Page X of Y" correct, embedded images render, tables intact, properties/DOCX metadata correct.
- **Deploy**: web is static prod (`pnpm --filter=web build` + `systemctl --user restart plane-web-prod.service` per AGENTS.md); api-rs rebuild via `docker-compose-local.yml` with the detached-LTO build procedure.

## Risks / open questions

- **Cross-origin images**: S3-served description images may fail CORS on fetch. Mitigation: reuse the existing base64 utility's transport; if still failing, fall back to the asset proxy or placeholder. Verify during implementation with a real S3-configured stack.
- **SVG workspace logos** cannot embed in DOCX; client-side rasterization may be blocked by canvas tainting. Acceptable degradation: omit logo.
- **Very large histories**: milestones-only keeps the appendix small; no pagination needed. Confirmed acceptable.
- **`docx` library API drift**: pin via catalog version at implementation time; page-number field support (`PageNumber.TOTAL_PAGES`) must be verified in Word and LibreOffice early.
- **Timezone**: timestamps render in the exporting user's timezone (consistent with the app); document does not carry a timezone label. Accepted.
