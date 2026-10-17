/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ServiceForm } from "@/components/services/service-form";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
import useKeypress from "@/hooks/use-keypress";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  /** Pre-selected owner project (e.g. the project filter of the catalog). */
  defaultOwnerProjectId?: string;
};

export const WorkspaceCreateServiceModal = observer(function WorkspaceCreateServiceModal(props: Props) {
  const { isOpen, onClose, workspaceSlug, defaultOwnerProjectId } = props;
  // states
  const [ownerProjectId, setOwnerProjectId] = useState(defaultOwnerProjectId ?? "");
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  const { createWorkspaceService } = useService();
  const { currentWorkspace } = useWorkspace();
  const { allowPermissions } = useUserPermissions();
  const workspaceId = currentWorkspace?.id;

  // list only projects the user may create services in (member+ or ws admin)
  const projects = useMemo(
    () =>
      (workspaceProjectIds ?? [])
        .map((id) => getProjectById(id))
        .filter((p): p is NonNullable<typeof p> => Boolean(p))
        .filter((p) =>
          allowPermissions(
            [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
            EUserPermissionsLevel.PROJECT,
            workspaceSlug,
            p.id
          )
        )
        .map((p) => ({ id: p.id, name: p.name })),
    [workspaceProjectIds, getProjectById, allowPermissions, workspaceSlug]
  );

  useEffect(() => {
    if (!ownerProjectId && projects.length > 0) setOwnerProjectId(projects[0].id);
  }, [ownerProjectId, projects]);

  useEffect(() => {
    if (isOpen && defaultOwnerProjectId) setOwnerProjectId(defaultOwnerProjectId);
  }, [isOpen, defaultOwnerProjectId]);

  const handleFormSubmit = async (values: Partial<IService>) => {
    if (!workspaceSlug || !workspaceId || !ownerProjectId) {
      throw new Error("Owner project is required");
    }
    try {
      await createWorkspaceService(workspaceSlug, workspaceId, {
        ...values,
        owner_project_id: ownerProjectId,
      });
      onClose();
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: "Service created successfully." });
    } catch (err) {
      const apiError = err as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? "Service could not be created. Please try again.",
      });
      throw err;
    }
  };

  useKeypress("Escape", () => {
    if (isOpen) onClose();
  });

  return (
    <ModalCore isOpen={isOpen} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <ServiceForm
        handleFormSubmit={handleFormSubmit}
        handleClose={onClose}
        status={false}
        projectId={ownerProjectId}
        workspaceSlug={workspaceSlug}
        ownerProjectField={{ value: ownerProjectId, onChange: setOwnerProjectId, projects }}
      />
    </ModalCore>
  );
});
