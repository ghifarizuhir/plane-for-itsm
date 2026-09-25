/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ChevronDownOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
// ui
import { CustomSelect } from "@plane/ui";

type Props<T extends string> = {
  value: T;
  options: readonly T[];
  i18nPrefix: string;
  onChange: (value: T) => void;
  disabled?: boolean;
};

export function ServicePropertySelect<T extends string>(props: Props<T>) {
  const { value, options, i18nPrefix, onChange, disabled = false } = props;
  // plane hooks
  const { t } = useTranslation();

  return (
    <CustomSelect
      value={value}
      onChange={(val: T) => onChange(val)}
      disabled={disabled}
      placement="bottom-start"
      className="w-full grow"
      customButtonClassName="group h-7.5 w-full grow px-2 text-left"
      customButton={
        <span className="flex w-full items-center justify-between gap-1">
          <span className="truncate text-body-xs-regular capitalize">{t(`${i18nPrefix}.${value}`)}</span>
          {!disabled && (
            <ChevronDownOutline aria-hidden="true" className="hidden h-3.5 w-3.5 shrink-0 group-hover:inline" />
          )}
        </span>
      }
    >
      {options.map((option) => (
        <CustomSelect.Option key={option} value={option}>
          <span className="capitalize">{t(`${i18nPrefix}.${option}`)}</span>
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
}
