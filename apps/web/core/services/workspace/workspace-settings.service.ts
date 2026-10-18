/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { API_BASE_URL } from "@plane/constants";
import type { TWorkspaceSettings, TWorkspaceSettingsPayload } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class WorkspaceSettingsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async fetchSettings(workspaceSlug: string): Promise<TWorkspaceSettings> {
    return this.get(`/api/workspaces/${workspaceSlug}/settings/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateSettings(workspaceSlug: string, data: TWorkspaceSettingsPayload): Promise<TWorkspaceSettings> {
    return this.patch(`/api/workspaces/${workspaceSlug}/settings/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
