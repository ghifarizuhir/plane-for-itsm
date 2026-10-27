/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { AiStar1Outline, DocumentationOutline, WorkItemsOutline } from "@makeplane/propel/icons";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { AppSidebarItemData } from "@/components/sidebar/sidebar-item";
import { useInstance } from "@/hooks/store/use-instance";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
import { canUseGalileo } from "@/lib/galileo-routing";

type WithDockItemsProps = {
  dockItems: (AppSidebarItemData & { shouldRender: boolean })[];
};

export function withDockItems<P extends WithDockItemsProps>(WrappedComponent: React.ComponentType<P>) {
  const ComponentWithDockItems = observer(function ComponentWithDockItems(props: Omit<P, keyof WithDockItemsProps>) {
    const { workspaceSlug } = useParams();
    const slug = workspaceSlug?.toString() ?? "";
    const { isDocsPath, isGalileoPath, isWorkPath } = useWorkspacePaths();
    const { config } = useInstance();
    const { allowPermissions } = useUserPermissions();
    const { t } = useTranslation();

    const canUseAi = allowPermissions(
      [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
      EUserPermissionsLevel.WORKSPACE,
      slug
    );
    const showGalileo = canUseGalileo(!!config?.has_llm_configured, canUseAi);

    const dockItems: (AppSidebarItemData & { shouldRender: boolean })[] = [
      {
        label: t("sidebar.work"),
        icon: <WorkItemsOutline className="size-5" />,
        href: `/${slug}`,
        isActive: isWorkPath,
        shouldRender: !!slug,
      },
      {
        label: t("sidebar.procedure"),
        icon: <DocumentationOutline className="size-5" />,
        href: `/${slug}/docs`,
        isActive: isDocsPath,
        shouldRender: !!slug,
      },
      {
        label: t("sidebar.galileo"),
        icon: <AiStar1Outline className="size-5" />,
        href: `/${slug}/galileo`,
        isActive: isGalileoPath,
        shouldRender: !!slug && showGalileo,
      },
    ];

    return <WrappedComponent {...(props as P)} dockItems={dockItems} />;
  });

  return ComponentWithDockItems;
}
