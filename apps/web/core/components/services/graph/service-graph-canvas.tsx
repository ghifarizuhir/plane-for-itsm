/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import type { IService, TServiceGraphData } from "@plane/types";
// components
import { ServiceNode } from "./service-node";
import { getLayoutedElements } from "./use-graph-layout";

const nodeTypes = { service: ServiceNode };

export type TServiceGraphCanvasProps = {
  graphData: TServiceGraphData;
  readOnly?: boolean;
  /** When provided, non-listed services are dimmed (war room blast radius). */
  highlightedServiceIds?: string[];
  onNodeClick?: (serviceId: string) => void;
  onConnect?: (sourceServiceId: string, targetServiceId: string) => void;
  onEdgeDelete?: (dependencyId: string) => void;
  onNodeDragStop?: (serviceId: string, position: { x: number; y: number }) => void;
};

/**
 * Presentational React Flow canvas. Owns layout + i18n labels + viewport state;
 * mutation callbacks are optional so the same canvas serves the editable
 * Services page and the read-only war room map.
 */
export function ServiceGraphCanvas({
  graphData,
  readOnly = false,
  highlightedServiceIds,
  onNodeClick,
  onConnect,
  onEdgeDelete,
  onNodeDragStop,
}: TServiceGraphCanvasProps) {
  // plane hooks
  const { t, currentLocale } = useTranslation();
  // states
  const [instance, setInstance] = useState<ReactFlowInstance | null>(null);
  // refs
  const fitDone = useRef(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  // derived values
  const highlighted = useMemo(() => new Set(highlightedServiceIds ?? []), [highlightedServiceIds]);

  const { nodes: layoutNodes, edges: layoutEdges } = useMemo(
    () => getLayoutedElements(graphData.services, graphData.dependencies),
    [graphData.dependencies, graphData.services]
  );

  const translatedNodes = useMemo(
    () =>
      layoutNodes.map((node) => {
        const service = (node.data as { service?: IService } | undefined)?.service;
        if (!service) return node;
        const health = graphData.health[service.id];
        return {
          ...node,
          data: {
            ...(node.data as Record<string, unknown>),
            service,
            statusLabel: t(`service.status_values.${service.status}`),
            criticalityLabel: t(`service.criticality_values.${service.criticality}`),
            health: health?.health ?? "unknown",
            incidents: health?.incidents ?? [],
            dimmed: highlighted.size > 0 && !highlighted.has(service.id),
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
    [layoutNodes, currentLocale, graphData.health, highlighted]
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
          const prevData = node.data as
            | {
                statusLabel?: string;
                criticalityLabel?: string;
                health?: string;
                incidents?: unknown[];
                dimmed?: boolean;
              }
            | undefined;
          const nextData = next?.data as
            | {
                statusLabel?: string;
                criticalityLabel?: string;
                health?: string;
                incidents?: unknown[];
                dimmed?: boolean;
              }
            | undefined;
          return (
            prevData?.statusLabel === nextData?.statusLabel &&
            prevData?.criticalityLabel === nextData?.criticalityLabel &&
            prevData?.health === nextData?.health &&
            prevData?.dimmed === nextData?.dimmed &&
            (prevData?.incidents?.length ?? 0) === (nextData?.incidents?.length ?? 0)
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

  const handleConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      onConnect?.(connection.source, connection.target);
    },
    [onConnect]
  );

  const handleEdgesDelete = useCallback(
    (deleted: Edge[]) => {
      deleted.forEach((edge) => onEdgeDelete?.(edge.id));
    },
    [onEdgeDelete]
  );

  const handleNodeDragStop = useCallback(
    (_event: unknown, node: Node) => {
      onNodeDragStop?.(node.id, node.position);
    },
    [onNodeDragStop]
  );

  const handleNodeClick = useCallback(
    (_event: unknown, node: Node) => {
      onNodeClick?.(node.id);
    },
    [onNodeClick]
  );

  // Fit the graph the first time the canvas actually has dimensions. The war room
  // mounts this inside an off-screen drawer below xl, so `onInit` alone would fit
  // against a 0x0 viewport and never retry.
  useEffect(() => {
    const node = wrapperRef.current;
    if (!node || !instance) return;
    const fit = () => {
      if (fitDone.current) return;
      const { width, height } = node.getBoundingClientRect();
      if (width === 0 || height === 0) return;
      fitDone.current = true;
      instance.fitView();
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(node);
    return () => observer.disconnect();
  }, [instance]);

  return (
    <div ref={wrapperRef} className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={readOnly ? undefined : handleConnect}
        onEdgesDelete={readOnly ? undefined : handleEdgesDelete}
        onNodeDragStop={readOnly ? undefined : handleNodeDragStop}
        onNodeClick={handleNodeClick}
        nodesDraggable={!readOnly}
        nodesConnectable={!readOnly}
        edgesFocusable={!readOnly}
        elementsSelectable={!readOnly}
        deleteKeyCode={readOnly ? null : ["Backspace", "Delete"]}
        onInit={setInstance}
      >
        <Background />
        <Controls />
        {!readOnly && <MiniMap />}
      </ReactFlow>
    </div>
  );
}
