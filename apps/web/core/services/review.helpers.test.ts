import { describe, expect, it } from "vitest";
import type { IReviewSession, IReviewSessionBriefing } from "@plane/types";
// local imports
import {
  activeRequestForSubject,
  briefingItemById,
  fromDateTimeLocal,
  isBriefingStale,
  isBriefingTextMode,
  latestRequestForSubject,
  reviewRequestsKey,
  reviewSessionsKey,
  sortSessionsByScheduledAt,
  toDateTimeLocal,
} from "./review.helpers";

describe("reviewRequestsKey", () => {
  it("is stable for equivalent params and distinguishes board/scope", () => {
    const base = { board_type: "tcb" as const, project_id: "p1" };
    expect(reviewRequestsKey(base)).toBe(reviewRequestsKey({ ...base }));
    expect(reviewRequestsKey(base)).not.toBe(reviewRequestsKey({ board_type: "rcb" }));
  });
});

describe("reviewSessionsKey", () => {
  it("distinguishes board and project scope", () => {
    expect(reviewSessionsKey({ board_type: "tcb", project_id: "p1" })).not.toBe(
      reviewSessionsKey({ board_type: "rcb" })
    );
  });
});

describe("latestRequestForSubject", () => {
  it("returns the most recent request", () => {
    const requests = [
      { submitted_at: "2026-01-01T00:00:00Z", status: "withdrawn" },
      { submitted_at: "2026-02-01T00:00:00Z", status: "pending" },
    ];
    expect(latestRequestForSubject(requests)?.status).toBe("pending");
  });

  it("returns null for an empty list", () => {
    expect(latestRequestForSubject([])).toBeNull();
  });
});

describe("activeRequestForSubject", () => {
  it("only considers pending and scheduled requests", () => {
    const requests = [
      { submitted_at: "2026-02-01T00:00:00Z", status: "decided" },
      { submitted_at: "2026-01-01T00:00:00Z", status: "scheduled" },
    ];
    expect(activeRequestForSubject(requests)?.status).toBe("scheduled");
  });
});

describe("toDateTimeLocal", () => {
  it("formats an ISO string for a datetime-local input", () => {
    const iso = new Date(2026, 9, 5, 9, 30).toISOString();
    expect(toDateTimeLocal(iso)).toBe("2026-10-05T09:30");
  });

  it("returns empty string for null/invalid input", () => {
    expect(toDateTimeLocal(null)).toBe("");
    expect(toDateTimeLocal("not-a-date")).toBe("");
  });
});

describe("fromDateTimeLocal", () => {
  it("converts a datetime-local value back to ISO", () => {
    const iso = fromDateTimeLocal("2026-10-05T09:30");
    expect(iso).not.toBeNull();
    expect(new Date(iso as string).getFullYear()).toBe(2026);
  });

  it("returns null for empty/invalid input", () => {
    expect(fromDateTimeLocal("")).toBeNull();
    expect(fromDateTimeLocal("nope")).toBeNull();
  });
});
describe("sortSessionsByScheduledAt", () => {
  it("orders newest first", () => {
    const sessions = [
      { id: "old", scheduled_at: "2026-01-01T00:00:00Z" },
      { id: "new", scheduled_at: "2026-03-01T00:00:00Z" },
    ] as IReviewSession[];
    expect(sortSessionsByScheduledAt(sessions).map((session) => session.id)).toEqual(["new", "old"]);
  });
});

const makeBriefing = (overrides: Partial<IReviewSessionBriefing> = {}): IReviewSessionBriefing =>
  ({
    version: 1,
    generated_at: "2026-10-03T00:00:00Z",
    generated_by_name: "Budi",
    model: "gpt-4o-mini",
    language: "id",
    format: "json",
    overall: "Ringkasan",
    items: [],
    included_items: 0,
    skipped_items: 0,
    ...overrides,
  }) as IReviewSessionBriefing;

describe("briefingItemById", () => {
  it("maps items by session_item_id and tolerates null", () => {
    const briefing = makeBriefing({
      items: [
        { session_item_id: "item-1", summary: "s", discussion_points: [], risks: [] },
        { session_item_id: "item-2", summary: "t", discussion_points: [], risks: [] },
      ],
    });
    expect(briefingItemById(briefing).get("item-2")?.summary).toBe("t");
    expect(briefingItemById(null).size).toBe(0);
  });
});

describe("isBriefingTextMode", () => {
  it("is true only for text format", () => {
    expect(isBriefingTextMode(makeBriefing({ format: "text" }))).toBe(true);
    expect(isBriefingTextMode(makeBriefing())).toBe(false);
    expect(isBriefingTextMode(null)).toBe(false);
  });
});

describe("isBriefingStale", () => {
  it("compares included plus skipped against the current agenda size", () => {
    expect(isBriefingStale(makeBriefing({ included_items: 2, skipped_items: 0 }), 3)).toBe(true);
    expect(isBriefingStale(makeBriefing({ included_items: 2, skipped_items: 1 }), 3)).toBe(false);
    expect(isBriefingStale(null, 3)).toBe(false);
  });
});
