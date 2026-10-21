/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

type TRichFilterNode = Record<string, unknown>;

/**
 * Adds a scope node to an existing rich-work-item-filter expression.
 *
 * The api-rs project issue endpoints only honor the rich `filters` JSON
 * (legacy `?module=` / `?cycle=` params are accepted-and-ignored), so
 * entity-scoped views must merge their scope into the rich expression.
 */
export const appendRichFilterNode = (
  currentFilters: string | boolean | undefined,
  node: TRichFilterNode
): string => {
  if (typeof currentFilters !== "string" || currentFilters === "{}") return JSON.stringify(node);

  try {
    const parsed: unknown = JSON.parse(currentFilters);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed) || Object.keys(parsed).length === 0)
      return JSON.stringify(node);
    return JSON.stringify({ and: [parsed, node] });
  } catch {
    return JSON.stringify(node);
  }
};
