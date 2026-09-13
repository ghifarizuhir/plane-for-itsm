/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Edge, Node } from "@xyflow/react";
import dagre from "dagre";
import type { IService, IServiceDependency } from "@plane/types";

export const SERVICE_NODE_WIDTH = 220;
export const SERVICE_NODE_HEIGHT = 72;

export const getLayoutedElements = (
  services: IService[],
  dependencies: IServiceDependency[]
): { nodes: Node[]; edges: Edge[] } => {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 90 });

  const ids = new Set(services.map((s) => s.id));
  services.forEach((s) => graph.setNode(s.id, { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT }));
  dependencies.forEach((d) => {
    if (ids.has(d.from_service_id) && ids.has(d.to_service_id)) graph.setEdge(d.from_service_id, d.to_service_id);
  });

  dagre.layout(graph);

  const nodes: Node[] = services.map((service) => {
    const pos = graph.node(service.id);
    return {
      id: service.id,
      type: "service",
      data: { service },
      position: service.position ?? {
        x: (pos?.x ?? 0) - SERVICE_NODE_WIDTH / 2,
        y: (pos?.y ?? 0) - SERVICE_NODE_HEIGHT / 2,
      },
      sourcePosition: "right",
      targetPosition: "left",
    } as Node;
  });

  const edges: Edge[] = dependencies
    .filter((d) => ids.has(d.from_service_id) && ids.has(d.to_service_id))
    .map((d) => ({
      id: d.id,
      source: d.from_service_id,
      target: d.to_service_id,
      type: "smoothstep",
    }));

  return { nodes, edges };
};
