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
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceServicesBreadcrumbs = observer(function WorkspaceServicesBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug, serviceId } = useParams();
  const { t } = useTranslation();
  const { getServiceById } = useService();
  const service = serviceId ? getServiceById(serviceId.toString()) : null;

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("service.catalog_title")}
                href={`/${workspaceSlug}/services/`}
                icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
                isLast={!serviceId}
              />
            }
            isLast={!serviceId}
          />
          {serviceId && <Breadcrumbs.Item component={<BreadcrumbLink label={service?.name ?? ""} isLast />} isLast />}
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceServicesLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceServicesBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
