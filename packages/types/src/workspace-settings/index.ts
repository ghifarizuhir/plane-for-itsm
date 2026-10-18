/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWorkspaceSettings = {
  id: string;
  workspace: string;
  cycle_view: boolean;
  module_view: boolean;
  issue_views_view: boolean;
  page_view: boolean;
  intake_view: boolean;
  is_issue_type_enabled: boolean;
  is_time_tracking_enabled: boolean;
  guest_view_all_features: boolean;
  archive_in: number;
  close_in: number;
  default_assignee_id: string | null;
  updated_at: string;
};

export type TWorkspaceSettingsPayload = Partial<
  Pick<
    TWorkspaceSettings,
    | "cycle_view"
    | "module_view"
    | "issue_views_view"
    | "page_view"
    | "intake_view"
    | "is_issue_type_enabled"
    | "is_time_tracking_enabled"
    | "guest_view_all_features"
    | "archive_in"
    | "close_in"
    | "default_assignee_id"
  >
>;
