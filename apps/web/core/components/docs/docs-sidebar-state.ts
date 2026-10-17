/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DOC_TOPIC_GROUPS, type TDocGroupKey } from "./registry";

export const DOCS_SIDEBAR_GROUPS_KEY = "plane-docs-sidebar-groups";

export type TGroupOverrides = Partial<Record<TDocGroupKey, boolean>>;

export function parseGroupOverrides(raw: string | null): TGroupOverrides {
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    const overrides: TGroupOverrides = {};
    for (const group of DOC_TOPIC_GROUPS) {
      const value = (parsed as Record<string, unknown>)[group.key];
      if (typeof value === "boolean") overrides[group.key] = value;
    }
    return overrides;
  } catch {
    return {};
  }
}

export function serializeGroupOverrides(overrides: TGroupOverrides): string {
  return JSON.stringify(overrides);
}

export function resolveGroupOpen(group: TDocGroupKey, activeGroup: TDocGroupKey, overrides: TGroupOverrides): boolean {
  return overrides[group] ?? group === activeGroup;
}

export function toggleGroupOverrides(overrides: TGroupOverrides, group: TDocGroupKey, open: boolean): TGroupOverrides {
  return { ...overrides, [group]: open };
}
