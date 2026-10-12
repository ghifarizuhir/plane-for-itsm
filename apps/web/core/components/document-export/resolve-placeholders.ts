/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const DOCUMENT_PLACEHOLDER_KEYS = [
  "workspace_name",
  "document_title",
  "document_id",
  "export_date",
  "exported_by",
] as const;

export type TDocumentPlaceholderKey = (typeof DOCUMENT_PLACEHOLDER_KEYS)[number];
export type TDocumentPlaceholderValues = Record<TDocumentPlaceholderKey, string>;

export function resolvePlaceholders(text: string, values: TDocumentPlaceholderValues): string {
  return text.replace(/\{([a-z_]+)\}/g, (match, key: string) =>
    (DOCUMENT_PLACEHOLDER_KEYS as readonly string[]).includes(key) ? values[key as TDocumentPlaceholderKey] : match
  );
}
