/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { useParams, usePathname, useSearchParams } from "next/navigation";
// hooks
import { useApplicationScope } from "@/hooks/store/use-application-scope";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { buildApplicationScopeUrl } from "@/lib/workspace-application-scope";

export const useWorkspaceApplicationScope = () => {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useAppRouter();
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  const { getProjectId, setProjectId } = useApplicationScope();
  // derived values
  const storedProjectId = workspaceSlug ? getProjectId(workspaceSlug) : null;
  const projectId = storedProjectId && workspaceProjectIds?.includes(storedProjectId) ? storedProjectId : null;

  // Seed from `?project=` and discard scopes that no longer belong to this workspace.
  useEffect(() => {
    if (!workspaceSlug || workspaceProjectIds === undefined) return;
    const paramProjectId = searchParams.get("project");
    if (paramProjectId && workspaceProjectIds.includes(paramProjectId)) {
      if (storedProjectId !== paramProjectId) setProjectId(workspaceSlug, paramProjectId);
      return;
    }
    if (storedProjectId && !workspaceProjectIds.includes(storedProjectId)) {
      setProjectId(workspaceSlug, null);
    }
  }, [workspaceSlug, workspaceProjectIds, searchParams, storedProjectId, setProjectId]);

  const setApplication = useCallback(
    (nextProjectId: string | null) => {
      if (!workspaceSlug) return;
      setProjectId(workspaceSlug, nextProjectId);
      router.replace(buildApplicationScopeUrl(pathname, searchParams.toString(), nextProjectId));
    },
    [workspaceSlug, setProjectId, router, pathname, searchParams]
  );

  const clearApplication = useCallback(() => setApplication(null), [setApplication]);

  const project = projectId ? getProjectById(projectId) : undefined;

  return { workspaceSlug, projectId, project, setApplication, clearApplication };
};
