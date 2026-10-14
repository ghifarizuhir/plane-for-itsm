# DOCX Export — Embed Images (Work Items, KB Articles, Header Logo) — Design

Date: 2026-10-14
Status: Approved in brainstorming (2026-10-14); awaiting written-spec review.
Scope: web only (`apps/web/core/components/document-export/`, two export modals, `logo.ts`, one i18n key across locales). No `apps/api-rs`, no `apps/api`, no `packages/utils` changes.
Related: `2026-10-12-document-export-docx-design.md` (original DOCX export design; section 8 and its "cross-origin images" risk), `html-to-docx.ts`, `build-work-item-doc.ts`, `build-page-doc.ts`, `work-item-export-modal.tsx`, `pages/modals/export-page-modal.tsx`, `logo.ts`.

## Background (analysis findings, 2026-10-14)

DOCX export currently drops every in-editor image. Root causes, verified against the running stack and database:

1. **Wrong tag handled.** The Plane editor stores images as `<image-component src="<asset-uuid>" width="299px" height="535px">` (Tiptap custom-image `renderHTML`). `html-to-docx.ts` only walks `<img>` elements. DB check: 54 issues contain `image-component`; 0 contain `<img>`. Page bodies use the same format.
2. **`src` is an asset id, not a URL.** `getBase64Image` (`defaultImageFetcher`) calls `new URL(src)` and throws for UUIDs. Asset ids must be resolved with `getEditorAssetSrc({ assetId, projectId, workspaceSlug })` → `/api/assets/v2/workspaces/{slug}/projects/{projectId}/{assetId}/`.
3. **Asset GET requires a session.** `project_get` (`apps/api-rs/.../asset.rs:1802`) gates on auth, then 302-redirects to a presigned MinIO URL served by the api-rs `s3proxy` route (`main.rs:1165`). The web app (`dashboard.terraline.space`) and API (`api.terraline.space`) are cross-origin; `getBase64Image`'s plain `fetch` sends no cookies. CORS on the API is already correct (`allow_credentials(true)`, allowed origin includes the dashboard origin).
4. **WebP unsupported by `docx@9.7.1`.** `ImageRun` accepts `jpg | png | gif | bmp` only; `image/webp` is an allowed upload type (`ALLOWED_FILE_TYPES`, editor `ACCEPTED_IMAGE_MIME_TYPES`).
5. **Custom header logo is fetched from a dead URL.** `DocumentExportSettingsService` receives `header_logo_url = /api/assets/v2/static/{asset_id}/`, but `StaticFileAssetEndpoint` (Django v2.py:491-533) and its Rust port (`asset.rs:2065`) only allow `USER_AVATAR | USER_COVER | WORKSPACE_LOGO | PROJECT_COVER`. `curl` against a real `DOC_HEADER_LOGO` asset returns **400**. The working route for that asset is `ws_get` (`/api/assets/v2/workspaces/{slug}/{asset_id}/`, any entity type, workspace member, auth required) using the `header_logo_asset_id` field already present in `TDocumentExportSettings`.
6. `buildWorkItemDocument`/`buildPageDocument` already expose an optional `imageFetcher` input that no caller fills; `resolveDocumentLogo` uses the same credential-less path as (2)-(3).

## Locked decisions (brainstorming 2026-10-14)

1. **Scope:** work item DOCX + KB article (page) DOCX + header logo. All three use one shared fetcher.
2. **Unsupported formats:** WebP/AVIF/BMP and any other browser-decodable format not supported by `docx` are re-encoded to PNG client-side via canvas. SVG stays unsupported → placeholder (no rasterization attempt).
3. **Failure placeholder:** `alt` text when present, otherwise a new i18n string `document_export.doc.image_unavailable` ("Image unavailable"); same gray italic bracket style as today. The key is added to all 20 locale files via the repo translate workflow.
4. **Image size:** honor the editor's stored `width`/`height` px attributes (only `Npx` values), capped at `MAX_IMAGE_WIDTH` (600). Percent or malformed values fall back to natural bitmap size, capped. Aspect ratio always comes from the bitmap, never from stored attributes.
5. **Approach A (client-side helper fetcher).** Keep the client-side pipeline; do not touch the shared `getBase64Image` utility or the PDF export path (approach B rejected: shared-util blast radius; approach C rejected: server-side renderer duplication).
6. **No backend changes.** `ws_get` + `s3proxy` + CORS already provide everything needed.

## Non-goals (this phase)

- PDF export image fixes (its own asset-id gap remains; separate work).
- Server-side DOCX rendering, async export jobs.
- Fixing the workspace-settings logo preview UI (it uses the broken static URL too; same helper could be reused later, but it is not part of this change).
- Image alignment/float layout from the editor's `alignment` attribute (images render as centered standalone paragraphs as today).
- Retry/queueing for flaky asset fetches; a failed fetch degrades to the placeholder.
- Attachment (`ISSUE_ATTACHMENT`) images; only editor description images are in scope.

## Design

### 1. New module: `document-export/image-fetcher.ts`

```ts
type TDocumentImageContext = { workspaceSlug: string; projectId?: string };

// Pure, unit-testable.
export const resolveDocumentImageSrc = (src: string, ctx: TDocumentImageContext): string | null;
// http(s) → as-is; "/..." → getFileURL(path); bare id → getEditorAssetSrc({ assetId, projectId, workspaceSlug })

export const loadDocumentImage = async (src: string, ctx: TDocumentImageContext): Promise<TDocxImage | null>;

export const createDocumentImageFetcher = (ctx: TDocumentImageContext): TImageFetcher;
// memoized per src (Map<string, Promise<TDocxImage | null>>), so repeated images fetch once per export
```

`loadDocumentImage` pipeline:

1. Resolve src via `resolveDocumentImageSrc`; resolution failure → `null`.
2. `fetch(url, { credentials })` where `credentials: "include"` when the resolved URL starts with `API_BASE_URL` (`@plane/constants`), else default — external hosts must not receive credentialed CORS requests.
3. `createImageBitmap(blob)` for dimensions and decodability; failure → `null`.
4. Type mapping from `blob.type`:
   - `image/jpeg`/`image/jpg` → pass bytes as `"jpg"`; `image/png` → `"png"`; `image/gif` → `"gif"` (original bytes, animation preserved).
   - `image/svg+xml` → `null`.
   - anything else (`webp`, `avif`, `bmp`, empty/unknown) → canvas re-encode to PNG at natural size (`createImageBitmap` → canvas `drawImage` → `toBlob("image/png")` → `Uint8Array`); if the canvas path is unavailable or fails → `null`.
5. Return `{ data: Uint8Array, width, height, type }` with bitmap natural dimensions.

`TDocxImage["type"]` stays `"png" | "jpg" | "gif"` (BMP is re-encoded, not passed through).

### 2. `html-to-docx.ts` — walker support for `image-component`

- Everywhere the walker currently processes `img` — standalone block, inside `<p>` (both the `querySelectorAll` and the child-skip in run collection), and inside `<li>` — also process `image-component`.
- New helper `imageElementHints(element)`: parses `width`/`height` attributes matching `/^(\d+)px$/`; returns pixel hints or none.
- `imageParagraph` gains the hints: target width = `min(pxWidthHint, maxImageWidth)` when the hint exists, else `min(naturalWidth, maxImageWidth)`; height keeps bitmap aspect ratio. No hint → current behavior.
- New option `imageUnavailableText?: string` in `THtmlToDocxOptions` (default `"Image unavailable"` in code for tests; callers always pass the i18n string). Placeholder text = non-empty `alt` else `imageUnavailableText`, rendered in the existing gray italic bracket paragraph.
- Run collection must not emit the element's placeholder text inline when an `image-component` sits inside a paragraph (same skip treatment as `img`).

### 3. Callers

| Caller                                             | Change                                                                                                                                                                                                                                                                                               |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `work-item-export-modal.tsx`                       | Build `imageFetcher = createDocumentImageFetcher({ workspaceSlug: slug, projectId: issue.project_id })`; pass `imageFetcher` to `buildWorkItemDocument` and `imageUnavailableText: t("document_export.doc.image_unavailable")` through `htmlToDocxBlocks` (plumbed via builder input).               |
| `pages/modals/export-page-modal.tsx` (DOCX branch) | Same fetcher with route `projectId`; pass fetcher + placeholder into `buildPageDocument` (covers body and all sub-page bodies). Header logo also uses the same fetcher.                                                                                                                              |
| `logo.ts`                                          | `resolveDocumentLogo(settings, workspace, fetcher)`; custom mode resolves via `header_logo_asset_id` (workspace asset URL through the fetcher) instead of the dead `header_logo_url`; workspace mode passes `workspace.logo_url`; both fall back to `null` on failure (header renders without logo). |
| `build-work-item-doc.ts` / `build-page-doc.ts`     | Plumb `imageUnavailableText` from their inputs into `htmlToDocxBlocks`.                                                                                                                                                                                                                              |

### 4. i18n

- New key `document_export.doc.image_unavailable`; en value `"Image unavailable"`.
- Propagate to all 20 locales under `packages/i18n/src/locales/*/document-export.json` via the translate skill workflow (do-not-translate rules, placeholder preservation).

### 5. Testing

Vitest (`@vitest-environment happy-dom`), in `apps/web/core/components/document-export/`:

- `image-fetcher.test.ts`: `resolveDocumentImageSrc` — bare UUID with/without `projectId`, `"/"` path, `http(s)` URL, empty/whitespace → null. `createDocumentImageFetcher` with stubbed `fetch`/`createImageBitmap`: credentials included for `API_BASE_URL` origins and not for external; memoization (one fetch per src); pass-through types; canvas re-encode path → png; fetch failure / SVG → null.
- `html-to-docx.test.ts` additions: `image-component` block embeds when fetcher succeeds and receives the raw asset-id src; `image-component` inside `<p>` and `<li>`; `Npx` size hint caps at 600 and scales height by aspect; `%` hint falls back to natural; failure with `alt` keeps `alt`; failure without `alt` uses `imageUnavailableText`; a `<p>` containing both text and an image-component renders the text runs and the image without leaking the raw asset id as inline text.
- `build-work-item-doc.test.ts` / `build-page-doc.test.ts`: fetcher injected → blob non-empty (existing pattern; no binary assertions).

### 6. Manual verification (acceptance)

1. `pnpm --filter=web build` + `systemctl --user restart plane-web-prod.service` (AGENTS.md).
2. Work item with 1+ description images (incl. a WebP and a resized image) → export DOCX; open in Word/LibreOffice: images at editor size, correct aspect, WebP visible.
3. KB article with body images + sub-page images → export DOCX: all sections have images.
4. Custom header logo configured → logo appears in the header of both documents.
5. Soft-delete one asset in DB → export shows the "Image unavailable" placeholder and still completes.
6. External pasted `<img>` (CORS-permitting host) still embeds; non-permitting host degrades to placeholder.

### 7. Risks

- **Cross-origin credentials**: relies on the API `CORS_ALLOWED_ORIGINS` + `SameSite` cookies between dashboard and api hosts (already proven by the app's own axios traffic). LAN origins (`http://<LAN-IP>:3000`) are not in `CORS_ALLOWED_ORIGINS`; exporting from a LAN origin will fail image fetches (pre-existing limitation, same as login).
- **Canvas availability**: re-encode requires a real browser; happy-dom tests stub the canvas path with mocks. Degradation is the placeholder, never an export abort.
- **Large images**: re-encoding happens at natural size; DOCX size grows with image count (accepted).
- **Static logo URL**: `header_logo_url` remains dead in the settings API/static endpoint; this design routes around it without backend changes. If the settings preview is fixed later, the same helper applies.

## Rollout

Web-only change; no migrations, no API deploy. Ship by rebuilding the static prod bundle and restarting `plane-web-prod.service`; verify per section 6.
