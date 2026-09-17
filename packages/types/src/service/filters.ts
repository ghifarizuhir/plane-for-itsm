/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceCriticality, TServiceHealth, TServiceStatus, TServiceType } from "./core";

export type TServiceLayoutOptions = "board" | "graph";

export type TServiceOrderByOptions = "health" | "name" | "-created_at" | "-updated_at" | "criticality" | "status";

export type TServiceIncidentFilter = "active";

export type TServiceFilters = {
  status?: TServiceStatus[];
  criticality?: TServiceCriticality[];
  type?: TServiceType[];
  health?: TServiceHealth[];
  incidents?: TServiceIncidentFilter[];
};

export type TServiceDisplayFilters = {
  layout: TServiceLayoutOptions;
  order_by: TServiceOrderByOptions;
};
