/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// icons
import { CloseOutline, ProjectsOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
// hooks
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const WorkspaceApplicationScopeChip = observer(function WorkspaceApplicationScopeChip() {
  // i18n
  const { t } = useTranslation();
  const { project, clearApplication } = useWorkspaceApplicationScope();

  if (!project) return null;

  return (
    <div className="flex max-w-[220px] items-center gap-1 rounded-sm border border-subtle bg-layer-1 px-2 py-0.5 text-11 text-secondary">
      <ProjectsOutline className="size-3 flex-shrink-0 text-tertiary" />
      <span className="truncate text-tertiary">{t("common.project")}:</span>
      <span className="truncate" title={project.name}>
        {project.name}
      </span>
      <button
        type="button"
        onClick={clearApplication}
        aria-label={t("common.clear")}
        className="grid size-5 flex-shrink-0 place-items-center rounded-xs hover:bg-layer-2"
      >
        <CloseOutline className="size-3" />
      </button>
    </div>
  );
});
