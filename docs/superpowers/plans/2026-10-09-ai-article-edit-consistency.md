# AI Article Edit Consistency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make AI `update_article` edits persist and appear everywhere: keep the page title inside the rewritten Yjs binary, push the same content into the live collaborative document, and purge stale client caches.

**Architecture:** The REST description write stays the durable source of truth. After it succeeds, the web app publishes an `apply_document` admin command (Redis → live server); the live server replaces the live Yjs document's `default` and `title` fragments with delete+insert CRDT ops (validated: no duplication, no leftovers), so every connected editor updates live and any later debounced store persists identical content (self-healing). On editor open, a revision check purges the IndexedDB cache when the server page changed since the last sync, so cold caches cannot union stale content.

**Tech Stack:** React/MobX (apps/web, vitest node env), Hocuspocus/Yjs + Tiptap/y-prosemirror (@plane/editor, apps/live vitest), Axum + Redis (apps/api-rs).

---

## Background (root causes this plan fixes)

Evidence gathered 2026-10-09 from API logs, live-server logs, DB binary decoding, and local reproductions:

1. **Title wiped:** `convertHTMLDocumentToAllFormats({variant:"document"})` calls `getBinaryDataFromDocumentEditorHTMLString(html)` without a title (`packages/editor/src/helpers/yjs-utils.ts:230`). Reproduced: resulting binary has an empty `title` fragment. Consequence: live `TitleSyncExtension` re-migrates the title from `page.name` and title observers can persist doubled titles (observed: doubled `name` at 06:36:37, repaired 06:48:56).
2. **Live doc never receives AI content:** the AI write is a REST write to `description_*` while the live server holds its own Yjs doc and stores it on a 10s debounce (`apps/live/src/hocuspocus.ts:50`). Observed: live `Store` events at 06:36:42 and 06:42:57 immediately reverted both AI writes; final binary contains none of the AI text.
3. **Regenerated binaries cannot be merged:** the AI binary is a full regeneration from HTML (new Yjs items). Merging it (client cache or live doc) unions items → duplication. Verified by union test (2 lists) and by the delete+applyUpdate replacement test (clean single copy).
4. **Stale client caches:** the web editor caches each page's Yjs doc in IndexedDB (`packages/editor/src/hooks/use-yjs-setup.ts:274`). After an external REST write, the cache is stale and merges back on next open.

The replacement primitive (`fragment.delete(0, len)` + `applyUpdate(newBinary)`) was validated end-to-end in Node with the real editor helpers: old body/title fully replaced, no duplicates.

## File Structure

- Modify `packages/editor/src/helpers/yjs-utils.ts` — accept optional `title` in `convertHTMLDocumentToAllFormats`.
- Modify `apps/web/core/store/ai-assistant.store.ts` — pass `title: page.name`; call `applyCollabDocument` after the description write.
- Modify `apps/web/core/services/page/project-page.service.ts` — new `applyCollabDocument` POST.
- Modify `apps/web/core/store/ai-assistant.store.test.ts` — title + collab-apply tests.
- Create `apps/web/core/lib/page-cache.ts` — cache revision helpers.
- Create `apps/web/core/lib/page-cache.test.ts` — tests for the helpers.
- Modify `apps/web/core/components/pages/editor/editor-body.tsx` — gate editor mount on cache check.
- Modify `apps/live/src/types/admin-commands.ts` — `APPLY_DOCUMENT` command + guard.
- Create `apps/live/src/lib/replace-document.ts` — CRDT replacement helper.
- Create `apps/live/tests/replace-document.test.ts` — helper tests.
- Create `apps/live/src/extensions/apply-document.ts` — admin command handler.
- Modify `apps/live/src/extensions/index.ts` — register the handler.
- Modify `apps/api-rs/crates/api/src/routes/page.rs` — `build_apply_document_payload` + `collab_apply` handler + unit test.
- Modify `apps/api-rs/crates/api/src/main.rs` — route registration.
- Modify `docs/superpowers/smoke/2026-10-06-galileo-tools-smoke-test.md` — new scenarios.

**Build note:** `@plane/editor` is consumed from `dist/` by both web and live. After editing the editor package, run `pnpm --filter=@plane/editor build` before any test/build that depends on it.

---

### Task 1: Preserve the page title in AI-generated document binaries

**Files:**

- Modify: `packages/editor/src/helpers/yjs-utils.ts:199-246`
- Modify: `apps/web/core/store/ai-assistant.store.ts:522-533`
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Write the failing test**

Add this test inside `describe("article proposals", ...)` in `apps/web/core/store/ai-assistant.store.test.ts`, after the existing `"appends to an article and rewrites the document payload"` test. Add these imports at the top of the file:

```ts
import { convertBase64StringToBinaryData, getAllDocumentFormatsFromDocumentEditorBinaryData } from "@plane/editor";
```

```ts
it("keeps the page title in the rewritten document binary", async () => {
  const services = makeServices({
    pages: {
      fetchById: vi.fn(async () => ({ id: "pg1", name: "Runbook", description_html: "<p>Step 1</p>" })),
    },
  });
  const { store } = await openAgent(services, updateArticleMetadata);
  await store.confirmProposal("srv-assistant", "k13", {
    kind: "update_article",
    projectId: "p1",
    pageId: "pg1",
    action: "append",
    descriptionHtml: "<p>Step 3</p>",
  });
  const [, , , payload] = services.pages.updateDescription.mock.calls[0];
  const formats = getAllDocumentFormatsFromDocumentEditorBinaryData(
    convertBase64StringToBinaryData(payload.description_binary),
    true
  );
  expect(formats.titleHTML).toContain("Runbook");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=@plane/editor build && cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts -t "keeps the page title"`
Expected: FAIL (titleHTML is empty because the binary has no `title` fragment).

- [ ] **Step 3: Implement title-aware conversion**

In `packages/editor/src/helpers/yjs-utils.ts`, change the args type and the `document` branch:

```ts
type TConvertHTMLDocumentToAllFormatsArgs = {
  document_html: string;
  variant: "rich" | "document";
  title?: string;
};
```

```ts
export const convertHTMLDocumentToAllFormats = (args: TConvertHTMLDocumentToAllFormatsArgs): TDocumentPayload => {
  const { document_html, variant, title } = args;
```

```ts
  } else if (variant === "document") {
    // Convert HTML to binary format for document editor
    const contentBinary = getBinaryDataFromDocumentEditorHTMLString(document_html, title);
```

In `apps/web/core/store/ai-assistant.store.ts`, replace the conversion call inside the `update_article` branch:

```ts
const document = convertHTMLDocumentToAllFormats({
  document_html: mergedHtml,
  variant: "document",
  title: page?.name ?? undefined,
});
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=@plane/editor build && cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts`
Expected: PASS (all article tests, including the pre-existing append test).

- [ ] **Step 5: Commit**

```bash
git add packages/editor/src/helpers/yjs-utils.ts apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "fix(editor): keep page title in AI document updates"
```

---

### Task 2: Live-server CRDT replacement helper

**Files:**

- Create: `apps/live/src/lib/replace-document.ts`
- Test: `apps/live/tests/replace-document.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/live/tests/replace-document.test.ts`:

```ts
import { getBinaryDataFromDocumentEditorHTMLString } from "@plane/editor";
import { describe, expect, it } from "vitest";
import * as Y from "yjs";
import { replaceDocumentContent } from "@/lib/replace-document";

const makeDoc = (html: string, title: string) => {
  const doc = new Y.Doc();
  Y.applyUpdate(doc, getBinaryDataFromDocumentEditorHTMLString(html, title));
  return doc;
};

describe("replaceDocumentContent", () => {
  it("replaces body and title without leaving old content behind", () => {
    const doc = makeDoc("<p>Old body</p>", "Old title");
    replaceDocumentContent(doc, "<p>New body</p>", "New title");
    const body = doc.getXmlFragment("default").toString();
    const title = doc.getXmlFragment("title").toString();
    expect(body).toContain("New body");
    expect(body).not.toContain("Old body");
    expect(title).toContain("New title");
    expect(title).not.toContain("Old title");
  });

  it("keeps exactly one body child (no union duplicates)", () => {
    const doc = makeDoc("<p>Old body</p>", "Title");
    replaceDocumentContent(doc, "<p>New body</p>", "Title");
    expect(doc.getXmlFragment("default").length).toBe(1);
    expect(doc.getXmlFragment("title").length).toBe(1);
  });

  it("is a no-op shape-wise when applied twice", () => {
    const doc = makeDoc("<p>Old body</p>", "Old title");
    replaceDocumentContent(doc, "<p>New body</p>", "New title");
    replaceDocumentContent(doc, "<p>New body</p>", "New title");
    expect(doc.getXmlFragment("default").toString()).toContain("New body");
    expect(doc.getXmlFragment("default").length).toBe(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/live && pnpm vitest run tests/replace-document.test.ts`
Expected: FAIL with "Cannot find module '@/lib/replace-document'".

- [ ] **Step 3: Write the helper**

Create `apps/live/src/lib/replace-document.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { getBinaryDataFromDocumentEditorHTMLString } from "@plane/editor";
import * as Y from "yjs";

/**
 * Replace the full content of a live collaborative document (body + title)
 * with new HTML. Deletes every existing child first, then applies the new
 * content as CRDT insert operations, so connected clients receive a clean
 * delete+insert instead of a union of old and new items.
 */
export const replaceDocumentContent = (doc: Y.Doc, descriptionHtml: string, title: string): void => {
  const defaultFragment = doc.getXmlFragment("default");
  const titleFragment = doc.getXmlFragment("title");

  defaultFragment.delete(0, defaultFragment.length);
  titleFragment.delete(0, titleFragment.length);

  const source = new Y.Doc();
  Y.applyUpdate(source, getBinaryDataFromDocumentEditorHTMLString(descriptionHtml, title));
  Y.applyUpdate(doc, Y.encodeStateAsUpdate(source));
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/live && pnpm vitest run tests/replace-document.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/live/src/lib/replace-document.ts apps/live/tests/replace-document.test.ts
git commit -m "feat(live): add collaborative document replacement helper"
```

---

### Task 3: Live-server `apply_document` admin command

**Files:**

- Modify: `apps/live/src/types/admin-commands.ts:61-117`
- Create: `apps/live/src/extensions/apply-document.ts`
- Modify: `apps/live/src/extensions/index.ts`
- Test: `apps/live/tests/replace-document.test.ts` (guard test)

- [ ] **Step 1: Write the failing guard test**

Append to `apps/live/tests/replace-document.test.ts`:

```ts
import { AdminCommand, isApplyDocumentCommand } from "@/types/admin-commands";

describe("isApplyDocumentCommand", () => {
  it("accepts an apply_document payload", () => {
    expect(
      isApplyDocumentCommand({
        command: AdminCommand.APPLY_DOCUMENT,
        docId: "page-1",
        descriptionHtml: "<p>Hi</p>",
        name: "Title",
        originServer: "api-rs",
      })
    ).toBe(true);
  });

  it("rejects other commands", () => {
    expect(
      isApplyDocumentCommand({
        command: AdminCommand.FORCE_CLOSE,
        docId: "page-1",
        reason: "admin_request",
        code: 4000,
        originServer: "api-rs",
      } as never)
    ).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/live && pnpm vitest run tests/replace-document.test.ts`
Expected: FAIL with "isApplyDocumentCommand is not a function" / missing export.

- [ ] **Step 3: Add the command type**

In `apps/live/src/types/admin-commands.ts`, extend the enum:

```ts
export enum AdminCommand {
  FORCE_CLOSE = "force_close",
  HEALTH_CHECK = "health_check",
  RESTART_DOCUMENT = "restart_document",
  APPLY_DOCUMENT = "apply_document",
}
```

Add after `ForceCloseCommandData`:

```ts
/**
 * Apply external content to a live document (AI article edits).
 */
export interface ApplyDocumentCommandData {
  command: AdminCommand.APPLY_DOCUMENT;
  docId: string;
  descriptionHtml: string;
  name: string;
  originServer: string;
  timestamp?: string;
}
```

Extend the union and add the guard:

```ts
export type AdminCommandData = ForceCloseCommandData | HealthCheckCommandData | ApplyDocumentCommandData;
```

```ts
/**
 * Type guard to check if data is an ApplyDocumentCommandData
 */
export function isApplyDocumentCommand(data: AdminCommandData): data is ApplyDocumentCommandData {
  return data.command === AdminCommand.APPLY_DOCUMENT;
}
```

- [ ] **Step 4: Implement the handler extension**

Create `apps/live/src/extensions/apply-document.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Extension, onConfigurePayload } from "@hocuspocus/server";
import { logger } from "@plane/logger";
import { Redis } from "@/extensions/redis";
import { replaceDocumentContent } from "@/lib/replace-document";
import { AdminCommand, isApplyDocumentCommand } from "@/types/admin-commands";
import type { ApplyDocumentCommandData } from "@/types/admin-commands";

/**
 * Applies external document updates (e.g. AI article edits) to live
 * collaborative documents. If the document is not in memory there is nothing
 * to update: the REST write already persisted the content and the next load
 * will fetch it.
 */
export class ApplyDocumentHandler implements Extension {
  name = "ApplyDocumentHandler";
  priority = 998;

  async onConfigure({ instance }: onConfigurePayload) {
    const redisExt = instance.configuration.extensions.find((ext) => ext instanceof Redis);

    if (!redisExt) {
      logger.warn("[APPLY_DOCUMENT_HANDLER] Redis extension not found");
      return;
    }

    redisExt.onAdminCommand<ApplyDocumentCommandData>(AdminCommand.APPLY_DOCUMENT, (data) => {
      if (!isApplyDocumentCommand(data)) {
        logger.error("[APPLY_DOCUMENT_HANDLER] Received invalid apply_document command");
        return;
      }

      const document = instance.documents.get(data.docId);
      if (!document) {
        logger.info(`[APPLY_DOCUMENT_HANDLER] No live document for ${data.docId}, skipping`);
        return;
      }

      try {
        replaceDocumentContent(document, data.descriptionHtml, data.name);
        logger.info(`[APPLY_DOCUMENT_HANDLER] Applied external update to ${data.docId}`);
      } catch (error) {
        logger.error("[APPLY_DOCUMENT_HANDLER] Failed to apply document update", error);
      }
    });

    logger.info("[APPLY_DOCUMENT_HANDLER] Registered with Redis extension");
  }
}
```

Register it in `apps/live/src/extensions/index.ts`:

```ts
import { ApplyDocumentHandler } from "./apply-document";
import { Database } from "./database";
import { ForceCloseHandler } from "./force-close-handler";
import { Logger } from "./logger";
import { Redis } from "./redis";
import { TitleSyncExtension } from "./title-sync";

export const getExtensions = () => [
  new Logger(),
  new Database(),
  new Redis(),
  new TitleSyncExtension(),
  new ApplyDocumentHandler(), // Must be after Redis to receive broadcasts
  new ForceCloseHandler(), // Must be after Redis to receive broadcasts
];
```

- [ ] **Step 5: Run tests and typecheck**

Run: `cd apps/live && pnpm vitest run && pnpm check:types`
Expected: PASS (guard tests + helper tests), no type errors.

- [ ] **Step 6: Commit**

```bash
git add apps/live/src/types/admin-commands.ts apps/live/src/extensions/apply-document.ts apps/live/src/extensions/index.ts apps/live/tests/replace-document.test.ts
git commit -m "feat(live): apply external document updates via admin command"
```

---

### Task 4: API endpoint publishing `apply_document`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/page.rs` (payload builder + handler + unit test)
- Modify: `apps/api-rs/crates/api/src/main.rs:1206-1210`

- [ ] **Step 1: Write the failing unit test**

Find the existing `#[cfg(test)] mod tests` block in `apps/api-rs/crates/api/src/routes/page.rs` (search for `mod tests`; it contains an assertion `starts_with("Invalid binary data: ")`). Add:

```rust
    #[test]
    fn apply_document_payload_shape() {
        let page_id = uuid::Uuid::parse_str("78706c41-3994-4ef3-bf2e-4bdf92e10af8").unwrap();
        let payload = build_apply_document_payload(page_id, "<p>Hi</p>", "Runbook");
        assert_eq!(payload["command"], "apply_document");
        assert_eq!(payload["docId"], "78706c41-3994-4ef3-bf2e-4bdf92e10af8");
        assert_eq!(payload["descriptionHtml"], "<p>Hi</p>");
        assert_eq!(payload["name"], "Runbook");
        assert_eq!(payload["originServer"], "api-rs");
        assert!(payload["timestamp"].as_str().unwrap().contains('T'));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/api-rs && cargo test -p api --lib apply_document_payload_shape`
Expected: FAIL with "cannot find function `build_apply_document_payload`".

- [ ] **Step 3: Implement the payload builder and handler**

In `apps/api-rs/crates/api/src/routes/page.rs`, add near the top-level helpers (e.g. after `parse_desc_binary_value`):

```rust
/// Payload for the live-server `apply_document` admin command. The live server
/// replaces the in-memory Yjs document content with this HTML + title so
/// connected editors receive the AI edit as CRDT operations.
pub fn build_apply_document_payload(page_id: uuid::Uuid, description_html: &str, name: &str) -> Value {
    json!({
        "command": "apply_document",
        "docId": page_id.to_string(),
        "descriptionHtml": description_html,
        "name": name,
        "originServer": "api-rs",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
}
```

Add the handler next to `desc_patch` (same module, reuses `project_in_workspace`, `require_page_perm`, `missing`, `deny_detail`):

```rust
// ============================================================================
// E4h — collab-apply: push an external description write into the live
// collaborative document (AI article edits).
// ============================================================================

/// POST `/pages/:page_id/collab-apply/` — publishes an `apply_document`
/// admin command on Redis (`hocuspocus:admin`) so the live server can update
/// the in-memory Yjs document. No-op on the live side when the document is
/// not loaded; the REST description write remains the source of truth.
pub async fn collab_apply(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, page_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if require_page_perm(&st.pool, auth.0, &slug, pid, page_id, "PATCH")
        .await?
        .is_err()
    {
        return Ok(deny_detail());
    }
    let description_html = body
        .get("description_html")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let name = body.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if description_html.is_empty() || name.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "description_html and name are required"})),
        ));
    }
    let mut conn = st.redis_client().await?;
    let payload = build_apply_document_payload(page_id, &description_html, &name);
    let _: i64 = redis::cmd("PUBLISH")
        .arg("hocuspocus:admin")
        .arg(payload.to_string())
        .query_async(&mut conn)
        .await?;
    Ok((StatusCode::OK, Json(json!({"message": "ok"}))))
}
```

- [ ] **Step 4: Register the route**

In `apps/api-rs/crates/api/src/main.rs`, after the `.../pages/:page_id/description/` route (around line 1206):

```rust
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/collab-apply/",
            post(routes::page::collab_apply),
        )
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/api-rs && cargo test -p api --lib apply_document_payload_shape && cargo build -p api`
Expected: test PASS; build finishes (warnings acceptable).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/page.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api): publish apply_document command for live article updates"
```

---

### Task 5: Web — call `collab-apply` after the description write

**Files:**

- Modify: `apps/web/core/services/page/project-page.service.ts:164-175`
- Modify: `apps/web/core/store/ai-assistant.store.ts` (type + `update_article` branch)
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Write the failing test**

Update the existing test `"appends to an article and rewrites the document payload"` (assertions added) and add a new test:

```ts
it("publishes the merged content to the live collaborative document", async () => {
  const services = makeServices({
    pages: {
      fetchById: vi.fn(async () => ({ id: "pg1", name: "Runbook", description_html: "<p>Step 1</p>" })),
    },
  });
  const { store } = await openAgent(services, updateArticleMetadata);
  await store.confirmProposal("srv-assistant", "k13", {
    kind: "update_article",
    projectId: "p1",
    pageId: "pg1",
    action: "append",
    descriptionHtml: "<p>Step 3</p>",
  });
  expect(services.pages.applyCollabDocument).toHaveBeenCalledWith("acme", "p1", "pg1", {
    description_html: expect.stringContaining("Step 3"),
    name: "Runbook",
  });
});

it("still applies the decision when the collab apply call fails", async () => {
  const services = makeServices({
    pages: {
      fetchById: vi.fn(async () => ({ id: "pg1", name: "Runbook", description_html: "<p>Step 1</p>" })),
      applyCollabDocument: vi.fn(async () => {
        throw new Error("live server down");
      }),
    },
  });
  const { store } = await openAgent(services, updateArticleMetadata);
  await store.confirmProposal("srv-assistant", "k13", {
    kind: "update_article",
    projectId: "p1",
    pageId: "pg1",
    action: "append",
    descriptionHtml: "<p>Step 3</p>",
  });
  expect(store.messages[0].proposalDecisions?.k13).toEqual({
    kind: "update_article",
    decision: "applied",
  });
});
```

Also update `makeServices` pages block to include the new mock:

```ts
  pages: {
    create: vi.fn(async (_slug: string, projectId: string) => ({ id: "pg1", project_id: projectId })),
    update: vi.fn(async () => ({ id: "pg1" })),
    fetchById: vi.fn(async () => ({ id: "pg1", name: "Runbook", description_html: "<p>Step 1</p>" })),
    updateDescription: vi.fn(async () => ({ message: "Updated successfully" })),
    applyCollabDocument: vi.fn(async () => undefined),
    ...overrides.pages,
  },
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts -t "collaborative"`
Expected: FAIL (`applyCollabDocument` is not a function / not called).

- [ ] **Step 3: Implement the service method**

In `apps/web/core/services/page/project-page.service.ts`, after `updateDescription`:

```ts
  async applyCollabDocument(
    workspaceSlug: string,
    projectId: string,
    pageId: string,
    data: { description_html: string; name: string }
  ): Promise<void> {
    return this.post(`/api/workspaces/${workspaceSlug}/projects/${projectId}/pages/${pageId}/collab-apply/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
```

- [ ] **Step 4: Wire the store**

In `apps/web/core/store/ai-assistant.store.ts`, extend the service pick:

```ts
type TPagesService = Pick<
  ProjectPageService,
  "create" | "update" | "fetchById" | "updateDescription" | "applyCollabDocument"
>;
```

In the `update_article` branch, after `await this.pagesService.updateDescription(...)`:

```ts
await this.pagesService
  .applyCollabDocument(slug, payload.projectId, payload.pageId, {
    description_html: mergedHtml,
    name: page?.name ?? "",
  })
  .catch(() => undefined);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts`
Expected: PASS (including the two new tests and the pre-existing suite).

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/services/page/project-page.service.ts apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): push AI article edits into the live document"
```

---

### Task 6: Web — purge stale page caches on editor open

**Files:**

- Create: `apps/web/core/lib/page-cache.ts`
- Create: `apps/web/core/lib/page-cache.test.ts`
- Modify: `apps/web/core/components/pages/editor/editor-body.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/page-cache.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearPageCache, ensureFreshPageCache } from "./page-cache";

const deleteDatabase = vi.fn();

const makeStorage = () => {
  const data = new Map<string, string>();
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => {
      data.set(key, value);
    },
    removeItem: (key: string) => {
      data.delete(key);
    },
  };
};

describe("page-cache", () => {
  beforeEach(() => {
    deleteDatabase.mockReset();
    deleteDatabase.mockImplementation(() => {
      const request: { onsuccess?: () => void; onerror?: () => void; onblocked?: () => void } = {};
      setTimeout(() => request.onsuccess?.(), 0);
      return request;
    });
    vi.stubGlobal("indexedDB", { deleteDatabase });
    vi.stubGlobal("localStorage", makeStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("deletes the cache when the revision changed", async () => {
    await ensureFreshPageCache("pg1", "rev-2");
    expect(deleteDatabase).toHaveBeenCalledWith("pg1");
    expect(localStorage.getItem("page-sync-rev:pg1")).toBe("rev-2");
  });

  it("keeps the cache when the revision matches", async () => {
    localStorage.setItem("page-sync-rev:pg1", "rev-1");
    await ensureFreshPageCache("pg1", "rev-1");
    expect(deleteDatabase).not.toHaveBeenCalled();
  });

  it("no-ops safely without indexedDB", async () => {
    vi.stubGlobal("indexedDB", undefined);
    await expect(clearPageCache("pg1")).resolves.toBeUndefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && pnpm vitest run core/lib/page-cache.test.ts`
Expected: FAIL with "Cannot find module './page-cache'".

- [ ] **Step 3: Implement the helper**

Create `apps/web/core/lib/page-cache.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

const REV_KEY_PREFIX = "page-sync-rev:";

/**
 * Deletes the IndexedDB cache of a page document. Best-effort: resolves on
 * success, error, or blocked (another tab still holds the database open).
 */
export const clearPageCache = async (pageId: string): Promise<void> => {
  if (typeof indexedDB === "undefined") return;
  await new Promise<void>((resolve) => {
    const request = indexedDB.deleteDatabase(pageId);
    request.onsuccess = request.onerror = request.onblocked = () => resolve();
  });
};

/**
 * Ensures the local collaborative cache is not stale relative to the server
 * page revision. When the revision differs (page changed elsewhere, e.g. an
 * AI edit), the cache is deleted so the next provider sync loads the server
 * document fresh instead of union-merging stale content.
 */
export const ensureFreshPageCache = async (pageId: string, revision: string | null | undefined): Promise<void> => {
  if (!pageId || !revision) return;
  try {
    const key = `${REV_KEY_PREFIX}${pageId}`;
    if (localStorage.getItem(key) === revision) return;
    await clearPageCache(pageId);
    localStorage.setItem(key, revision);
  } catch {
    // Best-effort: a failed cache purge must never block the editor.
  }
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && pnpm vitest run core/lib/page-cache.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Gate the editor mount on the cache check**

In `apps/web/core/components/pages/editor/editor-body.tsx`:

Add to the imports:

```ts
import { useState } from "react";
```

(extend the existing `import { useCallback, useEffect, useMemo, useRef } from "react";` line to include `useState`), and:

```ts
import { ensureFreshPageCache } from "@/lib/page-cache";
```

Inside the component (after the existing `useRealtimePageEvents` call, around line 144), add:

```ts
const [cacheChecked, setCacheChecked] = useState(false);

useEffect(() => {
  let cancelled = false;
  void ensureFreshPageCache(pageId, page.updated_at).finally(() => {
    if (!cancelled) setCacheChecked(true);
  });
  return () => {
    cancelled = true;
  };
}, [pageId, page.updated_at]);
```

In the render, gate the collaborative editor (around line 275):

```tsx
{
  cacheChecked && (
    <CollaborativeDocumentEditorWithRef
      editable={isContentEditable}
      id={pageId}
      fileHandler={config.fileHandler}
      handleEditorReady={handleEditorReady}
      ref={editorForwardRef}
      titleRef={titleEditorRef}
      containerClassName="h-full p-0 pb-64"
      displayConfig={displayConfig}
      getEditorMetaData={getEditorMetaData}
      mentionHandler={{
        searchCallback: async (query) => {
          const res = await fetchMentions(query);
          if (!res) throw new Error("Failed in fetching mentions");
          return res;
        },
        // oxlint-disable-next-line no-shadow
        renderComponent: (props) => <EditorMentionsRoot {...props} />,
        getMentionedEntityDetails: (id: string) => ({ display_name: getUserDetails(id)?.display_name ?? "" }),
      }}
      updatePageProperties={updatePageProperties}
      realtimeConfig={realtimeConfig}
      serverHandler={serverHandler}
      user={userConfig}
      disabledExtensions={documentEditorExtensions.disabled}
      flaggedExtensions={documentEditorExtensions.flagged}
      aiHandler={{
        menu: getAIMenu,
      }}
      onAssetChange={updateAssetsList}
      extendedEditorProps={extendedEditorProps}
      isFetchingFallbackBinary={isFetchingFallbackBinary}
    />
  );
}
```

- [ ] **Step 6: Run checks**

Run: `cd apps/web && pnpm check:types && pnpm check:lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/lib/page-cache.ts apps/web/core/lib/page-cache.test.ts apps/web/core/components/pages/editor/editor-body.tsx
git commit -m "fix(web): purge stale collaborative cache before opening page editor"
```

---

### Task 7: Smoke scenarios, build, and end-to-end verification

**Files:**

- Modify: `docs/superpowers/smoke/2026-10-06-galileo-tools-smoke-test.md`

- [ ] **Step 1: Add smoke scenarios**

In `docs/superpowers/smoke/2026-10-06-galileo-tools-smoke-test.md`, append to section S5 (after S5.5):

```md
### S5.6 Edit saat editor terbuka — harus live update

1. Buka artikel di editor (biarkan terbuka).
2. Di AI assistant, minta: `tambahkan satu baris ke artikel <judul>: dicek ulang 2026-10-09`
3. Klik Confirm.
4. **Verifikasi:** baris baru muncul di editor TANPA reload (dalam ±2 detik); judul tidak berubah; tidak ada konten ganda.
5. Tunggu 15 detik (debounce store), refresh → baris tetap ada.

### S5.7 Edit saat editor tertutup — harus persist tanpa duplikat

1. Tutup semua tab artikel (editor tidak terbuka di browser mana pun).
2. Minta edit lain via AI assistant, Confirm.
3. Buka lagi artikelnya.
4. **Verifikasi:** perubahan ada, TIDAK ada konten duplikat (list tidak dobel).

### S5.8 Judul aman setelah edit AI

1. Setelah S5.6/S5.7, cek judul artikel.
2. **Verifikasi:** judul tetap satu (tidak "X X"), dan kolom `name` di DB sama.

### S5.9 Dua tab terbuka — keduanya update

1. Buka artikel yang sama di 2 tab.
2. Confirm edit AI di salah satu tab.
3. **Verifikasi:** kedua tab menampilkan perubahan tanpa reload.
```

And in S7, after S7.6:

```md
### S7.7 Store basi tidak boleh menimpa AI

1. Dengan editor artikel terbuka, lakukan edit kecil manual (ketik 1 huruf lalu hapus) agar store terjadwal.
2. Segera (dalam <10 detik) Confirm edit AI pada artikel itu.
3. Tunggu 30 detik, refresh artikel.
4. **Verifikasi:** perubahan AI tetap ada (tidak tertimpa store), judul utuh.
```

- [ ] **Step 2: Full test suite**

Run: `pnpm --filter=@plane/editor build && cd apps/web && pnpm test && pnpm check:types && pnpm check:lint`
Expected: all PASS.

Run: `cd apps/live && pnpm test && pnpm check:types && pnpm build`
Expected: all PASS.

Run: `cd apps/api-rs && cargo test -p api --lib && cargo build -p api -p worker`
Expected: tests PASS; build finishes.

- [ ] **Step 3: Deploy and run the smoke scenarios**

Follow `AGENTS.md`:

```bash
# 1. live server (uses the rebuilt @plane/editor dist)
pnpm --filter=live build && systemctl --user restart plane-live.service && curl http://localhost:3100/live/health/

# 2. backend (LTO build can take 10+ minutes; run detached and poll)
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
# poll: curl http://localhost:8000/health until 200

# 3. web prod
pnpm --filter=web build && systemctl --user restart plane-web-prod.service
```

Then execute S5.6–S5.9 and S7.7 on `dashboard.terraline.space`. Record results in the smoke rekap table.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/smoke/2026-10-06-galileo-tools-smoke-test.md
git commit -m "docs(smoke): add AI article edit consistency scenarios"
```

---

## Verification checklist (end of plan)

- [ ] `pnpm --filter=@plane/editor build` clean.
- [ ] `apps/web`: `pnpm test` (new page-cache + ai-assistant tests), `check:types`, `check:lint` pass.
- [ ] `apps/live`: `pnpm test` (replace-document + guard tests), `check:types`, `build` pass.
- [ ] `apps/api-rs`: `cargo test -p api --lib` (payload test), `cargo build -p api -p worker` pass.
- [ ] Smoke S5.6–S5.9, S7.7 LULUS on the tunnel deployment.
- [ ] No regression: S5.1–S5.5 still LULUS.

## Known limitations (accepted for v1)

- Multi-tab same-browser: `indexedDB.deleteDatabase` can be blocked while another tab holds the database open; the purge resolves on `onblocked` and the other tab is live-synced anyway. Rare stale merge possible in this edge case.
- An AI append computes the merged HTML from the REST copy, which can lag an actively-typing editor by the last unsaved keystroke; the CRDT merge keeps both edits, worst case ordering at the document tail.
