/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import type { TIssue } from "@plane/types";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import { stripHtml } from "@/lib/ai-context";
import {
  matchAssignees,
  matchLabels,
  matchState,
  parseWorkItemRef,
  textToDescriptionHtml,
  workItemHref,
  WORK_ITEM_PRIORITIES,
} from "@/lib/ai-work-items";
import {
  validateUpdateWorkItemProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiUpdateWorkItemProposal,
  type TAiWorkItemChanges,
} from "@/lib/ai-proposals";

type Props = {
  proposal: TAiUpdateWorkItemProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const WorkItemUpdateProposalCard = observer(function WorkItemUpdateProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  // store hooks
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { getProjectStates, fetchProjectStates, getStateById } = useProjectState();
  const { project: projectMemberStore } = useMember();
  const { getProjectLabels, fetchProjectLabels } = useLabel();
  // component state
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
  const [draft, setDraft] = useState<TAiWorkItemChanges>(proposal.changes);
  const [stateId, setStateId] = useState("");
  const [assigneeIds, setAssigneeIds] = useState<string[]>([]);
  const [labelIds, setLabelIds] = useState<string[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceSlug) return;
    const ref = parseWorkItemRef(proposal.work_item);
    if (!ref) {
      setResolveError("Work item reference is invalid.");
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const fetched = await fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId);
        if (!cancelled) setIssue(fetched);
      } catch {
        if (!cancelled) setResolveError("Work item not found or not accessible.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, proposal.work_item, fetchIssueWithIdentifier]);

  const projectId = issue?.project_id ?? null;

  useEffect(() => {
    if (!projectId || !workspaceSlug) return;
    if (!getProjectStates(projectId)) void fetchProjectStates(workspaceSlug, projectId);
    if (!projectMemberStore.getProjectMemberFetchStatus(projectId))
      void projectMemberStore.fetchProjectMembers(workspaceSlug, projectId);
    if (!getProjectLabels(projectId)) void fetchProjectLabels(workspaceSlug, projectId);
  }, [
    projectId,
    workspaceSlug,
    getProjectStates,
    fetchProjectStates,
    projectMemberStore,
    getProjectLabels,
    fetchProjectLabels,
  ]);

  const states = projectId ? getProjectStates(projectId) : undefined;
  const labels = projectId ? getProjectLabels(projectId) : undefined;
  const memberIds = projectId ? (projectMemberStore.getProjectMemberIds(projectId, true) ?? []) : [];
  const members = memberIds
    .map((id) => ({ id, details: projectMemberStore.getProjectMemberDetails(id, projectId ?? "") }))
    .filter((entry) => Boolean(entry.details))
    .map((entry) => ({
      id: entry.id,
      display_name: entry.details?.member.display_name,
      email: entry.details?.member.email,
    }));

  // Resolve the agent's human-readable values against the loaded project data
  // (same adjust-state-during-render pattern as WorkItemProposalCard).
  const resolutionKey = [
    projectId ?? "",
    states?.length ?? -1,
    labels?.length ?? -1,
    members.length,
    draft.state ?? "",
    (draft.assignees ?? []).join("|"),
    (draft.labels ?? []).join("|"),
  ].join("::");
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    if (!projectId) {
      setStateId("");
      setAssigneeIds([]);
      setLabelIds([]);
      setMissing([]);
    } else {
      setStateId(matchState(states ?? [], draft.state)?.id ?? "");
      const assigneeResult = matchAssignees(members, draft.assignees ?? []);
      setAssigneeIds(assigneeResult.matched);
      const labelResult = matchLabels(labels ?? [], draft.labels ?? []);
      setLabelIds(labelResult.matched);
      setMissing([
        ...assigneeResult.missing.map((name) => `Assignee not found: ${name}`),
        ...labelResult.missing.map((name) => `Label not found: ${name}`),
      ]);
    }
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Work item updated.{" "}
        {workspaceSlug && projectId && issue && (
          <Link href={workItemHref(workspaceSlug, projectId, issue.id)} className="text-accent-primary hover:underline">
            Open work item
          </Link>
        )}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Update cancelled.
      </p>
    );
  }

  const has = (field: keyof TAiWorkItemChanges) =>
    proposal.changes[field] !== null && proposal.changes[field] !== undefined;
  const patch = (fields: Partial<TAiWorkItemChanges>) => setDraft((current) => ({ ...current, ...fields }));
  const toggleAssignee = (id: string, checked: boolean) =>
    setAssigneeIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));
  const toggleLabel = (id: string, checked: boolean) =>
    setLabelIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));

  const validationError = issue
    ? validateUpdateWorkItemProposal({ work_item: proposal.work_item, changes: draft })
    : null;
  const oldStateName = issue ? getStateById(issue.state_id)?.name : undefined;
  const oldAssigneeNames = (issue?.assignee_ids ?? [])
    .map((id) => members.find((member) => member.id === id))
    .map((member) => member?.display_name || member?.email)
    .filter((name): name is string => Boolean(name));
  const oldLabelNames = (issue?.label_ids ?? [])
    .map((id) => labels?.find((label) => label.id === id)?.name)
    .filter((name): name is string => Boolean(name));

  const confirm = async () => {
    if (!issue || !projectId) return;
    setSubmitting(true);
    setError(null);
    try {
      const changes: Partial<TIssue> = {};
      if (has("name") && draft.name) changes.name = draft.name.trim();
      if (has("description")) {
        const descriptionHtml = textToDescriptionHtml(draft.description);
        if (descriptionHtml) changes.description_html = descriptionHtml;
      }
      if (has("priority") && draft.priority) changes.priority = draft.priority as TIssue["priority"];
      if (has("state") && stateId) changes.state_id = stateId;
      if (has("assignees")) changes.assignee_ids = assigneeIds;
      if (has("labels")) changes.label_ids = labelIds;
      if (has("start_date") && draft.start_date) changes.start_date = draft.start_date;
      if (has("target_date") && draft.target_date) changes.target_date = draft.target_date;
      await onConfirm({
        kind: "update_work_item",
        projectId,
        issueId: issue.id,
        changes,
      });
    } catch {
      setError("Could not update the work item. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Work item update proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Update {proposal.work_item}</p>
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}

      {has("name") && (
        <label className="mt-2 block text-12 text-secondary">
          Title <span className="text-tertiary">(was: {issue?.name ?? "—"})</span>
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.name ?? ""}
            onChange={(event) => patch({ name: event.target.value })}
          />
        </label>
      )}

      {has("description") && (
        <label className="mt-2 block text-12 text-secondary">
          Description{" "}
          <span className="text-tertiary">(was: {stripHtml(issue?.description_html ?? "").slice(0, 80) || "—"})</span>
          <textarea
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            rows={3}
            value={draft.description ?? ""}
            onChange={(event) => patch({ description: event.target.value })}
          />
        </label>
      )}

      {has("priority") && (
        <label className="mt-2 block text-12 text-secondary">
          Priority <span className="text-tertiary">(was: {issue?.priority ?? "none"})</span>
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.priority ?? ""}
            onChange={(event) => patch({ priority: event.target.value || null })}
          >
            <option value="">No priority</option>
            {WORK_ITEM_PRIORITIES.map((priority) => (
              <option key={priority} value={priority}>
                {priority}
              </option>
            ))}
          </select>
        </label>
      )}

      {has("state") && (
        <label className="mt-2 block text-12 text-secondary">
          State <span className="text-tertiary">(was: {oldStateName ?? "—"})</span>
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={stateId}
            onChange={(event) => setStateId(event.target.value)}
          >
            <option value="">Project default</option>
            {(states ?? []).map((state) => (
              <option key={state.id} value={state.id}>
                {state.name}
              </option>
            ))}
          </select>
        </label>
      )}

      {has("assignees") && (
        <div className="mt-2 text-12 text-secondary">
          Assignees <span className="text-tertiary">(was: {oldAssigneeNames.join(", ") || "none"})</span>
          <div className="mt-1 flex max-h-24 flex-col gap-1 overflow-y-auto">
            {members.map((member) => (
              <label key={member.id} className="flex items-center gap-1 text-12 text-primary">
                <input
                  type="checkbox"
                  checked={assigneeIds.includes(member.id)}
                  onChange={(event) => toggleAssignee(member.id, event.target.checked)}
                />
                {member.display_name || member.email || member.id}
              </label>
            ))}
          </div>
        </div>
      )}

      {has("labels") && (labels ?? []).length > 0 && (
        <div className="mt-2 text-12 text-secondary">
          Labels <span className="text-tertiary">(was: {oldLabelNames.join(", ") || "none"})</span>
          <div className="mt-1 flex flex-wrap gap-2">
            {(labels ?? []).map((label) => (
              <label key={label.id} className="flex items-center gap-1 text-12 text-primary">
                <input
                  type="checkbox"
                  checked={labelIds.includes(label.id)}
                  onChange={(event) => toggleLabel(label.id, event.target.checked)}
                />
                {label.name}
              </label>
            ))}
          </div>
        </div>
      )}

      {(has("start_date") || has("target_date")) && (
        <div className="mt-2 flex gap-2">
          {has("start_date") && (
            <label className="flex-1 text-12 text-secondary">
              Start date <span className="text-tertiary">(was: {issue?.start_date ?? "—"})</span>
              <input
                type="date"
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={draft.start_date ?? ""}
                onChange={(event) => patch({ start_date: event.target.value || null })}
              />
            </label>
          )}
          {has("target_date") && (
            <label className="flex-1 text-12 text-secondary">
              Target date <span className="text-tertiary">(was: {issue?.target_date ?? "—"})</span>
              <input
                type="date"
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={draft.target_date ?? ""}
                onChange={(event) => patch({ target_date: event.target.value || null })}
              />
            </label>
          )}
        </div>
      )}

      {missing.map((notice) => (
        <p key={notice} className="mt-1 text-12 text-tertiary">
          {notice} — pick one manually.
        </p>
      ))}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || Boolean(validationError)}
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
