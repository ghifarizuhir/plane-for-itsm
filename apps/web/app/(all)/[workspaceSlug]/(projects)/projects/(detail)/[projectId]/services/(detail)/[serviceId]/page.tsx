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
import { ServiceDetailRoot } from "@/components/services";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import type { Route } from "./+types/page";

function ProjectServiceDetailPage({ params }: Route.ComponentProps) {
  const { workspaceSlug, projectId, serviceId } = params;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  const { fetchedMap, fetchServices, getServiceById } = useService();
  // derived values
  const project = getProjectById(projectId);
  const service = getServiceById(serviceId);
  const pageTitle =
    project?.name && service?.name
      ? `${project?.name} - ${service?.name}`
      : project?.name
        ? `${project?.name} - ${t("service.title")}`
        : undefined;
  const workspaceId = currentWorkspace?.id;
  const hasFetched = projectId ? fetchedMap[projectId] : undefined;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || !projectId || hasFetched) return;
    fetchServices(workspaceSlug, workspaceId, projectId);
  }, [workspaceSlug, workspaceId, projectId, hasFetched, fetchServices]);

  return (
    <>
      <PageHead title={pageTitle} />
      <ServiceDetailRoot serviceId={serviceId} />
    </>
  );
}

export default observer(ProjectServiceDetailPage);
