/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ILiteService, IService } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { getServiceGroups } from "./graph-groups";
import {
  CLUSTER_HEADER_HEIGHT,
  CLUSTER_PADDING,
  getLayoutedElements,
  SERVICE_NODE_HEIGHT,
  SERVICE_NODE_WIDTH,
} from "./use-graph-layout";

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
  position: null,
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

const getNodes = (services: (IService | ILiteService)[]) => getLayoutedElements(services, []).nodes;

const boundsFor = (
  nodes: ReturnType<typeof getNodes>,
  ids: string[]
): { x: number; y: number; width: number; height: number } => {
  const positions = nodes.filter((node) => ids.includes(node.id)).map((node) => node.position);
  const minX = Math.min(...positions.map((position) => position.x));
  const minY = Math.min(...positions.map((position) => position.y));
  const maxX = Math.max(...positions.map((position) => position.x + SERVICE_NODE_WIDTH));
  const maxY = Math.max(...positions.map((position) => position.y + SERVICE_NODE_HEIGHT));
  return {
    x: minX - CLUSTER_PADDING,
    y: minY - CLUSTER_HEADER_HEIGHT - CLUSTER_PADDING,
    width: maxX - minX + CLUSTER_PADDING * 2,
    height: maxY - minY + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2,
  };
};

const overlaps = (
  a: { x: number; y: number; width: number; height: number },
  b: { x: number; y: number; width: number; height: number }
) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

describe("getServiceGroups", () => {
  it("groups by project and kind, with unassigned and restricted fallbacks", () => {
    const groups = getServiceGroups([
      makeService("svc", { kind: "service" }),
      makeService("db", { kind: "database" }),
      makeService("db2", { owner_project_id: "p2", project_id: "p2", kind: "database" }),
      makeService("unassigned", { owner_project_id: null, project_id: null, kind: "server" }),
      makeLite("lite"),
    ]);

    expect(groups.map((group) => group.key)).toEqual([
      "p1::service",
      "p1::database",
      "p2::database",
      "__unassigned__::server",
      "__restricted__",
    ]);
    expect(groups.at(-1)?.restricted).toBe(true);
    expect(groups.find((group) => group.key === "__unassigned__::server")?.projectId).toBeNull();
  });
});

describe("grouped layout", () => {
  it("is deterministic", () => {
    const services = [makeService("a"), makeService("db", { kind: "database" })];

    expect(getNodes(services)).toEqual(getNodes(services));
  });

  it("wraps a large kind into a compact card grid", () => {
    const services = Array.from({ length: 12 }, (_, index) => makeService(`svc-${index}`));

    const nodes = getNodes(services);
    const xPositions = new Set(nodes.map((node) => node.position.x));
    const yPositions = new Set(nodes.map((node) => node.position.y));

    expect(xPositions.size).toBe(4);
    expect(yPositions.size).toBe(3);
  });

  it("packages different groups without overlapping", () => {
    const services = [
      makeService("p1-svc", { owner_project_id: "p1", project_id: "p1" }),
      makeService("p1-db", { owner_project_id: "p1", project_id: "p1", kind: "database" }),
      makeService("p2-svc", { owner_project_id: "p2", project_id: "p2" }),
      makeService("p2-db", { owner_project_id: "p2", project_id: "p2", kind: "database" }),
    ];
    const nodes = getNodes(services);

    const p1Service = boundsFor(nodes, ["p1-svc"]);
    const p1Db = boundsFor(nodes, ["p1-db"]);
    const p2Service = boundsFor(nodes, ["p2-svc"]);
    const p2Db = boundsFor(nodes, ["p2-db"]);

    expect(overlaps(p1Service, p1Db)).toBe(false);
    expect(overlaps(p1Service, p2Service)).toBe(false);
    expect(overlaps(p1Service, p2Db)).toBe(false);
    expect(overlaps(p1Db, p2Db)).toBe(false);
  });

  it("keeps a stored position", () => {
    const nodes = getNodes([makeService("db", { kind: "database", position: { x: 900, y: 700 } })]);

    expect(nodes.find((node) => node.id === "db")?.position).toEqual({ x: 900, y: 700 });
  });

  it("shifts unpositioned nodes toward the stored centroid of their group", () => {
    const nodes = getNodes([
      makeService("stored", { kind: "database", position: { x: 1000, y: 1000 } }),
      makeService("fresh", { kind: "database" }),
    ]);
    const fresh = nodes.find((node) => node.id === "fresh");

    expect(fresh).toBeDefined();
    expect(Math.abs((fresh?.position.x ?? 0) - 1000)).toBeLessThan(SERVICE_NODE_WIDTH * 2);
    expect(Math.abs((fresh?.position.y ?? 0) - 1000)).toBeLessThan(SERVICE_NODE_HEIGHT * 3);
  });

  it("keeps cross-group edges", () => {
    const from = makeService("from", { kind: "service" });
    const to = makeService("to", { kind: "database" });
    const { edges } = getLayoutedElements(
      [from, to],
      [
        {
          id: "dep-1",
          workspace_id: "workspace-1",
          project_id: "p1",
          from_service_id: from.id,
          to_service_id: to.id,
          dependency_type: "depends_on",
          created_at: "2026-10-25T00:00:00Z",
        },
      ]
    );

    expect(edges.map((edge) => edge.id)).toEqual(["dep-1"]);
  });
});
