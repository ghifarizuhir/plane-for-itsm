/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { Badge } from "@plane/propel/badge";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// lib
import { sanitizeAssistantHtml } from "@/lib/ai-context";
import type { TAiScheduleRun } from "@/lib/ai-schedule";

type Props = {
  runs: TAiScheduleRun[];
};

const STATUS_BADGE_VARIANTS: Record<TAiScheduleRun["status"], "success" | "danger" | "brand"> = {
  success: "success",
  failed: "danger",
  queued: "brand",
  running: "brand",
};

const runDurationInSeconds = (run: TAiScheduleRun): number | null => {
  if (!run.started_at || !run.finished_at) return null;
  const seconds = Math.round((Date.parse(run.finished_at) - Date.parse(run.started_at)) / 1000);
  return Number.isFinite(seconds) ? seconds : null;
};

export function ScheduleRunsList({ runs }: Props) {
  if (runs.length === 0) {
    return <p className="text-xs mt-2 text-tertiary">No runs yet.</p>;
  }

  return (
    <ul className="mt-2 space-y-2 border-t border-subtle pt-2">
      {runs.map((run) => {
        const duration = runDurationInSeconds(run);
        return (
          <li key={run.id} className="rounded-md border border-subtle bg-layer-2 p-2">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant={STATUS_BADGE_VARIANTS[run.status]} size="sm">
                {run.status}
              </Badge>
              <span className="text-xs text-secondary">{run.trigger === "manual" ? "Manual" : "Scheduled"}</span>
              <span className="text-xs text-tertiary">
                {renderFormattedDate(run.created_at)} at {renderFormattedTime(run.created_at)}
              </span>
              {duration !== null && <span className="text-xs text-tertiary">{duration}s</span>}
            </div>
            {run.status === "success" && run.response_html && (
              <div
                className="text-xs mt-1.5 leading-relaxed text-secondary"
                dangerouslySetInnerHTML={{ __html: sanitizeAssistantHtml(run.response_html) }}
              />
            )}
            {run.status === "failed" && run.error && (
              <p className="text-xs mt-1.5 break-words text-danger-primary">{run.error}</p>
            )}
            {run.status === "queued" && <p className="text-xs mt-1.5 text-tertiary">Waiting…</p>}
            {run.status === "running" && <p className="text-xs mt-1.5 text-tertiary">Running…</p>}
          </li>
        );
      })}
    </ul>
  );
}
