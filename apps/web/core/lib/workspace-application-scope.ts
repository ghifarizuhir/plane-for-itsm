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
