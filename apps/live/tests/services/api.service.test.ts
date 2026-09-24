/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, it, expect } from "vitest";

process.env.API_BASE_URL = "http://localhost:8000";
process.env.LIVE_SERVER_SECRET_KEY = "test-secret";

const { buildDefaultHeaders } = await import("@/services/api.service");

describe("APIService default headers", () => {
  it("sets Origin when WEB_BASE_URL is provided", () => {
    expect(buildDefaultHeaders("https://dashboard.terraline.space")).toEqual({
      Origin: "https://dashboard.terraline.space",
    });
  });

  it("omits Origin when WEB_BASE_URL is empty or undefined", () => {
    expect(buildDefaultHeaders(undefined)).toEqual({});
    expect(buildDefaultHeaders("")).toEqual({});
    expect(buildDefaultHeaders("   ")).toEqual({});
  });
});
