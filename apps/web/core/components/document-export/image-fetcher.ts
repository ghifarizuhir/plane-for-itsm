/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { API_BASE_URL } from "@plane/constants";
import { getEditorAssetSrc, getFileURL } from "@plane/utils";
// local imports
import type { TDocxImage, TImageFetcher } from "./html-to-docx";

export type TDocumentImageContext = {
  workspaceSlug: string;
  projectId?: string;
};

const DOCX_PASSTHROUGH_TYPES: Record<string, TDocxImage["type"]> = {
  "image/jpeg": "jpg",
  "image/jpg": "jpg",
  "image/png": "png",
  "image/gif": "gif",
};

const toUint8Array = async (blob: Blob): Promise<Uint8Array> => new Uint8Array(await blob.arrayBuffer());

const reencodeAsPng = async (bitmap: ImageBitmap): Promise<Uint8Array | null> => {
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) return null;
    context.drawImage(bitmap, 0, 0);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    return blob ? toUint8Array(blob) : null;
  } catch {
    return null;
  }
};

export const loadDocumentImage = async (src: string, context: TDocumentImageContext): Promise<TDocxImage | null> => {
  const url = resolveDocumentImageSrc(src, context);
  if (!url) return null;
  try {
    const credentials = API_BASE_URL !== "" && url.startsWith(API_BASE_URL) ? "include" : "same-origin";
    const response = await fetch(url, { credentials });
    if (!response.ok) return null;
    const blob = await response.blob();
    const mime = (blob.type || "").toLowerCase();
    if (mime === "image/svg+xml") return null;
    const bitmap = await createImageBitmap(blob);
    const passthroughType = DOCX_PASSTHROUGH_TYPES[mime];
    const data = passthroughType ? await toUint8Array(blob) : await reencodeAsPng(bitmap);
    if (!data) return null;
    return { data, width: bitmap.width, height: bitmap.height, type: passthroughType ?? "png" };
  } catch {
    return null;
  }
};

export const createDocumentImageFetcher = (context: TDocumentImageContext): TImageFetcher => {
  const cache = new Map<string, Promise<TDocxImage | null>>();
  return (src) => {
    const cached = cache.get(src);
    if (cached) return cached;
    const promise = loadDocumentImage(src, context);
    cache.set(src, promise);
    return promise;
  };
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
