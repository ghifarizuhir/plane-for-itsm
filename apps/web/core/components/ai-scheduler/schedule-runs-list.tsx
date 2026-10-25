/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { Badge } from "@plane/propel/badge";
import { cn, renderFormattedDate, renderFormattedTime } from "@plane/utils";
import { sanitizeAssistantHtml } from "@/lib/ai-context";
import {
  RUN_STATUS_BADGE_VARIANTS,
  runDurationInSeconds,
  scheduleStatusLabel,
  type TAiScheduleRun,
} from "@/lib/ai-schedule";

type Props = {
  runs: TAiScheduleRun[];
  defaultExpanded?: boolean;
  emptyMessage?: string;
};

export function ScheduleRunsList({ runs, defaultExpanded = false, emptyMessage = "No runs yet." }: Props) {
  if (runs.length === 0) {
    return <p className="mt-2 text-12 text-tertiary">{emptyMessage}</p>;
  }

  return (
    <ul className="mt-2 space-y-0">
      {runs.map((run, index) => (
        <ScheduleRunRow
          key={run.id}
          run={run}
          defaultExpanded={defaultExpanded}
          showConnector={index < runs.length - 1}
        />
      ))}
    </ul>
  );
}

function ScheduleRunRow({
  run,
  defaultExpanded,
  showConnector,
}: {
  run: TAiScheduleRun;
  defaultExpanded: boolean;
  showConnector: boolean;
}) {
  const duration = runDurationInSeconds(run);
  const [expanded, setExpanded] = useState(defaultExpanded);
  const hasOutput = run.status === "success" && Boolean(run.response_html || run.response);
  const hasError = run.status === "failed" && Boolean(run.error);

  return (
    <li className="relative flex gap-3 pb-3 last:pb-0">
      {showConnector && <span className="bg-subtle absolute top-4 bottom-0 left-[5px] w-px" aria-hidden="true" />}
      <span
        className={cn("ring-surface-1 relative z-[1] mt-1.5 size-2.5 flex-shrink-0 rounded-full ring-2", {
          "bg-success-primary": run.status === "success",
          "bg-danger-primary": run.status === "failed",
          "bg-accent-primary": run.status === "queued" || run.status === "running",
        })}
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={RUN_STATUS_BADGE_VARIANTS[run.status]} size="sm">
            {scheduleStatusLabel(run.status) ?? run.status}
          </Badge>
          <span className="text-11 text-secondary">{run.trigger === "manual" ? "Manual" : "Scheduled"}</span>
          <span className="text-11 text-tertiary">
            {renderFormattedDate(run.created_at)} at {renderFormattedTime(run.created_at)}
          </span>
          {duration !== null && <span className="text-11 text-tertiary">{duration}s</span>}
          {hasOutput && (
            <button
              type="button"
              onClick={() => setExpanded((value) => !value)}
              className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-11 font-medium text-secondary transition-colors hover:bg-layer-1 hover:text-primary"
              aria-expanded={expanded}
            >
              {expanded ? "Hide output" : "Show output"}
              <ChevronDownOutline className={cn("size-3 transition-transform", expanded && "rotate-180")} />
            </button>
          )}
        </div>
        {run.status === "queued" && <p className="mt-1.5 text-11 text-tertiary">Waiting for a worker…</p>}
        {run.status === "running" && <p className="mt-1.5 text-11 text-tertiary">Running…</p>}
        {hasError && <p className="mt-1.5 text-12 break-words whitespace-pre-wrap text-danger-primary">{run.error}</p>}
        {hasOutput && expanded && (
          <div
            className="ai-prose mt-2 rounded-md border border-subtle bg-layer-1 p-3 text-12 break-words text-secondary"
            dangerouslySetInnerHTML={{
              __html: run.response_html
                ? sanitizeAssistantHtml(run.response_html)
                : sanitizeAssistantHtml(run.response ?? ""),
            }}
          />
        )}
      </div>
    </li>
  );
}
