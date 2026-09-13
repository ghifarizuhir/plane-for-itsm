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
// assets
import emptyModule from "@/app/assets/empty-state/module.svg?url";
// components
import { EmptyState } from "@/components/common/empty-state";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { ServiceDependencies } from "./dependencies";
import { ServiceDetailHeader } from "./header";
import { ServiceDetailTabs, type TServiceDetailTab } from "./tabs";
import { ServiceOverview } from "./overview";
import { ServiceWorkItems } from "./work-items";

type Props = {
  serviceId: string;
};

export const ServiceDetailRoot = observer(function ServiceDetailRoot(props: Props) {
  const { serviceId } = props;
  // states
  const [activeTab, setActiveTab] = useState<TServiceDetailTab>("overview");
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { fetchedMap, getServiceById } = useService();
  // derived values
  const pid = projectId?.toString();
  const service = getServiceById(serviceId);
  const hasFetched = pid ? fetchedMap[pid] : undefined;

  if (!service) {
    if (!hasFetched) return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    return (
      <EmptyState
        image={emptyModule}
        title="Service does not exist"
        description="The service you are looking for does not exist or has been deleted."
        primaryButton={{
          text: "View other services",
          onClick: () => router.push(`/${workspaceSlug}/projects/${projectId}/services`),
        }}
      />
    );
  }

  return (
    <div className="flex h-full w-full flex-col gap-4 p-4">
      <ServiceDetailHeader serviceId={serviceId} />
      <ServiceDetailTabs activeTab={activeTab} onChange={setActiveTab} />
      <div className="min-h-0 flex-1">
        {activeTab === "overview" && <ServiceOverview serviceId={serviceId} />}
        {activeTab === "dependencies" && <ServiceDependencies serviceId={serviceId} />}
        {activeTab === "work_items" && <ServiceWorkItems serviceId={serviceId} />}
      </div>
    </div>
  );
});
