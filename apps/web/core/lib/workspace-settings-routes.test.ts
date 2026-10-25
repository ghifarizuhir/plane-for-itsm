/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { RouteConfigEntry } from "@react-router/dev/routes";
import { WORKSPACE_SETTINGS } from "@plane/constants";
import { describe, expect, it } from "vitest";
import { coreRoutes } from "../../app/routes/core";

const flattenRoutes = (routes: RouteConfigEntry[]): RouteConfigEntry[] =>
  routes.flatMap((entry) => [entry, ...flattenRoutes((entry.children ?? []) as RouteConfigEntry[])]);

const registeredPaths = new Set(flattenRoutes(coreRoutes as RouteConfigEntry[]).map((entry) => entry.path));

describe("workspace settings routes", () => {
  it.each(Object.values(WORKSPACE_SETTINGS))("registers a route for the $key settings item", (item) => {
    expect(registeredPaths.has(`:workspaceSlug${item.href}`)).toBe(true);
  });
});
