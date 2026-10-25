/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { IService } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { syncGraphNodes } from "./node-sync";

const makeNode = (id: string, service = { id, name: id }): Node => ({
  id,
  type: "service",
  position: { x: 0, y: 0 },
  data: {
    service: service as unknown as IService,
    statusLabel: "Active",
    criticalityLabel: "Medium",
    typeLabel: "Internal",
    kindLabel: "Database",
    healthLabel: "Healthy",
    ownerProjectIdentifier: "P1",
    ownerProjectName: "Project One",
    health: "healthy",
    incidents: [],
    lastDeployedAt: null,
    noDeploymentLabel: "No deployment",
    dimmed: false,
  },
});

describe("syncGraphNodes", () => {
  it("returns the previous array when nothing changed", () => {
    const prev = [makeNode("a"), makeNode("b")];
    const next = prev.map((node) => ({ ...node, data: { ...node.data } }));

    expect(syncGraphNodes(prev, next)).toBe(prev);
  });

  it("applies a new position", () => {
    const prev = [makeNode("a")];
    const next = [{ ...prev[0], position: { x: 100, y: 200 } }];

    const result = syncGraphNodes(prev, next);

    expect(result[0].position).toEqual({ x: 100, y: 200 });
  });

  it("applies new labels", () => {
    const prev = [makeNode("a")];
    const next = [
      {
        ...prev[0],
        data: { ...prev[0].data, healthLabel: "Degraded" },
      },
    ];

    const result = syncGraphNodes(prev, next);

    expect((result[0].data as { healthLabel: string }).healthLabel).toBe("Degraded");
  });

  it("applies a new kind label", () => {
    const prev = [makeNode("a")];
    const next = [{ ...prev[0], data: { ...prev[0].data, kindLabel: "Server" } }];

    const result = syncGraphNodes(prev, next);

    expect((result[0].data as { kindLabel: string }).kindLabel).toBe("Server");
  });

  it("applies a new service reference", () => {
    const prev = [makeNode("a")];
    const next = [makeNode("a", { id: "a", name: "Renamed" })];

    const result = syncGraphNodes(prev, next);

    expect((result[0].data as { service: IService }).service.name).toBe("Renamed");
  });

  it("applies focus state changes", () => {
    const prev = [makeNode("a")];
    const next = [{ ...prev[0], data: { ...prev[0].data, focused: true, focusDimmed: false } }];

    const result = syncGraphNodes(prev, next);

    expect((result[0].data as { focused: boolean }).focused).toBe(true);
  });

  it("preserves selection", () => {
    const prev = [{ ...makeNode("a"), selected: true }];
    const next = [{ ...makeNode("a"), selected: false }];

    const result = syncGraphNodes(prev, next);

    expect(result[0].selected).toBe(true);
  });

  it("does not move a node while it is being dragged", () => {
    const dragging = { ...makeNode("a"), dragging: true };
    const prev = [dragging];
    const next = [{ ...makeNode("a"), position: { x: 500, y: 600 } }];

    const result = syncGraphNodes(prev, next);

    expect(result[0]).toBe(dragging);
    expect(result[0].position).toEqual({ x: 0, y: 0 });
  });
});
