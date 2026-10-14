// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@plane/constants", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@plane/constants")>();
  return { ...actual, API_BASE_URL: "https://api.test" };
});

import { createDocumentImageFetcher, loadDocumentImage, resolveDocumentImageSrc } from "./image-fetcher";

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

const imageResponse = (bytes: number[], type: string) => ({
  ok: true,
  blob: async () => new Blob([new Uint8Array(bytes)], { type }),
});

const stubCanvas = (outputBytes: number[]) => {
  // oxlint-disable-next-line no-extend-native -- canvas stub for the png re-encode path
  Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
    configurable: true,
    value: () => ({ drawImage: () => {} }),
  });
  // oxlint-disable-next-line no-extend-native -- canvas stub for the png re-encode path
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
      { credentials: "include" }
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
