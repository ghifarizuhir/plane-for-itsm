/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import { isValidServiceUrl, normalizeServiceUrl } from "./service.helpers";

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
