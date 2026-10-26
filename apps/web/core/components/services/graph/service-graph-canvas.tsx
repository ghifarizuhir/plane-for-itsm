/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import {
  Background,
  BackgroundVariant,
  MarkerType,
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
import { AnimatePresence } from "framer-motion";
import { LockOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { ILiteService, IService, TServiceDependencyType, TServiceGraphData, TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident } from "@/services/service-health.helpers";
import { isLiteService } from "@/services/service.helpers";
// components
import { HEALTH_CONFIG } from "../health/health-config";
import { RESTRICTED_CLUSTER_STYLE, SERVICE_KIND_CLUSTER_STYLE, SERVICE_KIND_ICON } from "../kind-config";
import { getServiceClusters, type TServiceCluster } from "./graph-clusters";
import { getServiceRelationships } from "./graph-relationships";
import {
  FOCUS_RAIL_MIN_CANVAS_WIDTH,
  FOCUS_RAIL_WIDTH,
  ServiceFocusCard,
  type TFocusRelationship,
} from "./service-focus-card";
import { ServiceGraphHeaderControls } from "./service-graph-header-controls";
import { ServiceNode } from "./service-node";
import { GRAPH_FIT_VIEW_PADDING, getLayoutedElements } from "./use-graph-layout";
import { useServiceFocusInteraction } from "./use-service-focus";
import { syncGraphNodes } from "./node-sync";

const nodeTypes = { service: ServiceNode };

const CLUSTER_GRIP_DOTS = ["tl", "tr", "ml", "mr", "bl", "br"] as const;

/** Edge labels only earn their space once the cards and gaps are large enough. */
const EDGE_LABEL_VISIBLE_ZOOM = 1.1;

const getClusterAccent = (cluster: TServiceCluster) =>
  cluster.restricted ? "var(--txt-tertiary)" : `var(--kind-${(cluster.kind ?? "other").replace(/_/g, "-")})`;

type TServiceClusterHeaderProps = {
  cluster: TServiceCluster;
  readOnly: boolean;
  dragging: boolean;
  onDragStart: (cluster: TServiceCluster, event: ReactPointerEvent<HTMLDivElement>) => void;
  onDragMove: (event: ReactPointerEvent<HTMLDivElement>) => void;
  onDragEnd: (event: ReactPointerEvent<HTMLDivElement>) => void;
  onDragCancel: (event: ReactPointerEvent<HTMLDivElement>) => void;
};

/**
 * Cluster headers keep a constant screen size: at overview zoom they are the
 * only readable orientation cue, so they must not shrink with the viewport.
 * The pill doubles as the group drag handle.
 */
function ServiceClusterHeader({
  cluster,
  readOnly,
  dragging,
  onDragStart,
  onDragMove,
  onDragEnd,
  onDragCancel,
}: TServiceClusterHeaderProps) {
  const zoom = useStore((state) => state.transform[2]);
  const scale = 1 / Math.max(zoom, 0.5);
  const ClusterIcon = cluster.restricted ? LockOutline : SERVICE_KIND_ICON[cluster.kind ?? "other"];
  const accent = getClusterAccent(cluster);
  const draggable = !readOnly && !cluster.restricted;

  return (
    <div
      title={cluster.label}
      data-dragging={dragging || undefined}
      onPointerDown={draggable ? (event) => onDragStart(cluster, event) : undefined}
      onPointerMove={draggable ? onDragMove : undefined}
      onPointerUp={draggable ? onDragEnd : undefined}
      onPointerCancel={draggable ? onDragCancel : undefined}
      className={cn(
        "service-cluster-handle nodrag nopan group pointer-events-auto absolute top-0 left-3 flex touch-none items-center gap-1.5 rounded-full border border-subtle bg-surface-1 py-1 pr-1.5 pl-1 shadow-raised-200 transition-[border-color,box-shadow] duration-150 select-none",
        draggable ? "cursor-grab active:cursor-grabbing" : "cursor-default",
        dragging && "cursor-grabbing"
      )}
      style={{
        transform: `translateY(-50%) scale(${scale})`,
        transformOrigin: "left center",
        borderColor: dragging ? accent : undefined,
        boxShadow: dragging ? `0 14px 34px -14px color-mix(in oklab, ${accent} 55%, transparent)` : undefined,
      }}
    >
      <span
        className="grid h-5 w-5 shrink-0 place-items-center rounded-full"
        style={{
          backgroundColor: `color-mix(in oklab, ${accent} 12%, transparent)`,
          color: accent,
        }}
      >
        <ClusterIcon className="h-3 w-3" />
      </span>
      {cluster.projectLabel && (
        <span className="font-code text-9 font-medium tracking-[0.1em] text-tertiary uppercase">
          {cluster.projectLabel}
        </span>
      )}
      {cluster.projectLabel && cluster.kindLabel && <span aria-hidden className="bg-subtle h-2.5 w-px shrink-0" />}
      {cluster.kindLabel && (
        <span className="text-10 font-semibold tracking-[-0.01em] whitespace-nowrap text-secondary">
          {cluster.kindLabel}
        </span>
      )}
      <span className="inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full border border-subtle bg-layer-2 px-1 font-code text-9 font-semibold text-secondary tabular-nums">
        {cluster.count}
      </span>
      {draggable && (
        <span
          aria-hidden
          className={cn(
            "ml-0.5 grid shrink-0 grid-cols-2 gap-[2.5px] text-tertiary opacity-40 transition-opacity duration-150 group-hover:opacity-90",
            dragging && "opacity-90"
          )}
        >
          {CLUSTER_GRIP_DOTS.map((dot) => (
            <span key={dot} className="h-[2.5px] w-[2.5px] rounded-full bg-current" />
          ))}
        </span>
      )}
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

type TClusterDragSession = {
  pointerId: number;
  startX: number;
  startY: number;
  zoom: number;
  origins: Map<string, { x: number; y: number }>;
  moved: boolean;
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
  /** Resets persisted node positions and re-runs the layout. */
  onReLayout?: () => void;
  isReLayouting?: boolean;
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
  onReLayout,
  isReLayouting,
}: TServiceGraphCanvasProps) {
  // plane hooks
  const { t, currentLocale } = useTranslation();
  // theme — React Flow renders its own chrome, so it needs the resolved mode
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme?.includes("dark") ?? false;
  // states
  const [instance, setInstance] = useState<ReactFlowInstance | null>(null);
  const [showEdgeLabels, setShowEdgeLabels] = useState(false);
  const [draggingClusterId, setDraggingClusterId] = useState<string | null>(null);
  const [railHeld, setRailHeld] = useState(false);
  const { effectiveServiceId, pinnedServiceId, pin, clear: clearFocus } = useServiceFocusInteraction();
  // refs
  const fitDone = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const clusterDragSession = useRef<TClusterDragSession | null>(null);
  const [containerWidth, setContainerWidth] = useState(0);
  // derived values
  const highlighted = useMemo(() => new Set(highlightedServiceIds ?? []), [highlightedServiceIds]);
  const selectedService = effectiveServiceId
    ? (graphData.services.find((service) => service.id === effectiveServiceId) ?? null)
    : null;
  const selectedHealth = effectiveServiceId ? graphData.health[effectiveServiceId] : undefined;
  const selectedProjectIdentifier = useMemo(() => {
    if (!selectedService || !("owner_project_id" in selectedService)) return null;
    const projectId = selectedService.owner_project_id;
    return projectId ? (projectLookup?.[projectId]?.identifier ?? null) : null;
  }, [projectLookup, selectedService]);
  const relationships = useMemo(
    () => (effectiveServiceId ? getServiceRelationships(effectiveServiceId, graphData.dependencies) : null),
    [graphData.dependencies, effectiveServiceId]
  );
  const focusRelationships = useMemo<TFocusRelationship[]>(() => {
    if (!effectiveServiceId) return [];

    const result: TFocusRelationship[] = [];
    graphData.dependencies.forEach((dependency) => {
      const isOutgoing = dependency.from_service_id === effectiveServiceId;
      const isIncoming = dependency.to_service_id === effectiveServiceId;
      if (!isOutgoing && !isIncoming) return;

      const relatedId = isOutgoing ? dependency.to_service_id : dependency.from_service_id;
      const related =
        graphData.services.find((candidate) => candidate.id === relatedId) ??
        (dependency.from_service?.id === relatedId ? dependency.from_service : dependency.to_service);

      if (!related) return;
      result.push({
        id: dependency.id,
        direction: isOutgoing ? "out" : "in",
        type: dependency.dependency_type ?? "depends_on",
        service: related,
        health: graphData.health[relatedId]?.health,
      });
    });

    return result;
  }, [effectiveServiceId, graphData]);

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
            noDeploymentLabel: t("service.deploy.no_deployment"),
            highestSeverityLabel: highestIncident
              ? t(`service.incident_severity.${highestIncident.severity}`)
              : undefined,
            dimmed: !relationships && highlighted.size > 0 && !highlighted.has(service.id),
            focusDimmed: relationships ? !relationships.nodeIds.has(service.id) : false,
            focused: service.id === effectiveServiceId,
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
    [layoutNodes, currentLocale, graphData.health, highlighted, relationships, effectiveServiceId]
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
  const selectedNode = effectiveServiceId ? (nodes.find((node) => node.id === effectiveServiceId) ?? null) : null;

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

  const handleClusterDragStart = useCallback(
    (cluster: TServiceCluster, event: ReactPointerEvent<HTMLDivElement>) => {
      if (readOnly || cluster.restricted || !instance || event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      event.currentTarget.setPointerCapture(event.pointerId);

      const memberIds = new Set(cluster.serviceIds);
      const origins = new Map<string, { x: number; y: number }>();
      instance.getNodes().forEach((node) => {
        if (memberIds.has(node.id)) origins.set(node.id, { ...node.position });
      });
      if (origins.size === 0) return;

      clusterDragSession.current = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        zoom: instance.getZoom(),
        origins,
        moved: false,
      };
      setDraggingClusterId(cluster.id);
      setNodes((current) => current.map((node) => (origins.has(node.id) ? { ...node, dragging: true } : node)));
    },
    [instance, readOnly, setNodes]
  );

  const handleClusterDragMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const session = clusterDragSession.current;
      if (!session || session.pointerId !== event.pointerId) return;
      event.preventDefault();
      event.stopPropagation();

      if (!session.moved && Math.hypot(event.clientX - session.startX, event.clientY - session.startY) < 2) return;
      session.moved = true;

      const dx = (event.clientX - session.startX) / session.zoom;
      const dy = (event.clientY - session.startY) / session.zoom;
      setNodes((current) =>
        current.map((node) => {
          const origin = session.origins.get(node.id);
          return origin ? { ...node, dragging: true, position: { x: origin.x + dx, y: origin.y + dy } } : node;
        })
      );
    },
    [setNodes]
  );

  const releaseClusterDrag = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    clusterDragSession.current = null;
    setDraggingClusterId(null);
  }, []);

  const handleClusterDragEnd = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const session = clusterDragSession.current;
      if (!session || session.pointerId !== event.pointerId) return;
      event.stopPropagation();

      if (session.moved) {
        const dx = (event.clientX - session.startX) / session.zoom;
        const dy = (event.clientY - session.startY) / session.zoom;
        const liteIds = new Set(graphData.services.filter(isLiteService).map((service) => service.id));
        session.origins.forEach((origin, serviceId) => {
          if (liteIds.has(serviceId)) return;
          onNodeDragStop?.(serviceId, { x: origin.x + dx, y: origin.y + dy });
        });
      }

      setNodes((current) =>
        current.map((node) => (session.origins.has(node.id) ? { ...node, dragging: false } : node))
      );
      releaseClusterDrag(event);
    },
    [graphData.services, onNodeDragStop, releaseClusterDrag, setNodes]
  );

  const handleClusterDragCancel = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const session = clusterDragSession.current;
      if (!session || session.pointerId !== event.pointerId) return;
      event.stopPropagation();

      setNodes((current) =>
        current.map((node) => {
          const origin = session.origins.get(node.id);
          return origin ? { ...node, dragging: false, position: origin } : node;
        })
      );
      releaseClusterDrag(event);
    },
    [releaseClusterDrag, setNodes]
  );

  const handleNodeClick = useCallback(
    (_event: unknown, node: Node) => {
      pin(node.id);
    },
    [pin]
  );

  const handleNodeDoubleClick = useCallback(
    (_event: unknown, node: Node) => {
      onNodeOpen?.(node.id);
    },
    [onNodeOpen]
  );

  const handlePaneClick = useCallback(() => {
    clearFocus();
  }, [clearFocus]);

  const closeFocus = useCallback(() => {
    clearFocus();
    setNodes((current) => current.map((node) => (node.selected ? { ...node, selected: false } : node)));
  }, [clearFocus, setNodes]);
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
    if (!effectiveServiceId) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeFocus();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [closeFocus, effectiveServiceId]);

  useEffect(() => {
    if (effectiveServiceId && !graphData.services.some((service) => service.id === effectiveServiceId)) {
      clearFocus();
    }
  }, [clearFocus, effectiveServiceId, graphData.services]);

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
        instance.fitView({ padding: GRAPH_FIT_VIEW_PADDING, duration: 300 });
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
      instance.fitView({ padding: GRAPH_FIT_VIEW_PADDING });
      syncEdgeLabels(instance.getZoom());
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(node);
    return () => observer.disconnect();
  }, [instance, syncEdgeLabels]);

  // Measure the full canvas so a pinned inspector can claim a rail without
  // feeding its own width back into the decision.
  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const measure = () => setContainerWidth(element.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const railTarget = pinnedServiceId !== null && containerWidth >= FOCUS_RAIL_MIN_CANVAS_WIDTH;

  // Hold the rail open for the inspector's exit animation before the map
  // reclaims the space.
  useEffect(() => {
    if (railTarget) {
      setRailHeld(true);
      return;
    }
    const timeout = setTimeout(() => setRailHeld(false), 160);
    return () => clearTimeout(timeout);
  }, [railTarget]);

  const railActive = railTarget || railHeld;

  // Give the map room for the inspector rail. The fit waits a frame so React
  // Flow has measured the narrower canvas, then eases into the new frame.
  const lastRailActive = useRef(false);
  useEffect(() => {
    if (!instance || railActive === lastRailActive.current) return;
    lastRailActive.current = railActive;
    const timeout = setTimeout(
      () => instance.fitView({ padding: GRAPH_FIT_VIEW_PADDING, duration: railActive ? 260 : 220 }),
      railActive ? 60 : 140
    );
    return () => clearTimeout(timeout);
  }, [instance, railActive]);

  return (
    <ReactFlowProvider>
      <div ref={containerRef} className="relative h-full w-full">
        <div
          ref={wrapperRef}
          className="relative h-full"
          style={{ width: railActive ? `calc(100% - ${FOCUS_RAIL_WIDTH}px)` : "100%" }}
        >
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
                const isDragging = draggingClusterId === cluster.id;
                const accent = getClusterAccent(cluster);

                return (
                  <div
                    key={cluster.id}
                    data-dragging={isDragging || undefined}
                    className={cn(
                      "service-cluster pointer-events-none absolute rounded-[18px] border transition-[opacity,border-color,box-shadow] duration-200",
                      cluster.restricted
                        ? RESTRICTED_CLUSTER_STYLE
                        : SERVICE_KIND_CLUSTER_STYLE[cluster.kind ?? "other"]
                    )}
                    style={{
                      transform: `translate(${cluster.x}px, ${cluster.y}px)`,
                      width: cluster.width,
                      height: cluster.height,
                      opacity: isRelatedCluster ? 1 : 0.3,
                      zIndex: isDragging ? 5 : 0,
                      borderColor: isDragging ? accent : undefined,
                      boxShadow: isDragging
                        ? `0 24px 56px -28px color-mix(in oklab, ${accent} 55%, transparent), inset 0 0 0 1px color-mix(in oklab, ${accent} 35%, transparent)`
                        : undefined,
                    }}
                  >
                    <span
                      aria-hidden
                      className="pointer-events-none absolute inset-0 rounded-[inherit]"
                      style={{
                        backgroundImage: `radial-gradient(120% 90% at 50% 0%, color-mix(in oklab, ${accent} 7%, transparent), transparent 62%)`,
                      }}
                    />
                    <span
                      aria-hidden
                      className="pointer-events-none absolute inset-x-6 top-0 h-px"
                      style={{
                        backgroundImage: `linear-gradient(90deg, transparent, color-mix(in oklab, ${accent} 65%, transparent), transparent)`,
                      }}
                    />
                    <ServiceClusterHeader
                      cluster={cluster}
                      readOnly={readOnly}
                      dragging={isDragging}
                      onDragStart={handleClusterDragStart}
                      onDragMove={handleClusterDragMove}
                      onDragEnd={handleClusterDragEnd}
                      onDragCancel={handleClusterDragCancel}
                    />
                  </div>
                );
              })}
            </ViewportPortal>
            <CanvasBackground />
          </ReactFlow>
          <ServiceGraphHeaderControls onReLayout={readOnly ? undefined : onReLayout} isReLayouting={isReLayouting} />
          <AnimatePresence mode="wait">
            {selectedService && selectedNode && (
              <ServiceFocusCard
                key={selectedService.id}
                node={selectedNode}
                service={selectedService}
                projectIdentifier={selectedProjectIdentifier}
                relationships={focusRelationships}
                health={selectedHealth?.health}
                lastDeployedAt={selectedHealth?.last_deployed_at}
                pinned={pinnedServiceId === selectedService.id}
                rail={railActive}
                onClose={closeFocus}
                onFocusService={pin}
              />
            )}
          </AnimatePresence>
        </div>
      </div>
    </ReactFlowProvider>
  );
}
