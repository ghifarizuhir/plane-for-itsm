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
import { buildWorkspaceServiceGroups } from "@/components/services/workspace/service-groups";
import { ServiceGraph } from "@/components/services/graph/service-graph";
import { ServiceLayoutIcon, SERVICE_VIEW_LAYOUTS } from "@/components/services/service-layout-icon";
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
  const [layout, setLayout] = useState<"board" | "graph">("board");
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
  const groups = buildWorkspaceServiceGroups(filteredIds, getServiceById, getProjectById);

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
          <div className="hidden items-center gap-1 rounded-sm bg-layer-3 p-1 md:flex">
            {SERVICE_VIEW_LAYOUTS.map((layoutOption) => (
              <button
                key={layoutOption.key}
                type="button"
                title={t(layoutOption.i18n_label)}
                disabled={layoutOption.key === "graph" && !projectFilter}
                className="grid h-5.5 w-7 place-items-center overflow-hidden rounded-sm transition-colors hover:bg-layer-transparent-hover disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => setLayout(layoutOption.key as "board" | "graph")}
              >
                <ServiceLayoutIcon layoutType={layoutOption.key} />
              </button>
            ))}
          </div>
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
      ) : layout === "graph" && projectFilter ? (
        <div className="min-h-0 w-full flex-1">
          <ServiceGraph scope="workspace" projectIdOverride={projectFilter} />
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {groups.map((group) => (
            <section key={group.key}>
              <div className="sticky top-0 z-[5] flex items-center justify-between gap-2 border-b border-subtle bg-layer-2 px-3 py-1">
                <div className="flex min-w-0 items-center gap-1.5">
                  {group.identifier && (
                    <span className="shrink-0 rounded-xs border border-subtle px-1 text-9 font-medium tracking-wide text-tertiary">
                      {group.identifier}
                    </span>
                  )}
                  <span className="truncate text-11 font-medium text-secondary">
                    {group.projectId ? group.name : t("common.unassigned")}
                  </span>
                </div>
                <span className="shrink-0 rounded-xs bg-layer-3 px-1.5 text-9 text-tertiary tabular-nums">
                  {group.serviceIds.length}
                </span>
              </div>
              {group.serviceIds.map((id) => (
                <ServicesBoardRow key={id} serviceId={id} showProject={false} showOwner={false} />
              ))}
            </section>
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
