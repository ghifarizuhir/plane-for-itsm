import { describe, expect, it } from "vitest";
import {
  AI_SCHEDULE_TOOLS,
  filterSchedules,
  humanizeSchedule,
  isScheduleCommand,
  isScheduleRunNotification,
  isStructuredProposal,
  runDurationInSeconds,
  scheduleDescription,
  scheduleRunNotificationHref,
  scheduleRunNotificationText,
  scheduleStatusLabel,
  validateScheduleSpec,
} from "./ai-schedule";
import type { TAiSchedule, TAiScheduleSpec } from "./ai-schedule";

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

describe("scheduleStatusLabel", () => {
  it("maps each run status to a display label", () => {
    expect(scheduleStatusLabel("queued")).toBe("Queued");
    expect(scheduleStatusLabel("running")).toBe("Running");
    expect(scheduleStatusLabel("success")).toBe("Success");
    expect(scheduleStatusLabel("failed")).toBe("Failed");
  });

  it("returns null for missing statuses", () => {
    expect(scheduleStatusLabel(null)).toBeNull();
    expect(scheduleStatusLabel(undefined)).toBeNull();
  });
});

describe("runDurationInSeconds", () => {
  it("computes the rounded duration in seconds", () => {
    expect(runDurationInSeconds({ started_at: "2024-01-01T00:00:00Z", finished_at: "2024-01-01T00:00:05.400Z" })).toBe(
      5
    );
    expect(
      runDurationInSeconds({ started_at: "2024-01-01T00:00:00.000Z", finished_at: "2024-01-01T00:00:00.400Z" })
    ).toBe(0);
  });

  it("returns null when a timestamp is missing", () => {
    expect(runDurationInSeconds({})).toBeNull();
    expect(runDurationInSeconds({ started_at: "2024-01-01T00:00:00Z", finished_at: null })).toBeNull();
    expect(runDurationInSeconds({ started_at: null, finished_at: "2024-01-01T00:00:05Z" })).toBeNull();
  });

  it("returns null for malformed timestamps", () => {
    expect(runDurationInSeconds({ started_at: "nope", finished_at: "2024-01-01T00:00:05Z" })).toBeNull();
    expect(runDurationInSeconds({ started_at: "2024-01-01T00:00:00Z", finished_at: "also-nope" })).toBeNull();
  });

  it("returns null for negative durations", () => {
    expect(
      runDurationInSeconds({ started_at: "2024-01-01T00:00:05Z", finished_at: "2024-01-01T00:00:00Z" })
    ).toBeNull();
  });
});

const validSpec: TAiScheduleSpec = {
  version: 1,
  description: "Summarize overdue work",
  how_to: ["Count overdue items"],
  tools: ["count_work_items"],
  expected_output: "A short list",
};

describe("validateScheduleSpec", () => {
  it("accepts a complete spec", () => {
    expect(validateScheduleSpec({ ...validSpec, tools: [...validSpec.tools] })).toBeNull();
  });

  it("rejects missing fields with a message", () => {
    expect(validateScheduleSpec({ ...validSpec, description: "  ", tools: [...validSpec.tools] })).toBe(
      "Description is required."
    );
    expect(validateScheduleSpec({ ...validSpec, how_to: [], tools: [...validSpec.tools] })).toBe(
      "Add at least one step."
    );
    expect(validateScheduleSpec({ ...validSpec, how_to: ["ok", "  "], tools: [...validSpec.tools] })).toBe(
      "Each step must be 1-500 characters."
    );
    expect(validateScheduleSpec({ ...validSpec, tools: [] })).toBe("Select at least one tool.");
    expect(validateScheduleSpec({ ...validSpec, expected_output: "", tools: [...validSpec.tools] })).toBe(
      "Expected output is required."
    );
  });

  it("rejects over-limit and unknown values", () => {
    expect(validateScheduleSpec({ ...validSpec, description: "x".repeat(501), tools: [...validSpec.tools] })).toBe(
      "Description must be at most 500 characters."
    );
    expect(
      validateScheduleSpec({
        ...validSpec,
        how_to: Array.from({ length: 11 }, (_, index) => `step ${index}`),
        tools: [...validSpec.tools],
      })
    ).toBe("At most 10 steps are allowed.");
    expect(validateScheduleSpec({ ...validSpec, tools: ["drop_tables" as never] })).toBe("Unknown tool selected.");
  });
});

describe("AI_SCHEDULE_TOOLS", () => {
  it("lists every read tool in backend canonical order", () => {
    expect([...AI_SCHEDULE_TOOLS]).toEqual([
      "list_projects",
      "count_work_items",
      "search_work_items",
      "get_work_item",
      "list_work_item_comments",
      "list_work_item_relations",
      "list_members",
      "list_states",
      "list_labels",
      "list_work_item_types",
    ]);
  });
});

describe("isStructuredProposal", () => {
  it("detects proposals carrying every spec field", () => {
    expect(
      isStructuredProposal({ ...validSpec, name: "Daily", frequency: "daily", time: "09:00", timezone: "UTC" })
    ).toBe(true);
    expect(
      isStructuredProposal({ name: "Daily", prompt: "Report", frequency: "daily", time: "09:00", timezone: "UTC" })
    ).toBe(false);
  });
});

describe("scheduleDescription", () => {
  it("prefers the spec description and falls back to the prompt", () => {
    expect(scheduleDescription({ spec: validSpec, prompt: "legacy" })).toBe("Summarize overdue work");
    expect(scheduleDescription({ spec: null, prompt: "legacy" })).toBe("legacy");
  });
});

const schedule = (overrides: Partial<TAiSchedule>): TAiSchedule => ({
  id: "s1",
  name: "Daily report",
  frequency: "daily",
  time: "09:00",
  timezone: "UTC",
  enabled: true,
  next_run_at: "2026-09-28T09:00:00Z",
  created_by_id: "u1",
  created_at: "2026-09-01T00:00:00Z",
  prompt: "Summarize",
  spec: null,
  ...overrides,
});

describe("filterSchedules", () => {
  it("filters by name case-insensitively", () => {
    const rows = [schedule({ id: "a", name: "Daily report" }), schedule({ id: "b", name: "Weekly digest" })];
    expect(filterSchedules(rows, { query: "daily", status: "all" }).map((row) => row.id)).toEqual(["a"]);
    expect(filterSchedules(rows, { query: "WEEKLY", status: "all" }).map((row) => row.id)).toEqual(["b"]);
    expect(filterSchedules(rows, { query: "  ", status: "all" })).toHaveLength(2);
  });

  it("filters by status", () => {
    const rows = [schedule({ id: "a", enabled: true }), schedule({ id: "b", enabled: false })];
    expect(filterSchedules(rows, { query: "", status: "active" }).map((row) => row.id)).toEqual(["a"]);
    expect(filterSchedules(rows, { query: "", status: "paused" }).map((row) => row.id)).toEqual(["b"]);
  });

  it("sorts active before paused, then by next run ascending", () => {
    const rows = [
      schedule({ id: "paused", enabled: false, next_run_at: "2026-09-01T00:00:00Z" }),
      schedule({ id: "late", enabled: true, next_run_at: "2026-09-30T09:00:00Z" }),
      schedule({ id: "soon", enabled: true, next_run_at: "2026-09-28T09:00:00Z" }),
    ];
    expect(filterSchedules(rows, { query: "", status: "all" }).map((row) => row.id)).toEqual([
      "soon",
      "late",
      "paused",
    ]);
  });
});

describe("schedule run notifications", () => {
  it("detects the ai_schedule payload", () => {
    expect(
      isScheduleRunNotification({
        ai_schedule: {
          schedule_id: "s1",
          run_id: "r1",
          name: "Daily report",
          status: "success",
          finished_at: null,
        },
      })
    ).toBe(true);
    expect(isScheduleRunNotification({})).toBe(false);
    expect(isScheduleRunNotification(undefined)).toBe(false);
    expect(
      isScheduleRunNotification({
        issue: { id: "i1" },
        issue_activity: { id: "a1", actor: "u1", field: "state", issue_comment: "", verb: "updated" },
      } as never)
    ).toBe(false);
  });

  it("builds the scheduler deep link and status sentence", () => {
    expect(scheduleRunNotificationHref("acme", "s1")).toBe("/acme/scheduler?schedule=s1");
    expect(scheduleRunNotificationText("success")).toBe("Scheduled run finished");
    expect(scheduleRunNotificationText("failed")).toBe("Scheduled run failed");
  });
});
