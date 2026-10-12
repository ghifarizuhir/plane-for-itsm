/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TIssueActivity } from "@plane/types";

export const MILESTONE_FIELDS = ["state", "priority", "assignees", "target_date", "start_date", "archived_at"] as const;

export type TMilestoneField = (typeof MILESTONE_FIELDS)[number];

export type TMilestoneLabels = {
  created: string;
  state: string;
  priority: string;
  assignees: string;
  target_date: string;
  start_date: string;
  archived_at: string;
  noValue: string;
};

export type TMilestoneLookups = {
  stateName: (id: string) => string | undefined;
  memberName: (id: string) => string | undefined;
};

export type TMilestoneRow = {
  createdAt: string;
  actor: string;
  event: string;
  from: string;
  to: string;
};

const splitIds = (value?: string): string[] =>
  (value ?? "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const formatDate = (value?: string): string => {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(0, 10);
};

export function milestoneRows(
  activities: TIssueActivity[],
  lookups: TMilestoneLookups,
  labels: TMilestoneLabels
): TMilestoneRow[] {
  const rows: TMilestoneRow[] = [];
  const sorted = [...activities].toSorted((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());

  for (const activity of sorted) {
    const actor = activity.actor_detail?.display_name ?? "";
    const base = { createdAt: activity.created_at, actor };

    if (activity.verb === "created") {
      rows.push({ ...base, event: labels.created, from: labels.noValue, to: labels.noValue });
      continue;
    }

    const field = activity.field as TMilestoneField | undefined;
    if (!field || !(MILESTONE_FIELDS as readonly string[]).includes(field)) continue;

    switch (field) {
      case "state":
        rows.push({
          ...base,
          event: labels.state,
          from: lookups.stateName(activity.old_value ?? "") ?? activity.old_value ?? labels.noValue,
          to: lookups.stateName(activity.new_value ?? "") ?? activity.new_value ?? labels.noValue,
        });
        break;
      case "priority":
        rows.push({
          ...base,
          event: labels.priority,
          from: activity.old_value ?? labels.noValue,
          to: activity.new_value ?? labels.noValue,
        });
        break;
      case "assignees": {
        const oldIds = splitIds(activity.old_value);
        const newIds = splitIds(activity.new_value);
        const added = newIds.filter((id) => !oldIds.includes(id)).map((id) => lookups.memberName(id) ?? id);
        const removed = oldIds.filter((id) => !newIds.includes(id)).map((id) => lookups.memberName(id) ?? id);
        if (!added.length && !removed.length) break;
        rows.push({
          ...base,
          event: labels.assignees,
          from: removed.join(", ") || labels.noValue,
          to: added.join(", ") || labels.noValue,
        });
        break;
      }
      case "target_date":
      case "start_date":
      case "archived_at":
        rows.push({
          ...base,
          event: labels[field],
          from: formatDate(activity.old_value) || labels.noValue,
          to: formatDate(activity.new_value) || labels.noValue,
        });
        break;
    }
  }

  return rows;
}
