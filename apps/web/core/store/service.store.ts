/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set, sortBy } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { IService, IServiceDependency, TServiceGraphData, TServiceWorkItemLink } from "@plane/types";
// helpers
import { filterServices, orderServices } from "@/services/service.helpers";
// services
import { ServiceService } from "@/services/service.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IServiceStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  serviceMap: Record<string, IService>;
  dependencyMap: Record<string, IServiceDependency>;
  workItemLinkMap: Record<string, TServiceWorkItemLink>;
  getServiceById: (serviceId: string) => IService | null;
  getProjectServiceIds: (projectId: string) => string[] | null;
  getFilteredServiceIds: (projectId: string) => string[] | null;
  getDependenciesByProject: (projectId: string) => IServiceDependency[];
  getWorkItemLinksByService: (serviceId: string) => TServiceWorkItemLink[];
  getGraphData: (projectId: string) => TServiceGraphData;
  fetchServices: (workspaceSlug: string, workspaceId: string, projectId: string) => Promise<IService[] | undefined>;
  createService: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ) => Promise<IService>;
  updateService: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ) => Promise<IService>;
  deleteService: (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => Promise<void>;
  addDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ) => Promise<IServiceDependency>;
  removeDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    dependencyId: string
  ) => Promise<void>;
  updateNodePosition: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ) => Promise<void>;
  linkWorkItem: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ) => Promise<TServiceWorkItemLink>;
  unlinkWorkItem: (workspaceSlug: string, workspaceId: string, projectId: string, linkId: string) => Promise<void>;
}

export class ServicesStore implements IServiceStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  serviceMap: Record<string, IService> = {};
  dependencyMap: Record<string, IServiceDependency> = {};
  workItemLinkMap: Record<string, TServiceWorkItemLink> = {};
  rootStore;
  serviceService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      serviceMap: observable,
      dependencyMap: observable,
      workItemLinkMap: observable,
      fetchServices: action,
      createService: action,
      updateService: action,
      deleteService: action,
      addDependency: action,
      removeDependency: action,
      updateNodePosition: action,
      linkWorkItem: action,
      unlinkWorkItem: action,
    });
    this.rootStore = _rootStore;
    this.serviceService = new ServiceService();
  }

  getServiceById = computedFn((serviceId: string) => this.serviceMap[serviceId] || null);

  getProjectServiceIds = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    const services = sortBy(
      Object.values(this.serviceMap).filter((s) => s.project_id === projectId),
      [(s) => s.sort_order]
    );
    return services.map((s) => s.id);
  });

  getFilteredServiceIds = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    const displayFilters = this.rootStore.serviceFilter.getDisplayFiltersByProjectId(projectId);
    const filters = this.rootStore.serviceFilter.getFiltersByProjectId(projectId);
    const searchQuery = this.rootStore.serviceFilter.searchQuery;
    const services = Object.values(this.serviceMap).filter((s) => s.project_id === projectId);
    const filtered = filterServices(services, filters, searchQuery);
    return orderServices(filtered, displayFilters?.order_by).map((s) => s.id);
  });

  getDependenciesByProject = computedFn((projectId: string) =>
    Object.values(this.dependencyMap).filter((d) => d.project_id === projectId)
  );

  getWorkItemLinksByService = computedFn((serviceId: string) =>
    Object.values(this.workItemLinkMap).filter((l) => l.service_id === serviceId)
  );

  getGraphData = computedFn((projectId: string): TServiceGraphData => {
    const serviceIds = this.getFilteredServiceIds(projectId) ?? [];
    const services = serviceIds
      .map((id) => this.serviceMap[id])
      .filter((service): service is IService => Boolean(service));
    const visible = new Set(serviceIds);
    const dependencies = this.getDependenciesByProject(projectId).filter(
      (d) => visible.has(d.from_service_id) && visible.has(d.to_service_id)
    );
    return { services, dependencies };
  });

  fetchServices = async (workspaceSlug: string, workspaceId: string, projectId: string) => {
    try {
      this.loader = true;
      const [services, dependencies, links] = await Promise.all([
        this.serviceService.getServices(workspaceSlug, workspaceId, projectId),
        this.serviceService.getDependencies(workspaceSlug, workspaceId, projectId),
        this.serviceService.getWorkItemLinks(workspaceSlug, workspaceId, projectId),
      ]);
      runInAction(() => {
        services.forEach((s) => set(this.serviceMap, [s.id], { ...this.serviceMap[s.id], ...s }));
        dependencies.forEach((d) => set(this.dependencyMap, [d.id], d));
        links.forEach((l) => set(this.workItemLinkMap, [l.id], l));
        set(this.fetchedMap, projectId, true);
        this.loader = false;
      });
      return services;
    } catch {
      runInAction(() => {
        this.loader = false;
      });
      return undefined;
    }
  };

  createService = async (workspaceSlug: string, workspaceId: string, projectId: string, data: Partial<IService>) => {
    const service = await this.serviceService.createService(workspaceSlug, workspaceId, projectId, data);
    runInAction(() => {
      set(this.serviceMap, [service.id], service);
    });
    return service;
  };

  updateService = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ) => {
    const original = this.getServiceById(serviceId);
    if (!original) throw new Error("Service not found");
    try {
      runInAction(() => {
        set(this.serviceMap, [serviceId], { ...original, ...data });
      });
      const response = await this.serviceService.updateService(workspaceSlug, workspaceId, projectId, serviceId, data);
      runInAction(() => {
        set(this.serviceMap, [serviceId], response);
      });
      return response;
    } catch (error) {
      console.error("Failed to update service in service store", error);
      runInAction(() => {
        set(this.serviceMap, [serviceId], original);
      });
      throw error;
    }
  };

  deleteService = async (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => {
    await this.serviceService.deleteService(workspaceSlug, workspaceId, projectId, serviceId);
    runInAction(() => {
      delete this.serviceMap[serviceId];
      Object.values(this.dependencyMap).forEach((d) => {
        if (d.from_service_id === serviceId || d.to_service_id === serviceId) delete this.dependencyMap[d.id];
      });
      Object.values(this.workItemLinkMap).forEach((l) => {
        if (l.service_id === serviceId) delete this.workItemLinkMap[l.id];
      });
    });
  };

  addDependency = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ) => {
    const dependency = await this.serviceService.createDependency(
      workspaceSlug,
      workspaceId,
      projectId,
      fromServiceId,
      toServiceId
    );
    runInAction(() => {
      set(this.dependencyMap, [dependency.id], dependency);
    });
    return dependency;
  };

  removeDependency = async (workspaceSlug: string, workspaceId: string, projectId: string, dependencyId: string) => {
    await this.serviceService.deleteDependency(workspaceSlug, workspaceId, projectId, dependencyId);
    runInAction(() => {
      delete this.dependencyMap[dependencyId];
    });
  };

  updateNodePosition = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ) => {
    const response = await this.serviceService.updateNodePosition(
      workspaceSlug,
      workspaceId,
      projectId,
      serviceId,
      position
    );
    runInAction(() => {
      set(this.serviceMap, [serviceId], response);
    });
  };

  linkWorkItem = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ) => {
    const link = await this.serviceService.linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, issue);
    runInAction(() => {
      set(this.workItemLinkMap, [link.id], link);
    });
    return link;
  };

  unlinkWorkItem = async (workspaceSlug: string, workspaceId: string, projectId: string, linkId: string) => {
    await this.serviceService.unlinkWorkItem(workspaceSlug, workspaceId, projectId, linkId);
    runInAction(() => {
      delete this.workItemLinkMap[linkId];
    });
  };
}
