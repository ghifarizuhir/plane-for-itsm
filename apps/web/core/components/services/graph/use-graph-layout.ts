/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Edge, Node } from "@xyflow/react";
import { MarkerType, Position } from "@xyflow/react";
import { orderBy } from "lodash-es";
import type { ILiteService, IService, IServiceDependency } from "@plane/types";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { getServiceGroups } from "./graph-groups";

export const SERVICE_NODE_WIDTH = 256;
export const SERVICE_NODE_HEIGHT = 104;
export const GROUP_NODE_COLUMN_GAP = 32;
export const GROUP_NODE_ROW_GAP = 24;
export const GROUP_GAP = 32;
export const GROUP_MAX_ROW_WIDTH = 1900;
export const CLUSTER_PADDING = 16;
export const CLUSTER_HEADER_HEIGHT = 28;

type TPosition = { x: number; y: number };

type TGroupLayout = {
  positions: Map<string, TPosition>;
  minX: number;
  minY: number;
  width: number;
  height: number;
};

type TPackingRow = {
  y: number;
  usedWidth: number;
  height: number;
};

const centroid = (positions: TPosition[]): TPosition => ({
  x: positions.reduce((sum, position) => sum + position.x, 0) / positions.length,
  y: positions.reduce((sum, position) => sum + position.y, 0) / positions.length,
});

/**
 * Lay a kind group out as a compact card grid. A tall single column of service
 * cards forces fitView to zoom so far out that the whole map is illegible; a
 * 3-row grid keeps each cluster close to the canvas aspect ratio instead.
 * Groups of four use two rows, smaller groups keep one column.
 */
const layoutGroup = (services: (IService | ILiteService)[]): TGroupLayout => {
  const rows = services.length <= 3 ? services.length : services.length <= 4 ? 2 : 3;
  const columns = Math.ceil(services.length / rows);
  const usedRows = Math.min(services.length, rows);
  const positions = new Map<string, TPosition>();

  services.forEach((service, index) => {
    const column = Math.floor(index / rows);
    const row = index % rows;
    positions.set(service.id, {
      x: column * (SERVICE_NODE_WIDTH + GROUP_NODE_COLUMN_GAP),
      y: row * (SERVICE_NODE_HEIGHT + GROUP_NODE_ROW_GAP),
    });
  });

  return {
    positions,
    minX: 0,
    minY: 0,
    width: (columns - 1) * (SERVICE_NODE_WIDTH + GROUP_NODE_COLUMN_GAP) + SERVICE_NODE_WIDTH,
    height: (usedRows - 1) * (SERVICE_NODE_HEIGHT + GROUP_NODE_ROW_GAP) + SERVICE_NODE_HEIGHT,
  };
};

export const getLayoutedElements = (
  services: (IService | ILiteService)[],
  dependencies: IServiceDependency[]
): { nodes: Node[]; edges: Edge[] } => {
  const groups = getServiceGroups(services);
  const layouts = groups.map((group) => ({ group, layout: layoutGroup(group.services) }));

  // First-fit decreasing: widest clusters claim a shelf first, then narrower
  // ones backfill earlier shelves. This leaves the bottom-right corner of the
  // canvas free for the minimap instead of always stacking a full bottom row.
  const ordered = orderBy(layouts, [(item) => item.layout.width], ["desc"]);
  const slots = new Map<string, TPosition>();
  const rows: TPackingRow[] = [];

  ordered.forEach(({ group, layout }) => {
    const outerWidth = layout.width + CLUSTER_PADDING * 2;
    const outerHeight = layout.height + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2;
    let row = rows.find(
      (candidate) => candidate.usedWidth + (candidate.usedWidth > 0 ? GROUP_GAP : 0) + outerWidth <= GROUP_MAX_ROW_WIDTH
    );

    if (!row) {
      const previous = rows.at(-1);
      row = { y: previous ? previous.y + previous.height + GROUP_GAP : 0, usedWidth: 0, height: 0 };
      rows.push(row);
    }

    const x = row.usedWidth > 0 ? row.usedWidth + GROUP_GAP : 0;
    slots.set(group.key, { x, y: row.y });
    row.usedWidth = x + outerWidth;
    row.height = Math.max(row.height, outerHeight);
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
      pathOptions: { borderRadius: 12 },
      data: { dependencyType: dependency.dependency_type ?? "depends_on" },
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
    }));

  return { nodes, edges };
};
