/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { EIssueLayoutTypes, TGroupedIssues } from "@plane/types";

const MOBILE_WORK_ITEM_LAYOUT = "list" as EIssueLayoutTypes;

/**
 * Render-only fallback for the work item layout.
 *
 * On a mobile viewport a persisted desktop-only layout (kanban/calendar/gantt/spreadsheet)
 * renders as list. The persisted value is never written back, so widening the viewport
 * restores the user's chosen layout.
 */
export const resolveWorkItemLayout = (
  layout: EIssueLayoutTypes | undefined,
  isMobileViewport: boolean
): EIssueLayoutTypes | undefined => {
  if (!isMobileViewport || !layout) return layout;
  return layout === MOBILE_WORK_ITEM_LAYOUT ? layout : MOBILE_WORK_ITEM_LAYOUT;
};

/**
 * Flattens any grouped or sub-grouped issue map into a single ordered list of issue ids.
 *
 * The mobile list fallback can receive data shaped for a different layout (flat for
 * spreadsheet/gantt/calendar, grouped for kanban, nested for kanban with sub-grouping),
 * so the list renders from the flattened ids instead of mismatched group keys.
 */
export const flattenGroupedIssueIds = (groupedIssueIds: TGroupedIssues): string[] =>
  Object.values(groupedIssueIds).flatMap((value) => {
    if (Array.isArray(value)) return value;
    const subGrouped = value as unknown as TGroupedIssues;
    return Object.values(subGrouped).flatMap((subValue) => (Array.isArray(subValue) ? subValue : []));
  });
