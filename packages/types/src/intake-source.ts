/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TIntakeSourcePriority = "urgent" | "high" | "medium" | "low" | "none";

export type TIntakeSourceConfig = {
  service_label_key?: string;
  service_map?: Record<string, string>;
  fallback_service_id?: string | null;
  severity_label_key?: string;
  severity_map?: Record<string, TIntakeSourcePriority>;
  default_priority?: TIntakeSourcePriority;
};

export type TIntakeSource = {
  id: string;
  project_id: string;
  name: string;
  token: string;
  is_active: boolean;
  auto_accept: boolean;
  type_id: string | null;
  config: TIntakeSourceConfig;
  created_at: string;
  updated_at: string;
  created_by: string | null;
};

export type TIntakeSourcePayload = {
  name: string;
  type_id: string;
  auto_accept?: boolean;
  config?: TIntakeSourceConfig;
};
