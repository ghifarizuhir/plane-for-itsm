/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import { humanizeSchedule, type TAiScheduleProposal } from "@/lib/ai-schedule";

type Props = {
  proposal: TAiScheduleProposal;
  decision?: "pending" | "created" | "cancelled";
  onConfirm: () => Promise<void>;
  onCancel: () => void;
};

export function ScheduleProposalCard({ proposal, decision, onConfirm, onCancel }: Props) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const rawWorkspaceSlug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm();
    } catch {
      setError("Could not create the schedule. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  if (decision === "created") {
    return (
      <p role="status" className="text-xs mt-1.5 text-tertiary">
        Schedule created.{" "}
        {rawWorkspaceSlug && (
          <Link href={`/${rawWorkspaceSlug}/scheduler/`} className="text-accent-primary hover:underline">
            Open Scheduler
          </Link>
        )}
      </p>
    );
  }
  if (decision === "cancelled") {
    return (
      <p role="status" className="text-xs mt-1.5 text-tertiary">
        Schedule cancelled.
      </p>
    );
  }
  if (decision !== "pending") return null;

  return (
    <div role="group" aria-label="Schedule proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-xs font-semibold break-words text-primary">{proposal.name}</p>
      <p className="text-xs mt-0.5 text-secondary">{humanizeSchedule(proposal)}</p>
      <p className="text-xs mt-1 line-clamp-3 text-tertiary">{proposal.prompt}</p>
      {error && <p className="text-xs mt-1 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button size="sm" variant="primary" loading={submitting} onClick={() => void confirm()}>
          Confirm
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
