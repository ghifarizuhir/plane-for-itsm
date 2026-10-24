/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IMetricSource,
  IMetricSourcePayload,
  IServiceMetric,
  IServiceMetricBucketsResponse,
  TServiceMetricResolution,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class ServiceMetricsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getMetrics(workspaceSlug: string, serviceId: string): Promise<IServiceMetric[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/`).then(
      (response) => response?.data?.metrics ?? []
    );
  }

  async getBuckets(
    workspaceSlug: string,
    serviceId: string,
    metricId: string,
    params: { from: string; to: string; resolution: TServiceMetricResolution }
  ): Promise<IServiceMetricBucketsResponse> {
    return this.get(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/${metricId}/buckets/`, {
      params,
    }).then((response) => response?.data);
  }

  async getSources(workspaceSlug: string): Promise<IMetricSource[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/metric-sources/`).then((response) => response?.data ?? []);
  }

  async createSource(workspaceSlug: string, data: IMetricSourcePayload): Promise<IMetricSource> {
    return this.post(`/api/workspaces/${workspaceSlug}/metric-sources/`, data).then((response) => response?.data);
  }

  async updateSource(
    workspaceSlug: string,
    sourceId: string,
    data: Partial<IMetricSourcePayload>
  ): Promise<IMetricSource> {
    return this.patch(`/api/workspaces/${workspaceSlug}/metric-sources/${sourceId}/`, data).then(
      (response) => response?.data
    );
  }

  async deleteSource(workspaceSlug: string, sourceId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/metric-sources/${sourceId}/`).then(
      (response) => response?.data
    );
  }

  async rotateSource(workspaceSlug: string, sourceId: string): Promise<{ id: string; token: string }> {
    return this.post(`/api/workspaces/${workspaceSlug}/metric-sources/${sourceId}/rotate/`, {}).then(
      (response) => response?.data
    );
  }

  async updateMetric(
    workspaceSlug: string,
    serviceId: string,
    metricId: string,
    data: { display_name?: string; unit?: string; description?: string }
  ): Promise<IServiceMetric> {
    return this.patch(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/${metricId}/`, data).then(
      (response) => response?.data
    );
  }

  async deleteMetric(workspaceSlug: string, serviceId: string, metricId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/services/${serviceId}/metrics/${metricId}/`).then(
      (response) => response?.data
    );
  }
}
