/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { EPageAccess } from "@plane/types";
import sanitizeHtml from "sanitize-html";
import type { TAiProposal, TAiProposalDecision } from "@/lib/ai-proposals";
import type { TAiScheduleProposal, TAiScheduleUpdateProposal } from "@/lib/ai-schedule";
import type { TAiWorkItemDecision, TAiWorkItemProposalEntry } from "@/lib/ai-work-items";

export type TAiContextKind = "work_item" | "service" | "article" | "cycle" | "module";

export type TAiWorkItemContext = {
  kind: "work_item";
  name: string;
  descriptionHtml: string;
  state?: string;
  priority?: string;
};

export type TAiServiceContext = {
  kind: "service";
  id: string;
  name: string;
  descriptionHtml: string;
  status?: string;
};

export type TAiArticleContext = {
  kind: "article";
  id: string;
  name: string;
  descriptionHtml: string;
  access?: EPageAccess;
};

export type TAiCycleContext = {
  kind: "cycle";
  id: string; // uuid, usable with the get_sprint tool
  name: string;
  descriptionHtml: string; // cycle.description is plain text; stripHtml is a no-op
  status?: string; // CURRENT | UPCOMING | COMPLETED | DRAFT (server-computed)
  startDate?: string | null;
  endDate?: string | null;
};

export type TAiModuleContext = {
  kind: "module";
  id: string; // uuid, usable with the get_track tool
  name: string;
  descriptionHtml: string;
  status?: string; // backlog | planned | in-progress | paused | completed | cancelled
  startDate?: string | null;
  endDate?: string | null; // module.target_date
};

export type TAiAssistantContext =
  | TAiWorkItemContext
  | TAiServiceContext
  | TAiArticleContext
  | TAiCycleContext
  | TAiModuleContext;

export type TAiMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  isError?: boolean;
  scheduleProposal?: TAiScheduleProposal;
  scheduleProposalKey?: string;
  scheduleDecision?: "pending" | "created" | "cancelled";
  createdScheduleId?: string;
  scheduleUpdateProposal?: TAiScheduleUpdateProposal;
  scheduleUpdateDecision?: "pending" | "applied" | "cancelled";
  workItemProposals?: TAiWorkItemProposalEntry[];
  workItemDecisions?: Record<string, TAiWorkItemDecision>;
  proposals?: TAiProposal[];
  proposalDecisions?: Record<string, TAiProposalDecision>;
  flow?: string | null;
  flowRef?: string | null;
  createdAt?: string;
};

export const AI_ASSISTANT_TASK =
  "You are an ITSM assistant. Answer using the context below when present — the work item, service, sprint, track, or knowledge base article the user is viewing. Be concise, use bullet points. If generating text (description/comment), output the text only.";

export const stripHtml = (html: string): string =>
  html
    .replace(/<[^>]*>/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();

export const CONTEXT_DESCRIPTION_LIMIT = 2000;

const truncateDescription = (text: string): string =>
  text.length <= CONTEXT_DESCRIPTION_LIMIT ? text : `${text.slice(0, CONTEXT_DESCRIPTION_LIMIT)}…`;

const formatDateRange = (startDate?: string | null, endDate?: string | null): string | undefined => {
  if (startDate && endDate) return `Dates: ${startDate} → ${endDate}`;
  if (startDate) return `Dates: from ${startDate}`;
  if (endDate) return `Dates: until ${endDate}`;
  return undefined;
};

const ASSISTANT_ALLOWED_TAGS = [
  "b",
  "strong",
  "i",
  "em",
  "u",
  "s",
  "br",
  "p",
  "ul",
  "ol",
  "li",
  "code",
  "pre",
  "blockquote",
  "h1",
  "h2",
  "h3",
  "h4",
  "hr",
  "a",
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "th",
  "td",
];

/** Allowlist-sanitize model output before it is injected as HTML. */
export const sanitizeAssistantHtml = (html: string): string =>
  sanitizeHtml(html, {
    allowedTags: ASSISTANT_ALLOWED_TAGS,
    allowedAttributes: { a: ["href", "target", "rel"] },
    transformTags: {
      a: sanitizeHtml.simpleTransform("a", { target: "_blank", rel: "noopener noreferrer" }),
    },
    allowProtocolRelative: false,
  });

const buildContextBlock = (context: TAiAssistantContext | undefined): string => {
  if (!context) return "No active context. Answer from general knowledge.";
  const description = truncateDescription(stripHtml(context.descriptionHtml ?? ""));
  switch (context.kind) {
    case "work_item": {
      const parts = [`Work item: ${context.name}`];
      if (description) parts.push(`Description: ${description}`);
      if (context.state) parts.push(`State: ${context.state}`);
      if (context.priority) parts.push(`Priority: ${context.priority}`);
      return `Work item context:\n${parts.join("\n")}`;
    }
    case "service": {
      const parts = [`Service: ${context.name}`, `Service id: ${context.id}`];
      if (context.status) parts.push(`Status: ${context.status}`);
      if (description) parts.push(`Description: ${description}`);
      return `Service context:\n${parts.join("\n")}`;
    }
    case "article": {
      const parts = [`Article: ${context.name}`, `Article id: ${context.id}`];
      if (context.access !== undefined) {
        parts.push(`Access: ${context.access === EPageAccess.PRIVATE ? "Private" : "Public"}`);
      }
      if (description) parts.push(`Description: ${description}`);
      return `Article context:\n${parts.join("\n")}`;
    }
    case "cycle": {
      const parts = [`Sprint: ${context.name}`, `Sprint id: ${context.id}`];
      if (context.status) parts.push(`Status: ${context.status}`);
      const dates = formatDateRange(context.startDate, context.endDate);
      if (dates) parts.push(dates);
      if (description) parts.push(`Description: ${description}`);
      return `Sprint context:\n${parts.join("\n")}`;
    }
    case "module": {
      const parts = [`Track: ${context.name}`, `Track id: ${context.id}`];
      if (context.status) parts.push(`Status: ${context.status}`);
      const dates = formatDateRange(context.startDate, context.endDate);
      if (dates) parts.push(dates);
      if (description) parts.push(`Description: ${description}`);
      return `Track context:\n${parts.join("\n")}`;
    }
  }
};

export const buildAiContext = (context: TAiAssistantContext | undefined, userTimezone?: string): string => {
  const timezoneBlock = userTimezone ? `\nUser timezone: ${userTimezone}` : "";
  return `${buildContextBlock(context)}${timezoneBlock}`;
};
