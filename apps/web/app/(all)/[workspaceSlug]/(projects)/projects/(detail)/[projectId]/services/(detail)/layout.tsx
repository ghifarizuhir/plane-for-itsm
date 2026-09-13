/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { ServerOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { CommonProjectBreadcrumbs } from "@/components/breadcrumbs/common";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useAppRouter } from "@/hooks/use-app-router";

const ServiceDetailBreadcrumbs = observer(function ServiceDetailBreadcrumbs() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId, serviceId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById } = useService();
  // derived values
  const service = serviceId ? getServiceById(serviceId.toString()) : null;

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("service.title")}
                href={`/${workspaceSlug}/projects/${projectId}/services/`}
                icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
              />
            }
          />
          <Breadcrumbs.Item component={<BreadcrumbLink label={service?.name ?? ""} isLast />} isLast />
        </Breadcrumbs>
      </Header.LeftItem>
      <Header.RightItem>
        <></>
      </Header.RightItem>
    </Header>
  );
});

export default function ProjectServiceDetailLayout() {
  return (
    <>
      <AppHeader header={<ServiceDetailBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
