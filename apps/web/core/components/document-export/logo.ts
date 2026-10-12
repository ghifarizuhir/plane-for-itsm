/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IWorkspace, TDocumentExportSettings } from "@plane/types";
import { getBase64Image } from "@plane/utils";
import type { TDocxImage } from "./html-to-docx";

const dataUrlToUint8Array = (dataUrl: string): Uint8Array => {
  const binary = atob(dataUrl.split(",")[1] ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
};

const loadImage = async (src: string | null | undefined): Promise<TDocxImage | null> => {
  if (!src) return null;
  try {
    const dataUrl = await getBase64Image(src);
    const mime = dataUrl.slice(5, dataUrl.indexOf(";"));
    if (mime === "image/svg+xml") return null;
    const type = mime === "image/jpeg" ? "jpg" : mime === "image/gif" ? "gif" : "png";
    const data = dataUrlToUint8Array(dataUrl);
    const bitmap = await createImageBitmap(await (await fetch(dataUrl)).blob());
    return { data, width: bitmap.width, height: bitmap.height, type };
  } catch {
    return null;
  }
};

export async function resolveDocumentLogo(
  settings: Pick<TDocumentExportSettings, "header_logo_mode" | "header_logo_url">,
  workspace: IWorkspace | null | undefined
): Promise<TDocxImage | null> {
  if (settings.header_logo_mode === "workspace") return loadImage(workspace?.logo_url);
  if (settings.header_logo_mode === "custom") return loadImage(settings.header_logo_url);
  return null;
}
