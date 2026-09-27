/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { SearchOutline } from "@makeplane/propel/icons";
import { useParams, useRouter, useSearchParams } from "next/navigation";
// plane imports
import { Button } from "@plane/propel/button";
// hooks
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useQueryParams } from "@/hooks/use-query-params";
// lib
import { filterSchedules, type TAiScheduleListFilter } from "@/lib/ai-schedule";
// local imports
import { ScheduleDetailModal } from "./schedule-detail-modal";
import { ScheduleItem } from "./schedule-item";

const STATUS_FILTERS: { key: TAiScheduleListFilter["status"]; label: string }[] = [
  { key: "all", label: "All" },
  { key: "active", label: "Active" },
  { key: "paused", label: "Paused" },
];

export const SchedulerView = observer(function SchedulerView() {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const router = useRouter();
  const searchParams = useSearchParams();
  const { updateQueryParams } = useQueryParams();
  // store hooks
  const { schedules, runsBySchedule, loader, error, setWorkspace, fetchSchedules, fetchRuns } = useAiSchedules();
  // local state
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<TAiScheduleListFilter["status"]>("all");
  const [openScheduleId, setOpenScheduleId] = useState<string | null>(null);

  const requestedScheduleId = searchParams.get("schedule");
  const visibleSchedules = useMemo(() => filterSchedules(schedules, { query, status }), [schedules, query, status]);
  const openSchedule = schedules.find((schedule) => schedule.id === openScheduleId) ?? null;

  const expandedRunsRef = useRef(runsBySchedule);
  useEffect(() => {
    expandedRunsRef.current = runsBySchedule;
  }, [runsBySchedule]);

  const refreshSchedules = useCallback(() => {
    if (!slug) return;
    void fetchSchedules(slug).catch(() => {});
    for (const scheduleId of Object.keys(expandedRunsRef.current)) void fetchRuns(slug, scheduleId).catch(() => {});
  }, [slug, fetchSchedules, fetchRuns]);

  useEffect(() => {
    if (!slug) return;
    setWorkspace(slug);
    refreshSchedules();
    const interval = window.setInterval(refreshSchedules, 30_000);
    return () => window.clearInterval(interval);
  }, [slug, setWorkspace, refreshSchedules]);

  useEffect(() => {
    if (!requestedScheduleId) {
      setOpenScheduleId(null);
      return;
    }
    if (schedules.some((schedule) => schedule.id === requestedScheduleId)) setOpenScheduleId(requestedScheduleId);
  }, [requestedScheduleId, schedules]);

  const openScheduleModal = (scheduleId: string) => {
    setOpenScheduleId(scheduleId);
    router.replace(updateQueryParams({ paramsToAdd: { schedule: scheduleId } }));
  };

  const closeScheduleModal = () => {
    setOpenScheduleId(null);
    if (requestedScheduleId) router.replace(updateQueryParams({ paramsToRemove: ["schedule"] }));
  };

  const showErrorState = !!error && schedules.length === 0;

  return (
    <div className="mx-auto max-w-3xl space-y-3 px-4 py-6">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-lg font-semibold text-primary">AI Scheduler</h2>
        <Button size="sm" variant="secondary" loading={loader} onClick={refreshSchedules}>
          Refresh
        </Button>
      </div>
      {schedules.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-1.5 rounded-md border border-subtle bg-surface-1 px-2.5 py-1.5">
            <SearchOutline className="h-3.5 w-3.5 text-placeholder" />
            <input
              className="w-full max-w-[234px] border-none bg-transparent text-body-xs-regular outline-none placeholder:text-placeholder"
              placeholder="Search schedules..."
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          {STATUS_FILTERS.map((filter) => (
            <Button
              key={filter.key}
              size="sm"
              variant={status === filter.key ? "secondary" : "ghost"}
              onClick={() => setStatus(filter.key)}
            >
              {filter.label}
            </Button>
          ))}
          <span className="text-xs text-tertiary">
            {visibleSchedules.length} of {schedules.length}
          </span>
        </div>
      )}
      {loader && schedules.length === 0 ? (
        <p className="text-sm text-tertiary">Loading schedules…</p>
      ) : showErrorState ? (
        <div className="flex flex-col items-start gap-2">
          <p className="text-sm text-danger-primary">{error}</p>
          <Button size="sm" variant="secondary" onClick={refreshSchedules}>
            Retry
          </Button>
        </div>
      ) : schedules.length === 0 ? (
        <p className="text-sm text-tertiary">
          No schedules yet. Create one from the AI chat by typing <code>/schedule …</code>
        </p>
      ) : visibleSchedules.length === 0 ? (
        <p className="text-sm text-tertiary">No schedules match your search.</p>
      ) : (
        <div className="space-y-3">
          {error && <p className="text-xs text-danger-primary">Couldn't refresh — retrying automatically.</p>}
          {visibleSchedules.map((schedule) => (
            <ScheduleItem key={schedule.id} schedule={schedule} onOpen={openScheduleModal} />
          ))}
        </div>
      )}
      <ScheduleDetailModal schedule={openSchedule} isOpen={!!openSchedule} onClose={closeScheduleModal} />
    </div>
  );
});
