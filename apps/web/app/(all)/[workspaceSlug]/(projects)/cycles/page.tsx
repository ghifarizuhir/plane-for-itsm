/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useTheme } from "next-themes";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import type { TCycleFilters } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { EHeaderVariant, Header } from "@plane/ui";
import { calculateTotalFilters } from "@plane/utils";
// assets
import darkEmptyState from "@/app/assets/empty-state/disabled-feature/cycles-dark.webp?url";
import lightEmptyState from "@/app/assets/empty-state/disabled-feature/cycles-light.webp?url";
// components
import { PageHead } from "@/components/core/page-title";
import { CycleAppliedFiltersList } from "@/components/cycles/applied-filters";
import { CyclesView } from "@/components/cycles/cycles-view";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useCycle } from "@/hooks/store/use-cycle";
import { useCycleFilter } from "@/hooks/store/use-cycle-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceCyclePermissions } from "@/hooks/use-workspace-cycle-permissions";
// helpers
import { WORKSPACE_CYCLE_FILTER_KEY } from "@/lib/workspace-cycles";

function WorkspaceSprintsPage() {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const router = useAppRouter();
  // theme
  const { resolvedTheme } = useTheme();
  // states
  const [isLoading, setIsLoading] = useState(true);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { cycleMap, fetchWorkspaceCycles } = useCycle();
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateCycle } = useWorkspaceCyclePermissions();
  const { toggleCreateCycleModal } = useCommandPalette();
  const { clearAllFilters, getFiltersByProjectId, initProjectCycleFilters, updateFilters, updateSearchQuery } =
    useCycleFilter();
  const { allowPermissions } = useUserPermissions();

  // derived values
  const filterKey = WORKSPACE_CYCLE_FILTER_KEY;
  const appliedFilters = getFiltersByProjectId(filterKey);
  const projectIds = useMemo(
    () => (scopedProjectId ? [scopedProjectId] : (workspaceProjectIds ?? [])),
    [scopedProjectId, workspaceProjectIds]
  );
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const resolvedEmptyState = resolvedTheme === "light" ? lightEmptyState : darkEmptyState;
  const hasAdminLevelPermission = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.cycle_view === false);
  const totalCycles = useMemo(
    () =>
      Object.values(cycleMap ?? {}).filter((cycle) => {
        if (cycle.archived_at || !projectIds.includes(cycle.project_id)) return false;
        const project = getProjectById(cycle.project_id);
        return project ? project.cycle_view !== false : true;
      }).length,
    [cycleMap, getProjectById, projectIds]
  );

  useEffect(() => {
    initProjectCycleFilters(filterKey);
    updateSearchQuery("");
  }, [filterKey, initProjectCycleFilters, updateSearchQuery]);

  useEffect(() => {
    if (!workspaceSlug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceCycles(workspaceSlug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [fetchWorkspaceCycles, workspaceSlug]);

  const handleRemoveFilter = (key: keyof TCycleFilters, value: string | null) => {
    const currentValues = appliedFilters?.[key] ?? [];
    const nextValues = value ? currentValues.filter((currentValue) => currentValue !== value) : [];
    updateFilters(filterKey, { [key]: nextValues });
  };

  if (!workspaceSlug) return null;
  if (isLoading || isInitializingProjects || workspaceProjectIds === undefined) return <CycleModuleListLayoutLoader />;

  if (isFeatureDisabled)
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title={t("disabled_project.empty_state.cycle.title")}
          description={t("disabled_project.empty_state.cycle.description")}
          assetPath={resolvedEmptyState}
          primaryButton={{
            text: t("disabled_project.empty_state.cycle.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !hasAdminLevelPermission,
          }}
        />
      </div>
    );

  return (
    <>
      <PageHead title={t("sidebar.cycles")} />
      <div className="h-full w-full">
        {totalCycles === 0 ? (
          <div className="h-full place-items-center">
            {scopedProjectId ? (
              <EmptyStateDetailed
                assetKey="cycle"
                title={t("project_empty_state.cycles.title")}
                description={t("project_empty_state.cycles.description")}
                actions={[
                  {
                    label: t("project_empty_state.cycles.cta_primary"),
                    onClick: () => toggleCreateCycleModal(true),
                    variant: "primary",
                    disabled: !canCreateCycle,
                  },
                ]}
              />
            ) : (
              <EmptyStateDetailed
                assetKey="cycle"
                title={t("workspace_empty_state.sprints.title")}
                description={t("workspace_empty_state.sprints.description")}
              />
            )}
          </div>
        ) : (
          <>
            {calculateTotalFilters(appliedFilters ?? {}) !== 0 && (
              <Header variant={EHeaderVariant.TERNARY}>
                <CycleAppliedFiltersList
                  appliedFilters={appliedFilters ?? {}}
                  handleClearAllFilters={() => clearAllFilters(filterKey)}
                  handleRemoveFilter={handleRemoveFilter}
                  alwaysAllowEditing
                />
              </Header>
            )}
            <CyclesView
              workspaceSlug={workspaceSlug}
              projectId={scopedProjectId ?? undefined}
              projectIds={projectIds}
              filterKey={filterKey}
            />
          </>
        )}
      </div>
    </>
  );
}

export default observer(WorkspaceSprintsPage);
