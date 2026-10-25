/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { ILiteService, IService, TServiceKind } from "@plane/types";
import { orderBy } from "lodash-es";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { SERVICE_KIND_ORDER } from "../kind";
import { getServiceGroupKey } from "./graph-groups";
import { CLUSTER_HEADER_HEIGHT, CLUSTER_PADDING, SERVICE_NODE_HEIGHT, SERVICE_NODE_WIDTH } from "./use-graph-layout";

export type TServiceCluster = {
  id: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  label: string;
  count: number;
  serviceIds: string[];
  x: number;
  y: number;
  width: number;
  height: number;
};

type TClusterBucket = {
  key: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  count: number;
  serviceIds: string[];
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
};

const kindRank = (kind: TServiceKind | null) => (kind ? SERVICE_KIND_ORDER.indexOf(kind) : SERVICE_KIND_ORDER.length);

const bucketRank = (bucket: TClusterBucket) => (bucket.restricted ? 2 : bucket.projectId === null ? 1 : 0);

export const getServiceClusters = (
  nodes: Node[],
  services: (IService | ILiteService)[],
  projectLookup: Record<string, { identifier: string; name: string }> | undefined,
  t: (key: string) => string
): TServiceCluster[] => {
  const serviceById = new Map(services.map((service) => [service.id, service]));
  const buckets = new Map<string, TClusterBucket>();

  nodes.forEach((node) => {
    const service = serviceById.get(node.id);
    if (!service) return;

    const key = getServiceGroupKey(service);
    const width = node.measured?.width ?? SERVICE_NODE_WIDTH;
    const height = node.measured?.height ?? SERVICE_NODE_HEIGHT;
    const bucket = buckets.get(key) ?? {
      key,
      projectId: isLiteService(service) ? null : service.owner_project_id,
      kind: isLiteService(service) ? null : service.kind,
      restricted: isLiteService(service),
      count: 0,
      serviceIds: [],
      minX: Number.POSITIVE_INFINITY,
      minY: Number.POSITIVE_INFINITY,
      maxX: Number.NEGATIVE_INFINITY,
      maxY: Number.NEGATIVE_INFINITY,
    };

    bucket.count += 1;
    bucket.serviceIds.push(service.id);
    bucket.minX = Math.min(bucket.minX, node.position.x);
    bucket.minY = Math.min(bucket.minY, node.position.y);
    bucket.maxX = Math.max(bucket.maxX, node.position.x + width);
    bucket.maxY = Math.max(bucket.maxY, node.position.y + height);
    buckets.set(key, bucket);
  });

  // NOTE: do not use `.sort()` here — the repo's `oxlint --fix` pre-commit
  // hook rewrites it to `.toSorted()`, which the TS lib target does not know.
  const ordered = orderBy(
    [...buckets.values()],
    [(bucket) => bucketRank(bucket), (bucket) => bucket.projectId ?? "", (bucket) => kindRank(bucket.kind)],
    ["asc", "asc", "asc"]
  );

  const namedProjectIds = new Set(
    ordered.filter((bucket) => bucket.projectId !== null).map((bucket) => bucket.projectId as string)
  );
  const hasUnassigned = ordered.some((bucket) => !bucket.restricted && bucket.projectId === null);
  const showProjectPrefix = namedProjectIds.size + (hasUnassigned ? 1 : 0) > 1;

  return ordered.map((bucket) => {
    const kindLabel = bucket.kind ? t(`service.kind_values.${bucket.kind}`) : null;
    const projectLabel = bucket.restricted
      ? t("service.graph.restricted")
      : bucket.projectId
        ? (projectLookup?.[bucket.projectId]?.identifier ?? null)
        : t("service.graph.unassigned");
    const parts = [showProjectPrefix ? projectLabel : null, kindLabel].filter(Boolean) as string[];
    const label = parts.length > 0 ? parts.join(" · ") : (projectLabel ?? "");

    return {
      id: bucket.key,
      projectId: bucket.projectId,
      kind: bucket.kind,
      restricted: bucket.restricted,
      label,
      count: bucket.count,
      serviceIds: bucket.serviceIds,
      x: bucket.minX - CLUSTER_PADDING,
      y: bucket.minY - CLUSTER_HEADER_HEIGHT - CLUSTER_PADDING,
      width: bucket.maxX - bucket.minX + CLUSTER_PADDING * 2,
      height: bucket.maxY - bucket.minY + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2,
    };
  });
};
