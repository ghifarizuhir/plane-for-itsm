/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { observer } from "mobx-react";
import { motion, useReducedMotion } from "framer-motion";
import { useParams } from "next/navigation";
import {
  CalendarOutline,
  CloseOutline,
  EditOutline,
  PauseOutline,
  PlayOutline,
  DeleteOutline,
} from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { Badge } from "@plane/propel/badge";
import { Button } from "@plane/propel/button";
import { Switch } from "@plane/propel/switch";
import { Tabs } from "@plane/propel/tabs";
import { EUserWorkspaceRoles } from "@plane/types";
import { calculateTimeAgo, cn, renderFormattedDate, renderFormattedTime } from "@plane/utils";
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useMember } from "@/hooks/store/use-member";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import {
  RUN_STATUS_BADGE_VARIANTS,
  humanizeSchedule,
  scheduleDescription,
  scheduleStatusLabel,
  type TAiSchedule,
} from "@/lib/ai-schedule";
import { ScheduleEditForm } from "./schedule-edit-form";
import { ScheduleRunsList } from "./schedule-runs-list";
import type { TSchedulePendingAction } from "./schedule-row";

type TScheduleTab = "overview" | "recipe" | "history";

type Props = {
  schedule: TAiSchedule | null;
  isOpen: boolean;
  initialEdit: boolean;
  onClose: () => void;
  onDeleteRequest: (schedule: TAiSchedule) => void;
  onToggle: (schedule: TAiSchedule, enabled: boolean) => void;
  onRunNow: (schedule: TAiSchedule) => void;
  pendingAction: TSchedulePendingAction;
};

const humanizeTool = (tool: string): string => {
  const label = tool.replaceAll("_", " ");
  return label.charAt(0).toUpperCase() + label.slice(1);
};

export const ScheduleDetailPanel = observer(function ScheduleDetailPanel(props: Props) {
  const { schedule, isOpen, initialEdit, onClose, onDeleteRequest, onToggle, onRunNow, pendingAction } = props;
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const { fetchRuns, runsBySchedule } = useAiSchedules();
  const {
    workspace: { getWorkspaceMemberDetails },
  } = useMember();
  const { data: currentUser } = useUser();
  const { getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  const reducedMotion = useReducedMotion();
  const [mounted, setMounted] = useState(false);
  const [activeTab, setActiveTab] = useState<TScheduleTab>("overview");
  const [isEditing, setIsEditing] = useState(initialEdit);
  const [loadingRuns, setLoadingRuns] = useState(false);
  const [runsError, setRunsError] = useState<string | null>(null);

  const scheduleId = schedule?.id;

  useEffect(() => {
    setMounted(true);
  }, []);

  useEffect(() => {
    setActiveTab("overview");
    setIsEditing(initialEdit);
  }, [scheduleId, isOpen, initialEdit]);

  const loadRuns = useCallback(() => {
    if (!slug || !scheduleId) return;
    setLoadingRuns(true);
    setRunsError(null);
    void fetchRuns(slug, scheduleId)
      .catch(() => setRunsError("Could not load the run history."))
      .finally(() => setLoadingRuns(false));
  }, [slug, scheduleId, fetchRuns]);

  useEffect(() => {
    if (!isOpen || !scheduleId) return;
    loadRuns();
  }, [isOpen, scheduleId, loadRuns]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (document.querySelector('[role="alertdialog"], [role="dialog"]')) return;
      if (isEditing) {
        setIsEditing(false);
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isEditing, onClose]);

  const role = slug ? getWorkspaceRoleByWorkspaceSlug(slug) : undefined;
  const creator = schedule ? getWorkspaceMemberDetails(schedule.created_by_id) : undefined;
  const creatorName = creator?.member?.display_name ?? "a workspace member";
  const runs = schedule ? (runsBySchedule[schedule.id] ?? schedule.runs ?? []) : [];
  const canManage = schedule ? currentUser?.id === schedule.created_by_id || role === EUserWorkspaceRoles.ADMIN : false;
  const recentRuns = runs.slice(0, 3);
  const isActionPending = pendingAction !== null;

  if (!mounted || !isOpen || !schedule) return null;

  const handleToggle = (enabled: boolean) => {
    if (!canManage || isActionPending) return;
    onToggle(schedule, enabled);
  };

  const handleRunNow = () => {
    if (!canManage || isActionPending) return;
    onRunNow(schedule);
  };

  const portalTarget = typeof document !== "undefined" ? document.getElementById("full-screen-portal") : null;
  const showMotion = !reducedMotion;
  const lastRunLabel = schedule.last_status ? scheduleStatusLabel(schedule.last_status) : null;

  const panel = (
    <div
      className={cn("inset-0 z-[25] flex justify-end", portalTarget ? "absolute" : "fixed", "pointer-events-none")}
      role="presentation"
    >
      <div
        className="pointer-events-auto absolute inset-0 bg-backdrop/50 md:hidden"
        onClick={onClose}
        role="presentation"
        aria-hidden="true"
      />
      <motion.aside
        initial={showMotion ? { opacity: 0, x: 28 } : false}
        animate={{ opacity: 1, x: 0 }}
        transition={{ duration: 0.2, ease: "easeOut" }}
        className="pointer-events-auto relative flex h-full w-full flex-col overflow-hidden border-l border-subtle bg-surface-1 shadow-raised-200 md:w-[36rem] xl:w-[42rem]"
        aria-label={`Schedule details: ${schedule.name}`}
      >
        <div className="flex h-11 flex-shrink-0 items-center justify-between border-b border-subtle px-4">
          <div className="flex min-w-0 items-center gap-2 text-11 font-medium text-tertiary">
            <CalendarOutline className="size-3.5" />
            <span className="truncate">Schedule</span>
          </div>
          <Tooltip label="Close" side="bottom">
            <button
              type="button"
              onClick={onClose}
              aria-label="Close schedule details"
              className="grid size-7 place-items-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
            >
              <CloseOutline className="size-4" />
            </button>
          </Tooltip>
        </div>

        <div className="flex-shrink-0 border-b border-subtle px-5 py-4">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-16 font-semibold break-words text-primary">{schedule.name}</h3>
            {!schedule.enabled && (
              <Badge variant="neutral" size="sm">
                Paused
              </Badge>
            )}
            {schedule.last_status && (
              <Badge variant={RUN_STATUS_BADGE_VARIANTS[schedule.last_status]} size="sm">
                {lastRunLabel}
              </Badge>
            )}
          </div>
          <p className="mt-1 text-12 text-secondary">{humanizeSchedule(schedule)}</p>
          <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-11 text-tertiary">
            <span>
              {schedule.enabled && schedule.next_run_at
                ? `Next run ${calculateTimeAgo(schedule.next_run_at)}`
                : "Paused"}
            </span>
            <span aria-hidden="true">·</span>
            <span>Created by {creatorName}</span>
          </div>
        </div>

        {isEditing ? (
          <div className="min-h-0 flex-1 overflow-y-auto">
            <ScheduleEditForm
              schedule={schedule}
              onCancel={() => setIsEditing(false)}
              onSaved={() => setIsEditing(false)}
            />
          </div>
        ) : (
          <>
            <div className="flex min-h-0 flex-1">
              <Tabs
                value={activeTab}
                onValueChange={(value) => setActiveTab(value as TScheduleTab)}
                className="min-h-0 flex-1"
              >
                <div className="flex-shrink-0 border-b border-subtle px-4 py-2">
                  <Tabs.List className="flex h-8 w-fit gap-1">
                    <Tabs.Trigger value="overview" size="sm" className="min-w-0 px-3">
                      Overview
                    </Tabs.Trigger>
                    <Tabs.Trigger value="recipe" size="sm" className="min-w-0 px-3">
                      Recipe
                    </Tabs.Trigger>
                    <Tabs.Trigger value="history" size="sm" className="min-w-0 px-3">
                      History
                      {runs.length > 0 && <span className="ml-1 text-10 text-tertiary">{runs.length}</span>}
                    </Tabs.Trigger>
                  </Tabs.List>
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
                  <Tabs.Content value="overview">
                    <div className="space-y-5">
                      <section>
                        <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">What it does</h4>
                        <p className="mt-1 text-13 leading-relaxed whitespace-pre-wrap text-secondary">
                          {scheduleDescription(schedule)}
                        </p>
                      </section>

                      <div className="grid grid-cols-2 gap-2">
                        <div className="rounded-lg border border-subtle bg-layer-1 p-3">
                          <p className="text-10 font-medium tracking-wide text-tertiary uppercase">Next run</p>
                          <p className="mt-1 text-12 font-medium text-primary">
                            {schedule.enabled && schedule.next_run_at
                              ? calculateTimeAgo(schedule.next_run_at)
                              : "Paused"}
                          </p>
                          {schedule.enabled && schedule.next_run_at && (
                            <p className="mt-0.5 text-11 text-tertiary">
                              {renderFormattedDate(schedule.next_run_at)} at {renderFormattedTime(schedule.next_run_at)}
                            </p>
                          )}
                        </div>
                        <div className="rounded-lg border border-subtle bg-layer-1 p-3">
                          <p className="text-10 font-medium tracking-wide text-tertiary uppercase">Last run</p>
                          <p className="mt-1 text-12 font-medium text-primary">{lastRunLabel ?? "No runs yet"}</p>
                          {schedule.last_run_at && (
                            <p className="mt-0.5 text-11 text-tertiary">{calculateTimeAgo(schedule.last_run_at)}</p>
                          )}
                        </div>
                      </div>

                      <div className="grid grid-cols-2 gap-3 rounded-lg border border-subtle bg-layer-1 p-3 lg:hidden">
                        <div className="col-span-2 flex items-center justify-between gap-2">
                          <span className="text-11 font-medium tracking-wide text-tertiary uppercase">Status</span>
                          <div className="flex items-center gap-2">
                            <Switch
                              value={schedule.enabled}
                              onChange={handleToggle}
                              label="Schedule active"
                              disabled={!canManage || isActionPending}
                            />
                            <span className="text-12 text-secondary">{schedule.enabled ? "Active" : "Paused"}</span>
                          </div>
                        </div>
                        <Property label="Cadence" value={humanizeSchedule(schedule)} />
                        <Property label="Creator" value={creatorName} />
                        <Property label="Created" value={renderFormattedDate(schedule.created_at) ?? "—"} />
                      </div>

                      {schedule.spec ? (
                        <section className="rounded-lg border border-subtle bg-layer-1 p-3">
                          <div className="flex items-center justify-between gap-2">
                            <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Recipe</h4>
                            <Button size="sm" variant="ghost" onClick={() => setActiveTab("recipe")}>
                              Open recipe
                            </Button>
                          </div>
                          <p className="mt-1 line-clamp-2 text-12 text-secondary">{schedule.spec.description}</p>
                          <p className="mt-1.5 text-11 text-tertiary">
                            {schedule.spec.how_to.length} steps · {schedule.spec.tools.length} tools
                          </p>
                        </section>
                      ) : (
                        <section>
                          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Prompt</h4>
                          <p className="mt-1 line-clamp-4 text-12 whitespace-pre-wrap text-secondary">
                            {schedule.prompt}
                          </p>
                        </section>
                      )}

                      <section>
                        <div className="flex items-center justify-between gap-2">
                          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Recent runs</h4>
                          {runs.length > 0 && (
                            <Button size="sm" variant="ghost" onClick={() => setActiveTab("history")}>
                              View all
                            </Button>
                          )}
                        </div>
                        {loadingRuns && runs.length === 0 ? (
                          <p className="mt-2 text-12 text-tertiary">Loading run history…</p>
                        ) : runsError ? (
                          <div className="mt-2 flex items-center gap-2">
                            <p className="text-12 text-danger-primary">{runsError}</p>
                            <Button size="sm" variant="ghost" onClick={loadRuns}>
                              Retry
                            </Button>
                          </div>
                        ) : (
                          <ScheduleRunsList
                            runs={recentRuns}
                            emptyMessage="No runs yet. Use Run now to trigger the first one."
                          />
                        )}
                      </section>
                    </div>
                  </Tabs.Content>

                  <Tabs.Content value="recipe">
                    {schedule.spec ? (
                      <div className="space-y-5">
                        <section>
                          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Description</h4>
                          <p className="mt-1 text-13 leading-relaxed whitespace-pre-wrap text-secondary">
                            {schedule.spec.description}
                          </p>
                        </section>
                        <section>
                          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">How to</h4>
                          <ol className="mt-2 space-y-2">
                            {schedule.spec.how_to.map((step, index) => (
                              <li key={`${schedule.id}-${step}`} className="flex gap-2.5 text-12 text-secondary">
                                <span className="grid size-5 flex-shrink-0 place-items-center rounded-full border border-subtle bg-layer-1 text-10 font-medium text-tertiary">
                                  {index + 1}
                                </span>
                                <span className="pt-0.5 leading-relaxed">{step}</span>
                              </li>
                            ))}
                          </ol>
                        </section>
                        <section>
                          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Tools</h4>
                          <div className="mt-2 flex flex-wrap gap-1.5">
                            {schedule.spec.tools.map((tool) => (
                              <Badge key={tool} variant="neutral" size="sm">
                                {humanizeTool(tool)}
                              </Badge>
                            ))}
                          </div>
                        </section>
                        <section>
                          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Expected output</h4>
                          <p className="mt-1 rounded-lg border border-subtle bg-layer-1 p-3 text-12 leading-relaxed whitespace-pre-wrap text-secondary">
                            {schedule.spec.expected_output}
                          </p>
                        </section>
                      </div>
                    ) : (
                      <section>
                        <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Prompt (legacy)</h4>
                        <pre className="font-sans mt-1 rounded-lg border border-subtle bg-layer-1 p-3 text-12 leading-relaxed whitespace-pre-wrap text-secondary">
                          {schedule.prompt}
                        </pre>
                      </section>
                    )}
                  </Tabs.Content>

                  <Tabs.Content value="history">
                    {loadingRuns && runs.length === 0 ? (
                      <p className="text-12 text-tertiary">Loading run history…</p>
                    ) : runsError ? (
                      <div className="flex flex-col items-start gap-2">
                        <p className="text-12 text-danger-primary">{runsError}</p>
                        <Button size="sm" variant="secondary" onClick={loadRuns}>
                          Retry
                        </Button>
                      </div>
                    ) : (
                      <ScheduleRunsList runs={runs} emptyMessage="No runs yet. Use Run now to trigger the first one." />
                    )}
                  </Tabs.Content>
                </div>
              </Tabs>

              <aside className="hidden w-60 flex-shrink-0 overflow-y-auto border-l border-subtle px-4 py-4 lg:block">
                <div className="space-y-4">
                  <div>
                    <p className="text-10 font-medium tracking-wide text-tertiary uppercase">Status</p>
                    <div className="mt-1.5 flex items-center gap-2">
                      <Switch
                        value={schedule.enabled}
                        onChange={handleToggle}
                        label="Schedule active"
                        disabled={!canManage || isActionPending}
                      />
                      <span className="text-12 text-secondary">{schedule.enabled ? "Active" : "Paused"}</span>
                    </div>
                  </div>
                  <Property label="Cadence" value={humanizeSchedule(schedule)} />
                  <Property
                    label="Next run"
                    value={
                      schedule.enabled && schedule.next_run_at
                        ? `${renderFormattedDate(schedule.next_run_at)} at ${renderFormattedTime(schedule.next_run_at)}`
                        : "Paused"
                    }
                  />
                  <Property label="Last run" value={lastRunLabel ?? "No runs yet"} />
                  <Property label="Creator" value={creatorName} />
                  <Property label="Created" value={renderFormattedDate(schedule.created_at) ?? "—"} />
                </div>
              </aside>
            </div>

            <div className="flex flex-shrink-0 flex-wrap items-center justify-end gap-2 border-t border-subtle px-4 py-3">
              <Button size="sm" variant="ghost" onClick={onClose}>
                Close
              </Button>
              {canManage && (
                <>
                  <Button size="sm" variant="secondary" disabled={isActionPending} onClick={handleRunNow}>
                    <span className="flex items-center gap-1.5">
                      <PlayOutline className="size-3.5" />
                      {pendingAction === "run" ? "Queueing…" : "Run now"}
                    </span>
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={isActionPending}
                    onClick={() => handleToggle(!schedule.enabled)}
                  >
                    <span className="flex items-center gap-1.5">
                      <PauseOutline className="size-3.5" />
                      {pendingAction === "toggle" ? "Saving…" : schedule.enabled ? "Pause" : "Resume"}
                    </span>
                  </Button>
                  <Button size="sm" variant="secondary" disabled={isActionPending} onClick={() => setIsEditing(true)}>
                    <span className="flex items-center gap-1.5">
                      <EditOutline className="size-3.5" />
                      Edit
                    </span>
                  </Button>
                  <Button
                    size="sm"
                    variant="error-outline"
                    disabled={isActionPending}
                    onClick={() => onDeleteRequest(schedule)}
                  >
                    <span className="flex items-center gap-1.5">
                      <DeleteOutline className="size-3.5" />
                      Delete
                    </span>
                  </Button>
                </>
              )}
            </div>
          </>
        )}
      </motion.aside>
    </div>
  );

  return createPortal(panel, portalTarget ?? document.body);
});

function Property({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-10 font-medium tracking-wide text-tertiary uppercase">{label}</p>
      <p className="mt-0.5 text-12 break-words text-secondary">{value}</p>
    </div>
  );
}
