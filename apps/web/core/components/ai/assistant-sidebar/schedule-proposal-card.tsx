/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import {
  humanizeSchedule,
  isStructuredProposal,
  validateScheduleSpec,
  type TAiScheduleProposal,
  type TAiScheduleSpec,
} from "@/lib/ai-schedule";
import { ScheduleRecipeFields } from "./schedule-recipe-fields";

type Props = {
  proposal: TAiScheduleProposal;
  decision?: "pending" | "created" | "cancelled";
  onConfirm: (proposal: TAiScheduleProposal) => Promise<void>;
  onCancel: () => void;
};

export function ScheduleProposalCard({ proposal, decision, onConfirm, onCancel }: Props) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<TAiScheduleProposal>(proposal);
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const rawWorkspaceSlug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm(draft);
    } catch {
      setError("Could not create the schedule. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  if (decision === "created") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
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
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule cancelled.
      </p>
    );
  }
  if (decision !== "pending") return null;

  const patch = (fields: Partial<TAiScheduleSpec>) => setDraft((current) => ({ ...current, ...fields }));

  // Proposals stored before the recipe rollout have no spec fields: keep the
  // old read-only card and let the backend take the legacy path.
  if (!isStructuredProposal(draft)) {
    return (
      <div role="group" aria-label="Schedule proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
        <p className="text-12 font-semibold break-words text-primary">{draft.name}</p>
        <p className="mt-0.5 text-12 text-secondary">{humanizeSchedule(draft)}</p>
        <p className="mt-1 line-clamp-3 text-12 text-tertiary">{draft.prompt}</p>
        {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
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

  const structured = draft;
  const validationError = validateScheduleSpec(structured);

  return (
    <div role="group" aria-label="Schedule proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <label className="text-12 text-secondary">
        Name
        <input
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={structured.name}
          onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
        />
      </label>
      <p className="mt-1 text-12 text-secondary">{humanizeSchedule(structured)}</p>

      <ScheduleRecipeFields value={structured} onChange={patch} />

      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={Boolean(validationError)}
          onClick={() => void confirm()}
        >
          Confirm
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
