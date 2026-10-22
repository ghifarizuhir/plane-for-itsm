/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
// plane imports
import { EPageAccess } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TPageNavigationTabs } from "@plane/types";
// components
import { PageLoader } from "@/components/pages/loaders/page-loader";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { usePageStore } from "@/hooks/store";
import type { EPageStoreType } from "@/hooks/store";

type Props = {
  children: React.ReactNode;
  pageType: TPageNavigationTabs;
  storeType: EPageStoreType;
  createProjectId?: string;
  canCreate?: boolean;
  onCreatePage?: (access: EPageAccess) => void | Promise<void>;
  emptyStateVariant?: "workspace";
};

export const PagesListMainContent = observer(function PagesListMainContent(props: Props) {
  const { children, pageType, storeType, createProjectId, canCreate, onCreatePage, emptyStateVariant } = props;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { currentProjectDetails } = useProject();
  const {
    canCurrentUserCreatePage,
    createPage,
    getCurrentProjectFilteredPageIdsByTab,
    getCurrentProjectPageIdsByTab,
    isAnyPageAvailable,
    loader,
  } = usePageStore(storeType);
  // states
  const [isCreatingPage, setIsCreatingPage] = useState(false);
  // router
  const router = useRouter();
  const { workspaceSlug } = useParams();
  // derived values
  const pageIds = getCurrentProjectPageIdsByTab(pageType);
  const filteredPageIds = getCurrentProjectFilteredPageIdsByTab(pageType);
  const canPerformEmptyStateActions = canCreate ?? canCurrentUserCreatePage;

  // handle page create
  const handleCreatePage = async () => {
    setIsCreatingPage(true);
    const access = pageType === "private" ? EPageAccess.PRIVATE : EPageAccess.PUBLIC;

    try {
      if (onCreatePage) {
        await onCreatePage(access);
        return;
      }

      const page = await createPage({ access }, createProjectId);
      const redirectProjectId = createProjectId ?? currentProjectDetails?.id ?? page?.project_ids?.[0];
      if (page?.id && redirectProjectId) {
        router.push(`/${workspaceSlug}/projects/${redirectProjectId}/pages/${page.id}`);
      }
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          (error as { data?: { error?: string } })?.data?.error || "Page could not be created. Please try again.",
      });
    } finally {
      setIsCreatingPage(false);
    }
  };

  if (loader === "init-loader") return <PageLoader />;

  if (!isAnyPageAvailable) {
    if (emptyStateVariant === "workspace") {
      return (
        <EmptyStateDetailed
          assetKey="page"
          title={t("workspace_empty_state.kb.title")}
          description={t("workspace_empty_state.kb.description")}
        />
      );
    }

    return (
      <EmptyStateDetailed
        assetKey="page"
        title={t("project_empty_state.pages.title")}
        description={t("project_empty_state.pages.description")}
        actions={[
          {
            label: t("project_empty_state.pages.cta_primary"),
            onClick: handleCreatePage,
            variant: "primary",
            disabled: !canPerformEmptyStateActions || isCreatingPage,
          },
        ]}
      />
    );
  }

  if (pageIds?.length === 0) {
    if (pageType === "archived") {
      return (
        <EmptyStateDetailed
          assetKey="page"
          title={t("project_empty_state.archive_pages.title")}
          description={t("project_empty_state.archive_pages.description")}
        />
      );
    }

    return (
      <EmptyStateDetailed
        assetKey="page"
        title={t("project_empty_state.pages.title")}
        description={t("project_empty_state.pages.description")}
        actions={[
          {
            label: t("project_empty_state.pages.cta_primary"),
            onClick: handleCreatePage,
            variant: "primary",
            disabled: !canPerformEmptyStateActions || isCreatingPage,
          },
        ]}
      />
    );
  }

  if (filteredPageIds?.length === 0) {
    return (
      <EmptyStateDetailed
        assetKey="search"
        title={t("common_empty_state.search.title")}
        description={t("common_empty_state.search.description")}
      />
    );
  }

  return <div className="h-full w-full overflow-hidden">{children}</div>;
});
