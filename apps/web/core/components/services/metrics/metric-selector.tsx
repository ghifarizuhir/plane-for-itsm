/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { IServiceMetric } from "@plane/types";
import { CustomSelect } from "@plane/ui";

type Props = {
  metrics: IServiceMetric[];
  value: string | null;
  onChange: (metricId: string) => void;
};

export function MetricSelector({ metrics, value, onChange }: Props) {
  const { t } = useTranslation();
  const selected = metrics.find((metric) => metric.id === value);
  const label = selected
    ? `${selected.display_name || selected.key}${selected.unit ? ` (${selected.unit})` : ""}`
    : t("service.monitoring.select_metric");

  return (
    <CustomSelect
      value={value}
      label={label}
      onChange={(next: string) => onChange(next)}
      noChevron
      className="w-full"
      buttonClassName="w-full"
    >
      {metrics.map((metric) => (
        <CustomSelect.Option key={metric.id} value={metric.id}>
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate">{metric.display_name || metric.key}</span>
            {metric.unit && <span className="text-tertiary">{metric.unit}</span>}
          </span>
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
}
