/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TServiceFilters } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { EHeaderVariant, Header } from "@plane/ui";
import { calculateTotalFilters } from "@plane/utils";
// components
import { PageHead } from "@/components/core/page-title";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { ServiceAppliedFiltersList, ServicesListView } from "@/components/services";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
// helpers
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";

function WorkspaceServicesPage() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { fetchWorkspaceServices } = useService();
  const {
    clearAllFilters,
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    initProjectServiceFilters,
    updateDisplayFilters,
    updateFilters,
    updateSearchQuery,
  } = useServiceFilter();
  const { getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { allowPermissions } = useUserPermissions();

  // derived values
  const filterKey = WORKSPACE_SERVICE_FILTER_KEY;
  const appliedFilters = getFiltersByProjectId(filterKey) ?? {};
  const displayFilters = getDisplayFiltersByProjectId(filterKey);
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const workspaceId = currentWorkspace?.id;
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.service_view === false);
  const canManageFeatures = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );

  useEffect(() => {
    initProjectServiceFilters(filterKey);
    updateSearchQuery("");
  }, [filterKey, initProjectServiceFilters, updateSearchQuery]);

  useEffect(() => {
    if (scopedProjectId || !displayFilters || displayFilters.layout !== "graph") return;
    updateDisplayFilters(filterKey, { layout: "board" });
  }, [displayFilters, filterKey, scopedProjectId, updateDisplayFilters]);

  useEffect(() => {
    if (!workspaceSlug || !workspaceId) return;
    void fetchWorkspaceServices(workspaceSlug, workspaceId);
  }, [workspaceSlug, workspaceId, fetchWorkspaceServices]);

  const handleRemoveFilter = (key: keyof TServiceFilters, value: string | null) => {
    const currentValues = appliedFilters[key] ?? [];
    const nextValues = value ? currentValues.filter((currentValue) => currentValue !== value) : [];
    updateFilters(filterKey, { [key]: nextValues });
  };

  if (!workspaceSlug) return null;

  if (isFeatureDisabled) {
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title="Services are not enabled for the application."
          description="Enable services from application settings to start mapping dependencies."
          primaryButton={{
            text: t("disabled_project.empty_state.module.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !canManageFeatures,
          }}
        />
      </div>
    );
  }

  return (
    <>
      <PageHead title={t("service.catalog_title")} />
      <div className="flex h-full w-full flex-col">
        {calculateTotalFilters(appliedFilters) !== 0 && (
          <Header variant={EHeaderVariant.TERNARY}>
            <ServiceAppliedFiltersList
              appliedFilters={appliedFilters}
              handleClearAllFilters={() => clearAllFilters(filterKey)}
              handleRemoveFilter={handleRemoveFilter}
              alwaysAllowEditing
            />
          </Header>
        )}
        <ServicesListView
          workspaceMode
          projectIds={scopedProjectId ? [scopedProjectId] : undefined}
          filterKey={filterKey}
          scopeProjectId={scopedProjectId ?? undefined}
        />
      </div>
    </>
  );
}

export default observer(WorkspaceServicesPage);
