/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceHealthSnapshot } from "@plane/types";
// helpers
import { buildHealthSnapshot } from "@/services/service-health.helpers";

/**
 * Mock health source. The backend has no health data yet, so snapshots are
 * derived locally. This class is the single seam to replace with an HTTP call
 * (`GET /api/workspaces/:slug/projects/:projectId/services/health/`) later —
 * stores and components depend only on this signature.
 */
export class ServiceHealthService {
  async getHealth(
    _workspaceSlug: string,
    _workspaceId: string,
    _projectId: string,
    services: IService[]
  ): Promise<IServiceHealthSnapshot[]> {
    return services.map((service) => buildHealthSnapshot(service));
  }
}
