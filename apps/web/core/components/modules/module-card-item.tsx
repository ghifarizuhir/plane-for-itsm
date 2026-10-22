/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { SyntheticEvent } from "react";
import React, { useRef } from "react";
import { observer } from "mobx-react";
import Link from "next/link";
import { useParams, usePathname, useSearchParams } from "next/navigation";
import { InfoOutline, ProjectsOutline, UserAltOutline, WorkItemsOutline } from "@makeplane/propel/icons";
// plane package imports
import { MODULE_STATUS, EUserPermissions, EUserPermissionsLevel, IS_FAVORITE_MENU_OPEN } from "@plane/constants";
import { useLocalStorage } from "@plane/hooks";
import { LinearProgress } from "@makeplane/propel/components/linear-progress";
import { TOAST_TYPE, setPromiseToast, setToast } from "@plane/propel/toast";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import type { IModule } from "@plane/types";
import { Card, FavoriteStar } from "@plane/ui";
import { getDate, renderFormattedPayloadDate, generateQueryParams } from "@plane/utils";
// components
import { DateRangeDropdown } from "@/components/dropdowns/date-range";
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
import { ModuleQuickActions } from "@/components/modules";
import { ModuleStatusDropdown } from "@/components/modules/module-status-dropdown";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { usePlatformOS } from "@/hooks/use-platform-os";

const handleEventPropagation = (e: SyntheticEvent<HTMLDivElement>) => {
  e.stopPropagation();
  e.preventDefault();
};

type Props = {
  moduleId: string;
  showProjectBadge?: boolean;
};

export const ModuleCardItem = observer(function ModuleCardItem(props: Props) {
  const { moduleId, showProjectBadge = false } = props;
  // refs
  const parentRef = useRef(null);
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  // store hooks
  const { allowPermissions } = useUserPermissions();
  const { getModuleById, addModuleToFavorites, removeModuleFromFavorites, updateModuleDetails } = useModule();
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();
  // local storage
  const { setValue: toggleFavoriteMenu, storedValue } = useLocalStorage<boolean>(IS_FAVORITE_MENU_OPEN, false);
  // derived values
  const moduleDetails = getModuleById(moduleId);
  const resolvedProjectId = moduleDetails?.project_id ?? projectId?.toString();
  const project = showProjectBadge && resolvedProjectId ? getProjectById(resolvedProjectId) : undefined;
  const isEditingAllowed = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug?.toString(),
    resolvedProjectId
  );
  const isDisabled = !isEditingAllowed || !!moduleDetails?.archived_at;
  const renderIcon = Boolean(moduleDetails?.start_date) || Boolean(moduleDetails?.target_date);

  const { isMobile } = usePlatformOS();
  const handleAddToFavorites = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    e.preventDefault();
    if (!workspaceSlug || !resolvedProjectId) return;

    const addToFavoritePromise = addModuleToFavorites(workspaceSlug.toString(), resolvedProjectId, moduleId).then(
      () => {
        if (!storedValue) toggleFavoriteMenu(true);
        return undefined;
      }
    );

    setPromiseToast(addToFavoritePromise, {
      loading: "Adding module to favorites...",
      success: {
        title: "Success!",
        message: () => "Module added to favorites.",
      },
      error: {
        title: "Error!",
        message: () => "Couldn't add the module to favorites. Please try again.",
      },
    });
  };

  const handleRemoveFromFavorites = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    e.preventDefault();
    if (!workspaceSlug || !resolvedProjectId) return;

    const removeFromFavoritePromise = removeModuleFromFavorites(workspaceSlug.toString(), resolvedProjectId, moduleId);

    setPromiseToast(removeFromFavoritePromise, {
      loading: "Removing module from favorites...",
      success: {
        title: "Success!",
        message: () => "Module removed from favorites.",
      },
      error: {
        title: "Error!",
        message: () => "Couldn't remove the module from favorites. Please try again.",
      },
    });
  };

  const handleModuleDetailsChange = async (payload: Partial<IModule>) => {
    if (!workspaceSlug || !resolvedProjectId) return;

    await updateModuleDetails(workspaceSlug.toString(), resolvedProjectId, moduleId, payload)
      .then(() => {
        setToast({
          type: TOAST_TYPE.SUCCESS,
          title: "Success!",
          message: "Module updated successfully.",
        });
        return undefined;
      })
      .catch((err) => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: err?.detail ?? "Module could not be updated. Please try again.",
        });
      });
  };

  const openModuleOverview = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    e.preventDefault();

    const query = generateQueryParams(searchParams, ["peekModule"]);
    if (searchParams.has("peekModule") && searchParams.get("peekModule") === moduleId) {
      router.push(`${pathname}?${query}`);
    } else {
      router.push(`${pathname}?${query && `${query}&`}peekModule=${moduleId}`);
    }
  };

  if (!moduleDetails) return null;

  const moduleTotalIssues =
    moduleDetails.backlog_issues +
    moduleDetails.unstarted_issues +
    moduleDetails.started_issues +
    moduleDetails.completed_issues +
    moduleDetails.cancelled_issues;

  const moduleCompletedIssues = moduleDetails.completed_issues;

  // const areYearsEqual = startDate.getFullYear() === endDate.getFullYear();

  const moduleStatus = MODULE_STATUS.find((status) => status.value === moduleDetails.status);

  const issueCount = moduleDetails
    ? !moduleTotalIssues || moduleTotalIssues === 0
      ? `0 work items`
      : moduleTotalIssues === moduleCompletedIssues
        ? `${moduleTotalIssues} Work item${moduleTotalIssues > 1 ? `s` : ``}`
        : `${moduleCompletedIssues}/${moduleTotalIssues} Work items`
    : `0 work items`;

  const moduleLeadDetails = moduleDetails.lead_id ? getUserDetails(moduleDetails.lead_id) : undefined;
  const progressValue = moduleTotalIssues > 0 ? (moduleCompletedIssues / moduleTotalIssues) * 100 : 0;

  return (
    <div className="relative" data-prevent-progress>
      <Link ref={parentRef} href={`/${workspaceSlug}/projects/${moduleDetails.project_id}/modules/${moduleDetails.id}`}>
        <Card>
          <div>
            <div className="flex items-center justify-between gap-2">
              <Tooltip label={moduleDetails.name} layout="stacked" disabled={isMobile}>
                <span className="truncate text-14 font-medium">{moduleDetails.name}</span>
              </Tooltip>
              <div
                role="presentation"
                className="flex items-center gap-2"
                onClick={handleEventPropagation}
                onKeyDown={handleEventPropagation}
              >
                {moduleStatus && (
                  <ModuleStatusDropdown
                    isDisabled={isDisabled}
                    moduleDetails={moduleDetails}
                    handleModuleDetailsChange={handleModuleDetailsChange}
                  />
                )}
                <button onClick={openModuleOverview}>
                  <InfoOutline className="h-4 w-4 text-placeholder" />
                </button>
              </div>
            </div>
          </div>
          {project && (
            <div className="flex items-center gap-1.5 text-11 text-tertiary">
              <ProjectsOutline className="h-3 w-3 flex-shrink-0" />
              <span className="truncate">{project.name}</span>
            </div>
          )}
          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-1.5 text-secondary">
                <WorkItemsOutline className="h-4 w-4 text-tertiary" />
                <span className="text-11 text-tertiary">{issueCount ?? "0 Work item"}</span>
              </div>
              {moduleLeadDetails ? (
                <span className="cursor-default">
                  <ButtonAvatars showTooltip={false} userIds={moduleLeadDetails?.id} />
                </span>
              ) : (
                <Tooltip label="No lead">
                  <UserAltOutline className="mx-1 h-4 w-4 text-tertiary" />
                </Tooltip>
              )}
            </div>
            <LinearProgress
              value={progressValue}
              size="md"
              variant="brand"
              showValue={false}
              aria-label="Module progress"
            />
            <div
              role="presentation"
              className="flex items-center justify-between py-0.5"
              onClick={handleEventPropagation}
              onKeyDown={handleEventPropagation}
            >
              <DateRangeDropdown
                buttonContainerClassName={`h-6 w-full flex ${isDisabled ? "cursor-not-allowed" : "cursor-pointer"} items-center gap-1.5 text-tertiary border-[0.5px] border-strong rounded-sm text-11`}
                buttonVariant="transparent-with-text"
                className="h-7"
                value={{
                  from: getDate(moduleDetails.start_date),
                  to: getDate(moduleDetails.target_date),
                }}
                onSelect={(val) => {
                  handleModuleDetailsChange({
                    start_date: val?.from ? renderFormattedPayloadDate(val.from) : null,
                    target_date: val?.to ? renderFormattedPayloadDate(val.to) : null,
                  });
                }}
                placeholder={{
                  from: "Start date",
                  to: "End date",
                }}
                disabled={isDisabled}
                hideIcon={{ from: renderIcon ?? true, to: renderIcon }}
              />
            </div>
          </div>
        </Card>
      </Link>
      <div className="absolute right-4 bottom-[18px] flex items-center gap-1.5">
        {isEditingAllowed && (
          <FavoriteStar
            onClick={(e) => {
              if (moduleDetails.is_favorite) handleRemoveFromFavorites(e);
              else handleAddToFavorites(e);
            }}
            selected={!!moduleDetails.is_favorite}
          />
        )}
        {workspaceSlug && resolvedProjectId && (
          <ModuleQuickActions
            parentRef={parentRef}
            moduleId={moduleId}
            projectId={resolvedProjectId}
            workspaceSlug={workspaceSlug.toString()}
          />
        )}
      </div>
    </div>
  );
});
