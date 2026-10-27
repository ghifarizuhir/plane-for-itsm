/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams, usePathname } from "next/navigation";
import { getWorkspaceMode } from "@/lib/workspace-modes";

/**
 * Custom hook to detect different workspace paths.
 * `isProjectsPath` is kept as a legacy alias for Work routes.
 */
export const useWorkspacePaths = () => {
  const { workspaceSlug } = useParams();
  const pathname = usePathname();
  const slug = workspaceSlug?.toString() ?? "";
  const mode = getWorkspaceMode(pathname ?? "", slug);

  const isSettingsPath = mode === "settings";
  const isDocsPath = mode === "procedure";
  const isGalileoPath = mode === "galileo";
  const isWorkPath = mode === "work";
  const isProjectsPath = isWorkPath;
  const isWikiPath = pathname.includes(`/${slug}/wiki`);
  const isAiPath = pathname.includes(`/${slug}/pi-chat`);
  const isNotificationsPath = pathname.includes(`/${slug}/notifications`);

  return {
    isSettingsPath,
    isWikiPath,
    isAiPath,
    isProjectsPath,
    isWorkPath,
    isDocsPath,
    isGalileoPath,
    isNotificationsPath,
  };
};
