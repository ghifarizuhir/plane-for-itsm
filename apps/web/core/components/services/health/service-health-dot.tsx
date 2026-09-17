/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "./health-config";

type Props = {
  health?: TServiceHealth | null;
  className?: string;
};

export function ServiceHealthDot({ health, className }: Props) {
  const state = health ?? DEFAULT_HEALTH;
  return (
    <span
      aria-hidden="true"
      className={cn("h-2 w-2 flex-shrink-0 rounded-full", HEALTH_CONFIG[state].dot, className)}
    />
  );
}
