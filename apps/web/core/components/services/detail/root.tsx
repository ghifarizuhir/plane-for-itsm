/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
// assets
import emptyModule from "@/app/assets/empty-state/module.svg?url";
// components
import { EmptyState } from "@/components/common/empty-state";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
import useReloadConfirmations from "@/hooks/use-reload-confirmation";
// local imports
import { ServiceLoadErrorState } from "../service-load-error-state";
import { ServiceDescription } from "./description";
import { ServiceDetailSidebar } from "./sidebar";
import { ServiceTitleInput } from "./title-input";
import { ServiceWorkItems } from "./work-items";

type Props = {
  serviceId: string;
};

export const ServiceDetailRoot = observer(function ServiceDetailRoot(props: Props) {
  const { serviceId } = props;
  // states
  const [isSubmitting, setIsSubmitting] = useState<TNameDescriptionLoader>("saved");
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // unsaved changes guard
  const { setShowAlert } = useReloadConfirmations(isSubmitting === "submitting");
  // store hooks
  const { fetchedMap, getServiceById, errorMap, fetchServices } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const pid = projectId?.toString();
  const service = getServiceById(serviceId);
  const hasFetched = pid ? fetchedMap[pid] : undefined;

  const workspaceId = currentWorkspace?.id;
  const hasError = pid ? errorMap[pid] : false;

  useEffect(() => {
    if (isSubmitting === "submitted") {
      setShowAlert(false);
      const timer = setTimeout(() => setIsSubmitting("saved"), 2000);
      return () => clearTimeout(timer);
    }
    if (isSubmitting === "submitting") setShowAlert(true);
  }, [isSubmitting, setShowAlert]);

  const handleRetry = () => {
    if (!workspaceSlug || !workspaceId || !pid) return;
    fetchServices(workspaceSlug.toString(), workspaceId, pid);
  };

  if (!service) {
    if (hasError) return <ServiceLoadErrorState onRetry={handleRetry} />;
    if (!hasFetched) return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    return (
      <EmptyState
        image={emptyModule}
        title={t("service.detail.not_found_title")}
        description={t("service.detail.not_found_description")}
        primaryButton={{
          text: t("service.detail.view_other_services"),
          onClick: () => router.push(`/${workspaceSlug}/projects/${projectId}/services`),
        }}
      />
    );
  }

  return (
    <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
      <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
        <ServiceTitleInput serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceDescription serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceWorkItems serviceId={serviceId} />
      </div>
      <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
        <ServiceDetailSidebar serviceId={serviceId} />
      </div>
    </div>
  );
});
