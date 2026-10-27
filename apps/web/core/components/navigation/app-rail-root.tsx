/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

"use client";

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { SettingsOutline, TickOutline } from "@makeplane/propel/icons";
import { ContextMenu } from "@plane/propel/context-menu";
import { cn } from "@plane/utils";
// components
import { AppSidebarItem } from "@/components/sidebar/sidebar-item";
// hooks
import { useAppRailPreferences } from "@/hooks/use-navigation-preferences";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
// local imports
import { AppSidebarItemsRoot } from "./items-root";

export const AppRailRoot = observer(function AppRailRoot() {
  // router
  const { workspaceSlug } = useParams();
  // preferences
  const { preferences, updateDisplayMode } = useAppRailPreferences();
  const { isSettingsPath } = useWorkspacePaths();
  // derived values
  const showLabel = preferences.displayMode === "icon_with_label";
  const railWidth = showLabel ? "3.75rem" : "3rem";

  return (
    <div
      className="z-[26] h-full flex-shrink-0 bg-canvas transition-all duration-300 ease-in-out"
      style={{
        width: railWidth,
        display: "block",
      }}
    >
      <ContextMenu>
        <ContextMenu.Trigger className="h-full">
          <div className="flex h-full flex-col gap-4 px-2 py-3">
            <div
              className={cn("flex flex-col", {
                "gap-4": showLabel,
                "gap-3": !showLabel,
              })}
            >
              <AppSidebarItemsRoot showLabel={showLabel} />
              <div className="mx-2 border-t border-strong" />
              <AppSidebarItem
                item={{
                  label: "Settings",
                  icon: <SettingsOutline className="size-5" />,
                  href: `/${workspaceSlug}/settings`,
                  isActive: isSettingsPath,
                  showLabel,
                }}
              />
            </div>
          </div>
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Content positionerClassName="z-30" className="outline-none">
            <ContextMenu.Item onClick={() => updateDisplayMode("icon_only")}>
              <div className="flex w-full items-center justify-between gap-2">
                <span className="text-11">Icon only</span>
                {preferences.displayMode === "icon_only" && <TickOutline className="size-3.5" />}
              </div>
            </ContextMenu.Item>
            <ContextMenu.Item onClick={() => updateDisplayMode("icon_with_label")}>
              <div className="flex w-full items-center justify-between gap-2">
                <span className="text-11">Icon with name</span>
                {preferences.displayMode === "icon_with_label" && <TickOutline className="size-3.5" />}
              </div>
            </ContextMenu.Item>
          </ContextMenu.Content>
        </ContextMenu.Portal>
      </ContextMenu>
    </div>
  );
});
