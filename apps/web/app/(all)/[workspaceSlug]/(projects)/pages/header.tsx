/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { EPageAccess } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { PagesOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { useWorkspacePagesCreate } from "./create-page-provider";

export const WorkspacePagesHeader = observer(function WorkspacePagesHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  // i18n
  const { t } = useTranslation();
  // create flow
  const { canCreatePage, isCreatingPage, requestCreatePage } = useWorkspacePagesCreate();
  const access = searchParams.get("type") === "private" ? EPageAccess.PRIVATE : EPageAccess.PUBLIC;

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.pages")}
                  href={`/${workspaceSlug}/pages/`}
                  icon={<PagesOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          <WorkspaceApplicationScopeChip />
        </div>
      </Header.LeftItem>
      {canCreatePage && (
        <Header.RightItem>
          <Button variant="primary" size="lg" loading={isCreatingPage} onClick={() => void requestCreatePage(access)}>
            Add page
          </Button>
        </Header.RightItem>
      )}
    </Header>
  );
});
