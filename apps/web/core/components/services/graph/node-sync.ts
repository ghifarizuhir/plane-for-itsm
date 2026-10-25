/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { ILiteService, IService } from "@plane/types";

type TServiceNodeData = {
  service?: IService | ILiteService;
  statusLabel?: string;
  criticalityLabel?: string;
  typeLabel?: string;
  kindLabel?: string;
  healthLabel?: string;
  ownerProjectIdentifier?: string;
  ownerProjectName?: string;
  health?: string;
  incidents?: unknown[];
  lastDeployedAt?: string | null;
  noDeploymentLabel?: string;
  dimmed?: boolean;
};

const hasSamePosition = (left: Node["position"], right: Node["position"]) => left.x === right.x && left.y === right.y;

const hasSameData = (left?: TServiceNodeData, right?: TServiceNodeData) =>
  left?.service === right?.service &&
  left?.statusLabel === right?.statusLabel &&
  left?.criticalityLabel === right?.criticalityLabel &&
  left?.typeLabel === right?.typeLabel &&
  left?.kindLabel === right?.kindLabel &&
  left?.healthLabel === right?.healthLabel &&
  left?.ownerProjectIdentifier === right?.ownerProjectIdentifier &&
  left?.ownerProjectName === right?.ownerProjectName &&
  left?.health === right?.health &&
  (left?.incidents?.length ?? 0) === (right?.incidents?.length ?? 0) &&
  left?.lastDeployedAt === right?.lastDeployedAt &&
  left?.noDeploymentLabel === right?.noDeploymentLabel &&
  left?.dimmed === right?.dimmed;

/**
 * Merges freshly computed layout nodes into React Flow state while preserving
 * selection and never fighting an in-progress drag.
 */
export function syncGraphNodes(previous: Node[], next: Node[]): Node[] {
  const previousById = new Map(previous.map((node) => [node.id, node]));
  let changed = previous.length !== next.length;

  const merged = next.map((node) => {
    const current = previousById.get(node.id);
    if (!current) {
      changed = true;
      return node;
    }
    if (current.dragging) return current;

    const same =
      hasSamePosition(current.position, node.position) &&
      hasSameData(current.data as TServiceNodeData, node.data as TServiceNodeData);

    if (same) return current;
    changed = true;
    return { ...node, selected: current.selected };
  });

  const sameOrder = previous.length === next.length && previous.every((node, index) => node.id === next[index]?.id);

  return changed || !sameOrder ? merged : previous;
}
