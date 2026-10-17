import { describe, expect, it } from "vitest";
import {
  DOCS_SIDEBAR_GROUPS_KEY,
  parseGroupOverrides,
  resolveGroupOpen,
  serializeGroupOverrides,
  toggleGroupOverrides,
} from "./docs-sidebar-state";

describe("docs sidebar group state", () => {
  it("parses valid overrides", () => {
    expect(parseGroupOverrides('{"member":true,"project":false}')).toEqual({ member: true, project: false });
  });

  it("ignores unknown keys and non-boolean values", () => {
    expect(parseGroupOverrides('{"member":true,"project":"yes","unknown":true}')).toEqual({ member: true });
  });

  it("falls back to empty overrides for invalid input", () => {
    expect(parseGroupOverrides(null)).toEqual({});
    expect(parseGroupOverrides("")).toEqual({});
    expect(parseGroupOverrides("{not json")).toEqual({});
    expect(parseGroupOverrides("[1,2]")).toEqual({});
    expect(parseGroupOverrides('"text"')).toEqual({});
  });

  it("opens only the active group by default", () => {
    expect(resolveGroupOpen("project", "project", {})).toBe(true);
    expect(resolveGroupOpen("member", "project", {})).toBe(false);
  });

  it("lets manual overrides win over the active group", () => {
    expect(resolveGroupOpen("project", "project", { project: false })).toBe(false);
    expect(resolveGroupOpen("member", "project", { member: true })).toBe(true);
  });

  it("sets a single override immutably", () => {
    const before = { member: true };
    const after = toggleGroupOverrides(before, "project", true);
    expect(after).toEqual({ member: true, project: true });
    expect(before).toEqual({ member: true });
    expect(toggleGroupOverrides(after, "member", false)).toEqual({ member: false, project: true });
  });

  it("roundtrips through serialization", () => {
    const overrides = { project: false, member: true };
    expect(parseGroupOverrides(serializeGroupOverrides(overrides))).toEqual(overrides);
  });

  it("exposes a stable storage key", () => {
    expect(DOCS_SIDEBAR_GROUPS_KEY).toBe("plane-docs-sidebar-groups");
  });
});
