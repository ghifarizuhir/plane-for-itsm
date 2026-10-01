/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IWarRoom } from "@plane/types";
import { cn } from "@plane/utils";
// components
import { WarRoomActivity } from "./activity";
import { WarRoomNotes } from "./notes";
import { WarRoomPeople } from "./people";
import { WarRoomRunbook } from "./runbook";
import { WarRoomWorkItems } from "./work-items";
// hooks
import useLocalStorage from "@/hooks/use-local-storage";

type TContextTab = "notes" | "work_items" | "runbook" | "activity" | "people";

const CONTEXT_TABS: { key: TContextTab; label_key: string }[] = [
  { key: "notes", label_key: "war_room.context_tabs.notes" },
  { key: "work_items", label_key: "war_room.context_tabs.work_items" },
  { key: "runbook", label_key: "war_room.context_tabs.runbook" },
  { key: "activity", label_key: "war_room.context_tabs.activity" },
  { key: "people", label_key: "war_room.context_tabs.people" },
];

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomContextPanel = observer(function WarRoomContextPanel({
  workspaceSlug,
  projectId,
  room,
  canWrite,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // local storage
  const { storedValue, setValue } = useLocalStorage<TContextTab>(`war-room-context-tab-${room.id}`, "notes");
  // derived values
  const activeTab = CONTEXT_TABS.some((tab) => tab.key === storedValue) ? storedValue : "notes";
  const tabBadges: Partial<Record<TContextTab, string>> = {
    work_items: room.issues.length > 0 ? String(room.issues.length) : "",
    runbook:
      room.runbook_items.length > 0
        ? `${room.runbook_items.filter((item) => item.is_done).length}/${room.runbook_items.length}`
        : "",
    people: room.participants.length > 0 ? String(room.participants.length) : "",
  };

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-subtle px-2 py-1.5">
        {CONTEXT_TABS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => setValue(tab.key)}
            className={cn(
              "flex shrink-0 items-center gap-1 rounded-sm px-2 py-1 font-code text-10 font-medium tracking-[0.08em] uppercase transition-colors",
              activeTab === tab.key
                ? "bg-layer-1 text-primary"
                : "text-tertiary hover:bg-layer-transparent-hover hover:text-secondary"
            )}
          >
            {t(tab.label_key)}
            {tabBadges[tab.key] && (
              <span className="font-code text-9 text-tertiary tabular-nums">{tabBadges[tab.key]}</span>
            )}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1">
        {activeTab === "notes" && (
          <WarRoomNotes workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
        {activeTab === "work_items" && (
          <WarRoomWorkItems workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
        {activeTab === "runbook" && (
          <WarRoomRunbook workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
        {activeTab === "activity" && (
          <WarRoomActivity workspaceSlug={workspaceSlug} projectId={projectId} room={room} />
        )}
        {activeTab === "people" && (
          <WarRoomPeople workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
      </div>
    </div>
  );
});
