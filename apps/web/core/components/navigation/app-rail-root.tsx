/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

"use client";

import type { CSSProperties } from "react";
import { observer } from "mobx-react";
import { useParams, usePathname } from "next/navigation";
import useSWR from "swr";
import { InboxOutline, SettingsOutline, TickOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { ContextMenu } from "@plane/propel/context-menu";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
// components
import { AiAssistantSidebarToggle } from "@/components/ai/assistant-sidebar/toggle-button";
import { AppSidebarItem } from "@/components/sidebar/sidebar-item";
import { HelpMenuRoot } from "@/components/workspace/sidebar/help-section/root";
import { UserMenuRoot } from "@/components/workspace/sidebar/user-menu-root";
import { WorkspaceMenuRoot } from "@/components/workspace/sidebar/workspace-menu-root";
// hooks
import { useWorkspaceNotifications } from "@/hooks/store/notifications";
import { useInstance } from "@/hooks/store/use-instance";
import { useAppRailPreferences } from "@/hooks/use-navigation-preferences";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
// local imports
import { AppSidebarItemsRoot } from "./items-root";

export const AppRailRoot = observer(function AppRailRoot() {
  // router
  const { workspaceSlug } = useParams();
  const pathname = usePathname();
  const slug = workspaceSlug?.toString() ?? "";
  // preferences
  const { preferences, updateDisplayMode } = useAppRailPreferences();
  const { isSettingsPath, isGalileoPath } = useWorkspacePaths();
  // store hooks
  const { config } = useInstance();
  const { unreadNotificationsCount, getUnreadNotificationsCount } = useWorkspaceNotifications();
  // i18n
  const { t } = useTranslation();

  useSWR(
    workspaceSlug ? "WORKSPACE_UNREAD_NOTIFICATION_COUNT" : null,
    workspaceSlug ? () => getUnreadNotificationsCount(workspaceSlug.toString()) : null
  );

  // derived values
  const showLabel = preferences.displayMode === "icon_with_label";
  const railWidth = showLabel ? "4rem" : "2.75rem";
  const totalNotifications =
    unreadNotificationsCount.mention_unread_notifications_count > 0
      ? unreadNotificationsCount.mention_unread_notifications_count
      : unreadNotificationsCount.total_unread_notifications_count;
  const railStyle = {
    width: railWidth,
    display: "block",
    "--app-rail-width": railWidth,
  } as CSSProperties;

  return (
    <div className="z-[26] h-full flex-shrink-0 bg-canvas transition-all duration-300 ease-in-out" style={railStyle}>
      <ContextMenu>
        <ContextMenu.Trigger className="h-full">
          <div className="flex h-full flex-col px-1 py-2">
            <div className="flex w-full items-center justify-center">
              <WorkspaceMenuRoot variant="sidebar" />
            </div>
            <div className="mx-1 my-2 border-t border-subtle" />
            <div className={cn("flex flex-col", showLabel ? "gap-1" : "gap-0.5")}>
              <AppSidebarItemsRoot showLabel={showLabel} size="sm" />
            </div>
            <div className="flex-1" />
            <div className="flex flex-col items-center gap-0.5">
              {config?.has_llm_configured && !isGalileoPath && <AiAssistantSidebarToggle compact />}
              <Tooltip label={t("sidebar.inbox")} side="right">
                <AppSidebarItem
                  variant="link"
                  size="sm"
                  item={{
                    href: `/${slug}/notifications/`,
                    label: t("sidebar.inbox"),
                    showLabel: false,
                    isActive: pathname?.includes("/notifications/"),
                    icon: (
                      <div className="relative">
                        <InboxOutline className="size-5" />
                        {totalNotifications > 0 && (
                          <span className="absolute top-0 right-0 size-2 rounded-full bg-danger-primary" />
                        )}
                      </div>
                    ),
                  }}
                />
              </Tooltip>
              <HelpMenuRoot compact placement="right-start" />
              <UserMenuRoot compact placement="right-start" />
            </div>
            <div className="mx-1 my-2 border-t border-subtle" />
            <AppSidebarItem
              size="sm"
              item={{
                label: t("settings"),
                icon: <SettingsOutline className="size-5" />,
                href: `/${slug}/settings`,
                isActive: isSettingsPath,
                showLabel,
              }}
            />
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
