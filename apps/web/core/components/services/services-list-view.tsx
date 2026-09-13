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
import { Button } from "@plane/propel/button";
// hooks
import { useService } from "@/hooks/store/use-service";
// components
import { CreateUpdateServiceModal } from "./modal";
import { ServiceListItem } from "./service-list-item";

export const ServicesListView = observer(function ServicesListView() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getFilteredServiceIds, loader } = useService();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const serviceIds = projectId ? getFilteredServiceIds(projectId.toString()) : null;

  const openCreateModal = () => setIsCreateModalOpen(true);
  const closeCreateModal = () => setIsCreateModalOpen(false);

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between gap-2 px-4 py-3">
        <h2 className="text-16 font-medium text-primary">{t("service.title")}</h2>
        <Button variant="primary" size="sm" onClick={openCreateModal}>
          {t("service.add")}
        </Button>
      </div>
      {loader || serviceIds === null ? (
        <div className="text-sm p-6 text-secondary">Loading services…</div>
      ) : serviceIds.length === 0 ? (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
          <Button variant="primary" size="sm" onClick={openCreateModal}>
            {t("service.add")}
          </Button>
        </div>
      ) : (
        <div className="flex flex-col gap-1 p-2">
          {serviceIds.map((id) => (
            <ServiceListItem key={id} serviceId={id} />
          ))}
        </div>
      )}
      {workspaceSlug && projectId && (
        <CreateUpdateServiceModal
          isOpen={isCreateModalOpen}
          onClose={closeCreateModal}
          workspaceSlug={workspaceSlug.toString()}
          projectId={projectId.toString()}
        />
      )}
    </div>
  );
});
