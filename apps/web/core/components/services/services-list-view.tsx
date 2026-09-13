/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// hooks
import { useService } from "@/hooks/store/use-service";
// components
import { ServiceListItem } from "./service-list-item";

export const ServicesListView = observer(function ServicesListView() {
  // router
  const { projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getFilteredServiceIds, loader } = useService();

  const serviceIds = projectId ? getFilteredServiceIds(projectId.toString()) : null;

  if (loader || serviceIds === null) {
    return <div className="text-sm p-6 text-secondary">Loading services…</div>;
  }

  if (serviceIds.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 p-6">
        <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
        <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1 p-2">
      {serviceIds.map((id) => (
        <ServiceListItem key={id} serviceId={id} />
      ))}
    </div>
  );
});
