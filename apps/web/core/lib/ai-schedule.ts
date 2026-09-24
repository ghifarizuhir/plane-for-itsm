export type TAiScheduleFrequency = "hourly" | "daily" | "weekly" | "monthly";

export type TAiScheduleProposal = {
  name: string;
  prompt: string;
  frequency: TAiScheduleFrequency;
  time: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone: string;
};

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

export type TAiSchedule = TAiScheduleProposal & {
  id: string;
  enabled: boolean;
  next_run_at: string;
  created_by_id: string;
  created_at: string;
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

/** True when the message starts with the `/schedule` slash command. */
export const isScheduleCommand = (text: string): boolean => {
  const trimmed = text.trimStart();
  if (!trimmed.startsWith("/schedule")) return false;
  const rest = trimmed.slice("/schedule".length);
  return rest === "" || /^\s/.test(rest);
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
  }
};
