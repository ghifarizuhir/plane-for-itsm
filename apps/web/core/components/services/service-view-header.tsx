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
import { ServiceOrderByDropdown } from "./dropdowns/order-by";
import { ServiceFiltersSelection } from "./filters";
import { ServiceSearchInput } from "./search-input";
import { CreateUpdateServiceModal } from "./modal";
import { SERVICE_VIEW_LAYOUTS, ServiceLayoutIcon } from "./service-layout-icon";
// hooks
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { usePlatformOS } from "@/hooks/use-platform-os";

export const ServiceViewHeader = observer(function ServiceViewHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // hooks
  const { isMobile } = usePlatformOS();
  // store hooks
  const { allowPermissions } = useUserPermissions();
  const { loader } = useProject();
  const {
    currentProjectDisplayFilters: displayFilters,
    currentProjectFilters: filters,
    updateDisplayFilters,
    updateFilters,
  } = useServiceFilter();

  const { t } = useTranslation();

  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // handlers
  const handleFilters = useCallback(
    (key: keyof TServiceFilters, value: string | string[]) => {
      if (!projectId) return;
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

      updateFilters(projectId.toString(), { [key]: newValues });
    },
    [filters, projectId, updateFilters]
  );

  const isFiltersApplied = calculateTotalFilters(filters ?? {}) !== 0;

  // auth
  const canUserCreateService = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.PROJECT
  );

  return (
    <Header>
      <Header.LeftItem>
        <div>
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
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        <div className="hidden h-full items-center gap-2 self-end sm:flex">
          <ServiceSearchInput />
          <ServiceOrderByDropdown
            value={displayFilters?.order_by}
            onChange={(val) => {
              if (!projectId || val === displayFilters?.order_by) return;
              updateDisplayFilters(projectId.toString(), {
                order_by: val,
              });
            }}
          />
          <FiltersDropdown
            icon={<FilterOutline className="h-3 w-3" />}
            title={t("common.filters")}
            placement="bottom-end"
            isFiltersApplied={isFiltersApplied}
          >
            <ServiceFiltersSelection filters={filters ?? {}} handleFiltersUpdate={handleFilters} />
          </FiltersDropdown>
          <div className="hidden items-center gap-1 rounded-sm bg-layer-3 p-1 md:flex">
            {SERVICE_VIEW_LAYOUTS.map((layout) => (
              <Tooltip key={layout.key} label={t(layout.i18n_label)} disabled={isMobile}>
                <button
                  type="button"
                  className={cn(
                    "group grid h-5.5 w-7 place-items-center overflow-hidden rounded-sm transition-all hover:bg-layer-transparent-hover",
                    {
                      "bg-layer-transparent-active hover:bg-layer-transparent-active":
                        displayFilters?.layout === layout.key,
                    }
                  )}
                  onClick={() => {
                    if (!projectId) return;
                    updateDisplayFilters(projectId.toString(), { layout: layout.key });
                  }}
                >
                  <ServiceLayoutIcon layoutType={layout.key} />
                </button>
              </Tooltip>
            ))}
          </div>
        </div>
        {canUserCreateService ? (
          <Button variant="primary" onClick={() => setIsCreateModalOpen(true)} size="lg">
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("service.add")}</div>
          </Button>
        ) : (
          <></>
        )}
      </Header.RightItem>
      {workspaceSlug && projectId && (
        <CreateUpdateServiceModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
          projectId={projectId.toString()}
        />
      )}
    </Header>
  );
});
