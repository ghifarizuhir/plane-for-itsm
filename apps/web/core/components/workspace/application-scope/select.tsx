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
import { CustomSelect } from "@plane/ui";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const WorkspaceApplicationScopeSelect = observer(function WorkspaceApplicationScopeSelect() {
  // router
  const { projectId: routeProjectId } = useParams();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  // scope
  const { projectId, setApplication, clearApplication } = useWorkspaceApplicationScope();

  if (routeProjectId) return null;
  if (workspaceProjectIds === undefined) return null;

  const selectedProject = projectId ? getProjectById(projectId) : undefined;

  return (
    <div className="flex flex-col gap-1 px-2">
      <span className="tracking-wider text-[11px] font-semibold text-placeholder uppercase">Application</span>
      <div className="flex min-w-0 items-center gap-1">
        <CustomSelect
          value={projectId ?? ""}
          onChange={(value: string) => setApplication(value || null)}
          className="min-w-0 flex-1"
          buttonClassName="w-full border border-subtle bg-layer-1"
          input
          label={
            <span className="flex min-w-0 items-center gap-1.5 py-0.5 text-12">
              <ProjectsOutline className="size-3.5 flex-shrink-0 text-tertiary" />
              <span className="truncate">{selectedProject?.name ?? t("service.catalog_all_projects")}</span>
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
            aria-label="Clear application scope"
            className="grid size-6 flex-shrink-0 place-items-center rounded-sm text-tertiary hover:bg-layer-transparent-hover hover:text-primary"
          >
            <CloseOutline className="size-3" />
          </button>
        )}
      </div>
    </div>
  );
});
