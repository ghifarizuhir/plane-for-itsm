/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ServicesListView } from "@/components/services";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
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
  // derived values
  const project = getProjectById(projectId);
  const pageTitle = project?.name ? `${project?.name} - ${t("service.title")}` : undefined;
  const workspaceId = currentWorkspace?.id;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || !projectId || fetchedMap[projectId]) return;
    fetchServices(workspaceSlug, workspaceId, projectId);
  }, [workspaceSlug, workspaceId, projectId, fetchedMap, fetchServices]);

  return (
    <>
      <PageHead title={pageTitle} />
      <div className="flex h-full w-full flex-col">
        <ServicesListView />
      </div>
    </>
  );
}

export default observer(ProjectServicesPage);
