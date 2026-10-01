/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TWarRoomSocketEvent } from "@plane/types";
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

export const WarRoomRoot = observer(function WarRoomRoot({ workspaceSlug, projectId, warRoomId }: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomDetailById, detailErrorMap, fetchWarRoomDetail, applySocketEvent } = useWarRoom();
  const { allowPermissions } = useUserPermissions();
  // derived values
  const room = getWarRoomDetailById(warRoomId);
  const hasError = detailErrorMap[warRoomId];

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
    <div className="flex h-full min-h-0 w-full flex-col overflow-hidden">
      <WarRoomHeader workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
      {canWrite === false && room.status === "archived" && (
        <div className="border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 text-secondary">
          {t("war_room.archived_notice")}
        </div>
      )}
      <div className="h-[42%] min-h-[200px] shrink-0 border-b border-subtle">
        <WarRoomServiceMap workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
        <div className="min-h-[240px] flex-1 border-b border-subtle lg:min-h-0 lg:border-r lg:border-b-0">
          <WarRoomChat
            workspaceSlug={workspaceSlug}
            projectId={projectId}
            room={room}
            canWrite={canWrite}
            connectionStatus={status}
            sendTyping={sendTyping}
          />
        </div>
        <div className="min-h-[240px] flex-1 lg:min-h-0 lg:w-[420px] lg:flex-none">
          <WarRoomContextPanel workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        </div>
      </div>
    </div>
  );
});
