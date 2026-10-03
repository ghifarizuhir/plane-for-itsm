/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { useTranslation } from "@plane/i18n";
import { IconButton } from "@plane/propel/icon-button";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { usePlatformOS } from "@/hooks/use-platform-os";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";

type Props = {
  workspaceSlug: string;
  projectId: string;
  issueId: string;
};

export const IssueWarRoomButton = observer(function IssueWarRoomButton({ workspaceSlug, projectId, issueId }: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  const { isMobile } = usePlatformOS();
  // store hooks
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();
  // derived values
  const issue = getIssueById(issueId);

  useEffect(() => {
    if (workItemTypes) return;
    void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
  }, [workItemTypes, workspaceSlug, fetchWorkItemTypes]);

  const issueType = workItemTypes?.find((type) => type.id === issue?.type_id);
  if (!issue || issue.archived_at || issueType?.name.toLowerCase() !== "incident") return <></>;

  const handleOpenWarRoom = () => {
    router.push(`/${workspaceSlug}/projects/${projectId}/war-rooms?primary_issue_id=${issueId}`);
  };

  return (
    <Tooltip label={t("war_room.open")} disabled={isMobile}>
      <IconButton variant="secondary" size="lg" onClick={handleOpenWarRoom} icon={AlertOctagonOutline} />
    </Tooltip>
  );
});
