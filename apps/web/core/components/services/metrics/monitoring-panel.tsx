/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { observer } from "mobx-react";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EmptyStateCompact } from "@plane/propel/empty-state";
import type { TMetricRange } from "@plane/types";
import { Loader } from "@plane/ui";
import { useUserPermissions } from "@/hooks/store/user";
import { useServiceMetricBuckets, useServiceMetricDefinitions } from "@/hooks/use-service-metrics";
import { buildTimeline } from "./helpers";
import { MetricChart } from "./metric-chart";
import { MetricSelector } from "./metric-selector";
import { MetricSummary } from "./metric-summary";
import { RangeSelector } from "./range-selector";

type Props = {
  serviceId: string;
};

export const ServiceMonitoringPanel = observer(function ServiceMonitoringPanel({ serviceId }: Props) {
  const { t } = useTranslation();
  const params = useParams();
  const workspaceSlug = params.workspaceSlug?.toString() ?? "";
  const { allowPermissions } = useUserPermissions();
  const isAdmin = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const [selectedMetricId, setSelectedMetricId] = useState<string | null>(null);
  const [range, setRange] = useState<TMetricRange>("24h");

  const { data: metrics, isLoading } = useServiceMetricDefinitions(workspaceSlug, serviceId);

  useEffect(() => {
    if (!selectedMetricId && metrics?.length) {
      setSelectedMetricId((metrics.find((metric) => metric.last_bucket) ?? metrics[0]).id);
    }
    if (selectedMetricId && metrics && !metrics.some((metric) => metric.id === selectedMetricId)) {
      setSelectedMetricId(null);
    }
  }, [metrics, selectedMetricId]);

  const metric = metrics?.find((item) => item.id === selectedMetricId) ?? null;
  const {
    data: bucketData,
    isLoading: bucketsLoading,
    error,
    mutate,
  } = useServiceMetricBuckets(workspaceSlug, serviceId, metric?.id ?? null, range);

  const timeline = useMemo(
    () =>
      bucketData ? buildTimeline(bucketData.buckets, bucketData.resolution, bucketData.from, bucketData.to, range) : [],
    [bucketData, range]
  );

  if (isLoading) {
    return (
      <Loader className="space-y-3">
        <Loader.Item height="32px" width="100%" />
        <Loader.Item height="120px" width="100%" />
      </Loader>
    );
  }

  if (!metrics?.length) {
    return (
      <div className="flex flex-col items-center justify-center gap-4 py-10">
        <EmptyStateCompact
          assetKey="unknown"
          title={t("service.monitoring.empty_title")}
          description={t("service.monitoring.empty_description")}
        />
        {isAdmin && (
          <Link href={`/${workspaceSlug}/settings/metric-sources`}>
            <Button variant="primary" size="sm">
              {t("service.monitoring.configure_source")}
            </Button>
          </Link>
        )}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 py-10">
        <p className="text-13 text-tertiary">{t("service.monitoring.error")}</p>
        <Button variant="secondary" size="sm" onClick={() => mutate()}>
          {t("common.retry")}
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-col gap-2">
        <MetricSelector metrics={metrics} value={metric?.id ?? null} onChange={setSelectedMetricId} />
        <RangeSelector value={range} onChange={setRange} />
      </div>
      <p className="text-11 text-tertiary">
        {t("service.monitoring.resolution")}: {bucketData?.resolution ? `${bucketData.resolution / 60}m` : "—"}
      </p>
      {bucketData && <MetricSummary buckets={bucketData.buckets} unit={metric?.unit ?? ""} />}
      {bucketsLoading && !bucketData ? (
        <Loader className="space-y-2">
          <Loader.Item height="240px" width="100%" />
        </Loader>
      ) : (
        <MetricChart data={timeline} unit={metric?.unit ?? ""} />
      )}
    </div>
  );
});
