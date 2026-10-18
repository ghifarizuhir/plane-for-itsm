/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { MODULE_STATUS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { ModuleStatusIcon } from "@plane/propel/icons";
import type { IModule } from "@plane/types";
import { renderFormattedDate } from "@plane/utils";
// components
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import { applyWorkspaceFilters, groupWorkspaceModules } from "@/lib/workspace-aggregate";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";

type TTrackRowProps = {
  module: IModule;
  workspaceSlug: string;
};

const TrackRow = observer(function TrackRow({ module, workspaceSlug }: TTrackRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();
  const project = getProjectById(module.project_id);
  const lead = module.lead_id ? getUserDetails(module.lead_id) : undefined;
  const progress = module.total_issues > 0 ? Math.round((module.completed_issues / module.total_issues) * 100) : 0;
  const estimatePoints =
    (module.total_estimate_points ?? 0) > 0
      ? `${module.completed_estimate_points ?? 0}/${module.total_estimate_points ?? 0} pts`
      : null;

  return (
    <ListItem
      title={module.name}
      itemLink={`/${workspaceSlug}/projects/${module.project_id}/modules/${module.id}`}
      prependTitleElement={<ModuleStatusIcon status={module.status ?? "backlog"} />}
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          {lead?.display_name && <span className="hidden lg:inline">{lead.display_name}</span>}
          {module.target_date && <span className="hidden md:inline">{renderFormattedDate(module.target_date)}</span>}
          {estimatePoints && <span className="hidden lg:inline">{estimatePoints}</span>}
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

export const WorkspaceTracksListView = observer(function WorkspaceTracksListView({ projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { moduleMap, fetchWorkspaceModules } = useModule();
  const { workspaceProjectIds, getProjectById } = useProject();
  const [search, setSearch] = useState("");
  const [isLoading, setIsLoading] = useState(true);

  const slug = workspaceSlug?.toString();

  useEffect(() => {
    if (!slug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceModules(slug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slug, fetchWorkspaceModules]);

  const filteredModules = useMemo(() => {
    const projectIds = new Set(workspaceProjectIds ?? []);
    const modules = Object.values(moduleMap ?? {}).filter((module) => !module.archived_at);
    const projectAllowed = (projectId: string) => {
      if (!projectIds.has(projectId)) return false;
      const project = getProjectById(projectId);
      return project ? project.module_view !== false : true;
    };
    return applyWorkspaceFilters(modules, { projectId: projectFilter, search, projectAllowed });
  }, [moduleMap, workspaceProjectIds, getProjectById, projectFilter, search]);

  const groups = useMemo(() => groupWorkspaceModules(filteredModules), [filteredModules]);

  const statusLabels = new Map(MODULE_STATUS.map((status) => [status.value, t(status.i18n_label)]));

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: statusLabels.get(group.key) ?? group.key,
    count: group.items.length,
    items: (
      <>
        {group.items.map((module) => (
          <TrackRow key={module.id} module={module} workspaceSlug={slug ?? ""} />
        ))}
      </>
    ),
  }));

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    router.push(projectId ? `/${slug}/tracks?project=${projectId}` : `/${slug}/tracks`);
  };

  return (
    <WorkspaceAggregateList
      projectFilter={projectFilter}
      onProjectFilterChange={handleProjectFilterChange}
      search={search}
      onSearchChange={setSearch}
      searchPlaceholder={t("service.catalog_search_placeholder")}
      groups={renderedGroups}
      totalCount={filteredModules.length}
      isLoading={isLoading}
      emptyTitle={t("workspace_empty_state.tracks.title")}
      emptyDescription={t("workspace_empty_state.tracks.description")}
    />
  );
});
