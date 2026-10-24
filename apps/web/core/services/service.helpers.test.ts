/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import {
  dependencyExists,
  groupServices,
  isLiteService,
  isValidServiceUrl,
  normalizeServiceUrl,
  validateDependency,
} from "./service.helpers";

const makeService = (id: string, overrides: Record<string, unknown> = {}) =>
  ({
    id,
    name: id,
    status: "active",
    criticality: "medium",
    type: "internal",
    ...overrides,
  }) as never;

describe("isLiteService", () => {
  it("detects lite projections", () => {
    expect(isLiteService({ access_level: "lite" } as never)).toBe(true);
    expect(isLiteService({ access_level: "full" } as never)).toBe(false);
  });
});

describe("normalizeServiceUrl", () => {
  it("returns null for empty and whitespace-only values", () => {
    expect(normalizeServiceUrl("")).toBeNull();
    expect(normalizeServiceUrl("   ")).toBeNull();
  });

  it("trims surrounding whitespace", () => {
    expect(normalizeServiceUrl("  https://github.com/plane/plane  ")).toBe("https://github.com/plane/plane");
  });

  it("keeps a normal URL unchanged", () => {
    expect(normalizeServiceUrl("https://docs.terraline.space")).toBe("https://docs.terraline.space");
  });
});

describe("isValidServiceUrl", () => {
  it("accepts an absolute https URL", () => {
    expect(isValidServiceUrl("https://github.com/plane/plane")).toBe(true);
  });

  it("accepts a localhost http URL", () => {
    expect(isValidServiceUrl("http://localhost:3000")).toBe(true);
  });

  it("rejects a host without a scheme", () => {
    expect(isValidServiceUrl("github.com")).toBe(false);
  });

  it("rejects a non-URL string", () => {
    expect(isValidServiceUrl("not a url")).toBe(false);
  });

  it("rejects an empty string", () => {
    expect(isValidServiceUrl("")).toBe(false);
  });
});

describe("typed dependency validation", () => {
  const dependencies = [
    {
      id: "dep-1",
      workspace_id: "ws-1",
      project_id: null,
      from_service_id: "service-1",
      to_service_id: "service-2",
      dependency_type: "uses",
      created_at: "2026-10-20T00:00:00Z",
    },
  ] as const;

  it("treats the same pair with another type as a new edge", () => {
    expect(dependencyExists([...dependencies], "service-1", "service-2", "uses")).toBe(true);
    expect(dependencyExists([...dependencies], "service-1", "service-2", "depends_on")).toBe(false);
  });

  it("validates duplicates per type", () => {
    expect(validateDependency([...dependencies], "service-1", "service-2", "uses")).toBe(
      "This dependency already exists."
    );
    expect(validateDependency([...dependencies], "service-1", "service-2", "runs_on")).toBeNull();
  });
});

describe("groupServices", () => {
  it("groups by status in lifecycle order", () => {
    const services = [
      makeService("retired", { status: "retired" }),
      makeService("active", { status: "active" }),
      makeService("planned", { status: "planned" }),
    ];

    const groups = groupServices(services, "status", {});
    expect(groups.map((group) => group.key)).toEqual(["active", "planned", "retired"]);
    expect(groups[0]?.services.map((service) => service.id)).toEqual(["active"]);
  });

  it("groups by health with missing snapshots as unknown", () => {
    const services = [makeService("a"), makeService("b")];
    const healthMap = {
      a: { service_id: "a", health: "degraded", incidents: [], last_deployed_at: null },
    } as never;

    const groups = groupServices(services, "health", healthMap);
    expect(groups.map((group) => group.key)).toEqual(["degraded", "unknown"]);
  });

  it("returns a single flat group when group by is null", () => {
    const services = [makeService("a"), makeService("b")];

    const groups = groupServices(services, null, {});
    expect(groups).toHaveLength(1);
    expect(groups[0]?.services).toHaveLength(2);
  });
});
