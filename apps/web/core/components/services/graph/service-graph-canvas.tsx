/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  ViewportPortal,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
  type ReactFlowInstance,
} from "@xyflow/react";
// oxlint-disable-next-line import/no-unassigned-import
import "@xyflow/react/dist/style.css";
import { useTheme } from "next-themes";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { ILiteService, IService, TServiceDependencyType, TServiceGraphData, TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident } from "@/services/service-health.helpers";
import { isLiteService } from "@/services/service.helpers";
// components
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { RESTRICTED_CLUSTER_STYLE, SERVICE_KIND_CLUSTER_STYLE } from "../kind-config";
import { getServiceClusters } from "./graph-clusters";
import { ServiceNode } from "./service-node";
import { getLayoutedElements } from "./use-graph-layout";
import { syncGraphNodes } from "./node-sync";

const nodeTypes = { service: ServiceNode };

const DEPENDENCY_TYPE_LABEL_KEYS: Record<TServiceDependencyType, string> = {
  depends_on: "service.detail.depends_on",
  uses: "service.relationships.uses",
  runs_on: "service.relationships.runs_on",
  connects_to: "service.relationships.connects_to",
  replicates_to: "service.relationships.replicates_to",
  other: "service.relationships.related_to",
};

export type TServiceGraphCanvasProps = {
  graphData: TServiceGraphData;
  readOnly?: boolean;
  /** When provided, non-listed services are dimmed (war room blast radius). */
  highlightedServiceIds?: string[];
  /** Optional lookup for owner project badges on service nodes. */
  projectLookup?: Record<string, { identifier: string; name: string }>;
  onNodeClick?: (serviceId: string) => void;
  onConnect?: (sourceServiceId: string, targetServiceId: string) => void;
  onEdgeDelete?: (dependencyId: string) => void;
  onNodeDragStop?: (serviceId: string, position: { x: number; y: number }) => void;
  /** Incremented by the store when topology/layout changes; triggers fitView. */
  layoutEpoch?: number;
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
  projectLookup,
  layoutEpoch = 0,
  onNodeClick,
  onConnect,
  onEdgeDelete,
  onNodeDragStop,
}: TServiceGraphCanvasProps) {
  // plane hooks
  const { t, currentLocale } = useTranslation();
  // theme — React Flow renders its own chrome, so it needs the resolved mode
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme?.includes("dark") ?? false;
  // states
  const [instance, setInstance] = useState<ReactFlowInstance | null>(null);
  // refs
  const fitDone = useRef(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  // derived values
  const highlighted = useMemo(() => new Set(highlightedServiceIds ?? []), [highlightedServiceIds]);

  const getMiniMapNodeColor = useCallback((node: Node) => {
    const health = (node.data as { health?: TServiceHealth } | undefined)?.health ?? DEFAULT_HEALTH;
    return HEALTH_CONFIG[health].color;
  }, []);

  const { nodes: layoutNodes, edges: layoutEdges } = useMemo(
    () => getLayoutedElements(graphData.services, graphData.dependencies),
    [graphData.dependencies, graphData.services]
  );

  const translatedNodes = useMemo(
    () =>
      layoutNodes.map((node) => {
        const service = (node.data as { service?: IService | ILiteService } | undefined)?.service;
        if (!service) return node;
        const snapshot = graphData.health[service.id];
        const healthState: TServiceHealth = snapshot?.health ?? "unknown";
        const incidents = snapshot?.incidents ?? [];
        const highestIncident = getHighestSeverityIncident(incidents);
        const isLite = isLiteService(service);
        const ownerProjectId = !isLite ? (service.owner_project_id ?? service.project_id) : null;
        const ownerProject = ownerProjectId ? projectLookup?.[ownerProjectId] : undefined;
        return {
          ...node,
          data: {
            ...(node.data as Record<string, unknown>),
            service,
            statusLabel: t(`service.status_values.${service.status}`),
            criticalityLabel: t(`service.criticality_values.${service.criticality}`),
            typeLabel: !isLite ? t(`service.type_values.${service.type}`) : t("service.lite.restricted"),
            kindLabel: !isLite ? t(`service.kind_values.${service.kind}`) : undefined,
            healthLabel: t(HEALTH_CONFIG[healthState].label_key),
            ownerProjectIdentifier: ownerProject?.identifier,
            ownerProjectName: ownerProject?.name,
            health: healthState,
            incidents,
            lastDeployedAt: snapshot?.last_deployed_at ?? null,
            noDeploymentLabel: t("service.deploy.no_deployment"),
            highestSeverityLabel: highestIncident
              ? t(`service.incident_severity.${highestIncident.severity}`)
              : undefined,
            dimmed: highlighted.size > 0 && !highlighted.has(service.id),
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
    [layoutNodes, currentLocale, graphData.health, highlighted, projectLookup]
  );

  const translatedEdges = useMemo<Edge[]>(
    () =>
      layoutEdges.map((edge): Edge => {
        const depType =
          (edge.data as { dependencyType?: TServiceDependencyType } | undefined)?.dependencyType ?? "depends_on";
        const targetHealth = graphData.health[edge.target]?.health ?? "unknown";
        const sourceHealth = graphData.health[edge.source]?.health ?? "unknown";
        const isDownPath = targetHealth === "down" || sourceHealth === "down";
        const isDegradedPath = !isDownPath && (targetHealth === "degraded" || sourceHealth === "degraded");
        const edgeStroke = isDownPath
          ? "var(--bg-danger-primary)"
          : isDegradedPath
            ? "var(--bg-warning-primary)"
            : "var(--border-strong)";

        return {
          ...edge,
          animated: isDownPath,
          label: depType !== "depends_on" ? t(DEPENDENCY_TYPE_LABEL_KEYS[depType]) : undefined,
          labelStyle: {
            fill: "var(--txt-secondary)",
            fontSize: 10,
            fontWeight: 500,
            fontFamily: "var(--font-code, monospace)",
          },
          labelBgStyle: {
            fill: "var(--bg-layer-1)",
            stroke: "var(--border-subtle)",
            strokeWidth: 1,
          },
          labelBgPadding: [6, 3] as [number, number],
          labelBgBorderRadius: 4,
          style: {
            stroke: edgeStroke,
            strokeWidth: isDownPath ? 2 : 1.5,
          },
          markerEnd: {
            type: MarkerType.ArrowClosed,
            width: 14,
            height: 14,
            color: edgeStroke,
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change
    [layoutEdges, currentLocale, graphData.health]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(translatedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(translatedEdges);

  const clusters = useMemo(
    () => getServiceClusters(nodes, graphData.services, projectLookup, t),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change
    [nodes, graphData.services, projectLookup, currentLocale]
  );

  useEffect(() => {
    setNodes((prev) => syncGraphNodes(prev, translatedNodes));
    setEdges((prev) => {
      if (
        prev.length === translatedEdges.length &&
        prev.every((edge, i) => {
          const next = translatedEdges[i];
          return (
            edge.id === next?.id &&
            edge.label === next?.label &&
            edge.animated === next?.animated &&
            edge.style?.stroke === next?.style?.stroke
          );
        })
      )
        return prev;
      const selectedById = new Map(prev.map((edge) => [edge.id, edge.selected]));
      return translatedEdges.map((edge) =>
        selectedById.has(edge.id) ? { ...edge, selected: selectedById.get(edge.id) } : edge
      );
    });
  }, [translatedNodes, translatedEdges, setNodes, setEdges]);

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

  // Re-fit after topology/layout changes (service/dependency mutations or manual
  // Re-layout). Deduplicated at the source: health polls, language changes, and
  // filters never bump the epoch.
  const lastLayoutEpoch = useRef(layoutEpoch);

  useEffect(() => {
    if (layoutEpoch === lastLayoutEpoch.current) return;
    lastLayoutEpoch.current = layoutEpoch;
    if (!instance) return;

    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        instance.fitView({ padding: 0.25, duration: 300 });
      });
    });

    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, [instance, layoutEpoch]);

  // Fit the graph on mount (the war room mounts this inside an off-screen
  // drawer below xl, so onInit alone would fit against a 0x0 viewport).
  useEffect(() => {
    const node = wrapperRef.current;
    if (!node || !instance) return;
    const fit = () => {
      if (fitDone.current) return;
      const { width, height } = node.getBoundingClientRect();
      if (width === 0 || height === 0) return;
      fitDone.current = true;
      instance.fitView({ padding: 0.25 });
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
        className="service-graph-flow"
        colorMode={isDark ? "dark" : "light"}
        defaultMarkerColor="var(--border-strong)"
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
        <ViewportPortal>
          {clusters.map((cluster) => (
            <div
              key={cluster.id}
              className={cn(
                "pointer-events-none absolute rounded-xl border",
                cluster.restricted ? RESTRICTED_CLUSTER_STYLE : SERVICE_KIND_CLUSTER_STYLE[cluster.kind ?? "other"]
              )}
              style={{
                transform: `translate(${cluster.x}px, ${cluster.y}px)`,
                width: cluster.width,
                height: cluster.height,
                zIndex: 0,
              }}
            >
              <span className="absolute top-2 left-3 text-11 font-medium">{cluster.label}</span>
            </div>
          ))}
        </ViewportPortal>
        <Background color="var(--border-subtle-1)" />
        <Controls />
        {!readOnly && <MiniMap nodeColor={getMiniMapNodeColor} />}
      </ReactFlow>
    </div>
  );
}
