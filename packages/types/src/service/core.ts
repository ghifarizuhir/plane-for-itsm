/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceStatus = "active" | "planned" | "maintenance" | "deprecated" | "retired";

export type TServiceCriticality = "critical" | "high" | "medium" | "low";

export type TServiceType = "internal" | "external" | "infrastructure" | "third_party";

export type TServiceKind =
  | "service"
  | "database"
  | "server"
  | "cache"
  | "queue"
  | "storage"
  | "network"
  | "external_api"
  | "other";

export type TServiceHealth = "healthy" | "degraded" | "down" | "unknown";

export type TServiceIncidentSeverity = "sev1" | "sev2" | "sev3" | "sev4";

export interface IServiceIncident {
  id: string;
  service_id: string;
  severity: TServiceIncidentSeverity;
  opened_at: string;
  identifier?: string;
  name?: string;
  war_room_id?: string | null;
  war_room_sequence_id?: number | null;
  war_room_name?: string | null;
}

export interface IServiceHealthSnapshot {
  service_id: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
  last_deployed_at: string | null;
}

export type TServiceHealthSummary = {
  down: number;
  degraded: number;
  healthy: number;
  unknown: number;
  criticalImpacted: number;
};

export type TServicePosition = {
  x: number;
  y: number;
};

export type TServiceAccessLevel = "full" | "lite";

export interface IServiceProjectLink {
  id: string;
  project_id: string;
  link_type: "owner" | "consumer";
}

/** Restricted projection returned for dependency endpoints the caller cannot fully access. */
export interface ILiteService {
  id: string;
  name: string;
  status: TServiceStatus;
  criticality: TServiceCriticality;
  access_level: "lite";
}

export interface IService {
  id: string;
  workspace_id: string;
  /** @deprecated equals `owner_project_id`; kept for backward compatibility. */
  project_id: string | null;
  owner_project_id: string | null;
  project_links: IServiceProjectLink[];
  access_level: "full";
  name: string;
  description: string;
  description_html: string;
  status: TServiceStatus;
  criticality: TServiceCriticality;
  type: TServiceType;
  kind: TServiceKind;
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
  /** Legacy dual-write column; compat endpoints echo the queried project. */
  project_id: string | null;
  from_service_id: string;
  to_service_id: string;
  dependency_type: TServiceDependencyType;
  created_at: string;
  from_service?: IService | ILiteService;
  to_service?: IService | ILiteService;
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
  services: (IService | ILiteService)[];
  dependencies: IServiceDependency[];
  health: Record<string, IServiceHealthSnapshot>;
};

export type TServiceDependencyType = "runs_on" | "connects_to" | "replicates_to" | "depends_on" | "uses" | "other";
