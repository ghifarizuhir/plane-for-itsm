/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TServiceGraphData } from "@plane/types";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
// components
import { ServiceGraphCanvas } from "./service-graph-canvas";

export const ServiceGraph = observer(function ServiceGraph() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getGraphData, addDependency, removeDependency, updateNodePosition, updateService } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString();
  const workspaceId = currentWorkspace?.id;
  // states
  const [isReLayouting, setIsReLayouting] = useState(false);

  const graphData: TServiceGraphData = pid ? getGraphData(pid) : { services: [], dependencies: [], health: {} };

  const handleConnect = useCallback(
    async (source: string, target: string) => {
      if (!slug || !workspaceId || !pid) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not create dependency. Please try again.",
        });
        return;
      }
      try {
        await addDependency(slug, workspaceId, pid, source, target);
      } catch (error) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: error instanceof Error ? error.message : "Could not create dependency. Please try again.",
        });
      }
    },
    [addDependency, pid, slug, workspaceId]
  );

  const handleEdgeDelete = useCallback(
    (dependencyId: string) => {
      if (!slug || !workspaceId || !pid) return;
      void removeDependency(slug, workspaceId, pid, dependencyId).catch(() => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not delete dependency. Please try again.",
        });
      });
    },
    [pid, removeDependency, slug, workspaceId]
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
      if (!slug || !pid) return;
      router.push(`/${slug}/projects/${pid}/services/${serviceId}`);
    },
    [pid, router, slug]
  );

  const handleReLayout = useCallback(async () => {
    if (!slug || !workspaceId || !pid) return;
    const positioned = graphData.services.filter((service) => service.position !== null);
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
      <div className="flex items-center justify-between gap-2 px-2 py-1.5">
        <p className="text-xs text-secondary">{t("service.graph.connect_hint")}</p>
        <Button variant="secondary" size="sm" onClick={handleReLayout} loading={isReLayouting}>
          {t("service.graph.re_layout")}
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <ServiceGraphCanvas
          graphData={graphData}
          onConnect={handleConnect}
          onEdgeDelete={handleEdgeDelete}
          onNodeDragStop={handleNodeDragStop}
          onNodeClick={handleNodeClick}
        />
      </div>
    </div>
  );
});
