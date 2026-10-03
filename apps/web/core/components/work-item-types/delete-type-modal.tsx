/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// ui
import { AlertModalCore } from "@plane/ui";
// hooks
import { useWorkItemType } from "@/hooks/store/use-work-item-type";

type Props = {
  workspaceSlug: string;
  isOpen: boolean;
  typeId: string | null;
  onClose: () => void;
};

export const DeleteTypeModal = observer(function DeleteTypeModal(props: Props) {
  const { workspaceSlug, isOpen, typeId, onClose } = props;
  // store hooks
  const { workItemTypes, deleteWorkItemType } = useWorkItemType();
  // plane hooks
  const { t } = useTranslation();
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
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("work_item_types.settings.item_delete_confirmation.toast.success.title"),
        message: t("work_item_types.settings.item_delete_confirmation.toast.success.message"),
      });
      handleClose();
    } catch (error: any) {
      setIsDeleteLoading(false);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("work_item_types.settings.item_delete_confirmation.toast.error.title"),
        message: error?.error ?? t("work_item_types.settings.item_delete_confirmation.toast.error.message"),
      });
    }
  };

  return (
    <AlertModalCore
      handleClose={handleClose}
      handleSubmit={handleDeletion}
      isSubmitting={isDeleteLoading}
      isOpen={isOpen}
      title={t("work_item_types.settings.item_delete_confirmation.title")}
      primaryButtonText={{
        loading: t("deleting"),
        default: t("work_item_types.settings.item_delete_confirmation.primary_button"),
      }}
      content={
        <>
          {t("work_item_types.settings.item_delete_confirmation.description")}
          {type?.name && <span className="mt-1 block font-medium text-primary">{type.name}</span>}
        </>
      }
    />
  );
});
