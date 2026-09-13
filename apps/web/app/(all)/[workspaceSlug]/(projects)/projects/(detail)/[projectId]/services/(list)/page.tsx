/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TServiceFilters } from "@plane/types";
import { calculateTotalFilters } from "@plane/utils";
// components
import { PageHead } from "@/components/core/page-title";
import { ServiceAppliedFiltersList, ServicesListView } from "@/components/services";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useWorkspace } from "@/hooks/store/use-workspace";
import type { Route } from "./+types/page";

function ProjectServicesPage({ params }: Route.ComponentProps) {
  const { workspaceSlug, projectId } = params;
  // plane hooks
  const { t } = useTranslation();
  // store
  const { getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  const { fetchedMap, fetchServices } = useService();
  const { currentProjectFilters = {}, clearAllFilters, updateFilters } = useServiceFilter();
  // derived values
  const project = getProjectById(projectId);
  const pageTitle = project?.name ? `${project?.name} - ${t("service.title")}` : undefined;
  const workspaceId = currentWorkspace?.id;
  const hasFetched = projectId ? fetchedMap[projectId] : undefined;

  const handleRemoveFilter = useCallback(
    (key: keyof TServiceFilters, value: string | null) => {
      let newValues = [...(currentProjectFilters[key] ?? [])];

      if (!value) newValues = [];
      else newValues = newValues.filter((val) => val !== value);

      updateFilters(projectId, { [key]: newValues });
    },
    [currentProjectFilters, projectId, updateFilters]
  );

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || !projectId || hasFetched) return;
    fetchServices(workspaceSlug, workspaceId, projectId);
  }, [workspaceSlug, workspaceId, projectId, hasFetched, fetchServices]);

  return (
    <>
      <PageHead title={pageTitle} />
      <div className="flex h-full w-full flex-col">
        {calculateTotalFilters(currentProjectFilters) !== 0 && (
          <ServiceAppliedFiltersList
            appliedFilters={currentProjectFilters}
            handleClearAllFilters={() => clearAllFilters(projectId)}
            handleRemoveFilter={handleRemoveFilter}
            alwaysAllowEditing
          />
        )}
        <ServicesListView />
      </div>
    </>
  );
}

export default observer(ProjectServicesPage);
