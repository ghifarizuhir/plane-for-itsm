/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { IServiceMetricBucket } from "@plane/types";
import { formatMetricValue, latestBucket } from "./helpers";

type Props = {
  buckets: IServiceMetricBucket[];
  unit: string;
};

export function MetricSummary({ buckets, unit }: Props) {
  const { t } = useTranslation();
  const latest = latestBucket(buckets);
  const items: { key: string; value: number | null; raw?: boolean }[] = [
    { key: "last", value: latest?.last ?? null },
    { key: "avg", value: latest?.avg ?? null },
    { key: "min", value: latest?.min ?? null },
    { key: "max", value: latest?.max ?? null },
    { key: "count", value: latest?.count ?? null, raw: true },
  ];

  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
      {items.map((item) => (
        <div key={item.key} className="rounded-md border border-subtle p-2">
          <p className="text-10 font-medium tracking-wide text-tertiary uppercase">
            {t(`service.monitoring.${item.key}`)}
          </p>
          <p className="truncate text-13 font-medium text-primary">
            {item.raw ? (item.value ?? "—") : formatMetricValue(item.value, unit)}
          </p>
        </div>
      ))}
    </div>
  );
}
