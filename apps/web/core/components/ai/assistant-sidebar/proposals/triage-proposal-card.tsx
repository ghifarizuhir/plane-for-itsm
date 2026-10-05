/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import type { TIssue } from "@plane/types";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useProjectInbox } from "@/hooks/store/use-project-inbox";
// lib
import { parseWorkItemRef } from "@/lib/ai-work-items";
import {
  TRIAGE_FIELD_VALUES,
  validateApplyTriageSuggestionProposal,
  validateTriageIntakeItemProposal,
  type TAiApplyTriageSuggestionProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiTriageIntakeItemProposal,
  type TTriageAction,
  type TTriageField,
} from "@/lib/ai-proposals";

type TriageProposalEntry =
  | { kind: "apply_triage_suggestion"; proposal: TAiApplyTriageSuggestionProposal }
  | { kind: "triage_intake_item"; proposal: TAiTriageIntakeItemProposal };

type Props = TriageProposalEntry & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const TriageProposalCard = (props: Props) => {
  if (props.kind === "apply_triage_suggestion") return <ApplyTriageCard {...props} />;
  return <TriageActionCard {...props} />;
};

const ACTION_LABELS: Record<TTriageAction, string> = {
  accept: "Accept",
  reject: "Reject",
  snooze: "Snooze",
  duplicate: "Duplicate",
};

const useIntakeResolution = (intakeItem: string) => {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { fetchInboxIssueById, getIssueInboxByIssueId } = useProjectInbox();
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceSlug) return;
    const ref = parseWorkItemRef(intakeItem);
    if (!ref) {
      setResolveError("Intake item reference is invalid.");
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const fetched = await fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId);
        if (!cancelled) setIssue(fetched);
      } catch {
        if (!cancelled) setResolveError("Intake item not found or not accessible.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, intakeItem, fetchIssueWithIdentifier]);

  useEffect(() => {
    if (!workspaceSlug || !issue || !issue.project_id) return;
    void fetchInboxIssueById(workspaceSlug, issue.project_id, issue.id).catch(() => undefined);
  }, [workspaceSlug, issue, fetchInboxIssueById]);

  const inboxStore = issue ? getIssueInboxByIssueId(issue.id) : undefined;
  useEffect(() => {
    if (inboxStore && !inboxStore.triageSuggestionFetched) void inboxStore.fetchTriageSuggestion();
  }, [inboxStore]);

  return { workspaceSlug, issue, inboxStore, resolveError };
};

type ApplyProps = Extract<TriageProposalEntry, { kind: "apply_triage_suggestion" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const ApplyTriageCard = observer(function ApplyTriageCard({ proposal, decision, onConfirm, onCancel }: ApplyProps) {
  const { issue, inboxStore, resolveError } = useIntakeResolution(proposal.intake_item);
  const [selectedFields, setSelectedFields] = useState<TTriageField[]>([]);
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const suggestion = inboxStore?.triageSuggestion ?? null;
  const availableFields = TRIAGE_FIELD_VALUES.filter((field) => {
    if (!suggestion) return false;
    if (field === "category") return Boolean(suggestion.category);
    if (field === "service") return Boolean(suggestion.service);
    return Boolean(suggestion.severity);
  });
  const resolutionKey = [issue?.id ?? "", suggestion?.id ?? "", availableFields.join("|")].join("::");
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    setSelectedFields(
      availableFields.filter(
        (field) => !suggestion?.applied_fields.includes(field) && !suggestion?.dismissed_fields.includes(field)
      )
    );
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Triage suggestion applied.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Triage suggestion cancelled.
      </p>
    );
  }

  const validationError = validateApplyTriageSuggestionProposal({
    intake_item: proposal.intake_item,
    fields: selectedFields,
  });
  const confirm = async () => {
    if (!issue || !issue.project_id || selectedFields.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "apply_triage_suggestion",
        projectId: issue.project_id,
        issueId: issue.id,
        fields: selectedFields,
      });
    } catch {
      setError("Could not apply the suggestion. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Triage suggestion proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Apply triage suggestion for {proposal.intake_item}</p>
      {issue && <p className="mt-0.5 text-12 text-tertiary">{issue.name}</p>}
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      {!suggestion && !resolveError && <p className="mt-1 text-12 text-tertiary">No ready suggestion found.</p>}
      {suggestion && (
        <div className="mt-2 flex flex-col gap-1">
          {TRIAGE_FIELD_VALUES.map((field) => {
            const available = availableFields.includes(field);
            const applied = suggestion.applied_fields.includes(field);
            const dismissed = suggestion.dismissed_fields.includes(field);
            const label =
              field === "category"
                ? suggestion.category?.label
                : field === "service"
                  ? suggestion.service?.label
                  : suggestion.severity?.priority;
            return (
              <label key={field} className="flex items-center gap-1 text-12 text-primary">
                <input
                  type="checkbox"
                  checked={selectedFields.includes(field)}
                  disabled={!available || applied || dismissed}
                  onChange={(event) =>
                    setSelectedFields((current) =>
                      event.target.checked ? [...current, field] : current.filter((value) => value !== field)
                    )
                  }
                />
                {field}: {label ?? "—"}
                {applied && <span className="text-tertiary"> (already applied)</span>}
                {dismissed && <span className="text-tertiary"> (dismissed)</span>}
                {!available && !applied && !dismissed && <span className="text-tertiary"> (unavailable)</span>}
              </label>
            );
          })}
        </div>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || !issue.project_id || selectedFields.length === 0 || Boolean(validationError)}
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
});

type ActionProps = Extract<TriageProposalEntry, { kind: "triage_intake_item" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const TriageActionCard = observer(function TriageActionCard({ proposal, decision, onConfirm, onCancel }: ActionProps) {
  const { workspaceSlug, issue, resolveError } = useIntakeResolution(proposal.intake_item);
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const [action, setAction] = useState<TTriageAction>(proposal.action);
  const [snoozedTill, setSnoozedTill] = useState(proposal.snoozed_till ?? "");
  const [duplicateOf, setDuplicateOf] = useState(proposal.duplicate_of ?? "");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Intake item{" "}
        {proposal.action === "accept"
          ? "accepted"
          : proposal.action === "reject"
            ? "rejected"
            : proposal.action === "snooze"
              ? "snoozed"
              : "marked as duplicate"}
        .
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Triage action cancelled.
      </p>
    );
  }

  const draft = {
    intake_item: proposal.intake_item,
    action,
    ...(action === "snooze" ? { snoozed_till: snoozedTill } : {}),
    ...(action === "duplicate" ? { duplicate_of: duplicateOf } : {}),
  };
  const validationError = validateTriageIntakeItemProposal(draft);

  const confirm = async () => {
    if (!issue || !issue.project_id || !workspaceSlug) return;
    setSubmitting(true);
    setError(null);
    try {
      let duplicateToIssueId: string | undefined;
      if (action === "duplicate") {
        const ref = parseWorkItemRef(duplicateOf);
        if (!ref) {
          setError("Duplicate target must look like PROJ-123.");
          return;
        }
        const target = await fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId);
        duplicateToIssueId = target.id;
      }
      await onConfirm({
        kind: "triage_intake_item",
        projectId: issue.project_id,
        issueId: issue.id,
        action,
        ...(action === "snooze" ? { snoozedTill: new Date(snoozedTill).toISOString() } : {}),
        ...(duplicateToIssueId ? { duplicateToIssueId } : {}),
      });
    } catch {
      setError("Could not triage the intake item. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Intake triage proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Triage {proposal.intake_item}</p>
      {issue && <p className="mt-0.5 text-12 text-tertiary">{issue.name}</p>}
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      <label className="mt-2 block text-12 text-secondary">
        Action
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={action}
          onChange={(event) => setAction(event.target.value as TTriageAction)}
        >
          {(["accept", "reject", "snooze", "duplicate"] as const).map((value) => (
            <option key={value} value={value}>
              {ACTION_LABELS[value]}
            </option>
          ))}
        </select>
      </label>
      {action === "snooze" && (
        <label className="mt-2 block text-12 text-secondary">
          Snooze until
          <input
            type="datetime-local"
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={snoozedTill}
            onChange={(event) => setSnoozedTill(event.target.value)}
          />
        </label>
      )}
      {action === "duplicate" && (
        <label className="mt-2 block text-12 text-secondary">
          Duplicate of
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            placeholder="LTS-42"
            value={duplicateOf}
            onChange={(event) => setDuplicateOf(event.target.value)}
          />
        </label>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || !issue.project_id || Boolean(validationError)}
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
});
