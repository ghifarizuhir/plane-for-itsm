/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useWorkspace } from "@/hooks/store/use-workspace";
// components
import { ServicesBoard } from "./board/services-board";
import { ServiceGraph } from "./graph/service-graph";
import { ServiceHealthSummary } from "./health/service-health-summary";
import { CreateUpdateServiceModal } from "./modal";
import { ServicePeekOverview } from "./peek-overview";
import { ServiceLoadErrorState } from "./service-load-error-state";
import { WorkspaceCreateServiceModal } from "./workspace/create-service-modal";
// helpers
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";

type Props = {
  workspaceMode?: boolean;
  projectIds?: string[];
  filterKey?: string;
  scopeProjectId?: string;
};

export const ServicesListView = observer(function ServicesListView({
  workspaceMode = false,
  projectIds,
  filterKey,
  scopeProjectId,
}: Props) {
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    errorMap,
    fetchServices,
    fetchWorkspaceServices,
    getFilteredServiceIds,
    getProjectHealthSummary,
    getProjectServiceIds,
    getWorkspaceFilteredServiceIds,
    getWorkspaceHealthSummary,
    getWorkspaceScopedServiceIds,
    loader,
    workspaceError,
    workspaceFetched,
  } = useService();
  const { getDisplayFiltersByProjectId, clearAllFilters } = useServiceFilter();
  const { currentWorkspace } = useWorkspace();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const openCreateModal = () => setIsCreateModalOpen(true);
  const closeCreateModal = () => setIsCreateModalOpen(false);

  // derived values
  const resolvedFilterKey = filterKey ?? (workspaceMode ? WORKSPACE_SERVICE_FILTER_KEY : projectId?.toString());
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const layout =
    workspaceMode && !scopeProjectId && displayFilters?.layout === "graph"
      ? "board"
      : (displayFilters?.layout ?? "board");
  const totalServiceIds = workspaceMode
    ? getWorkspaceScopedServiceIds(projectIds)
    : projectId
      ? getProjectServiceIds(projectId.toString())
      : null;
  const serviceIds = workspaceMode
    ? getWorkspaceFilteredServiceIds(projectIds)
    : projectId
      ? getFilteredServiceIds(projectId.toString())
      : null;
  const summary = workspaceMode
    ? getWorkspaceHealthSummary(projectIds)
    : projectId
      ? getProjectHealthSummary(projectId.toString())
      : null;

  const workspaceId = currentWorkspace?.id;
  const hasError = workspaceMode ? workspaceError : projectId ? errorMap[projectId.toString()] : false;
  const isLoading = loader && (workspaceMode ? !workspaceFetched : totalServiceIds === null);

  const handleRetry = () => {
    if (!workspaceSlug || !workspaceId) return;
    if (workspaceMode) {
      void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId);
      return;
    }
    if (projectId) void fetchServices(workspaceSlug.toString(), workspaceId, projectId.toString());
  };

  const renderContent = () => {
    if (hasError) {
      return <ServiceLoadErrorState onRetry={handleRetry} />;
    }
    if (isLoading || totalServiceIds === null || serviceIds === null) {
      return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    }
    if (totalServiceIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
          <Button variant="primary" size="sm" onClick={openCreateModal}>
            {t("service.add")}
          </Button>
        </div>
      );
    }
    if (serviceIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.no_matches.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.no_matches.description")}</p>
          {resolvedFilterKey && (
            <Button variant="secondary" size="sm" onClick={() => clearAllFilters(resolvedFilterKey)}>
              {t("common.clear_all")}
            </Button>
          )}
        </div>
      );
    }
    if (layout === "graph" && (workspaceMode ? scopeProjectId : projectId)) {
      return (
        <div className="min-h-0 w-full flex-1">
          <ServiceGraph
            scope={workspaceMode ? "workspace" : "project"}
            projectIdOverride={workspaceMode ? scopeProjectId : undefined}
          />
        </div>
      );
    }
    return (
      <>
        {summary && <ServiceHealthSummary summary={summary} />}
        <ServicesBoard serviceIds={serviceIds} />
      </>
    );
  };

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      {renderContent()}
      <ServicePeekOverview />
      {workspaceMode ? (
        <WorkspaceCreateServiceModal
          isOpen={isCreateModalOpen}
          onClose={closeCreateModal}
          workspaceSlug={workspaceSlug?.toString() ?? ""}
          defaultOwnerProjectId={scopeProjectId}
        />
      ) : (
        workspaceSlug &&
        projectId && (
          <CreateUpdateServiceModal
            isOpen={isCreateModalOpen}
            onClose={closeCreateModal}
            workspaceSlug={workspaceSlug.toString()}
            projectId={projectId.toString()}
          />
        )
      )}
    </div>
  );
});
