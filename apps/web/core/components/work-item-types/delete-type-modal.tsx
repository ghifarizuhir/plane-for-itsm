/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// ui
import { AlertModalCore } from "@plane/ui";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  isOpen: boolean;
  typeId: string | null;
  onClose: () => void;
};

export const DeleteTypeModal = observer(function DeleteTypeModal(props: Props) {
  const { workspaceSlug, isOpen, typeId, onClose } = props;
  // store hooks
  const { workItemTypes, deleteWorkItemType } = useWorkflow();
  // states
  const [isDeleteLoading, setIsDeleteLoading] = useState(false);
  // derived values
  const type = typeId ? workItemTypes?.find((item) => item.id === typeId) : undefined;

  const handleClose = () => {
    onClose();
    setIsDeleteLoading(false);
  };

  const handleDeletion = async () => {
    if (!typeId) return;

    setIsDeleteLoading(true);

    try {
      await deleteWorkItemType(workspaceSlug, typeId);
      handleClose();
    } catch (error: any) {
      setIsDeleteLoading(false);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: error?.error ?? "Work item type could not be deleted. Please try again.",
      });
    }
  };

  return (
    <AlertModalCore
      handleClose={handleClose}
      handleSubmit={handleDeletion}
      isSubmitting={isDeleteLoading}
      isOpen={isOpen}
      title="Delete work item type"
      content={
        <>
          Are you sure you want to delete <span className="font-medium text-primary">{type?.name}</span>? Work items
          using this type will no longer be able to use it.
        </>
      }
    />
  );
});
