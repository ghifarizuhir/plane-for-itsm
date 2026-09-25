/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import { normalizeServiceUrl } from "./service.helpers";

describe("normalizeServiceUrl", () => {
  it("returns null for empty and whitespace-only values", () => {
    expect(normalizeServiceUrl("")).toBeNull();
    expect(normalizeServiceUrl("   ")).toBeNull();
  });

  it("trims surrounding whitespace", () => {
    expect(normalizeServiceUrl("  https://github.com/plane/plane  ")).toBe("https://github.com/plane/plane");
  });

  it("keeps a normal URL unchanged", () => {
    expect(normalizeServiceUrl("https://docs.plane.so")).toBe("https://docs.plane.so");
  });
});
