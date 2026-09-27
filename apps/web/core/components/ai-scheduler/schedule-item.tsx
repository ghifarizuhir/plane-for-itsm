/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { Badge } from "@plane/propel/badge";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// hooks
import { useMember } from "@/hooks/store/use-member";
// lib
import {
  RUN_STATUS_BADGE_VARIANTS,
  humanizeSchedule,
  scheduleDescription,
  scheduleStatusLabel,
  type TAiSchedule,
} from "@/lib/ai-schedule";

type Props = {
  schedule: TAiSchedule;
  onOpen: (scheduleId: string) => void;
};

export const ScheduleItem = observer(function ScheduleItem({ schedule, onOpen }: Props) {
  const {
    workspace: { getWorkspaceMemberDetails },
  } = useMember();
  const creator = getWorkspaceMemberDetails(schedule.created_by_id);

  return (
    <button
      type="button"
      className="w-full rounded-lg border border-subtle bg-layer-1 p-3 text-left transition-colors hover:bg-layer-2"
      onClick={() => onOpen(schedule.id)}
    >
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-medium break-words text-primary">{schedule.name}</p>
          {!schedule.enabled && (
            <Badge variant="neutral" size="sm">
              Paused
            </Badge>
          )}
          {schedule.last_status && (
            <Badge variant={RUN_STATUS_BADGE_VARIANTS[schedule.last_status]} size="sm">
              {scheduleStatusLabel(schedule.last_status)}
            </Badge>
          )}
        </div>
        <p className="text-xs mt-0.5 text-secondary">{humanizeSchedule(schedule)}</p>
        <p className="text-xs line-clamp-1 text-tertiary">{scheduleDescription(schedule)}</p>
        {schedule.enabled && schedule.next_run_at && (
          <p className="text-xs mt-0.5 text-tertiary">
            Next run: {renderFormattedDate(schedule.next_run_at)} at {renderFormattedTime(schedule.next_run_at)}
          </p>
        )}
        <p className="text-xs mt-0.5 text-tertiary">
          Created by {creator?.member?.display_name ?? "a workspace member"}
        </p>
      </div>
    </button>
  );
});
