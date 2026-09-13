/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
  type ReactFlowInstance,
} from "@xyflow/react";
// oxlint-disable-next-line import/no-unassigned-import
import "@xyflow/react/dist/style.css";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
// components
import { ServiceNode } from "./service-node";
import { getLayoutedElements } from "./use-graph-layout";

const nodeTypes = { service: ServiceNode };

export const ServiceGraph = observer(function ServiceGraph() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t, currentLocale } = useTranslation();
  // store hooks
  const { getGraphData, addDependency, removeDependency, updateNodePosition, updateService } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString();
  const workspaceId = currentWorkspace?.id;
  // states
  const [isReLayouting, setIsReLayouting] = useState(false);
  const fitDone = useRef(false);

  const graphData = pid ? getGraphData(pid) : { services: [], dependencies: [] };

  const { nodes: layoutNodes, edges: layoutEdges } = useMemo(
    () => getLayoutedElements(graphData.services, graphData.dependencies),
    [graphData.dependencies, graphData.services]
  );

  const translatedNodes = useMemo(
    () =>
      layoutNodes.map((node) => {
        const service = (node.data as { service?: IService } | undefined)?.service;
        if (!service) return node;
        return {
          ...node,
          data: {
            ...(node.data as Record<string, unknown>),
            service,
            statusLabel: t(`service.status_values.${service.status}`),
            criticalityLabel: t(`service.criticality_values.${service.criticality}`),
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
    [layoutNodes, currentLocale]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(translatedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutEdges);

  useEffect(() => {
    setNodes((prev) => {
      if (
        prev.length === translatedNodes.length &&
        prev.every((node, i) => {
          const next = translatedNodes[i];
          if (node.id !== next?.id) return false;
          const prevData = node.data as { statusLabel?: string; criticalityLabel?: string } | undefined;
          const nextData = next?.data as { statusLabel?: string; criticalityLabel?: string } | undefined;
          return (
            prevData?.statusLabel === nextData?.statusLabel && prevData?.criticalityLabel === nextData?.criticalityLabel
          );
        })
      )
        return prev;
      const selectedById = new Map(prev.map((node) => [node.id, node.selected]));
      return translatedNodes.map((node) =>
        selectedById.has(node.id) ? { ...node, selected: selectedById.get(node.id) } : node
      );
    });
    setEdges((prev) => {
      if (prev.length === layoutEdges.length && prev.every((edge, i) => edge.id === layoutEdges[i]?.id)) return prev;
      const selectedById = new Map(prev.map((edge) => [edge.id, edge.selected]));
      return layoutEdges.map((edge) =>
        selectedById.has(edge.id) ? { ...edge, selected: selectedById.get(edge.id) } : edge
      );
    });
  }, [translatedNodes, layoutEdges, setNodes, setEdges]);

  const onConnect = useCallback(
    async (connection: Connection) => {
      const { source, target } = connection;
      if (!source || !target) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not create dependency. Both services must be selected.",
        });
        return;
      }
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

  const onEdgesDelete = useCallback(
    async (deleted: Edge[]) => {
      if (!slug || !workspaceId || !pid) return;
      try {
        await Promise.all(deleted.map((edge) => removeDependency(slug, workspaceId, pid, edge.id)));
      } catch {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not delete dependency. Please try again.",
        });
      }
    },
    [pid, removeDependency, slug, workspaceId]
  );

  const onNodeDragStop = useCallback(
    async (_event: unknown, node: Node) => {
      if (!slug || !workspaceId || !pid) return;
      try {
        await updateNodePosition(slug, workspaceId, pid, node.id, node.position);
      } catch {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not save node position. Please try again.",
        });
      }
    },
    [pid, slug, updateNodePosition, workspaceId]
  );

  const onNodeClick = useCallback(
    (_event: unknown, node: Node) => {
      if (!slug || !pid) return;
      router.push(`/${slug}/projects/${pid}/services/${node.id}`);
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
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onEdgesDelete={onEdgesDelete}
          onNodeDragStop={onNodeDragStop}
          onNodeClick={onNodeClick}
          onInit={(instance: ReactFlowInstance) => {
            if (!fitDone.current) {
              fitDone.current = true;
              instance.fitView();
            }
          }}
          deleteKeyCode={["Backspace", "Delete"]}
        >
          <Background />
          <Controls />
          <MiniMap />
        </ReactFlow>
      </div>
    </div>
  );
});
