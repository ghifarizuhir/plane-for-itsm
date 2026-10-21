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
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { buildApplicationScopeUrl, resolveWorkspaceApplicationScope } from "@/lib/workspace-application-scope";

export const useWorkspaceApplicationScope = () => {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useAppRouter();
  const projectParam = searchParams.get("project");
  // store hooks
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { getProjectId, setProjectId } = useApplicationScope();
  // refs
  const lastHandledRef = useRef<{ workspaceSlug?: string; projectParam?: string | null }>({});
  const pendingRef = useRef<string | null | undefined>(undefined);
  // derived values
  const projectsReady = !isInitializingProjects && workspaceProjectIds !== undefined;
  const storedProjectId = workspaceSlug ? getProjectId(workspaceSlug) : null;
  const resolution = resolveWorkspaceApplicationScope({
    storedProjectId,
    paramProjectId: projectParam,
    workspaceProjectIds,
    projectsReady,
  });
  const projectId = pendingRef.current !== undefined ? pendingRef.current : resolution.projectId;

  // Seed from `?project=` once per URL value, and discard scopes that no longer
  // belong to this workspace. Never let a store write re-seed from a stale URL.
  useEffect(() => {
    if (!workspaceSlug || !projectsReady) return;
    const alreadyHandled =
      lastHandledRef.current.workspaceSlug === workspaceSlug && lastHandledRef.current.projectParam === projectParam;
    if (alreadyHandled) return;
    lastHandledRef.current = { workspaceSlug, projectParam };
    if (resolution.seedProjectId) setProjectId(workspaceSlug, resolution.seedProjectId);
    else if (resolution.shouldClearStored) setProjectId(workspaceSlug, null);
  }, [
    workspaceSlug,
    projectsReady,
    projectParam,
    resolution.seedProjectId,
    resolution.shouldClearStored,
    setProjectId,
  ]);

  // Keep an explicit selection stable until the deferred router.replace lands.
  useEffect(() => {
    if (pendingRef.current === undefined) return;
    if (projectParam === (pendingRef.current ?? null)) pendingRef.current = undefined;
  }, [projectParam]);

  const setApplication = useCallback(
    (nextProjectId: string | null) => {
      if (!workspaceSlug) return;
      const normalized =
        nextProjectId && workspaceProjectIds && !workspaceProjectIds.includes(nextProjectId) ? null : nextProjectId;
      pendingRef.current = normalized;
      lastHandledRef.current = { workspaceSlug, projectParam };
      setProjectId(workspaceSlug, normalized);
      router.replace(buildApplicationScopeUrl(pathname, searchParams.toString(), normalized));
    },
    [workspaceSlug, workspaceProjectIds, setProjectId, router, pathname, searchParams, projectParam]
  );

  const clearApplication = useCallback(() => setApplication(null), [setApplication]);

  const project = projectId ? getProjectById(projectId) : undefined;

  return { workspaceSlug, projectId, project, setApplication, clearApplication };
};
