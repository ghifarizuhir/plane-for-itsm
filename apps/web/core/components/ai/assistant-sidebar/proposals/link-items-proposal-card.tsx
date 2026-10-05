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
import { useCycle } from "@/hooks/store/use-cycle";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { matchProject, parseWorkItemRef, workItemHref } from "@/lib/ai-work-items";
import {
  matchByNameOrId,
  validateManageServiceLinksProposal,
  validateManageSprintItemsProposal,
  validateManageTrackItemsProposal,
  type TAiManageServiceLinksProposal,
  type TAiManageSprintItemsProposal,
  type TAiManageTrackItemsProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TNamedRef,
} from "@/lib/ai-proposals";

type LinkProposalEntry =
  | { kind: "manage_service_links"; proposal: TAiManageServiceLinksProposal }
  | { kind: "manage_sprint_items"; proposal: TAiManageSprintItemsProposal }
  | { kind: "manage_track_items"; proposal: TAiManageTrackItemsProposal };

type Props = LinkProposalEntry & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

type ServiceLinksProps = Extract<Props, { kind: "manage_service_links" }>;
type ContainerProps = Extract<Props, { kind: "manage_sprint_items" | "manage_track_items" }>;

export const LinkItemsProposalCard = (props: Props) => {
  if (props.kind === "manage_service_links") return <ServiceLinksCard {...props} />;
  return <ContainerItemsCard {...props} />;
};

type ServiceLinkRow = {
  reference: string;
  serviceId: string | null;
  linkId: string | null;
  status: "actionable" | "skipped" | "missing";
};

const ServiceLinksCard = observer(function ServiceLinksCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: ServiceLinksProps) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { currentWorkspace } = useWorkspace();
  const workspaceId = currentWorkspace?.id;
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { fetchServices, getProjectServiceIds, getServiceById, getWorkItemLinksByService } = useService();
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
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

  const projectId = issue?.project_id ?? null;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || !projectId) return;
    void fetchServices(workspaceSlug, workspaceId, projectId);
  }, [workspaceSlug, workspaceId, projectId, fetchServices]);

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Service links updated.{" "}
        {workspaceSlug && projectId && issue && (
          <Link href={workItemHref(workspaceSlug, projectId, issue.id)} className="text-accent-primary hover:underline">
            Open work item
          </Link>
        )}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Service link update cancelled.
      </p>
    );
  }

  const serviceIds = projectId ? (getProjectServiceIds(projectId) ?? []) : [];
  const services: TNamedRef[] = serviceIds.flatMap((id) => {
    const service = getServiceById(id);
    return service ? [{ id: service.id, name: service.name }] : [];
  });
  const rows: ServiceLinkRow[] = proposal.services.map((reference) => {
    const service = matchByNameOrId(services, reference);
    if (!service || !issue) return { reference, serviceId: null, linkId: null, status: "missing" };
    const link = getWorkItemLinksByService(service.id).find((candidate) => candidate.issue_id === issue.id);
    const isActionable = proposal.action === "link" ? !link : Boolean(link);
    return {
      reference,
      serviceId: service.id,
      linkId: link?.id ?? null,
      status: isActionable ? "actionable" : "skipped",
    };
  });
  const actionable = rows.filter((row) => row.status === "actionable");
  const validationError = issue ? validateManageServiceLinksProposal(proposal) : null;

  const confirm = async () => {
    if (!issue || !projectId || actionable.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const links = actionable.map((row) => ({
        serviceId: row.serviceId as string,
        ...(row.linkId ? { linkId: row.linkId } : {}),
      }));
      await onConfirm({
        kind: "manage_service_links",
        projectId,
        issueId: issue.id,
        action: proposal.action,
        links,
      });
    } catch {
      setError("Could not update the service links. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Service link proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {proposal.action === "link" ? "Link services to" : "Unlink services from"} {proposal.work_item}
      </p>
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      <ul className="mt-2 flex flex-col gap-1">
        {rows.map((row) => (
          <li key={row.reference} className="text-12 text-secondary">
            {row.reference}
            {row.status === "missing" && <span className="text-tertiary"> — not found in this project, skipped</span>}
            {row.status === "skipped" && (
              <span className="text-tertiary">
                {" "}
                — {proposal.action === "link" ? "already linked" : "not linked"}, skipped
              </span>
            )}
          </li>
        ))}
      </ul>
      {actionable.length === 0 && !resolveError && <p className="mt-1 text-12 text-tertiary">Nothing to change.</p>}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || actionable.length === 0 || Boolean(validationError)}
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

type ContainerRow = {
  reference: string;
  issueId: string | null;
  status: "actionable" | "skipped" | "missing" | "other_project";
};

const ContainerItemsCard = observer(function ContainerItemsCard(props: ContainerProps) {
  const { decision, onConfirm, onCancel } = props;
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { fetchAllCycles, getProjectCycleDetails } = useCycle();
  const { fetchModules, getProjectModuleDetails } = useModule();
  const [issues, setIssues] = useState<Record<string, TIssue | null>>({});
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [containerId, setContainerId] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const kind = props.kind;
  const containerName = kind === "manage_sprint_items" ? props.proposal.sprint : props.proposal.track;
  const containerLabel = kind === "manage_sprint_items" ? "Sprint" : "Track";
  const workItemsKey = props.proposal.work_items.join("|");

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));

  useEffect(() => {
    if (!workspaceSlug) return;
    let cancelled = false;
    const references = workItemsKey.split("|").filter(Boolean);
    void (async () => {
      const entries = await Promise.all(
        references.map(async (reference) => {
          const parsed = parseWorkItemRef(reference);
          if (!parsed) return [reference, null] as const;
          try {
            const issue = await fetchIssueWithIdentifier(workspaceSlug, parsed.projectIdentifier, parsed.sequenceId);
            return [reference, issue] as const;
          } catch {
            return [reference, null] as const;
          }
        })
      );
      if (!cancelled) setIssues(Object.fromEntries(entries));
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, workItemsKey, fetchIssueWithIdentifier]);

  const matchedProjectId = props.proposal.project ? (matchProject(projects, props.proposal.project)?.id ?? null) : null;
  const firstResolvedIssue = Object.values(issues).find((issue): issue is TIssue => Boolean(issue));
  const effectiveProjectId = selectedProjectId ?? matchedProjectId ?? firstResolvedIssue?.project_id ?? null;

  useEffect(() => {
    if (!workspaceSlug || !effectiveProjectId) return;
    if (kind === "manage_sprint_items") void fetchAllCycles(workspaceSlug, effectiveProjectId);
    else void fetchModules(workspaceSlug, effectiveProjectId);
  }, [workspaceSlug, effectiveProjectId, kind, fetchAllCycles, fetchModules]);

  const containers =
    effectiveProjectId == null
      ? []
      : ((kind === "manage_sprint_items"
          ? getProjectCycleDetails(effectiveProjectId)
          : getProjectModuleDetails(effectiveProjectId)) ?? []);
  const namedContainers = containers.map((container) => ({ id: container.id, name: container.name }));
  const matchedContainer = matchByNameOrId(namedContainers, containerName);
  const effectiveContainerId = containerId || matchedContainer?.id || "";

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} updated.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} update cancelled.
      </p>
    );
  }

  const rows: ContainerRow[] = props.proposal.work_items.map((reference) => {
    const issue = issues[reference];
    if (!issue) return { reference, issueId: null, status: "missing" };
    if (issue.project_id !== effectiveProjectId) return { reference, issueId: issue.id, status: "other_project" };
    const included =
      kind === "manage_sprint_items"
        ? issue.cycle_id === effectiveContainerId
        : (issue.module_ids ?? []).includes(effectiveContainerId);
    const isActionable = props.proposal.action === "add" ? !included : included;
    return { reference, issueId: issue.id, status: isActionable ? "actionable" : "skipped" };
  });
  const actionable = rows.filter((row) => row.status === "actionable");
  const validationError =
    kind === "manage_sprint_items"
      ? validateManageSprintItemsProposal(props.proposal)
      : validateManageTrackItemsProposal(props.proposal);

  const confirm = async () => {
    if (!effectiveProjectId || !effectiveContainerId || actionable.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const issueIds = actionable.map((row) => row.issueId as string);
      if (kind === "manage_sprint_items") {
        await onConfirm({
          kind: "manage_sprint_items",
          projectId: effectiveProjectId,
          cycleId: effectiveContainerId,
          action: props.proposal.action,
          issueIds,
        });
      } else {
        await onConfirm({
          kind: "manage_track_items",
          projectId: effectiveProjectId,
          moduleId: effectiveContainerId,
          action: props.proposal.action,
          issueIds,
        });
      }
    } catch {
      setError(`Could not update the ${containerLabel.toLowerCase()}. Please retry.`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Container items proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {props.proposal.action === "add"
          ? `Add work items to ${containerName}`
          : `Remove work items from ${containerName}`}
      </p>
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
      {effectiveProjectId && (
        <label className="mt-2 block text-12 text-secondary">
          {containerLabel}
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={effectiveContainerId}
            onChange={(event) => setContainerId(event.target.value)}
          >
            <option value="">Select a {containerLabel.toLowerCase()}</option>
            {namedContainers.map((container) => (
              <option key={container.id} value={container.id}>
                {container.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <ul className="mt-2 flex flex-col gap-1">
        {rows.map((row) => (
          <li key={row.reference} className="text-12 text-secondary">
            {row.reference}
            {row.status === "missing" && <span className="text-tertiary"> — not found, skipped</span>}
            {row.status === "other_project" && (
              <span className="text-tertiary"> — belongs to another project, skipped</span>
            )}
            {row.status === "skipped" && (
              <span className="text-tertiary">
                {" "}
                — {props.proposal.action === "add" ? "already included" : "not included"}, skipped
              </span>
            )}
          </li>
        ))}
      </ul>
      {actionable.length === 0 && !validationError && <p className="mt-1 text-12 text-tertiary">Nothing to change.</p>}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!effectiveProjectId || !effectiveContainerId || actionable.length === 0 || Boolean(validationError)}
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
