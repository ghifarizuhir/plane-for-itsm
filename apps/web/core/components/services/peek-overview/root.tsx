/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import { ServicePeekView } from "./view";

export const ServicePeekOverview = observer(function ServicePeekOverview() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // store hooks
  const { peekService, setPeekService, fetchedMap, errorMap, fetchServices } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString();
  const isMatching = Boolean(peekService && peekService.workspaceSlug === slug && peekService.projectId === pid);

  // Drop the peek when the route moves to another workspace/project
  useEffect(() => {
    if (peekService && !isMatching) setPeekService(undefined);
  }, [peekService, isMatching, setPeekService]);

  // Ensure the service list (and health) is hydrated for this project
  useEffect(() => {
    if (!isMatching || !slug || !pid) return;
    if (fetchedMap[pid] || !currentWorkspace?.id) return;
    void fetchServices(slug, currentWorkspace.id, pid);
  }, [isMatching, slug, pid, fetchedMap, currentWorkspace?.id, fetchServices]);

  if (!isMatching || !peekService || !slug || !pid) return null;

  const status: "loading" | "error" | "ready" = errorMap[pid] ? "error" : fetchedMap[pid] ? "ready" : "loading";

  const handleRetry = () => {
    if (!slug || !pid || !currentWorkspace?.id) return;
    void fetchServices(slug, currentWorkspace.id, pid);
  };

  return (
    <ServicePeekView
      workspaceSlug={slug}
      projectId={pid}
      serviceId={peekService.serviceId}
      status={status}
      onRetry={handleRetry}
    />
  );
});
