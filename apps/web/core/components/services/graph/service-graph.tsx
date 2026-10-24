/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
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
import { ServiceHealthDot } from "../health/service-health-dot";
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
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getGraphData,
    getWorkspaceGraphData,
    getServiceById,
    addDependency,
    removeDependency,
    updateNodePosition,
    updateService,
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
      if (!slug || !workspaceId || !pid) return;
      void updateNodePosition(slug, workspaceId, pid, serviceId, position).catch(() => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not save node position. Please try again.",
        });
      });
    },
    [pid, slug, updateNodePosition, workspaceId]
  );

  const handleNodeClick = useCallback(
    (serviceId: string) => {
      if (!slug) return;
      const service = getServiceById(serviceId);
      if (!service) return;
      handleRedirection(slug, service, isMobile);
    },
    [getServiceById, handleRedirection, isMobile, slug]
  );

  const handleReLayout = useCallback(async () => {
    if (!slug || !workspaceId || !pid) return;
    const positioned = graphData.services.filter((service) => !isLiteService(service) && service.position !== null);
    if (positioned.length === 0) return;
    setIsReLayouting(true);
    try {
      await Promise.all(
        positioned.map((service) => updateService(slug, workspaceId, pid, service.id, { position: null }))
      );
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: "Could not reset layout. Please try again.",
      });
    } finally {
      setIsReLayouting(false);
    }
  }, [graphData.services, pid, slug, updateService, workspaceId]);

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-subtle px-3 py-2">
        <div className="flex flex-wrap items-center gap-4">
          <p className="text-12 text-secondary">{t("service.graph.connect_hint")}</p>
          <div className="hidden items-center gap-3 border-l border-subtle pl-4 sm:flex">
            <span className="inline-flex items-center gap-1.5 text-11 text-secondary">
              <ServiceHealthDot health="healthy" />
              <span>{t("service.health_values.healthy")}</span>
            </span>
            <span className="inline-flex items-center gap-1.5 text-11 text-secondary">
              <ServiceHealthDot health="degraded" />
              <span>{t("service.health_values.degraded")}</span>
            </span>
            <span className="inline-flex items-center gap-1.5 text-11 text-secondary">
              <ServiceHealthDot health="down" />
              <span>{t("service.health_values.down")}</span>
            </span>
          </div>
        </div>
        <Button variant="secondary" size="sm" onClick={handleReLayout} loading={isReLayouting}>
          {t("service.graph.re_layout")}
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <ServiceGraphCanvas
          graphData={graphData}
          projectLookup={projectLookup}
          onConnect={handleConnect}
          onEdgeDelete={handleEdgeDelete}
          onNodeDragStop={handleNodeDragStop}
          onNodeClick={handleNodeClick}
        />
      </div>
    </div>
  );
});
