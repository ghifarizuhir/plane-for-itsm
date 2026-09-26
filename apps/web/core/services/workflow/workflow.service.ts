/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { API_BASE_URL } from "@plane/constants";
import type {
  TWorkflow,
  TWorkflowMap,
  TWorkflowPayload,
  TWorkflowState,
  TWorkflowStatePayload,
  TWorkflowTransition,
  TWorkItemType,
  TWorkItemTypePayload,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

export class WorkflowService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getWorkflows(workspaceSlug: string): Promise<TWorkflow[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/workflows/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkflow(workspaceSlug: string, data: TWorkflowPayload): Promise<TWorkflow> {
    return this.post(`/api/workspaces/${workspaceSlug}/workflows/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateWorkflow(workspaceSlug: string, workflowId: string, data: Partial<TWorkflowPayload>): Promise<TWorkflow> {
    return this.patch(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkflow(workspaceSlug: string, workflowId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async getWorkflowStates(workspaceSlug: string, workflowId: string): Promise<TWorkflowState[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkflowState(
    workspaceSlug: string,
    workflowId: string,
    data: TWorkflowStatePayload
  ): Promise<TWorkflowState> {
    return this.post(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateWorkflowState(
    workspaceSlug: string,
    workflowId: string,
    stateId: string,
    data: Partial<TWorkflowStatePayload>
  ): Promise<TWorkflowState> {
    return this.patch(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/${stateId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkflowState(workspaceSlug: string, workflowId: string, stateId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/${stateId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async getWorkflowTransitions(workspaceSlug: string, workflowId: string): Promise<TWorkflowTransition[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/transitions/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkflowTransition(
    workspaceSlug: string,
    workflowId: string,
    data: { from_state_id: string; to_state_id: string }
  ): Promise<TWorkflowTransition> {
    return this.post(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/transitions/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkflowTransition(workspaceSlug: string, workflowId: string, transitionId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/transitions/${transitionId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
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

  async getWorkflowMap(workspaceSlug: string, projectId: string): Promise<TWorkflowMap> {
    return this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/workflow-map/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
