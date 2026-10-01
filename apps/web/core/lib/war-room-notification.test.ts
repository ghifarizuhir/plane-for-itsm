import { describe, expect, it } from "vitest";
import type { TNotificationData, TNotificationWarRoom } from "@plane/types";
import { isWarRoomNotification } from "./war-room-notification";

const warRoom: TNotificationWarRoom = {
  id: "8b0e6c3a-6f4f-4c2a-9a3f-0b1c2d3e4f50",
  project_id: "1c2d3e4f-5a6b-7c8d-9e0f-1a2b3c4d5e6f",
  workspace_slug: "acme",
  name: "Checkout down",
  sequence_id: 3,
};

describe("isWarRoomNotification", () => {
  it("accepts data carrying a war_room payload", () => {
    expect(isWarRoomNotification({ war_room: warRoom })).toBe(true);
  });

  it("rejects missing data, empty ids, and other notification kinds", () => {
    expect(isWarRoomNotification(undefined)).toBe(false);
    expect(isWarRoomNotification({})).toBe(false);
    expect(isWarRoomNotification({ war_room: { ...warRoom, id: "" } })).toBe(false);
    const scheduleRun: TNotificationData = {
      ai_schedule: {
        schedule_id: "schedule-1",
        run_id: "run-1",
        name: "Daily report",
        status: "success",
        finished_at: null,
      },
    };
    expect(isWarRoomNotification(scheduleRun)).toBe(false);
  });
});
