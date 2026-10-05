import { describe, expect, it } from "vitest";
import {
  isWorkItemCommand,
  matchAssignees,
  matchLabels,
  matchProject,
  matchState,
  parseWorkItemRef,
  textToDescriptionHtml,
  validateWorkItemProposal,
  workItemHref,
} from "./ai-work-items";

describe("isWorkItemCommand", () => {
  it("matches the /task prefix only", () => {
    expect(isWorkItemCommand("/task fix pump")).toBe(true);
    expect(isWorkItemCommand("  /TASK fix pump")).toBe(true);
    expect(isWorkItemCommand("/taskforce")).toBe(false);
    expect(isWorkItemCommand("buatkan task")).toBe(false);
  });
});

describe("validateWorkItemProposal", () => {
  it("requires a title", () => {
    expect(validateWorkItemProposal({ name: "  " })).toBe("Title is required.");
    expect(validateWorkItemProposal({ name: "Fix pump" })).toBeNull();
  });

  it("bounds lengths and refs", () => {
    expect(validateWorkItemProposal({ name: "x".repeat(256) })).toContain("255");
    expect(validateWorkItemProposal({ name: "ok", priority: "p0" })).toBe("Unknown priority.");
    expect(
      validateWorkItemProposal({ name: "ok", assignees: Array.from({ length: 11 }, (_, i) => `u${i}`) })
    ).toContain("10");
  });

  it("checks dates", () => {
    expect(validateWorkItemProposal({ name: "ok", start_date: "01-10-2026" })).toContain("YYYY-MM-DD");
    expect(validateWorkItemProposal({ name: "ok", start_date: "2026-10-05", target_date: "2026-10-01" })).toContain(
      "after"
    );
    expect(validateWorkItemProposal({ name: "ok", start_date: "2026-10-01", target_date: "2026-10-05" })).toBeNull();
  });
});

describe("textToDescriptionHtml", () => {
  it("escapes and keeps line breaks", () => {
    expect(textToDescriptionHtml("a < b & c\nsecond")).toBe("<p>a &lt; b &amp; c<br/>second</p>");
  });

  it("returns null for empty input", () => {
    expect(textToDescriptionHtml("   ")).toBeNull();
    expect(textToDescriptionHtml(undefined)).toBeNull();
  });
});

describe("matching helpers", () => {
  const projects = [
    { id: "p1", identifier: "LTS", name: "Logistics" },
    { id: "p2", identifier: "OPS", name: "Operations" },
  ];

  it("matches projects by identifier, exact name, then unique substring", () => {
    expect(matchProject(projects, "lts")?.id).toBe("p1");
    expect(matchProject(projects, "logistics")?.id).toBe("p1");
    expect(matchProject(projects, "oper")?.id).toBe("p2");
    expect(matchProject(projects, "o")).toBeUndefined();
    expect(matchProject(projects, "nope")).toBeUndefined();
  });

  it("matches members by display name or email and reports misses", () => {
    const members = [
      { id: "u1", display_name: "Budi", email: "budi@example.com" },
      { id: "u2", display_name: "Sari", email: "sari@example.com" },
    ];
    expect(matchAssignees(members, ["budi", "sari@example.com", "budi", "Ghost"])).toEqual({
      matched: ["u1", "u2"],
      missing: ["Ghost"],
    });
  });

  it("matches labels by name and reports misses", () => {
    const labels = [
      { id: "l1", name: "Maintenance" },
      { id: "l2", name: "Safety" },
    ];
    expect(matchLabels(labels, ["maintenance", "Nope"])).toEqual({ matched: ["l1"], missing: ["Nope"] });
  });

  it("matches states by name", () => {
    const states = [
      { id: "s1", name: "In Progress" },
      { id: "s2", name: "Done" },
    ];
    expect(matchState(states, "in progress")?.id).toBe("s1");
    expect(matchState(states, "")).toBeUndefined();
    expect(matchState(states, "Review")).toBeUndefined();
  });
});

describe("workItemHref", () => {
  it("builds the project issue route", () => {
    expect(workItemHref("acme", "p1", "i9")).toBe("/acme/projects/p1/issues/i9");
  });
});

describe("parseWorkItemRef", () => {
  it("splits an identifier into project and sequence", () => {
    expect(parseWorkItemRef(" LTS-42 ")).toEqual({ projectIdentifier: "LTS", sequenceId: "42" });
    expect(parseWorkItemRef("lts-7")).toEqual({ projectIdentifier: "lts", sequenceId: "7" });
  });

  it("rejects malformed references", () => {
    expect(parseWorkItemRef("LTS")).toBeNull();
    expect(parseWorkItemRef("LTS-")).toBeNull();
    expect(parseWorkItemRef("-42")).toBeNull();
    expect(parseWorkItemRef("LTS-abc")).toBeNull();
    expect(parseWorkItemRef("LTS-0")).toBeNull();
    expect(parseWorkItemRef("L T S-4")).toBeNull();
  });
});
