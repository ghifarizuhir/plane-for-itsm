/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { autoResolutionForRange, buildTimeline, formatMetricValue, latestBucket, rangeToFromTo } from "./helpers";

describe("service metrics helpers", () => {
  it("maps ranges to resolutions", () => {
    expect(autoResolutionForRange("1h")).toBe(60);
    expect(autoResolutionForRange("6h")).toBe(60);
    expect(autoResolutionForRange("24h")).toBe(300);
    expect(autoResolutionForRange("7d")).toBe(600);
    expect(autoResolutionForRange("30d")).toBe(1800);
    expect(autoResolutionForRange("90d")).toBe(3600);
  });

  it("builds from/to from the range", () => {
    const now = new Date("2026-10-24T10:00:00.000Z");
    expect(rangeToFromTo("1h", now)).toEqual({
      from: "2026-10-24T09:00:00.000Z",
      to: "2026-10-24T10:00:00.000Z",
    });
  });

  it("formats values with units", () => {
    expect(formatMetricValue(120, "ms")).toBe("120 ms");
    expect(formatMetricValue(99.5, "%")).toBe("99.5%");
    expect(formatMetricValue(null, "ms")).toBe("—");
  });

  it("fills gaps with null points", () => {
    const buckets = [
      {
        bucket_start: "2026-10-24T10:00:00.000Z",
        count: 60,
        sum: 7200,
        min: 80,
        max: 250,
        avg: 120,
        last: 130,
      },
      {
        bucket_start: "2026-10-24T10:05:00.000Z",
        count: 60,
        sum: 6000,
        min: 70,
        max: 200,
        avg: 100,
        last: 90,
      },
    ];
    const points = buildTimeline(buckets, 300, "2026-10-24T10:00:00.000Z", "2026-10-24T10:10:00.000Z", "1h");
    expect(points).toHaveLength(2);
    expect(points[0].avg).toBe(120);
    expect(points[1].avg).toBe(100);
    expect(points[0].domainValue).toBe(250);
  });

  it("returns the latest non-empty bucket", () => {
    expect(latestBucket([])).toBeNull();
    expect(
      latestBucket([
        { bucket_start: "a", count: null, sum: null, min: null, max: null, avg: null, last: null },
        { bucket_start: "b", count: 1, sum: 2, min: 1, max: 3, avg: 2, last: 2 },
      ])?.bucket_start
    ).toBe("b");
  });
});
