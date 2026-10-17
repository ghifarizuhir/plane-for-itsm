/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceServicesListView } from "@/components/services/workspace/workspace-services-list-view";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

function WorkspaceServicesPage() {
  const { workspaceSlug } = useParams();
  const searchParams = useSearchParams();
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();
  const { currentWorkspace } = useWorkspace();
  const { fetchWorkspaceServices } = useService();
  const workspaceId = currentWorkspace?.id;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId) return;
    void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId, projectFilter);
  }, [workspaceSlug, workspaceId, projectFilter, fetchWorkspaceServices]);

  return (
    <>
      <PageHead title={t("service.catalog_title")} />
      <WorkspaceServicesListView projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceServicesPage);
