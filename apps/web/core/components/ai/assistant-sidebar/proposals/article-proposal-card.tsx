/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { EPageAccess } from "@plane/types";
// hooks
import { useProject } from "@/hooks/store/use-project";
// lib
import { matchProject, textToDescriptionHtml } from "@/lib/ai-work-items";
import {
  ARTICLE_ACCESS_VALUES,
  validateCreateArticleProposal,
  validateUpdateArticleProposal,
  type TAiCreateArticleProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiUpdateArticleProposal,
} from "@/lib/ai-proposals";

type ArticleProposalEntry =
  | { kind: "create_article"; proposal: TAiCreateArticleProposal }
  | { kind: "update_article"; proposal: TAiUpdateArticleProposal };

type Props = ArticleProposalEntry & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const ArticleProposalCard = (props: Props) => {
  if (props.kind === "create_article") return <CreateArticleCard {...props} />;
  return <UpdateArticleCard {...props} />;
};

type CreateProps = Extract<ArticleProposalEntry, { kind: "create_article" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const CreateArticleCard = observer(function CreateArticleCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: CreateProps) {
  const { workspaceProjectIds, getProjectById } = useProject();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [name, setName] = useState(proposal.name);
  const [content, setContent] = useState(proposal.content);
  const [access, setAccess] = useState<string>(proposal.access ?? "public");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));
  const matchedProject = matchProject(projects, proposal.project);
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? null;

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article created.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article creation cancelled.
      </p>
    );
  }

  const draft = { project: proposal.project, name, content, parent_article: proposal.parent_article, access };
  const validationError = validateCreateArticleProposal(draft);
  const preview = textToDescriptionHtml(content);

  const confirm = async () => {
    if (!effectiveProjectId) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "create_article",
        projectId: effectiveProjectId,
        data: {
          name: name.trim(),
          descriptionHtml: preview ?? "",
          access: access === "private" ? EPageAccess.PRIVATE : EPageAccess.PUBLIC,
          ...(proposal.parent_article ? { parent: proposal.parent_article } : {}),
        },
      });
    } catch {
      setError("Could not create the article. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Article proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-12 font-medium text-primary">Create article</p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => setSelectedProjectId(event.target.value || null)}
          >
            <option value="">Select a project</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.identifier} — {project.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <label className="mt-2 block text-12 text-secondary">
        Title
        <input
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <label className="mt-2 block text-12 text-secondary">
        Body
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={5}
          value={content}
          onChange={(event) => setContent(event.target.value)}
        />
      </label>
      <label className="mt-2 block text-12 text-secondary">
        Access
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={access}
          onChange={(event) => setAccess(event.target.value)}
        >
          {ARTICLE_ACCESS_VALUES.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </label>
      {proposal.parent_article && (
        <p className="mt-1 text-12 text-tertiary">Sub-page of article {proposal.parent_article}</p>
      )}
      {preview && (
        <div className="mt-2 rounded border border-subtle bg-layer-2 p-2">
          <p className="text-11 text-tertiary">Preview</p>
          <div className="ai-prose mt-1 text-12 text-primary" dangerouslySetInnerHTML={{ __html: preview }} />
        </div>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!effectiveProjectId || Boolean(validationError)}
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

type UpdateProps = Extract<ArticleProposalEntry, { kind: "update_article" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const UpdateArticleCard = observer(function UpdateArticleCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: UpdateProps) {
  const { workspaceProjectIds, getProjectById } = useProject();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [action, setAction] = useState<"append" | "replace">(proposal.action);
  const [name, setName] = useState(proposal.name ?? "");
  const [content, setContent] = useState(proposal.content ?? "");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));
  const matchedProject = proposal.project ? matchProject(projects, proposal.project) : undefined;
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? null;

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article updated.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article update cancelled.
      </p>
    );
  }

  const draft = {
    article: proposal.article,
    action,
    ...(proposal.name != null ? { name } : {}),
    ...(proposal.content != null ? { content } : {}),
  };
  const validationError = validateUpdateArticleProposal(draft);
  const preview = proposal.content != null ? textToDescriptionHtml(content) : null;

  const confirm = async () => {
    if (!effectiveProjectId) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "update_article",
        projectId: effectiveProjectId,
        pageId: proposal.article,
        action,
        ...(proposal.name != null ? { name: name.trim() } : {}),
        ...(proposal.content != null ? { descriptionHtml: preview ?? "" } : {}),
      });
    } catch {
      setError("Could not update the article. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Article update proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Update article</p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => setSelectedProjectId(event.target.value || null)}
          >
            <option value="">Select a project</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.identifier} — {project.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {proposal.content != null && (
        <label className="mt-2 block text-12 text-secondary">
          Action
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={action}
            onChange={(event) => setAction(event.target.value === "replace" ? "replace" : "append")}
          >
            <option value="append">Append at the end</option>
            <option value="replace">Replace the whole body</option>
          </select>
        </label>
      )}
      {proposal.name != null && (
        <label className="mt-2 block text-12 text-secondary">
          Title
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
      )}
      {proposal.content != null && (
        <label className="mt-2 block text-12 text-secondary">
          Body
          <textarea
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            rows={5}
            value={content}
            onChange={(event) => setContent(event.target.value)}
          />
        </label>
      )}
      {preview && (
        <div className="mt-2 rounded border border-subtle bg-layer-2 p-2">
          <p className="text-11 text-tertiary">{action === "append" ? "Will be appended" : "Will replace the body"}</p>
          <div className="ai-prose mt-1 text-12 text-primary" dangerouslySetInnerHTML={{ __html: preview }} />
        </div>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!effectiveProjectId || Boolean(validationError)}
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
