/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { orderBy } from "lodash-es";
import type { TNotificationData, TNotificationScheduleRun } from "@plane/types";

export type TAiScheduleFrequency = "hourly" | "daily" | "weekly" | "monthly";

export const AI_SCHEDULE_TOOLS = ["list_projects", "count_work_items", "search_work_items"] as const;
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

export type TAiAgentPendingAction = {
  kind: "create_schedule";
  proposal: TAiScheduleProposal;
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

export const isStructuredProposal = (proposal: TAiScheduleProposal): proposal is TStructuredScheduleProposal =>
  Array.isArray(proposal.how_to) &&
  Array.isArray(proposal.tools) &&
  typeof proposal.description === "string" &&
  typeof proposal.expected_output === "string";

export const scheduleDescription = (schedule: { spec?: TAiScheduleSpec | null; prompt: string }): string =>
  schedule.spec?.description?.trim() || schedule.prompt;

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
