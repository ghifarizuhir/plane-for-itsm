/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Badge } from "@plane/propel/badge";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { EUserWorkspaceRoles } from "@plane/types";
import { AlertModalCore, ModalCore } from "@plane/ui";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// hooks
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useMember } from "@/hooks/store/use-member";
import { useUser, useUserPermissions } from "@/hooks/store/user";
// lib
import { RUN_STATUS_BADGE_VARIANTS, humanizeSchedule, scheduleStatusLabel, type TAiSchedule } from "@/lib/ai-schedule";
// local imports
import { ScheduleRunsList } from "./schedule-runs-list";

type Props = {
  schedule: TAiSchedule | null;
  isOpen: boolean;
  onClose: () => void;
};

export const ScheduleDetailModal = observer(function ScheduleDetailModal({ schedule, isOpen, onClose }: Props) {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  // store hooks
  const { toggleSchedule, deleteSchedule, runNow, fetchRuns, runsBySchedule } = useAiSchedules();
  const {
    workspace: { getWorkspaceMemberDetails },
  } = useMember();
  const { data: currentUser } = useUser();
  const { getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  // local state
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [running, setRunning] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [loadingRuns, setLoadingRuns] = useState(false);

  const scheduleId = schedule?.id;

  useEffect(() => {
    if (!isOpen || !slug || !scheduleId) return;
    setLoadingRuns(true);
    void fetchRuns(slug, scheduleId)
      .catch(() => {})
      .finally(() => setLoadingRuns(false));
  }, [isOpen, slug, scheduleId, fetchRuns]);

  if (!schedule) return null;

  const role = slug ? getWorkspaceRoleByWorkspaceSlug(slug) : undefined;
  const canManage = currentUser?.id === schedule.created_by_id || role === EUserWorkspaceRoles.ADMIN;
  const runs = runsBySchedule[schedule.id] ?? schedule.runs ?? [];
  const creator = getWorkspaceMemberDetails(schedule.created_by_id);
  const recipeSteps = schedule.spec?.how_to.map((step, index) => ({ id: `${index}-${step}`, step })) ?? [];

  const handleToggle = async (checked: boolean) => {
    if (!slug || toggling) return;
    setToggling(true);
    try {
      await toggleSchedule(slug, schedule.id, checked);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not update the schedule" });
    } finally {
      setToggling(false);
    }
  };

  const handleRunNow = async () => {
    if (!slug) return;
    setRunning(true);
    try {
      await runNow(slug, schedule.id);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not queue the run" });
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
      onClose();
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not delete the schedule" });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <>
      <ModalCore isOpen={isOpen} handleClose={onClose}>
        <div className="max-h-[80vh] space-y-4 overflow-y-auto p-5">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-base font-semibold break-words text-primary">{schedule.name}</h3>
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
          <p className="text-xs text-secondary">{humanizeSchedule(schedule)}</p>
          <p className="text-xs text-tertiary">
            {schedule.enabled && schedule.next_run_at
              ? `Next run: ${renderFormattedDate(schedule.next_run_at)} at ${renderFormattedTime(schedule.next_run_at)}`
              : "Paused"}
          </p>
          <p className="text-xs text-tertiary">Created by {creator?.member?.display_name ?? "a workspace member"}</p>

          <div className="rounded-md border border-subtle bg-layer-2 p-3">
            <p className="text-xs font-medium text-secondary">Recipe</p>
            {schedule.spec ? (
              <>
                <p className="text-xs mt-1 text-tertiary">{schedule.spec.description}</p>
                <p className="text-xs mt-2 font-medium text-secondary">How to</p>
                <ol className="text-xs mt-1 list-decimal space-y-0.5 pl-4 text-tertiary">
                  {recipeSteps.map((entry) => (
                    <li key={entry.id}>{entry.step}</li>
                  ))}
                </ol>
                <p className="text-xs mt-2 font-medium text-secondary">Tools</p>
                <div className="mt-1 flex flex-wrap gap-1">
                  {schedule.spec.tools.map((tool) => (
                    <Badge key={tool} variant="neutral" size="sm">
                      {tool}
                    </Badge>
                  ))}
                </div>
                <p className="text-xs mt-2 font-medium text-secondary">Expected output</p>
                <p className="text-xs mt-1 text-tertiary">{schedule.spec.expected_output}</p>
              </>
            ) : (
              <>
                <p className="text-xs mt-2 font-medium text-secondary">Prompt (legacy schedule)</p>
                <p className="text-xs mt-1 whitespace-pre-wrap text-tertiary">{schedule.prompt}</p>
              </>
            )}
          </div>

          <div>
            <p className="text-xs font-medium text-secondary">History</p>
            {loadingRuns ? (
              <p className="text-xs mt-2 text-tertiary">Loading history…</p>
            ) : (
              <ScheduleRunsList runs={runs} />
            )}
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-subtle p-4">
          {canManage && (
            <Button
              size="sm"
              variant="secondary"
              disabled={toggling}
              onClick={() => void handleToggle(!schedule.enabled)}
            >
              {schedule.enabled ? "Pause" : "Resume"}
            </Button>
          )}
          {canManage && (
            <Button size="sm" variant="secondary" disabled={running} onClick={() => void handleRunNow()}>
              {running ? "Queueing…" : "Run now"}
            </Button>
          )}
          {canManage && (
            <Button size="sm" variant="error-outline" onClick={() => setDeleteOpen(true)}>
              Delete
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </div>
      </ModalCore>
      <AlertModalCore
        isOpen={deleteOpen}
        handleClose={() => setDeleteOpen(false)}
        handleSubmit={() => void handleDelete()}
        isSubmitting={deleting}
        title="Delete schedule"
        content={`"${schedule.name}" will stop running and be hidden. Existing runs are kept.`}
        primaryButtonText={{ loading: "Deleting", default: "Delete" }}
      />
    </>
  );
});
