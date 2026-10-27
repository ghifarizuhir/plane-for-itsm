/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// components
import { observer } from "mobx-react";
import { useParams, usePathname } from "next/navigation";
import useSWR from "swr";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { InboxOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { AppSidebarItem } from "@/components/sidebar/sidebar-item";
import { AiAssistantSidebarToggle } from "@/components/ai/assistant-sidebar/toggle-button";
import { HelpMenuRoot } from "@/components/workspace/sidebar/help-section/root";
import { UserMenuRoot } from "@/components/workspace/sidebar/user-menu-root";
import { WorkspaceMenuRoot } from "@/components/workspace/sidebar/workspace-menu-root";
import { useWorkspaceNotifications } from "@/hooks/store/notifications";
import { useInstance } from "@/hooks/store/use-instance";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
import { WorkspaceModeMenu } from "./workspace-mode-menu";

export const TopNavigationRoot = observer(function TopNavigationRoot() {
  // router
  const { workspaceSlug } = useParams();
  const pathname = usePathname();

  // store hooks
  const { unreadNotificationsCount, getUnreadNotificationsCount } = useWorkspaceNotifications();
  const { config } = useInstance();
  const { t } = useTranslation();
  const { isGalileoPath } = useWorkspacePaths();

  // Fetch notification count
  useSWR(
    workspaceSlug ? "WORKSPACE_UNREAD_NOTIFICATION_COUNT" : null,
    workspaceSlug ? () => getUnreadNotificationsCount(workspaceSlug.toString()) : null
  );

  // Calculate notification count
  const isMentionsEnabled = unreadNotificationsCount.mention_unread_notifications_count > 0;
  const totalNotifications = isMentionsEnabled
    ? unreadNotificationsCount.mention_unread_notifications_count
    : unreadNotificationsCount.total_unread_notifications_count;

  return (
    <div className="z-[27] flex min-h-10 w-full items-center bg-canvas px-3.5 transition-all duration-300 md:hidden">
      {/* Workspace Menu */}
      <div className="flex-1 shrink-0">
        <WorkspaceMenuRoot variant="top-navigation" />
      </div>
      {/* Additional Actions */}
      <div className="flex flex-1 shrink-0 items-center justify-end gap-1">
        <WorkspaceModeMenu />
        {config?.has_llm_configured && !isGalileoPath && <AiAssistantSidebarToggle />}
        <Tooltip label={t("sidebar.inbox")} side="bottom">
          <AppSidebarItem
            variant="link"
            item={{
              href: `/${workspaceSlug?.toString()}/notifications/`,
              label: t("sidebar.inbox"),
              showLabel: false,
              icon: (
                <div className="relative">
                  <InboxOutline className="size-5" />
                  {totalNotifications > 0 && (
                    <span className="absolute top-0 right-0 size-2 rounded-full bg-danger-primary" />
                  )}
                </div>
              ),
              isActive: pathname?.includes("/notifications/"),
            }}
          />
        </Tooltip>
        <HelpMenuRoot />
        <div className="flex size-8 items-center justify-center rounded-md hover:bg-layer-1-hover">
          <UserMenuRoot />
        </div>
      </div>
    </div>
  );
});
