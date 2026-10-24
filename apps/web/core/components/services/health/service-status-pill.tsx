/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceStatus } from "@plane/types";
import { cn } from "@plane/utils";

const STATUS_PILL_STYLE: Record<TServiceStatus, string> = {
  active: "bg-success-subtle text-success-primary",
  maintenance: "bg-warning-subtle text-warning-primary",
  planned: "bg-layer-2 text-tertiary",
  deprecated: "bg-layer-2 text-secondary",
  retired: "bg-layer-2 text-tertiary",
};

type Props = {
  status: TServiceStatus;
  className?: string;
};

export function ServiceStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-11 font-medium capitalize",
        STATUS_PILL_STYLE[status],
        className
      )}
    >
      {t(`service.status_values.${status}`)}
    </span>
  );
}
