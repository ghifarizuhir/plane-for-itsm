/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WarRoomsListView } from "@/components/war-rooms";
// hooks
import { useProject } from "@/hooks/store/use-project";
import type { Route } from "./+types/page";

function ProjectWarRoomsPage({ params }: Route.ComponentProps) {
  const { projectId } = params;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  // query params (entry point from an Incident work item)
  const searchParams = useSearchParams();
  const prefillIssueId = searchParams.get("primary_issue_id") ?? undefined;
  // derived values
  const project = getProjectById(projectId);
  const pageTitle = project?.name ? `${project.name} - ${t("war_room.title")}` : undefined;

  return (
    <>
      <PageHead title={pageTitle} />
      <WarRoomsListView prefillIssueId={prefillIssueId} />
    </>
  );
}

export default observer(ProjectWarRoomsPage);
