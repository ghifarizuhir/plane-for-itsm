/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// ui
import { CustomSelect } from "@plane/ui";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
};

export const ServiceProjectLinks = observer(function ServiceProjectLinks({ serviceId }: Props) {
  // router
  const { workspaceSlug } = useParams();
  // states
  const [selectedProjectId, setSelectedProjectId] = useState("");
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, addProjectLink, removeProjectLink, transferOwner } = useService();
  const { workspaceProjectIds, getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  const { allowPermissions } = useUserPermissions();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const workspaceId = currentWorkspace?.id;
  const ownerProjectId = service.owner_project_id;
  const isWorkspaceAdmin = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE, slug);
  const isOwnerProjectAdmin = ownerProjectId
    ? allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.PROJECT, slug, ownerProjectId)
    : false;
  const canManageLinks = isWorkspaceAdmin || isOwnerProjectAdmin;
  const canAddForProject = (projectId: string) =>
    isWorkspaceAdmin ||
    isOwnerProjectAdmin ||
    allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.PROJECT, slug, projectId);
  const addableProjects = (workspaceProjectIds ?? [])
    .filter((id) => !service.project_links.some((l) => l.project_id === id))
    .filter((id) => canAddForProject(id))
    .map((id) => getProjectById(id))
    .filter((p): p is NonNullable<typeof p> => Boolean(p));

  const handleAdd = async () => {
    if (!slug || !workspaceId || !selectedProjectId) return;
    try {
      await addProjectLink(slug, workspaceId, serviceId, selectedProjectId);
      setSelectedProjectId("");
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: error instanceof Error ? error.message : t("service.project_links.add_error"),
      });
    }
  };

  const handleRemove = async (linkId: string) => {
    if (!slug || !workspaceId) return;
    try {
      await removeProjectLink(slug, workspaceId, serviceId, linkId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("error"), message: t("service.project_links.remove_error") });
    }
  };

  const handleTransfer = async (projectId: string) => {
    if (!slug || !workspaceId) return;
    try {
      await transferOwner(slug, workspaceId, serviceId, projectId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("error"), message: t("service.project_links.transfer_error") });
    }
  };

  return (
    <div className="mt-3 flex flex-col gap-2">
      {service.project_links.map((link) => {
        const project = getProjectById(link.project_id);
        const isOwner = link.link_type === "owner";
        return (
          <div
            key={link.id}
            className="flex items-center justify-between gap-2 rounded-sm border border-subtle px-2 py-1.5"
          >
            <span className="flex min-w-0 items-center gap-2">
              <span className="truncate text-12 text-primary">{project?.name ?? link.project_id}</span>
              {isOwner && (
                <span className="shrink-0 rounded-xs border border-accent-strong px-1 text-10 text-accent-primary">
                  {t("service.project_links.owner")}
                </span>
              )}
            </span>
            <span className="flex shrink-0 items-center gap-2">
              {isOwner && isWorkspaceAdmin && addableProjects.length > 0 && (
                <CustomSelect
                  value=""
                  onChange={handleTransfer}
                  label={<span className="text-11 text-tertiary">{t("service.project_links.transfer")}</span>}
                  noChevron
                >
                  {addableProjects.map((candidate) => (
                    <CustomSelect.Option key={candidate.id} value={candidate.id}>
                      <span className="truncate">{candidate.name}</span>
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
              )}
              {!isOwner && canManageLinks && (
                <button
                  type="button"
                  onClick={() => handleRemove(link.id)}
                  className="text-11 text-tertiary hover:text-primary"
                >
                  {t("remove")}
                </button>
              )}
            </span>
          </div>
        );
      })}
      {addableProjects.length > 0 && (
        <div className="flex items-center gap-2">
          <CustomSelect
            value={selectedProjectId}
            onChange={setSelectedProjectId}
            className="h-7.5 min-w-0 grow basis-0"
            label={
              <span className="text-12">
                {selectedProjectId ? (
                  getProjectById(selectedProjectId)?.name
                ) : (
                  <span className="text-secondary">{t("service.project_links.add_project")}</span>
                )}
              </span>
            }
          >
            {addableProjects.map((candidate) => (
              <CustomSelect.Option key={candidate.id} value={candidate.id}>
                <span className="truncate">{candidate.name}</span>
              </CustomSelect.Option>
            ))}
          </CustomSelect>
          <Button variant="secondary" size="sm" onClick={handleAdd} disabled={!selectedProjectId}>
            {t("add")}
          </Button>
        </div>
      )}
      <p className="text-11 text-tertiary">{t("service.project_links.hint")}</p>
    </div>
  );
});
