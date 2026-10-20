/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import {
  dependencyExists,
  isLiteService,
  isValidServiceUrl,
  normalizeServiceUrl,
  validateDependency,
} from "./service.helpers";

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
