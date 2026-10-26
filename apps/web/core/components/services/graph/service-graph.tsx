/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TServiceGraphData } from "@plane/types";
// hooks
import { usePlatformOS } from "@/hooks/use-platform-os";
import useServicePeekOverviewRedirection from "@/hooks/use-service-peek-overview-redirection";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// helpers
import { isLiteService } from "@/services/service.helpers";
// components
import { ServiceGraphCanvas } from "./service-graph-canvas";

export const ServiceGraph = observer(function ServiceGraph({
  scope = "project",
  projectIdOverride,
}: {
  scope?: "project" | "workspace";
  projectIdOverride?: string;
}) {
  // router
  const { workspaceSlug, projectId } = useParams();
  // store hooks
  const {
    getGraphData,
    getWorkspaceGraphData,
    getServiceById,
    addDependency,
    removeDependency,
    updateNodePosition,
    layoutEpoch,
    markLayoutChanged,
  } = useService();
  const { projectMap } = useProject();
  const { currentWorkspace } = useWorkspace();
  // peek overview
  const { isMobile } = usePlatformOS();
  const { handleRedirection } = useServicePeekOverviewRedirection();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectIdOverride ?? projectId?.toString();
  const projectScope = projectIdOverride ?? projectId?.toString() ?? null;
  const workspaceId = currentWorkspace?.id;
  // states
  const [isReLayouting, setIsReLayouting] = useState(false);

  const graphData: TServiceGraphData =
    scope === "workspace"
      ? getWorkspaceGraphData(projectIdOverride)
      : pid
        ? getGraphData(pid)
        : { services: [], dependencies: [], health: {} };

  const projectLookup = useMemo(() => {
    const lookup: Record<string, { identifier: string; name: string }> = {};
    Object.values(projectMap).forEach((proj) => {
      if (proj?.id && proj.identifier) {
        lookup[proj.id] = { identifier: proj.identifier, name: proj.name };
      }
    });
    return lookup;
  }, [projectMap]);

  const handleConnect = useCallback(
    async (source: string, target: string) => {
      if (!slug || !workspaceId) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not create dependency. Please try again.",
        });
        return;
      }
      try {
        await addDependency(slug, workspaceId, projectScope, source, target);
      } catch (error) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: error instanceof Error ? error.message : "Could not create dependency. Please try again.",
        });
      }
    },
    [addDependency, projectScope, slug, workspaceId]
  );

  const handleEdgeDelete = useCallback(
    (dependencyId: string) => {
      if (!slug || !workspaceId) return;
      void removeDependency(slug, workspaceId, projectScope, dependencyId).catch(() => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not delete dependency. Please try again.",
        });
      });
    },
    [projectScope, removeDependency, slug, workspaceId]
  );

  const handleNodeDragStop = useCallback(
    (serviceId: string, position: { x: number; y: number }) => {
      const targetProjectId: string | null = scope === "workspace" ? null : (pid ?? null);
      if (!slug || !workspaceId || (scope === "project" && !targetProjectId)) return;
      void updateNodePosition(slug, workspaceId, targetProjectId, serviceId, position).catch(() => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not save node position. Please try again.",
        });
      });
    },
    [pid, scope, slug, updateNodePosition, workspaceId]
  );

  const handleNodeOpen = useCallback(
    (serviceId: string) => {
      if (!slug) return;
      const service = getServiceById(serviceId);
      if (!service) return;
      handleRedirection(slug, service, isMobile);
    },
    [getServiceById, handleRedirection, isMobile, slug]
  );

  const handleReLayout = useCallback(async () => {
    const targetProjectId: string | null = scope === "workspace" ? null : (pid ?? null);
    if (!slug || !workspaceId || (scope === "project" && !targetProjectId)) return;
    const positioned = graphData.services.filter((service) => !isLiteService(service) && service.position !== null);
    setIsReLayouting(true);
    try {
      if (positioned.length > 0) {
        const results = await Promise.allSettled(
          positioned.map((service) => updateNodePosition(slug, workspaceId, targetProjectId, service.id, null))
        );
        if (results.some((result) => result.status === "rejected")) {
          setToast({
            type: TOAST_TYPE.ERROR,
            title: "Error!",
            message: "Could not reset layout. Please try again.",
          });
        }
      }
    } finally {
      markLayoutChanged();
      setIsReLayouting(false);
    }
  }, [graphData.services, markLayoutChanged, pid, scope, slug, updateNodePosition, workspaceId]);

  return (
    <div className="relative h-full w-full">
      <div className="absolute inset-0">
        <ServiceGraphCanvas
          graphData={graphData}
          projectLookup={projectLookup}
          layoutEpoch={layoutEpoch}
          onConnect={handleConnect}
          onEdgeDelete={handleEdgeDelete}
          onNodeDragStop={handleNodeDragStop}
          onNodeOpen={handleNodeOpen}
          onReLayout={handleReLayout}
          isReLayouting={isReLayouting}
        />
      </div>
    </div>
  );
});
