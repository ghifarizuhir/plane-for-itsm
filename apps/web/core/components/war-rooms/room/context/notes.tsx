/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { debounce } from "lodash-es";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom } from "@plane/types";
import { calculateTimeAgo } from "@plane/utils";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS } from "@/services/service.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

type TSavingState = "idle" | "saving" | "saved";

export const WarRoomNotes = observer(function WarRoomNotes({ workspaceSlug, projectId, room, canWrite }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateWarRoom } = useWarRoom();
  const { getWorkspaceBySlug } = useWorkspace();
  // states
  const [savingState, setSavingState] = useState<TSavingState>("idle");
  // refs
  const saveRef = useRef<(html: string) => void>(() => undefined);
  const workspaceId = getWorkspaceBySlug(workspaceSlug)?.id ?? "";

  saveRef.current = (html: string) => {
    setSavingState("saving");
    void updateWarRoom(workspaceSlug, projectId, room.id, { notes_html: html })
      .then(() => setSavingState("saved"))
      .catch(() => {
        setSavingState("idle");
        setToast({
          type: TOAST_TYPE.ERROR,
          title: t("toast.error"),
          message: t("war_room.notes.save_failed"),
        });
      });
  };

  const debouncedSave = useMemo(
    () => debounce((html: string) => saveRef.current(html), 1000),
    // oxlint-disable-next-line exhaustive-deps -- debounce instance must survive re-renders
    []
  );

  useEffect(() => () => debouncedSave.flush(), [debouncedSave]);

  const handleChange = (_json: object, html: string) => {
    setSavingState("saving");
    debouncedSave(html);
  };

  const statusLabel =
    savingState === "saving"
      ? t("war_room.notes.saving")
      : savingState === "saved"
        ? t("war_room.notes.saved")
        : t("war_room.notes.edited_at", { time: calculateTimeAgo(room.updated_at) });

  const editorProps = {
    id: "war-room-notes-editor",
    initialValue: room.notes_html,
    workspaceSlug,
    workspaceId,
    projectId,
    disabledExtensions: SERVICE_DESCRIPTION_DISABLED_EXTENSIONS,
    onChange: handleChange,
    placeholder: t("war_room.notes.placeholder"),
    containerClassName: "min-h-48",
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.notes")}
        </span>
        <span className="text-10 text-tertiary">{statusLabel}</span>
      </div>
      {!canWrite && (
        <p className="border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 text-secondary">
          {t("war_room.notes.read_only")}
        </p>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {canWrite ? (
          <RichTextEditor
            {...editorProps}
            key={room.id}
            editable
            searchMentionCallback={async () => ({})}
            uploadFile={async () => {
              throw new Error("File upload is disabled for war room notes.");
            }}
            duplicateFile={async () => {
              throw new Error("File upload is disabled for war room notes.");
            }}
          />
        ) : (
          <RichTextEditor {...editorProps} key={room.id} editable={false} />
        )}
      </div>
    </div>
  );
});
