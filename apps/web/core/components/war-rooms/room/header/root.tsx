/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { AvatarGroup } from "@makeplane/propel/components/avatar-group";
import { GridOutline } from "@makeplane/propel/icons";
import {
  EUserPermissions,
  EUserPermissionsLevel,
  WAR_ROOM_SEVERITIES,
  WAR_ROOM_SEVERITY_CONFIG,
  WAR_ROOM_STATUS_CONFIG,
  getWarRoomLink,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { IconButton } from "@plane/propel/icon-button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, TWarRoomSeverity, TWarRoomStatus } from "@plane/types";
import { CustomMenu, CustomSelect } from "@plane/ui";
import { cn, getFileURL } from "@plane/utils";
// components
import { WarRoomConfirmModal } from "./confirm-modal";
import { WarRoomEditDetailsModal } from "./edit-details-modal";
import { WarRoomResolveModal } from "./resolve-modal";
import { WarRoomStatusControl } from "./status-control";
// helpers
import { appendNoteToHtml, formatElapsed } from "@/services/war-room.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
  isMapOpen: boolean;
  onToggleMap: () => void;
};

type TDialog = "resolve" | "archive" | "delete" | "edit" | null;

export const WarRoomHeader = observer(function WarRoomHeader({
  workspaceSlug,
  projectId,
  room,
  canWrite,
  isMapOpen,
  onToggleMap,
}: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateWarRoom, deleteWarRoom, addParticipant, removeParticipant, getOnlineUserIds } = useWarRoom();
  const { data: currentUser } = useUser();
  const { allowPermissions } = useUserPermissions();
  // states
  const [now, setNow] = useState(() => Date.now());
  const [isEditingName, setIsEditingName] = useState(false);
  const [nameValue, setNameValue] = useState(room.name);
  const [dialog, setDialog] = useState<TDialog>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  // derived values
  const currentUserId = currentUser?.id;
  const isRunning = room.status === "active" || room.status === "monitoring";
  const elapsed = formatElapsed(room.started_at, room.resolved_at, now);
  const onlineUserIds = getOnlineUserIds(room.id);
  const onlineParticipants = room.participants.filter((participant) => onlineUserIds.includes(participant.member_id));
  const visibleOnline = onlineParticipants.slice(0, 4);
  const overflowOnline = onlineParticipants.length - visibleOnline.length;
  const selfParticipant = room.participants.find((participant) => participant.member_id === currentUserId);
  const canManage =
    canWrite && allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT);

  useEffect(() => {
    if (!isRunning) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [isRunning]);

  useEffect(() => {
    setNameValue(room.name);
  }, [room.name]);

  const showError = (message: string) => setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message });

  const handleNameSave = async () => {
    const name = nameValue.trim();
    setIsEditingName(false);
    if (name === "" || name === room.name) {
      setNameValue(room.name);
      return;
    }
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { name });
    } catch {
      setNameValue(room.name);
      showError(t("war_room.errors.generic"));
    }
  };

  const handleStatusChange = async (status: TWarRoomStatus) => {
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { status });
    } catch {
      showError(t("war_room.errors.generic"));
    }
  };

  const handleSeverityChange = async (severity: TWarRoomSeverity) => {
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { severity });
    } catch {
      showError(t("war_room.errors.generic"));
    }
  };

  const handleResolve = async (note: string) => {
    setIsSubmitting(true);
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, {
        status: "resolved",
        notes_html: appendNoteToHtml(room.notes_html, note),
      });
      setDialog(null);
    } catch {
      showError(t("war_room.errors.generic"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleArchive = async () => {
    setIsSubmitting(true);
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { status: "archived" });
      setDialog(null);
    } catch {
      showError(t("war_room.errors.generic"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDelete = async () => {
    setIsSubmitting(true);
    try {
      await deleteWarRoom(workspaceSlug, projectId, room.id);
      setDialog(null);
      router.push(getWarRoomLink(workspaceSlug, projectId));
    } catch {
      showError(t("war_room.errors.generic"));
      setIsSubmitting(false);
    }
  };

  const handleEditDetails = async (data: { name: string; description_html: string }) => {
    setIsSubmitting(true);
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, data);
      setDialog(null);
    } catch {
      showError(t("war_room.errors.generic"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleJoin = async () => {
    if (!currentUserId) return;
    try {
      await addParticipant(workspaceSlug, projectId, room.id, { member_id: currentUserId });
    } catch {
      showError(t("war_room.people.join_failed"));
    }
  };

  const handleLeave = async () => {
    if (!selfParticipant) return;
    try {
      await removeParticipant(workspaceSlug, projectId, room.id, selfParticipant.id);
    } catch {
      showError(t("war_room.people.leave_failed"));
    }
  };

  return (
    <header className="relative shrink-0 border-b border-subtle bg-surface-1">
      {/* Severity rail: the room's threat level, always visible at the very top. */}
      <span
        aria-hidden="true"
        className={cn("absolute inset-x-0 top-0 h-[2px]", WAR_ROOM_SEVERITY_CONFIG[room.severity].rail, {
          "animate-pulse": isRunning,
        })}
      />
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 pt-3 pb-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
          <IconButton
            variant="ghost"
            size="sm"
            className={cn("shrink-0 xl:hidden", { "bg-layer-1 text-primary": isMapOpen })}
            icon={GridOutline}
            aria-label={t("war_room.map.title")}
            onClick={onToggleMap}
          />
          <span className="shrink-0 rounded-xs border border-subtle px-1.5 py-0.5 font-code text-10 font-medium tracking-[0.14em] text-secondary">
            WR-{room.sequence_id}
          </span>
          {isEditingName ? (
            <input
              value={nameValue}
              onChange={(event) => setNameValue(event.target.value)}
              onBlur={() => void handleNameSave()}
              onKeyDown={(event) => {
                if (event.key === "Enter") void handleNameSave();
                if (event.key === "Escape") {
                  setNameValue(room.name);
                  setIsEditingName(false);
                }
              }}
              // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus -- inline name edit should take focus
              autoFocus
              maxLength={255}
              className="max-w-72 min-w-40 flex-1 rounded-sm border border-subtle bg-surface-1 px-1.5 py-0.5 text-16 font-semibold text-primary outline-none focus:border-strong"
            />
          ) : (
            <button
              type="button"
              disabled={!canManage}
              onClick={() => setIsEditingName(true)}
              className="max-w-72 min-w-0 truncate text-left text-16 font-semibold text-primary hover:underline disabled:hover:no-underline"
              title={room.name}
            >
              {room.name}
            </button>
          )}
          {canManage ? (
            <CustomSelect
              value={room.severity}
              label={
                <span
                  className={`rounded-full px-2 py-0.5 text-11 font-medium ${WAR_ROOM_SEVERITY_CONFIG[room.severity].pill}`}
                >
                  {t(WAR_ROOM_SEVERITY_CONFIG[room.severity].label_key)}
                </span>
              }
              onChange={(severity: TWarRoomSeverity) => void handleSeverityChange(severity)}
              noChevron
            >
              {WAR_ROOM_SEVERITIES.map((severity) => (
                <CustomSelect.Option key={severity} value={severity}>
                  {t(WAR_ROOM_SEVERITY_CONFIG[severity].label_key)}
                </CustomSelect.Option>
              ))}
            </CustomSelect>
          ) : (
            <span
              className={`shrink-0 rounded-full px-2 py-0.5 text-11 font-medium ${WAR_ROOM_SEVERITY_CONFIG[room.severity].pill}`}
            >
              {t(WAR_ROOM_SEVERITY_CONFIG[room.severity].label_key)}
            </span>
          )}
          <WarRoomStatusControl
            status={room.status}
            disabled={!canManage}
            onChange={(status) => void handleStatusChange(status)}
          />
        </div>

        {/* Ops timer: the room's heartbeat. */}
        <div className="order-last flex w-full items-center justify-center gap-2 md:order-none md:w-auto">
          <span
            aria-hidden="true"
            className={cn("size-1.5 rounded-full", WAR_ROOM_STATUS_CONFIG[room.status].rail, {
              "animate-pulse": isRunning,
            })}
          />
          <span className="font-code text-20 font-medium text-primary tabular-nums">{elapsed}</span>
          <span className="font-code text-9 tracking-[0.18em] text-tertiary uppercase">
            {t("war_room.fields.elapsed")}
          </span>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <div className="flex items-center gap-1">
            {onlineParticipants.length > 0 && (
              <AvatarGroup size="xs">
                {visibleOnline.map((participant) => (
                  <Avatar
                    key={participant.id}
                    src={participant.avatar_url ? getFileURL(participant.avatar_url) : undefined}
                    alt={participant.display_name ?? ""}
                    fallback={participant.display_name?.[0]?.toUpperCase()}
                  />
                ))}
                {overflowOnline > 0 && <Avatar alt={`+${overflowOnline}`} fallback={`+${overflowOnline}`} />}
              </AvatarGroup>
            )}
            <span className="hidden text-10 text-tertiary sm:block">
              {t("war_room.header.online", { count: onlineParticipants.length })}
            </span>
          </div>
          {canManage && !selfParticipant && (
            <Button variant="secondary" size="sm" onClick={() => void handleJoin()}>
              {t("war_room.header.join")}
            </Button>
          )}
          {canManage && selfParticipant && (
            <Button variant="tertiary" size="sm" onClick={() => void handleLeave()}>
              {t("war_room.header.leave")}
            </Button>
          )}
          {canManage && isRunning && (
            <Button variant="primary" size="sm" onClick={() => setDialog("resolve")}>
              {t("war_room.header.resolve")}
            </Button>
          )}
          {canManage && (
            <CustomMenu ellipsis placement="bottom-end">
              <CustomMenu.MenuItem onClick={() => setDialog("edit")}>
                {t("war_room.header.edit_details")}
              </CustomMenu.MenuItem>
              <CustomMenu.MenuItem onClick={() => setDialog("archive")}>
                {t("war_room.header.archive")}
              </CustomMenu.MenuItem>
              <CustomMenu.MenuItem onClick={() => setDialog("delete")}>
                {t("war_room.header.delete")}
              </CustomMenu.MenuItem>
            </CustomMenu>
          )}
        </div>
      </div>

      <WarRoomResolveModal
        isOpen={dialog === "resolve"}
        onClose={() => setDialog(null)}
        isSubmitting={isSubmitting}
        onConfirm={(note) => void handleResolve(note)}
      />
      <WarRoomConfirmModal
        isOpen={dialog === "archive"}
        onClose={() => setDialog(null)}
        title={t("war_room.archive_modal.title")}
        description={t("war_room.archive_modal.description")}
        cancelLabel={t("common.cancel")}
        confirmLabel={t("war_room.archive_modal.confirm")}
        isSubmitting={isSubmitting}
        onConfirm={() => void handleArchive()}
      />
      <WarRoomConfirmModal
        isOpen={dialog === "delete"}
        onClose={() => setDialog(null)}
        title={t("war_room.delete_modal.title")}
        description={t("war_room.delete_modal.description")}
        cancelLabel={t("common.cancel")}
        confirmLabel={t("war_room.delete_modal.confirm")}
        isDestructive
        isSubmitting={isSubmitting}
        onConfirm={() => void handleDelete()}
      />
      {dialog === "edit" && (
        <WarRoomEditDetailsModal
          isOpen
          onClose={() => setDialog(null)}
          room={room}
          workspaceSlug={workspaceSlug}
          projectId={projectId}
          isSubmitting={isSubmitting}
          onSave={(data) => void handleEditDetails(data)}
        />
      )}
    </header>
  );
});
