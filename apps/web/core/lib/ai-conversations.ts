/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiMessage } from "@/lib/ai-context";
import type { TAiScheduleProposal } from "@/lib/ai-schedule";

export type TAiConversationMode = "classic" | "agent";

export type TAiConversation = {
  id: string;
  title: string;
  mode: TAiConversationMode;
  created_at: string;
  updated_at: string;
};

export type TAiMessageMetadata = {
  schedule_proposal?: TAiScheduleProposal;
  schedule_proposal_key?: string;
  schedule_decision?: "pending" | "created" | "cancelled";
  created_schedule_id?: string;
  is_error?: boolean;
};

export type TAiStoredMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  content_html?: string | null;
  metadata: TAiMessageMetadata;
  created_at: string;
};

/** Keys the `PATCH .../messages/:id/` endpoint accepts (server allowlist). */
export type TAiMessageMetadataPatch = Pick<TAiMessageMetadata, "schedule_decision" | "created_schedule_id">;

/** Map a server-stored message onto the chat bubble model. */
export const toAiMessage = (stored: TAiStoredMessage): TAiMessage => ({
  id: stored.id,
  role: stored.role,
  content: stored.role === "assistant" ? (stored.content_html ?? stored.content) : stored.content,
  isError: stored.metadata?.is_error === true,
  scheduleProposal: stored.metadata?.schedule_proposal,
  scheduleProposalKey: stored.metadata?.schedule_proposal_key,
  scheduleDecision: stored.metadata?.schedule_decision,
  createdScheduleId: stored.metadata?.created_schedule_id,
  createdAt: stored.created_at,
});
