/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse, IWarRoom } from "@plane/types";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
// helpers
import { getWarRoomIncidentLink } from "@/services/war-room.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomWorkItems = observer(function WarRoomWorkItems({
  workspaceSlug,
  projectId,
  room,
  canWrite,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { addIssues, removeIssue } = useWarRoom();
  // states
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  // derived values
  const hiddenIssueIds = new Set([room.primary_issue_id, ...room.issues.map((issue) => issue.id)]);
  const isEmpty = !room.primary_issue && room.issues.length === 0;

  const handleAdd = async (issues: ISearchIssueResponse[]) => {
    const issueIds = issues.map((issue) => issue.id).filter((issueId) => !hiddenIssueIds.has(issueId));
    if (issueIds.length === 0) {
      setIsPickerOpen(false);
      return;
    }
    try {
      await addIssues(workspaceSlug, projectId, room.id, issueIds);
      setIsPickerOpen(false);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.work_items.link_failed") });
    }
  };

  const handleUnlink = async (issueId: string) => {
    try {
      await removeIssue(workspaceSlug, projectId, room.id, issueId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.work_items.unlink_failed") });
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.work_items")}
        </span>
        {canWrite && (
          <Button variant="secondary" size="sm" onClick={() => setIsPickerOpen(true)}>
            {t("war_room.work_items.add")}
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-3">
        {isEmpty && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.work_items.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.work_items.empty_description")}</p>
          </div>
        )}
        {room.primary_issue && (
          <div className="rounded-md border border-subtle bg-layer-1 p-2">
            <span className="rounded-full bg-accent-subtle px-1.5 py-0.5 text-10 font-medium text-accent-primary">
              {t("war_room.work_items.primary_badge")}
            </span>
            <Link
              href={getWarRoomIncidentLink(workspaceSlug, room.primary_issue.identifier)}
              className="mt-1 flex items-center gap-2 text-13 text-primary hover:underline"
            >
              <span className="shrink-0 font-medium text-tertiary">{room.primary_issue.identifier}</span>
              <span className="truncate">{room.primary_issue.name}</span>
            </Link>
          </div>
        )}
        {room.issues.map((issue) => (
          <div
            key={issue.id}
            className="flex items-center justify-between gap-2 rounded-md border border-subtle p-2 hover:bg-layer-transparent-hover"
          >
            <Link
              href={getWarRoomIncidentLink(workspaceSlug, issue.identifier)}
              className="flex min-w-0 flex-1 items-center gap-2 text-13 text-primary hover:underline"
            >
              <span className="shrink-0 font-medium text-tertiary">{issue.identifier}</span>
              <span className="truncate">{issue.name}</span>
            </Link>
            {canWrite && (
              <Button variant="tertiary" size="sm" onClick={() => void handleUnlink(issue.id)}>
                {t("war_room.work_items.unlink")}
              </Button>
            )}
          </div>
        ))}
      </div>
      <ExistingIssuesListModal
        isOpen={isPickerOpen}
        handleClose={() => setIsPickerOpen(false)}
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        searchParams={{ workspace_search: false }}
        shouldHideIssue={(issue) => hiddenIssueIds.has(issue.id)}
        handleOnSubmit={handleAdd}
      />
    </div>
  );
});
