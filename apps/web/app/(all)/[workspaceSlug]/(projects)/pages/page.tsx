/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo } from "react";
import { observer } from "mobx-react";
import { useTheme } from "next-themes";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TPageNavigationTabs } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
// assets
import darkPagesAsset from "@/app/assets/empty-state/disabled-feature/pages-dark.webp?url";
import lightPagesAsset from "@/app/assets/empty-state/disabled-feature/pages-light.webp?url";
// components
import { PageHead } from "@/components/core/page-title";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { PageLoader } from "@/components/pages/loaders/page-loader";
import { PagesListRoot } from "@/components/pages/list/root";
import { PagesListView } from "@/components/pages/pages-list-view";
// hooks
import { EPageStoreType, usePageStore } from "@/hooks/store";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
// local imports
import { useWorkspacePagesCreate } from "./create-page-provider";

const getPageType = (pageType?: string | null): TPageNavigationTabs => {
  if (pageType === "private") return "private";
  if (pageType === "archived") return "archived";
  return "public";
};

function WorkspacePagesPage() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  const pageType = getPageType(searchParams.get("type"));
  // theme
  const { resolvedTheme } = useTheme();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreatePage, requestCreatePage } = useWorkspacePagesCreate();
  const { setScopeProjectIds } = usePageStore(EPageStoreType.WORKSPACE);
  const { allowPermissions } = useUserPermissions();

  // derived values
  const projectIds = useMemo(
    () => (scopedProjectId ? [scopedProjectId] : (workspaceProjectIds ?? [])),
    [scopedProjectId, workspaceProjectIds]
  );
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const resolvedEmptyState = resolvedTheme === "light" ? lightPagesAsset : darkPagesAsset;
  const canPerformFeatureAction = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.page_view === false);

  useEffect(() => {
    setScopeProjectIds(projectIds);
  }, [projectIds, setScopeProjectIds]);

  if (!workspaceSlug) return null;
  if (isInitializingProjects || workspaceProjectIds === undefined) return <PageLoader />;

  if (isFeatureDisabled) {
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title={t("disabled_project.empty_state.page.title")}
          description={t("disabled_project.empty_state.page.description")}
          assetPath={resolvedEmptyState}
          primaryButton={{
            text: t("disabled_project.empty_state.page.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !canPerformFeatureAction,
          }}
        />
      </div>
    );
  }

  return (
    <>
      <PageHead title={t("sidebar.pages")} />
      <PagesListView
        pageType={pageType}
        projectId={scopedProjectId ?? undefined}
        storeType={EPageStoreType.WORKSPACE}
        workspaceSlug={workspaceSlug}
        canCreate={canCreatePage}
        onCreatePage={requestCreatePage}
        emptyStateVariant={scopedProjectId ? undefined : "workspace"}
      >
        <PagesListRoot pageType={pageType} storeType={EPageStoreType.WORKSPACE} showProjectBadge={!scopedProjectId} />
      </PagesListView>
    </>
  );
}

export default observer(WorkspacePagesPage);
