/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { WAR_ROOM_STATUS_TABS, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { cn } from "@plane/utils";
// helpers
import { statusFilterForTab } from "@/services/war-room.helpers";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import useDebounce from "@/hooks/use-debounce";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWarRoomFilter } from "@/hooks/store/use-war-room-filter";
// components
import { CreateWarRoomModal } from "../create/create-war-room-modal";
import { WarRoomLoadErrorState } from "./war-room-load-error-state";
import { WarRoomSummaryChips } from "./war-room-summary-chips";
import { WarRoomsBoard } from "./war-rooms-board";

type Props = {
  prefillIssueId?: string;
};

export const WarRoomsListView = observer(function WarRoomsListView({ prefillIssueId }: Props) {
  // router
  const { workspaceSlug, projectId } = useParams();
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    loader,
    errorMap,
    fetchWarRooms,
    fetchWarRoomSummary,
    getProjectWarRoomIds,
    getProjectSummary,
    getActiveWarRoomByIssue,
  } = useWarRoom();
  const { searchQuery, updateSearchQuery, getStatusTab, setStatusTab } = useWarRoomFilter();
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);
  // refs
  const prefillHandled = useRef(false);

  // derived values
  const workspaceSlugString = workspaceSlug?.toString();
  const projectIdString = projectId?.toString();
  const statusTab = projectIdString ? getStatusTab(projectIdString) : "active";
  const debouncedQuery = useDebounce(searchQuery, 500);
  const roomIds = projectIdString ? getProjectWarRoomIds(projectIdString) : null;
  const summary = projectIdString ? getProjectSummary(projectIdString) : null;
  const hasError = projectIdString ? errorMap[projectIdString] : false;
  const prefillIssue = prefillIssueId ? getIssueById(prefillIssueId) : undefined;

  useEffect(() => {
    if (!workspaceSlugString || !projectIdString) return;
    void fetchWarRooms(workspaceSlugString, projectIdString, {
      status: statusFilterForTab(statusTab),
      q: debouncedQuery.trim() !== "" ? debouncedQuery.trim() : undefined,
    });
  }, [workspaceSlugString, projectIdString, statusTab, debouncedQuery, fetchWarRooms]);

  useEffect(() => {
    if (!workspaceSlugString || !projectIdString) return;
    void fetchWarRoomSummary(workspaceSlugString, projectIdString);
  }, [workspaceSlugString, projectIdString, fetchWarRoomSummary]);

  // `?primary_issue_id=` entry point: open the active room if one exists, otherwise
  // open the create modal prefilled with that incident.
  useEffect(() => {
    if (!prefillIssueId || !workspaceSlugString || !projectIdString || prefillHandled.current) return;
    if (roomIds === null) return;
    prefillHandled.current = true;
    const existingRoom = getActiveWarRoomByIssue(projectIdString, prefillIssueId);
    if (existingRoom) {
      router.replace(getWarRoomLink(workspaceSlugString, projectIdString, existingRoom.id));
      return;
    }
    setIsCreateModalOpen(true);
  }, [prefillIssueId, roomIds, workspaceSlugString, projectIdString, getActiveWarRoomByIssue, router]);

  const closeCreateModal = () => {
    setIsCreateModalOpen(false);
    if (prefillIssueId && workspaceSlugString && projectIdString) {
      router.replace(getWarRoomLink(workspaceSlugString, projectIdString));
    }
  };

  const handleRetry = () => {
    if (!workspaceSlugString || !projectIdString) return;
    void fetchWarRooms(workspaceSlugString, projectIdString, {
      status: statusFilterForTab(statusTab),
      q: debouncedQuery.trim() !== "" ? debouncedQuery.trim() : undefined,
    });
  };

  const handleClearFilters = () => {
    if (!projectIdString) return;
    setStatusTab(projectIdString, "active");
    updateSearchQuery("");
  };

  const renderContent = () => {
    if (hasError) {
      return <WarRoomLoadErrorState onRetry={handleRetry} />;
    }
    if (loader || roomIds === null) {
      return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    }
    const isFiltered = statusTab !== "active" || debouncedQuery.trim() !== "";
    if (roomIds.length === 0 && isFiltered) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("war_room.empty_state.no_matches.title")}</p>
          <p className="text-xs text-secondary">{t("war_room.empty_state.no_matches.description")}</p>
          <Button variant="secondary" size="sm" onClick={handleClearFilters}>
            {t("common.clear_all")}
          </Button>
        </div>
      );
    }
    if (roomIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("war_room.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("war_room.empty_state.description")}</p>
          <Button variant="primary" size="sm" onClick={() => setIsCreateModalOpen(true)}>
            {t("war_room.add")}
          </Button>
        </div>
      );
    }
    return (
      <>
        {summary && <WarRoomSummaryChips summary={summary} />}
        <WarRoomsBoard />
      </>
    );
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center gap-1 border-b border-subtle px-3 py-1.5">
        {WAR_ROOM_STATUS_TABS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => projectIdString && setStatusTab(projectIdString, tab.key)}
            className={cn(
              "rounded-sm px-2 py-1 text-12 font-medium transition-colors",
              statusTab === tab.key ? "bg-layer-2 text-primary" : "text-secondary hover:bg-layer-1"
            )}
          >
            {t(tab.label_key)}
          </button>
        ))}
      </div>
      {renderContent()}
      {workspaceSlugString && projectIdString && (
        <CreateWarRoomModal
          isOpen={isCreateModalOpen}
          onClose={closeCreateModal}
          workspaceSlug={workspaceSlugString}
          projectId={projectIdString}
          initialIssueId={prefillIssueId}
          initialIssue={
            prefillIssue
              ? { id: prefillIssue.id, name: prefillIssue.name, priority: prefillIssue.priority ?? null }
              : undefined
          }
        />
      )}
    </div>
  );
});
