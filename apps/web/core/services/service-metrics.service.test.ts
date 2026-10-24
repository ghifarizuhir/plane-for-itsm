/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import { ServiceMetricsService } from "./service-metrics.service";

describe("ServiceMetricsService", () => {
  it("fetches metric definitions", async () => {
    const service = new ServiceMetricsService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: { metrics: [] } } as never);
    await expect(service.getMetrics("itsm", "s1")).resolves.toEqual([]);
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/services/s1/metrics/");
  });

  it("passes range params to buckets", async () => {
    const service = new ServiceMetricsService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: {} } as never);
    await service.getBuckets("itsm", "s1", "m1", {
      from: "2026-10-24T09:00:00Z",
      to: "2026-10-24T10:00:00Z",
      resolution: 300,
    });
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/services/s1/metrics/m1/buckets/", {
      params: {
        from: "2026-10-24T09:00:00Z",
        to: "2026-10-24T10:00:00Z",
        resolution: 300,
      },
    });
  });

  it("creates, updates, rotates, and deletes sources", async () => {
    const service = new ServiceMetricsService();
    const post = vi.spyOn(service, "post").mockResolvedValue({ data: { id: "s" } } as never);
    const patch = vi.spyOn(service, "patch").mockResolvedValue({ data: { id: "s" } } as never);
    const del = vi.spyOn(service, "delete").mockResolvedValue({ data: {} } as never);

    await service.createSource("itsm", { name: "Prometheus" });
    await service.updateSource("itsm", "s", { name: "Prom" });
    await service.rotateSource("itsm", "s");
    await service.deleteSource("itsm", "s");

    expect(post).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/", { name: "Prometheus" });
    expect(patch).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/s/", { name: "Prom" });
    expect(post).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/s/rotate/", {});
    expect(del).toHaveBeenCalledWith("/api/workspaces/itsm/metric-sources/s/");
  });
});
