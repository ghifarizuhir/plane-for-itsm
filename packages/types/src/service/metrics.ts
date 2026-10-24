/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceMetricResolution = 60 | 300 | 600 | 1800 | 3600;

export type TMetricRange = "1h" | "6h" | "24h" | "7d" | "30d" | "90d";

export interface IServiceMetricBucket {
  bucket_start: string;
  count: number | null;
  sum: number | null;
  min: number | null;
  max: number | null;
  avg: number | null;
  last: number | null;
}

export interface IServiceMetricLastBucket extends IServiceMetricBucket {
  resolution: TServiceMetricResolution;
}

export interface IServiceMetric {
  id: string;
  service_id: string;
  key: string;
  display_name: string;
  unit: string;
  description: string;
  last_bucket: IServiceMetricLastBucket | null;
}

export interface IServiceMetricBucketsResponse {
  metric: Pick<IServiceMetric, "id" | "key" | "display_name" | "unit" | "description">;
  resolution: TServiceMetricResolution;
  from: string;
  to: string;
  buckets: IServiceMetricBucket[];
}

export interface IMetricSourceConfig {
  service_map?: Record<string, string>;
  fallback_service_id?: string | null;
}

export interface IMetricSource {
  id: string;
  workspace_id: string;
  name: string;
  token: string;
  is_active: boolean;
  config: IMetricSourceConfig;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface IMetricSourcePayload {
  name: string;
  is_active?: boolean;
  config?: IMetricSourceConfig;
}
