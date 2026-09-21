/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TAiIssueContext = {
  name: string;
  descriptionHtml: string;
  state?: string;
  priority?: string;
};

export type TAiMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  isError?: boolean;
};

export const AI_ASSISTANT_TASK =
  "You are an ITSM work-item assistant. Answer using the work item context below. Be concise, use bullet points. If generating text (description/comment), output the text only.";

const HISTORY_MESSAGE_LIMIT = 8;

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

const buildContextBlock = (context: TAiIssueContext | undefined): string => {
  if (!context) return "No active work item context. Answer from general knowledge.";
  const parts = [`Work item: ${context.name}`];
  const description = stripHtml(context.descriptionHtml ?? "");
  if (description) parts.push(`Description: ${description}`);
  if (context.state) parts.push(`State: ${context.state}`);
  if (context.priority) parts.push(`Priority: ${context.priority}`);
  return `Work item context:\n${parts.join("\n")}`;
};

export const buildAiPrompt = (
  context: TAiIssueContext | undefined,
  history: TAiMessage[],
  question: string
): string => {
  const historyBlock = history
    .slice(-HISTORY_MESSAGE_LIMIT)
    .map((message) => `${message.role === "user" ? "User" : "Assistant"}: ${message.content}`)
    .join("\n");
  return `${buildContextBlock(context)}\n\nConversation so far:\n${historyBlock || "(empty)"}\n\nUser's new question: ${question}`;
};
