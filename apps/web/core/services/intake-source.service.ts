/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type { TIntakeSource, TIntakeSourcePayload } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class IntakeSourceService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private base(workspaceSlug: string, projectId: string): string {
    return `/api/workspaces/${workspaceSlug}/projects/${projectId}/intake-sources`;
  }

  async list(workspaceSlug: string, projectId: string): Promise<TIntakeSource[]> {
    return this.get(`${this.base(workspaceSlug, projectId)}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async create(workspaceSlug: string, projectId: string, data: TIntakeSourcePayload): Promise<TIntakeSource> {
    return this.post(`${this.base(workspaceSlug, projectId)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async update(
    workspaceSlug: string,
    projectId: string,
    sourceId: string,
    data: Partial<TIntakeSourcePayload> & { is_active?: boolean }
  ): Promise<TIntakeSource> {
    return this.patch(`${this.base(workspaceSlug, projectId)}/${sourceId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async destroy(workspaceSlug: string, projectId: string, sourceId: string): Promise<void> {
    return this.delete(`${this.base(workspaceSlug, projectId)}/${sourceId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async rotate(workspaceSlug: string, projectId: string, sourceId: string): Promise<{ id: string; token: string }> {
    return this.post(`${this.base(workspaceSlug, projectId)}/${sourceId}/rotate/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
