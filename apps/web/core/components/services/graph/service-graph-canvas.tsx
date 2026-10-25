/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  ViewportPortal,
  useEdgesState,
  useNodesState,
  useStore,
  type Connection,
  type Edge,
  type Node,
  type ReactFlowInstance,
  type Viewport,
} from "@xyflow/react";
// oxlint-disable-next-line import/no-unassigned-import
import "@xyflow/react/dist/style.css";
import { useTheme } from "next-themes";
import { LockOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { ILiteService, IService, TServiceDependencyType, TServiceGraphData, TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident } from "@/services/service-health.helpers";
import { isLiteService } from "@/services/service.helpers";
// components
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { RESTRICTED_CLUSTER_STYLE, SERVICE_KIND_CLUSTER_STYLE, SERVICE_KIND_ICON } from "../kind-config";
import { getServiceClusters, type TServiceCluster } from "./graph-clusters";
import { getServiceRelationships } from "./graph-relationships";
import { ServiceFocusCard } from "./service-focus-card";
import { ServiceNode } from "./service-node";
import { CLUSTER_HEADER_HEIGHT, getLayoutedElements } from "./use-graph-layout";
import { syncGraphNodes } from "./node-sync";

const nodeTypes = { service: ServiceNode };

/** Edge labels only earn their space once the cards and gaps are large enough. */
const EDGE_LABEL_VISIBLE_ZOOM = 1.1;

/** Leave the floating legend room above the map without shrinking the sides. */
const FIT_VIEW_PADDING = { top: "56px", right: 0.12, bottom: 0.12, left: 0.12 } as const;

/**
 * Cluster headers keep a constant screen size: at overview zoom they are the
 * only readable orientation cue, so they must not shrink with the viewport.
 */
function ServiceClusterHeader({ cluster }: { cluster: TServiceCluster }) {
  const zoom = useStore((state) => state.transform[2]);
  const scale = 1 / Math.max(zoom, 0.5);
  const ClusterIcon = cluster.restricted ? LockOutline : SERVICE_KIND_ICON[cluster.kind ?? "other"];

  return (
    <div
      className="flex min-w-0 items-center"
      style={{ height: CLUSTER_HEADER_HEIGHT, gap: 6 * scale, paddingInline: 12 * scale }}
    >
      <span className="shrink-0" style={{ width: 14 * scale, height: 14 * scale }}>
        <ClusterIcon className="h-full w-full" />
      </span>
      <span
        className="min-w-0 truncate font-semibold tracking-[-0.01em]"
        style={{ fontSize: 11 * scale, lineHeight: 1.2 }}
      >
        {cluster.label}
      </span>
      <span
        className="shrink-0 font-code font-medium tabular-nums"
        style={{ fontSize: 10 * scale, paddingLeft: 4 * scale }}
      >
        {cluster.count}
      </span>
    </div>
  );
}

/**
 * Dot grid kept at a constant screen size so it stays legible at the zoom
 * levels a full topology settles on (React Flow scales patterns with zoom).
 */
function CanvasBackground() {
  const zoom = useStore((state) => state.transform[2]);
  const scale = Math.max(zoom, 0.05);

  return (
    <>
      <Background
        id="service-graph-dots"
        variant={BackgroundVariant.Dots}
        gap={12 / scale}
        size={2.2 / scale}
        color="var(--canvas-dot)"
      />
      <Background
        id="service-graph-dots-major"
        variant={BackgroundVariant.Dots}
        gap={24 / scale}
        size={2.8 / scale}
        color="var(--canvas-dot-strong)"
      />
    </>
  );
}

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
  /** Opens a service (peek on the catalog page, new tab in the war room). */
  onNodeOpen?: (serviceId: string) => void;
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
  onNodeOpen,
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
  const [showEdgeLabels, setShowEdgeLabels] = useState(false);
  const [selectedServiceId, setSelectedServiceId] = useState<string | null>(null);
  // refs
  const fitDone = useRef(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  // derived values
  const highlighted = useMemo(() => new Set(highlightedServiceIds ?? []), [highlightedServiceIds]);
  const selectedService = selectedServiceId
    ? (graphData.services.find((service) => service.id === selectedServiceId) ?? null)
    : null;
  const selectedHealth = selectedServiceId ? graphData.health[selectedServiceId] : undefined;
  const relationships = useMemo(
    () => (selectedServiceId ? getServiceRelationships(selectedServiceId, graphData.dependencies) : null),
    [graphData.dependencies, selectedServiceId]
  );

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
        return {
          ...node,
          data: {
            ...(node.data as Record<string, unknown>),
            service,
            statusLabel: t(`service.status_values.${service.status}`),
            criticalityLabel: t(`service.criticality_values.${service.criticality}`),
            typeLabel: !isLite ? t(`service.type_values.${service.type}`) : t("service.lite.restricted"),
            healthLabel: t(HEALTH_CONFIG[healthState].label_key),
            health: healthState,
            incidents,
            lastDeployedAt: snapshot?.last_deployed_at ?? null,
            highestSeverityLabel: highestIncident
              ? t(`service.incident_severity.${highestIncident.severity}`)
              : undefined,
            dimmed: !relationships && highlighted.size > 0 && !highlighted.has(service.id),
            focusDimmed: relationships ? !relationships.nodeIds.has(service.id) : false,
            focused: service.id === selectedServiceId,
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
    [layoutNodes, currentLocale, graphData.health, highlighted, relationships, selectedServiceId]
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
        const isRelated = relationships?.edgeIds.has(edge.id) ?? false;
        const isMuted = relationships !== null && !isRelated;
        const focusedStroke = isRelated && !isDownPath && !isDegradedPath ? "var(--border-accent-strong)" : edgeStroke;

        return {
          ...edge,
          animated: isDownPath && !isMuted,
          label:
            showEdgeLabels && !isMuted && depType !== "depends_on" ? t(DEPENDENCY_TYPE_LABEL_KEYS[depType]) : undefined,
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
          labelBgPadding: [7, 4] as [number, number],
          labelBgBorderRadius: 6,
          style: {
            stroke: focusedStroke,
            strokeWidth: isDownPath || isRelated ? 2 : 1.5,
            opacity: isMuted ? 0.08 : 1,
          },
          markerEnd: isMuted
            ? undefined
            : {
                type: MarkerType.ArrowClosed,
                width: 14,
                height: 14,
                color: focusedStroke,
              },
          zIndex: isRelated ? 2 : 0,
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change
    [layoutEdges, currentLocale, graphData.health, showEdgeLabels, relationships]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(translatedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(translatedEdges);
  const selectedNode = selectedServiceId ? (nodes.find((node) => node.id === selectedServiceId) ?? null) : null;

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
            edge.style?.stroke === next?.style?.stroke &&
            edge.style?.opacity === next?.style?.opacity &&
            edge.style?.strokeWidth === next?.style?.strokeWidth &&
            edge.zIndex === next?.zIndex &&
            Boolean(edge.markerEnd) === Boolean(next?.markerEnd)
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

  const handleNodeClick = useCallback((_event: unknown, node: Node) => {
    setSelectedServiceId(node.id);
  }, []);

  const handleNodeDoubleClick = useCallback(
    (_event: unknown, node: Node) => {
      onNodeOpen?.(node.id);
    },
    [onNodeOpen]
  );

  const handlePaneClick = useCallback(() => {
    setSelectedServiceId(null);
  }, []);

  const closeFocus = useCallback(() => {
    setSelectedServiceId(null);
    setNodes((current) => current.map((node) => (node.selected ? { ...node, selected: false } : node)));
  }, [setNodes]);
  const openSelectedService = useCallback(() => {
    if (selectedServiceId) onNodeOpen?.(selectedServiceId);
  }, [onNodeOpen, selectedServiceId]);

  const syncEdgeLabels = useCallback((zoom: number) => {
    setShowEdgeLabels((previous) => {
      const next = zoom >= EDGE_LABEL_VISIBLE_ZOOM;
      return previous === next ? previous : next;
    });
  }, []);

  const handleMove = useCallback(
    (_event: unknown, viewport: Viewport) => {
      syncEdgeLabels(viewport.zoom);
    },
    [syncEdgeLabels]
  );

  useEffect(() => {
    if (!selectedServiceId) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeFocus();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [closeFocus, selectedServiceId]);

  useEffect(() => {
    if (selectedServiceId && !graphData.services.some((service) => service.id === selectedServiceId)) {
      setSelectedServiceId(null);
    }
  }, [graphData.services, selectedServiceId]);

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
        instance.fitView({ padding: FIT_VIEW_PADDING, duration: 300 });
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
      instance.fitView({ padding: FIT_VIEW_PADDING });
      syncEdgeLabels(instance.getZoom());
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(node);
    return () => observer.disconnect();
  }, [instance, syncEdgeLabels]);

  return (
    <ReactFlowProvider>
      <div ref={wrapperRef} className="relative h-full w-full">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          className="service-graph-flow"
          colorMode={isDark ? "dark" : "light"}
          defaultMarkerColor="var(--border-strong)"
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onMove={handleMove}
          onConnect={readOnly ? undefined : handleConnect}
          onEdgesDelete={readOnly ? undefined : handleEdgesDelete}
          onNodeDragStop={readOnly ? undefined : handleNodeDragStop}
          onNodeClick={handleNodeClick}
          onNodeDoubleClick={handleNodeDoubleClick}
          onPaneClick={handlePaneClick}
          nodesDraggable={!readOnly}
          nodesConnectable={!readOnly}
          edgesFocusable={!readOnly}
          elementsSelectable={!readOnly}
          deleteKeyCode={readOnly ? null : ["Backspace", "Delete"]}
          minZoom={0.05}
          proOptions={{ hideAttribution: true }}
          onInit={setInstance}
        >
          <ViewportPortal>
            {clusters.map((cluster) => {
              const isRelatedCluster =
                !relationships || cluster.serviceIds.some((serviceId) => relationships.nodeIds.has(serviceId));

              return (
                <div
                  key={cluster.id}
                  className={cn(
                    "pointer-events-none absolute rounded-xl border transition-opacity duration-150",
                    cluster.restricted ? RESTRICTED_CLUSTER_STYLE : SERVICE_KIND_CLUSTER_STYLE[cluster.kind ?? "other"]
                  )}
                  style={{
                    transform: `translate(${cluster.x}px, ${cluster.y}px)`,
                    width: cluster.width,
                    height: cluster.height,
                    opacity: isRelatedCluster ? 1 : 0.35,
                    zIndex: 0,
                  }}
                >
                  <ServiceClusterHeader cluster={cluster} />
                </div>
              );
            })}
          </ViewportPortal>
          <CanvasBackground />
          <Controls showInteractive={false} />
          {!readOnly && <MiniMap pannable nodeColor={getMiniMapNodeColor} />}
        </ReactFlow>
        {selectedService && selectedNode && (
          <ServiceFocusCard
            key={selectedService.id}
            node={selectedNode}
            service={selectedService}
            relationshipCount={relationships?.edgeIds.size ?? 0}
            health={selectedHealth?.health}
            lastDeployedAt={selectedHealth?.last_deployed_at}
            onClose={closeFocus}
            onOpen={onNodeOpen ? openSelectedService : undefined}
          />
        )}
      </div>
    </ReactFlowProvider>
  );
}
