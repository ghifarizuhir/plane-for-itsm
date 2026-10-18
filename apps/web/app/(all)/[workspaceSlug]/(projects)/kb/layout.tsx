/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { PagesOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceKbBreadcrumbs = observer(function WorkspaceKbBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.pages")}
                href={`/${workspaceSlug}/kb/`}
                icon={<PagesOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceKbLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceKbBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
