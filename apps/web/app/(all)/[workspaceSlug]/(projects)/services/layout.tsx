/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { RightSidePaneOutline, ServerOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
import { Tooltip } from "@makeplane/propel/components/tooltip";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useServiceDetailPane } from "@/hooks/services/use-service-detail-pane";
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceServicesBreadcrumbs = observer(function WorkspaceServicesBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug, serviceId } = useParams();
  const { t } = useTranslation();
  const { getServiceById } = useService();
  const { isOpen, openPane } = useServiceDetailPane();
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
      <Header.RightItem>
        {serviceId && !isOpen && (
          <Tooltip label={t("page_navigation_pane.open_button")}>
            <button
              type="button"
              className="grid size-6 shrink-0 place-items-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-primary"
              onClick={openPane}
              aria-label={t("page_navigation_pane.open_button")}
            >
              <RightSidePaneOutline className="size-3.5" />
            </button>
          </Tooltip>
        )}
      </Header.RightItem>
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
