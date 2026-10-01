/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { WAR_ROOM_ROLE_LABEL_KEYS, WAR_ROOM_SEVERITY_CONFIG, WAR_ROOM_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, IWarRoomEvent, TWarRoomParticipantRole, TWarRoomSeverity, TWarRoomStatus } from "@plane/types";
import { calculateTimeAgo, getFileURL } from "@plane/utils";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
};

export const WarRoomActivity = observer(function WarRoomActivity({ workspaceSlug, projectId, room }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getEvents, hasMoreEvents, fetchEvents } = useWarRoom();
  const { getUserDetails } = useMember();
  // derived values
  const events = getEvents(room.id);
  const hasMore = hasMoreEvents(room.id);

  useEffect(() => {
    void fetchEvents(workspaceSlug, projectId, room.id).catch(() => {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.activity.load_failed") });
    });
    // oxlint-disable-next-line exhaustive-deps -- fetch once per room
  }, [room.id, workspaceSlug, projectId]);

  const describeEvent = (event: IWarRoomEvent): string => {
    const payload = event.payload ?? {};
    const asString = (key: string): string => (typeof payload[key] === "string" ? (payload[key] as string) : "");
    const asArray = (key: string): string[] => (Array.isArray(payload[key]) ? (payload[key] as string[]) : []);
    const statusLabel = (value: string) =>
      value in WAR_ROOM_STATUS_CONFIG ? t(WAR_ROOM_STATUS_CONFIG[value as TWarRoomStatus].label_key) : value;
    const severityLabel = (value: string) =>
      value in WAR_ROOM_SEVERITY_CONFIG ? t(WAR_ROOM_SEVERITY_CONFIG[value as TWarRoomSeverity].label_key) : value;
    const roleLabel = (value: string) =>
      value in WAR_ROOM_ROLE_LABEL_KEYS ? t(WAR_ROOM_ROLE_LABEL_KEYS[value as TWarRoomParticipantRole]) : value;
    const memberName = (memberId: string) =>
      getUserDetails(memberId)?.display_name ?? t("war_room.activity.actor_unknown");
    const serviceNames = (serviceIds: string[]) =>
      serviceIds
        .map((serviceId) => room.services.find((service) => service.id === serviceId)?.name ?? serviceId)
        .join(", ");
    const issueNames = (issueIds: string[]) =>
      issueIds.map((issueId) => room.issues.find((issue) => issue.id === issueId)?.identifier ?? issueId).join(", ");

    switch (event.event_type) {
      case "room.created":
        return t("war_room.activity.events.room_created");
      case "room.status_changed":
        return t("war_room.activity.events.status_changed", {
          from: statusLabel(asString("from")),
          to: statusLabel(asString("to")),
        });
      case "room.severity_changed":
        return t("war_room.activity.events.severity_changed", {
          from: severityLabel(asString("from")),
          to: severityLabel(asString("to")),
        });
      case "room.resolved":
        return t("war_room.activity.events.resolved");
      case "room.reopened":
        return t("war_room.activity.events.reopened");
      case "room.archived":
        return t("war_room.activity.events.archived");
      case "participant.joined":
        return t("war_room.activity.events.participant_joined");
      case "participant.left":
        return t("war_room.activity.events.participant_left");
      case "participant.role_changed":
        return t("war_room.activity.events.participant_role_changed", {
          target: memberName(asString("member_id")),
          from: roleLabel(asString("from")),
          to: roleLabel(asString("to")),
        });
      case "service.linked":
        return t("war_room.activity.events.service_linked", { services: serviceNames(asArray("service_ids")) });
      case "service.unlinked":
        return t("war_room.activity.events.service_unlinked", { service: serviceNames([asString("service_id")]) });
      case "issue.linked":
        return t("war_room.activity.events.issue_linked", { issues: issueNames(asArray("issue_ids")) });
      case "issue.unlinked":
        return t("war_room.activity.events.issue_unlinked", { issue: issueNames([asString("issue_id")]) });
      case "runbook.item_done":
        return t("war_room.activity.events.runbook_item_done", { title: asString("title") });
      case "runbook.item_reopened":
        return t("war_room.activity.events.runbook_item_reopened", { title: asString("title") });
      default:
        return t("war_room.activity.events.unknown");
    }
  };

  const handleLoadOlder = async () => {
    const oldest = events[events.length - 1];
    if (!oldest) return;
    try {
      await fetchEvents(workspaceSlug, projectId, room.id, { before_id: oldest.id });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.activity.load_failed") });
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.activity")}
        </span>
        {hasMore && (
          <Button variant="tertiary" size="sm" onClick={() => void handleLoadOlder()}>
            {t("war_room.activity.load_older")}
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {events.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.activity.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.activity.empty_description")}</p>
          </div>
        )}
        <div className="space-y-3">
          {events.map((event) => {
            const actor = event.actor_id ? getUserDetails(event.actor_id) : null;
            const actorName = actor?.display_name ?? t("war_room.activity.actor_unknown");
            return (
              <div key={event.id} className="flex items-start gap-2">
                <Avatar
                  size="xs"
                  src={actor?.avatar_url ? getFileURL(actor.avatar_url) : undefined}
                  alt={actorName}
                  fallback={actorName[0]?.toUpperCase()}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-12 text-primary">
                    <span className="font-medium">{actorName}</span>{" "}
                    <span className="text-secondary">{describeEvent(event)}</span>
                  </p>
                  <p className="text-10 text-tertiary">{calculateTimeAgo(event.created_at)}</p>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
});
