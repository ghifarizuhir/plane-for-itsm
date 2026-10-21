/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { CyclesOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CyclesViewHeader } from "@/components/cycles/cycles-view-header";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useWorkspaceCyclePermissions } from "@/hooks/use-workspace-cycle-permissions";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { WORKSPACE_CYCLE_FILTER_KEY } from "@/lib/workspace-cycles";

export const WorkspaceCyclesHeader = observer(function WorkspaceCyclesHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { toggleCreateCycleModal } = useCommandPalette();
  const { canCreateCycle } = useWorkspaceCyclePermissions();

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.cycles")}
                  href={`/${workspaceSlug}/cycles/`}
                  icon={<CyclesOutline className="h-4 w-4 text-tertiary" />}
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
        <CyclesViewHeader filterKey={WORKSPACE_CYCLE_FILTER_KEY} />
        {canCreateCycle && (
          <Button variant="primary" size="lg" onClick={() => toggleCreateCycleModal(true)}>
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("project_cycles.add_cycle")}</div>
          </Button>
        )}
      </Header.RightItem>
    </Header>
  );
});
