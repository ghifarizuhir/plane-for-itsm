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

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex shrink-0 items-center gap-1 border-b border-subtle px-2 pt-1.5">
        {CONTEXT_TABS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => setValue(tab.key)}
            className={cn(
              "rounded-t-sm border-b-2 px-2 py-1 text-11 font-medium transition-colors",
              activeTab === tab.key
                ? "border-accent-primary text-primary"
                : "border-transparent text-tertiary hover:text-secondary"
            )}
          >
            {t(tab.label_key)}
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
