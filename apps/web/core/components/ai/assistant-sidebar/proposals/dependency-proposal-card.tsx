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
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { matchProject } from "@/lib/ai-work-items";
import {
  createsDependencyCycle,
  matchByNameOrId,
  validateManageServiceDependenciesProposal,
  type TAiManageServiceDependenciesProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TNamedRef,
} from "@/lib/ai-proposals";

type Props = {
  proposal: TAiManageServiceDependenciesProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const DependencyProposalCard = observer(function DependencyProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { currentWorkspace } = useWorkspace();
  const workspaceId = currentWorkspace?.id;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchServices, getProjectServiceIds, getServiceById, getDependenciesByProject } = useService();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));
  const projectsKey = projects.map((project) => project.id).join("|");
  const matchedProjectId = proposal.project ? (matchProject(projects, proposal.project)?.id ?? null) : null;
  const activeProjectId = selectedProjectId ?? matchedProjectId;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || !activeProjectId) return;
    void fetchServices(workspaceSlug, workspaceId, activeProjectId);
  }, [workspaceSlug, workspaceId, activeProjectId, fetchServices]);

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || activeProjectId) return;
    let cancelled = false;
    void (async () => {
      setScanning(true);
      try {
        for (const projectId of projectsKey.split("|").filter(Boolean)) {
          if (cancelled) return;
          // oxlint-disable-next-line no-await-in-loop -- projects are scanned sequentially so the first match wins
          await fetchServices(workspaceSlug, workspaceId, projectId);
          const ids = getProjectServiceIds(projectId) ?? [];
          const services: TNamedRef[] = ids.flatMap((id) => {
            const service = getServiceById(id);
            return service ? [{ id: service.id, name: service.name }] : [];
          });
          if (matchByNameOrId(services, proposal.service)) {
            if (!cancelled) setSelectedProjectId(projectId);
            return;
          }
        }
      } finally {
        if (!cancelled) setScanning(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [
    workspaceSlug,
    workspaceId,
    activeProjectId,
    projectsKey,
    fetchServices,
    getProjectServiceIds,
    getServiceById,
    proposal.service,
  ]);

  const serviceIds = activeProjectId ? (getProjectServiceIds(activeProjectId) ?? []) : [];
  const services: TNamedRef[] = serviceIds.flatMap((id) => {
    const service = getServiceById(id);
    return service ? [{ id: service.id, name: service.name }] : [];
  });
  const servicesLoaded = activeProjectId ? getProjectServiceIds(activeProjectId) !== null : false;
  const fromService = matchByNameOrId(services, proposal.service);
  const toService = matchByNameOrId(services, proposal.depends_on);
  const dependencies = activeProjectId ? getDependenciesByProject(activeProjectId) : [];
  const existing =
    fromService && toService
      ? dependencies.find(
          (dependency) => dependency.from_service_id === fromService.id && dependency.to_service_id === toService.id
        )
      : undefined;
  const validationError = validateManageServiceDependenciesProposal(proposal);

  let blockedReason: string | null = null;
  if (servicesLoaded) {
    if (!fromService || !toService) blockedReason = "Service not found in this project.";
    else if (fromService.id === toService.id) blockedReason = "A service cannot depend on itself.";
    else if (proposal.action === "link" && existing) blockedReason = "Already connected — nothing to do.";
    else if (proposal.action === "link" && createsDependencyCycle(dependencies, fromService.id, toService.id))
      blockedReason = "This connection would create a dependency cycle.";
    else if (proposal.action === "unlink" && !existing) blockedReason = "Not connected — nothing to do.";
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {proposal.action === "link" ? "Connection added." : "Connection removed."}{" "}
        {workspaceSlug && activeProjectId && fromService && (
          <Link
            href={`/${workspaceSlug}/projects/${activeProjectId}/services/${fromService.id}`}
            className="text-accent-primary hover:underline"
          >
            Open service
          </Link>
        )}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Connection update cancelled.
      </p>
    );
  }

  const confirm = async () => {
    if (!activeProjectId || !fromService || !toService || blockedReason || validationError) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "manage_service_dependencies",
        projectId: activeProjectId,
        action: proposal.action,
        fromServiceId: fromService.id,
        toServiceId: toService.id,
        ...(proposal.action === "unlink" && existing ? { dependencyId: existing.id } : {}),
      });
      if (workspaceSlug && workspaceId) void fetchServices(workspaceSlug, workspaceId, activeProjectId);
    } catch {
      setError("Could not update the connection. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Service dependency proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {proposal.action === "link" ? "Connect services" : "Disconnect services"}
      </p>
      <p className="mt-1 text-12 text-secondary">
        {proposal.service} → {proposal.depends_on}{" "}
        <span className="text-tertiary">
          ({proposal.service} depends on {proposal.depends_on})
        </span>
      </p>
      {!activeProjectId && scanning && <p className="mt-1 text-12 text-tertiary">Looking up the services…</p>}
      {!activeProjectId && !scanning && projects.length > 0 && (
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
      {activeProjectId && !servicesLoaded && <p className="mt-1 text-12 text-tertiary">Loading services…</p>}
      {blockedReason && <p className="mt-1 text-12 text-tertiary">{blockedReason}</p>}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={
            !activeProjectId ||
            !servicesLoaded ||
            !fromService ||
            !toService ||
            Boolean(blockedReason) ||
            Boolean(validationError)
          }
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
