/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceStatus = "active" | "planned" | "maintenance" | "deprecated" | "retired";

export type TServiceCriticality = "critical" | "high" | "medium" | "low";

export type TServiceType = "internal" | "external" | "infrastructure" | "third_party";

export type TServicePosition = {
  x: number;
  y: number;
};

export interface IService {
  id: string;
  workspace_id: string;
  project_id: string;
  name: string;
  description: string;
  description_html: string;
  status: TServiceStatus;
  criticality: TServiceCriticality;
  type: TServiceType;
  owner_id: string | null;
  repository_url: string | null;
  documentation_url: string | null;
  position: TServicePosition | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  updated_by: string | null;
}

export interface IServiceDependency {
  id: string;
  workspace_id: string;
  project_id: string;
  from_service_id: string;
  to_service_id: string;
  created_at: string;
}

export interface TServiceWorkItemLink {
  id: string;
  service_id: string;
  issue_id: string;
  project_id: string;
  workspace_id: string;
  issue_identifier?: string;
  issue_name?: string;
}

export type TServiceGraphData = {
  services: IService[];
  dependencies: IServiceDependency[];
};
