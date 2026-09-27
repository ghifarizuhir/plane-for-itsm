/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import { v4 as uuidv4 } from "uuid";
import {
  AI_SCHEDULE_TOOLS,
  humanizeSchedule,
  isStructuredProposal,
  validateScheduleSpec,
  type TAiScheduleProposal,
  type TAiScheduleSpec,
  type TAiScheduleTool,
} from "@/lib/ai-schedule";

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
  const [stepKeys, setStepKeys] = useState<string[]>(() =>
    isStructuredProposal(proposal) ? proposal.how_to.map(() => uuidv4()) : []
  );
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
  const stepEntries = structured.how_to.map((step, index) => ({
    id: stepKeys[index] ?? uuidv4(),
    step,
    index,
  }));

  const updateStep = (index: number, value: string) => {
    const steps = [...structured.how_to];
    steps[index] = value;
    patch({ how_to: steps });
  };
  const removeStep = (index: number) => {
    patch({ how_to: structured.how_to.filter((_, stepIndex) => stepIndex !== index) });
    setStepKeys((keys) => keys.filter((_, keyIndex) => keyIndex !== index));
  };
  const addStep = () => {
    patch({ how_to: [...structured.how_to, ""] });
    setStepKeys((keys) => [...keys, uuidv4()]);
  };
  const toggleTool = (tool: TAiScheduleTool, checked: boolean) =>
    patch({
      tools: checked ? [...structured.tools, tool] : structured.tools.filter((current) => current !== tool),
    });

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

      <label className="mt-2 block text-12 text-secondary">
        Description
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={structured.description}
          onChange={(event) => patch({ description: event.target.value })}
        />
      </label>

      <div className="mt-2 text-12 text-secondary">
        Steps
        {stepEntries.map((entry) => (
          <div key={entry.id} className="mt-1 flex items-center gap-1">
            <input
              className="w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={entry.step}
              onChange={(event) => updateStep(entry.index, event.target.value)}
            />
            <Button
              size="sm"
              variant="ghost"
              disabled={structured.how_to.length === 1}
              onClick={() => removeStep(entry.index)}
            >
              Remove
            </Button>
          </div>
        ))}
        <Button size="sm" variant="ghost" onClick={addStep}>
          Add step
        </Button>
      </div>

      <div className="mt-2 text-12 text-secondary">
        Tools
        <div className="mt-1 flex flex-wrap gap-2">
          {AI_SCHEDULE_TOOLS.map((tool) => (
            <label key={tool} className="flex items-center gap-1 text-12 text-primary">
              <input
                type="checkbox"
                checked={structured.tools.includes(tool)}
                onChange={(event) => toggleTool(tool, event.target.checked)}
              />
              {tool}
            </label>
          ))}
        </div>
      </div>

      <label className="mt-2 block text-12 text-secondary">
        Expected output
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={structured.expected_output}
          onChange={(event) => patch({ expected_output: event.target.value })}
        />
      </label>

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
