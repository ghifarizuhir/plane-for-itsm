/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { MouseEvent } from "react";
import { useRef } from "react";
import { observer } from "mobx-react";
import { usePathname, useSearchParams } from "next/navigation";
import { CircularProgress } from "@makeplane/propel/components/circular-progress";
import { TickOutline } from "@makeplane/propel/icons";
// plane imports
import type { TCycleGroups } from "@plane/types";
// components
import { generateQueryParams, calculateCycleProgress } from "@plane/utils";
import { ListItem } from "@/components/core/list";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
import { usePlatformOS } from "@/hooks/use-platform-os";
// local imports
import { CycleQuickActions } from "../quick-actions";
import { CycleListItemAction } from "./cycle-list-item-action";

type TCyclesListItem = {
  cycleId: string;
  handleEditCycle?: () => void;
  handleDeleteCycle?: () => void;
  handleAddToFavorites?: () => void;
  handleRemoveFromFavorites?: () => void;
  workspaceSlug: string;
  projectId?: string;
  showProjectBadge?: boolean;
  className?: string;
};

export const CyclesListItem = observer(function CyclesListItem(props: TCyclesListItem) {
  const { cycleId, workspaceSlug, projectId, showProjectBadge = false, className = "" } = props;
  // refs
  const parentRef = useRef(null);
  // router
  const router = useAppRouter();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  // hooks
  const { isMobile } = usePlatformOS();
  // store hooks
  const { getCycleById } = useCycle();
  const { getProjectById } = useProject();

  // derived values
  const cycleDetails = getCycleById(cycleId);
  const resolvedProjectId = cycleDetails?.project_id || projectId;
  const project = showProjectBadge && resolvedProjectId ? getProjectById(resolvedProjectId) : undefined;

  if (!cycleDetails || !resolvedProjectId) return null;

  // computed
  // TODO: change this logic once backend fix the response
  const cycleStatus = cycleDetails.status ? (cycleDetails.status.toLocaleLowerCase() as TCycleGroups) : "draft";
  const isActive = cycleStatus === "current";

  // handlers
  const openCycleOverview = (e: MouseEvent<HTMLButtonElement | HTMLAnchorElement>) => {
    e.preventDefault();
    e.stopPropagation();

    const query = generateQueryParams(searchParams, ["peekCycle"]);
    if (searchParams.has("peekCycle") && searchParams.get("peekCycle") === cycleId) {
      router.push(`${pathname}?${query}`);
    } else {
      router.push(`${pathname}?${query && `${query}&`}peekCycle=${cycleId}`);
    }
  };

  // handlers
  const handleArchivedCycleClick = (e: MouseEvent<HTMLAnchorElement>) => {
    openCycleOverview(e);
  };

  const handleItemClick = cycleDetails.archived_at ? handleArchivedCycleClick : undefined;

  const progress = calculateCycleProgress(cycleDetails);

  return (
    <ListItem
      title={cycleDetails?.name ?? ""}
      itemLink={`/${workspaceSlug}/projects/${resolvedProjectId}/cycles/${cycleDetails.id}`}
      onItemClick={handleItemClick}
      className={className}
      prependTitleElement={
        <div className="relative size-[30px]">
          <div className="absolute inset-0 flex scale-150 items-center justify-center">
            <CircularProgress
              value={progress}
              size="md"
              variant={progress === 100 ? "success" : "brand"}
              aria-label="Cycle progress"
            />
          </div>
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            {progress === 100 ? (
              <TickOutline className="h-3 w-3" />
            ) : (
              <span className="text-9 text-primary">{`${progress}%`}</span>
            )}
          </div>
        </div>
      }
      appendTitleElement={
        project ? (
          <span className="max-w-[140px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5 text-11 text-tertiary">
            {project.name}
          </span>
        ) : undefined
      }
      actionableItems={
        <CycleListItemAction
          workspaceSlug={workspaceSlug}
          projectId={resolvedProjectId}
          cycleId={cycleId}
          cycleDetails={cycleDetails}
          parentRef={parentRef}
          isActive={isActive}
        />
      }
      quickActionElement={
        <div className="block md:hidden">
          <CycleQuickActions
            parentRef={parentRef}
            cycleId={cycleId}
            projectId={resolvedProjectId}
            workspaceSlug={workspaceSlug}
          />
        </div>
      }
      isMobile={isMobile}
      parentRef={parentRef}
      isSidebarOpen={searchParams.has("peekCycle")}
    />
  );
});
