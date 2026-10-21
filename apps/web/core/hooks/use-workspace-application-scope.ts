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
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import {
  buildApplicationScopeUrl,
  resolveWorkspaceApplicationScope,
  shouldHandleApplicationScopeParam,
} from "@/lib/workspace-application-scope";

const handledScopeParams = new Map<string, string | null>();

export const useWorkspaceApplicationScope = () => {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useAppRouter();
  const projectParam = searchParams.get("project");
  // store hooks
  const { currentWorkspace } = useWorkspace();
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { getProjectId, setProjectId } = useApplicationScope();
  // derived values
  const workspaceMatches = !workspaceSlug || currentWorkspace?.slug === workspaceSlug;
  const projectsReady = workspaceMatches && !isInitializingProjects && workspaceProjectIds !== undefined;
  const storedProjectId = workspaceSlug ? getProjectId(workspaceSlug) : null;
  const resolution = resolveWorkspaceApplicationScope({
    storedProjectId,
    paramProjectId: projectParam,
    workspaceProjectIds,
    projectsReady,
  });
  const projectId = resolution.projectId;

  // Seed from `?project=` once per URL value. Never let a store write re-seed
  // from a stale URL, and never delete a persisted scope on a transient failure.
  useEffect(() => {
    if (!workspaceSlug || !projectsReady) return;
    const hasHandledParam = handledScopeParams.has(workspaceSlug);
    const shouldHandle = shouldHandleApplicationScopeParam({
      hasHandledParam,
      lastHandledProjectParam: handledScopeParams.get(workspaceSlug),
      workspaceSlug,
      projectParam,
      projectsReady,
    });
    if (!shouldHandle) return;
    if (resolution.seedProjectId) {
      handledScopeParams.set(workspaceSlug, projectParam);
      setProjectId(workspaceSlug, resolution.seedProjectId);
      return;
    }
    const isParamAuthoritative = !projectParam || Boolean(workspaceProjectIds?.includes(projectParam));
    if (isParamAuthoritative) handledScopeParams.set(workspaceSlug, projectParam);
  }, [workspaceSlug, projectsReady, projectParam, resolution.seedProjectId, workspaceProjectIds, setProjectId]);

  const setApplication = useCallback(
    (nextProjectId: string | null) => {
      if (!workspaceSlug || !workspaceMatches) return;
      let normalized = nextProjectId || null;
      if (normalized && workspaceProjectIds && !workspaceProjectIds.includes(normalized)) {
        normalized = null;
      }
      if ((projectParam || null) === normalized) {
        setProjectId(workspaceSlug, normalized);
        return;
      }
      handledScopeParams.set(workspaceSlug, projectParam);
      setProjectId(workspaceSlug, normalized);
      router.replace(buildApplicationScopeUrl(pathname, searchParams.toString(), normalized));
    },
    [workspaceSlug, workspaceMatches, workspaceProjectIds, setProjectId, router, pathname, searchParams, projectParam]
  );

  const clearApplication = useCallback(() => setApplication(null), [setApplication]);

  const project = projectId ? getProjectById(projectId) : undefined;

  return { workspaceSlug, projectId, project, setApplication, clearApplication };
};
