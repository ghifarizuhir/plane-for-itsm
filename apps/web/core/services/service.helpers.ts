/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TExtensions } from "@plane/editor";
import type {
  IService,
  IServiceDependency,
  IServiceHealthSnapshot,
  TServiceFilters,
  TServiceOrderByOptions,
} from "@plane/types";
// helpers
import { HEALTH_WEIGHT } from "@/services/service-health.helpers";

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

const matchesFilters = (
  service: IService,
  filters: TServiceFilters,
  health: IServiceHealthSnapshot | undefined
): boolean => {
  if (filters.status && filters.status.length > 0 && !filters.status.includes(service.status)) return false;
  if (filters.criticality && filters.criticality.length > 0 && !filters.criticality.includes(service.criticality))
    return false;
  if (filters.type && filters.type.length > 0 && !filters.type.includes(service.type)) return false;
  if (filters.health && filters.health.length > 0) {
    const state = health?.health ?? "unknown";
    if (!filters.health.includes(state)) return false;
  }
  if (filters.incidents && filters.incidents.length > 0 && filters.incidents.includes("active")) {
    if ((health?.incidents.length ?? 0) === 0) return false;
  }
  return true;
};

export const filterServices = (
  services: IService[],
  filters: TServiceFilters,
  searchQuery: string,
  healthMap: Record<string, IServiceHealthSnapshot> = {}
): IService[] =>
  services.filter(
    (service) =>
      service.name.toLowerCase().includes(searchQuery.toLowerCase()) &&
      matchesFilters(service, filters, healthMap[service.id])
  );

export const orderServices = (
  services: IService[],
  orderBy: TServiceOrderByOptions = "health",
  healthMap: Record<string, IServiceHealthSnapshot> = {}
): IService[] => {
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
      case "health": {
        const healthDiff =
          (HEALTH_WEIGHT[healthMap[a.id]?.health ?? "unknown"] ?? 2) -
          (HEALTH_WEIGHT[healthMap[b.id]?.health ?? "unknown"] ?? 2);
        if (healthDiff !== 0) return healthDiff;
        const criticalityDiff = (CRITICALITY_WEIGHT[a.criticality] ?? 9) - (CRITICALITY_WEIGHT[b.criticality] ?? 9);
        if (criticalityDiff !== 0) return criticalityDiff;
        return a.name.localeCompare(b.name);
      }
      case "name":
      default:
        return a.name.localeCompare(b.name);
    }
  });
  return ordered;
};

/**
 * Extensions disabled for the service description editor: image upload (no
 * `SERVICE_*` file-asset type exists yet) and AI. `TExtensions`
 * (`packages/editor/src/types/extensions.ts`) only supports
 * `"ai" | "collaboration-cursor" | "issue-embed" | "slash-commands" |
 * "enter-key" | "image"`, and only `"image"` is actually gated in
 * `CoreEditorExtensions` — table/mention stay enabled for full work-item
 * parity. `collaboration-cursor` is additionally disabled by default in
 * `useEditorFlagging` for all richText editors. `"ai"` duplicates the
 * richText default-disable in `useEditorFlagging` and is kept explicitly as defense-in-depth.
 */
export const SERVICE_DESCRIPTION_DISABLED_EXTENSIONS: TExtensions[] = ["image", "ai"];

/**
 * Derives the plain-text `description` column from editor HTML for the
 * existing Rust `services` table. Block closings become newlines, all other
 * tags are stripped, common entities decoded, blank lines dropped.
 */
export const stripHtmlToText = (html: string): string => {
  if (!html || html.trim() === "") return "";
  return html
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|h[1-6]|li|ul|ol|tr|td|th|table|thead|tbody|tfoot|blockquote|pre)>/gi, "\n")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/gi, " ")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .replace(/&amp;/gi, "&")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .join("\n")
    .trim();
};

/**
 * Trims a service link field and collapses blank input to null so the API
 * clears the column instead of storing whitespace.
 */
export const normalizeServiceUrl = (value: string): string | null => {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
};

/**
 * True when the string parses as an absolute URL. Mirrors the service form's
 * `validateUrl` so inline edits cannot persist values the modal would reject.
 */
export const isValidServiceUrl = (value: string): boolean => {
  const canParse = (URL as unknown as { canParse?: (url: string) => boolean }).canParse;
  if (typeof canParse === "function") return canParse(value);
  try {
    void new URL(value);
    return true;
  } catch {
    return false;
  }
};
