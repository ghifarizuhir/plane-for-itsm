import { describe, expect, it } from "vitest";
import type { TNotificationData } from "@plane/types";
// local imports
import { isReviewRequestNotification, isReviewSessionNotification } from "./review-notification";

describe("review notification guards", () => {
  it("detects review_request payloads", () => {
    const data = {
      review_request: {
        id: "req-1",
        board_type: "tcb",
        project_id: "p1",
        workspace_slug: "ws",
        status: "decided",
        subject_label: "CHG-1 Fix",
        session_id: "s1",
        session_title: "Weekly",
      },
    } as unknown as TNotificationData;
    expect(isReviewRequestNotification(data)).toBe(true);
    expect(isReviewSessionNotification(data)).toBe(false);
  });

  it("detects review_session payloads", () => {
    const data = {
      review_session: {
        id: "s1",
        board_type: "rcb",
        project_id: null,
        workspace_slug: "ws",
        title: "Release board",
        scheduled_at: "2026-10-05T09:00:00Z",
      },
    } as unknown as TNotificationData;
    expect(isReviewSessionNotification(data)).toBe(true);
    expect(isReviewRequestNotification(data)).toBe(false);
  });

  it("rejects payloads without an id", () => {
    expect(isReviewRequestNotification({ review_request: { id: "" } } as unknown as TNotificationData)).toBe(false);
    expect(isReviewSessionNotification(undefined)).toBe(false);
  });
});
