/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
// ui
import { AlertModalCore } from "@plane/ui";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  data: IService;
  isOpen: boolean;
  onClose: () => void;
};

export const DeleteServiceModal = observer(function DeleteServiceModal(props: Props) {
  const { data, isOpen, onClose } = props;
  // states
  const [isDeleteLoading, setIsDeleteLoading] = useState(false);
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId, serviceId } = useParams();
  // store hooks
  const { deleteService } = useService();
  const { currentWorkspace } = useWorkspace();

  const handleClose = () => {
    onClose();
    setIsDeleteLoading(false);
  };

  const handleDeletion = async () => {
    if (!workspaceSlug || !projectId || !currentWorkspace?.id) return;

    setIsDeleteLoading(true);

    try {
      await deleteService(workspaceSlug.toString(), currentWorkspace.id, projectId.toString(), data.id);
      if (serviceId) router.push(`/${workspaceSlug}/projects/${data.project_id}/services`);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: "Success!",
        message: "Service deleted successfully.",
      });
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: "Service could not be deleted. Please try again.",
      });
    } finally {
      handleClose();
    }
  };

  return (
    <AlertModalCore
      handleClose={handleClose}
      handleSubmit={handleDeletion}
      isSubmitting={isDeleteLoading}
      isOpen={isOpen}
      title="Delete service"
      content={
        <>
          Are you sure you want to delete service-{" "}
          <span className="font-medium break-all text-primary">{data?.name}</span>? All of the data related to the
          service will be permanently removed. This action cannot be undone.
        </>
      }
    />
  );
});
