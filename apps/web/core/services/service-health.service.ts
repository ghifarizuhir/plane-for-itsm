/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type { IServiceHealthSnapshot } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class ServiceHealthService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getHealth(workspaceSlug: string, projectId: string | null): Promise<IServiceHealthSnapshot[]> {
    const url = projectId
      ? `/api/workspaces/${workspaceSlug}/projects/${projectId}/services/health/`
      : `/api/workspaces/${workspaceSlug}/services/health/`;
    return this.get(url).then((response) => response?.data);
  }
}
