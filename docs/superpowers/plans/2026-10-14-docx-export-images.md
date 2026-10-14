# DOCX Export Images Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed description images (and the header logo) into DOCX exports for work items and KB pages, resolving editor asset ids through the authenticated API.

**Architecture:** A new `image-fetcher.ts` module resolves `image-component` `src` values (bare asset UUIDs, `/`-paths, external URLs) to absolute URLs, fetches them with credentials, and normalizes them to a `TDocxImage` for the existing `html-to-docx.ts` walker. The walker learns the `image-component` tag, honors stored px widths, and uses a localized placeholder when an image cannot be embedded. Both export modals and the logo resolver are wired to the fetcher.

**Tech Stack:** TypeScript, React, Vitest (happy-dom), `docx@9.7.1`, `@plane/utils` (`getEditorAssetSrc`, `getFileURL`), `@plane/constants` (`API_BASE_URL`).

**Spec:** `docs/superpowers/specs/2026-10-14-docx-export-images-design.md`

---

## File Structure

| File                                                                                                                                       | Responsibility                                                                                                             |
| ------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `apps/web/core/components/document-export/image-fetcher.ts` (create)                                                                       | Resolve src → URL; fetch with credentials; bitmap decode; webp/avif/bmp → PNG re-encode; memoized `TImageFetcher` factory. |
| `apps/web/core/components/document-export/html-to-docx.ts` (modify)                                                                        | Walk `image-component` like `img`; px width hints; `imageUnavailableText` option.                                          |
| `apps/web/core/components/document-export/logo.ts` (modify)                                                                                | `resolveDocumentLogo` takes a `TImageFetcher`; custom mode uses `header_logo_asset_id`.                                    |
| `apps/web/core/components/document-export/build-work-item-doc.ts` (modify)                                                                 | Plumb `imageUnavailableText` into `htmlToDocxBlocks`.                                                                      |
| `apps/web/core/components/document-export/build-page-doc.ts` (modify)                                                                      | Same, for body + sub-pages.                                                                                                |
| `apps/web/core/components/document-export/work-item-export-modal.tsx` (modify)                                                             | Build fetchers; pass fetcher + label; add `header_logo_asset_id` to `DEFAULT_SETTINGS`.                                    |
| `apps/web/core/components/pages/modals/export-page-modal.tsx` (modify)                                                                     | Same for the DOCX branch.                                                                                                  |
| `packages/i18n/src/locales/*/document-export.json` (modify ×20)                                                                            | New key `document_export.doc.image_unavailable`.                                                                           |
| Tests: `image-fetcher.test.ts` (create), `logo.test.ts` (create), `html-to-docx.test.ts` (modify), `build-work-item-doc.test.ts` (modify). |                                                                                                                            |

---

### Task 1: `resolveDocumentImageSrc` — pure src resolution

**Files:**

- Create: `apps/web/core/components/document-export/image-fetcher.ts`
- Test: `apps/web/core/components/document-export/image-fetcher.test.ts`

- [ ] **Step 1: Write the failing test**

Create `image-fetcher.test.ts`:

```ts
// @vitest-environment happy-dom
import { describe, expect, it, vi } from "vitest";

vi.mock("@plane/constants", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@plane/constants")>();
  return { ...actual, API_BASE_URL: "https://api.test" };
});

import { resolveDocumentImageSrc } from "./image-fetcher";

const context = { workspaceSlug: "acme", projectId: "project-1" };

describe("resolveDocumentImageSrc", () => {
  it("keeps absolute http(s) urls and data urls", () => {
    expect(resolveDocumentImageSrc("https://cdn.example.com/a.png", context)).toBe("https://cdn.example.com/a.png");
    expect(resolveDocumentImageSrc("data:image/png;base64,AAAA", context)).toBe("data:image/png;base64,AAAA");
  });

  it("resolves project-scoped asset ids", () => {
    expect(resolveDocumentImageSrc("4c8008d6-72ba-458a-912b-9965c77f1e39", context)).toBe(
      "https://api.test/api/assets/v2/workspaces/acme/projects/project-1/4c8008d6-72ba-458a-912b-9965c77f1e39/"
    );
  });

  it("resolves workspace-scoped asset ids when no project is given", () => {
    expect(resolveDocumentImageSrc("4c8008d6-72ba-458a-912b-9965c77f1e39", { workspaceSlug: "acme" })).toBe(
      "https://api.test/api/assets/v2/workspaces/acme/4c8008d6-72ba-458a-912b-9965c77f1e39/"
    );
  });

  it("resolves root-relative paths against the API base url", () => {
    expect(resolveDocumentImageSrc("/apin/v2/static/abc/", context)).toBe("https://api.test/apin/v2/static/abc/");
  });

  it("returns null for empty input", () => {
    expect(resolveDocumentImageSrc("   ", context)).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/components/document-export/image-fetcher.test.ts`
Expected: FAIL — `Cannot find module './image-fetcher'`.

- [ ] **Step 3: Write minimal implementation**

Create `image-fetcher.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { API_BASE_URL } from "@plane/constants";
import { getEditorAssetSrc, getFileURL } from "@plane/utils";
// local imports
import type { TDocxImage, TImageFetcher } from "./html-to-docx";

export type TDocumentImageContext = {
  workspaceSlug: string;
  projectId?: string;
};

export const resolveDocumentImageSrc = (src: string, context: TDocumentImageContext): string | null => {
  const trimmed = src?.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("http") || trimmed.startsWith("data:")) return trimmed;
  if (trimmed.startsWith("/")) return getFileURL(trimmed) ?? null;
  return (
    getEditorAssetSrc({
      assetId: trimmed,
      projectId: context.projectId,
      workspaceSlug: context.workspaceSlug,
    }) ?? null
  );
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/components/document-export/image-fetcher.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/image-fetcher.ts apps/web/core/components/document-export/image-fetcher.test.ts
git commit -m "feat(web): resolve document image sources for docx export"
```

---

### Task 2: `loadDocumentImage` + memoized `createDocumentImageFetcher`

**Files:**

- Modify: `apps/web/core/components/document-export/image-fetcher.ts`
- Test: `apps/web/core/components/document-export/image-fetcher.test.ts`

- [ ] **Step 1: Write the failing tests**

Append to `image-fetcher.test.ts` (keep the existing `vi.mock` at the top; import the new symbols in the same import line):

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
// ... existing mock ...
import { createDocumentImageFetcher, loadDocumentImage, resolveDocumentImageSrc } from "./image-fetcher";

const imageResponse = (bytes: number[], type: string) => ({
  ok: true,
  blob: async () => new Blob([new Uint8Array(bytes)], { type }),
});

const stubCanvas = (outputBytes: number[]) => {
  Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
    configurable: true,
    value: () => ({ drawImage: () => {} }),
  });
  Object.defineProperty(HTMLCanvasElement.prototype, "toBlob", {
    configurable: true,
    value: (callback: BlobCallback) => callback(new Blob([new Uint8Array(outputBytes)], { type: "image/png" })),
  });
};

describe("loadDocumentImage", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => ({ width: 20, height: 10 }))
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("passes png bytes through with bitmap dimensions", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => imageResponse([1, 2], "image/png"))
    );
    const image = await loadDocumentImage("asset-1", context);
    expect(image?.type).toBe("png");
    expect(Array.from(image?.data ?? [])).toEqual([1, 2]);
    expect(image?.width).toBe(20);
    expect(image?.height).toBe(10);
  });

  it("re-encodes webp to png via canvas", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => imageResponse([7], "image/webp"))
    );
    stubCanvas([9]);
    const image = await loadDocumentImage("asset-1", context);
    expect(image?.type).toBe("png");
    expect(Array.from(image?.data ?? [])).toEqual([9]);
  });

  it("returns null for svg", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => imageResponse([1], "image/svg+xml"))
    );
    expect(await loadDocumentImage("asset-1", context)).toBeNull();
  });

  it("returns null when the fetch fails or decoding fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, blob: async () => new Blob() }))
    );
    expect(await loadDocumentImage("asset-1", context)).toBeNull();
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => Promise.reject(new Error("decode")))
    );
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => imageResponse([1], "image/png"))
    );
    expect(await loadDocumentImage("asset-1", context)).toBeNull();
  });

  it("includes credentials for the API origin only", async () => {
    const fetchMock = vi.fn(async () => imageResponse([1], "image/png"));
    vi.stubGlobal("fetch", fetchMock);
    await loadDocumentImage("asset-1", context);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.test/api/assets/v2/workspaces/acme/projects/project-1/asset-1/",
      {
        credentials: "include",
      }
    );
    await loadDocumentImage("https://cdn.example.com/a.png", context);
    expect(fetchMock).toHaveBeenLastCalledWith("https://cdn.example.com/a.png", { credentials: "same-origin" });
  });
});

describe("createDocumentImageFetcher", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => ({ width: 20, height: 10 }))
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("fetches each src once per fetcher instance", async () => {
    const fetchMock = vi.fn(async () => imageResponse([1], "image/png"));
    vi.stubGlobal("fetch", fetchMock);
    const fetcher = createDocumentImageFetcher(context);
    await Promise.all([fetcher("asset-1"), fetcher("asset-1")]);
    await fetcher("asset-1");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/components/document-export/image-fetcher.test.ts`
Expected: FAIL — `loadDocumentImage` / `createDocumentImageFetcher` are not exported.

- [ ] **Step 3: Implement loader + factory**

Append to `image-fetcher.ts`:

```ts
const DOCX_PASSTHROUGH_TYPES: Record<string, TDocxImage["type"]> = {
  "image/jpeg": "jpg",
  "image/jpg": "jpg",
  "image/png": "png",
  "image/gif": "gif",
};

const toUint8Array = async (blob: Blob): Promise<Uint8Array> => new Uint8Array(await blob.arrayBuffer());

const reencodeAsPng = async (bitmap: ImageBitmap): Promise<Uint8Array | null> => {
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) return null;
    context.drawImage(bitmap, 0, 0);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    return blob ? toUint8Array(blob) : null;
  } catch {
    return null;
  }
};

export const loadDocumentImage = async (src: string, context: TDocumentImageContext): Promise<TDocxImage | null> => {
  const url = resolveDocumentImageSrc(src, context);
  if (!url) return null;
  try {
    const credentials = API_BASE_URL !== "" && url.startsWith(API_BASE_URL) ? "include" : "same-origin";
    const response = await fetch(url, { credentials });
    if (!response.ok) return null;
    const blob = await response.blob();
    const mime = (blob.type || "").toLowerCase();
    if (mime === "image/svg+xml") return null;
    const bitmap = await createImageBitmap(blob);
    const passthroughType = DOCX_PASSTHROUGH_TYPES[mime];
    const data = passthroughType ? await toUint8Array(blob) : await reencodeAsPng(bitmap);
    if (!data) return null;
    return { data, width: bitmap.width, height: bitmap.height, type: passthroughType ?? "png" };
  } catch {
    return null;
  }
};

export const createDocumentImageFetcher = (context: TDocumentImageContext): TImageFetcher => {
  const cache = new Map<string, Promise<TDocxImage | null>>();
  return (src) => {
    const cached = cache.get(src);
    if (cached) return cached;
    const promise = loadDocumentImage(src, context);
    cache.set(src, promise);
    return promise;
  };
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/components/document-export/image-fetcher.test.ts`
Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/image-fetcher.ts apps/web/core/components/document-export/image-fetcher.test.ts
git commit -m "feat(web): fetch docx images with credentials and png re-encoding"
```

---

### Task 3: Walker support for `image-component`, px width hints, placeholder text

**Files:**

- Modify: `apps/web/core/components/document-export/html-to-docx.ts`
- Test: `apps/web/core/components/document-export/html-to-docx.test.ts`

- [ ] **Step 1: Write the failing tests**

Append to `html-to-docx.test.ts` (inside the existing `describe`):

```ts
it("embeds image-component blocks and passes the raw src to the fetcher", async () => {
  const requested: string[] = [];
  const blocks = await htmlToDocxBlocks(
    '<image-component src="asset-1" width="299px" height="535px"></image-component>',
    {
      imageFetcher: async (src) => {
        requested.push(src);
        return { data: new Uint8Array([1, 2, 3]), width: 800, height: 400, type: "png" };
      },
    }
  );
  expect(requested).toEqual(["asset-1"]);
  expect(JSON.stringify(blocks)).toContain("image");
});

it("scales image-component to the stored px width using the bitmap aspect ratio", async () => {
  const fetcher: TImageFetcher = async () => ({ data: new Uint8Array([1]), width: 800, height: 400, type: "png" });
  const json = JSON.stringify(
    await htmlToDocxBlocks('<image-component src="a" width="299px" height="535px"></image-component>', {
      imageFetcher: fetcher,
    })
  );
  expect(json).toContain(String(299 * 9525));
  expect(json).toContain(String(150 * 9525));
});

it("caps the stored px width at the max image width", async () => {
  const fetcher: TImageFetcher = async () => ({ data: new Uint8Array([1]), width: 1000, height: 500, type: "png" });
  const json = JSON.stringify(
    await htmlToDocxBlocks('<image-component src="a" width="900px"></image-component>', { imageFetcher: fetcher })
  );
  expect(json).toContain(String(600 * 9525));
  expect(json).toContain(String(300 * 9525));
});

it("falls back to natural size for percent widths", async () => {
  const fetcher: TImageFetcher = async () => ({ data: new Uint8Array([1]), width: 800, height: 400, type: "png" });
  const json = JSON.stringify(
    await htmlToDocxBlocks('<image-component src="a" width="35%" height="auto"></image-component>', {
      imageFetcher: fetcher,
    })
  );
  expect(json).toContain(String(600 * 9525));
  expect(json).toContain(String(300 * 9525));
});

it("embeds image-component inside paragraphs and list items", async () => {
  const fetcher: TImageFetcher = async () => ({ data: new Uint8Array([1]), width: 100, height: 100, type: "png" });
  const paragraphBlocks = await htmlToDocxBlocks('<p>before <image-component src="a"></image-component> after</p>', {
    imageFetcher: fetcher,
  });
  const listBlocksResult = await htmlToDocxBlocks(
    '<ul><li>item <image-component src="b"></image-component></li></ul>',
    {
      imageFetcher: fetcher,
    }
  );
  expect(paragraphBlocks).toHaveLength(2);
  expect(listBlocksResult.length).toBeGreaterThanOrEqual(2);
});

it("uses alt text for failed image-component fetches", async () => {
  const json = JSON.stringify(
    await htmlToDocxBlocks('<image-component src="a" alt="diagram"></image-component>', {
      imageFetcher: async () => null,
    })
  );
  expect(json).toContain("diagram");
});

it("uses the configured unavailable text when alt is missing", async () => {
  const json = JSON.stringify(
    await htmlToDocxBlocks('<image-component src="a"></image-component>', {
      imageFetcher: async () => null,
      imageUnavailableText: "Bild nicht verfügbar",
    })
  );
  expect(json).toContain("Bild nicht verfügbar");
});

it("does not leak the asset id as inline text in a mixed paragraph", async () => {
  const json = JSON.stringify(
    await htmlToDocxBlocks('<p>before <image-component src="asset-secret"></image-component> after</p>', {
      imageFetcher: async () => null,
    })
  );
  expect(json).toContain("before");
  expect(json).toContain("after");
  expect(json).not.toContain("asset-secret");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/components/document-export/html-to-docx.test.ts`
Expected: FAIL — `image-component` blocks produce no image, placeholder text missing, `imageUnavailableText` ignored.

- [ ] **Step 3: Implement walker changes**

In `html-to-docx.ts`:

3a. Extend the options type and add the default (after `MAX_IMAGE_WIDTH`):

```ts
export const DEFAULT_IMAGE_UNAVAILABLE_TEXT = "Image unavailable";

export type THtmlToDocxOptions = {
  imageFetcher?: TImageFetcher;
  maxImageWidth?: number;
  imageUnavailableText?: string;
};
```

3b. Replace `scaledSize` with a hint-aware version and add element helpers (near the current `scaledSize`):

```ts
const scaledSize = (image: TDocxImage, maxWidth: number, widthHint?: number) => {
  const targetWidth = Math.min(widthHint ?? image.width, maxWidth);
  if (targetWidth === image.width) return { width: image.width, height: image.height };
  const ratio = targetWidth / image.width;
  return { width: targetWidth, height: Math.round(image.height * ratio) };
};

const isImageElement = (element: Element) => ["img", "image-component"].includes(element.tagName.toLowerCase());

const pxWidthHint = (element: Element): number | undefined => {
  const match = element.getAttribute("width")?.match(/^(\d+)px$/);
  return match ? Number(match[1]) : undefined;
};
```

3c. In `collectRuns`, extend the skip set:

```ts
if (tag === "script" || tag === "style" || isImageElement(element)) return [];
```

3d. Replace `TWalkContext` and `imageParagraph`:

```ts
type TWalkContext = { imageFetcher: TImageFetcher; maxImageWidth: number; imageUnavailableText: string };

const imageParagraph = async (element: Element, ctx: TWalkContext): Promise<Paragraph> => {
  const image = await ctx.imageFetcher(element.getAttribute("src") ?? "");
  if (!image) {
    const alt = element.getAttribute("alt") ?? "";
    return new Paragraph({
      children: [new TextRun({ text: `[${alt || ctx.imageUnavailableText}]`, italics: true, color: "808080" })],
      spacing: { after: 120 },
    });
  }
  return new Paragraph({
    children: [
      new ImageRun({
        type: image.type,
        data: image.data,
        transformation: scaledSize(image, ctx.maxImageWidth, pxWidthHint(element)),
      }),
    ],
    spacing: { before: 120, after: 120 },
  });
};
```

3e. In `listBlocks`, update the image loop:

```ts
for (const image of Array.from(item.querySelectorAll("img, image-component"))) {
  // oxlint-disable-next-line no-await-in-loop -- images must keep DOM order
  blocks.push(await imageParagraph(image, ctx));
}
```

3f. In `walkBlocks`, update the `p` branch:

```ts
if (tag === "p") {
  const images = Array.from(element.querySelectorAll("img, image-component"));
  for (const image of images) {
    // oxlint-disable-next-line no-await-in-loop -- blocks must keep DOM order
    blocks.push(await imageParagraph(image, ctx));
  }
  const runs = images.length
    ? Array.from(element.childNodes).flatMap((child) =>
        isElement(child) && isImageElement(child) ? [] : collectRuns(child, {})
      )
    : collectRuns(element, {});
  if (runs.length) blocks.push(new Paragraph({ children: runs, spacing: { after: 120 } }));
  continue;
}
```

3g. Replace the standalone `img` branch:

```ts
if (isImageElement(element)) {
  // oxlint-disable-next-line no-await-in-loop -- blocks must keep DOM order
  blocks.push(await imageParagraph(element, ctx));
  continue;
}
```

3h. In `htmlToDocxBlocks`, wire the option:

```ts
export async function htmlToDocxBlocks(html: string, options: THtmlToDocxOptions = {}): Promise<TDocumentBlock[]> {
  const imageFetcher = options.imageFetcher ?? defaultImageFetcher;
  const maxImageWidth = options.maxImageWidth ?? MAX_IMAGE_WIDTH;
  const imageUnavailableText = options.imageUnavailableText ?? DEFAULT_IMAGE_UNAVAILABLE_TEXT;
  if (typeof DOMParser === "undefined") return [];
  const document = new DOMParser().parseFromString(html || "", "text/html");
  return walkBlocks(Array.from(document.body.childNodes), { imageFetcher, maxImageWidth, imageUnavailableText });
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=web test core/components/document-export/html-to-docx.test.ts`
Expected: PASS (15 tests, including the 7 pre-existing).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/html-to-docx.ts apps/web/core/components/document-export/html-to-docx.test.ts
git commit -m "feat(web): embed image-component blocks and size hints in docx export"
```

---

### Task 4: i18n key `document_export.doc.image_unavailable`

**Files:**

- Modify: `packages/i18n/src/locales/en/document-export.json`
- Modify: `packages/i18n/src/locales/{cs,de,es,fr,id,it,ja,ka-ge,ko,pl,pt-BR,ro,ru,sk,tr-TR,ua,vi-VN,zh-CN,zh-TW}/document-export.json`

Load the repo `translate` skill (`.claude/skills/translate/SKILL.md`) before editing locale files.

- [ ] **Step 1: Add the English key**

In `en/document-export.json` under `document_export.doc`, after `"no_value": "—",`:

```json
      "image_unavailable": "Image unavailable",
```

- [ ] **Step 2: Add the key to every other locale**

Insert `"image_unavailable": <value>,` at the same position (after `no_value`) in all 19 target locale files:

| Locale | Value                         |
| ------ | ----------------------------- |
| cs     | `Obrázek není k dispozici`    |
| de     | `Bild nicht verfügbar`        |
| es     | `Imagen no disponible`        |
| fr     | `Image indisponible`          |
| id     | `Gambar tidak tersedia`       |
| it     | `Immagine non disponibile`    |
| ja     | `画像を表示できません`        |
| ka-ge  | `სურათი მიუწვდომელია`         |
| ko     | `이미지를 표시할 수 없습니다` |
| pl     | `Nie można wyświetlić obrazu` |
| pt-BR  | `Imagem indisponível`         |
| ro     | `Imagine indisponibilă`       |
| ru     | `Изображение недоступно`      |
| sk     | `Obrázok nie je k dispozícii` |
| tr-TR  | `Görsel kullanılamıyor`       |
| ua     | `Зображення недоступне`       |
| vi-VN  | `Không thể hiển thị hình ảnh` |
| zh-CN  | `图片不可用`                  |
| zh-TW  | `圖片無法使用`                |

Verify per the translate skill's checklist: no placeholders/tags involved, sentence case, per-locale punctuation rules (no trailing period).

- [ ] **Step 3: Regenerate types and check sync**

Run: `pnpm --filter @plane/i18n run generate:types`
Run: `pnpm --filter @plane/i18n run sync:check`
Expected: `0 missing, 0 stale, 0 collisions`.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/*/document-export.json packages/i18n/src/types
git commit -m "feat(i18n): add document export image_unavailable string

machine_translated: ka-ge (native review recommended)"
```

---

### Task 5: `resolveDocumentLogo` via the shared fetcher

**Files:**

- Modify: `apps/web/core/components/document-export/logo.ts`
- Test: `apps/web/core/components/document-export/logo.test.ts` (create)

- [ ] **Step 1: Write the failing test**

Create `logo.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import type { TDocxImage } from "./html-to-docx";
import { resolveDocumentLogo } from "./logo";

const image: TDocxImage = { data: new Uint8Array([1]), width: 10, height: 10, type: "png" };

describe("resolveDocumentLogo", () => {
  it("uses the workspace logo url in workspace mode", async () => {
    const requested: string[] = [];
    const logo = await resolveDocumentLogo(
      { header_logo_mode: "workspace", header_logo_asset_id: null, header_logo_url: null },
      { logo_url: "/apin/v2/static/ws-logo/" },
      async (src) => {
        requested.push(src);
        return image;
      }
    );
    expect(requested).toEqual(["/apin/v2/static/ws-logo/"]);
    expect(logo).toEqual(image);
  });

  it("prefers the custom asset id over the static url", async () => {
    const requested: string[] = [];
    await resolveDocumentLogo(
      {
        header_logo_mode: "custom",
        header_logo_asset_id: "asset-9",
        header_logo_url: "/api/assets/v2/static/asset-9/",
      },
      null,
      async (src) => {
        requested.push(src);
        return image;
      }
    );
    expect(requested).toEqual(["asset-9"]);
  });

  it("returns null when mode is none", async () => {
    const logo = await resolveDocumentLogo(
      { header_logo_mode: "none", header_logo_asset_id: null, header_logo_url: null },
      null,
      async () => image
    );
    expect(logo).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/components/document-export/logo.test.ts`
Expected: FAIL — `resolveDocumentLogo` requires a third argument at runtime (`imageFetcher` is undefined) / type error.

- [ ] **Step 3: Rewrite `logo.ts`**

Replace the entire file content with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TDocumentExportSettings } from "@plane/types";
import type { TDocxImage, TImageFetcher } from "./html-to-docx";

export async function resolveDocumentLogo(
  settings: Pick<TDocumentExportSettings, "header_logo_mode" | "header_logo_url" | "header_logo_asset_id">,
  workspace: { logo_url?: string | null } | null | undefined,
  imageFetcher: TImageFetcher
): Promise<TDocxImage | null> {
  if (settings.header_logo_mode === "workspace") {
    return workspace?.logo_url ? imageFetcher(workspace.logo_url) : null;
  }
  if (settings.header_logo_mode === "custom") {
    const src = settings.header_logo_asset_id ?? settings.header_logo_url;
    return src ? imageFetcher(src) : null;
  }
  return null;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/components/document-export/logo.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/logo.ts apps/web/core/components/document-export/logo.test.ts
git commit -m "feat(web): load docx header logo through the document image fetcher"
```

---

### Task 6: Plumb `imageUnavailableText` + `imageFetcher` through both builders

**Files:**

- Modify: `apps/web/core/components/document-export/build-work-item-doc.ts`
- Modify: `apps/web/core/components/document-export/build-page-doc.ts`
- Test: `apps/web/core/components/document-export/build-work-item-doc.test.ts`

- [ ] **Step 1: Write the failing test**

In `build-work-item-doc.test.ts`, change the vitest import to include `vi`, add a capture mock below the imports, and append the new test (inside the existing `describe`):

```ts
import { describe, expect, it, vi } from "vitest";
import { htmlToDocxBlocks } from "./html-to-docx";
import { buildWorkItemDocument, type TWorkItemDocInput } from "./build-work-item-doc";

vi.mock("./html-to-docx", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./html-to-docx")>();
  return { ...actual, htmlToDocxBlocks: vi.fn(actual.htmlToDocxBlocks) };
});
```

```ts
it("forwards description image options to the html walker", async () => {
  const requested: string[] = [];
  await buildWorkItemDocument({
    ...baseInput(),
    descriptionHtml: '<image-component src="asset-1" width="200px" height="100px"></image-component>',
    imageFetcher: async (src) => {
      requested.push(src);
      return { data: new Uint8Array([1, 2, 3]), width: 400, height: 200, type: "png" };
    },
    imageUnavailableText: "Gambar tidak tersedia",
  });
  expect(requested).toEqual(["asset-1"]);
  expect(vi.mocked(htmlToDocxBlocks)).toHaveBeenCalledWith(
    expect.stringContaining("image-component"),
    expect.objectContaining({ imageUnavailableText: "Gambar tidak tersedia" })
  );
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/components/document-export/build-work-item-doc.test.ts`
Expected: FAIL — `expect.objectContaining({ imageUnavailableText })` does not match because the builder does not forward the option yet.

- [ ] **Step 3: Implement plumbing**

In `build-work-item-doc.ts`, add to `TWorkItemDocInput` (after `imageFetcher?: TImageFetcher;`):

```ts
  imageUnavailableText?: string;
```

and update the description call:

```ts
children.push(
  ...(await htmlToDocxBlocks(input.descriptionHtml, {
    imageFetcher: input.imageFetcher,
    imageUnavailableText: input.imageUnavailableText,
  }))
);
```

In `build-page-doc.ts`, add to `TPageDocInput` (after `imageFetcher?: TImageFetcher;`):

```ts
  imageUnavailableText?: string;
```

and update both calls:

```ts
children.push(
  ...(await htmlToDocxBlocks(input.bodyHtml, {
    imageFetcher: input.imageFetcher,
    imageUnavailableText: input.imageUnavailableText,
  }))
);
```

```ts
children.push(
  ...(await htmlToDocxBlocks(subPage.bodyHtml, {
    imageFetcher: input.imageFetcher,
    imageUnavailableText: input.imageUnavailableText,
  }))
);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=web test core/components/document-export/build-work-item-doc.test.ts core/components/document-export/build-page-doc.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/build-work-item-doc.ts apps/web/core/components/document-export/build-page-doc.ts apps/web/core/components/document-export/build-work-item-doc.test.ts
git commit -m "feat(web): forward image options through docx builders"
```

---

### Task 7: Wire fetchers into both export modals

**Files:**

- Modify: `apps/web/core/components/document-export/work-item-export-modal.tsx`
- Modify: `apps/web/core/components/pages/modals/export-page-modal.tsx`

- [ ] **Step 1: Update the work item modal**

Add to `DEFAULT_SETTINGS` (after `header_logo_url: null,`):

```ts
  header_logo_asset_id: null,
```

Add the import next to the other `document-export` imports:

```ts
import { createDocumentImageFetcher } from "@/components/document-export/image-fetcher";
```

In `handleExport`, after the `slug` guard and before `buildWorkItemDocument`, add:

```ts
const documentImageFetcher = createDocumentImageFetcher({
  workspaceSlug: slug,
  projectId: issue.project_id,
});
const logoImageFetcher = createDocumentImageFetcher({ workspaceSlug: slug });
```

Replace the logo line:

```ts
const logo = await resolveDocumentLogo(resolvedSettings, currentWorkspace, logoImageFetcher);
```

In the `buildWorkItemDocument` input object, after `descriptionHtml: issue.description_html ?? "",` add:

```ts
        imageFetcher: documentImageFetcher,
        imageUnavailableText: t("document_export.doc.image_unavailable"),
```

- [ ] **Step 2: Update the page export modal (DOCX branch)**

Add the import:

```ts
import { createDocumentImageFetcher } from "@/components/document-export/image-fetcher";
```

Add `header_logo_asset_id: null,` to the inline `resolvedSettings` fallback object (after `header_logo_url: null,`).

Replace the logo line:

```ts
const logo = await resolveDocumentLogo(
  resolvedSettings,
  currentWorkspace,
  createDocumentImageFetcher({ workspaceSlug: slug })
);
```

In the `buildPageDocument` input object, after `subPages,` add:

```ts
        imageFetcher: createDocumentImageFetcher({ workspaceSlug: slug, projectId: projectIdString }),
        imageUnavailableText: t("document_export.doc.image_unavailable"),
```

- [ ] **Step 3: Typecheck and lint**

Run: `pnpm --filter=web check:types`
Expected: the only error is the pre-existing `document-export/page.tsx(17,28) error TS2307: Cannot find module './+types/page'` (if present on the base commit); no errors in document-export/ or export-page-modal.

Run: `pnpm --filter=web exec oxlint core/components/document-export core/components/pages/modals/export-page-modal.tsx`
Expected: no errors (pre-existing warnings in other files are out of scope).

- [ ] **Step 4: Run the full document-export test suite**

Run: `pnpm --filter=web test core/components/document-export`
Expected: PASS (all suites).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/work-item-export-modal.tsx apps/web/core/components/pages/modals/export-page-modal.tsx
git commit -m "feat(web): wire image fetchers into docx export modals"
```

---

### Task 8: Manual verification + prod deploy

**Files:** none (verification only)

- [ ] **Step 1: Build and restart prod web**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/
```

Expected: build succeeds; service active; `200`.

- [ ] **Step 2: Verify work item export with images**

Pick a work item known to include `image-component` in `description_html` (e.g. query: `SELECT id, name FROM issues WHERE description_html LIKE '%image-component%' LIMIT 1;`). Export it from the peek overview → open the DOCX in LibreOffice/Word. Expect: images present, sized like the editor, aspect ratio preserved, WebP images visible (re-encoded), no raw UUIDs anywhere.

- [ ] **Step 3: Verify KB page export and sub-pages**

Open a page with body images → Export → DOCX (Formal document). Expect: body and sub-page images embedded.

- [ ] **Step 4: Verify custom header logo**

With the existing `workspace_document_settings` row (`header_logo_mode='custom'`), export any document. Expect: logo in the header (it currently never appears).

- [ ] **Step 5: Verify placeholder degradation**

Soft-delete one image asset temporarily (`UPDATE file_assets SET deleted_at = now() WHERE id = '<asset>';` — restore after with `deleted_at = NULL`). Export again. Expect: `[Image unavailable]` (in the app locale) placeholder, export completes.

- [ ] **Step 6: Report**

Report pass/fail per step with the artifact files inspected. No commit for this task.

---

## Self-Review Notes

- Spec section 1 (fetcher) → Tasks 1-2. Spec section 2 (walker) → Task 3. Spec section 3 (callers) → Tasks 5-7. Spec section 4 (i18n) → Task 4. Spec section 5 (tests) → embedded in Tasks 1-6. Spec section 6 (manual) → Task 8.
- Deviation: the spec mentions parsing both `width` and `height` px attributes; the plan parses `width` only because aspect ratio always comes from the bitmap, making the height hint dead code (YAGNI). Behavior matches the spec's sizing rule.
- Type names are consistent across tasks: `TDocumentImageContext`, `resolveDocumentImageSrc`, `loadDocumentImage`, `createDocumentImageFetcher`, `DEFAULT_IMAGE_UNAVAILABLE_TEXT`, `imageUnavailableText`.
