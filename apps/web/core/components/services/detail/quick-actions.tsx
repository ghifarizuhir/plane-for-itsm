/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// ui
import { CustomMenu } from "@plane/ui";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { DeleteServiceModal } from "../delete-service-modal";
import { CreateUpdateServiceModal } from "../modal";

type Props = {
  serviceId: string;
};

export const ServiceDetailQuickActions = observer(function ServiceDetailQuickActions(props: Props) {
  const { serviceId } = props;
  // states
  const [isEditModalOpen, setIsEditModalOpen] = useState(false);
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById } = useService();
  // derived values
  const service = getServiceById(serviceId);

  if (!service || !workspaceSlug || !projectId) return <></>;
  const slug = workspaceSlug.toString();
  const pid = projectId.toString();

  return (
    <>
      <CustomMenu
        ellipsis
        placement="bottom-end"
        closeOnSelect
        ariaLabel={t("aria_labels.projects_sidebar.toggle_quick_actions_menu")}
      >
        <CustomMenu.MenuItem onClick={() => setIsEditModalOpen(true)}>{t("edit")}</CustomMenu.MenuItem>
        <CustomMenu.MenuItem onClick={() => setIsDeleteModalOpen(true)}>{t("delete")}</CustomMenu.MenuItem>
      </CustomMenu>
      <CreateUpdateServiceModal
        isOpen={isEditModalOpen}
        onClose={() => setIsEditModalOpen(false)}
        data={service}
        workspaceSlug={slug}
        projectId={pid}
      />
      <DeleteServiceModal data={service} isOpen={isDeleteModalOpen} onClose={() => setIsDeleteModalOpen(false)} />
    </>
  );
});
