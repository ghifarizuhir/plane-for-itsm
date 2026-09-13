/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceDependency, TServiceWorkItemLink } from "@plane/types";
// services
import { ServiceMockRepository } from "@/services/service-mock.repository";

/**
 * Async facade over the localStorage mock.
 * Swap the method bodies for APIService HTTP calls when the backend lands;
 * signatures and return shapes stay identical.
 */
export class ServiceService {
  repository: ServiceMockRepository;

  constructor(repository: ServiceMockRepository = new ServiceMockRepository()) {
    this.repository = repository;
  }

  async getServices(workspaceSlug: string, workspaceId: string, projectId: string): Promise<IService[]> {
    return Promise.resolve(this.repository.getServices(workspaceSlug, workspaceId, projectId));
  }

  async getDependencies(workspaceSlug: string, workspaceId: string, projectId: string): Promise<IServiceDependency[]> {
    return Promise.resolve(this.repository.getDependencies(workspaceSlug, workspaceId, projectId));
  }

  async getWorkItemLinks(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string
  ): Promise<TServiceWorkItemLink[]> {
    return Promise.resolve(this.repository.getLinks(workspaceSlug, workspaceId, projectId));
  }

  async createService(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ): Promise<IService> {
    return Promise.resolve(this.repository.createService(workspaceSlug, workspaceId, projectId, data));
  }

  async updateService(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ): Promise<IService> {
    const updated = this.repository.updateService(workspaceSlug, workspaceId, projectId, serviceId, data);
    if (!updated) throw new Error("Service not found");
    return Promise.resolve(updated);
  }

  async deleteService(workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string): Promise<void> {
    this.repository.deleteService(workspaceSlug, workspaceId, projectId, serviceId);
    return Promise.resolve();
  }

  async createDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ): Promise<IServiceDependency> {
    return Promise.resolve(
      this.repository.createDependency(workspaceSlug, workspaceId, projectId, fromServiceId, toServiceId)
    );
  }

  async deleteDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    dependencyId: string
  ): Promise<void> {
    this.repository.deleteDependency(workspaceSlug, workspaceId, projectId, dependencyId);
    return Promise.resolve();
  }

  async updateNodePosition(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ): Promise<IService> {
    const updated = this.repository.updateService(workspaceSlug, workspaceId, projectId, serviceId, { position });
    if (!updated) throw new Error("Service not found");
    return Promise.resolve(updated);
  }

  async linkWorkItem(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ): Promise<TServiceWorkItemLink> {
    return Promise.resolve(this.repository.linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, issue));
  }

  async unlinkWorkItem(workspaceSlug: string, workspaceId: string, projectId: string, linkId: string): Promise<void> {
    this.repository.unlinkWorkItem(workspaceSlug, workspaceId, projectId, linkId);
    return Promise.resolve();
  }
}
