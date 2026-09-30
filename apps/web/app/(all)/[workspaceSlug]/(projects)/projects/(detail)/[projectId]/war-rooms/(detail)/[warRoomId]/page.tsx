/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WarRoomRoot } from "@/components/war-rooms";
// hooks
import { useProject } from "@/hooks/store/use-project";
import type { Route } from "./+types/page";

function ProjectWarRoomDetailPage({ params }: Route.ComponentProps) {
  const { workspaceSlug, projectId, warRoomId } = params;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  // derived values
  const project = getProjectById(projectId);
  const pageTitle = project?.name ? `${project.name} - ${t("war_room.title")}` : undefined;

  return (
    <>
      <PageHead title={pageTitle} />
      <WarRoomRoot workspaceSlug={workspaceSlug} projectId={projectId} warRoomId={warRoomId} />
    </>
  );
}

export default observer(ProjectWarRoomDetailPage);
