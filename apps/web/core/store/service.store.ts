/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set, sortBy } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type {
  ILiteService,
  IService,
  IServiceDependency,
  IServiceHealthSnapshot,
  IServiceProjectLink,
  TServiceDependencyType,
  TServiceGraphData,
  TServiceHealthSummary,
  TServiceWorkItemLink,
} from "@plane/types";
// helpers
import { selectWorkspaceServiceIds, WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";
import { filterServices, orderServices } from "@/services/service.helpers";
// services
import { ServiceHealthService } from "@/services/service-health.service";
import { ServiceService } from "@/services/service.service";
// store
import type { CoreRootStore } from "./root.store";

export type TServicePeek = {
  workspaceSlug: string;
  projectId: string;
  serviceId: string;
};

export interface IServiceStore {
  loader: boolean;
  workspaceFetched: boolean;
  fetchedMap: Record<string, boolean>;
  serviceMap: Record<string, IService>;
  dependencyMap: Record<string, IServiceDependency>;
  workItemLinkMap: Record<string, TServiceWorkItemLink>;
  healthMap: Record<string, IServiceHealthSnapshot>;
  errorMap: Record<string, boolean>;
  workspaceError: boolean;
  peekService?: TServicePeek;
  layoutEpoch: number;
  markLayoutChanged: () => void;
  clearLayoutPositions: () => void;
  getServiceById: (serviceId: string) => IService | null;
  getServiceHealth: (serviceId: string) => IServiceHealthSnapshot | null;
  getProjectHealthSummary: (projectId: string) => TServiceHealthSummary;
  getProjectServiceIds: (projectId: string) => string[] | null;
  getFilteredServiceIds: (projectId: string) => string[] | null;
  getWorkspaceServiceIds: () => string[];
  getWorkspaceScopedServiceIds: (projectIds?: string[]) => string[];
  getWorkspaceFilteredServiceIds: (projectIds?: string[]) => string[];
  getWorkspaceHealthSummary: (projectIds?: string[]) => TServiceHealthSummary;
  getDependenciesByProject: (projectId: string) => IServiceDependency[];
  getDependenciesByService: (serviceId: string) => IServiceDependency[];
  getWorkItemLinksByService: (serviceId: string) => TServiceWorkItemLink[];
  getGraphData: (projectId: string) => TServiceGraphData;
  getWorkspaceGraphData: (projectId?: string) => TServiceGraphData;
  getIsServicePeeked: (serviceId: string) => boolean;
  setPeekService: (peek?: TServicePeek) => void;
  refreshHealth: (workspaceSlug: string, projectId: string | null) => Promise<void>;
  fetchServices: (workspaceSlug: string, workspaceId: string, projectId: string) => Promise<IService[] | undefined>;
  fetchWorkspaceServices: (
    workspaceSlug: string,
    workspaceId: string,
    projectId?: string
  ) => Promise<IService[] | undefined>;
  createService: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ) => Promise<IService>;
  createWorkspaceService: (workspaceSlug: string, workspaceId: string, data: Partial<IService>) => Promise<IService>;
  updateService: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ) => Promise<IService>;
  deleteService: (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => Promise<void>;
  deleteWorkspaceService: (workspaceSlug: string, workspaceId: string, serviceId: string) => Promise<void>;
  addDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    fromServiceId: string,
    toServiceId: string,
    dependencyType?: TServiceDependencyType
  ) => Promise<IServiceDependency>;
  updateDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    dependencyId: string,
    dependencyType: TServiceDependencyType
  ) => Promise<IServiceDependency>;
  removeDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    dependencyId: string
  ) => Promise<void>;
  updateNodePosition: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    serviceId: string,
    position: { x: number; y: number } | null
  ) => Promise<void>;
  addProjectLink: (
    workspaceSlug: string,
    workspaceId: string,
    serviceId: string,
    projectId: string
  ) => Promise<IServiceProjectLink>;
  removeProjectLink: (workspaceSlug: string, workspaceId: string, serviceId: string, linkId: string) => Promise<void>;
  transferOwner: (
    workspaceSlug: string,
    workspaceId: string,
    serviceId: string,
    projectId: string
  ) => Promise<IService>;
  linkWorkItem: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ) => Promise<TServiceWorkItemLink>;
  linkWorkItems: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issues: { id: string; identifier?: string; name?: string }[]
  ) => Promise<TServiceWorkItemLink[]>;
  unlinkWorkItem: (workspaceSlug: string, workspaceId: string, projectId: string, linkId: string) => Promise<void>;
}

export class ServicesStore implements IServiceStore {
  loader: boolean = false;
  workspaceFetched: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  serviceMap: Record<string, IService> = {};
  dependencyMap: Record<string, IServiceDependency> = {};
  workItemLinkMap: Record<string, TServiceWorkItemLink> = {};
  healthMap: Record<string, IServiceHealthSnapshot> = {};
  errorMap: Record<string, boolean> = {};
  workspaceError: boolean = false;
  peekService: TServicePeek | undefined = undefined;
  layoutEpoch: number = 0;
  rootStore;
  serviceService;
  serviceHealthService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      workspaceFetched: observable.ref,
      fetchedMap: observable,
      serviceMap: observable,
      dependencyMap: observable,
      workItemLinkMap: observable,
      healthMap: observable,
      errorMap: observable,
      workspaceError: observable.ref,
      peekService: observable.ref,
      layoutEpoch: observable.ref,
      markLayoutChanged: action,
      clearLayoutPositions: action,
      fetchServices: action,
      fetchWorkspaceServices: action,
      refreshHealth: action,
      setPeekService: action,
      createService: action,
      createWorkspaceService: action,
      updateService: action,
      deleteService: action,
      deleteWorkspaceService: action,
      addDependency: action,
      updateDependency: action,
      removeDependency: action,
      updateNodePosition: action,
      addProjectLink: action,
      removeProjectLink: action,
      transferOwner: action,
      linkWorkItem: action,
      linkWorkItems: action,
      unlinkWorkItem: action,
    });
    this.rootStore = _rootStore;
    this.serviceService = new ServiceService();
    this.serviceHealthService = new ServiceHealthService();
  }

  getServiceById = computedFn((serviceId: string) => this.serviceMap[serviceId] || null);

  getServiceHealth = computedFn((serviceId: string) => this.healthMap[serviceId] || null);

  getProjectHealthSummary = computedFn((projectId: string): TServiceHealthSummary => {
    const summary: TServiceHealthSummary = { down: 0, degraded: 0, healthy: 0, unknown: 0, criticalImpacted: 0 };
    Object.values(this.serviceMap)
      .filter((service) => service.project_id === projectId)
      .forEach((service) => {
        const state = this.healthMap[service.id]?.health ?? "unknown";
        summary[state] += 1;
        if (state !== "healthy" && state !== "unknown" && service.criticality === "critical") {
          summary.criticalImpacted += 1;
        }
      });
    return summary;
  });

  getProjectServiceIds = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    const services = sortBy(
      Object.values(this.serviceMap).filter(
        (s) => s.project_id === projectId || s.project_links?.some((l) => l.project_id === projectId)
      ),
      [(s) => s.sort_order]
    );
    return services.map((s) => s.id);
  });

  getFilteredServiceIds = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    const displayFilters = this.rootStore.serviceFilter.getDisplayFiltersByProjectId(projectId);
    const filters = this.rootStore.serviceFilter.getFiltersByProjectId(projectId);
    const searchQuery = this.rootStore.serviceFilter.searchQuery;
    const services = Object.values(this.serviceMap).filter(
      (s) => s.project_id === projectId || s.project_links?.some((l) => l.project_id === projectId)
    );
    const filtered = filterServices(services, filters, searchQuery, this.healthMap);
    return orderServices(filtered, displayFilters?.order_by, this.healthMap).map((s) => s.id);
  });

  getDependenciesByProject = computedFn((projectId: string) =>
    Object.values(this.dependencyMap).filter((d) => d.project_id === projectId)
  );

  getDependenciesByService = computedFn((serviceId: string) =>
    Object.values(this.dependencyMap).filter((d) => d.from_service_id === serviceId || d.to_service_id === serviceId)
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
    const health: Record<string, IServiceHealthSnapshot> = {};
    services.forEach((service) => {
      const snapshot = this.healthMap[service.id];
      if (snapshot) health[service.id] = snapshot;
    });
    return { services, dependencies, health };
  });

  getIsServicePeeked = computedFn((serviceId: string) => this.peekService?.serviceId === serviceId);

  getWorkspaceServiceIds = computedFn(() => {
    if (!this.workspaceFetched) return [];
    return sortBy(Object.values(this.serviceMap), [(s) => s.sort_order]).map((s) => s.id);
  });

  getWorkspaceScopedServiceIds = computedFn((projectIds?: string[]) => {
    if (!this.workspaceFetched) return [];
    return selectWorkspaceServiceIds({
      services: Object.values(this.serviceMap),
      projectIds,
      filters: {},
      searchQuery: "",
      orderBy: "name",
      healthMap: {},
    });
  });

  getWorkspaceFilteredServiceIds = computedFn((projectIds?: string[]) => {
    if (!this.workspaceFetched) return [];
    const displayFilters = this.rootStore.serviceFilter.getDisplayFiltersByProjectId(WORKSPACE_SERVICE_FILTER_KEY);
    const filters = this.rootStore.serviceFilter.getFiltersByProjectId(WORKSPACE_SERVICE_FILTER_KEY);
    const searchQuery = this.rootStore.serviceFilter.searchQuery;
    return selectWorkspaceServiceIds({
      services: Object.values(this.serviceMap),
      projectIds,
      filters,
      searchQuery,
      orderBy: displayFilters?.order_by ?? "health",
      healthMap: this.healthMap,
    });
  });

  getWorkspaceHealthSummary = computedFn((projectIds?: string[]): TServiceHealthSummary => {
    const summary: TServiceHealthSummary = { down: 0, degraded: 0, healthy: 0, unknown: 0, criticalImpacted: 0 };
    this.getWorkspaceScopedServiceIds(projectIds).forEach((serviceId) => {
      const service = this.serviceMap[serviceId];
      if (!service) return;
      const state = this.healthMap[service.id]?.health ?? "unknown";
      summary[state] += 1;
      if (state !== "healthy" && state !== "unknown" && service.criticality === "critical") {
        summary.criticalImpacted += 1;
      }
    });
    return summary;
  });

  getWorkspaceGraphData = computedFn((projectId?: string): TServiceGraphData => {
    const serviceIds = this.getWorkspaceServiceIds();
    const services = serviceIds
      .map((id) => this.serviceMap[id])
      .filter((service): service is IService => Boolean(service))
      .filter(
        (service) =>
          !projectId ||
          service.owner_project_id === projectId ||
          service.project_links?.some((l) => l.project_id === projectId)
      );
    const visible = new Set(services.map((s) => s.id));
    const dependencies = Object.values(this.dependencyMap).filter(
      (d) => visible.has(d.from_service_id) || visible.has(d.to_service_id)
    );
    const endpoints = new Map<string, IService | ILiteService>();
    for (const dep of dependencies) {
      if (dep.from_service && !visible.has(dep.from_service_id)) endpoints.set(dep.from_service_id, dep.from_service);
      if (dep.to_service && !visible.has(dep.to_service_id)) endpoints.set(dep.to_service_id, dep.to_service);
    }
    const allServices: (IService | ILiteService)[] = [...services, ...endpoints.values()];
    const allIds = new Set(allServices.map((s) => s.id));
    const health: Record<string, IServiceHealthSnapshot> = {};
    services.forEach((service) => {
      const snapshot = this.healthMap[service.id];
      if (snapshot) health[service.id] = snapshot;
    });
    return {
      services: allServices,
      dependencies: dependencies.filter((d) => allIds.has(d.from_service_id) && allIds.has(d.to_service_id)),
      health,
    };
  });

  setPeekService = (peek?: TServicePeek) => {
    this.peekService = peek;
  };

  markLayoutChanged = () => {
    this.layoutEpoch += 1;
  };

  clearLayoutPositions = () => {
    Object.entries(this.serviceMap).forEach(([serviceId, service]) => {
      if (service.position !== null) {
        set(this.serviceMap, [serviceId], { ...service, position: null });
      }
    });
    this.markLayoutChanged();
  };

  fetchServices = async (workspaceSlug: string, workspaceId: string, projectId: string) => {
    try {
      set(this.errorMap, projectId, false);
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
      try {
        const health = await this.serviceHealthService.getHealth(workspaceSlug, projectId);
        runInAction(() => {
          health.forEach((snapshot) => {
            set(this.healthMap, [snapshot.service_id], snapshot);
          });
        });
      } catch (error) {
        // Health is supplementary; the board falls back to "unknown" without it.
        console.error("Failed to fetch service health", error);
      }
      return services;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, projectId, true);
      });
      return undefined;
    }
  };

  fetchWorkspaceServices = async (workspaceSlug: string, workspaceId: string, projectId?: string) => {
    try {
      this.workspaceError = false;
      this.loader = true;
      const [services, dependencies, links] = await Promise.all([
        this.serviceService.getWorkspaceServices(workspaceSlug, projectId ? { project: projectId } : undefined),
        this.serviceService.getWorkspaceDependencies(workspaceSlug),
        this.serviceService.getWorkspaceWorkItemLinks(workspaceSlug),
      ]);
      runInAction(() => {
        services.forEach((s) => set(this.serviceMap, [s.id], s));
        dependencies.forEach((d) => set(this.dependencyMap, [d.id], d));
        links.forEach((l) => set(this.workItemLinkMap, [l.id], l));
        this.workspaceFetched = true;
        this.loader = false;
      });
      try {
        const health = await this.serviceHealthService.getHealth(workspaceSlug, projectId ?? null);
        runInAction(() => {
          health.forEach((snapshot) => {
            set(this.healthMap, [snapshot.service_id], snapshot);
          });
        });
      } catch (error) {
        // Health is supplementary; the board falls back to "unknown" without it.
        console.error("Failed to fetch service health", error);
      }
      return services;
    } catch {
      runInAction(() => {
        this.loader = false;
        this.workspaceError = true;
      });
      return undefined;
    }
  };

  refreshHealth = async (workspaceSlug: string, projectId: string | null) => {
    try {
      const health = await this.serviceHealthService.getHealth(workspaceSlug, projectId);
      runInAction(() => {
        health.forEach((snapshot) => {
          set(this.healthMap, [snapshot.service_id], snapshot);
        });
      });
    } catch (error) {
      console.error("Failed to refresh service health", error);
    }
  };

  createService = async (workspaceSlug: string, workspaceId: string, projectId: string, data: Partial<IService>) => {
    const service = await this.serviceService.createService(workspaceSlug, workspaceId, projectId, data);
    runInAction(() => {
      set(this.serviceMap, [service.id], service);
      this.clearLayoutPositions();
    });
    return service;
  };

  createWorkspaceService = async (workspaceSlug: string, _workspaceId: string, data: Partial<IService>) => {
    const service = await this.serviceService.createWorkspaceService(workspaceSlug, data);
    runInAction(() => {
      set(this.serviceMap, [service.id], service);
      this.clearLayoutPositions();
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
    const kindChanged = data.kind !== undefined && data.kind !== original.kind;
    try {
      runInAction(() => {
        set(this.serviceMap, [serviceId], { ...original, ...data });
      });
      const response = await this.serviceService.updateService(workspaceSlug, workspaceId, projectId, serviceId, data);
      runInAction(() => {
        set(this.serviceMap, [serviceId], response);
        if (kindChanged) this.markLayoutChanged();
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

  private removeLocalService = (serviceId: string) => {
    delete this.serviceMap[serviceId];
    Object.values(this.dependencyMap).forEach((d) => {
      if (d.from_service_id === serviceId || d.to_service_id === serviceId) delete this.dependencyMap[d.id];
    });
    Object.values(this.workItemLinkMap).forEach((l) => {
      if (l.service_id === serviceId) delete this.workItemLinkMap[l.id];
    });
    delete this.healthMap[serviceId];
    if (this.peekService?.serviceId === serviceId) this.peekService = undefined;
  };

  deleteService = async (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => {
    await this.serviceService.deleteService(workspaceSlug, workspaceId, projectId, serviceId);
    runInAction(() => {
      this.removeLocalService(serviceId);
      this.clearLayoutPositions();
    });
  };

  deleteWorkspaceService = async (workspaceSlug: string, _workspaceId: string, serviceId: string) => {
    await this.serviceService.deleteWorkspaceService(workspaceSlug, serviceId);
    runInAction(() => {
      this.removeLocalService(serviceId);
      this.clearLayoutPositions();
    });
  };

  addProjectLink = async (workspaceSlug: string, _workspaceId: string, serviceId: string, projectId: string) => {
    const link = await this.serviceService.addProjectLink(workspaceSlug, serviceId, projectId);
    const service = this.getServiceById(serviceId);
    if (service) {
      runInAction(() => {
        set(this.serviceMap, [serviceId], { ...service, project_links: [...service.project_links, link] });
      });
    }
    return link;
  };

  removeProjectLink = async (workspaceSlug: string, _workspaceId: string, serviceId: string, linkId: string) => {
    await this.serviceService.removeProjectLink(workspaceSlug, serviceId, linkId);
    const service = this.getServiceById(serviceId);
    if (service) {
      runInAction(() => {
        set(this.serviceMap, [serviceId], {
          ...service,
          project_links: service.project_links.filter((l) => l.id !== linkId),
        });
      });
    }
  };

  transferOwner = async (workspaceSlug: string, _workspaceId: string, serviceId: string, projectId: string) => {
    const updated = await this.serviceService.transferServiceOwner(workspaceSlug, serviceId, projectId);
    runInAction(() => {
      set(this.serviceMap, [serviceId], updated);
    });
    return updated;
  };

  addDependency = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    fromServiceId: string,
    toServiceId: string,
    dependencyType: TServiceDependencyType = "depends_on"
  ) => {
    const dependency = await this.serviceService.createDependency(
      workspaceSlug,
      workspaceId,
      projectId,
      fromServiceId,
      toServiceId,
      dependencyType
    );
    runInAction(() => {
      set(this.dependencyMap, [dependency.id], dependency);
      this.clearLayoutPositions();
    });
    return dependency;
  };

  updateDependency = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    dependencyId: string,
    dependencyType: TServiceDependencyType
  ) => {
    const dependency = await this.serviceService.updateDependency(
      workspaceSlug,
      workspaceId,
      projectId,
      dependencyId,
      dependencyType
    );
    runInAction(() => {
      set(this.dependencyMap, [dependency.id], dependency);
    });
    return dependency;
  };

  removeDependency = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    dependencyId: string
  ) => {
    await this.serviceService.deleteDependency(workspaceSlug, workspaceId, projectId, dependencyId);
    runInAction(() => {
      delete this.dependencyMap[dependencyId];
      this.clearLayoutPositions();
    });
  };

  updateNodePosition = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    serviceId: string,
    position: { x: number; y: number } | null
  ) => {
    const original = this.getServiceById(serviceId);
    if (!original) throw new Error("Service not found");
    try {
      runInAction(() => {
        set(this.serviceMap, [serviceId], { ...original, position });
      });
      const response = projectId
        ? await this.serviceService.updateNodePosition(workspaceSlug, workspaceId, projectId, serviceId, position)
        : await this.serviceService.updateWorkspaceService(workspaceSlug, serviceId, { position });
      runInAction(() => {
        set(this.serviceMap, [serviceId], response);
      });
    } catch (error) {
      console.error("Failed to update service position", error);
      runInAction(() => {
        set(this.serviceMap, [serviceId], original);
      });
      throw error;
    }
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

  linkWorkItems = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issues: { id: string; identifier?: string; name?: string }[]
  ) => {
    const links = await this.serviceService.linkWorkItems(workspaceSlug, workspaceId, projectId, serviceId, issues);
    runInAction(() => {
      links.forEach((link) => set(this.workItemLinkMap, [link.id], link));
    });
    return links;
  };

  unlinkWorkItem = async (workspaceSlug: string, workspaceId: string, projectId: string, linkId: string) => {
    await this.serviceService.unlinkWorkItem(workspaceSlug, workspaceId, projectId, linkId);
    runInAction(() => {
      delete this.workItemLinkMap[linkId];
    });
  };
}
