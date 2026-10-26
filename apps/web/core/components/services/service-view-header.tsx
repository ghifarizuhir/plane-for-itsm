/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { FilterOutline, ServerOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import type { TServiceFilters } from "@plane/types";
import { Breadcrumbs, Header } from "@plane/ui";
// helpers
import { cn, calculateTotalFilters } from "@plane/utils";
// components
import { FiltersDropdown } from "@/components/issues/issue-layouts/filters";
import { CommonProjectBreadcrumbs } from "@/components/breadcrumbs/common";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
import { ServiceOrderByDropdown } from "./dropdowns/order-by";
import { ServiceGroupByDropdown } from "./dropdowns/group-by";
import { ServiceFiltersSelection } from "./filters";
import { SERVICE_GRAPH_CONTROLS_SLOT_ID } from "./graph/service-graph-controls-slot";
import { ServiceSearchInput } from "./search-input";
import { CreateUpdateServiceModal } from "./modal";
import { SERVICE_VIEW_LAYOUTS, ServiceLayoutIcon } from "./service-layout-icon";
import { WorkspaceCreateServiceModal } from "./workspace/create-service-modal";
// hooks
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { usePlatformOS } from "@/hooks/use-platform-os";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceServicePermissions } from "@/hooks/use-workspace-service-permissions";
// helpers
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";

type Props = {
  workspaceMode?: boolean;
  filterKey?: string;
};

export const ServiceViewHeader = observer(function ServiceViewHeader({ workspaceMode = false, filterKey }: Props) {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // hooks
  const { isMobile } = usePlatformOS();
  // store hooks
  const { allowPermissions } = useUserPermissions();
  const { loader } = useProject();
  const { getDisplayFiltersByProjectId, getFiltersByProjectId, updateDisplayFilters, updateFilters } =
    useServiceFilter();
  const { t } = useTranslation();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateService } = useWorkspaceServicePermissions();
  const resolvedFilterKey = filterKey ?? (workspaceMode ? WORKSPACE_SERVICE_FILTER_KEY : projectId?.toString());
  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;

  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // handlers
  const handleFilters = useCallback(
    (key: keyof TServiceFilters, value: string | string[]) => {
      if (!resolvedFilterKey) return;
      const newValues = [...(filters?.[key] ?? [])];

      if (Array.isArray(value))
        value.forEach((val) => {
          if (!newValues.includes(val as never)) newValues.push(val as never);
          else newValues.splice(newValues.indexOf(val as never), 1);
        });
      else {
        if (filters?.[key]?.includes(value as never)) newValues.splice(newValues.indexOf(value as never), 1);
        else newValues.push(value as never);
      }

      updateFilters(resolvedFilterKey, { [key]: newValues });
    },
    [filters, resolvedFilterKey, updateFilters]
  );

  const isFiltersApplied = calculateTotalFilters(filters ?? {}) !== 0;
  const isGraphLayout = displayFilters?.layout === "graph" && (!workspaceMode || Boolean(scopedProjectId));

  // auth
  const canUserCreateService = workspaceMode
    ? canCreateService
    : allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT);

  return (
    <Header>
      <Header.LeftItem>
        {workspaceMode ? (
          <div className="flex min-w-0 items-center gap-2">
            <Breadcrumbs onBack={router.back}>
              <Breadcrumbs.Item
                component={
                  <BreadcrumbLink
                    label={t("service.catalog_title")}
                    href={`/${workspaceSlug}/services/`}
                    icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
                    isLast
                  />
                }
                isLast
              />
            </Breadcrumbs>
            <WorkspaceApplicationScopeChip />
          </div>
        ) : (
          <Breadcrumbs onBack={router.back} isLoading={loader === "init-loader"}>
            <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("service.title")}
                  href={`/${workspaceSlug}/projects/${projectId}/services/`}
                  icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
        )}
      </Header.LeftItem>
      <Header.RightItem>
        <div className="hidden h-full items-center gap-2 self-end sm:flex">
          <ServiceSearchInput />
          <ServiceOrderByDropdown
            value={displayFilters?.order_by}
            onChange={(val) => {
              if (!resolvedFilterKey || val === displayFilters?.order_by) return;
              updateDisplayFilters(resolvedFilterKey, {
                order_by: val,
              });
            }}
          />
          {displayFilters?.layout === "list" && (
            <ServiceGroupByDropdown
              value={displayFilters?.group_by}
              onChange={(val) => {
                if (!resolvedFilterKey || val === displayFilters?.group_by) return;
                updateDisplayFilters(resolvedFilterKey, {
                  group_by: val,
                });
              }}
            />
          )}
          <FiltersDropdown
            icon={<FilterOutline className="h-3 w-3" />}
            title={t("common.filters")}
            placement="bottom-end"
            isFiltersApplied={isFiltersApplied}
          >
            <ServiceFiltersSelection filters={filters ?? {}} handleFiltersUpdate={handleFilters} />
          </FiltersDropdown>
          {isGraphLayout && <div id={SERVICE_GRAPH_CONTROLS_SLOT_ID} className="flex items-center" />}
          <div className="hidden items-center gap-1 rounded-sm bg-layer-3 p-1 md:flex">
            {SERVICE_VIEW_LAYOUTS.map((layout) => {
              const isLayoutDisabled = workspaceMode && !scopedProjectId && layout.key === "graph";

              return (
                <Tooltip
                  key={layout.key}
                  label={isLayoutDisabled ? "Select an application to view graph" : t(layout.i18n_label)}
                  disabled={isMobile && !isLayoutDisabled}
                >
                  <button
                    type="button"
                    aria-disabled={isLayoutDisabled}
                    className={cn(
                      "group grid h-5.5 w-7 place-items-center overflow-hidden rounded-sm transition-all hover:bg-layer-transparent-hover",
                      {
                        "bg-layer-transparent-active hover:bg-layer-transparent-active":
                          displayFilters?.layout === layout.key,
                        "cursor-not-allowed opacity-40 hover:bg-transparent": isLayoutDisabled,
                      }
                    )}
                    onClick={() => {
                      if (isLayoutDisabled || !resolvedFilterKey) return;
                      updateDisplayFilters(resolvedFilterKey, { layout: layout.key });
                    }}
                  >
                    <ServiceLayoutIcon layoutType={layout.key} />
                  </button>
                </Tooltip>
              );
            })}
          </div>
        </div>
        {canUserCreateService && (
          <Button variant="primary" onClick={() => setIsCreateModalOpen(true)} size="lg">
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("service.add")}</div>
          </Button>
        )}
      </Header.RightItem>
      {workspaceMode ? (
        <WorkspaceCreateServiceModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug?.toString() ?? ""}
          defaultOwnerProjectId={scopedProjectId ?? undefined}
        />
      ) : (
        workspaceSlug &&
        projectId && (
          <CreateUpdateServiceModal
            isOpen={isCreateModalOpen}
            onClose={() => setIsCreateModalOpen(false)}
            workspaceSlug={workspaceSlug.toString()}
            projectId={projectId.toString()}
          />
        )
      )}
    </Header>
  );
});
