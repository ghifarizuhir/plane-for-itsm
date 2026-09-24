import { describe, expect, it } from "vitest";
import { humanizeSchedule, isScheduleCommand } from "./ai-schedule";

describe("isScheduleCommand", () => {
  it("matches only leading /schedule commands", () => {
    expect(isScheduleCommand("/schedule")).toBe(true);
    expect(isScheduleCommand("/schedule every Monday")).toBe(true);
    expect(isScheduleCommand("  /schedule now")).toBe(true);
    expect(isScheduleCommand("please /schedule")).toBe(false);
    expect(isScheduleCommand("/scheduled")).toBe(false);
  });

  it("is case-insensitive for the command token and accepts any trailing whitespace", () => {
    expect(isScheduleCommand("/schedule\t")).toBe(true);
    expect(isScheduleCommand("/schedule\n")).toBe(true);
    expect(isScheduleCommand("/Schedule daily")).toBe(true);
    expect(isScheduleCommand("/schedule/")).toBe(false);
  });
});

describe("humanizeSchedule", () => {
  it("renders each preset with time and timezone", () => {
    expect(humanizeSchedule({ frequency: "hourly", time: "00:30", timezone: "UTC" })).toBe("Every hour at :30 · UTC");
    expect(humanizeSchedule({ frequency: "daily", time: "09:00", timezone: "Asia/Jakarta" })).toBe(
      "Every day at 09:00 · Asia/Jakarta"
    );
    expect(humanizeSchedule({ frequency: "weekly", time: "09:00", day_of_week: 1, timezone: "Asia/Jakarta" })).toBe(
      "Every Monday at 09:00 · Asia/Jakarta"
    );
    expect(humanizeSchedule({ frequency: "monthly", time: "09:00", day_of_month: 31, timezone: "UTC" })).toBe(
      "Every month on day 31 at 09:00 · UTC"
    );
  });

  it("applies defaults for missing day/timezone fields", () => {
    expect(humanizeSchedule({ frequency: "daily", time: "09:00", timezone: "" })).toBe("Every day at 09:00 · UTC");
    expect(humanizeSchedule({ frequency: "weekly", time: "09:00", timezone: "UTC" })).toBe(
      "Every Monday at 09:00 · UTC"
    );
    expect(humanizeSchedule({ frequency: "monthly", time: "09:00", timezone: "UTC" })).toBe(
      "Every month on day 1 at 09:00 · UTC"
    );
  });

  it("renders an em dash for an unknown frequency", () => {
    expect(humanizeSchedule({ frequency: "yearly" as never, time: "09:00", timezone: "UTC" })).toBe("—");
  });
});
