/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ICycle, IModule, IService } from "@plane/types";
import {
  MODULE_STATUS_VALUES,
  SERVICE_CRITICALITY_VALUES,
  SERVICE_STATUS_VALUES,
  SERVICE_TYPE_VALUES,
  type TAiCreateServiceProposal,
  type TAiCreateSprintProposal,
  type TAiCreateTrackProposal,
  type TAiUpdateServiceProposal,
  type TAiUpdateSprintProposal,
  type TAiUpdateTrackProposal,
} from "@/lib/ai-proposals";
import { textToDescriptionHtml } from "@/lib/ai-work-items";

export const FORM_PROPOSAL_KINDS = [
  "create_service",
  "update_service",
  "create_sprint",
  "update_sprint",
  "create_track",
  "update_track",
] as const;
export type TFormProposalKind = (typeof FORM_PROPOSAL_KINDS)[number];

export type TFormProposal =
  | TAiCreateServiceProposal
  | TAiUpdateServiceProposal
  | TAiCreateSprintProposal
  | TAiUpdateSprintProposal
  | TAiCreateTrackProposal
  | TAiUpdateTrackProposal;

export type TFormDraftValue = string | string[] | null;
export type TFormDraft = Record<string, TFormDraftValue>;

export type TFormFieldKind = "text" | "textarea" | "select" | "date" | "member" | "members" | "url";

export type TFormField = {
  key: string;
  label: string;
  kind: TFormFieldKind;
  required?: boolean;
  maxLength?: number;
  options?: readonly string[];
};

export const PROPOSAL_FORM_FIELDS: Record<TFormProposalKind, readonly TFormField[]> = {
  create_service: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "status", label: "Status", kind: "select", options: SERVICE_STATUS_VALUES },
    { key: "criticality", label: "Criticality", kind: "select", options: SERVICE_CRITICALITY_VALUES },
    { key: "type", label: "Type", kind: "select", options: SERVICE_TYPE_VALUES },
    { key: "owner", label: "Owner", kind: "member" },
    { key: "repository_url", label: "Repository URL", kind: "url", maxLength: 2048 },
    { key: "documentation_url", label: "Documentation URL", kind: "url", maxLength: 2048 },
  ],
  update_service: [
    { key: "status", label: "Status", kind: "select", options: SERVICE_STATUS_VALUES },
    { key: "criticality", label: "Criticality", kind: "select", options: SERVICE_CRITICALITY_VALUES },
    { key: "type", label: "Type", kind: "select", options: SERVICE_TYPE_VALUES },
    { key: "owner", label: "Owner", kind: "member" },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "repository_url", label: "Repository URL", kind: "url", maxLength: 2048 },
    { key: "documentation_url", label: "Documentation URL", kind: "url", maxLength: 2048 },
  ],
  create_sprint: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "end_date", label: "End date", kind: "date" },
  ],
  update_sprint: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "end_date", label: "End date", kind: "date" },
  ],
  create_track: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "target_date", label: "Target date", kind: "date" },
    { key: "status", label: "Status", kind: "select", options: MODULE_STATUS_VALUES },
    { key: "lead", label: "Lead", kind: "member" },
    { key: "members", label: "Members", kind: "members" },
  ],
  update_track: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "target_date", label: "Target date", kind: "date" },
    { key: "status", label: "Status", kind: "select", options: MODULE_STATUS_VALUES },
    { key: "lead", label: "Lead", kind: "member" },
    { key: "members", label: "Members", kind: "members" },
  ],
};

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;
const isHttpUrl = (value: string): boolean => value.startsWith("http://") || value.startsWith("https://");

/** Fields to render: all of them for create, only changed fields for update. */
export const visibleFields = (kind: TFormProposalKind, proposal: TFormProposal): readonly TFormField[] => {
  const fields = PROPOSAL_FORM_FIELDS[kind];
  if (!kind.startsWith("update_")) return fields;
  const changes = (proposal as { changes?: Record<string, unknown> }).changes ?? {};
  return fields.filter((field) => changes[field.key] !== null && changes[field.key] !== undefined);
};

/** Draft seeded from the proposal; update kinds seed only their changed fields. */
export const initialDraftForProposal = (kind: TFormProposalKind, proposal: TFormProposal): TFormDraft => {
  const source = (
    kind.startsWith("update_") ? ((proposal as { changes?: Record<string, unknown> }).changes ?? {}) : proposal
  ) as Record<string, unknown>;
  const draft: TFormDraft = {};
  for (const field of visibleFields(kind, proposal)) {
    const value = source[field.key];
    if (field.kind === "members") draft[field.key] = Array.isArray(value) ? value.map(String) : [];
    else if (typeof value === "string") draft[field.key] = value;
    else if (typeof value === "number") draft[field.key] = String(value);
    else draft[field.key] = "";
  }
  return draft;
};

/** Draft-level validation mirroring the backend plus endpoint constraints. */
export const validateFormDraft = (kind: TFormProposalKind, draft: TFormDraft): string | null => {
  const fields = PROPOSAL_FORM_FIELDS[kind];
  for (const field of fields) {
    const value = draft[field.key];
    if (value === undefined) continue;
    if (field.kind === "members") {
      if ((Array.isArray(value) ? value.length : 0) > 10) return "At most 10 members are allowed.";
      continue;
    }
    const text = typeof value === "string" ? value.trim() : "";
    if (field.required && !text) return `${field.label} is required.`;
    if (field.maxLength && text.length > field.maxLength)
      return `${field.label} must be at most ${field.maxLength} characters.`;
    if (field.kind === "url" && text && !isHttpUrl(text)) return `${field.label} must start with http:// or https://.`;
    if (field.kind === "date" && text && !ISO_DATE.test(text)) return `${field.label} must be YYYY-MM-DD.`;
    if (field.kind === "select" && text && field.options && !field.options.includes(text))
      return `Unknown ${field.label.toLowerCase()}.`;
  }
  const start = typeof draft.start_date === "string" ? draft.start_date.trim() : "";
  const endKey = kind.includes("sprint") ? "end_date" : "target_date";
  const end = typeof draft[endKey] === "string" ? (draft[endKey] as string).trim() : "";
  if (start && end && start > end) {
    return kind.includes("sprint")
      ? "Start date must not be after end date."
      : "Start date must not be after target date.";
  }
  if (kind === "create_sprint" && Boolean(start) !== Boolean(end)) return "Provide both dates or neither.";
  if (kind === "update_sprint") {
    if (draft.start_date !== undefined && !start) return "Sprint dates cannot be cleared; pick a new date.";
    if (draft.end_date !== undefined && !end) return "Sprint dates cannot be cleared; pick a new date.";
  }
  if (kind.startsWith("update_")) {
    const hasEffectiveChange = fields.some((field) => {
      const value = draft[field.key];
      if (value === null || value === undefined) return false;
      if (field.kind === "members") return true;
      if (typeof value !== "string") return true;
      return value.trim() !== "" || field.kind !== "select";
    });
    if (!hasEffectiveChange) return "At least one change is required.";
  }
  return null;
};

const draftText = (draft: TFormDraft, key: string): string =>
  typeof draft[key] === "string" ? (draft[key] as string).trim() : "";

export const buildServiceWrite = (
  kind: "create_service" | "update_service",
  draft: TFormDraft,
  ownerId: string | null
): Partial<IService> => {
  const isUpdate = kind === "update_service";
  const data: Partial<IService> = {};
  if (!isUpdate || "name" in draft) data.name = draftText(draft, "name");
  if (!isUpdate || "description" in draft) {
    const description = draftText(draft, "description");
    if (description || isUpdate) {
      data.description = description;
      data.description_html = textToDescriptionHtml(description) ?? "";
    }
  }
  if (draftText(draft, "status")) data.status = draftText(draft, "status") as IService["status"];
  if (draftText(draft, "criticality")) data.criticality = draftText(draft, "criticality") as IService["criticality"];
  if (draftText(draft, "type")) data.type = draftText(draft, "type") as IService["type"];
  if (!isUpdate || "owner" in draft) {
    if (ownerId) data.owner_id = ownerId;
    else if (isUpdate) data.owner_id = null;
  }
  if (!isUpdate || "repository_url" in draft) {
    const url = draftText(draft, "repository_url");
    if (url || isUpdate) data.repository_url = url || null;
  }
  if (!isUpdate || "documentation_url" in draft) {
    const url = draftText(draft, "documentation_url");
    if (url || isUpdate) data.documentation_url = url || null;
  }
  return data;
};

export const buildSprintWrite = (kind: "create_sprint" | "update_sprint", draft: TFormDraft): Partial<ICycle> => {
  const isUpdate = kind === "update_sprint";
  const data: Partial<ICycle> = {};
  if (!isUpdate || "name" in draft) data.name = draftText(draft, "name");
  if (!isUpdate || "description" in draft) {
    const description = draftText(draft, "description");
    if (description || isUpdate) data.description = description;
  }
  if (draftText(draft, "start_date")) data.start_date = draftText(draft, "start_date");
  if (draftText(draft, "end_date")) data.end_date = draftText(draft, "end_date");
  return data;
};

export const buildTrackWrite = (
  kind: "create_track" | "update_track",
  draft: TFormDraft,
  leadId: string | null,
  memberIds: string[]
): Partial<IModule> => {
  const isUpdate = kind === "update_track";
  const data: Partial<IModule> = {};
  if (!isUpdate || "name" in draft) data.name = draftText(draft, "name");
  if (!isUpdate || "description" in draft) {
    const description = draftText(draft, "description");
    if (description || isUpdate) data.description = description;
  }
  if (!isUpdate || "start_date" in draft) {
    const start = draftText(draft, "start_date");
    if (start || isUpdate) data.start_date = start || null;
  }
  if (!isUpdate || "target_date" in draft) {
    const target = draftText(draft, "target_date");
    if (target || isUpdate) data.target_date = target || null;
  }
  if (draftText(draft, "status")) data.status = draftText(draft, "status") as IModule["status"];
  if (!isUpdate || "lead" in draft) {
    if (leadId) data.lead_id = leadId;
    else if (isUpdate) data.lead_id = null;
  }
  if (!isUpdate || "members" in draft) data.member_ids = memberIds;
  return data;
};
