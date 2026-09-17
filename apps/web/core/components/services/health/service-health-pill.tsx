/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "./health-config";

type Props = {
  health?: TServiceHealth | null;
  className?: string;
};

export function ServiceHealthPill({ health, className }: Props) {
  const { t } = useTranslation();
  const state = health ?? DEFAULT_HEALTH;
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-11 font-medium",
        HEALTH_CONFIG[state].pill,
        className
      )}
    >
      {t(HEALTH_CONFIG[state].label_key)}
    </span>
  );
}
