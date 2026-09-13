/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ChevronDownOutline, SortAscendingOutline, SortDescendingOutline, TickOutline } from "@makeplane/propel/icons";
import { getButtonStyling } from "@plane/propel/button";
import type { TServiceOrderByOptions } from "@plane/types";
// ui
import { CustomMenu } from "@plane/ui";
// helpers
import { cn } from "@plane/utils";

export const SERVICE_ORDER_BY_OPTIONS: { key: TServiceOrderByOptions; label: string }[] = [
  { key: "name", label: "Name" },
  { key: "-created_at", label: "Created date" },
  { key: "-updated_at", label: "Updated date" },
  { key: "criticality", label: "Criticality" },
  { key: "status", label: "Status" },
];

type Props = {
  onChange: (value: TServiceOrderByOptions) => void;
  value: TServiceOrderByOptions | undefined;
};

export function ServiceOrderByDropdown(props: Props) {
  const { onChange, value } = props;

  const orderByDetails = SERVICE_ORDER_BY_OPTIONS.find((option) => option.key === value);

  const isDescending = value?.[0] === "-";

  return (
    <CustomMenu
      customButton={
        <div className={cn(getButtonStyling("secondary", "lg"), "px-2 text-tertiary")}>
          {!isDescending ? <SortAscendingOutline className="size-3" /> : <SortDescendingOutline className="size-3" />}
          {orderByDetails?.label}
          <ChevronDownOutline className="size-3" />
        </div>
      }
      placement="bottom-end"
      maxHeight="lg"
      closeOnSelect
    >
      {SERVICE_ORDER_BY_OPTIONS.map((option) => (
        <CustomMenu.MenuItem
          key={option.key}
          className="flex items-center justify-between gap-2"
          onClick={() => onChange(option.key)}
        >
          {option.label}
          {value === option.key && <TickOutline className="h-3 w-3" />}
        </CustomMenu.MenuItem>
      ))}
    </CustomMenu>
  );
}
