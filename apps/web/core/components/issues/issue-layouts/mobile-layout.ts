/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { EIssueLayoutTypes } from "@plane/types";

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
