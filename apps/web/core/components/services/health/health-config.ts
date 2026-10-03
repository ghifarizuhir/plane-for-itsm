/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceHealth } from "@plane/types";

export const DEFAULT_HEALTH: TServiceHealth = "unknown";

export const HEALTH_CONFIG: Record<
  TServiceHealth,
  { dot: string; rail: string; pill: string; label_key: string; color: string }
> = {
  down: {
    dot: "bg-danger-primary",
    rail: "bg-danger-primary",
    pill: "bg-danger-subtle text-danger-primary",
    label_key: "service.health_values.down",
    color: "var(--bg-danger-primary)",
  },
  degraded: {
    dot: "bg-warning-primary",
    rail: "bg-warning-primary",
    pill: "bg-warning-subtle text-warning-primary",
    label_key: "service.health_values.degraded",
    color: "var(--bg-warning-primary)",
  },
  healthy: {
    dot: "bg-success-primary",
    rail: "bg-success-primary",
    pill: "bg-success-subtle text-success-primary",
    label_key: "service.health_values.healthy",
    color: "var(--bg-success-primary)",
  },
  unknown: {
    dot: "bg-layer-3",
    rail: "bg-layer-3",
    pill: "bg-layer-2 text-tertiary",
    label_key: "service.health_values.unknown",
    color: "var(--bg-layer-3)",
  },
};
