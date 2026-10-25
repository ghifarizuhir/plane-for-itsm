/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ILiteService, IService, TServiceKind } from "@plane/types";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { SERVICE_KIND_ORDER } from "../kind";

export const RESTRICTED_GROUP_KEY = "__restricted__";
export const UNASSIGNED_GROUP_PREFIX = "__unassigned__";

export type TServiceGroup = {
  key: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  services: (IService | ILiteService)[];
};

const kindRank = (kind: TServiceKind | null) => (kind ? SERVICE_KIND_ORDER.indexOf(kind) : SERVICE_KIND_ORDER.length);

const groupRank = (group: TServiceGroup) => (group.restricted ? 2 : group.projectId === null ? 1 : 0);

export const getServiceGroupKey = (service: IService | ILiteService): string => {
  if (isLiteService(service)) return RESTRICTED_GROUP_KEY;
  const projectKey = service.owner_project_id ?? UNASSIGNED_GROUP_PREFIX;
  return `${projectKey}::${service.kind}`;
};

export const getServiceGroups = (services: (IService | ILiteService)[]): TServiceGroup[] => {
  const groups = new Map<string, TServiceGroup>();

  services.forEach((service) => {
    const key = getServiceGroupKey(service);
    const existing = groups.get(key);
    if (existing) {
      existing.services.push(service);
      return;
    }
    groups.set(key, {
      key,
      projectId: isLiteService(service) ? null : service.owner_project_id,
      kind: isLiteService(service) ? null : service.kind,
      restricted: isLiteService(service),
      services: [service],
    });
  });

  const sortedGroups = [...groups.values()];
  sortedGroups.forEach((group) => {
    group.services = [...group.services].toSorted((a, b) => a.name.localeCompare(b.name));
  });

  return sortedGroups.toSorted(
    (a, b) =>
      groupRank(a) - groupRank(b) ||
      (a.projectId ?? "").localeCompare(b.projectId ?? "") ||
      kindRank(a.kind) - kindRank(b.kind)
  );
};
