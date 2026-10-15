/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, it, expect } from "vitest";
import { mergeRefreshedCookies } from "@/utils/cookies";

describe("mergeRefreshedCookies", () => {
  it("replaces rotated access + refresh cookies, keeps the rest", () => {
    const merged = mergeRefreshedCookies("plane_at=old-at; plane_rt=old-rt; other=1", [
      "plane_at=new-at; HttpOnly; Path=/; Max-Age=900; SameSite=Lax",
      "plane_rt=new-rt; HttpOnly; Path=/; Max-Age=2592000; SameSite=Lax",
    ]);
    expect(merged).toBe("other=1; plane_at=new-at; plane_rt=new-rt");
  });

  it("handles __Host- prefixed pairs", () => {
    const merged = mergeRefreshedCookies("__Host-plane_at=old; plane_rt=old-rt", [
      "__Host-plane_at=new; HttpOnly; Path=/; Max-Age=900; SameSite=Lax",
    ]);
    expect(merged).toBe("plane_rt=old-rt; __Host-plane_at=new");
  });

  it("returns the old cookie unchanged when there are no set-cookies", () => {
    expect(mergeRefreshedCookies("plane_at=a; plane_rt=b", [])).toBe("plane_at=a; plane_rt=b");
  });
});
