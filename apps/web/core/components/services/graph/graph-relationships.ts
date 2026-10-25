/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IServiceDependency } from "@plane/types";

export type TServiceRelationships = {
  /** Direct neighbours: the selected service plus both ends of its dependencies. */
  nodeIds: Set<string>;
  /** Dependencies attached to the selected service (either direction). */
  edgeIds: Set<string>;
};

/**
 * Resolve the direct relationships of one service. Only first-degree lines are
 * highlighted; lines between two neighbours are deliberately excluded.
 */
export const getServiceRelationships = (
  serviceId: string,
  dependencies: IServiceDependency[]
): TServiceRelationships => {
  const nodeIds = new Set([serviceId]);
  const edgeIds = new Set<string>();

  dependencies.forEach((dependency) => {
    if (dependency.from_service_id === serviceId) {
      edgeIds.add(dependency.id);
      nodeIds.add(dependency.to_service_id);
    }
    if (dependency.to_service_id === serviceId) {
      edgeIds.add(dependency.id);
      nodeIds.add(dependency.from_service_id);
    }
  });

  return { nodeIds, edgeIds };
};
