/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type { IService, IServiceDependency, TServiceWorkItemLink } from "@plane/types";
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
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ): Promise<IServiceDependency> {
    return this.post(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-dependencies/`, {
      from_service_id: fromServiceId,
      to_service_id: toServiceId,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    dependencyId: string
  ): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-dependencies/${dependencyId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateNodePosition(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ): Promise<IService> {
    return this.updateService(workspaceSlug, workspaceId, projectId, serviceId, { position });
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

  async unlinkWorkItem(workspaceSlug: string, _workspaceId: string, projectId: string, linkId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/projects/${projectId}/service-issues/${linkId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
}
