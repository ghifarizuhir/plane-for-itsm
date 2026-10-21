/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef } from "react";
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
  // refs
  const lastHandledRef = useRef<{ workspaceSlug?: string; projectParam?: string | null }>({});
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
    const shouldHandle = shouldHandleApplicationScopeParam({
      lastHandledWorkspaceSlug: lastHandledRef.current.workspaceSlug,
      lastHandledProjectParam: lastHandledRef.current.projectParam,
      workspaceSlug,
      projectParam,
      projectsReady,
    });
    if (!shouldHandle) return;
    lastHandledRef.current = { workspaceSlug, projectParam };
    if (resolution.seedProjectId) setProjectId(workspaceSlug, resolution.seedProjectId);
    else if (resolution.shouldClearParam) {
      router.replace(buildApplicationScopeUrl(pathname, searchParams.toString(), null));
    }
  }, [
    workspaceSlug,
    projectsReady,
    projectParam,
    resolution.seedProjectId,
    resolution.shouldClearParam,
    setProjectId,
    router,
    pathname,
    searchParams,
  ]);

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
      lastHandledRef.current = { workspaceSlug, projectParam };
      setProjectId(workspaceSlug, normalized);
      router.replace(buildApplicationScopeUrl(pathname, searchParams.toString(), normalized));
    },
    [workspaceSlug, workspaceMatches, workspaceProjectIds, setProjectId, router, pathname, searchParams, projectParam]
  );

  const clearApplication = useCallback(() => setApplication(null), [setApplication]);

  const project = projectId ? getProjectById(projectId) : undefined;

  return { workspaceSlug, projectId, project, setApplication, clearApplication };
};
