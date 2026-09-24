/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Switch } from "@makeplane/propel/components/switch";
import { Badge } from "@plane/propel/badge";
import { Button } from "@plane/propel/button";
import { EUserWorkspaceRoles } from "@plane/types";
import { AlertModalCore } from "@plane/ui";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// hooks
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useUser, useUserPermissions } from "@/hooks/store/user";
// lib
import { humanizeSchedule, type TAiSchedule, type TAiScheduleRun } from "@/lib/ai-schedule";
// local imports
import { ScheduleRunsList } from "./schedule-runs-list";

type Props = {
  schedule: TAiSchedule;
};

const LAST_STATUS_BADGE_VARIANTS: Record<TAiScheduleRun["status"], "success" | "danger" | "brand"> = {
  success: "success",
  failed: "danger",
  queued: "brand",
  running: "brand",
};

export const ScheduleItem = observer(function ScheduleItem({ schedule }: Props) {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  // store hooks
  const { toggleSchedule, deleteSchedule, runNow, fetchRuns, runsBySchedule } = useAiSchedules();
  const { data: currentUser } = useUser();
  const { getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  // local state
  const [expanded, setExpanded] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [running, setRunning] = useState(false);

  const role = slug ? getWorkspaceRoleByWorkspaceSlug(slug) : undefined;
  const canManage = currentUser?.id === schedule.created_by_id || role === EUserWorkspaceRoles.ADMIN;
  const runs = runsBySchedule[schedule.id] ?? schedule.runs ?? [];

  const handleToggleExpanded = () => {
    const next = !expanded;
    setExpanded(next);
    if (next && slug) void fetchRuns(slug, schedule.id).catch(() => {});
  };

  const handleRunNow = async () => {
    if (!slug) return;
    setRunning(true);
    try {
      await runNow(slug, schedule.id);
    } catch {
      // the run was not queued; the next poll refreshes the list
    } finally {
      setRunning(false);
    }
  };

  const handleDelete = async () => {
    if (!slug) return;
    setDeleting(true);
    try {
      await deleteSchedule(slug, schedule.id);
      setDeleteOpen(false);
    } catch {
      // keep the dialog open so the user can retry
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="rounded-lg border border-subtle bg-layer-1 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-medium break-words text-primary">{schedule.name}</p>
            {schedule.last_status && (
              <Badge variant={LAST_STATUS_BADGE_VARIANTS[schedule.last_status]} size="sm">
                {schedule.last_status}
              </Badge>
            )}
            {!schedule.enabled && <span className="text-xs text-tertiary">Paused</span>}
          </div>
          <p className="text-xs mt-0.5 text-secondary">{humanizeSchedule(schedule)}</p>
          <p className="text-xs mt-0.5 text-tertiary">
            Next run: {renderFormattedDate(schedule.next_run_at)} at {renderFormattedTime(schedule.next_run_at)}
          </p>
        </div>
        <Switch
          size="sm"
          checked={schedule.enabled}
          onCheckedChange={(checked) => {
            if (slug) void toggleSchedule(slug, schedule.id, checked).catch(() => {});
          }}
          disabled={!canManage}
          aria-label={schedule.enabled ? "Pause schedule" : "Resume schedule"}
        />
      </div>
      <div className="mt-2.5 flex flex-wrap items-center gap-2">
        {canManage && (
          <>
            <Button size="sm" variant="secondary" disabled={running} onClick={() => void handleRunNow()}>
              {running ? "Queueing…" : "Run now"}
            </Button>
            <Button size="sm" variant="error-outline" onClick={() => setDeleteOpen(true)}>
              Delete
            </Button>
          </>
        )}
        <Button size="sm" variant="ghost" onClick={handleToggleExpanded}>
          {expanded ? "Hide history" : "History"}
        </Button>
      </div>
      {expanded && <ScheduleRunsList runs={runs} />}
      <AlertModalCore
        isOpen={deleteOpen}
        handleClose={() => setDeleteOpen(false)}
        handleSubmit={() => void handleDelete()}
        isSubmitting={deleting}
        title="Delete schedule"
        content={`"${schedule.name}" will stop running and its run history will be removed.`}
        primaryButtonText={{ loading: "Deleting", default: "Delete" }}
      />
    </div>
  );
});
