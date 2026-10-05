/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiProposal, TAiProposalConfirmPayload, TAiProposalDecision } from "@/lib/ai-proposals";
import { ArticleProposalCard } from "./article-proposal-card";
import { CommentProposalCard } from "./comment-proposal-card";
import { LinkItemsProposalCard } from "./link-items-proposal-card";
import { ProposalFormCard } from "./proposal-form-card";
import { TriageProposalCard } from "./triage-proposal-card";
import { WorkItemUpdateProposalCard } from "./work-item-update-proposal-card";

type Props = {
  entry: TAiProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const ProposalCard = ({ entry, decision, onConfirm, onCancel }: Props) => {
  if (entry.kind === "update_work_item")
    return (
      <WorkItemUpdateProposalCard
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "add_comment")
    return (
      <CommentProposalCard proposal={entry.proposal} decision={decision} onConfirm={onConfirm} onCancel={onCancel} />
    );
  if (entry.kind === "manage_service_links")
    return (
      <LinkItemsProposalCard
        kind="manage_service_links"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "manage_sprint_items")
    return (
      <LinkItemsProposalCard
        kind="manage_sprint_items"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "manage_track_items")
    return (
      <LinkItemsProposalCard
        kind="manage_track_items"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_service")
    return (
      <ProposalFormCard
        kind="create_service"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "update_service")
    return (
      <ProposalFormCard
        kind="update_service"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_sprint")
    return (
      <ProposalFormCard
        kind="create_sprint"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "update_sprint")
    return (
      <ProposalFormCard
        kind="update_sprint"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_track")
    return (
      <ProposalFormCard
        kind="create_track"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_article")
    return (
      <ArticleProposalCard
        kind="create_article"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "update_article")
    return (
      <ArticleProposalCard
        kind="update_article"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "apply_triage_suggestion")
    return (
      <TriageProposalCard
        kind="apply_triage_suggestion"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "triage_intake_item")
    return (
      <TriageProposalCard
        kind="triage_intake_item"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  return (
    <ProposalFormCard
      kind="update_track"
      proposal={entry.proposal}
      decision={decision}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
};
