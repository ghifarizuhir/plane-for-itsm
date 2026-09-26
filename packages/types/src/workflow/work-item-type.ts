/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWorkItemType = {
  id: string;
  name: string;
  description: string;
  logo_props: Record<string, unknown>;
  is_epic: boolean;
  is_default: boolean;
  is_active: boolean;
  level: number;
  workflow: string | null;
  workspace: string;
  project_ids: string[];
  external_id: string | null;
  external_source: string | null;
  created_at: string;
  updated_at: string;
};

export type TWorkItemTypePayload = {
  name?: string;
  description?: string;
  is_active?: boolean;
  workflow?: string | null;
  project_ids?: string[];
};
