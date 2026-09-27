/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
// plane imports
import type { TNotificationScheduleRun } from "@plane/types";
import { Badge } from "@plane/propel/badge";
import { Button } from "@plane/propel/button";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// lib
import {
  scheduleRunNotificationHref,
  scheduleRunNotificationText,
  scheduleStatusLabel,
  type TAiScheduleRun,
} from "@/lib/ai-schedule";
// services
import { AiSchedulesService } from "@/services/ai-schedules.service";
// local imports
import { ScheduleRunsList } from "../ai-scheduler/schedule-runs-list";

type Props = {
  workspaceSlug: string;
  scheduleRun: TNotificationScheduleRun;
  embedRemoveCurrentNotification: () => void;
};

export function ScheduleRunInboxDetail({ workspaceSlug, scheduleRun, embedRemoveCurrentNotification }: Props) {
  // router
  const router = useRouter();
  // local state
  const [runs, setRuns] = useState<TAiScheduleRun[] | null>(null);
  const [scheduleMissing, setScheduleMissing] = useState(false);
  const [loadFailed, setLoadFailed] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setRuns(null);
    setScheduleMissing(false);
    setLoadFailed(false);
    const load = async () => {
      try {
        const schedule = await new AiSchedulesService().retrieve(workspaceSlug, scheduleRun.schedule_id);
        if (!cancelled) setRuns(schedule?.runs ?? []);
      } catch (error: unknown) {
        if (cancelled) return;
        if ((error as { status?: number } | null)?.status === 404) setScheduleMissing(true);
        else setLoadFailed(true);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, scheduleRun.schedule_id, reloadKey]);

  const run = runs?.find((candidate) => candidate.id === scheduleRun.run_id) ?? null;

  const openInScheduler = () => {
    router.push(scheduleRunNotificationHref(workspaceSlug, scheduleRun.schedule_id));
  };

  return (
    <div className="h-full w-full overflow-y-auto p-4">
      <div className="mx-auto max-w-2xl space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-base font-semibold break-words text-primary">{scheduleRun.name}</h3>
          <Badge variant={scheduleRun.status === "success" ? "success" : "danger"} size="sm">
            {scheduleStatusLabel(scheduleRun.status)}
          </Badge>
        </div>
        <p className="text-xs text-tertiary">
          {scheduleRunNotificationText(scheduleRun.status)}
          {scheduleRun.finished_at &&
            ` · ${renderFormattedDate(scheduleRun.finished_at)} at ${renderFormattedTime(scheduleRun.finished_at)}`}
        </p>

        {scheduleMissing ? (
          <p className="text-sm text-tertiary">This schedule no longer exists.</p>
        ) : loadFailed ? (
          <div className="flex flex-col items-start gap-2">
            <p className="text-sm text-danger-primary">Could not load the run output.</p>
            <Button size="sm" variant="secondary" onClick={() => setReloadKey((key) => key + 1)}>
              Retry
            </Button>
          </div>
        ) : runs === null ? (
          <p className="text-sm text-tertiary">Loading run output…</p>
        ) : run ? (
          <ScheduleRunsList runs={[run]} />
        ) : (
          <div className="rounded-md border border-subtle bg-layer-1 p-3">
            <p className="text-xs text-tertiary">
              The full output is no longer retained (only the 20 most recent runs are kept).
            </p>
            {scheduleRun.status === "failed" && scheduleRun.error && (
              <p className="text-xs mt-1.5 break-words text-danger-primary">{scheduleRun.error}</p>
            )}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="secondary" onClick={openInScheduler}>
            View in Scheduler
          </Button>
          <Button size="sm" variant="ghost" onClick={embedRemoveCurrentNotification}>
            Dismiss
          </Button>
        </div>
      </div>
    </div>
  );
}
