/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import type { ICycle, IModule } from "@plane/types";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useMember } from "@/hooks/store/use-member";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { matchAssignees, matchProject } from "@/lib/ai-work-items";
import {
  validateCreateServiceProposal,
  validateCreateSprintProposal,
  validateCreateTrackProposal,
  validateUpdateServiceProposal,
  validateUpdateSprintProposal,
  validateUpdateTrackProposal,
  type TAiCreateServiceProposal,
  type TAiCreateSprintProposal,
  type TAiCreateTrackProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiUpdateServiceProposal,
  type TAiUpdateSprintProposal,
  type TAiUpdateTrackProposal,
} from "@/lib/ai-proposals";
import {
  buildServiceWrite,
  buildSprintWrite,
  buildTrackWrite,
  initialDraftForProposal,
  validateFormDraft,
  visibleFields,
  type TFormDraft,
  type TFormDraftValue,
  type TFormField,
} from "@/lib/ai-proposal-forms";

type FormProposalEntry =
  | { kind: "create_service"; proposal: TAiCreateServiceProposal }
  | { kind: "update_service"; proposal: TAiUpdateServiceProposal }
  | { kind: "create_sprint"; proposal: TAiCreateSprintProposal }
  | { kind: "update_sprint"; proposal: TAiUpdateSprintProposal }
  | { kind: "create_track"; proposal: TAiCreateTrackProposal }
  | { kind: "update_track"; proposal: TAiUpdateTrackProposal };

type CardHandlers = {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

type Props = FormProposalEntry & CardHandlers;
type ServiceProps = Extract<FormProposalEntry, { kind: "create_service" | "update_service" }> & CardHandlers;
type ContainerProps = Extract<
  FormProposalEntry,
  { kind: "create_sprint" | "update_sprint" | "create_track" | "update_track" }
> &
  CardHandlers;
type TMemberOption = { id: string; display_name?: string | null; email?: string | null };

export const ProposalFormCard = (props: Props) => {
  if (props.kind === "create_service" || props.kind === "update_service") return <ServiceFormCard {...props} />;
  return <ContainerFormCard {...props} />;
};

const FormFields = observer(function FormFields({
  fields,
  draft,
  onChange,
  members,
  ownerId,
  onOwnerChange,
  leadId,
  onLeadChange,
  memberIds,
  onToggleMember,
}: {
  fields: readonly TFormField[];
  draft: TFormDraft;
  onChange: (key: string, value: TFormDraftValue) => void;
  members: TMemberOption[];
  ownerId?: string;
  onOwnerChange?: (id: string) => void;
  leadId?: string;
  onLeadChange?: (id: string) => void;
  memberIds?: string[];
  onToggleMember?: (id: string, checked: boolean) => void;
}) {
  return (
    <>
      {fields.map((field) => {
        const value = draft[field.key];
        if (field.kind === "member") {
          const selected = field.key === "owner" ? ownerId : leadId;
          const onSelect = field.key === "owner" ? onOwnerChange : onLeadChange;
          return (
            <label key={field.key} className="mt-2 block text-12 text-secondary">
              {field.label}
              <select
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={selected ?? ""}
                onChange={(event) => onSelect?.(event.target.value)}
              >
                <option value="">No {field.label.toLowerCase()}</option>
                {members.map((member) => (
                  <option key={member.id} value={member.id}>
                    {member.display_name || member.email || member.id}
                  </option>
                ))}
              </select>
            </label>
          );
        }
        if (field.kind === "members") {
          return (
            <div key={field.key} className="mt-2 text-12 text-secondary">
              {field.label}
              <div className="mt-1 flex max-h-24 flex-col gap-1 overflow-y-auto">
                {members.map((member) => (
                  <label key={member.id} className="flex items-center gap-1 text-12 text-primary">
                    <input
                      type="checkbox"
                      checked={(memberIds ?? []).includes(member.id)}
                      onChange={(event) => onToggleMember?.(member.id, event.target.checked)}
                    />
                    {member.display_name || member.email || member.id}
                  </label>
                ))}
              </div>
            </div>
          );
        }
        if (field.kind === "textarea") {
          return (
            <label key={field.key} className="mt-2 block text-12 text-secondary">
              {field.label}
              <textarea
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                rows={3}
                value={typeof value === "string" ? value : ""}
                onChange={(event) => onChange(field.key, event.target.value)}
              />
            </label>
          );
        }
        if (field.kind === "select") {
          return (
            <label key={field.key} className="mt-2 block text-12 text-secondary">
              {field.label}
              <select
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={typeof value === "string" ? value : ""}
                onChange={(event) => onChange(field.key, event.target.value)}
              >
                <option value="">Default</option>
                {(field.options ?? []).map((option) => (
                  <option key={option} value={option}>
                    {option.replace(/-/g, " ")}
                  </option>
                ))}
              </select>
            </label>
          );
        }
        return (
          <label key={field.key} className="mt-2 block text-12 text-secondary">
            {field.label}
            <input
              type={field.kind === "date" ? "date" : field.kind === "url" ? "url" : "text"}
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={typeof value === "string" ? value : ""}
              onChange={(event) => onChange(field.key, event.target.value)}
            />
          </label>
        );
      })}
    </>
  );
});

const ServiceFormCard = observer(function ServiceFormCard(props: ServiceProps) {
  const { decision, onConfirm, onCancel } = props;
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { currentWorkspace } = useWorkspace();
  const workspaceId = currentWorkspace?.id;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchServices, getProjectServiceIds, getServiceById } = useService();
  const { project: projectMemberStore } = useMember();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedServiceId, setSelectedServiceId] = useState("");
  const [draft, setDraft] = useState<TFormDraft>(() => initialDraftForProposal(props.kind, props.proposal));
  const [ownerId, setOwnerId] = useState("");
  const [missing, setMissing] = useState<string[]>([]);
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isUpdate = props.kind === "update_service";
  const serviceRef = props.kind === "update_service" ? props.proposal.service : "";
  const projectRef = props.kind === "update_service" ? (props.proposal.project ?? "") : props.proposal.project;
  const projectIdsKey = (workspaceProjectIds ?? []).join("|");
  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));

  useEffect(() => {
    if (!isUpdate || !workspaceSlug || !workspaceId) return;
    const ids = projectIdsKey.split("|").filter(Boolean);
    const pending = ids.filter((id) => getProjectServiceIds(id) === null);
    if (pending.length === 0) return;
    void Promise.all(pending.map((id) => fetchServices(workspaceSlug, workspaceId, id)));
  }, [isUpdate, workspaceSlug, workspaceId, projectIdsKey, getProjectServiceIds, fetchServices]);

  const serviceCandidates = isUpdate
    ? (workspaceProjectIds ?? []).flatMap((projectId) =>
        (getProjectServiceIds(projectId) ?? []).flatMap((serviceId) => {
          const service = getServiceById(serviceId);
          return service ? [{ id: service.id, name: service.name, projectId }] : [];
        })
      )
    : [];
  const needle = serviceRef.trim().toLowerCase();
  const matches = isUpdate
    ? serviceCandidates.filter(
        (candidate) => candidate.id === serviceRef.trim() || candidate.name.toLowerCase() === needle
      )
    : [];
  const uniqueMatch = matches.length === 1 ? matches[0] : undefined;
  const matchedProject = projectRef ? matchProject(projects, projectRef) : undefined;
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? uniqueMatch?.projectId ?? null;
  const serviceInProject = effectiveProjectId
    ? serviceCandidates.find(
        (candidate) =>
          candidate.projectId === effectiveProjectId &&
          (candidate.id === serviceRef.trim() || candidate.name.toLowerCase() === needle)
      )
    : undefined;
  const fallbackServiceId = !selectedProjectId && !matchedProject && uniqueMatch ? uniqueMatch.id : "";
  const effectiveServiceId = selectedServiceId || serviceInProject?.id || fallbackServiceId || "";
  const serviceOptions = effectiveProjectId
    ? serviceCandidates.filter((candidate) => candidate.projectId === effectiveProjectId)
    : [];

  useEffect(() => {
    if (!workspaceSlug || !effectiveProjectId) return;
    if (!projectMemberStore.getProjectMemberFetchStatus(effectiveProjectId))
      void projectMemberStore.fetchProjectMembers(workspaceSlug, effectiveProjectId);
  }, [workspaceSlug, effectiveProjectId, projectMemberStore]);

  const memberIds = effectiveProjectId ? (projectMemberStore.getProjectMemberIds(effectiveProjectId, true) ?? []) : [];
  const members: TMemberOption[] = memberIds
    .map((id) => ({ id, details: projectMemberStore.getProjectMemberDetails(id, effectiveProjectId ?? "") }))
    .filter((entry) => Boolean(entry.details))
    .map((entry) => ({
      id: entry.id,
      display_name: entry.details?.member.display_name,
      email: entry.details?.member.email,
    }));

  const ownerRef =
    props.kind === "update_service" ? (props.proposal.changes.owner ?? "") : (props.proposal.owner ?? "");
  const resolutionKey = [effectiveProjectId ?? "", members.length, ownerRef].join("::");
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    const result = matchAssignees(members, ownerRef ? [ownerRef] : []);
    setOwnerId(result.matched[0] ?? "");
    setMissing(result.missing.map((name) => `Owner not found: ${name}`));
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {isUpdate ? "Service updated." : "Service created."}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {isUpdate ? "Service update cancelled." : "Service creation cancelled."}
      </p>
    );
  }

  const fields = visibleFields(props.kind, props.proposal);
  const proposalError =
    props.kind === "create_service"
      ? validateCreateServiceProposal(props.proposal)
      : validateUpdateServiceProposal(props.proposal);
  const draftError = validateFormDraft(props.kind, draft);
  const targetReady = isUpdate ? Boolean(effectiveProjectId && effectiveServiceId) : Boolean(effectiveProjectId);

  const confirm = async () => {
    if (!effectiveProjectId || !targetReady) return;
    setSubmitting(true);
    setError(null);
    try {
      if (props.kind === "create_service") {
        await onConfirm({
          kind: "create_service",
          projectId: effectiveProjectId,
          data: buildServiceWrite("create_service", draft, ownerId || null),
        });
      } else {
        await onConfirm({
          kind: "update_service",
          projectId: effectiveProjectId,
          serviceId: effectiveServiceId,
          changes: buildServiceWrite("update_service", draft, ownerId || null),
        });
      }
    } catch {
      setError("Could not save the service. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Service proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-12 font-medium text-primary">
        {isUpdate ? `Update service ${serviceRef}` : `Create service ${props.proposal.name}`}
      </p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => {
              setSelectedProjectId(event.target.value || null);
              setSelectedServiceId("");
            }}
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
      {isUpdate && effectiveProjectId && (
        <label className="mt-2 block text-12 text-secondary">
          Service
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={effectiveServiceId}
            onChange={(event) => setSelectedServiceId(event.target.value)}
          >
            <option value="">Select a service</option>
            {serviceOptions.map((service) => (
              <option key={service.id} value={service.id}>
                {service.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <FormFields
        fields={fields}
        draft={draft}
        onChange={(key, value) => setDraft((current) => ({ ...current, [key]: value }))}
        members={members}
        ownerId={ownerId}
        onOwnerChange={setOwnerId}
      />
      {missing.map((notice) => (
        <p key={notice} className="mt-1 text-12 text-tertiary">
          {notice} — pick one manually.
        </p>
      ))}
      {proposalError && <p className="mt-1 text-12 text-danger-primary">{proposalError}</p>}
      {draftError && <p className="mt-1 text-12 text-danger-primary">{draftError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!targetReady || Boolean(proposalError) || Boolean(draftError)}
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

const ContainerFormCard = observer(function ContainerFormCard(props: ContainerProps) {
  const { decision, onConfirm, onCancel } = props;
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchWorkspaceCycles } = useCycle();
  const { fetchWorkspaceModules } = useModule();
  const { project: projectMemberStore } = useMember();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedContainerId, setSelectedContainerId] = useState("");
  const [containers, setContainers] = useState<{ id: string; name: string; projectId: string }[]>([]);
  const [draft, setDraft] = useState<TFormDraft>(() => initialDraftForProposal(props.kind, props.proposal));
  const [leadId, setLeadId] = useState("");
  const [memberIds, setMemberIds] = useState<string[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isSprint = props.kind === "create_sprint" || props.kind === "update_sprint";
  const isTrack = props.kind === "create_track" || props.kind === "update_track";
  const isUpdate = props.kind === "update_sprint" || props.kind === "update_track";
  const containerLabel = isSprint ? "Sprint" : "Track";
  const containerRef =
    props.kind === "update_sprint" ? props.proposal.sprint : props.kind === "update_track" ? props.proposal.track : "";
  const projectRef =
    props.kind === "create_sprint" || props.kind === "create_track"
      ? props.proposal.project
      : (props.proposal.project ?? "");
  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));

  useEffect(() => {
    if (!isUpdate || !workspaceSlug) return;
    let cancelled = false;
    void (async () => {
      try {
        const fetched: (ICycle | IModule)[] = isSprint
          ? await fetchWorkspaceCycles(workspaceSlug)
          : await fetchWorkspaceModules(workspaceSlug);
        if (cancelled) return;
        setContainers(
          fetched.map((container) => ({ id: container.id, name: container.name, projectId: container.project_id }))
        );
      } catch {
        if (!cancelled) setContainers([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isUpdate, isSprint, workspaceSlug, fetchWorkspaceCycles, fetchWorkspaceModules]);

  const needle = containerRef.trim().toLowerCase();
  const matches = isUpdate
    ? containers.filter((container) => container.id === containerRef.trim() || container.name.toLowerCase() === needle)
    : [];
  const uniqueMatch = matches.length === 1 ? matches[0] : undefined;
  const matchedProject = projectRef ? matchProject(projects, projectRef) : undefined;
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? uniqueMatch?.projectId ?? null;
  const containerInProject = effectiveProjectId
    ? containers.find(
        (container) =>
          container.projectId === effectiveProjectId &&
          (container.id === containerRef.trim() || container.name.toLowerCase() === needle)
      )
    : undefined;
  const fallbackContainerId = !selectedProjectId && !matchedProject && uniqueMatch ? uniqueMatch.id : "";
  const effectiveContainerId = selectedContainerId || containerInProject?.id || fallbackContainerId || "";
  const containerOptions = effectiveProjectId
    ? containers.filter((container) => container.projectId === effectiveProjectId)
    : [];

  useEffect(() => {
    if (!isTrack || !workspaceSlug || !effectiveProjectId) return;
    if (!projectMemberStore.getProjectMemberFetchStatus(effectiveProjectId))
      void projectMemberStore.fetchProjectMembers(workspaceSlug, effectiveProjectId);
  }, [isTrack, workspaceSlug, effectiveProjectId, projectMemberStore]);

  const memberIdsForProject =
    isTrack && effectiveProjectId ? (projectMemberStore.getProjectMemberIds(effectiveProjectId, true) ?? []) : [];
  const members: TMemberOption[] = memberIdsForProject
    .map((id) => ({ id, details: projectMemberStore.getProjectMemberDetails(id, effectiveProjectId ?? "") }))
    .filter((entry) => Boolean(entry.details))
    .map((entry) => ({
      id: entry.id,
      display_name: entry.details?.member.display_name,
      email: entry.details?.member.email,
    }));

  const leadRef =
    props.kind === "create_track"
      ? (props.proposal.lead ?? "")
      : props.kind === "update_track"
        ? (props.proposal.changes.lead ?? "")
        : "";
  const memberRefs =
    props.kind === "create_track"
      ? (props.proposal.members ?? [])
      : props.kind === "update_track"
        ? (props.proposal.changes.members ?? [])
        : [];
  const resolutionKey = [effectiveProjectId ?? "", members.length, leadRef, memberRefs.join("|")].join("::");
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    const leadResult = matchAssignees(members, leadRef ? [leadRef] : []);
    setLeadId(leadResult.matched[0] ?? "");
    const memberResult = matchAssignees(members, memberRefs);
    setMemberIds(memberResult.matched);
    setMissing([
      ...leadResult.missing.map((name) => `Lead not found: ${name}`),
      ...memberResult.missing.map((name) => `Member not found: ${name}`),
    ]);
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} {isUpdate ? "updated." : "created."}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} {isUpdate ? "update cancelled." : "creation cancelled."}
      </p>
    );
  }

  const fields = visibleFields(props.kind, props.proposal);
  const proposalError =
    props.kind === "create_sprint"
      ? validateCreateSprintProposal(props.proposal)
      : props.kind === "update_sprint"
        ? validateUpdateSprintProposal(props.proposal)
        : props.kind === "create_track"
          ? validateCreateTrackProposal(props.proposal)
          : validateUpdateTrackProposal(props.proposal);
  const draftError = validateFormDraft(props.kind, draft);
  const targetReady = isUpdate ? Boolean(effectiveProjectId && effectiveContainerId) : Boolean(effectiveProjectId);

  const confirm = async () => {
    if (!effectiveProjectId || !targetReady) return;
    setSubmitting(true);
    setError(null);
    try {
      if (props.kind === "create_sprint") {
        await onConfirm({
          kind: "create_sprint",
          projectId: effectiveProjectId,
          data: buildSprintWrite("create_sprint", draft),
        });
      } else if (props.kind === "update_sprint") {
        await onConfirm({
          kind: "update_sprint",
          projectId: effectiveProjectId,
          cycleId: effectiveContainerId,
          changes: buildSprintWrite("update_sprint", draft),
        });
      } else if (props.kind === "create_track") {
        await onConfirm({
          kind: "create_track",
          projectId: effectiveProjectId,
          data: buildTrackWrite("create_track", draft, leadId || null, memberIds),
        });
      } else {
        await onConfirm({
          kind: "update_track",
          projectId: effectiveProjectId,
          moduleId: effectiveContainerId,
          changes: buildTrackWrite("update_track", draft, leadId || null, memberIds),
        });
      }
    } catch {
      setError(`Could not save the ${containerLabel.toLowerCase()}. Please retry.`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label={`${containerLabel} proposal`}
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {isUpdate
          ? `Update ${containerLabel.toLowerCase()} ${containerRef}`
          : `Create ${containerLabel.toLowerCase()} ${props.proposal.name}`}
      </p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => {
              setSelectedProjectId(event.target.value || null);
              setSelectedContainerId("");
            }}
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
      {isUpdate && effectiveProjectId && (
        <label className="mt-2 block text-12 text-secondary">
          {containerLabel}
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={effectiveContainerId}
            onChange={(event) => setSelectedContainerId(event.target.value)}
          >
            <option value="">Select a {containerLabel.toLowerCase()}</option>
            {containerOptions.map((container) => (
              <option key={container.id} value={container.id}>
                {container.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {isSprint && !isUpdate && (
        <p className="mt-1 text-12 text-tertiary">The sprint owner is you (the confirming user).</p>
      )}
      <FormFields
        fields={fields}
        draft={draft}
        onChange={(key, value) => setDraft((current) => ({ ...current, [key]: value }))}
        members={members}
        leadId={leadId}
        onLeadChange={setLeadId}
        memberIds={memberIds}
        onToggleMember={(id, checked) =>
          setMemberIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)))
        }
      />
      {missing.map((notice) => (
        <p key={notice} className="mt-1 text-12 text-tertiary">
          {notice} — pick one manually.
        </p>
      ))}
      {proposalError && <p className="mt-1 text-12 text-danger-primary">{proposalError}</p>}
      {draftError && <p className="mt-1 text-12 text-danger-primary">{draftError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!targetReady || Boolean(proposalError) || Boolean(draftError)}
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
