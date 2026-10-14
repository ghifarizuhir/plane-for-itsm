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
