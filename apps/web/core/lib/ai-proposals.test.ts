import { describe, expect, it } from "vitest";
import { changedFieldCount, validateAddCommentProposal, validateUpdateWorkItemProposal } from "./ai-proposals";

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
