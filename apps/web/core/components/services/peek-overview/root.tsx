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
  const {
    peekService,
    setPeekService,
    fetchedMap,
    errorMap,
    fetchServices,
    workspaceFetched,
    workspaceError,
    fetchWorkspaceServices,
  } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString();
  const workspaceId = currentWorkspace?.id;
  // Project routes scope the peek to their project; the workspace catalog has no
  // projectId param, so any peek in the same workspace belongs to that surface.
  const isMatching = Boolean(
    peekService && peekService.workspaceSlug === slug && (pid ? peekService.projectId === pid : true)
  );
  const status: "loading" | "error" | "ready" = pid
    ? errorMap[pid]
      ? "error"
      : fetchedMap[pid]
        ? "ready"
        : "loading"
    : workspaceError
      ? "error"
      : workspaceFetched
        ? "ready"
        : "loading";

  // Drop the peek when the route moves to another workspace/project
  useEffect(() => {
    if (peekService && !isMatching) setPeekService(undefined);
  }, [peekService, isMatching, setPeekService]);

  // Ensure the list (and health) behind the peek is hydrated for this scope
  useEffect(() => {
    if (!isMatching || !slug || !workspaceId) return;
    if (pid) {
      if (fetchedMap[pid]) return;
      void fetchServices(slug, workspaceId, pid);
      return;
    }
    if (workspaceFetched) return;
    void fetchWorkspaceServices(slug, workspaceId);
  }, [isMatching, slug, pid, workspaceId, fetchedMap, workspaceFetched, fetchServices, fetchWorkspaceServices]);

  if (!isMatching || !peekService || !slug) return null;

  const handleRetry = () => {
    if (!slug || !workspaceId) return;
    if (pid) {
      void fetchServices(slug, workspaceId, pid);
      return;
    }
    void fetchWorkspaceServices(slug, workspaceId);
  };

  return (
    <ServicePeekView
      workspaceSlug={slug}
      projectId={peekService.projectId || pid || ""}
      serviceId={peekService.serviceId}
      status={status}
      onRetry={handleRetry}
    />
  );
});
