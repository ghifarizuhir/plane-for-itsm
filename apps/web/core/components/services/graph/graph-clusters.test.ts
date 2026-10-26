/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { ILiteService, IService } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { getServiceClusters } from "./graph-clusters";
import { SERVICE_NODE_HEIGHT, SERVICE_NODE_WIDTH } from "./use-graph-layout";

const makeService = (id: string, overrides: Partial<IService> = {}): IService => ({
  id,
  workspace_id: "workspace-1",
  project_id: "p1",
  owner_project_id: "p1",
  project_links: [],
  access_level: "full",
  name: id,
  description: "",
  description_html: "",
  status: "active",
  criticality: "medium",
  type: "internal",
  kind: "service",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
  position: { x: 100, y: 100 },
  sort_order: 1,
  created_at: "2026-10-25T00:00:00Z",
  updated_at: "2026-10-25T00:00:00Z",
  created_by: null,
  updated_by: null,
  ...overrides,
});

const makeLite = (id: string): ILiteService => ({
  id,
  name: id,
  status: "active",
  criticality: "medium",
  access_level: "lite",
});

const makeNode = (id: string, x: number, y: number): Node => ({
  id,
  position: { x, y },
  measured: { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT },
  data: {},
});

const labels: Record<string, string> = {
  "service.kind_values.database": "Database",
  "service.kind_values.service": "Service",
  "service.kind_values.server": "Server",
  "service.graph.unassigned": "Unassigned",
  "service.graph.restricted": "Restricted",
};
const t = (key: string) => labels[key] ?? key;

describe("getServiceClusters", () => {
  it("computes frame bounds with padding and header", () => {
    const service = makeService("db", { kind: "database" });
    const clusters = getServiceClusters([makeNode("db", 100, 100)], [service], {}, t);

    expect(clusters).toHaveLength(1);
    expect(clusters[0]).toMatchObject({
      x: 84,
      y: 56,
      width: 288,
      height: 164,
      label: "Database",
      count: 1,
      serviceIds: ["db"],
    });
  });

  it("counts every member of a cluster", () => {
    const services = [
      makeService("db-1", { kind: "database" }),
      makeService("db-2", { kind: "database" }),
      makeService("svc", { kind: "service" }),
    ];
    const clusters = getServiceClusters(
      [makeNode("db-1", 0, 0), makeNode("db-2", 0, 200), makeNode("svc", 400, 0)],
      services,
      {},
      t
    );

    expect(clusters.map((cluster) => [cluster.label, cluster.count])).toEqual([
      ["Service", 1],
      ["Database", 2],
    ]);
  });

  it("prefixes the project identifier when multiple projects are visible", () => {
    const services = [
      makeService("db", { kind: "database" }),
      makeService("svc", { owner_project_id: "p2", project_id: "p2" }),
    ];
    const clusters = getServiceClusters(
      [makeNode("db", 0, 0), makeNode("svc", 400, 0)],
      services,
      { p1: { identifier: "PAY", name: "Payments" }, p2: { identifier: "AUTH", name: "Auth" } },
      t
    );

    expect(clusters.map((cluster) => cluster.label)).toEqual(["PAY · Database", "AUTH · Service"]);
  });

  it("labels unassigned and restricted groups", () => {
    const services = [
      makeService("named"),
      makeService("unassigned", { owner_project_id: null, project_id: null, kind: "server" }),
      makeLite("lite"),
    ];
    const clusters = getServiceClusters(
      [makeNode("named", 0, 0), makeNode("unassigned", 400, 0), makeNode("lite", 800, 0)],
      services,
      {},
      t
    );

    expect(clusters.map((cluster) => cluster.label)).toEqual(["Service", "Unassigned · Server", "Restricted"]);
  });

  it("exposes split labels so the header can style project and kind separately", () => {
    const services = [
      makeService("db", { kind: "database" }),
      makeService("svc", { owner_project_id: "p2", project_id: "p2" }),
      makeLite("lite"),
    ];
    const clusters = getServiceClusters(
      [makeNode("db", 0, 0), makeNode("svc", 400, 0), makeNode("lite", 800, 0)],
      services,
      { p1: { identifier: "PAY", name: "Payments" }, p2: { identifier: "AUTH", name: "Auth" } },
      t
    );

    expect(clusters.map((cluster) => [cluster.projectLabel, cluster.kindLabel])).toEqual([
      ["PAY", "Database"],
      ["AUTH", "Service"],
      ["Restricted", null],
    ]);
  });

  it("hides the project label when only one project is visible", () => {
    const service = makeService("db", { kind: "database" });
    const [cluster] = getServiceClusters([makeNode("db", 0, 0)], [service], {}, t);

    expect(cluster.projectLabel).toBeNull();
    expect(cluster.kindLabel).toBe("Database");
  });

  it("moves the frame when a member node moves", () => {
    const service = makeService("db", { kind: "database" });
    const before = getServiceClusters([makeNode("db", 100, 100)], [service], {}, t)[0];
    const after = getServiceClusters([makeNode("db", 300, 100)], [service], {}, t)[0];

    expect(after.x - before.x).toBe(200);
  });
});
