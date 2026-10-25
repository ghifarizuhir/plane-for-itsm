/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { cn } from "../utils";
import type { TTabNavigationListProps } from "./tab-navigation-types";

export function TabNavigationList({ children, className }: TTabNavigationListProps) {
  return <div className={cn("relative flex items-center gap-1 rounded-md", className)}>{children}</div>;
}

TabNavigationList.displayName = "TabNavigationList";
