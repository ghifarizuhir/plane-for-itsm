/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { AvatarGroup } from "@makeplane/propel/components/avatar-group";
import { ArrowExpandOutline } from "@makeplane/propel/icons";
import { WAR_ROOM_SEVERITY_CONFIG, WAR_ROOM_STATUS_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { cn, getFileURL } from "@plane/utils";
// helpers
import { formatElapsed, getWarRoomIncidentLink, isActiveWarRoomStatus } from "@/services/war-room.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";

const openWarRoomInNewTab = (href: string) => {
  window.open(href, "_blank", "noopener,noreferrer");
};

type Props = {
  warRoomId: string;
};

export const WarRoomsBoardRow = observer(function WarRoomsBoardRow({ warRoomId }: Props) {
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomById } = useWarRoom();
  // states
  const [now, setNow] = useState(() => Date.now());
  // derived values
  const room = getWarRoomById(warRoomId);
  const isRunning = room ? isActiveWarRoomStatus(room.status) : false;

  useEffect(() => {
    if (!isRunning) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [isRunning]);

  if (!room) return null;

  const severityConfig = WAR_ROOM_SEVERITY_CONFIG[room.severity];
  const statusConfig = WAR_ROOM_STATUS_CONFIG[room.status];
  const warRoomLink = getWarRoomLink(workspaceSlug?.toString() ?? "", room.project_id, room.id);
  const elapsed = formatElapsed(room.started_at, room.resolved_at, now);
  const visibleParticipants = room.participants.slice(0, 3);
  const overflowParticipants = room.participants.length - visibleParticipants.length;
  const visibleServices = room.services.slice(0, 3);
  const overflowServices = room.services.length - visibleServices.length;

  return (
    <div
      // Row is a div (not a link) because it nests the incident work-item link.
      // oxlint-disable-next-line eslint-plugin-jsx-a11y/prefer-tag-over-role
      role="link"
      tabIndex={0}
      onClick={() => openWarRoomInNewTab(warRoomLink)}
      onKeyDown={(e) => {
        if (e.key === "Enter") openWarRoomInNewTab(warRoomLink);
      }}
      className="group/row relative flex cursor-pointer items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover"
    >
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", severityConfig.rail)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className="shrink-0 text-11 font-medium text-tertiary">WR-{room.sequence_id}</span>
        <span className="truncate text-13 font-medium text-primary">{room.name}</span>
        {room.primary_issue && workspaceSlug && (
          <Link
            href={getWarRoomIncidentLink(workspaceSlug.toString(), room.primary_issue.identifier)}
            target="_blank"
            rel="noopener noreferrer"
            onClick={(e) => e.stopPropagation()}
            className="hidden shrink-0 rounded-xs border border-subtle px-1.5 py-0.5 text-10 text-secondary hover:text-primary sm:block"
          >
            {room.primary_issue.identifier}
          </Link>
        )}
      </div>
      <span
        className={cn("hidden shrink-0 rounded-full px-2 py-0.5 text-10 font-medium sm:block", severityConfig.pill)}
      >
        {t(severityConfig.label_key)}
      </span>
      <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-10 font-medium", statusConfig.pill)}>
        {t(statusConfig.label_key)}
      </span>
      <div className="hidden items-center gap-1 lg:flex">
        {visibleServices.map((service) => (
          <span
            key={service.id}
            className="max-w-[120px] truncate rounded-xs border border-subtle px-1.5 py-0.5 text-10 text-secondary"
          >
            {service.name}
          </span>
        ))}
        {overflowServices > 0 && <span className="text-10 text-tertiary">+{overflowServices}</span>}
      </div>
      <div className="hidden w-[72px] shrink-0 items-center justify-end md:flex">
        {room.participants.length > 0 && (
          <AvatarGroup size="xs">
            {visibleParticipants.map((participant) => (
              <Avatar
                key={participant.id}
                src={participant.avatar_url ? getFileURL(participant.avatar_url) : undefined}
                alt={participant.display_name ?? ""}
                fallback={participant.display_name?.[0]?.toUpperCase()}
              />
            ))}
            {overflowParticipants > 0 && (
              <Avatar alt={`${overflowParticipants} more`} fallback={`+${overflowParticipants}`} />
            )}
          </AvatarGroup>
        )}
      </div>
      <span className="hidden w-[72px] shrink-0 text-right text-11 text-secondary tabular-nums xl:block">
        {elapsed}
      </span>
      <span className="hidden w-[48px] shrink-0 text-right text-11 text-tertiary xl:block">{room.message_count}</span>
      <span className="hidden shrink-0 text-tertiary opacity-0 transition-opacity group-hover/row:opacity-100 md:block">
        <ArrowExpandOutline className="size-3.5" />
      </span>
    </div>
  );
});
