/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { Button } from "@plane/propel/button";
import { diffScheduleUpdate, type TAiScheduleUpdateProposal } from "@/lib/ai-schedule";

type Props = {
  proposal: TAiScheduleUpdateProposal;
  decision?: "pending" | "applied" | "cancelled";
  onConfirm: () => Promise<void>;
  onCancel: () => void;
};

export function ScheduleUpdateProposalCard({ proposal, decision, onConfirm, onCancel }: Props) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const rows = diffScheduleUpdate(proposal);
  const canApply = proposal.can_edit && Boolean(proposal.before);

  if (decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule updated.{" "}
        {slug && (
          <Link href={`/${slug}/scheduler/`} className="text-accent-primary hover:underline">
            Open Scheduler
          </Link>
        )}
      </p>
    );
  }
  if (decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule update cancelled.
      </p>
    );
  }
  if (decision !== "pending") return null;

  const apply = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm();
    } catch {
      setError("Could not update the schedule. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Schedule update proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-semibold break-words text-primary">{proposal.before?.name ?? "Unknown schedule"}</p>
      {!proposal.before && <p className="mt-1 text-12 text-danger-primary">This schedule no longer exists.</p>}
      {proposal.before && !proposal.can_edit && (
        <p className="mt-1 text-12 text-danger-primary">
          Only the schedule creator or a workspace admin can edit this schedule.
        </p>
      )}
      {rows.length > 0 ? (
        <div className="mt-2 space-y-1.5">
          {rows.map((row) => (
            <div key={row.label} className="text-12">
              <p className="font-medium text-secondary">{row.label}</p>
              <p className="whitespace-pre-wrap text-tertiary line-through">{row.before}</p>
              <p className="whitespace-pre-wrap text-primary">{row.after}</p>
            </div>
          ))}
        </div>
      ) : (
        <p className="mt-1 text-12 text-tertiary">No visible changes.</p>
      )}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button size="sm" variant="primary" loading={submitting} disabled={!canApply} onClick={() => void apply()}>
          Apply
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
