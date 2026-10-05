/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ICycle, IModule, IService, TIssue } from "@plane/types";
import { WORK_ITEM_PRIORITIES, type TWorkItemPriority } from "@/lib/ai-work-items";

export const PROPOSAL_KINDS = [
  "update_work_item",
  "add_comment",
  "manage_service_links",
  "manage_sprint_items",
  "manage_track_items",
  "create_service",
  "update_service",
  "create_sprint",
  "update_sprint",
  "create_track",
  "update_track",
  "create_article",
  "update_article",
] as const;
export type TAiProposalKind = (typeof PROPOSAL_KINDS)[number];

export const PROPOSAL_LIMITS = {
  comment: 5000,
  name: 255,
  description: 5000,
  refs: 10,
  services: 10,
  items: 25,
  url: 2048,
  articleContent: 20000,
} as const;

export const SERVICE_STATUS_VALUES = ["active", "planned", "maintenance", "deprecated", "retired"] as const;
export const SERVICE_CRITICALITY_VALUES = ["critical", "high", "medium", "low"] as const;
export const SERVICE_TYPE_VALUES = ["internal", "external", "infrastructure", "third_party"] as const;
export const MODULE_STATUS_VALUES = ["backlog", "planned", "in-progress", "paused", "completed", "cancelled"] as const;
export const ARTICLE_ACCESS_VALUES = ["public", "private"] as const;
export const ARTICLE_ACTION_VALUES = ["append", "replace"] as const;

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

export type TAiManageServiceLinksProposal = {
  work_item: string;
  services: string[];
  action: "link" | "unlink";
};

export type TAiManageSprintItemsProposal = {
  sprint: string;
  project?: string | null;
  work_items: string[];
  action: "add" | "remove";
};

export type TAiManageTrackItemsProposal = {
  track: string;
  project?: string | null;
  work_items: string[];
  action: "add" | "remove";
};

export type TAiCreateServiceProposal = {
  project: string;
  name: string;
  description?: string | null;
  status?: string | null;
  criticality?: string | null;
  type?: string | null;
  owner?: string | null;
  repository_url?: string | null;
  documentation_url?: string | null;
};

export type TAiUpdateServiceChanges = {
  status?: string | null;
  criticality?: string | null;
  type?: string | null;
  owner?: string | null;
  description?: string | null;
  repository_url?: string | null;
  documentation_url?: string | null;
};

export type TAiUpdateServiceProposal = {
  service: string;
  project?: string | null;
  changes: TAiUpdateServiceChanges;
};

export type TAiCreateSprintProposal = {
  project: string;
  name: string;
  description?: string | null;
  start_date?: string | null;
  end_date?: string | null;
};

export type TAiUpdateSprintChanges = {
  name?: string | null;
  description?: string | null;
  start_date?: string | null;
  end_date?: string | null;
};

export type TAiUpdateSprintProposal = {
  sprint: string;
  project?: string | null;
  changes: TAiUpdateSprintChanges;
};

export type TAiCreateTrackProposal = {
  project: string;
  name: string;
  description?: string | null;
  start_date?: string | null;
  target_date?: string | null;
  status?: string | null;
  lead?: string | null;
  members?: string[] | null;
};

export type TAiUpdateTrackChanges = {
  name?: string | null;
  description?: string | null;
  start_date?: string | null;
  target_date?: string | null;
  status?: string | null;
  lead?: string | null;
  members?: string[] | null;
};

export type TAiUpdateTrackProposal = {
  track: string;
  project?: string | null;
  changes: TAiUpdateTrackChanges;
};

export type TAiCreateArticleProposal = {
  project: string;
  name: string;
  content: string;
  parent_article?: string | null;
  access?: string | null;
};

export type TAiUpdateArticleProposal = {
  article: string;
  project?: string | null;
  action: "append" | "replace";
  name?: string | null;
  content?: string | null;
};

export type TAiProposal =
  | { key: string; kind: "update_work_item"; proposal: TAiUpdateWorkItemProposal }
  | { key: string; kind: "add_comment"; proposal: TAiAddCommentProposal }
  | { key: string; kind: "manage_service_links"; proposal: TAiManageServiceLinksProposal }
  | { key: string; kind: "manage_sprint_items"; proposal: TAiManageSprintItemsProposal }
  | { key: string; kind: "manage_track_items"; proposal: TAiManageTrackItemsProposal }
  | { key: string; kind: "create_service"; proposal: TAiCreateServiceProposal }
  | { key: string; kind: "update_service"; proposal: TAiUpdateServiceProposal }
  | { key: string; kind: "create_sprint"; proposal: TAiCreateSprintProposal }
  | { key: string; kind: "update_sprint"; proposal: TAiUpdateSprintProposal }
  | { key: string; kind: "create_track"; proposal: TAiCreateTrackProposal }
  | { key: string; kind: "update_track"; proposal: TAiUpdateTrackProposal }
  | { key: string; kind: "create_article"; proposal: TAiCreateArticleProposal }
  | { key: string; kind: "update_article"; proposal: TAiUpdateArticleProposal };

export type TAiProposalDecisionResult = {
  created_comment_id?: string;
  created_service_id?: string;
  created_sprint_id?: string;
  created_track_id?: string;
  created_article_id?: string;
};

export type TAiProposalDecision = {
  kind: TAiProposalKind;
  decision: "applied" | "cancelled";
  result?: TAiProposalDecisionResult;
};

export type TAiProposalConfirmPayload =
  | { kind: "update_work_item"; projectId: string; issueId: string; changes: Partial<TIssue> }
  | { kind: "add_comment"; projectId: string; issueId: string; commentHtml: string }
  | {
      kind: "manage_service_links";
      projectId: string;
      issueId: string;
      action: "link" | "unlink";
      links: { serviceId: string; linkId?: string }[];
    }
  | {
      kind: "manage_sprint_items";
      projectId: string;
      cycleId: string;
      action: "add" | "remove";
      issueIds: string[];
    }
  | {
      kind: "manage_track_items";
      projectId: string;
      moduleId: string;
      action: "add" | "remove";
      issueIds: string[];
    }
  | { kind: "create_service"; projectId: string; data: Partial<IService> }
  | { kind: "update_service"; projectId: string; serviceId: string; changes: Partial<IService> }
  | { kind: "create_sprint"; projectId: string; data: Partial<ICycle> }
  | { kind: "update_sprint"; projectId: string; cycleId: string; changes: Partial<ICycle> }
  | { kind: "create_track"; projectId: string; data: Partial<IModule> }
  | { kind: "update_track"; projectId: string; moduleId: string; changes: Partial<IModule> }
  | {
      kind: "create_article";
      projectId: string;
      data: { name: string; descriptionHtml: string; access: number; parent?: string };
    }
  | {
      kind: "update_article";
      projectId: string;
      pageId: string;
      action: "append" | "replace";
      name?: string;
      descriptionHtml?: string;
    };

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

export type TNamedRef = { id: string; name: string };

/** Match a reference to an item by exact id or case-insensitive exact name. */
export const matchByNameOrId = (items: TNamedRef[], reference: string): TNamedRef | undefined => {
  const trimmed = reference.trim();
  if (!trimmed) return undefined;
  const byId = items.find((item) => item.id === trimmed);
  if (byId) return byId;
  const needle = trimmed.toLowerCase();
  return items.find((item) => item.name.toLowerCase() === needle);
};

/** Mirrors `manage_service_links_proposal_from_args` on the backend. */
export const validateManageServiceLinksProposal = (proposal: Partial<TAiManageServiceLinksProposal>): string | null => {
  if (!proposal.work_item?.trim()) return "Work item is required.";
  if (proposal.action !== "link" && proposal.action !== "unlink") return "Action must be link or unlink.";
  const services = (proposal.services ?? []).map((service) => service.trim()).filter(Boolean);
  if (services.length === 0) return "At least one service is required.";
  if (services.length > PROPOSAL_LIMITS.services) return `At most ${PROPOSAL_LIMITS.services} services are allowed.`;
  return null;
};

/** Mirrors `manage_sprint_items_proposal_from_args` on the backend. */
export const validateManageSprintItemsProposal = (proposal: Partial<TAiManageSprintItemsProposal>): string | null => {
  if (!proposal.sprint?.trim()) return "Sprint is required.";
  if (proposal.action !== "add" && proposal.action !== "remove") return "Action must be add or remove.";
  const items = proposal.work_items ?? [];
  if (items.length === 0) return "At least one work item is required.";
  if (items.length > PROPOSAL_LIMITS.items) return `At most ${PROPOSAL_LIMITS.items} work items are allowed.`;
  return null;
};

/** Mirrors `manage_track_items_proposal_from_args` on the backend. */
export const validateManageTrackItemsProposal = (proposal: Partial<TAiManageTrackItemsProposal>): string | null => {
  if (!proposal.track?.trim()) return "Track is required.";
  if (proposal.action !== "add" && proposal.action !== "remove") return "Action must be add or remove.";
  const items = proposal.work_items ?? [];
  if (items.length === 0) return "At least one work item is required.";
  if (items.length > PROPOSAL_LIMITS.items) return `At most ${PROPOSAL_LIMITS.items} work items are allowed.`;
  return null;
};

const isHttpUrl = (value: string): boolean => value.startsWith("http://") || value.startsWith("https://");

/** Count fields explicitly set (null/undefined means "not changed"). */
export const countChangedFields = (changes: object): number =>
  Object.values(changes).filter((value) => value !== null && value !== undefined).length;

/** Mirrors `create_service_proposal_from_args` on the backend. */
export const validateCreateServiceProposal = (proposal: Partial<TAiCreateServiceProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((proposal.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (proposal.status && !SERVICE_STATUS_VALUES.includes(proposal.status as (typeof SERVICE_STATUS_VALUES)[number]))
    return "Unknown status.";
  if (
    proposal.criticality &&
    !SERVICE_CRITICALITY_VALUES.includes(proposal.criticality as (typeof SERVICE_CRITICALITY_VALUES)[number])
  )
    return "Unknown criticality.";
  if (proposal.type && !SERVICE_TYPE_VALUES.includes(proposal.type as (typeof SERVICE_TYPE_VALUES)[number]))
    return "Unknown type.";
  if ((proposal.owner?.length ?? 0) > PROPOSAL_LIMITS.name) return "Owner is too long.";
  if (proposal.repository_url) {
    if (proposal.repository_url.length > PROPOSAL_LIMITS.url)
      return `Repository URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(proposal.repository_url)) return "Repository URL must start with http:// or https://.";
  }
  if (proposal.documentation_url) {
    if (proposal.documentation_url.length > PROPOSAL_LIMITS.url)
      return `Documentation URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(proposal.documentation_url)) return "Documentation URL must start with http:// or https://.";
  }
  return null;
};

/** Mirrors `update_service_proposal_from_args` on the backend. */
export const validateUpdateServiceProposal = (proposal: Partial<TAiUpdateServiceProposal>): string | null => {
  if (!proposal.service?.trim()) return "Service is required.";
  const changes = proposal.changes ?? {};
  if (countChangedFields(changes) === 0) return "At least one change is required.";
  if (changes.status && !SERVICE_STATUS_VALUES.includes(changes.status as (typeof SERVICE_STATUS_VALUES)[number]))
    return "Unknown status.";
  if (
    changes.criticality &&
    !SERVICE_CRITICALITY_VALUES.includes(changes.criticality as (typeof SERVICE_CRITICALITY_VALUES)[number])
  )
    return "Unknown criticality.";
  if (changes.type && !SERVICE_TYPE_VALUES.includes(changes.type as (typeof SERVICE_TYPE_VALUES)[number]))
    return "Unknown type.";
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if ((changes.owner?.length ?? 0) > PROPOSAL_LIMITS.name) return "Owner is too long.";
  if (changes.repository_url) {
    if (changes.repository_url.length > PROPOSAL_LIMITS.url)
      return `Repository URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(changes.repository_url)) return "Repository URL must start with http:// or https://.";
  }
  if (changes.documentation_url) {
    if (changes.documentation_url.length > PROPOSAL_LIMITS.url)
      return `Documentation URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(changes.documentation_url)) return "Documentation URL must start with http:// or https://.";
  }
  return null;
};

/** Mirrors `create_sprint_proposal_from_args` on the backend. */
export const validateCreateSprintProposal = (proposal: Partial<TAiCreateSprintProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((proposal.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  const start = proposal.start_date?.trim() ?? "";
  const end = proposal.end_date?.trim() ?? "";
  if (start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (end && !ISO_DATE.test(end)) return "End date must be YYYY-MM-DD.";
  if (Boolean(start) !== Boolean(end)) return "Provide both start and end dates or neither.";
  if (start && end && start > end) return "Start date must not be after end date.";
  return null;
};

/** Mirrors `update_sprint_proposal_from_args` on the backend. */
export const validateUpdateSprintProposal = (proposal: Partial<TAiUpdateSprintProposal>): string | null => {
  if (!proposal.sprint?.trim()) return "Sprint is required.";
  const changes = proposal.changes ?? {};
  if (countChangedFields(changes) === 0) return "At least one change is required.";
  if (changes.name != null) {
    const name = changes.name.trim();
    if (!name) return "Name must not be empty.";
    if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  }
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  const start = changes.start_date?.trim() ?? "";
  const end = changes.end_date?.trim() ?? "";
  if (changes.start_date != null && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (changes.end_date != null && !ISO_DATE.test(end)) return "End date must be YYYY-MM-DD.";
  if (start && end && start > end) return "Start date must not be after end date.";
  return null;
};

/** Mirrors `create_track_proposal_from_args` on the backend. */
export const validateCreateTrackProposal = (proposal: Partial<TAiCreateTrackProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((proposal.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (proposal.status && !MODULE_STATUS_VALUES.includes(proposal.status as (typeof MODULE_STATUS_VALUES)[number]))
    return "Unknown status.";
  const start = proposal.start_date?.trim() ?? "";
  const target = proposal.target_date?.trim() ?? "";
  if (start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (target && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  if ((proposal.members?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} members are allowed.`;
  return null;
};

/** Mirrors `update_track_proposal_from_args` on the backend. */
export const validateUpdateTrackProposal = (proposal: Partial<TAiUpdateTrackProposal>): string | null => {
  if (!proposal.track?.trim()) return "Track is required.";
  const changes = proposal.changes ?? {};
  if (countChangedFields(changes) === 0) return "At least one change is required.";
  if (changes.name != null) {
    const name = changes.name.trim();
    if (!name) return "Name must not be empty.";
    if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  }
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (changes.status && !MODULE_STATUS_VALUES.includes(changes.status as (typeof MODULE_STATUS_VALUES)[number]))
    return "Unknown status.";
  const start = changes.start_date?.trim() ?? "";
  const target = changes.target_date?.trim() ?? "";
  if (changes.start_date != null && start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (changes.target_date != null && target && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  if ((changes.members?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} members are allowed.`;
  return null;
};

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Mirrors `create_article_proposal_from_args` on the backend. */
export const validateCreateArticleProposal = (proposal: Partial<TAiCreateArticleProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  const content = proposal.content?.trim() ?? "";
  if (!content) return "Content is required.";
  if (content.length > PROPOSAL_LIMITS.articleContent)
    return `Content must be at most ${PROPOSAL_LIMITS.articleContent} characters.`;
  if (proposal.parent_article && !UUID_RE.test(proposal.parent_article.trim()))
    return "Parent article must be a page uuid.";
  if (proposal.access && !ARTICLE_ACCESS_VALUES.includes(proposal.access as (typeof ARTICLE_ACCESS_VALUES)[number]))
    return "Unknown access.";
  return null;
};

/** Mirrors `update_article_proposal_from_args` on the backend. */
export const validateUpdateArticleProposal = (proposal: Partial<TAiUpdateArticleProposal>): string | null => {
  if (!proposal.article || !UUID_RE.test(proposal.article.trim())) return "Article must be a page uuid.";
  if (!ARTICLE_ACTION_VALUES.includes(proposal.action as (typeof ARTICLE_ACTION_VALUES)[number]))
    return "Action must be append or replace.";
  if (proposal.name != null) {
    const name = proposal.name.trim();
    if (!name) return "Name must not be empty.";
    if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  }
  if (proposal.content != null) {
    const content = proposal.content.trim();
    if (!content) return "Content must not be empty.";
    if (content.length > PROPOSAL_LIMITS.articleContent)
      return `Content must be at most ${PROPOSAL_LIMITS.articleContent} characters.`;
  }
  if (proposal.name == null && proposal.content == null) return "At least one of name or content is required.";
  return null;
};
