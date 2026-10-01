/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, IWarRoomRunbookItem } from "@plane/types";
import { cn } from "@plane/utils";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomRunbook = observer(function WarRoomRunbook({ workspaceSlug, projectId, room, canWrite }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { createRunbookItem, updateRunbookItem, deleteRunbookItem } = useWarRoom();
  // states
  const [newTitle, setNewTitle] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState("");
  // derived values
  const doneCount = room.runbook_items.filter((item) => item.is_done).length;
  const totalCount = room.runbook_items.length;
  const progress = totalCount === 0 ? 0 : Math.round((doneCount / totalCount) * 100);

  const showError = () =>
    setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.runbook.update_failed") });

  const handleCreate = async () => {
    const title = newTitle.trim();
    if (title === "") return;
    try {
      await createRunbookItem(workspaceSlug, projectId, room.id, title);
      setNewTitle("");
    } catch {
      showError();
    }
  };

  const handleToggle = async (item: IWarRoomRunbookItem) => {
    try {
      await updateRunbookItem(workspaceSlug, projectId, room.id, item.id, { is_done: !item.is_done });
    } catch {
      showError();
    }
  };

  const handleSaveTitle = async (itemId: string) => {
    const title = editingTitle.trim();
    if (title === "") return;
    try {
      await updateRunbookItem(workspaceSlug, projectId, room.id, itemId, { title });
      setEditingId(null);
    } catch {
      showError();
    }
  };

  const handleDelete = async (itemId: string) => {
    try {
      await deleteRunbookItem(workspaceSlug, projectId, room.id, itemId);
    } catch {
      showError();
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="border-b border-subtle px-3 py-1.5">
        <div className="flex items-center justify-between">
          <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
            {t("war_room.context_tabs.runbook")}
          </span>
          <span className="text-10 text-tertiary">
            {t("war_room.runbook.progress", { done: doneCount, total: totalCount })}
          </span>
        </div>
        <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-layer-2">
          <div className="h-full rounded-full bg-success-primary transition-all" style={{ width: `${progress}%` }} />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {room.runbook_items.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.runbook.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.runbook.empty_description")}</p>
          </div>
        )}
        <div className="space-y-1">
          {room.runbook_items.map((item) => (
            <div
              key={item.id}
              className="group/runbook flex items-start gap-2 rounded-md px-1 py-1.5 hover:bg-layer-transparent-hover"
            >
              <input
                type="checkbox"
                checked={item.is_done}
                disabled={!canWrite}
                onChange={() => void handleToggle(item)}
                aria-label={item.is_done ? t("war_room.runbook.mark_undone") : t("war_room.runbook.mark_done")}
                className="accent-success-primary mt-0.5 h-3.5 w-3.5 shrink-0"
              />
              {editingId === item.id ? (
                <div className="flex min-w-0 flex-1 items-center gap-1">
                  <input
                    value={editingTitle}
                    onChange={(event) => setEditingTitle(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void handleSaveTitle(item.id);
                      if (event.key === "Escape") setEditingId(null);
                    }}
                    // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus -- inline edit input should take focus
                    autoFocus
                    className="min-w-0 flex-1 rounded-sm border border-subtle bg-surface-1 px-1.5 py-0.5 text-13 text-primary outline-none focus:border-strong"
                  />
                  <Button variant="primary" size="sm" onClick={() => void handleSaveTitle(item.id)}>
                    {t("war_room.runbook.save")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => setEditingId(null)}>
                    {t("war_room.runbook.cancel")}
                  </Button>
                </div>
              ) : (
                <>
                  <span
                    className={cn("min-w-0 flex-1 text-13 text-primary", item.is_done && "text-tertiary line-through")}
                  >
                    {item.title}
                  </span>
                  {canWrite && (
                    <span className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover/runbook:opacity-100">
                      <button
                        type="button"
                        onClick={() => {
                          setEditingId(item.id);
                          setEditingTitle(item.title);
                        }}
                        className="text-10 text-secondary hover:text-primary"
                      >
                        {t("war_room.runbook.edit")}
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleDelete(item.id)}
                        className="text-10 text-danger-primary hover:opacity-80"
                      >
                        {t("war_room.runbook.delete")}
                      </button>
                    </span>
                  )}
                </>
              )}
            </div>
          ))}
        </div>
      </div>
      {canWrite && (
        <div className="border-t border-subtle p-2">
          <input
            value={newTitle}
            onChange={(event) => setNewTitle(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void handleCreate();
            }}
            placeholder={t("war_room.runbook.add_placeholder")}
            className="w-full rounded-sm border border-subtle bg-surface-1 px-2 py-1.5 text-13 text-primary outline-none focus:border-strong"
          />
        </div>
      )}
    </div>
  );
});
