/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { API_BASE_URL } from "@plane/constants";
import type { TWorkItemType, TWorkItemTypePayload } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class WorkItemTypeService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getWorkItemTypes(workspaceSlug: string): Promise<TWorkItemType[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/work-item-types/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkItemType(workspaceSlug: string, data: TWorkItemTypePayload): Promise<TWorkItemType> {
    return this.post(`/api/workspaces/${workspaceSlug}/work-item-types/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateWorkItemType(
    workspaceSlug: string,
    typeId: string,
    data: Partial<TWorkItemTypePayload>
  ): Promise<TWorkItemType> {
    return this.patch(`/api/workspaces/${workspaceSlug}/work-item-types/${typeId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkItemType(workspaceSlug: string, typeId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/work-item-types/${typeId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async importWorkItemTypes(workspaceSlug: string, projectId: string, typeIds: string[]): Promise<void> {
    return this.post(`/api/workspaces/${workspaceSlug}/projects/${projectId}/import-work-item-types/`, {
      work_item_types: typeIds,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async unlinkWorkItemType(workspaceSlug: string, projectId: string, typeId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/projects/${projectId}/work-item-types/${typeId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
