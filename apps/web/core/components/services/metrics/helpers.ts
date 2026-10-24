/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { format } from "date-fns";
import type { IServiceMetricBucket, TServiceMetricResolution, TMetricRange } from "@plane/types";

export const MAX_CHART_POINTS = 3000;

export const RANGE_HOURS: Record<TMetricRange, number> = {
  "1h": 1,
  "6h": 6,
  "24h": 24,
  "7d": 7 * 24,
  "30d": 30 * 24,
  "90d": 90 * 24,
};

export const RANGE_OPTIONS: { key: TMetricRange; label: string }[] = [
  { key: "1h", label: "service.monitoring.range_1h" },
  { key: "6h", label: "service.monitoring.range_6h" },
  { key: "24h", label: "service.monitoring.range_24h" },
  { key: "7d", label: "service.monitoring.range_7d" },
  { key: "30d", label: "service.monitoring.range_30d" },
  { key: "90d", label: "service.monitoring.range_90d" },
];

export type TMetricChartPoint = {
  timestamp: number;
  label: string;
  domainValue: number | null;
  avg: number | null;
  max: number | null;
  min: number | null;
  last: number | null;
  count: number | null;
  sum: number | null;
};

export function autoResolutionForRange(range: TMetricRange): TServiceMetricResolution {
  if (range === "1h" || range === "6h") return 60;
  if (range === "24h") return 300;
  if (range === "7d") return 600;
  if (range === "30d") return 1800;
  return 3600;
}

export function rangeToFromTo(range: TMetricRange, now = new Date()): { from: string; to: string } {
  const to = now.toISOString();
  const from = new Date(now.getTime() - RANGE_HOURS[range] * 60 * 60 * 1000).toISOString();
  return { from, to };
}

export function formatMetricValue(value: number | null | undefined, unit: string): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  const formatted = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value);
  if (!unit) return formatted;
  return unit === "%" ? `${formatted}%` : `${formatted} ${unit}`;
}

export function formatBucketLabel(date: Date, range: TMetricRange): string {
  if (range === "1h" || range === "6h" || range === "24h") return format(date, "HH:mm");
  if (range === "7d" || range === "30d") return format(date, "MMM d, HH:mm");
  return format(date, "MMM d");
}

export function latestBucket(buckets: IServiceMetricBucket[]): IServiceMetricBucket | null {
  for (let index = buckets.length - 1; index >= 0; index -= 1) {
    const bucket = buckets[index];
    if (
      bucket.count !== null ||
      bucket.sum !== null ||
      bucket.min !== null ||
      bucket.max !== null ||
      bucket.avg !== null ||
      bucket.last !== null
    ) {
      return bucket;
    }
  }
  return null;
}

export function buildTimeline(
  buckets: IServiceMetricBucket[],
  resolution: TServiceMetricResolution,
  from: string,
  to: string,
  range: TMetricRange
): TMetricChartPoint[] {
  const stepMs = resolution * 1000;
  const startMs = Math.floor(new Date(from).getTime() / stepMs) * stepMs;
  const endMs = new Date(to).getTime();
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) return [];
  const byTime = new Map(buckets.map((bucket) => [new Date(bucket.bucket_start).getTime(), bucket] as const));
  const points: TMetricChartPoint[] = [];
  for (let timestamp = startMs; timestamp < endMs && points.length < MAX_CHART_POINTS; timestamp += stepMs) {
    const bucket = byTime.get(timestamp);
    points.push({
      timestamp,
      label: formatBucketLabel(new Date(timestamp), range),
      domainValue: bucket?.max ?? bucket?.avg ?? bucket?.min ?? bucket?.last ?? null,
      avg: bucket?.avg ?? null,
      max: bucket?.max ?? null,
      min: bucket?.min ?? null,
      last: bucket?.last ?? null,
      count: bucket?.count ?? null,
      sum: bucket?.sum ?? null,
    });
  }
  return points;
}
