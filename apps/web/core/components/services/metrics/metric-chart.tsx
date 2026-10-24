/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import { LineChart } from "@plane/propel/charts/line-chart";
import type { TChartData } from "@plane/types";
import { formatMetricValue, type TMetricChartPoint } from "./helpers";

type Props = {
  data: TMetricChartPoint[];
  unit: string;
};

export function MetricChart({ data, unit }: Props) {
  const { t } = useTranslation();
  const hasData = data.some((point) => point.avg !== null || point.max !== null);
  if (!hasData) {
    return (
      <div className="flex h-60 items-center justify-center text-13 text-tertiary">
        {t("service.monitoring.empty_range")}
      </div>
    );
  }

  return (
    <LineChart
      className="h-60 w-full"
      data={data as unknown as TChartData<string, string>[]}
      xAxis={{ key: "label" }}
      yAxis={{ key: "domainValue", label: unit }}
      lines={[
        {
          key: "avg",
          label: t("service.monitoring.avg"),
          dashedLine: false,
          fill: "#1192E8",
          showDot: false,
          smoothCurves: false,
          stroke: "#1192E8",
        },
        {
          key: "max",
          label: t("service.monitoring.max"),
          dashedLine: true,
          fill: "#F0A000",
          showDot: false,
          smoothCurves: false,
          stroke: "#F0A000",
        },
      ]}
      customTooltipContent={({ active, payload }) => {
        if (!active || !payload?.length) return null;
        const point = payload[0].payload as TMetricChartPoint;
        const rows: [string, number | null][] = [
          ["last", point.last],
          ["avg", point.avg],
          ["min", point.min],
          ["max", point.max],
        ];
        return (
          <div className="shadow-lg rounded-md border border-subtle bg-surface-1 p-2 text-11">
            <p className="mb-1 font-medium text-primary">{point.label}</p>
            {rows.map(([key, value]) => (
              <p key={key} className="flex justify-between gap-3 text-tertiary">
                <span>{t(`service.monitoring.${key}`)}</span>
                <span className="text-secondary">{formatMetricValue(value, unit)}</span>
              </p>
            ))}
            <p className="flex justify-between gap-3 text-tertiary">
              <span>{t("service.monitoring.count")}</span>
              <span className="text-secondary">{point.count ?? "—"}</span>
            </p>
          </div>
        );
      }}
    />
  );
}
