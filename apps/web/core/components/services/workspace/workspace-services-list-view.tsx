/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// ui
import { CustomSelect } from "@plane/ui";
// components
import { ServicesBoardRow } from "@/components/services/board/services-board-row";
import { WorkspaceCreateServiceModal } from "@/components/services/workspace/create-service-modal";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  projectFilter?: string;
};

export const WorkspaceServicesListView = observer(function WorkspaceServicesListView({ projectFilter }: Props) {
  // router
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWorkspaceServiceIds, getServiceById, loader, workspaceFetched } = useService();
  const { workspaceProjectIds, getProjectById } = useProject();
  // states
  const [search, setSearch] = useState("");
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const serviceIds = getWorkspaceServiceIds();
  const filteredIds = useMemo(
    () =>
      serviceIds.filter((id) => {
        const service = getServiceById(id);
        return !search || (service?.name ?? "").toLowerCase().includes(search.toLowerCase());
      }),
    [serviceIds, search, getServiceById]
  );

  const handleProjectFilter = (projectId: string) => {
    if (!workspaceSlug) return;
    router.push(projectId ? `/${workspaceSlug}/services?project=${projectId}` : `/${workspaceSlug}/services`);
  };

  if (loader && !workspaceFetched) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center justify-between gap-2 border-b border-subtle px-3 py-2">
        <div className="flex items-center gap-2">
          <CustomSelect
            value={projectFilter ?? ""}
            onChange={handleProjectFilter}
            label={
              <span className="flex items-center gap-2 py-0.5 text-12">
                {projectFilter ? getProjectById(projectFilter)?.name : t("service.catalog_all_projects")}
              </span>
            }
            noChevron
          >
            <CustomSelect.Option value="">{t("service.catalog_all_projects")}</CustomSelect.Option>
            {(workspaceProjectIds ?? []).map((id) => {
              const project = getProjectById(id);
              return (
                <CustomSelect.Option key={id} value={id}>
                  <span className="truncate">{project?.name}</span>
                </CustomSelect.Option>
              );
            })}
          </CustomSelect>
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("service.catalog_search_placeholder")}
            className="h-7 rounded-sm border border-subtle bg-layer-1 px-2 text-12 text-primary outline-none"
          />
        </div>
        <Button variant="primary" size="sm" onClick={() => setIsCreateModalOpen(true)}>
          {t("service.add")}
        </Button>
      </div>
      {filteredIds.length === 0 ? (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {filteredIds.map((id) => (
            <ServicesBoardRow key={id} serviceId={id} />
          ))}
        </div>
      )}
      {workspaceSlug && (
        <WorkspaceCreateServiceModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
          defaultOwnerProjectId={projectFilter}
        />
      )}
    </div>
  );
});
