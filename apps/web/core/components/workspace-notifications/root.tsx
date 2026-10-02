/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { observer } from "mobx-react";
import useSWR from "swr";
// plane imports
import { ENotificationLoader, ENotificationQueryParamType } from "@plane/constants";
import { EmptyStateCompact } from "@plane/propel/empty-state";
import { cn } from "@plane/utils";
// components
import { LogoSpinner } from "@/components/common/logo-spinner";
// hooks
import { useWorkspaceNotifications } from "@/hooks/store/notifications";
import { useNotification } from "@/hooks/store/notifications/use-notification";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspaceIssueProperties } from "@/hooks/use-workspace-issue-properties";
import { useNotificationPreview } from "@/hooks/use-notification-preview";
// lib
import { isScheduleRunNotification } from "@/lib/ai-schedule";
import { isReviewRequestNotification, isReviewSessionNotification } from "@/lib/review-notification";
import { isWarRoomNotification } from "@/lib/war-room-notification";
// local imports
import { InboxContentRoot } from "../inbox/content";
import { ReviewInboxDetail } from "./review-detail";
import { ScheduleRunInboxDetail } from "./schedule-run-detail";
import { WarRoomInboxDetail } from "./war-room-detail";

type NotificationsRootProps = {
  workspaceSlug?: string;
};

export const NotificationsRoot = observer(function NotificationsRoot({ workspaceSlug }: NotificationsRootProps) {
  // hooks
  const { currentWorkspace } = useWorkspace();
  const {
    currentSelectedNotificationId,
    setCurrentSelectedNotificationId,
    notificationLiteByNotificationId,
    notificationIdsByWorkspaceId,
    getNotifications,
  } = useWorkspaceNotifications();
  const { fetchUserProjectInfo } = useUserPermissions();
  const { isWorkItem, PeekOverviewComponent, setPeekWorkItem } = useNotificationPreview();
  // derived values
  const { workspace_slug, project_id, issue_id, is_inbox_issue } =
    notificationLiteByNotificationId(currentSelectedNotificationId);
  const { asJson: selectedNotification } = useNotification(currentSelectedNotificationId);
  const selectedScheduleRun = isScheduleRunNotification(selectedNotification?.data)
    ? selectedNotification?.data?.ai_schedule
    : undefined;
  const selectedWarRoom = isWarRoomNotification(selectedNotification?.data)
    ? selectedNotification.data.war_room
    : undefined;
  const selectedReviewRequest = isReviewRequestNotification(selectedNotification?.data)
    ? selectedNotification?.data?.review_request
    : undefined;
  const selectedReviewSession = isReviewSessionNotification(selectedNotification?.data)
    ? selectedNotification?.data?.review_session
    : undefined;

  // fetching workspace work item properties
  useWorkspaceIssueProperties(workspaceSlug);

  // fetch workspace notifications
  const notificationMutation =
    currentWorkspace && notificationIdsByWorkspaceId(currentWorkspace.id)
      ? ENotificationLoader.MUTATION_LOADER
      : ENotificationLoader.INIT_LOADER;
  const notificationLoader =
    currentWorkspace && notificationIdsByWorkspaceId(currentWorkspace.id)
      ? ENotificationQueryParamType.CURRENT
      : ENotificationQueryParamType.INIT;
  useSWR(
    currentWorkspace?.slug ? `WORKSPACE_NOTIFICATION_${currentWorkspace?.slug}` : null,
    currentWorkspace?.slug
      ? () => getNotifications(currentWorkspace?.slug, notificationMutation, notificationLoader)
      : null
  );

  // fetching user project member info
  const { isLoading: projectMemberInfoLoader } = useSWR(
    workspace_slug && project_id && is_inbox_issue
      ? `PROJECT_MEMBER_PERMISSION_INFO_${workspace_slug}_${project_id}`
      : null,
    workspace_slug && project_id && is_inbox_issue ? () => fetchUserProjectInfo(workspace_slug, project_id) : null
  );

  const embedRemoveCurrentNotification = useCallback(
    () => setCurrentSelectedNotificationId(undefined),
    [setCurrentSelectedNotificationId]
  );

  // clearing up the selected notifications when unmounting the page
  useEffect(
    () => () => {
      setPeekWorkItem(undefined);
    },
    [setCurrentSelectedNotificationId, setPeekWorkItem]
  );

  return (
    <div className={cn("h-full w-full overflow-hidden", isWorkItem && "overflow-y-auto")}>
      {!currentSelectedNotificationId ? (
        <div className="flex size-full items-center justify-center">
          <EmptyStateCompact assetKey="unknown" assetClassName="size-20" />
        </div>
      ) : (
        <>
          {selectedScheduleRun && workspace_slug ? (
            <ScheduleRunInboxDetail
              workspaceSlug={workspace_slug}
              scheduleRun={selectedScheduleRun}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
          ) : selectedWarRoom && workspace_slug ? (
            <WarRoomInboxDetail
              workspaceSlug={workspace_slug}
              warRoom={selectedWarRoom}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
          ) : (selectedReviewRequest || selectedReviewSession) && workspace_slug ? (
            <ReviewInboxDetail
              workspaceSlug={workspace_slug}
              reviewRequest={selectedReviewRequest}
              reviewSession={selectedReviewSession}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
          ) : is_inbox_issue === true && workspace_slug && project_id && issue_id ? (
            <>
              {projectMemberInfoLoader ? (
                <div className="flex h-full w-full items-center justify-center">
                  <LogoSpinner />
                </div>
              ) : (
                <InboxContentRoot
                  setIsMobileSidebar={() => {}}
                  isMobileSidebar={false}
                  workspaceSlug={workspace_slug}
                  projectId={project_id}
                  inboxIssueId={issue_id}
                  isNotificationEmbed
                  embedRemoveCurrentNotification={embedRemoveCurrentNotification}
                />
              )}
            </>
          ) : (
            <PeekOverviewComponent embedIssue embedRemoveCurrentNotification={embedRemoveCurrentNotification} />
          )}
        </>
      )}
    </div>
  );
});
