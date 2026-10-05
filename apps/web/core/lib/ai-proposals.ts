/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TIssue } from "@plane/types";
import { WORK_ITEM_PRIORITIES, type TWorkItemPriority } from "@/lib/ai-work-items";

export const PROPOSAL_KINDS = ["update_work_item", "add_comment"] as const;
export type TAiProposalKind = (typeof PROPOSAL_KINDS)[number];

export const PROPOSAL_LIMITS = {
  comment: 5000,
  name: 255,
  description: 5000,
  refs: 10,
} as const;

export type TAiWorkItemChanges = {
  name?: string | null;
  description?: string | null;
  priority?: string | null;
  state?: string | null;
  assignees?: string[] | null;
  labels?: string[] | null;
  start_date?: string | null;
  target_date?: string | null;
};

export type TAiUpdateWorkItemProposal = {
  work_item: string;
  changes: TAiWorkItemChanges;
};

export type TAiAddCommentProposal = {
  work_item: string;
  comment: string;
};

export type TAiProposal =
  | { key: string; kind: "update_work_item"; proposal: TAiUpdateWorkItemProposal }
  | { key: string; kind: "add_comment"; proposal: TAiAddCommentProposal };

export type TAiProposalDecisionResult = {
  created_comment_id?: string;
};

export type TAiProposalDecision = {
  kind: TAiProposalKind;
  decision: "applied" | "cancelled";
  result?: TAiProposalDecisionResult;
};

export type TAiProposalConfirmPayload =
  | { kind: "update_work_item"; projectId: string; issueId: string; changes: Partial<TIssue> }
  | { kind: "add_comment"; projectId: string; issueId: string; commentHtml: string };

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

/** Count fields explicitly set (null/undefined means "not changed"). */
export const changedFieldCount = (changes: TAiWorkItemChanges): number =>
  Object.values(changes).filter((value) => value !== null && value !== undefined).length;

/** Mirrors `update_work_item_proposal_from_args` on the backend. */
export const validateUpdateWorkItemProposal = (proposal: Partial<TAiUpdateWorkItemProposal>): string | null => {
  if (!proposal.work_item?.trim()) return "Work item is required.";
  const changes = proposal.changes ?? {};
  if (changedFieldCount(changes) === 0) return "At least one change is required.";
  const name = changes.name?.trim() ?? "";
  if (changes.name != null && !name) return "Title must not be empty.";
  if (name.length > PROPOSAL_LIMITS.name) return `Title must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (changes.priority && !WORK_ITEM_PRIORITIES.includes(changes.priority as TWorkItemPriority))
    return "Unknown priority.";
  if ((changes.assignees?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} assignees are allowed.`;
  if ((changes.labels?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} labels are allowed.`;
  const start = changes.start_date?.trim() ?? "";
  const target = changes.target_date?.trim() ?? "";
  if (changes.start_date != null && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (changes.target_date != null && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  return null;
};

/** Mirrors `add_comment_proposal_from_args` on the backend. */
export const validateAddCommentProposal = (proposal: Partial<TAiAddCommentProposal>): string | null => {
  if (!proposal.work_item?.trim()) return "Work item is required.";
  const comment = proposal.comment?.trim() ?? "";
  if (!comment) return "Comment is required.";
  if (comment.length > PROPOSAL_LIMITS.comment) return `Comment must be at most ${PROPOSAL_LIMITS.comment} characters.`;
  return null;
};
