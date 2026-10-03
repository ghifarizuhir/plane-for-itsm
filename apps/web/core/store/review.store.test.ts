import { describe, expect, it, vi } from "vitest";
import type { IReviewSessionBriefing, IReviewSessionDetail } from "@plane/types";
// store
import { ReviewStore } from "./review.store";

const makeBriefing = (overrides: Partial<IReviewSessionBriefing> = {}): IReviewSessionBriefing =>
  ({
    version: 1,
    generated_at: "2026-10-03T00:00:00.000Z",
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

const makeSessionDetail = (): IReviewSessionDetail =>
  ({
    id: "session-1",
    workspace_id: "ws-1",
    board_type: "tcb",
    project_id: "project-1",
    title: "Review",
    scheduled_at: "2026-10-10T09:00:00.000Z",
    status: "scheduled",
    minutes: "",
    location: null,
    completed_at: null,
    cancelled_at: null,
    created_at: "2026-10-01T00:00:00.000Z",
    updated_at: "2026-10-01T00:00:00.000Z",
    created_by: "user-1",
    counts: { items: 0, participants: 0, pending_outcome: 0 },
    items: [],
    participants: [],
    briefing: null,
  }) as IReviewSessionDetail;

const makeStore = () => {
  const store = new ReviewStore({} as never);
  const reviewService = {
    generateReviewBriefing: vi.fn(async () => makeBriefing()),
  };
  (store as unknown as { reviewService: typeof reviewService }).reviewService = reviewService;
  return { store, reviewService };
};

describe("ReviewStore.generateBriefing", () => {
  it("stores the generated briefing on the cached session detail", async () => {
    const { store, reviewService } = makeStore();
    store.sessionDetailMap["session-1"] = makeSessionDetail();

    await store.generateBriefing("acme", "session-1", "id");

    expect(reviewService.generateReviewBriefing).toHaveBeenCalledWith("acme", "session-1", "id");
    expect(store.getSessionDetailById("session-1")?.briefing?.overall).toBe("Ringkasan");
  });

  it("propagates service errors", async () => {
    const { store, reviewService } = makeStore();
    reviewService.generateReviewBriefing.mockRejectedValueOnce(new Error("boom"));

    await expect(store.generateBriefing("acme", "session-1", "en")).rejects.toThrow("boom");
  });
});
