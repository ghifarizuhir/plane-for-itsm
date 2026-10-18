/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CycleGroupIcon } from "@plane/propel/icons";
import type { ICycle, TCycleGroups } from "@plane/types";
import { calculateCycleProgress, renderFormattedDate } from "@plane/utils";
// components
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import { applyWorkspaceFilters, deriveCycleStatus, groupWorkspaceCycles } from "@/lib/workspace-aggregate";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";

type TSprintRowProps = {
  cycle: ICycle;
  workspaceSlug: string;
};

const SprintRow = observer(function SprintRow({ cycle, workspaceSlug }: TSprintRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const project = getProjectById(cycle.project_id);
  const status = (cycle.status?.toLowerCase() as TCycleGroups | undefined) ?? deriveCycleStatus(cycle);
  const progress = calculateCycleProgress(cycle);
  const dateRange =
    cycle.start_date && cycle.end_date
      ? `${renderFormattedDate(cycle.start_date)} - ${renderFormattedDate(cycle.end_date)}`
      : null;

  return (
    <ListItem
      title={cycle.name}
      itemLink={`/${workspaceSlug}/projects/${cycle.project_id}/cycles/${cycle.id}`}
      prependTitleElement={<CycleGroupIcon cycleGroup={status} className="h-4 w-4 flex-shrink-0" />}
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          {dateRange && <span className="hidden md:inline">{dateRange}</span>}
          <span>{`${progress}%`}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  projectFilter?: string;
};

export const WorkspaceSprintsListView = observer(function WorkspaceSprintsListView({ projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { cycleMap, fetchWorkspaceCycles } = useCycle();
  const { workspaceProjectIds, getProjectById } = useProject();
  const [search, setSearch] = useState("");
  const [isLoading, setIsLoading] = useState(true);

  const slug = workspaceSlug?.toString();

  useEffect(() => {
    if (!slug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceCycles(slug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slug, fetchWorkspaceCycles]);

  const filteredCycles = useMemo(() => {
    const projectIds = new Set(workspaceProjectIds ?? []);
    const cycles = Object.values(cycleMap ?? {}).filter((cycle) => !cycle.archived_at);
    const projectAllowed = (projectId: string) => {
      if (!projectIds.has(projectId)) return false;
      const project = getProjectById(projectId);
      return project ? project.cycle_view !== false : true;
    };
    return applyWorkspaceFilters(cycles, { projectId: projectFilter, search, projectAllowed });
  }, [cycleMap, workspaceProjectIds, getProjectById, projectFilter, search]);

  const groups = useMemo(() => groupWorkspaceCycles(filteredCycles), [filteredCycles]);

  const groupLabels: Record<TCycleGroups, string> = {
    current: t("project_cycles.active_cycle.label"),
    upcoming: t("project_cycles.upcoming_cycle.label"),
    completed: t("project_cycles.completed_cycle.label"),
    draft: t("project_cycles.status.draft"),
  };

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: groupLabels[group.key],
    count: group.items.length,
    items: (
      <>
        {group.items.map((cycle) => (
          <SprintRow key={cycle.id} cycle={cycle} workspaceSlug={slug ?? ""} />
        ))}
      </>
    ),
  }));

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    router.push(projectId ? `/${slug}/sprints?project=${projectId}` : `/${slug}/sprints`);
  };

  return (
    <WorkspaceAggregateList
      projectFilter={projectFilter}
      onProjectFilterChange={handleProjectFilterChange}
      search={search}
      onSearchChange={setSearch}
      searchPlaceholder={t("service.catalog_search_placeholder")}
      groups={renderedGroups}
      totalCount={filteredCycles.length}
      isLoading={isLoading}
      emptyTitle={t("workspace_empty_state.sprints.title")}
      emptyDescription={t("workspace_empty_state.sprints.description")}
    />
  );
});
