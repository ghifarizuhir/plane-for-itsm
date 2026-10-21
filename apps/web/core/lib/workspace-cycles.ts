/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ICycle, TCycleFilters, TCycleGroups } from "@plane/types";
import { getDate, shouldFilterCycle } from "@plane/utils";
// local imports
import { deriveCycleStatus } from "./workspace-aggregate";

export const WORKSPACE_CYCLE_FILTER_KEY = "__workspace__";

export type TWorkspaceCycleSelection = {
  currentCycleIds: string[];
  upcomingCycleIds: string[];
  draftCycleIds: string[];
  completedCycleIds: string[];
  totalCount: number;
};

type TSelectWorkspaceCyclesArgs = {
  cycles: ICycle[];
  projectIds: string[];
  filters: TCycleFilters;
  searchQuery: string;
  projectAllowed?: (projectId: string) => boolean;
  now?: Date;
};

const dateValue = (date: string | null | undefined) => {
  const parsed = getDate(date);
  const time = parsed?.getTime();
  return time !== undefined && !Number.isNaN(time) ? time : Number.POSITIVE_INFINITY;
};

const CYCLE_GROUPS = new Set<TCycleGroups>(["current", "upcoming", "draft", "completed"]);

const isCycleGroup = (status: string): status is TCycleGroups => CYCLE_GROUPS.has(status as TCycleGroups);

const byName = (a: ICycle, b: ICycle) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
const byStartDate = (a: ICycle, b: ICycle) => dateValue(a.start_date) - dateValue(b.start_date);
const completedFirst = (a: ICycle, b: ICycle) => Number(!a.start_date) - Number(!b.start_date);

/**
 * The workspace cycles endpoint does not always return `status`. Normalize it
 * from the date range so every row/action behaves like a project cycle.
 */
export const normalizeWorkspaceCycle = (cycle: ICycle, now: Date = new Date()): ICycle => {
  const explicitStatus = typeof cycle.status === "string" ? cycle.status.trim().toLowerCase() : undefined;
  const status = explicitStatus && isCycleGroup(explicitStatus) ? explicitStatus : deriveCycleStatus(cycle, now);
  return cycle.status === status ? cycle : { ...cycle, status };
};

export const selectWorkspaceCycles = ({
  cycles,
  projectIds,
  filters,
  searchQuery,
  projectAllowed,
  now = new Date(),
}: TSelectWorkspaceCyclesArgs): TWorkspaceCycleSelection => {
  const projectIdSet = new Set(projectIds);
  const normalizedSearch = searchQuery.trim().toLowerCase();

  const filteredCycles = cycles
    .map((cycle) => normalizeWorkspaceCycle(cycle, now))
    .filter((cycle) => projectIdSet.has(cycle.project_id))
    .filter((cycle) => !cycle.archived_at)
    .filter((cycle) => (projectAllowed ? projectAllowed(cycle.project_id) : true))
    .filter((cycle) => cycle.name.toLowerCase().includes(normalizedSearch))
    .filter((cycle) => shouldFilterCycle(cycle, filters));

  const groups: Record<TCycleGroups, ICycle[]> = {
    current: [],
    upcoming: [],
    draft: [],
    completed: [],
  };

  filteredCycles.forEach((cycle) => {
    const status = (cycle.status ?? "draft") as TCycleGroups;
    const bucket = isCycleGroup(status) ? groups[status] : groups.draft;
    bucket.push(cycle);
  });

  groups.current.sort(byName);
  groups.upcoming.sort(byStartDate);
  groups.draft.sort(byName);
  groups.completed.sort(completedFirst);

  return {
    currentCycleIds: groups.current.map((cycle) => cycle.id),
    upcomingCycleIds: groups.upcoming.map((cycle) => cycle.id),
    draftCycleIds: groups.draft.map((cycle) => cycle.id),
    completedCycleIds: groups.completed.map((cycle) => cycle.id),
    totalCount: filteredCycles.length,
  };
};
