/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { IService, IServiceHealthSnapshot } from "@plane/types";
import { selectWorkspaceServiceIds, WORKSPACE_SERVICE_FILTER_KEY } from "./workspace-services";

const makeService = (overrides: Partial<IService>): IService =>
  ({
    id: "service",
    name: "Service",
    owner_project_id: "p1",
    project_links: [],
    status: "active",
    criticality: "medium",
    type: "internal",
    kind: "service",
    created_at: "2026-10-01T00:00:00.000Z",
    updated_at: "2026-10-01T00:00:00.000Z",
    ...overrides,
  }) as IService;

const select = (services: IService[], overrides: Partial<Parameters<typeof selectWorkspaceServiceIds>[0]> = {}) =>
  selectWorkspaceServiceIds({
    services,
    projectIds: ["p1"],
    filters: {},
    searchQuery: "",
    orderBy: "name",
    healthMap: {},
    ...overrides,
  });

describe("selectWorkspaceServiceIds", () => {
  it("matches services by owner project or linked project", () => {
    const selection = select([
      makeService({ id: "owner", name: "Owner", owner_project_id: "p1" }),
      makeService({
        id: "linked",
        name: "Linked",
        owner_project_id: "p2",
        project_links: [{ id: "l1", project_id: "p1", link_type: "consumer" }],
      }),
      makeService({ id: "other", name: "Other", owner_project_id: "p2" }),
    ]);

    expect(selection).toEqual(["linked", "owner"]);
  });

  it("returns all services for All applications", () => {
    const selection = select(
      [
        makeService({ id: "assigned", owner_project_id: "p1" }),
        makeService({ id: "unassigned", owner_project_id: null }),
      ],
      { projectIds: undefined }
    );

    expect(selection).toEqual(["assigned", "unassigned"]);
  });

  it("applies the search query", () => {
    const selection = select(
      [makeService({ id: "billing", name: "Billing API" }), makeService({ id: "auth", name: "Auth" })],
      { searchQuery: "bill" }
    );

    expect(selection).toEqual(["billing"]);
  });

  it("applies status, criticality, and type filters", () => {
    const selection = select(
      [
        makeService({ id: "match", status: "active", criticality: "high", type: "internal" }),
        makeService({ id: "wrong-status", status: "retired", criticality: "high", type: "internal" }),
        makeService({ id: "wrong-criticality", status: "active", criticality: "low", type: "internal" }),
        makeService({ id: "wrong-type", status: "active", criticality: "high", type: "external" }),
      ],
      { filters: { status: ["active"], criticality: ["high"], type: ["internal"] } }
    );

    expect(selection).toEqual(["match"]);
  });

  it("filters by component kind", () => {
    const selection = select(
      [
        makeService({ id: "db", name: "Primary DB", kind: "database" }),
        makeService({ id: "svc", name: "Payments", kind: "service" }),
      ],
      { filters: { kind: ["database"] } }
    );

    expect(selection).toEqual(["db"]);
  });

  it("applies health and active-incident filters", () => {
    const healthMap: Record<string, IServiceHealthSnapshot> = {
      sick: {
        service_id: "sick",
        health: "down",
        incidents: [{ id: "i1", service_id: "sick", severity: "sev1", opened_at: "2026-10-01" }],
        last_deployed_at: null,
      },
      healthy: {
        service_id: "healthy",
        health: "healthy",
        incidents: [],
        last_deployed_at: null,
      },
    };

    const selection = select([makeService({ id: "sick" }), makeService({ id: "healthy" })], {
      filters: { health: ["down"], incidents: ["active"] },
      healthMap,
    });

    expect(selection).toEqual(["sick"]);
  });

  it("orders by health and then criticality", () => {
    const healthMap: Record<string, IServiceHealthSnapshot> = {
      down: { service_id: "down", health: "down", incidents: [], last_deployed_at: null },
      healthy: { service_id: "healthy", health: "healthy", incidents: [], last_deployed_at: null },
    };

    const selection = select(
      [
        makeService({ id: "healthy", name: "A", criticality: "low" }),
        makeService({ id: "down", name: "Z", criticality: "critical" }),
      ],
      { orderBy: "health", healthMap }
    );

    expect(selection).toEqual(["down", "healthy"]);
  });

  it("exports the reserved workspace filter key", () => {
    expect(WORKSPACE_SERVICE_FILTER_KEY).toBe("__workspace__");
  });
});
