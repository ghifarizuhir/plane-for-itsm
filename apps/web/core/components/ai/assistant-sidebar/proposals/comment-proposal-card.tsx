/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import type { TIssue } from "@plane/types";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
// lib
import { parseWorkItemRef, textToDescriptionHtml, workItemHref } from "@/lib/ai-work-items";
import {
  validateAddCommentProposal,
  type TAiAddCommentProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
} from "@/lib/ai-proposals";

type Props = {
  proposal: TAiAddCommentProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const CommentProposalCard = observer(function CommentProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
  const [comment, setComment] = useState(proposal.comment);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceSlug) return;
    const ref = parseWorkItemRef(proposal.work_item);
    if (!ref) {
      setResolveError("Work item reference is invalid.");
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const fetched = await fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId);
        if (!cancelled) setIssue(fetched);
      } catch {
        if (!cancelled) setResolveError("Work item not found or not accessible.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, proposal.work_item, fetchIssueWithIdentifier]);

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Comment added.{" "}
        {workspaceSlug && issue?.project_id && (
          <Link
            href={workItemHref(workspaceSlug, issue.project_id, issue.id)}
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
        Comment cancelled.
      </p>
    );
  }

  const previewHtml = textToDescriptionHtml(comment) ?? "";
  const validationError = issue ? validateAddCommentProposal({ work_item: proposal.work_item, comment }) : null;

  const confirm = async () => {
    const projectId = issue?.project_id ?? null;
    if (!issue || !projectId || !previewHtml) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "add_comment",
        projectId,
        issueId: issue.id,
        commentHtml: previewHtml,
      });
    } catch {
      setError("Could not add the comment. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Comment proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-12 font-medium text-primary">Comment on {proposal.work_item}</p>
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      <textarea
        className="mt-2 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
        rows={3}
        value={comment}
        onChange={(event) => setComment(event.target.value)}
      />
      {previewHtml && (
        <div
          className="ai-prose mt-2 rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          dangerouslySetInnerHTML={{ __html: previewHtml }}
        />
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || Boolean(validationError)}
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
