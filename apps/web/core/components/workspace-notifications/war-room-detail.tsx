/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRouter } from "next/navigation";
// plane imports
import { getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TNotificationWarRoom } from "@plane/types";

type Props = {
  workspaceSlug: string;
  warRoom: TNotificationWarRoom;
  embedRemoveCurrentNotification: () => void;
};

export function WarRoomInboxDetail({ workspaceSlug, warRoom, embedRemoveCurrentNotification }: Props) {
  // router
  const router = useRouter();
  const { t } = useTranslation();

  const openWarRoom = () => {
    router.push(getWarRoomLink(workspaceSlug, warRoom.project_id, warRoom.id));
  };

  return (
    <div className="h-full w-full overflow-y-auto p-4">
      <div className="mx-auto max-w-2xl space-y-3">
        <h3 className="text-base font-semibold break-words text-primary">{warRoom.name}</h3>
        <p className="text-xs text-tertiary">WR-{warRoom.sequence_id}</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="secondary" onClick={openWarRoom}>
            {t("war_room.open")}
          </Button>
          <Button size="sm" variant="ghost" onClick={embedRemoveCurrentNotification}>
            {t("war_room.notification.dismiss")}
          </Button>
        </div>
      </div>
    </div>
  );
}
