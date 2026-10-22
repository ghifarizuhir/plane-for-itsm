/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import { createContext, useCallback, useContext, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EPageAccess } from "@plane/constants";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// components
import { WorkspaceApplicationPickerModal } from "@/components/workspace/application-scope/project-picker-modal";
// hooks
import { EPageStoreType, usePageStore } from "@/hooks/store";
import { useProject } from "@/hooks/store/use-project";
import { useUser } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspacePagePermissions } from "@/hooks/use-workspace-page-permissions";

type TWorkspacePagesCreateContext = {
  canCreatePage: boolean;
  isCreatingPage: boolean;
  requestCreatePage: (access?: EPageAccess) => Promise<void>;
};

const WorkspacePagesCreateContext = createContext<TWorkspacePagesCreateContext | null>(null);

export const useWorkspacePagesCreate = () => {
  const context = useContext(WorkspacePagesCreateContext);
  if (!context) throw new Error("useWorkspacePagesCreate must be used within WorkspacePagesCreateProvider");
  return context;
};

export const WorkspacePagesCreateProvider = observer(function WorkspacePagesCreateProvider({
  children,
}: {
  children: ReactNode;
}) {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // scope
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  // store hooks
  const { createPage } = usePageStore(EPageStoreType.WORKSPACE);
  const { workspaceProjectIds, getProjectById } = useProject();
  const { projectsWithCreatePermissions } = useUser();
  const { canCreatePage } = useWorkspacePagePermissions();
  // states
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [pendingAccess, setPendingAccess] = useState<EPageAccess>(EPageAccess.PUBLIC);
  const [isCreatingPage, setIsCreatingPage] = useState(false);

  const eligibleProjectIds = useMemo(
    () =>
      (workspaceProjectIds ?? []).filter((projectId) => {
        const project = getProjectById(projectId);
        return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.page_view !== false;
      }),
    [getProjectById, projectsWithCreatePermissions, workspaceProjectIds]
  );

  const createInProject = useCallback(
    async (projectId: string, access: EPageAccess) => {
      if (!workspaceSlug) return;
      setIsCreatingPage(true);
      try {
        const page = await createPage({ access }, projectId);
        if (page?.id) router.push(`/${workspaceSlug}/projects/${projectId}/pages/${page.id}`);
      } catch (error) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message:
            (error as { data?: { error?: string } })?.data?.error || "Page could not be created. Please try again.",
        });
      } finally {
        setIsCreatingPage(false);
      }
    },
    [createPage, router, workspaceSlug]
  );

  const requestCreatePage = useCallback(
    async (access: EPageAccess = EPageAccess.PUBLIC) => {
      if (scopedProjectId) {
        await createInProject(scopedProjectId, access);
        return;
      }
      setPendingAccess(access);
      setIsPickerOpen(true);
    },
    [createInProject, scopedProjectId]
  );

  const handleClose = () => {
    setIsPickerOpen(false);
  };

  const handleSelect = (projectId: string) => {
    setIsPickerOpen(false);
    void createInProject(projectId, pendingAccess);
  };

  const value = useMemo(
    () => ({ canCreatePage, isCreatingPage, requestCreatePage }),
    [canCreatePage, isCreatingPage, requestCreatePage]
  );

  return (
    <WorkspacePagesCreateContext.Provider value={value}>
      {children}
      <WorkspaceApplicationPickerModal
        isOpen={isPickerOpen}
        handleClose={handleClose}
        projectIds={eligibleProjectIds}
        onSelect={handleSelect}
      />
    </WorkspacePagesCreateContext.Provider>
  );
});
