/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const WORK_ITEM_PRIORITIES = ["urgent", "high", "medium", "low", "none"] as const;
export type TWorkItemPriority = (typeof WORK_ITEM_PRIORITIES)[number];

export type TAiWorkItemProposal = {
  project: string;
  name: string;
  description?: string | null;
  priority?: string | null;
  state?: string | null;
  assignees?: string[] | null;
  labels?: string[] | null;
  start_date?: string | null;
  target_date?: string | null;
};

export type TAiWorkItemProposalEntry = {
  key: string;
  proposal: TAiWorkItemProposal;
};

export type TAiWorkItemDecision = {
  decision: "created" | "cancelled";
  created_work_item_id?: string;
  created_project_id?: string;
};

export const WORK_ITEM_LIMITS = {
  name: 255,
  description: 5000,
  refs: 10,
} as const;

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

/** True when the message starts with the `/task` slash command (case-insensitive). */
export const isWorkItemCommand = (text: string): boolean => /^\/task(?:\s|$)/i.test(text.trimStart());

export type TWorkItemRef = { projectIdentifier: string; sequenceId: string };

/** Split "LTS-42" into its project identifier and sequence id; null when malformed. */
export const parseWorkItemRef = (reference: string): TWorkItemRef | null => {
  const trimmed = reference.trim();
  const index = trimmed.lastIndexOf("-");
  if (index <= 0 || index === trimmed.length - 1) return null;
  const projectIdentifier = trimmed.slice(0, index);
  const sequenceId = trimmed.slice(index + 1);
  if (!/^[A-Za-z0-9]+$/.test(projectIdentifier) || !/^\d+$/.test(sequenceId)) return null;
  if (Number(sequenceId) <= 0) return null;
  return { projectIdentifier, sequenceId };
};

/** Mirrors `work_item_proposal_from_args` on the backend; returns the first error. */
export const validateWorkItemProposal = (proposal: Partial<TAiWorkItemProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Title is required.";
  if (name.length > WORK_ITEM_LIMITS.name) return `Title must be at most ${WORK_ITEM_LIMITS.name} characters.`;
  const description = proposal.description?.trim() ?? "";
  if (description.length > WORK_ITEM_LIMITS.description)
    return `Description must be at most ${WORK_ITEM_LIMITS.description} characters.`;
  if (proposal.priority && !WORK_ITEM_PRIORITIES.includes(proposal.priority as TWorkItemPriority))
    return "Unknown priority.";
  if ((proposal.assignees?.length ?? 0) > WORK_ITEM_LIMITS.refs)
    return `At most ${WORK_ITEM_LIMITS.refs} assignees are allowed.`;
  if ((proposal.labels?.length ?? 0) > WORK_ITEM_LIMITS.refs)
    return `At most ${WORK_ITEM_LIMITS.refs} labels are allowed.`;
  const start = proposal.start_date?.trim() ?? "";
  const target = proposal.target_date?.trim() ?? "";
  if (start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (target && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  return null;
};

/** Plain text → safe HTML: escape, newlines to <br/>, wrapped in a paragraph. */
export const textToDescriptionHtml = (text: string | null | undefined): string | null => {
  const trimmed = text?.trim() ?? "";
  if (!trimmed) return null;
  const escaped = trimmed.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  return `<p>${escaped.split("\n").join("<br/>")}</p>`;
};

export const workItemHref = (workspaceSlug: string, projectId: string, issueId: string): string =>
  `/${workspaceSlug}/projects/${projectId}/issues/${issueId}`;

type TProjectRef = { id: string; identifier: string; name: string };

export const matchProject = (projects: TProjectRef[], reference: string): TProjectRef | undefined => {
  const needle = reference.trim().toLowerCase();
  if (!needle) return undefined;
  const byIdentifier = projects.find((project) => project.identifier.toLowerCase() === needle);
  if (byIdentifier) return byIdentifier;
  const byName = projects.find((project) => project.name.toLowerCase() === needle);
  if (byName) return byName;
  const partial = projects.filter((project) => project.name.toLowerCase().includes(needle));
  return partial.length === 1 ? partial[0] : undefined;
};

type TMemberRef = { id: string; display_name?: string | null; email?: string | null };

export const matchAssignees = (
  members: TMemberRef[],
  references: string[]
): { matched: string[]; missing: string[] } => {
  const matched: string[] = [];
  const missing: string[] = [];
  references.forEach((reference) => {
    const needle = reference.trim().toLowerCase();
    if (!needle) return;
    const member = members.find(
      (candidate) => candidate.display_name?.toLowerCase() === needle || candidate.email?.toLowerCase() === needle
    );
    if (member) {
      if (!matched.includes(member.id)) matched.push(member.id);
    } else {
      missing.push(reference);
    }
  });
  return { matched, missing };
};

type TLabelRef = { id: string; name: string };

export const matchLabels = (labels: TLabelRef[], references: string[]): { matched: string[]; missing: string[] } => {
  const matched: string[] = [];
  const missing: string[] = [];
  references.forEach((reference) => {
    const needle = reference.trim().toLowerCase();
    if (!needle) return;
    const label = labels.find((candidate) => candidate.name.toLowerCase() === needle);
    if (label) {
      if (!matched.includes(label.id)) matched.push(label.id);
    } else {
      missing.push(reference);
    }
  });
  return { matched, missing };
};

type TStateRef = { id: string; name: string };

export const matchState = (states: TStateRef[], reference: string | null | undefined): TStateRef | undefined => {
  const needle = reference?.trim().toLowerCase();
  if (!needle) return undefined;
  return states.find((state) => state.name.toLowerCase() === needle);
};
