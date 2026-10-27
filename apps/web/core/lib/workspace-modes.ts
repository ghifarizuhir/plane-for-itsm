/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWorkspaceMode = "work" | "procedure" | "galileo" | "settings" | "other";

const trimTrailingSlashes = (pathname: string): string => {
  const trimmed = pathname.replace(/\/+$/, "");
  return trimmed.length > 0 ? trimmed : "/";
};

export const getWorkspaceMode = (pathname: string, workspaceSlug: string): TWorkspaceMode => {
  if (!workspaceSlug) return "other";

  const base = `/${workspaceSlug}`;
  const path = trimTrailingSlashes(pathname);

  if (path === `${base}/settings` || path.startsWith(`${base}/settings/`)) return "settings";
  if (path === `${base}/docs` || path.startsWith(`${base}/docs/`)) return "procedure";
  if (path === `${base}/galileo` || path.startsWith(`${base}/galileo/`)) return "galileo";
  if (path === base || path.startsWith(`${base}/`)) return "work";
  return "other";
};
