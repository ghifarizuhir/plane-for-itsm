/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Button } from "@plane/propel/button";
// hooks
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
// local imports
import { ScheduleItem } from "./schedule-item";

export const SchedulerView = observer(function SchedulerView() {
  // router
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  // store hooks
  const { schedules, loader, error, setWorkspace, fetchSchedules } = useAiSchedules();

  useEffect(() => {
    if (!slug) return;
    setWorkspace(slug);
    void fetchSchedules(slug).catch(() => {});
    const interval = window.setInterval(() => void fetchSchedules(slug).catch(() => {}), 30_000);
    return () => window.clearInterval(interval);
  }, [slug, setWorkspace, fetchSchedules]);

  const refresh = () => {
    if (!slug) return;
    void fetchSchedules(slug).catch(() => {});
  };

  return (
    <div className="mx-auto max-w-3xl space-y-3 px-4 py-6">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-lg font-semibold text-primary">AI Scheduler</h2>
        <Button size="sm" variant="secondary" loading={loader} onClick={refresh}>
          Refresh
        </Button>
      </div>
      {loader && schedules.length === 0 ? (
        <p className="text-sm text-tertiary">Loading schedules…</p>
      ) : error ? (
        <div className="flex flex-col items-start gap-2">
          <p className="text-sm text-danger-primary">{error}</p>
          <Button size="sm" variant="secondary" onClick={refresh}>
            Retry
          </Button>
        </div>
      ) : schedules.length === 0 ? (
        <p className="text-sm text-tertiary">
          No schedules yet. Create one from the AI chat by typing <code>/schedule …</code>
        </p>
      ) : (
        <div className="space-y-3">
          {schedules.map((schedule) => (
            <ScheduleItem key={schedule.id} schedule={schedule} />
          ))}
        </div>
      )}
    </div>
  );
});
