/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { EditOutline, MoreHorizontalOutline, PauseOutline, PlayOutline, DeleteOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { Avatar } from "@plane/propel/avatar";
import { Badge } from "@plane/propel/badge";
import { CustomMenu } from "@plane/ui";
import { calculateTimeAgo, cn, getFileURL, renderFormattedDate, renderFormattedTime } from "@plane/utils";
import { useMember } from "@/hooks/store/use-member";
import {
  RUN_STATUS_BADGE_VARIANTS,
  compactScheduleCadence,
  humanizeSchedule,
  scheduleDescription,
  scheduleStatusLabel,
  type TAiSchedule,
  type TAiScheduleDisplayProperties,
} from "@/lib/ai-schedule";

export type TSchedulePendingAction = "run" | "toggle" | "delete" | null;

type Props = {
  schedule: TAiSchedule;
  displayProperties: TAiScheduleDisplayProperties;
  canManage: boolean;
  pendingAction: TSchedulePendingAction;
  onOpen: (schedule: TAiSchedule) => void;
  onRunNow: (schedule: TAiSchedule) => void;
  onToggle: (schedule: TAiSchedule, enabled: boolean) => void;
  onEdit: (schedule: TAiSchedule) => void;
  onDelete: (schedule: TAiSchedule) => void;
};

export const ScheduleRow = observer(function ScheduleRow(props: Props) {
  const { schedule, displayProperties, canManage, pendingAction, onOpen, onRunNow, onToggle, onEdit, onDelete } = props;
  const {
    workspace: { getWorkspaceMemberDetails },
  } = useMember();
  const creator = getWorkspaceMemberDetails(schedule.created_by_id);
  const creatorName = creator?.member?.display_name ?? "a workspace member";
  const isBusy = pendingAction !== null;
  const cadence = compactScheduleCadence(schedule);
  const lastStatus = schedule.last_status ? scheduleStatusLabel(schedule.last_status) : null;

  return (
    <div
      className={cn(
        "group/row flex items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors focus-within:bg-layer-transparent-hover hover:bg-layer-transparent-hover",
        isBusy && "opacity-60"
      )}
    >
      <button
        type="button"
        aria-label={`Open schedule ${schedule.name}`}
        onClick={() => onOpen(schedule)}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 text-left outline-none"
      >
        <div className="flex min-w-0 flex-1 items-start gap-2.5">
          <span
            className={cn(
              "mt-1.5 size-2 flex-shrink-0 rounded-full",
              schedule.enabled ? "bg-success-primary" : "bg-placeholder"
            )}
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-1.5">
              <p className="truncate text-13 font-medium text-primary">{schedule.name}</p>
              {!schedule.enabled && (
                <Badge variant="neutral" size="sm">
                  Paused
                </Badge>
              )}
              {schedule.last_status && (
                <Badge variant={RUN_STATUS_BADGE_VARIANTS[schedule.last_status]} size="sm">
                  {lastStatus}
                </Badge>
              )}
            </div>
            <p className="mt-0.5 line-clamp-1 text-11 text-tertiary">{scheduleDescription(schedule)}</p>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-11 text-secondary lg:hidden">
              <span>{cadence}</span>
              {displayProperties.nextRun && schedule.enabled && schedule.next_run_at && (
                <span className="text-tertiary">Next {calculateTimeAgo(schedule.next_run_at)}</span>
              )}
              {displayProperties.lastRun && schedule.last_run_at && (
                <span className="text-tertiary">Last run {calculateTimeAgo(schedule.last_run_at)}</span>
              )}
              {displayProperties.creator && <span className="text-tertiary">{creatorName}</span>}
            </div>
          </div>
        </div>

        <div className="hidden flex-shrink-0 items-center gap-4 text-11 lg:flex">
          {displayProperties.cadence && (
            <Tooltip label={humanizeSchedule(schedule)} side="bottom">
              <span className="w-36 truncate text-secondary">{cadence}</span>
            </Tooltip>
          )}
          {displayProperties.nextRun && (
            <span className="w-28 text-secondary">
              {schedule.enabled && schedule.next_run_at ? (
                <Tooltip
                  label={`${renderFormattedDate(schedule.next_run_at)} at ${renderFormattedTime(schedule.next_run_at)}`}
                  side="bottom"
                >
                  <span>Next {calculateTimeAgo(schedule.next_run_at)}</span>
                </Tooltip>
              ) : (
                <span className="text-placeholder">Paused</span>
              )}
            </span>
          )}
          {displayProperties.lastRun && (
            <span className="flex w-36 items-center gap-1.5 text-tertiary">
              {schedule.last_status ? (
                <>
                  <span className="size-1.5 rounded-full bg-current" aria-hidden="true" />
                  <span className="text-secondary">{lastStatus}</span>
                  {schedule.last_run_at && <span>{calculateTimeAgo(schedule.last_run_at)}</span>}
                </>
              ) : (
                <span className="text-placeholder">No runs yet</span>
              )}
            </span>
          )}
          {displayProperties.creator && (
            <Tooltip label={creatorName} side="bottom">
              <span className="flex w-7 justify-end">
                <Avatar
                  size="md"
                  src={creator?.member?.avatar_url ? getFileURL(creator.member.avatar_url) : undefined}
                  fallbackText={creatorName[0]?.toUpperCase()}
                  name={creatorName}
                />
              </span>
            </Tooltip>
          )}
        </div>
      </button>

      {canManage && (
        <div className="flex flex-shrink-0 items-center">
          <CustomMenu
            placement="bottom-end"
            ariaLabel={`Actions for ${schedule.name}`}
            customButton={
              <span
                className={cn(
                  "grid size-7 place-items-center rounded-md text-tertiary transition-all hover:bg-layer-1 hover:text-primary",
                  "md:opacity-0 md:group-focus-within/row:opacity-100 md:group-hover/row:opacity-100",
                  isBusy && "pointer-events-none"
                )}
              >
                <MoreHorizontalOutline className="size-4" />
              </span>
            }
          >
            <CustomMenu.MenuItem disabled={isBusy} onClick={() => onRunNow(schedule)}>
              <span className="flex items-center gap-2">
                <PlayOutline className="size-3.5" />
                Run now
              </span>
            </CustomMenu.MenuItem>
            <CustomMenu.MenuItem disabled={isBusy} onClick={() => onToggle(schedule, !schedule.enabled)}>
              <span className="flex items-center gap-2">
                <PauseOutline className="size-3.5" />
                {schedule.enabled ? "Pause" : "Resume"}
              </span>
            </CustomMenu.MenuItem>
            <CustomMenu.MenuItem disabled={isBusy} onClick={() => onEdit(schedule)}>
              <span className="flex items-center gap-2">
                <EditOutline className="size-3.5" />
                Edit
              </span>
            </CustomMenu.MenuItem>
            <CustomMenu.MenuItem disabled={isBusy} onClick={() => onDelete(schedule)}>
              <span className="flex items-center gap-2 text-danger-primary">
                <DeleteOutline className="size-3.5" />
                Delete
              </span>
            </CustomMenu.MenuItem>
          </CustomMenu>
        </div>
      )}
    </div>
  );
});
