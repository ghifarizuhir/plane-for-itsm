/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TMetricRange } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { RANGE_OPTIONS } from "./helpers";

type Props = {
  value: TMetricRange;
  onChange: (range: TMetricRange) => void;
};

export function RangeSelector({ value, onChange }: Props) {
  const { t } = useTranslation();
  const selected = RANGE_OPTIONS.find((option) => option.key === value);

  return (
    <CustomSelect
      value={value}
      label={selected ? t(selected.label) : value}
      onChange={(next: TMetricRange) => onChange(next)}
      noChevron
      className="w-full"
      buttonClassName="w-full"
    >
      {RANGE_OPTIONS.map((option) => (
        <CustomSelect.Option key={option.key} value={option.key}>
          {t(option.label)}
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
}
