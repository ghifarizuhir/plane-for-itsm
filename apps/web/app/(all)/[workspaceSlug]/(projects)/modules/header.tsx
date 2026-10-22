/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { MODULE_VIEW_LAYOUTS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { ChevronDownOutline, ModuleOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, CustomMenu, Header, Row } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { ModuleLayoutIcon, ModuleViewHeader } from "@/components/modules";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceModulePermissions } from "@/hooks/use-workspace-module-permissions";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { WORKSPACE_MODULE_FILTER_KEY } from "@/lib/workspace-modules";

export const WorkspaceModulesHeader = observer(function WorkspaceModulesHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { toggleCreateModuleModal } = useCommandPalette();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateModule } = useWorkspaceModulePermissions();

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.modules")}
                  href={`/${workspaceSlug}/modules/`}
                  icon={<ModuleOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          <WorkspaceApplicationScopeChip />
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        <ModuleViewHeader filterKey={WORKSPACE_MODULE_FILTER_KEY} isGanttDisabled={!scopedProjectId} />
        {canCreateModule && (
          <Button variant="primary" size="lg" onClick={() => toggleCreateModuleModal(true)}>
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("project_module.add_module")}</div>
          </Button>
        )}
      </Header.RightItem>
    </Header>
  );
});

export const WorkspaceModulesMobileHeader = observer(function WorkspaceModulesMobileHeader() {
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { updateDisplayFilters } = useModuleFilter();

  return (
    <div className="flex justify-start md:hidden">
      <CustomMenu
        maxHeight="md"
        className="flex flex-grow justify-start border-b border-subtle bg-surface-1 py-2 text-13 text-secondary"
        customButton={
          <Row className="flex flex-grow justify-center gap-2 text-13 text-secondary">
            <span>Layout</span>
            <ChevronDownOutline className="my-auto h-4 w-4 text-secondary" />
          </Row>
        }
        customButtonClassName="flex flex-grow justify-center items-center text-secondary text-13"
        closeOnSelect
      >
        {MODULE_VIEW_LAYOUTS.map((layout) => {
          if (layout.key === "gantt") return null;

          return (
            <CustomMenu.MenuItem
              key={layout.key}
              onClick={() => updateDisplayFilters(WORKSPACE_MODULE_FILTER_KEY, { layout: layout.key })}
              className="flex items-center gap-2"
            >
              <ModuleLayoutIcon layoutType={layout.key} />
              <div className="text-tertiary">{t(layout.i18n_title)}</div>
            </CustomMenu.MenuItem>
          );
        })}
      </CustomMenu>
    </div>
  );
});
