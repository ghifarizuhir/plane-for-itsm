import { describe, expect, it } from "vitest";
import {
  changedFieldCount,
  matchByNameOrId,
  validateAddCommentProposal,
  validateCreateArticleProposal,
  validateCreateServiceProposal,
  validateCreateSprintProposal,
  validateCreateTrackProposal,
  validateManageServiceLinksProposal,
  validateManageSprintItemsProposal,
  validateManageTrackItemsProposal,
  validateUpdateArticleProposal,
  validateUpdateServiceProposal,
  validateUpdateSprintProposal,
  validateUpdateTrackProposal,
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

describe("container proposal validators", () => {
  it("validates create-service proposals", () => {
    expect(validateCreateServiceProposal({ project: "LTS", name: "Email", status: "active" })).toBeNull();
    expect(validateCreateServiceProposal({ project: "LTS", name: "  " })).toBe("Name is required.");
    expect(validateCreateServiceProposal({ project: "LTS", name: "Email", status: "broken" })).toBe("Unknown status.");
    expect(validateCreateServiceProposal({ project: "LTS", name: "Email", repository_url: "git.example.com" })).toBe(
      "Repository URL must start with http:// or https://."
    );
  });

  it("validates update-service proposals", () => {
    expect(validateUpdateServiceProposal({ service: "Email", changes: { status: "deprecated" } })).toBeNull();
    expect(validateUpdateServiceProposal({ service: "Email", changes: {} })).toBe("At least one change is required.");
    expect(validateUpdateServiceProposal({ changes: { status: "active" } })).toBe("Service is required.");
    expect(validateUpdateServiceProposal({ service: "Email", changes: { owner: "" } })).toBeNull();
    expect(validateUpdateServiceProposal({ service: "Email", changes: { type: "legacy" } })).toBe("Unknown type.");
  });

  it("validates create-sprint proposals", () => {
    expect(
      validateCreateSprintProposal({
        project: "LTS",
        name: "Sprint 4",
        start_date: "2026-11-01",
        end_date: "2026-11-14",
      })
    ).toBeNull();
    expect(validateCreateSprintProposal({ project: "LTS", name: "Sprint 4", start_date: "2026-11-01" })).toBe(
      "Provide both start and end dates or neither."
    );
    expect(
      validateCreateSprintProposal({
        project: "LTS",
        name: "Sprint 4",
        start_date: "2026-11-14",
        end_date: "2026-11-01",
      })
    ).toBe("Start date must not be after end date.");
  });

  it("validates update-sprint proposals", () => {
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: { name: "Sprint 4b" } })).toBeNull();
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: {} })).toBe("At least one change is required.");
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: { name: "  " } })).toBe(
      "Name must not be empty."
    );
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: { start_date: "" } })).toBe(
      "Start date must be YYYY-MM-DD."
    );
  });

  it("validates create-track proposals", () => {
    expect(validateCreateTrackProposal({ project: "LTS", name: "Onboarding", status: "planned" })).toBeNull();
    expect(validateCreateTrackProposal({ project: "LTS", name: "Onboarding", status: "live" })).toBe("Unknown status.");
    expect(
      validateCreateTrackProposal({
        project: "LTS",
        name: "Onboarding",
        start_date: "2026-12-01",
        target_date: "2026-11-01",
      })
    ).toBe("Start date must not be after target date.");
    expect(
      validateCreateTrackProposal({
        project: "LTS",
        name: "Onboarding",
        members: ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"],
      })
    ).toBe("At most 10 members are allowed.");
  });

  it("validates update-track proposals", () => {
    expect(validateUpdateTrackProposal({ track: "Onboarding", changes: { description: "" } })).toBeNull();
    expect(validateUpdateTrackProposal({ track: "Onboarding", changes: { members: [] } })).toBeNull();
    expect(validateUpdateTrackProposal({ track: "Onboarding", changes: {} })).toBe("At least one change is required.");
    expect(validateUpdateTrackProposal({ changes: { status: "paused" } })).toBe("Track is required.");
  });
});

describe("article proposal validators", () => {
  it("validates create-article proposals", () => {
    expect(
      validateCreateArticleProposal({
        project: "LTS",
        name: "Runbook",
        content: "Step 1",
        access: "private",
      })
    ).toBeNull();
    expect(validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "  " })).toBe(
      "Content is required."
    );
    expect(validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "x".repeat(20001) })).toBe(
      "Content must be at most 20000 characters."
    );
    expect(
      validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "Step", access: "secret" as never })
    ).toBe("Unknown access.");
    expect(
      validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "Step", parent_article: "nope" })
    ).toBe("Parent article must be a page uuid.");
  });

  it("validates update-article proposals", () => {
    expect(
      validateUpdateArticleProposal({
        article: "0f3f3f3f-0000-0000-0000-000000000002",
        action: "append",
        content: "Step 3",
      })
    ).toBeNull();
    expect(validateUpdateArticleProposal({ article: "0f3f3f3f-0000-0000-0000-000000000002", action: "append" })).toBe(
      "At least one of name or content is required."
    );
    expect(validateUpdateArticleProposal({ article: "nope", action: "append", content: "x" })).toBe(
      "Article must be a page uuid."
    );
    expect(
      validateUpdateArticleProposal({
        article: "0f3f3f3f-0000-0000-0000-000000000002",
        action: "rewrite" as never,
        content: "x",
      })
    ).toBe("Action must be append or replace.");
    expect(
      validateUpdateArticleProposal({
        article: "0f3f3f3f-0000-0000-0000-000000000002",
        action: "append",
        name: "  ",
      })
    ).toBe("Name must not be empty.");
  });
});
