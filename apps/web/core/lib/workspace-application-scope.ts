/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const buildApplicationScopeUrl = (pathname: string, queryString: string, projectId: string | null): string => {
  const params = new URLSearchParams(queryString);
  if (projectId) params.set("project", projectId);
  else params.delete("project");
  const query = params.toString();
  return `${pathname}${query ? `?${query}` : ""}`;
};

export type TWorkspaceApplicationScopeResolution = {
  projectId: string | null;
  seedProjectId: string | null;
  shouldClearParam: boolean;
};

export const resolveWorkspaceApplicationScope = ({
  storedProjectId,
  paramProjectId,
  workspaceProjectIds,
  projectsReady,
}: {
  storedProjectId: string | null;
  paramProjectId: string | null;
  workspaceProjectIds: string[] | undefined;
  projectsReady: boolean;
}): TWorkspaceApplicationScopeResolution => {
  const stored = storedProjectId || null;
  const param = paramProjectId || null;
  if (!projectsReady || workspaceProjectIds === undefined) {
    return { projectId: stored, seedProjectId: null, shouldClearParam: false };
  }
  const validStored = stored && workspaceProjectIds.includes(stored) ? stored : null;
  const validParam = param && workspaceProjectIds.includes(param) ? param : null;
  return {
    projectId: validStored,
    seedProjectId: validParam && validParam !== stored ? validParam : null,
    shouldClearParam: Boolean(param && !validParam && workspaceProjectIds.length > 0),
  };
};

export const shouldHandleApplicationScopeParam = ({
  lastHandledWorkspaceSlug,
  lastHandledProjectParam,
  workspaceSlug,
  projectParam,
  projectsReady,
}: {
  lastHandledWorkspaceSlug?: string;
  lastHandledProjectParam?: string | null;
  workspaceSlug: string | undefined;
  projectParam: string | null;
  projectsReady: boolean;
}): boolean => {
  if (!workspaceSlug || !projectsReady) return false;
  return !(lastHandledWorkspaceSlug === workspaceSlug && lastHandledProjectParam === projectParam);
};
