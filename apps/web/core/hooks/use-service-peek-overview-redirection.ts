/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback } from "react";
import { useParams } from "next/navigation";
// types
import type { IService } from "@plane/types";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
// store
import { useService } from "./store/use-service";

const useServicePeekOverviewRedirection = () => {
  // router
  const router = useAppRouter();
  const { projectId } = useParams();
  // store hooks
  const { getIsServicePeeked, setPeekService } = useService();

  const handleRedirection = useCallback(
    (workspaceSlug: string | undefined, service: IService | undefined, isMobile = false) => {
      if (!workspaceSlug || !service) return;
      const { project_id, id } = service;
      const serviceLink = `/${workspaceSlug}/services/${id}`;
      // Workspace catalog route has no projectId param and renders no peek panel.
      // Navigate directly to the detail page instead of setting peek state that nobody consumes.
      if (!projectId) {
        router.push(serviceLink);
        return;
      }
      if (getIsServicePeeked(id)) return;
      if (isMobile) router.push(serviceLink);
      else setPeekService({ workspaceSlug, projectId: project_id ?? "", serviceId: id });
    },
    [getIsServicePeeked, projectId, router, setPeekService]
  );

  return { handleRedirection };
};

export default useServicePeekOverviewRedirection;
