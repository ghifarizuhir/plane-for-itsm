/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// Plane imports
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ServiceForm } from "./service-form";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import useKeypress from "@/hooks/use-keypress";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  data?: IService;
  workspaceSlug: string;
  projectId: string;
};

export const CreateUpdateServiceModal = observer(function CreateUpdateServiceModal(props: Props) {
  const { isOpen, onClose, data, workspaceSlug, projectId } = props;
  // store hooks
  const { createService, updateService } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const workspaceId = currentWorkspace?.id;

  const handleClose = () => {
    onClose();
  };

  const handleCreateService = async (payload: Partial<IService>) => {
    if (!workspaceSlug || !workspaceId || !projectId) return;

    try {
      await createService(workspaceSlug, workspaceId, projectId, payload);
      handleClose();
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: "Success!",
        message: "Service created successfully.",
      });
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

  const handleUpdateService = async (payload: Partial<IService>) => {
    if (!workspaceSlug || !workspaceId || !projectId || !data) return;

    try {
      await updateService(workspaceSlug, workspaceId, projectId, data.id, payload);
      handleClose();
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: "Success!",
        message: "Service updated successfully.",
      });
    } catch (err) {
      const apiError = err as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? "Service could not be updated. Please try again.",
      });
      throw err;
    }
  };

  const handleFormSubmit = async (formData: Partial<IService>) => {
    if (!workspaceSlug || !workspaceId || !projectId) return;

    if (!data) await handleCreateService(formData);
    else await handleUpdateService(formData);
  };

  useKeypress("Escape", () => {
    if (isOpen) handleClose();
  });

  return (
    <ModalCore isOpen={isOpen} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <ServiceForm
        handleFormSubmit={handleFormSubmit}
        handleClose={handleClose}
        status={!!data}
        projectId={projectId}
        data={data}
      />
    </ModalCore>
  );
});

export const ServiceModal = CreateUpdateServiceModal;
