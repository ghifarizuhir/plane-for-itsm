/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// components
import { PageHead } from "@/components/core/page-title";
import { ServiceDetailRoot } from "@/components/services";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

function WorkspaceServiceDetailPage() {
  const { workspaceSlug, serviceId } = useParams();
  const { currentWorkspace } = useWorkspace();
  const { workspaceFetched, fetchWorkspaceServices, getServiceById } = useService();
  const service = getServiceById(serviceId?.toString() ?? "");
  const workspaceId = currentWorkspace?.id;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || workspaceFetched) return;
    void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId);
  }, [workspaceSlug, workspaceId, workspaceFetched, fetchWorkspaceServices]);

  return (
    <>
      <PageHead title={service ? `${service.name} - Service` : undefined} />
      <ServiceDetailRoot serviceId={serviceId?.toString() ?? ""} fetched={workspaceFetched} />
    </>
  );
}

export default observer(WorkspaceServiceDetailPage);
