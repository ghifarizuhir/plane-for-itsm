/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ALL_ISSUES } from "@plane/constants";
import type { EIssueLayoutTypes, GroupByColumnTypes, IIssueDisplayFilterOptions, TGroupedIssues } from "@plane/types";

const LIST = "list" as EIssueLayoutTypes;
const KANBAN = "kanban" as EIssueLayoutTypes;
const CALENDAR = "calendar" as EIssueLayoutTypes;
const GANTT = "gantt_chart" as EIssueLayoutTypes;
const SPREADSHEET = "spreadsheet" as EIssueLayoutTypes;

/**
 * Render-only fallback for the work item layout on mobile viewports.
 *
 * The fetched data shape must match what the list renders, so the fallback is only
 * applied for layouts whose data the list can render correctly:
 * - calendar keeps its dedicated mobile agenda and date-windowed data.
 * - kanban falls back unless it is both grouped and sub-grouped (only then is the data nested).
 * - spreadsheet and gantt fetch flat data, which the list renders ungrouped.
 *
 * The persisted value is never written back, so widening the viewport restores the
 * user's chosen layout.
 */
export const resolveWorkItemLayout = (
  displayFilters: IIssueDisplayFilterOptions | undefined,
  isMobileViewport: boolean
): EIssueLayoutTypes | undefined => {
  const layout = displayFilters?.layout as EIssueLayoutTypes | undefined;
  if (!isMobileViewport || !layout) return layout;
  if (layout === CALENDAR) return CALENDAR;
  if (layout === KANBAN) return displayFilters?.group_by && displayFilters?.sub_group_by ? KANBAN : LIST;
  if (layout === GANTT || layout === SPREADSHEET) return LIST;
  return layout;
};

// Layout-agnostic data-shape helpers: used by the list renderer on all viewports, not viewport-gated.

/**
 * Flat responses (spreadsheet/gantt fetches, ungrouped lists) store their ids under
 * ALL_ISSUES; grouped and sub-grouped responses never do.
 */
export const isFlatGroupedIssueData = (groupedIssueIds: TGroupedIssues | undefined): boolean =>
  groupedIssueIds?.[ALL_ISSUES] !== undefined;

/**
 * Grouped rendering only makes sense when the fetched data is actually grouped by the
 * persisted key; flat data must render ungrouped.
 */
export const resolveRenderedGroupBy = (
  groupedIssueIds: TGroupedIssues | undefined,
  groupBy: GroupByColumnTypes | null
): GroupByColumnTypes | null => (isFlatGroupedIssueData(groupedIssueIds) ? null : groupBy);
