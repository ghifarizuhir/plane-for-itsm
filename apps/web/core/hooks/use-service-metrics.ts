/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import useSWR from "swr";
import type { IServiceMetric, IServiceMetricBucketsResponse, TMetricRange } from "@plane/types";
// components
import { autoResolutionForRange, rangeToFromTo } from "@/components/services/metrics/helpers";
// services
import { ServiceMetricsService } from "@/services/service-metrics.service";

const service = new ServiceMetricsService();

export function useServiceMetricDefinitions(workspaceSlug: string, serviceId: string) {
  return useSWR<IServiceMetric[]>(
    workspaceSlug && serviceId ? `SERVICE_METRIC_DEFS_${workspaceSlug}_${serviceId}` : null,
    () => service.getMetrics(workspaceSlug, serviceId),
    { refreshInterval: 60_000, revalidateOnFocus: true }
  );
}

export function useServiceMetricBuckets(
  workspaceSlug: string,
  serviceId: string,
  metricId: string | null,
  range: TMetricRange
) {
  return useSWR<IServiceMetricBucketsResponse>(
    metricId ? ["SERVICE_METRIC_BUCKETS", workspaceSlug, serviceId, metricId, range] : null,
    () => {
      const { from, to } = rangeToFromTo(range);
      return service.getBuckets(workspaceSlug, serviceId, metricId!, {
        from,
        to,
        resolution: autoResolutionForRange(range),
      });
    },
    { refreshInterval: 60_000, revalidateOnFocus: true, keepPreviousData: true }
  );
}
