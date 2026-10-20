/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiMessage } from "@/lib/ai-context";
import type { TAiProposal, TAiProposalDecision } from "@/lib/ai-proposals";
import type { TAiScheduleProposal, TAiScheduleUpdateProposal } from "@/lib/ai-schedule";
import type { TAiWorkItemDecision, TAiWorkItemProposalEntry } from "@/lib/ai-work-items";

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
  schedule_update_proposal?: TAiScheduleUpdateProposal;
  schedule_update_decision?: "pending" | "applied" | "cancelled";
  work_item_proposals?: TAiWorkItemProposalEntry[];
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
  proposals?: TAiProposal[];
  proposal_decisions?: Record<string, TAiProposalDecision>;
  flow?: string | null;
  flow_ref?: string | null;
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
export type TAiMessageMetadataPatch = Pick<TAiMessageMetadata, "schedule_decision" | "created_schedule_id"> & {
  schedule_update_decision?: "applied" | "cancelled";
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
  proposal_decisions?: Record<string, TAiProposalDecision>;
  flow?: "incident" | null;
  flow_ref?: string | null;
};

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
  scheduleUpdateProposal: stored.metadata?.schedule_update_proposal,
  scheduleUpdateDecision: stored.metadata?.schedule_update_decision,
  workItemProposals: stored.metadata?.work_item_proposals,
  workItemDecisions: stored.metadata?.work_item_decisions,
  proposals: stored.metadata?.proposals,
  proposalDecisions: stored.metadata?.proposal_decisions,
  flow: stored.metadata?.flow,
  flowRef: stored.metadata?.flow_ref,
  createdAt: stored.created_at,
});
