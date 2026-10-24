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

export const SERVICE_NODE_WIDTH = 256;
export const SERVICE_NODE_HEIGHT = 104;

export const getLayoutedElements = (
  services: (IService | ILiteService)[],
  dependencies: IServiceDependency[]
): { nodes: Node[]; edges: Edge[] } => {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", nodesep: 48, ranksep: 112 });

  const ids = new Set(services.map((s) => s.id));
  services.forEach((s) => graph.setNode(s.id, { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT }));
  dependencies.forEach((d) => {
    if (ids.has(d.from_service_id) && ids.has(d.to_service_id)) graph.setEdge(d.from_service_id, d.to_service_id);
  });

  dagre.layout(graph);

  const nodes: Node[] = services.map((service) => {
    const pos = graph.node(service.id);
    const storedPosition = isLiteService(service) ? null : service.position;
    return {
      id: service.id,
      type: "service",
      data: { service, accessLevel: isLiteService(service) ? "lite" : "full" },
      position: storedPosition ?? {
        x: (pos?.x ?? 0) - SERVICE_NODE_WIDTH / 2,
        y: (pos?.y ?? 0) - SERVICE_NODE_HEIGHT / 2,
      },
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
    } as Node;
  });

  const edges: Edge[] = dependencies
    .filter((d) => ids.has(d.from_service_id) && ids.has(d.to_service_id))
    .map((d) => ({
      id: d.id,
      source: d.from_service_id,
      target: d.to_service_id,
      type: "smoothstep",
      data: { dependencyType: d.dependency_type ?? "depends_on" },
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
    }));

  return { nodes, edges };
};
