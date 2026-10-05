import { describe, expect, it } from "vitest";
import {
  changedFieldCount,
  matchByNameOrId,
  validateAddCommentProposal,
  validateManageServiceLinksProposal,
  validateManageSprintItemsProposal,
  validateManageTrackItemsProposal,
  validateUpdateWorkItemProposal,
} from "./ai-proposals";

describe("validateUpdateWorkItemProposal", () => {
  it("accepts a minimal change", () => {
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { priority: "high" } })).toBeNull();
  });

  it("requires a work item and at least one change", () => {
    expect(validateUpdateWorkItemProposal({ changes: { priority: "high" } })).toBe("Work item is required.");
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: {} })).toBe(
      "At least one change is required."
    );
  });

  it("rejects invalid fields", () => {
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { name: "  " } })).toBe(
      "Title must not be empty."
    );
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { priority: "p0" } })).toBe(
      "Unknown priority."
    );
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { start_date: "05-10-2026" } })).toBe(
      "Start date must be YYYY-MM-DD."
    );
    expect(
      validateUpdateWorkItemProposal({
        work_item: "LTS-1",
        changes: { start_date: "2026-10-05", target_date: "2026-10-01" },
      })
    ).toBe("Start date must not be after target date.");
  });

  it("counts only set fields", () => {
    expect(changedFieldCount({ name: null, priority: "high", labels: [] })).toBe(2);
    expect(changedFieldCount({})).toBe(0);
  });
});

describe("validateAddCommentProposal", () => {
  it("accepts a comment and rejects empty or oversized ones", () => {
    expect(validateAddCommentProposal({ work_item: "LTS-1", comment: "hi" })).toBeNull();
    expect(validateAddCommentProposal({ work_item: "LTS-1", comment: "  " })).toBe("Comment is required.");
    expect(validateAddCommentProposal({ work_item: "LTS-1", comment: "a".repeat(5001) })).toBe(
      "Comment must be at most 5000 characters."
    );
    expect(validateAddCommentProposal({ comment: "hi" })).toBe("Work item is required.");
  });
});

describe("link proposal validators", () => {
  it("validates service link proposals", () => {
    expect(validateManageServiceLinksProposal({ work_item: "LTS-1", services: ["Email"], action: "link" })).toBeNull();
    expect(validateManageServiceLinksProposal({ work_item: "LTS-1", services: [], action: "link" })).toBe(
      "At least one service is required."
    );
    expect(
      validateManageServiceLinksProposal({
        work_item: "LTS-1",
        services: ["Email"],
        action: "attach" as never,
      })
    ).toBe("Action must be link or unlink.");
    expect(validateManageServiceLinksProposal({ services: ["Email"], action: "link" })).toBe("Work item is required.");
  });

  it("validates sprint and track item proposals", () => {
    expect(validateManageSprintItemsProposal({ sprint: "Sprint 3", work_items: ["LTS-1"], action: "add" })).toBeNull();
    expect(validateManageSprintItemsProposal({ sprint: "Sprint 3", work_items: [], action: "add" })).toBe(
      "At least one work item is required."
    );
    expect(
      validateManageTrackItemsProposal({ track: "Onboarding", work_items: ["LTS-1"], action: "remove" })
    ).toBeNull();
    expect(validateManageTrackItemsProposal({ work_items: ["LTS-1"], action: "remove" })).toBe("Track is required.");
  });
});

describe("matchByNameOrId", () => {
  const items = [
    { id: "1", name: "Email" },
    { id: "2", name: "VPN" },
  ];

  it("matches by id first, then exact name case-insensitively", () => {
    expect(matchByNameOrId(items, "2")?.name).toBe("VPN");
    expect(matchByNameOrId(items, " email ")?.id).toBe("1");
    expect(matchByNameOrId(items, "nope")).toBeUndefined();
    expect(matchByNameOrId(items, "  ")).toBeUndefined();
  });
});
