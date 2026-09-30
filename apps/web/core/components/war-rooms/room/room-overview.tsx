/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// plane imports
import { WAR_ROOM_STATUS_CONFIG, WAR_ROOM_SEVERITY_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { IWarRoom } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { formatElapsed, getWarRoomIncidentLink } from "@/services/war-room.helpers";

type Props = {
  room: IWarRoom;
};

export const WarRoomOverview = observer(function WarRoomOverview({ room }: Props) {
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const severityConfig = WAR_ROOM_SEVERITY_CONFIG[room.severity];
  const statusConfig = WAR_ROOM_STATUS_CONFIG[room.status];
  const elapsed = formatElapsed(room.started_at, room.resolved_at);

  return (
    <div className="flex h-full w-full flex-col gap-4 overflow-y-auto p-4 sm:p-6">
      <div className="rounded-lg border border-subtle bg-surface-1 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", severityConfig.pill)}>
            {t(severityConfig.label_key)}
          </span>
          <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", statusConfig.pill)}>
            {t(statusConfig.label_key)}
          </span>
          <span className="text-12 font-medium text-tertiary">WR-{room.sequence_id}</span>
          <span className="text-12 text-tertiary tabular-nums">{elapsed}</span>
        </div>
        <h2 className="mt-2 text-18 font-medium text-primary">{room.name}</h2>
        {room.primary_issue && workspaceSlug && (
          <Link
            href={getWarRoomIncidentLink(workspaceSlug.toString(), room.primary_issue.identifier)}
            className="mt-2 inline-flex items-center gap-2 rounded-xs border border-subtle px-2 py-1 text-12 text-secondary hover:text-primary"
          >
            <span className="font-medium">{room.primary_issue.identifier}</span>
            <span className="truncate">{room.primary_issue.name}</span>
          </Link>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-4 text-12 text-secondary">
          <span>
            {t("war_room.fields.affected_services")}: {room.services.length}
          </span>
          <span>
            {t("war_room.fields.participants")}: {room.participants.length}
          </span>
          <span>
            {t("war_room.fields.messages")}: {room.counts.messages}
          </span>
        </div>
      </div>
    </div>
  );
});
