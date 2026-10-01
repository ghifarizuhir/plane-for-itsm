/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IWarRoom, TWarRoomConnectionStatus, TWarRoomSocketEvent } from "@plane/types";
import { cn } from "@plane/utils";
// components
import { WarRoomChat } from "./chat/root";
import { WarRoomContextPanel } from "./context/root";
import { WarRoomHeader } from "./header/root";
import { WarRoomServiceMap } from "./service-map";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUserPermissions } from "@/hooks/store/user";
import { useWarRoomSocket } from "@/hooks/use-war-room-socket";

type Props = {
  workspaceSlug: string;
  projectId: string;
  warRoomId: string;
};

/** Bottom telemetry strip: connection, presence and room counters in one glance. */
function WarRoomStatusBar({
  connectionStatus,
  room,
  onlineCount,
}: {
  connectionStatus: TWarRoomConnectionStatus;
  room: IWarRoom;
  onlineCount: number;
}) {
  const { t } = useTranslation();
  const isLive = connectionStatus === "connected";
  const isSyncing = connectionStatus === "reconnecting" || connectionStatus === "connecting";

  return (
    <div className="flex h-7 shrink-0 items-center gap-3 border-t border-subtle bg-layer-1 px-3 font-code text-10 tracking-[0.12em] text-tertiary uppercase">
      <span className="flex items-center gap-1.5">
        <span
          className={cn("size-1.5 rounded-full", {
            "bg-success-primary": isLive,
            "animate-pulse bg-warning-primary": isSyncing,
            "bg-layer-3": !isLive && !isSyncing,
          })}
        />
        {t("war_room.header.online", { count: onlineCount })}
      </span>
      <span aria-hidden className="bg-subtle h-3 w-px" />
      <span>
        {room.counts.messages} {t("war_room.fields.messages")}
      </span>
      <span aria-hidden className="bg-subtle h-3 w-px" />
      <span>
        {room.participants.length} {t("war_room.fields.participants")}
      </span>
      <span className="ml-auto hidden sm:block">
        {t("war_room.fields.started")}{" "}
        {new Date(room.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
      </span>
    </div>
  );
}

export const WarRoomRoot = observer(function WarRoomRoot({ workspaceSlug, projectId, warRoomId }: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomDetailById, detailErrorMap, fetchWarRoomDetail, applySocketEvent, getOnlineUserIds } = useWarRoom();
  const { allowPermissions } = useUserPermissions();
  // states
  const [isMapOpen, setIsMapOpen] = useState(false);
  // derived values
  const room = getWarRoomDetailById(warRoomId);
  const hasError = detailErrorMap[warRoomId];
  const onlineCount = room ? getOnlineUserIds(room.id).length : 0;

  useEffect(() => {
    if (room || hasError) return;
    void fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  }, [room, hasError, workspaceSlug, projectId, warRoomId, fetchWarRoomDetail]);

  const handleSocketEvent = useCallback(
    (event: TWarRoomSocketEvent) => {
      applySocketEvent(workspaceSlug, projectId, warRoomId, event);
    },
    [applySocketEvent, projectId, warRoomId, workspaceSlug]
  );

  const { status, sendTyping } = useWarRoomSocket({
    workspaceSlug,
    projectId,
    warRoomId,
    enabled: Boolean(room),
    onEvent: handleSocketEvent,
  });

  const canWrite =
    allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT) &&
    room?.status !== "archived";

  if (hasError) {
    return (
      <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
        <p className="text-sm font-medium text-primary">{t("war_room.detail.not_found_title")}</p>
        <p className="text-xs text-secondary">{t("war_room.detail.not_found_description")}</p>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => router.push(`/${workspaceSlug}/projects/${projectId}/war-rooms`)}
        >
          {t("war_room.detail.view_other_rooms")}
        </Button>
      </div>
    );
  }

  if (!room) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-surface-1">
      <WarRoomHeader
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        room={room}
        canWrite={canWrite}
        isMapOpen={isMapOpen}
        onToggleMap={() => setIsMapOpen((value) => !value)}
      />
      {canWrite === false && room.status === "archived" && (
        <div className="border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 text-secondary">
          {t("war_room.archived_notice")}
        </div>
      )}

      <div className="relative flex min-h-0 flex-1">
        {isMapOpen && (
          <button
            type="button"
            aria-label={t("common.close")}
            onClick={() => setIsMapOpen(false)}
            className="fixed inset-0 z-30 cursor-default bg-backdrop xl:hidden"
          />
        )}

        {/* Blast radius: static column from xl, slide-over drawer below it. */}
        <aside
          className={cn(
            "z-40 flex min-h-0 flex-col border-subtle bg-surface-1",
            "max-xl:shadow-2xl max-xl:fixed max-xl:inset-y-0 max-xl:left-0 max-xl:w-[min(88vw,360px)] max-xl:border-r max-xl:transition-transform max-xl:duration-200",
            isMapOpen ? "max-xl:translate-x-0" : "max-xl:-translate-x-full",
            "xl:relative xl:z-auto xl:w-[340px] xl:shrink-0 xl:translate-x-0 xl:border-r"
          )}
        >
          <WarRoomServiceMap workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        </aside>

        <div className="flex min-h-0 min-w-0 flex-1 flex-col lg:flex-row">
          <div className="min-h-0 min-w-0 flex-1">
            <WarRoomChat
              workspaceSlug={workspaceSlug}
              projectId={projectId}
              room={room}
              canWrite={canWrite}
              connectionStatus={status}
              sendTyping={sendTyping}
            />
          </div>
          <div className="h-[42%] min-h-[280px] shrink-0 border-t border-subtle lg:h-auto lg:min-h-0 lg:w-[400px] lg:border-t-0 lg:border-l">
            <WarRoomContextPanel workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
          </div>
        </div>
      </div>

      <WarRoomStatusBar connectionStatus={status} room={room} onlineCount={onlineCount} />
    </div>
  );
});
