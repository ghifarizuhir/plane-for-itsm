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
  shouldClearStored: boolean;
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
  if (!projectsReady || workspaceProjectIds === undefined) {
    return { projectId: storedProjectId, seedProjectId: null, shouldClearStored: false };
  }
  const validStored = storedProjectId && workspaceProjectIds.includes(storedProjectId) ? storedProjectId : null;
  const validParam = paramProjectId && workspaceProjectIds.includes(paramProjectId) ? paramProjectId : null;
  return {
    projectId: validStored,
    seedProjectId: validParam && validParam !== storedProjectId ? validParam : null,
    shouldClearStored: Boolean(storedProjectId && !validStored),
  };
};
