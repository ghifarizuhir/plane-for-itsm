/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceGroup = {
  key: string;
  projectId: string | null;
  identifier: string | null;
  name: string;
  serviceIds: string[];
};

type TServiceLike = {
  name: string;
  owner_project_id: string | null;
};

type TProjectLike = {
  name: string;
  identifier: string;
};

const UNASSIGNED_GROUP_KEY = "unassigned";

const compareNames = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

export function buildWorkspaceServiceGroups(
  serviceIds: string[],
  getServiceById: (id: string) => TServiceLike | undefined,
  getProjectById: (id: string) => TProjectLike | undefined
): TServiceGroup[] {
  const groups = new Map<string, TServiceGroup>();
  const seenServiceIds = new Set<string>();

  for (const serviceId of serviceIds) {
    if (seenServiceIds.has(serviceId)) continue;
    seenServiceIds.add(serviceId);

    const service = getServiceById(serviceId);
    if (!service) continue;

    const ownerProjectId = service.owner_project_id;
    const project = ownerProjectId ? getProjectById(ownerProjectId) : undefined;
    const projectId = ownerProjectId && project ? ownerProjectId : null;
    const key = projectId ?? UNASSIGNED_GROUP_KEY;
    const existingGroup = groups.get(key);

    if (existingGroup) {
      existingGroup.serviceIds.push(serviceId);
      continue;
    }

    groups.set(key, {
      key,
      projectId,
      identifier: project?.identifier ?? null,
      name: project?.name ?? "",
      serviceIds: [serviceId],
    });
  }

  const allGroups = [...groups.values()];

  for (const group of allGroups) {
    group.serviceIds = [...group.serviceIds].toSorted((a, b) => {
      const nameA = getServiceById(a)?.name ?? "";
      const nameB = getServiceById(b)?.name ?? "";
      return compareNames(nameA, nameB) || a.localeCompare(b);
    });
  }

  allGroups.sort((a, b) => {
    const aUnassigned = a.projectId === null;
    const bUnassigned = b.projectId === null;
    if (aUnassigned !== bUnassigned) return aUnassigned ? 1 : -1;
    return compareNames(a.name, b.name) || a.key.localeCompare(b.key);
  });

  return allGroups;
}
