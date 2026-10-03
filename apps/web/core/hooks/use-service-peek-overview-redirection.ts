/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback } from "react";
// types
import type { IService } from "@plane/types";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
// store
import { useService } from "./store/use-service";

const useServicePeekOverviewRedirection = () => {
  // router
  const router = useAppRouter();
  // store hooks
  const { getIsServicePeeked, setPeekService } = useService();

  const handleRedirection = useCallback(
    (workspaceSlug: string | undefined, service: IService | undefined, isMobile = false) => {
      if (!workspaceSlug || !service) return;
      const { project_id, id } = service;
      if (getIsServicePeeked(id)) return;
      const serviceLink = `/${workspaceSlug}/projects/${project_id}/services/${id}`;
      if (isMobile) router.push(serviceLink);
      else setPeekService({ workspaceSlug, projectId: project_id, serviceId: id });
    },
    [getIsServicePeeked, router, setPeekService]
  );

  return { handleRedirection };
};

export default useServicePeekOverviewRedirection;
