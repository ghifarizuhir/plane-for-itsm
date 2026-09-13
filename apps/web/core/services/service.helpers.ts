/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceDependency, TServiceFilters, TServiceOrderByOptions } from "@plane/types";

/**
 * Adding `from -> to` is invalid if it is a self-loop.
 */
export const isSelfDependency = (from: string, to: string) => from === to;

/**
 * Adding `from -> to` is invalid if the exact edge already exists.
 */
export const dependencyExists = (dependencies: IServiceDependency[], from: string, to: string): boolean =>
  dependencies.some((d) => d.from_service_id === from && d.to_service_id === to);

/**
 * Adding `from -> to` creates a cycle if `to` can already reach `from`
 * by following existing edges (A -> B means A depends on B).
 */
export const wouldCreateCycle = (dependencies: IServiceDependency[], from: string, to: string): boolean => {
  const adjacency = new Map<string, string[]>();
  for (const dep of dependencies) {
    const children = adjacency.get(dep.from_service_id) ?? [];
    children.push(dep.to_service_id);
    adjacency.set(dep.from_service_id, children);
  }
  const stack: string[] = [to];
  const seen = new Set<string>([to]);
  while (stack.length > 0) {
    const current = stack.pop() as string;
    if (current === from) return true;
    for (const child of adjacency.get(current) ?? []) {
      if (!seen.has(child)) {
        seen.add(child);
        stack.push(child);
      }
    }
  }
  return false;
};

/**
 * Returns an error message when the dependency cannot be added, else null.
 */
export const validateDependency = (dependencies: IServiceDependency[], from: string, to: string): string | null => {
  if (isSelfDependency(from, to)) return "A service cannot depend on itself.";
  if (dependencyExists(dependencies, from, to)) return "This dependency already exists.";
  if (wouldCreateCycle(dependencies, from, to)) return "This dependency would create a cycle.";
  return null;
};

const CRITICALITY_WEIGHT: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };

const matchesFilters = (service: IService, filters: TServiceFilters): boolean => {
  if (filters.status && filters.status.length > 0 && !filters.status.includes(service.status)) return false;
  if (filters.criticality && filters.criticality.length > 0 && !filters.criticality.includes(service.criticality))
    return false;
  if (filters.type && filters.type.length > 0 && !filters.type.includes(service.type)) return false;
  return true;
};

export const filterServices = (services: IService[], filters: TServiceFilters, searchQuery: string): IService[] =>
  services.filter((s) => s.name.toLowerCase().includes(searchQuery.toLowerCase()) && matchesFilters(s, filters));

export const orderServices = (services: IService[], orderBy: TServiceOrderByOptions = "name"): IService[] => {
  const ordered = [...services];
  // oxlint-disable-next-line unicorn/no-array-sort
  ordered.sort((a, b) => {
    switch (orderBy) {
      case "-created_at":
        return b.created_at.localeCompare(a.created_at);
      case "-updated_at":
        return b.updated_at.localeCompare(a.updated_at);
      case "criticality":
        return (CRITICALITY_WEIGHT[a.criticality] ?? 9) - (CRITICALITY_WEIGHT[b.criticality] ?? 9);
      case "status":
        return a.status.localeCompare(b.status);
      case "name":
      default:
        return a.name.localeCompare(b.name);
    }
  });
  return ordered;
};
