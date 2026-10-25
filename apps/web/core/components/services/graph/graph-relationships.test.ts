/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IServiceDependency } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { getServiceRelationships } from "./graph-relationships";

const makeDependency = (id: string, from: string, to: string): IServiceDependency => ({
  id,
  workspace_id: "workspace-1",
  project_id: "p1",
  from_service_id: from,
  to_service_id: to,
  dependency_type: "depends_on",
  created_at: "2026-10-25T00:00:00Z",
});

describe("getServiceRelationships", () => {
  it("keeps both incoming and outgoing first-degree edges", () => {
    const relationships = getServiceRelationships("a", [
      makeDependency("a-b", "a", "b"),
      makeDependency("c-a", "c", "a"),
      makeDependency("b-d", "b", "d"),
    ]);

    expect(relationships.nodeIds).toEqual(new Set(["a", "b", "c"]));
    expect(relationships.edgeIds).toEqual(new Set(["a-b", "c-a"]));
  });

  it("does not include edges between two neighbours", () => {
    const relationships = getServiceRelationships("a", [
      makeDependency("a-b", "a", "b"),
      makeDependency("a-c", "a", "c"),
      makeDependency("b-c", "b", "c"),
    ]);

    expect(relationships.edgeIds).toEqual(new Set(["a-b", "a-c"]));
  });

  it("returns only the selected service when it has no dependencies", () => {
    const relationships = getServiceRelationships("solo", []);

    expect(relationships.nodeIds).toEqual(new Set(["solo"]));
    expect(relationships.edgeIds.size).toBe(0);
  });
});
