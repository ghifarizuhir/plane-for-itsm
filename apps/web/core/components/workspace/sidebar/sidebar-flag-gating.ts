/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import type { IWorkspaceSidebarNavigationItem } from "@plane/constants";
import type { TWorkspaceSettings } from "@plane/types";

/** Sidebar items gated by workspace feature flags (program P1/S2). */
export type TWorkspaceNavFlag = Pick<TWorkspaceSettings, "cycle_view" | "module_view" | "page_view" | "intake_view">;

export type TFlaggableNavItem = Pick<IWorkspaceSidebarNavigationItem, "flag">;

/**
 * Whether a sidebar item stays visible under the workspace feature flags.
 * Fail-open: missing settings (or no flag on the item) keeps it visible.
 */
export function isNavItemVisibleByFlag(
  item: TFlaggableNavItem,
  settings: TWorkspaceNavFlag | null | undefined
): boolean {
  if (item.flag === undefined) return true;
  return settings?.[item.flag] !== false;
}
