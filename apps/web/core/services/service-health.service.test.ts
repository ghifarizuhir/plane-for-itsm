/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import { ServiceHealthService } from "./service-health.service";

describe("ServiceHealthService", () => {
  it("uses the workspace endpoint without a project", async () => {
    const service = new ServiceHealthService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: [] } as never);

    await expect(service.getHealth("itsm", null)).resolves.toEqual([]);
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/services/health/");
  });

  it("uses the project compat endpoint with a project", async () => {
    const service = new ServiceHealthService();
    const get = vi.spyOn(service, "get").mockResolvedValue({ data: [] } as never);

    await expect(service.getHealth("itsm", "p1")).resolves.toEqual([]);
    expect(get).toHaveBeenCalledWith("/api/workspaces/itsm/projects/p1/services/health/");
  });
});
