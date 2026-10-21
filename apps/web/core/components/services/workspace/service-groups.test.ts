/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import { buildWorkspaceServiceGroups } from "./service-groups";

const services: Record<string, { name: string; owner_project_id: string | null }> = {
  "s-1": { name: "Payment Gateway", owner_project_id: "p-2" },
  "s-2": { name: "Billing API", owner_project_id: "p-2" },
  "s-3": { name: "Customer Portal", owner_project_id: "p-1" },
  "s-4": { name: "Legacy Worker", owner_project_id: null },
  "s-5": { name: "Orphan", owner_project_id: "p-missing" },
  "s-6": { name: "Auth", owner_project_id: "p-3" },
};

const projects: Record<string, { name: string; identifier: string }> = {
  "p-1": { name: "Customer", identifier: "CRM" },
  "p-2": { name: "Payments", identifier: "PAY" },
  "p-3": { name: "Identity", identifier: "IAM" },
};

const getServiceById = (id: string) => services[id];
const getProjectById = (id: string) => projects[id];

describe("buildWorkspaceServiceGroups", () => {
  it("groups services by owner project and sorts groups by name", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-2", "s-3", "s-6"], getServiceById, getProjectById);

    expect(groups.map((group) => group.key)).toEqual(["p-1", "p-3", "p-2"]);
    expect(groups.map((group) => group.name)).toEqual(["Customer", "Identity", "Payments"]);
  });

  it("sorts services alphabetically inside a group", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-2"], getServiceById, getProjectById);

    expect(groups).toHaveLength(1);
    expect(groups[0].serviceIds).toEqual(["s-2", "s-1"]);
  });

  it("places unassigned and unresolved owners in a final unassigned group", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-4", "s-5", "s-3"], getServiceById, getProjectById);

    const lastGroup = groups[groups.length - 1];
    expect(lastGroup.key).toBe("unassigned");
    expect(lastGroup.projectId).toBeNull();
    expect(lastGroup.identifier).toBeNull();
    expect(lastGroup.serviceIds).toEqual(["s-4", "s-5"]);
  });

  it("deduplicates ids and skips services that cannot be resolved", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-1", "s-missing", "s-2"], getServiceById, getProjectById);

    expect(groups).toHaveLength(1);
    expect(groups[0].serviceIds).toEqual(["s-2", "s-1"]);
  });

  it("omits empty groups", () => {
    const groups = buildWorkspaceServiceGroups(["s-3"], getServiceById, getProjectById);

    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("p-1");
  });
});
