/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ChevronDownOutline, GroupOutline, TickOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { getButtonStyling } from "@plane/propel/button";
import type { TServiceGroupByOptions } from "@plane/types";
// ui
import { CustomMenu } from "@plane/ui";
// helpers
import { cn } from "@plane/utils";

const SERVICE_GROUP_BY_OPTIONS: { key: Exclude<TServiceGroupByOptions, null>; i18n_label: string }[] = [
  { key: "status", i18n_label: "service.list.group_by.status" },
  { key: "criticality", i18n_label: "service.list.group_by.criticality" },
  { key: "health", i18n_label: "service.list.group_by.health" },
];

type Props = {
  onChange: (value: TServiceGroupByOptions) => void;
  value: TServiceGroupByOptions | undefined;
};

export function ServiceGroupByDropdown(props: Props) {
  const { onChange, value } = props;
  // hooks
  const { t } = useTranslation();

  const groupByDetails = SERVICE_GROUP_BY_OPTIONS.find((option) => option.key === value);

  return (
    <CustomMenu
      customButton={
        <div className={cn(getButtonStyling("secondary", "lg"), "px-2 text-tertiary")}>
          <GroupOutline className="size-3" />
          {groupByDetails ? t(groupByDetails.i18n_label) : t("service.list.group_by.label")}
          <ChevronDownOutline className="size-3" />
        </div>
      }
      placement="bottom-end"
      maxHeight="lg"
      closeOnSelect
    >
      {SERVICE_GROUP_BY_OPTIONS.map((option) => (
        <CustomMenu.MenuItem
          key={option.key}
          className="flex items-center justify-between gap-2"
          onClick={() => onChange(option.key)}
        >
          {t(option.i18n_label)}
          {value === option.key && <TickOutline className="h-3 w-3" />}
        </CustomMenu.MenuItem>
      ))}
    </CustomMenu>
  );
}
