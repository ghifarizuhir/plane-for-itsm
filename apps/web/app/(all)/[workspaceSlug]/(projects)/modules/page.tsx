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
import type { TModuleFilters } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { EHeaderVariant, Header } from "@plane/ui";
import { calculateTotalFilters } from "@plane/utils";
// assets
import darkModulesAsset from "@/app/assets/empty-state/disabled-feature/modules-dark.webp?url";
import lightModulesAsset from "@/app/assets/empty-state/disabled-feature/modules-light.webp?url";
// components
import { PageHead } from "@/components/core/page-title";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { ModuleAppliedFiltersList, ModulesListView } from "@/components/modules";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useModule } from "@/hooks/store/use-module";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceModulePermissions } from "@/hooks/use-workspace-module-permissions";
// helpers
import { WORKSPACE_MODULE_FILTER_KEY } from "@/lib/workspace-modules";

function WorkspaceModulesPage() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // theme hook
  const { resolvedTheme } = useTheme();
  // plane hooks
  const { t } = useTranslation();
  // states
  const [isLoading, setIsLoading] = useState(true);
  // store hooks
  const { moduleMap, fetchWorkspaceModules } = useModule();
  const {
    clearAllFilters,
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    initProjectModuleFilters,
    updateDisplayFilters,
    updateFilters,
    updateSearchQuery,
  } = useModuleFilter();
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateModule } = useWorkspaceModulePermissions();
  const { toggleCreateModuleModal } = useCommandPalette();
  const { allowPermissions } = useUserPermissions();

  // derived values
  const filterKey = WORKSPACE_MODULE_FILTER_KEY;
  const appliedFilters = getFiltersByProjectId(filterKey) ?? {};
  const displayFilters = getDisplayFiltersByProjectId(filterKey);
  const projectIds = useMemo(
    () => (scopedProjectId ? [scopedProjectId] : (workspaceProjectIds ?? [])),
    [scopedProjectId, workspaceProjectIds]
  );
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const resolvedEmptyState = resolvedTheme === "light" ? lightModulesAsset : darkModulesAsset;
  const hasAdminLevelPermission = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.module_view === false);
  const totalModules = useMemo(
    () =>
      Object.values(moduleMap ?? {}).filter((module) => {
        if (module.archived_at || !projectIds.includes(module.project_id)) return false;
        const project = getProjectById(module.project_id);
        return project ? project.module_view !== false : true;
      }).length,
    [moduleMap, getProjectById, projectIds]
  );

  useEffect(() => {
    initProjectModuleFilters(filterKey);
    updateSearchQuery("");
  }, [filterKey, initProjectModuleFilters, updateSearchQuery]);

  useEffect(() => {
    if (scopedProjectId || !displayFilters || displayFilters.layout !== "gantt") return;
    updateDisplayFilters(filterKey, { layout: "list" });
  }, [displayFilters, filterKey, scopedProjectId, updateDisplayFilters]);

  useEffect(() => {
    if (!workspaceSlug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceModules(workspaceSlug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [fetchWorkspaceModules, workspaceSlug]);

  const handleRemoveFilter = (key: keyof TModuleFilters, value: string | null) => {
    const currentValues = appliedFilters[key] ?? [];
    const nextValues = value ? currentValues.filter((currentValue) => currentValue !== value) : [];
    updateFilters(filterKey, { [key]: nextValues });
  };

  if (!workspaceSlug) return null;
  if (isLoading || isInitializingProjects || workspaceProjectIds === undefined) return <CycleModuleListLayoutLoader />;

  if (isFeatureDisabled)
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title={t("disabled_project.empty_state.module.title")}
          description={t("disabled_project.empty_state.module.description")}
          assetPath={resolvedEmptyState}
          primaryButton={{
            text: t("disabled_project.empty_state.module.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !hasAdminLevelPermission,
          }}
        />
      </div>
    );

  return (
    <>
      <PageHead title={t("sidebar.modules")} />
      <div className="flex h-full w-full flex-col">
        {totalModules === 0 ? (
          <div className="h-full place-items-center">
            {scopedProjectId ? (
              <EmptyStateDetailed
                assetKey="module"
                title={t("project_empty_state.modules.title")}
                description={t("project_empty_state.modules.description")}
                actions={[
                  {
                    label: t("project_empty_state.modules.cta_primary"),
                    onClick: () => toggleCreateModuleModal(true),
                    variant: "primary",
                    disabled: !canCreateModule,
                  },
                ]}
              />
            ) : (
              <EmptyStateDetailed
                assetKey="module"
                title={t("workspace_empty_state.tracks.title")}
                description={t("workspace_empty_state.tracks.description")}
              />
            )}
          </div>
        ) : (
          <>
            {(calculateTotalFilters(appliedFilters) !== 0 || displayFilters?.favorites) && (
              <Header variant={EHeaderVariant.TERNARY}>
                <ModuleAppliedFiltersList
                  appliedFilters={appliedFilters}
                  isFavoriteFilterApplied={displayFilters?.favorites ?? false}
                  handleClearAllFilters={() => clearAllFilters(filterKey)}
                  handleRemoveFilter={handleRemoveFilter}
                  handleDisplayFiltersUpdate={(val) => updateDisplayFilters(filterKey, val)}
                  alwaysAllowEditing
                />
              </Header>
            )}
            <ModulesListView
              projectId={scopedProjectId ?? undefined}
              projectIds={projectIds}
              filterKey={filterKey}
              isGanttDisabled={!scopedProjectId}
              canCreate={canCreateModule}
            />
          </>
        )}
      </div>
    </>
  );
}

export default observer(WorkspaceModulesPage);
