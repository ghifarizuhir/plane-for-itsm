/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { getEditorAssetSrc, getFileURL } from "@plane/utils";

export type TDocumentImageContext = {
  workspaceSlug: string;
  projectId?: string;
};

export const resolveDocumentImageSrc = (src: string, context: TDocumentImageContext): string | null => {
  const trimmed = src?.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("http") || trimmed.startsWith("data:")) return trimmed;
  if (trimmed.startsWith("/")) return getFileURL(trimmed) ?? null;
  return (
    getEditorAssetSrc({
      assetId: trimmed,
      projectId: context.projectId,
      workspaceSlug: context.workspaceSlug,
    }) ?? null
  );
};
