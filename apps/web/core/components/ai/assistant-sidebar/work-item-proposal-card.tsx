/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import type { TIssue } from "@plane/types";
// hooks
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import {
  matchAssignees,
  matchLabels,
  matchProject,
  matchState,
  textToDescriptionHtml,
  validateWorkItemProposal,
  workItemHref,
  WORK_ITEM_PRIORITIES,
  type TAiWorkItemDecision,
  type TAiWorkItemProposal,
} from "@/lib/ai-work-items";

type Props = {
  proposal: TAiWorkItemProposal;
  decision?: TAiWorkItemDecision;
  onConfirm: (payload: { projectId: string; issue: Partial<TIssue> }) => Promise<void>;
  onCancel: () => void;
};

export const WorkItemProposalCard = observer(function WorkItemProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  const { getProjectStates, fetchProjectStates } = useProjectState();
  const { project: projectMemberStore } = useMember();
  const { getProjectLabels, fetchProjectLabels } = useLabel();
  // component state
  const [draft, setDraft] = useState<TAiWorkItemProposal>(proposal);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [stateId, setStateId] = useState("");
  const [assigneeIds, setAssigneeIds] = useState<string[]>([]);
  const [labelIds, setLabelIds] = useState<string[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = useMemo(
    () =>
      (workspaceProjectIds ?? [])
        .map((id) => getProjectById(id))
        .filter((project): project is NonNullable<typeof project> => Boolean(project))
        .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name })),
    [workspaceProjectIds, getProjectById]
  );

  const resolvedProjectId = useMemo(() => matchProject(projects, draft.project)?.id ?? null, [projects, draft.project]);

  useEffect(() => {
    setProjectId(resolvedProjectId);
  }, [resolvedProjectId]);

  // Fetch project data that is not loaded yet. Fetch-only: no local state
  // writes, so it cannot loop.
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

  // Resolve the agent's human-readable values against the loaded project data.
  // Runs during render (React's adjust-state-during-render pattern): re-runs
  // when the project or proposal changes, or when a fetch lands (lengths in
  // the key change). User edits to the resolved ids never touch the key, so
  // they are never clobbered.
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

  if (decision?.decision === "created" && decision.created_work_item_id && decision.created_project_id) {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Work item created.{" "}
        {workspaceSlug && (
          <Link
            href={workItemHref(workspaceSlug, decision.created_project_id, decision.created_work_item_id)}
            className="text-accent-primary hover:underline"
          >
            Open work item
          </Link>
        )}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Work item cancelled.
      </p>
    );
  }

  const patch = (fields: Partial<TAiWorkItemProposal>) => setDraft((current) => ({ ...current, ...fields }));
  const validationError = projectId ? validateWorkItemProposal(draft) : "Project is required.";

  const toggleAssignee = (id: string, checked: boolean) =>
    setAssigneeIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));
  const toggleLabel = (id: string, checked: boolean) =>
    setLabelIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));

  const confirm = async () => {
    if (!projectId) return;
    setSubmitting(true);
    setError(null);
    try {
      const issue: Partial<TIssue> = { name: draft.name.trim() };
      const descriptionHtml = textToDescriptionHtml(draft.description);
      if (descriptionHtml) issue.description_html = descriptionHtml;
      if (draft.priority) issue.priority = draft.priority as TIssue["priority"];
      if (stateId) issue.state_id = stateId;
      if (assigneeIds.length > 0) issue.assignee_ids = assigneeIds;
      if (labelIds.length > 0) issue.label_ids = labelIds;
      if (draft.start_date) issue.start_date = draft.start_date;
      if (draft.target_date) issue.target_date = draft.target_date;
      await onConfirm({ projectId, issue });
    } catch {
      setError("Could not create the work item. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Work item proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <label className="text-12 text-secondary">
        Project
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={projectId ?? ""}
          onChange={(event) => setProjectId(event.target.value)}
        >
          <option value="">Select a project</option>
          {projects.map((project) => (
            <option key={project.id} value={project.id}>
              {project.identifier} — {project.name}
            </option>
          ))}
        </select>
      </label>

      <label className="mt-2 block text-12 text-secondary">
        Title
        <input
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={draft.name}
          onChange={(event) => patch({ name: event.target.value })}
        />
      </label>

      <label className="mt-2 block text-12 text-secondary">
        Description
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={3}
          value={draft.description ?? ""}
          onChange={(event) => patch({ description: event.target.value })}
        />
      </label>

      <label className="mt-2 block text-12 text-secondary">
        Priority
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

      <label className="mt-2 block text-12 text-secondary">
        State
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

      {members.length > 0 && (
        <div className="mt-2 text-12 text-secondary">
          Assignees
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

      {(labels ?? []).length > 0 && (
        <div className="mt-2 text-12 text-secondary">
          Labels
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

      <div className="mt-2 flex gap-2">
        <label className="flex-1 text-12 text-secondary">
          Start date
          <input
            type="date"
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.start_date ?? ""}
            onChange={(event) => patch({ start_date: event.target.value || null })}
          />
        </label>
        <label className="flex-1 text-12 text-secondary">
          Target date
          <input
            type="date"
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.target_date ?? ""}
            onChange={(event) => patch({ target_date: event.target.value || null })}
          />
        </label>
      </div>

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
});
