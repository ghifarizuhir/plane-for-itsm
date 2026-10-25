/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { CalendarOutline, ChevronRightOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
import type {
  TAiSchedule,
  TAiScheduleDisplayProperties,
  TAiScheduleGroup,
  TAiScheduleGroupBy,
} from "@/lib/ai-schedule";
import { ScheduleRow, type TSchedulePendingAction } from "./schedule-row";

type Props = {
  groups: TAiScheduleGroup[];
  groupBy: TAiScheduleGroupBy;
  collapsedGroups: string[];
  onToggleGroup: (groupKey: string) => void;
  displayProperties: TAiScheduleDisplayProperties;
  canManage: (schedule: TAiSchedule) => boolean;
  pendingAction: { id: string; type: Exclude<TSchedulePendingAction, null> } | null;
  onOpen: (schedule: TAiSchedule) => void;
  onRunNow: (schedule: TAiSchedule) => void;
  onToggle: (schedule: TAiSchedule, enabled: boolean) => void;
  onEdit: (schedule: TAiSchedule) => void;
  onDelete: (schedule: TAiSchedule) => void;
};

export function ScheduleList(props: Props) {
  const {
    groups,
    groupBy,
    collapsedGroups,
    onToggleGroup,
    displayProperties,
    canManage,
    pendingAction,
    onOpen,
    onRunNow,
    onToggle,
    onEdit,
    onDelete,
  } = props;

  return (
    <div className="flex flex-col">
      {groups.map((group) => {
        const isCollapsed = collapsedGroups.includes(group.key);
        return (
          <section key={group.key} aria-label={group.label}>
            <button
              type="button"
              onClick={() => onToggleGroup(group.key)}
              className="sticky top-0 z-[2] flex w-full items-center gap-2 border-b border-subtle bg-surface-1 px-3 py-2 text-left"
            >
              <span className="grid size-5 flex-shrink-0 place-items-center text-tertiary">
                {groupBy === "status" ? (
                  <span
                    className={cn(
                      "size-2 rounded-full",
                      group.key === "active" ? "bg-success-primary" : "bg-placeholder"
                    )}
                  />
                ) : (
                  <CalendarOutline className="size-3.5" />
                )}
              </span>
              <span className="text-12 font-semibold text-primary">{group.label}</span>
              <span className="rounded bg-layer-1 px-1.5 py-0.5 text-10 font-medium text-tertiary">
                {group.schedules.length}
              </span>
              <ChevronRightOutline
                className={cn("ml-auto size-3.5 text-tertiary transition-transform", !isCollapsed && "rotate-90")}
              />
            </button>
            {!isCollapsed && (
              <div>
                {group.schedules.map((schedule) => (
                  <ScheduleRow
                    key={schedule.id}
                    schedule={schedule}
                    displayProperties={displayProperties}
                    canManage={canManage(schedule)}
                    pendingAction={pendingAction?.id === schedule.id ? pendingAction.type : null}
                    onOpen={onOpen}
                    onRunNow={onRunNow}
                    onToggle={onToggle}
                    onEdit={onEdit}
                    onDelete={onDelete}
                  />
                ))}
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
