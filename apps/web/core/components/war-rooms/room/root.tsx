/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// components
import { WarRoomOverview } from "./room-overview";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useWarRoom } from "@/hooks/store/use-war-room";

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
  const { getWarRoomDetailById, detailErrorMap, fetchWarRoomDetail } = useWarRoom();
  // derived values
  const room = getWarRoomDetailById(warRoomId);
  const hasError = detailErrorMap[warRoomId];

  useEffect(() => {
    if (room || hasError) return;
    void fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  }, [room, hasError, workspaceSlug, projectId, warRoomId, fetchWarRoomDetail]);

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

  return <WarRoomOverview room={room} />;
});
