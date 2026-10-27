/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { CloseOutline, ProjectsOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { getButtonStyling } from "@plane/propel/button";
import { CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const WorkspaceApplicationScopeSelect = observer(function WorkspaceApplicationScopeSelect() {
  // router
  const { projectId: routeProjectId } = useParams();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  // scope
  const { projectId, setApplication, clearApplication } = useWorkspaceApplicationScope();

  if (routeProjectId) return null;
  if (isInitializingProjects || workspaceProjectIds === undefined) return null;

  const selectedProject = projectId ? getProjectById(projectId) : undefined;
  const selectedLabel = selectedProject?.name ?? t("service.catalog_all_projects");

  return (
    <div className="flex min-w-0 items-center gap-1">
      <CustomSelect
        value={projectId ?? ""}
        onChange={(value: string) => setApplication(value || null)}
        className="min-w-0 flex-1"
        buttonClassName={cn(getButtonStyling("secondary", "xl"), "w-full justify-between py-0")}
        input
        label={
          <span className="flex min-w-0 items-center gap-1.5 text-13 font-medium" title={selectedLabel}>
            <ProjectsOutline className="size-4 flex-shrink-0" />
            <span className="truncate">{selectedLabel}</span>
          </span>
        }
      >
        <CustomSelect.Option value="">{t("service.catalog_all_projects")}</CustomSelect.Option>
        {workspaceProjectIds.map((id) => {
          const project = getProjectById(id);
          return (
            <CustomSelect.Option key={id} value={id}>
              <span className="truncate">{project?.name}</span>
            </CustomSelect.Option>
          );
        })}
      </CustomSelect>
      {projectId && (
        <button
          type="button"
          onClick={clearApplication}
          aria-label={t("common.clear")}
          className="grid size-8 flex-shrink-0 place-items-center rounded-md text-tertiary hover:bg-layer-transparent-hover hover:text-primary"
        >
          <CloseOutline className="size-3.5" />
        </button>
      )}
    </div>
  );
});
