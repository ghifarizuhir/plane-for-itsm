/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { orderBy } from "lodash-es";
import type { TNotificationData, TNotificationScheduleRun } from "@plane/types";

export type TAiScheduleFrequency = "hourly" | "daily" | "weekly" | "monthly";

export const AI_SCHEDULE_TOOLS = [
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
  "list_services",
  "get_service",
  "list_intake_items",
  "get_intake_item",
  "count_intake_items",
  "list_sprints",
  "get_sprint",
  "list_sprint_work_items",
  "list_tracks",
  "get_track",
  "list_track_work_items",
  "search_articles",
  "get_article",
] as const;
export type TAiScheduleTool = (typeof AI_SCHEDULE_TOOLS)[number];

export type TAiScheduleSpec = {
  version: number;
  description: string;
  how_to: string[];
  tools: TAiScheduleTool[];
  expected_output: string;
};

export type TAiScheduleProposal = {
  name: string;
  frequency: TAiScheduleFrequency;
  time: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone: string;
  prompt?: string;
} & Partial<TAiScheduleSpec>;

export type TAiScheduleUpdateChanges = {
  name?: string;
  description?: string;
  how_to?: string[];
  tools?: TAiScheduleTool[];
  expected_output?: string;
  prompt?: string;
  frequency?: TAiScheduleFrequency;
  time?: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone?: string;
};

export type TAiScheduleUpdate = TAiScheduleUpdateChanges & { enabled?: boolean };

export type TAiScheduleUpdateProposal = {
  schedule_id: string;
  changes: TAiScheduleUpdateChanges;
  before: TAiSchedule | null;
  can_edit: boolean;
};

export type TAiScheduleDiffRow = {
  label: string;
  before: string;
  after: string;
};

export type TStructuredScheduleProposal = TAiScheduleProposal & TAiScheduleSpec;

export type TAiScheduleRun = {
  id: string;
  status: "queued" | "running" | "success" | "failed";
  trigger: "scheduled" | "manual";
  prompt: string;
  response?: string | null;
  response_html?: string | null;
  error?: string | null;
  created_at: string;
  started_at?: string | null;
  finished_at?: string | null;
};

export type TAiSchedule = Omit<TAiScheduleProposal, "prompt"> & {
  id: string;
  enabled: boolean;
  next_run_at: string;
  created_by_id: string;
  created_at: string;
  prompt: string;
  spec?: TAiScheduleSpec | null;
  last_status?: TAiScheduleRun["status"] | null;
  last_finished_at?: string | null;
  last_run_at?: string | null;
  runs?: TAiScheduleRun[];
};

export type TAiAgentPendingAction =
  | { kind: "create_schedule"; proposal: TAiScheduleProposal }
  | {
      kind: "update_schedule";
      proposal: { schedule_id: string; changes: TAiScheduleUpdateChanges };
    };

const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/** True when the message starts with the `/schedule` slash command (case-insensitive). */
export const isScheduleCommand = (text: string): boolean => /^\/schedule(?:\s|$)/i.test(text.trimStart());

export const scheduleStatusLabel = (
  status: "queued" | "running" | "success" | "failed" | null | undefined
): string | null => {
  switch (status) {
    case "queued":
      return "Queued";
    case "running":
      return "Running";
    case "success":
      return "Success";
    case "failed":
      return "Failed";
    default:
      return null;
  }
};

export const runDurationInSeconds = (run: {
  started_at?: string | null;
  finished_at?: string | null;
}): number | null => {
  if (!run.started_at || !run.finished_at) return null;
  const started = Date.parse(run.started_at);
  const finished = Date.parse(run.finished_at);
  if (!Number.isFinite(started) || !Number.isFinite(finished)) return null;
  const seconds = Math.round((finished - started) / 1000);
  return seconds >= 0 ? seconds : null;
};

export const humanizeSchedule = (
  proposal: Pick<TAiScheduleProposal, "frequency" | "time" | "day_of_week" | "day_of_month" | "timezone">
): string => {
  const zone = proposal.timezone || "UTC";
  switch (proposal.frequency) {
    case "hourly":
      return `Every hour at :${proposal.time.slice(-2)} · ${zone}`;
    case "daily":
      return `Every day at ${proposal.time} · ${zone}`;
    case "weekly":
      return `Every ${WEEKDAYS[(proposal.day_of_week ?? 1) - 1] ?? "Monday"} at ${proposal.time} · ${zone}`;
    case "monthly":
      return `Every month on day ${proposal.day_of_month ?? 1} at ${proposal.time} · ${zone}`;
    default:
      return "—";
  }
};

export const SCHEDULE_SPEC_LIMITS = {
  description: 500,
  steps: 10,
  step: 500,
  expectedOutput: 1000,
} as const;

/** Mirrors `ScheduleSpec::validated` on the backend; returns the first error. */
export const validateScheduleSpec = (spec: Partial<TAiScheduleSpec>): string | null => {
  const description = spec.description?.trim() ?? "";
  if (!description) return "Description is required.";
  if (description.length > SCHEDULE_SPEC_LIMITS.description)
    return `Description must be at most ${SCHEDULE_SPEC_LIMITS.description} characters.`;

  const howTo = spec.how_to ?? [];
  if (howTo.length === 0) return "Add at least one step.";
  if (howTo.length > SCHEDULE_SPEC_LIMITS.steps) return `At most ${SCHEDULE_SPEC_LIMITS.steps} steps are allowed.`;
  if (howTo.some((step) => !step.trim() || step.trim().length > SCHEDULE_SPEC_LIMITS.step))
    return `Each step must be 1-${SCHEDULE_SPEC_LIMITS.step} characters.`;

  const tools = spec.tools ?? [];
  if (tools.length === 0) return "Select at least one tool.";
  if (tools.some((tool) => !AI_SCHEDULE_TOOLS.includes(tool))) return "Unknown tool selected.";

  const expected = spec.expected_output?.trim() ?? "";
  if (!expected) return "Expected output is required.";
  if (expected.length > SCHEDULE_SPEC_LIMITS.expectedOutput)
    return `Expected output must be at most ${SCHEDULE_SPEC_LIMITS.expectedOutput} characters.`;

  return null;
};

export const validateScheduleBasics = (draft: {
  name: string;
  frequency: TAiScheduleFrequency;
  time: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone: string;
}): string | null => {
  const name = draft.name.trim();
  if (!name) return "Name is required.";
  if (name.length > 120) return "Name must be at most 120 characters.";
  if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(draft.time)) return "Time must be HH:MM (24h).";
  if (draft.frequency === "weekly" && !(draft.day_of_week && draft.day_of_week >= 1 && draft.day_of_week <= 7)) {
    return "Choose a weekday.";
  }
  if (draft.frequency === "monthly" && !(draft.day_of_month && draft.day_of_month >= 1 && draft.day_of_month <= 31)) {
    return "Choose a day of month (1-31).";
  }
  const timezone = draft.timezone.trim();
  if (!timezone) return "Timezone is required.";
  try {
    Intl.DateTimeFormat("en-US", { timeZone: timezone });
  } catch {
    return `Unknown timezone: ${timezone}`;
  }
  return null;
};

export const isStructuredProposal = (proposal: TAiScheduleProposal): proposal is TStructuredScheduleProposal =>
  Array.isArray(proposal.how_to) &&
  Array.isArray(proposal.tools) &&
  typeof proposal.description === "string" &&
  typeof proposal.expected_output === "string";

export const scheduleDescription = (schedule: { spec?: TAiScheduleSpec | null; prompt: string }): string =>
  schedule.spec?.description?.trim() || schedule.prompt;

export const diffScheduleUpdate = (proposal: TAiScheduleUpdateProposal): TAiScheduleDiffRow[] => {
  const { before, changes } = proposal;
  if (!before) return [];
  const rows: TAiScheduleDiffRow[] = [];
  const push = (label: string, previous: string, next: string | undefined) => {
    if (next === undefined || previous === next) return;
    rows.push({ label, before: previous || "—", after: next || "—" });
  };

  push("Name", before.name, changes.name);

  const cadenceChanged = [
    changes.frequency,
    changes.time,
    changes.day_of_week,
    changes.day_of_month,
    changes.timezone,
  ].some((value) => value !== undefined);
  if (cadenceChanged) {
    const after = humanizeSchedule({
      frequency: changes.frequency ?? before.frequency,
      time: changes.time ?? before.time,
      day_of_week: changes.day_of_week !== undefined ? changes.day_of_week : before.day_of_week,
      day_of_month: changes.day_of_month !== undefined ? changes.day_of_month : before.day_of_month,
      timezone: changes.timezone ?? before.timezone,
    });
    push("Cadence", humanizeSchedule(before), after);
  }

  push("Description", before.spec?.description ?? "", changes.description);
  if (changes.how_to !== undefined) {
    const previous = (before.spec?.how_to ?? []).map((step, index) => `${index + 1}. ${step}`).join("\n");
    const next = changes.how_to.map((step, index) => `${index + 1}. ${step}`).join("\n");
    push("Steps", previous, next);
  }
  if (changes.tools !== undefined) {
    push("Tools", (before.spec?.tools ?? []).join(", "), changes.tools.join(", "));
  }
  push("Expected output", before.spec?.expected_output ?? "", changes.expected_output);
  push("Prompt", before.prompt, changes.prompt);
  return rows;
};

export type TAiScheduleListFilter = {
  query: string;
  status: "all" | "active" | "paused";
};

export const RUN_STATUS_BADGE_VARIANTS: Record<TAiScheduleRun["status"], "success" | "danger" | "brand"> = {
  success: "success",
  failed: "danger",
  queued: "brand",
  running: "brand",
};

export const isScheduleRunNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { ai_schedule: TNotificationScheduleRun } =>
  typeof data?.ai_schedule?.schedule_id === "string" && data.ai_schedule.schedule_id.length > 0;

export const scheduleRunNotificationHref = (workspaceSlug: string, scheduleId: string): string =>
  `/${workspaceSlug}/scheduler?schedule=${scheduleId}`;

export const scheduleRunNotificationText = (status: TNotificationScheduleRun["status"]): string =>
  status === "success" ? "Scheduled run finished" : "Scheduled run failed";

/** Search + status filter + deterministic ordering for the scheduler list. */
export const filterSchedules = (schedules: TAiSchedule[], filter: TAiScheduleListFilter): TAiSchedule[] => {
  const query = filter.query.trim().toLowerCase();
  const matches = schedules.filter((schedule) => {
    if (filter.status === "active" && !schedule.enabled) return false;
    if (filter.status === "paused" && schedule.enabled) return false;
    if (!query) return true;
    return schedule.name.toLowerCase().includes(query);
  });
  // NOTE: do not use `.sort()` here — the repo's `oxlint --fix` pre-commit
  // hook rewrites it to `.toSorted()`, which the TS lib target does not know.
  return orderBy(
    matches,
    [
      (schedule) => (schedule.enabled ? 0 : 1),
      (schedule) => (schedule.next_run_at ? 0 : 1),
      (schedule) => schedule.next_run_at ?? "",
      (schedule) => schedule.created_at,
    ],
    ["asc", "asc", "asc", "desc"]
  );
};
