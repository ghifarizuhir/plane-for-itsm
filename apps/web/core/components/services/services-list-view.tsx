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
// components
import { ServicesBoard } from "./board/services-board";
import { ServiceGraph } from "./graph/service-graph";
import { ServiceHealthSummary } from "./health/service-health-summary";
import { CreateUpdateServiceModal } from "./modal";

export const ServicesListView = observer(function ServicesListView() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectServiceIds, getFilteredServiceIds, getProjectHealthSummary, loader } = useService();
  const { currentProjectDisplayFilters, clearAllFilters } = useServiceFilter();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // derived values
  const layout = currentProjectDisplayFilters?.layout ?? "board";
  const projectServiceIds = projectId ? getProjectServiceIds(projectId.toString()) : null;
  const serviceIds = projectId ? getFilteredServiceIds(projectId.toString()) : null;
  const summary = projectId ? getProjectHealthSummary(projectId.toString()) : null;

  const openCreateModal = () => setIsCreateModalOpen(true);
  const closeCreateModal = () => setIsCreateModalOpen(false);

  const renderContent = () => {
    if (loader || projectServiceIds === null || serviceIds === null) {
      return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    }
    if (projectServiceIds.length === 0) {
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
          {projectId && (
            <Button variant="secondary" size="sm" onClick={() => clearAllFilters(projectId.toString())}>
              {t("common.clear_all")}
            </Button>
          )}
        </div>
      );
    }
    if (layout === "graph") {
      return (
        <div className="h-[calc(100vh-12rem)] w-full">
          <ServiceGraph />
        </div>
      );
    }
    return (
      <>
        {summary && <ServiceHealthSummary summary={summary} />}
        <ServicesBoard />
      </>
    );
  };

  return (
    <div className="flex h-full w-full flex-col">
      {renderContent()}
      {workspaceSlug && projectId && (
        <CreateUpdateServiceModal
          isOpen={isCreateModalOpen}
          onClose={closeCreateModal}
          workspaceSlug={workspaceSlug.toString()}
          projectId={projectId.toString()}
        />
      )}
    </div>
  );
});
