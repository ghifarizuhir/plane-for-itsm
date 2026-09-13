/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceCriticality, TServiceStatus, TServiceType } from "./core";

export type TServiceLayoutOptions = "list" | "grid" | "graph";

export type TServiceOrderByOptions = "name" | "-created_at" | "-updated_at" | "criticality" | "status";

export type TServiceFilters = {
  status?: TServiceStatus[];
  criticality?: TServiceCriticality[];
  type?: TServiceType[];
};

export type TServiceDisplayFilters = {
  layout: TServiceLayoutOptions;
  order_by: TServiceOrderByOptions;
};
