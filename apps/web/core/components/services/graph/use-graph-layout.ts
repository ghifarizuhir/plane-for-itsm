/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Edge, Node } from "@xyflow/react";
import { MarkerType, Position } from "@xyflow/react";
import dagre from "dagre";
import type { ILiteService, IService, IServiceDependency } from "@plane/types";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { getServiceGroups } from "./graph-groups";

export const SERVICE_NODE_WIDTH = 256;
export const SERVICE_NODE_HEIGHT = 104;
export const GROUP_GAP = 64;
export const GROUP_MAX_ROW_WIDTH = 1800;
export const CLUSTER_PADDING = 24;
export const CLUSTER_HEADER_HEIGHT = 32;

type TPosition = { x: number; y: number };

type TGroupLayout = {
  positions: Map<string, TPosition>;
  minX: number;
  minY: number;
  width: number;
  height: number;
};

const centroid = (positions: TPosition[]): TPosition => ({
  x: positions.reduce((sum, position) => sum + position.x, 0) / positions.length,
  y: positions.reduce((sum, position) => sum + position.y, 0) / positions.length,
});

const layoutGroup = (services: (IService | ILiteService)[], dependencies: IServiceDependency[]): TGroupLayout => {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", nodesep: 48, ranksep: 112 });

  const ids = new Set(services.map((service) => service.id));
  services.forEach((service) => graph.setNode(service.id, { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT }));
  dependencies.forEach((dependency) => {
    if (ids.has(dependency.from_service_id) && ids.has(dependency.to_service_id)) {
      graph.setEdge(dependency.from_service_id, dependency.to_service_id);
    }
  });

  dagre.layout(graph);

  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  const positions = new Map<string, TPosition>();

  services.forEach((service) => {
    const position = graph.node(service.id);
    const x = (position?.x ?? 0) - SERVICE_NODE_WIDTH / 2;
    const y = (position?.y ?? 0) - SERVICE_NODE_HEIGHT / 2;
    positions.set(service.id, { x, y });
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x + SERVICE_NODE_WIDTH);
    maxY = Math.max(maxY, y + SERVICE_NODE_HEIGHT);
  });

  return {
    positions,
    minX: Number.isFinite(minX) ? minX : 0,
    minY: Number.isFinite(minY) ? minY : 0,
    width: Number.isFinite(maxX) ? maxX - minX : SERVICE_NODE_WIDTH,
    height: Number.isFinite(maxY) ? maxY - minY : SERVICE_NODE_HEIGHT,
  };
};

export const getLayoutedElements = (
  services: (IService | ILiteService)[],
  dependencies: IServiceDependency[]
): { nodes: Node[]; edges: Edge[] } => {
  const groups = getServiceGroups(services);
  const layouts = groups.map((group) => ({ group, layout: layoutGroup(group.services, dependencies) }));

  const slots = new Map<string, TPosition>();
  let cursorX = 0;
  let cursorY = 0;
  let rowHeight = 0;

  layouts.forEach(({ group, layout }) => {
    const outerWidth = layout.width + CLUSTER_PADDING * 2;
    const outerHeight = layout.height + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2;
    if (cursorX > 0 && cursorX + outerWidth > GROUP_MAX_ROW_WIDTH) {
      cursorX = 0;
      cursorY += rowHeight + GROUP_GAP;
      rowHeight = 0;
    }
    slots.set(group.key, { x: cursorX, y: cursorY });
    cursorX += outerWidth + GROUP_GAP;
    rowHeight = Math.max(rowHeight, outerHeight);
  });

  const finalPositions = new Map<string, TPosition>();

  layouts.forEach(({ group, layout }) => {
    const slot = slots.get(group.key) ?? { x: 0, y: 0 };
    const defaults = new Map<string, TPosition>();
    group.services.forEach((service) => {
      const position = layout.positions.get(service.id) ?? { x: 0, y: 0 };
      defaults.set(service.id, {
        x: slot.x + CLUSTER_PADDING + (position.x - layout.minX),
        y: slot.y + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING + (position.y - layout.minY),
      });
    });

    const stored = group.services.filter(
      (service): service is IService => !isLiteService(service) && service.position !== null
    );

    if (stored.length === 0) {
      group.services.forEach((service) => {
        finalPositions.set(service.id, defaults.get(service.id) ?? { x: 0, y: 0 });
      });
      return;
    }

    const defaultCentroid = centroid(group.services.map((service) => defaults.get(service.id) ?? { x: 0, y: 0 }));
    const storedCentroid = centroid(stored.map((service) => service.position ?? { x: 0, y: 0 }));
    const offset = { x: storedCentroid.x - defaultCentroid.x, y: storedCentroid.y - defaultCentroid.y };

    group.services.forEach((service) => {
      const storedPosition = !isLiteService(service) ? service.position : null;
      if (storedPosition) {
        finalPositions.set(service.id, storedPosition);
        return;
      }
      const fallback = defaults.get(service.id) ?? { x: 0, y: 0 };
      finalPositions.set(service.id, { x: fallback.x + offset.x, y: fallback.y + offset.y });
    });
  });

  const ids = new Set(services.map((service) => service.id));
  const nodes: Node[] = services.map((service) => ({
    id: service.id,
    type: "service",
    data: { service, accessLevel: isLiteService(service) ? "lite" : "full" },
    position: finalPositions.get(service.id) ?? { x: 0, y: 0 },
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
    zIndex: 10,
  }));

  const edges: Edge[] = dependencies
    .filter((dependency) => ids.has(dependency.from_service_id) && ids.has(dependency.to_service_id))
    .map((dependency) => ({
      id: dependency.id,
      source: dependency.from_service_id,
      target: dependency.to_service_id,
      type: "smoothstep",
      data: { dependencyType: dependency.dependency_type ?? "depends_on" },
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
    }));

  return { nodes, edges };
};
