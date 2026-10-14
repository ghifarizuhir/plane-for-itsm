/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TDocumentExportSettings } from "@plane/types";
import type { TDocxImage, TImageFetcher } from "./html-to-docx";

export async function resolveDocumentLogo(
  settings: Pick<TDocumentExportSettings, "header_logo_mode" | "header_logo_url" | "header_logo_asset_id">,
  workspace: { logo_url?: string | null } | null | undefined,
  imageFetcher: TImageFetcher
): Promise<TDocxImage | null> {
  if (settings.header_logo_mode === "workspace") {
    return workspace?.logo_url ? imageFetcher(workspace.logo_url) : null;
  }
  if (settings.header_logo_mode === "custom") {
    const src = settings.header_logo_asset_id ?? settings.header_logo_url;
    return src ? imageFetcher(src) : null;
  }
  return null;
}
