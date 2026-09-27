/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { CalendarOutline, ClockOutline } from "@makeplane/propel/icons";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { Badge } from "@plane/propel/badge";
import { Row } from "@plane/ui";
import { cn, calculateTimeAgo, renderFormattedDate, renderFormattedTime, getFileURL } from "@plane/utils";
// hooks
import { useWorkspaceNotifications } from "@/hooks/store/notifications";
import { useNotification } from "@/hooks/store/notifications/use-notification";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { isScheduleRunNotification, scheduleRunNotificationText, scheduleStatusLabel } from "@/lib/ai-schedule";
// local imports
import { NotificationContent } from "./content";
import { NotificationOption } from "./options";

type TNotificationItem = {
  workspaceSlug: string;
  notificationId: string;
};

export const NotificationItem = observer(function NotificationItem(props: TNotificationItem) {
  const { workspaceSlug, notificationId } = props;
  // hooks
  const { currentSelectedNotificationId, setCurrentSelectedNotificationId } = useWorkspaceNotifications();
  const { asJson: notification, markNotificationAsRead } = useNotification(notificationId);
  const { getIsIssuePeeked, setPeekIssue } = useIssueDetail();
  const { getWorkspaceBySlug } = useWorkspace();
  // states
  const [isSnoozeStateModalOpen, setIsSnoozeStateModalOpen] = useState(false);
  const [customSnoozeModal, setCustomSnoozeModal] = useState(false);

  // derived values
  const projectId = notification?.project || undefined;
  const issueId = notification?.data?.issue?.id || undefined;
  const workspace = getWorkspaceBySlug(workspaceSlug);
  const scheduleRun = isScheduleRunNotification(notification?.data) ? notification?.data?.ai_schedule : undefined;

  const notificationField = notification?.data?.issue_activity?.field || undefined;
  const notificationTriggeredBy = notification.triggered_by_details || undefined;

  const handleNotificationClick = async () => {
    if (!workspaceSlug || !notification?.id || isSnoozeStateModalOpen || customSnoozeModal) return;

    setCurrentSelectedNotificationId(notificationId);

    // make the notification as read
    if (notification.read_at === null) {
      try {
        await markNotificationAsRead(workspaceSlug);
      } catch (error) {
        console.error(error);
      }
    }

    // schedule runs render their output in the right pane; nothing else to do
    if (scheduleRun) return;

    setPeekIssue(undefined);
    if (projectId && issueId && notification?.is_inbox_issue === false && !getIsIssuePeeked(issueId)) {
      setPeekIssue({ workspaceSlug, projectId, issueId });
    }
  };

  if (!workspaceSlug || !notificationId || !notification?.id || !workspace?.id) return <></>;
  if (!scheduleRun && (!notificationField || !projectId)) return <></>;

  return (
    <Row
      className={cn(
        "group relative flex cursor-pointer items-center gap-2 border-b border-subtle py-4 transition-all",
        {
          "bg-layer-1/30": currentSelectedNotificationId === notification?.id,
          "bg-accent-primary/5": notification.read_at === null,
        }
      )}
      onClick={handleNotificationClick}
    >
      {notification.read_at === null && (
        <div className="absolute top-[50%] left-2 h-1.5 w-1.5 flex-shrink-0 rounded-full bg-accent-primary" />
      )}

      <div className="relative flex w-full gap-2">
        <div className="relative flex h-12 w-12 flex-shrink-0 items-center justify-center rounded-full bg-layer-1">
          {notificationTriggeredBy ? (
            <Avatar
              alt={notificationTriggeredBy.display_name || notificationTriggeredBy?.first_name}
              fallback={(notificationTriggeredBy.display_name ||
                notificationTriggeredBy?.first_name)?.[0]?.toUpperCase()}
              src={getFileURL(notificationTriggeredBy.avatar_url)}
              size="xl"
            />
          ) : scheduleRun ? (
            <CalendarOutline className="h-5 w-5 text-tertiary" />
          ) : null}
        </div>

        <div className="-mt-2 w-full space-y-1">
          <div className="relative flex h-8 items-center gap-3">
            <div className="line-clamp-1 w-full truncate overflow-hidden text-body-xs-medium break-all whitespace-normal text-primary">
              {scheduleRun ? (
                <span className="font-medium text-primary">{scheduleRun.name}</span>
              ) : (
                <NotificationContent
                  notification={notification}
                  workspaceId={workspace.id}
                  workspaceSlug={workspaceSlug}
                  // reached only when projectId is defined (see the guard above)
                  projectId={projectId ?? ""}
                />
              )}
            </div>
            <NotificationOption
              workspaceSlug={workspaceSlug}
              notificationId={notification?.id}
              isSnoozeStateModalOpen={isSnoozeStateModalOpen}
              setIsSnoozeStateModalOpen={setIsSnoozeStateModalOpen}
              customSnoozeModal={customSnoozeModal}
              setCustomSnoozeModal={setCustomSnoozeModal}
            />
          </div>

          <div className="relative flex items-center gap-3 text-caption-sm-regular text-secondary">
            <div className="line-clamp-1 w-full truncate overflow-hidden break-words whitespace-normal">
              {scheduleRun ? (
                <span className="flex items-center gap-2">
                  <Badge variant={scheduleRun.status === "success" ? "success" : "danger"} size="sm">
                    {scheduleStatusLabel(scheduleRun.status)}
                  </Badge>
                  <span>{scheduleRunNotificationText(scheduleRun.status)}</span>
                  {scheduleRun.status === "failed" && scheduleRun.error && (
                    <span className="truncate text-danger-primary">{scheduleRun.error}</span>
                  )}
                </span>
              ) : (
                <>
                  {notification?.data?.issue?.identifier}-{notification?.data?.issue?.sequence_id}&nbsp;
                  {notification?.data?.issue?.name}
                </>
              )}
            </div>
            <div className="flex-shrink-0">
              {notification?.snoozed_till ? (
                <p className="flex flex-shrink-0 items-center justify-end gap-x-1 text-tertiary">
                  <ClockOutline className="h-4 w-4" />
                  <span>
                    Till {renderFormattedDate(notification.snoozed_till)},&nbsp;
                    {renderFormattedTime(notification.snoozed_till, "12-hour")}
                  </span>
                </p>
              ) : (
                <p className="mt-auto flex-shrink-0 text-tertiary">
                  {notification.created_at && calculateTimeAgo(notification.created_at)}
                </p>
              )}
            </div>
          </div>
        </div>
      </div>
    </Row>
  );
});
