/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IService,
  IServiceDependency,
  IServiceProjectLink,
  TServiceDependencyType,
  TServiceWorkItemLink,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TServiceErrorBody = { detail?: string; error?: string; [key: string]: unknown };

/**
 * Normalizes an axios error into a real Error whose `.message` carries the
 * backend message, while also exposing `.detail`/`.error`. Components use
 * both styles: the modal reads `err.detail/err.error`, the graph and
 * dependency views branch on `error instanceof Error`.
 */
const toServiceError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TServiceErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  return normalized;
};

export class ServiceService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getServices(workspaceSlug: string, _workspaceId: string, projectId: string): Promise<IService[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/services/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getDependencies(workspaceSlug: string, _workspaceId: string, projectId: string): Promise<IServiceDependency[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-dependencies/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkItemLinks(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string
  ): Promise<TServiceWorkItemLink[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-issues/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async createService(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ): Promise<IService> {
    return this.post(`/api/workspaces/${workspaceSlug}/projects/${projectId}/services/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateService(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ): Promise<IService> {
    return this.patch(`/api/workspaces/${workspaceSlug}/projects/${projectId}/services/${serviceId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteService(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    serviceId: string
  ): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/projects/${projectId}/services/${serviceId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async createDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string | null,
    fromServiceId: string,
    toServiceId: string,
    dependencyType?: TServiceDependencyType
  ): Promise<IServiceDependency> {
    return this.post(this.dependencyUrl(workspaceSlug, projectId), {
      from_service_id: fromServiceId,
      to_service_id: toServiceId,
      dependency_type: dependencyType,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string | null,
    dependencyId: string,
    dependencyType: TServiceDependencyType
  ): Promise<IServiceDependency> {
    return this.patch(`${this.dependencyUrl(workspaceSlug, projectId)}${dependencyId}/`, {
      dependency_type: dependencyType,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string | null,
    dependencyId: string
  ): Promise<void> {
    return this.delete(`${this.dependencyUrl(workspaceSlug, projectId)}${dependencyId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  private dependencyUrl(workspaceSlug: string, projectId: string | null) {
    return projectId
      ? `/api/workspaces/${workspaceSlug}/projects/${projectId}/service-dependencies/`
      : `/api/workspaces/${workspaceSlug}/service-dependencies/`;
  }

  async updateNodePosition(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    serviceId: string,
    position: { x: number; y: number } | null
  ): Promise<IService> {
    return projectId
      ? this.updateService(workspaceSlug, workspaceId, projectId, serviceId, { position })
      : this.updateWorkspaceService(workspaceSlug, serviceId, { position });
  }

  async linkWorkItem(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ): Promise<TServiceWorkItemLink> {
    return this.post(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-issues/`, {
      service_id: serviceId,
      issue_id: issue.id,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async linkWorkItems(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issues: { id: string; identifier?: string; name?: string }[]
  ): Promise<TServiceWorkItemLink[]> {
    return Promise.all(
      issues.map((issue) => this.linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, issue))
    );
  }

  async unlinkWorkItem(workspaceSlug: string, _workspaceId: string, projectId: string, linkId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-issues/${linkId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkspaceServices(
    workspaceSlug: string,
    params?: { project?: string; q?: string; status?: string; criticality?: string; link?: string }
  ): Promise<IService[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/services/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkspaceDependencies(workspaceSlug: string): Promise<IServiceDependency[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/service-dependencies/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkspaceWorkItemLinks(workspaceSlug: string): Promise<TServiceWorkItemLink[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/service-issues/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async createWorkspaceService(workspaceSlug: string, data: Partial<IService>): Promise<IService> {
    return this.post(`/api/workspaces/${workspaceSlug}/services/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateWorkspaceService(workspaceSlug: string, serviceId: string, data: Partial<IService>): Promise<IService> {
    return this.patch(`/api/workspaces/${workspaceSlug}/services/${serviceId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteWorkspaceService(workspaceSlug: string, serviceId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/services/${serviceId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async addProjectLink(workspaceSlug: string, serviceId: string, projectId: string): Promise<IServiceProjectLink> {
    return this.post(`/api/workspaces/${workspaceSlug}/services/${serviceId}/project-links/`, {
      project_id: projectId,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async removeProjectLink(workspaceSlug: string, serviceId: string, linkId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/services/${serviceId}/project-links/${linkId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async transferServiceOwner(workspaceSlug: string, serviceId: string, projectId: string): Promise<IService> {
    return this.post(`/api/workspaces/${workspaceSlug}/services/${serviceId}/transfer-owner/`, {
      project_id: projectId,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
}
