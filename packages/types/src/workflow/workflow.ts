/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TStateGroups } from "../state";

export type TWorkflow = {
  id: string;
  name: string;
  description: string;
  is_active: boolean;
  workspace_id: string;
  created_at: string;
  updated_at: string;
};

export type TWorkflowState = {
  id: string;
  workflow_id: string;
  name: string;
  description: string;
  color: string;
  slug: string;
  sequence: number;
  group: TStateGroups;
  is_default: boolean;
};

export type TWorkflowTransition = {
  id: string;
  workflow_id: string;
  from_state_id: string;
  to_state_id: string;
};

/** Mirror state di project (id = `State.id`, bukan `WorkflowState.id`). */
export type TWorkflowMapState = {
  id: string;
  name: string;
  color: string;
  group: TStateGroups;
  sequence: number;
  is_default: boolean;
};

export type TWorkflowMapTransition = {
  from_state_id: string;
  to_state_id: string;
};

export type TWorkflowMapType = {
  type_id: string;
  type_name: string;
  workflow_id: string;
  default_state_id: string | null;
  states: TWorkflowMapState[];
  transitions: TWorkflowMapTransition[];
};

export type TWorkflowMap = {
  types: TWorkflowMapType[];
};

export type TWorkflowPayload = {
  name: string;
  description?: string;
  is_active?: boolean;
};

export type TWorkflowStatePayload = {
  name?: string;
  description?: string;
  color?: string;
  group?: TStateGroups;
  sequence?: number;
  is_default?: boolean;
};
