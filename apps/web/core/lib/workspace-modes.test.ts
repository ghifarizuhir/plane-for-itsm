import { describe, expect, it } from "vitest";
import { getWorkspaceMode } from "./workspace-modes";

describe("getWorkspaceMode", () => {
  const slug = "acme";

  it("detects the workspace home", () => {
    expect(getWorkspaceMode("/acme", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/", slug)).toBe("work");
  });

  it("detects Work routes", () => {
    expect(getWorkspaceMode("/acme/projects/abc/issues", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/notifications", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/projects/abc/settings", slug)).toBe("work");
  });

  it("detects Procedure", () => {
    expect(getWorkspaceMode("/acme/docs", slug)).toBe("procedure");
    expect(getWorkspaceMode("/acme/docs/itil4-overview", slug)).toBe("procedure");
  });

  it("detects Galileo", () => {
    expect(getWorkspaceMode("/acme/galileo", slug)).toBe("galileo");
    expect(getWorkspaceMode("/acme/galileo/7f4c", slug)).toBe("galileo");
  });

  it("detects Settings", () => {
    expect(getWorkspaceMode("/acme/settings", slug)).toBe("settings");
    expect(getWorkspaceMode("/acme/settings/members", slug)).toBe("settings");
  });

  it("does not match prefix collisions", () => {
    expect(getWorkspaceMode("/acme/settings-archive", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/docs-old", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/galileo-old", slug)).toBe("work");
  });

  it("returns other outside the workspace", () => {
    expect(getWorkspaceMode("/other/docs", slug)).toBe("other");
    expect(getWorkspaceMode("/", slug)).toBe("other");
  });

  it("returns other for an empty slug", () => {
    expect(getWorkspaceMode("/acme", "")).toBe("other");
  });
});
