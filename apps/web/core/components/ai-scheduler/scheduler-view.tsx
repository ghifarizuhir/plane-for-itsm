/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import { Button } from "@plane/propel/button";
import { EmptyStateCompact, EmptyStateDetailed } from "@plane/propel/empty-state";
import { Skeleton } from "@plane/propel/skeleton";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { EUserWorkspaceRoles } from "@plane/types";
import { AlertModalCore } from "@plane/ui";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useAppTheme } from "@/hooks/store/use-app-theme";
import { useInstance } from "@/hooks/store/use-instance";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useQueryParams } from "@/hooks/use-query-params";
import {
  DEFAULT_SCHEDULE_DISPLAY_PROPERTIES,
  filterSchedules,
  groupSchedules,
  orderSchedules,
  type TAiSchedule,
  type TAiScheduleDisplayProperties,
  type TAiScheduleGroupBy,
  type TAiScheduleListFilter,
  type TAiScheduleOrderBy,
} from "@/lib/ai-schedule";
import { ScheduleDetailPanel } from "./schedule-detail-panel";
import { ScheduleList } from "./schedule-list";
import { ScheduleToolbar } from "./schedule-toolbar";
import type { TSchedulePendingAction } from "./schedule-row";

export const SchedulerView = observer(function SchedulerView() {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const router = useRouter();
  const searchParams = useSearchParams();
  const { updateQueryParams } = useQueryParams();
  // store hooks
  const {
    schedules,
    runsBySchedule,
    loader,
    error,
    setWorkspace,
    fetchSchedules,
    fetchRuns,
    toggleSchedule,
    runNow,
    deleteSchedule,
  } = useAiSchedules();
  const { data: currentUser } = useUser();
  const { getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  const { config } = useInstance();
  const { toggleAiSidebar } = useAppTheme();
  const { setMode } = useAiAssistant();
  // local state
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<TAiScheduleListFilter["status"]>("all");
  const [groupBy, setGroupBy] = useState<TAiScheduleGroupBy>("status");
  const [orderBy, setOrderBy] = useState<TAiScheduleOrderBy>("next_run");
  const [displayProperties, setDisplayProperties] = useState<TAiScheduleDisplayProperties>(
    DEFAULT_SCHEDULE_DISPLAY_PROPERTIES
  );
  const [collapsedGroups, setCollapsedGroups] = useState<string[]>([]);
  const [openScheduleId, setOpenScheduleId] = useState<string | null>(null);
  const [editMode, setEditMode] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<TAiSchedule | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [pendingAction, setPendingAction] = useState<{
    id: string;
    type: Exclude<TSchedulePendingAction, null>;
  } | null>(null);
  const [manualRefreshing, setManualRefreshing] = useState(false);
  const [lastUpdatedAt, setLastUpdatedAt] = useState<Date | null>(null);

  const requestedScheduleId = searchParams.get("schedule");
  const role = slug ? getWorkspaceRoleByWorkspaceSlug(slug) : undefined;
  const openSchedule = schedules.find((schedule) => schedule.id === openScheduleId) ?? null;

  const expandedRunsRef = useRef(runsBySchedule);
  useEffect(() => {
    expandedRunsRef.current = runsBySchedule;
  }, [runsBySchedule]);

  const refreshSchedules = useCallback(
    async (manual = false) => {
      if (!slug) return;
      if (manual) setManualRefreshing(true);
      try {
        await fetchSchedules(slug);
        setLastUpdatedAt(new Date());
        const scheduleIds = Object.keys(expandedRunsRef.current);
        await Promise.allSettled(scheduleIds.map((scheduleId) => fetchRuns(slug, scheduleId)));
      } catch {
        // the store keeps the previous list and surfaces the error inline
      } finally {
        if (manual) setManualRefreshing(false);
      }
    },
    [slug, fetchSchedules, fetchRuns]
  );

  useEffect(() => {
    if (!slug) return;
    setWorkspace(slug);
    void refreshSchedules();
    const interval = window.setInterval(() => {
      void refreshSchedules();
    }, 30_000);
    return () => window.clearInterval(interval);
  }, [slug, setWorkspace, refreshSchedules]);

  useEffect(() => {
    if (!requestedScheduleId) {
      setOpenScheduleId(null);
      setEditMode(false);
      return;
    }
    if (schedules.some((schedule) => schedule.id === requestedScheduleId)) {
      setOpenScheduleId(requestedScheduleId);
      return;
    }
    if (!loader) {
      setOpenScheduleId(null);
      setEditMode(false);
      router.replace(updateQueryParams({ paramsToRemove: ["schedule"] }));
    }
  }, [requestedScheduleId, schedules, loader, router, updateQueryParams]);

  const counts = useMemo(() => {
    const base = filterSchedules(schedules, { query, status: "all" });
    return {
      all: base.length,
      active: base.filter((schedule) => schedule.enabled).length,
      paused: base.filter((schedule) => !schedule.enabled).length,
    };
  }, [schedules, query]);

  const visibleSchedules = useMemo(
    () => orderSchedules(filterSchedules(schedules, { query, status }), orderBy),
    [schedules, query, status, orderBy]
  );

  const groups = useMemo(() => groupSchedules(visibleSchedules, groupBy), [visibleSchedules, groupBy]);

  const canManage = (schedule: TAiSchedule) =>
    currentUser?.id === schedule.created_by_id || role === EUserWorkspaceRoles.ADMIN;

  const openScheduleModal = (schedule: TAiSchedule, edit = false) => {
    setOpenScheduleId(schedule.id);
    setEditMode(edit);
    router.replace(updateQueryParams({ paramsToAdd: { schedule: schedule.id } }));
  };

  const closeScheduleModal = () => {
    setOpenScheduleId(null);
    setEditMode(false);
    if (requestedScheduleId) router.replace(updateQueryParams({ paramsToRemove: ["schedule"] }));
  };

  const handleToggle = async (schedule: TAiSchedule, enabled: boolean) => {
    if (!slug || pendingAction) return;
    setPendingAction({ id: schedule.id, type: "toggle" });
    try {
      await toggleSchedule(slug, schedule.id, enabled);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not update the schedule" });
    } finally {
      setPendingAction(null);
    }
  };

  const handleRunNow = async (schedule: TAiSchedule) => {
    if (!slug || pendingAction) return;
    setPendingAction({ id: schedule.id, type: "run" });
    try {
      await runNow(slug, schedule.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Run queued" });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not queue the run" });
    } finally {
      setPendingAction(null);
    }
  };

  const handleDelete = async () => {
    if (!slug || !deleteTarget) return;
    setDeleting(true);
    setPendingAction({ id: deleteTarget.id, type: "delete" });
    try {
      await deleteSchedule(slug, deleteTarget.id);
      if (openScheduleId === deleteTarget.id) closeScheduleModal();
      setDeleteTarget(null);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Schedule deleted" });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Could not delete the schedule" });
    } finally {
      setDeleting(false);
      setPendingAction(null);
    }
  };

  const toggleGroup = (groupKey: string) => {
    setCollapsedGroups((current) =>
      current.includes(groupKey) ? current.filter((key) => key !== groupKey) : [...current, groupKey]
    );
  };

  const openGalileo = () => {
    setMode("agent");
    toggleAiSidebar(false);
  };

  const showErrorState = !!error && schedules.length === 0;
  const initialLoading = loader && schedules.length === 0;
  const noSchedules = !loader && !error && schedules.length === 0;
  const noResults = schedules.length > 0 && visibleSchedules.length === 0;
  const openSchedulePendingAction = pendingAction && pendingAction.id === openSchedule?.id ? pendingAction.type : null;

  return (
    <>
      <div className="flex h-full w-full flex-col overflow-hidden">
        {schedules.length > 0 && (
          <ScheduleToolbar
            query={query}
            onQueryChange={setQuery}
            status={status}
            onStatusChange={setStatus}
            counts={counts}
            groupBy={groupBy}
            onGroupByChange={(value) => {
              setGroupBy(value);
              setCollapsedGroups([]);
            }}
            orderBy={orderBy}
            onOrderByChange={setOrderBy}
            displayProperties={displayProperties}
            onDisplayPropertiesChange={setDisplayProperties}
            resultCount={visibleSchedules.length}
            totalCount={schedules.length}
            refreshing={manualRefreshing}
            lastUpdatedAt={lastUpdatedAt}
            onRefresh={() => void refreshSchedules(true)}
          />
        )}

        <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto bg-surface-1">
          <div className="px-4 pb-8">
            {initialLoading ? (
              <ScheduleListSkeleton />
            ) : showErrorState ? (
              <div className="flex flex-col items-start gap-3 py-16">
                <p className="text-13 text-danger-primary">{error}</p>
                <Button size="sm" variant="secondary" onClick={() => void refreshSchedules(true)}>
                  Retry
                </Button>
              </div>
            ) : noSchedules ? (
              <EmptyStateDetailed
                assetKey="cycle"
                title="No scheduled runs yet"
                description={
                  config?.has_llm_configured
                    ? "Ask Galileo to run a report on a recurring cadence. Describe the outcome you want and the schedule, and confirm the proposal in chat."
                    : "AI is not configured for this workspace. Add an LLM configuration to start scheduling recurring agent runs."
                }
                align="center"
                rootClassName="py-16"
                actions={[
                  ...(config?.has_llm_configured
                    ? [{ label: "Open Galileo", variant: "primary" as const, onClick: openGalileo }]
                    : []),
                  { label: "Refresh", variant: "secondary" as const, onClick: () => void refreshSchedules(true) },
                ]}
              />
            ) : noResults ? (
              <EmptyStateCompact
                assetKey="search"
                title="No schedules match your filters"
                description="Try a different search term, status, or cadence group."
                className="py-16"
                actions={[
                  {
                    label: "Clear filters",
                    variant: "secondary",
                    onClick: () => {
                      setQuery("");
                      setStatus("all");
                    },
                  },
                ]}
              />
            ) : (
              <>
                {error && (
                  <div className="mt-3 flex flex-wrap items-center gap-2 rounded-md border border-danger-strong/40 bg-danger-subtle px-3 py-2">
                    <p className="text-12 text-danger-primary">
                      Couldn&apos;t refresh the list. Showing the last loaded schedules.
                    </p>
                    <Button size="sm" variant="ghost" onClick={() => void refreshSchedules(true)}>
                      Retry
                    </Button>
                  </div>
                )}
                <ScheduleList
                  groups={groups}
                  groupBy={groupBy}
                  collapsedGroups={collapsedGroups}
                  onToggleGroup={toggleGroup}
                  displayProperties={displayProperties}
                  canManage={canManage}
                  pendingAction={pendingAction}
                  onOpen={openScheduleModal}
                  onRunNow={(schedule) => void handleRunNow(schedule)}
                  onToggle={(schedule, enabled) => void handleToggle(schedule, enabled)}
                  onEdit={(schedule) => openScheduleModal(schedule, true)}
                  onDelete={setDeleteTarget}
                />
              </>
            )}
          </div>
        </div>
      </div>

      <ScheduleDetailPanel
        key={`${openSchedule?.id ?? "none"}-${editMode ? "edit" : "view"}`}
        schedule={openSchedule}
        isOpen={!!openSchedule}
        initialEdit={editMode}
        onClose={closeScheduleModal}
        onDeleteRequest={setDeleteTarget}
        onToggle={(schedule, enabled) => void handleToggle(schedule, enabled)}
        onRunNow={(schedule) => void handleRunNow(schedule)}
        pendingAction={openSchedulePendingAction}
      />

      <AlertModalCore
        isOpen={!!deleteTarget}
        handleClose={() => setDeleteTarget(null)}
        handleSubmit={() => void handleDelete()}
        isSubmitting={deleting}
        title="Delete schedule"
        content={
          deleteTarget
            ? `"${deleteTarget.name}" will stop running and be hidden. Existing runs are kept.`
            : "This schedule will stop running and be hidden."
        }
        primaryButtonText={{ loading: "Deleting", default: "Delete" }}
      />
    </>
  );
});

function ScheduleListSkeleton() {
  return (
    <Skeleton className="mt-3 space-y-1" ariaLabel="Loading schedules">
      <Skeleton.Item height="32px" className="mb-2 w-40" />
      {["one", "two", "three", "four", "five"].map((key) => (
        <Skeleton.Item key={key} height="56px" className="rounded-none border-b border-subtle" />
      ))}
    </Skeleton>
  );
}
