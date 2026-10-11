/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TDocumentExportTextAlign = "left" | "center" | "right";

export type TDocumentExportTextLine = {
  text: string;
  align: TDocumentExportTextAlign;
};

export type TDocumentExportLogoMode = "none" | "workspace" | "custom";

export type TDocumentExportSettings = {
  id: string | null;
  header_logo_mode: TDocumentExportLogoMode;
  header_logo_asset_id: string | null;
  header_logo_url: string | null;
  header_text_lines: TDocumentExportTextLine[];
  footer_text_lines: TDocumentExportTextLine[];
  show_page_numbers: boolean;
  show_generated_by: boolean;
  created_at: string | null;
  updated_at: string | null;
};

export type TDocumentExportSettingsPayload = Partial<
  Pick<
    TDocumentExportSettings,
    | "header_logo_mode"
    | "header_logo_asset_id"
    | "header_text_lines"
    | "footer_text_lines"
    | "show_page_numbers"
    | "show_generated_by"
  >
>;
