/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import {
  addEdge,
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
} from "@xyflow/react";
// oxlint-disable-next-line import/no-unassigned-import
import "@xyflow/react/dist/style.css";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
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

  const graphData = pid ? getGraphData(pid) : { services: [], dependencies: [] };

  const { nodes: layoutNodes, edges: layoutEdges } = useMemo(
    () => getLayoutedElements(graphData.services, graphData.dependencies),
    [graphData.dependencies, graphData.services]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutEdges);

  useEffect(() => {
    setNodes(layoutNodes);
    setEdges(layoutEdges);
  }, [layoutNodes, layoutEdges, setNodes, setEdges]);

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
        const dependency = await addDependency(slug, workspaceId, pid, source, target);
        setEdges((prev) => addEdge({ id: dependency.id, source, target, type: "smoothstep" }, prev));
      } catch (error) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: error instanceof Error ? error.message : "Could not create dependency. Please try again.",
        });
      }
    },
    [addDependency, pid, setEdges, slug, workspaceId]
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
      await updateNodePosition(slug, workspaceId, pid, node.id, node.position);
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
          fitView
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
