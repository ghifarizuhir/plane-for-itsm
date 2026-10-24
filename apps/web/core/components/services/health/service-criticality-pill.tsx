/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceCriticality } from "@plane/types";
import { cn } from "@plane/utils";

const CRITICALITY_PILL_STYLE: Record<TServiceCriticality, string> = {
  critical: "bg-danger-subtle text-danger-primary",
  high: "bg-warning-subtle text-warning-primary",
  medium: "bg-layer-2 text-secondary",
  low: "bg-layer-2 text-tertiary",
};

type Props = {
  criticality: TServiceCriticality;
  className?: string;
};

export function ServiceCriticalityPill({ criticality, className }: Props) {
  const { t } = useTranslation();
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-11 font-medium capitalize",
        CRITICALITY_PILL_STYLE[criticality],
        className
      )}
    >
      {t(`service.criticality_values.${criticality}`)}
    </span>
  );
}
