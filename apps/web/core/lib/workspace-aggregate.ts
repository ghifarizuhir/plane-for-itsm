/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { MODULE_STATUS } from "@plane/constants";
import type { TCycleGroups, TModuleStatus } from "@plane/types";
import { getDate } from "@plane/utils";

export type TWorkspaceCycle = {
  id: string;
  name: string;
  project_id: string;
  start_date: string | null;
  end_date: string | null;
};

export type TWorkspaceModule = {
  id: string;
  name: string;
  project_id: string;
  status?: TModuleStatus;
  target_date: string | null;
};

export type TWorkspaceAggregateGroup<T, K extends string> = {
  key: K;
  items: T[];
};

export type TWorkspaceFilterOptions = {
  projectId?: string;
  search?: string;
  projectAllowed?: (projectId: string) => boolean;
};

export const WORKSPACE_SPRINT_GROUP_ORDER: TCycleGroups[] = ["draft", "current", "upcoming", "completed"];

const toMidnight = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate());

const dateValue = (date: string | null | undefined) => {
  const parsed = getDate(date);
  const time = parsed?.getTime();
  return time !== undefined && !Number.isNaN(time) ? time : Number.POSITIVE_INFINITY;
};

/**
 * Derives the sprint (cycle) group from its dates. The workspace cycles
 * endpoint does not return `status`, so the client derives it the same way
 * the project UI's server-provided status behaves.
 */
export function deriveCycleStatus(
  cycle: Pick<TWorkspaceCycle, "start_date" | "end_date">,
  now: Date = new Date()
): TCycleGroups {
  const start = getDate(cycle.start_date);
  const end = getDate(cycle.end_date);
  if (!start || !end || Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) return "draft";
  const startDay = toMidnight(start);
  const endDay = toMidnight(end);
  if (startDay > endDay) return "draft";
  const today = toMidnight(now);
  if (endDay < today) return "completed";
  if (startDay > today) return "upcoming";
  return "current";
}

export function groupWorkspaceCycles<T extends TWorkspaceCycle>(
  cycles: T[],
  now: Date = new Date()
): TWorkspaceAggregateGroup<T, TCycleGroups>[] {
  const buckets: Record<TCycleGroups, T[]> = { draft: [], current: [], upcoming: [], completed: [] };
  cycles.forEach((cycle) => buckets[deriveCycleStatus(cycle, now)].push(cycle));
  const groups: TWorkspaceAggregateGroup<T, TCycleGroups>[] = [];
  for (const key of WORKSPACE_SPRINT_GROUP_ORDER) {
    const items = [...buckets[key]];
    if (key === "current") items.sort((a, b) => dateValue(a.end_date) - dateValue(b.end_date));
    if (key === "upcoming") items.sort((a, b) => dateValue(a.start_date) - dateValue(b.start_date));
    if (key === "completed") items.sort((a, b) => dateValue(b.end_date) - dateValue(a.end_date));
    if (items.length > 0) groups.push({ key, items });
  }
  return groups;
}

export function groupWorkspaceModules<T extends TWorkspaceModule>(
  modules: T[]
): TWorkspaceAggregateGroup<T, TModuleStatus>[] {
  const groups: TWorkspaceAggregateGroup<T, TModuleStatus>[] = [];
  for (const status of MODULE_STATUS) {
    const items = modules.filter((module) => (module.status ?? "backlog") === status.value);
    items.sort((a, b) => {
      const targetDiff = dateValue(a.target_date) - dateValue(b.target_date);
      if (targetDiff !== 0) return targetDiff;
      return a.name.localeCompare(b.name);
    });
    if (items.length > 0) groups.push({ key: status.value, items });
  }
  return groups;
}

export function applyWorkspaceFilters<T extends { name: string; project_id: string }>(
  items: T[],
  { projectId, search, projectAllowed }: TWorkspaceFilterOptions
): T[] {
  const normalizedSearch = search?.trim().toLowerCase() ?? "";
  return items.filter((item) => {
    if (projectAllowed && !projectAllowed(item.project_id)) return false;
    if (projectId && item.project_id !== projectId) return false;
    if (normalizedSearch && !item.name.toLowerCase().includes(normalizedSearch)) return false;
    return true;
  });
}
