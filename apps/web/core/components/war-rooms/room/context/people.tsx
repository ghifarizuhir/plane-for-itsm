/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { WAR_ROOM_PARTICIPANT_ROLES, WAR_ROOM_ROLE_LABEL_KEYS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, IWarRoomParticipant, TWarRoomParticipantRole } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { getFileURL } from "@plane/utils";
// components
import { MemberDropdown } from "@/components/dropdowns/member/dropdown";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUser } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomPeople = observer(function WarRoomPeople({ workspaceSlug, projectId, room, canWrite }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { addParticipant, updateParticipantRole, removeParticipant, getOnlineUserIds } = useWarRoom();
  const {
    getUserDetails,
    project: { getProjectMemberIds, fetchProjectMembers },
  } = useMember();
  const { data: currentUser } = useUser();
  // derived values
  const currentUserId = currentUser?.id;
  const onlineUserIds = getOnlineUserIds(room.id);
  const participantMemberIds = new Set(room.participants.map((participant) => participant.member_id));
  const selfParticipant = room.participants.find((participant) => participant.member_id === currentUserId);
  const availableMemberIds = (getProjectMemberIds(projectId, true) ?? []).filter(
    (memberId) => !participantMemberIds.has(memberId)
  );

  useEffect(() => {
    if (getProjectMemberIds(projectId, true) === null) void fetchProjectMembers(workspaceSlug, projectId);
    // oxlint-disable-next-line exhaustive-deps -- hydrate once per project
  }, [projectId, workspaceSlug]);

  const handleAdd = async (memberId: string) => {
    try {
      await addParticipant(workspaceSlug, projectId, room.id, { member_id: memberId });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.update_failed") });
    }
  };

  const handleRoleChange = async (participant: IWarRoomParticipant, role: TWarRoomParticipantRole) => {
    if (participant.role === role) return;
    try {
      await updateParticipantRole(workspaceSlug, projectId, room.id, participant.id, { role });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.update_failed") });
    }
  };

  const handleJoin = async () => {
    if (!currentUserId) return;
    try {
      await addParticipant(workspaceSlug, projectId, room.id, { member_id: currentUserId });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.join_failed") });
    }
  };

  const handleLeave = async () => {
    if (!selfParticipant) return;
    try {
      await removeParticipant(workspaceSlug, projectId, room.id, selfParticipant.id);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.leave_failed") });
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.people")}
        </span>
        {canWrite && (
          <div className="flex items-center gap-2">
            {availableMemberIds.length > 0 && (
              <MemberDropdown
                buttonVariant="border-with-text"
                multiple={false}
                value={null}
                memberIds={availableMemberIds}
                projectId={projectId}
                placeholder={t("war_room.people.add")}
                onChange={(memberId) => {
                  if (memberId) void handleAdd(memberId);
                }}
              />
            )}
            {!selfParticipant ? (
              <Button variant="secondary" size="sm" onClick={() => void handleJoin()}>
                {t("war_room.people.join")}
              </Button>
            ) : (
              <Button variant="secondary" size="sm" onClick={() => void handleLeave()}>
                {t("war_room.people.leave")}
              </Button>
            )}
          </div>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {room.participants.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.people.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.people.empty_description")}</p>
          </div>
        )}
        <div className="space-y-1">
          {room.participants.map((participant) => {
            const member = getUserDetails(participant.member_id);
            const displayName =
              participant.display_name ?? member?.display_name ?? t("war_room.activity.actor_unknown");
            const isOnline = onlineUserIds.includes(participant.member_id);
            return (
              <div
                key={participant.id}
                className="flex items-center gap-2 rounded-md px-1 py-1.5 hover:bg-layer-transparent-hover"
              >
                <div className="relative shrink-0">
                  <Avatar
                    size="sm"
                    src={participant.avatar_url ? getFileURL(participant.avatar_url) : undefined}
                    alt={displayName}
                    fallback={displayName[0]?.toUpperCase()}
                  />
                  {isOnline && (
                    <span className="border-surface-1 absolute -right-0.5 -bottom-0.5 h-2 w-2 rounded-full border bg-success-primary" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-13 text-primary">
                    {displayName}
                    {participant.member_id === currentUserId && (
                      <span className="ml-1 text-10 text-tertiary">({t("war_room.header.you")})</span>
                    )}
                  </p>
                </div>
                {canWrite ? (
                  <CustomSelect
                    value={participant.role}
                    label={
                      <span className="text-11 text-secondary">{t(WAR_ROOM_ROLE_LABEL_KEYS[participant.role])}</span>
                    }
                    onChange={(role: TWarRoomParticipantRole) => void handleRoleChange(participant, role)}
                    noChevron
                  >
                    {WAR_ROOM_PARTICIPANT_ROLES.map((role) => (
                      <CustomSelect.Option key={role} value={role}>
                        {t(WAR_ROOM_ROLE_LABEL_KEYS[role])}
                      </CustomSelect.Option>
                    ))}
                  </CustomSelect>
                ) : (
                  <span className="text-11 text-secondary">{t(WAR_ROOM_ROLE_LABEL_KEYS[participant.role])}</span>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
});
