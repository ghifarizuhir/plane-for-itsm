/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { API_BASE_URL } from "@plane/constants";
import type { TDocumentExportSettings, TDocumentExportSettingsPayload } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class DocumentExportSettingsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async fetchSettings(workspaceSlug: string): Promise<TDocumentExportSettings> {
    return this.get(`/api/workspaces/${workspaceSlug}/document-export-settings/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateSettings(workspaceSlug: string, data: TDocumentExportSettingsPayload): Promise<TDocumentExportSettings> {
    return this.patch(`/api/workspaces/${workspaceSlug}/document-export-settings/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
