/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceDependency, TServiceWorkItemLink } from "@plane/types";
// helpers
import { validateDependency } from "./service.helpers";

export type TServiceStoreData = {
  version: 1;
  services: IService[];
  dependencies: IServiceDependency[];
  links: TServiceWorkItemLink[];
};

export type TStorageLike = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
};

const EMPTY_DATA = (): TServiceStoreData => ({ version: 1, services: [], dependencies: [], links: [] });

export const serviceStorageKey = (workspaceSlug: string, projectId: string) =>
  `plane:services:${workspaceSlug}:${projectId}`;

export const createMemoryStorage = (): TStorageLike => {
  const map = new Map<string, string>();
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => {
      map.set(key, value);
    },
  };
};

export const createDefaultStorage = (): TStorageLike => {
  try {
    if (typeof window !== "undefined" && window.localStorage) return window.localStorage;
  } catch {
    // blocked storage (private mode) falls through to memory
  }
  return createMemoryStorage();
};

const nowIso = () => new Date().toISOString();

const uid = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `id-${Math.random().toString(36).slice(2)}-${Date.now()}`;

const seedServices = (workspaceId: string, projectId: string): IService[] =>
  [
    {
      name: "Payment Gateway",
      status: "active",
      criticality: "critical",
      type: "internal",
      description: "Handles all card payments.",
      repo: "https://example.com/payment",
    },
    {
      name: "Auth Service",
      status: "active",
      criticality: "critical",
      type: "internal",
      description: "Authentication and sessions.",
      repo: "https://example.com/auth",
    },
    {
      name: "Notification Service",
      status: "maintenance",
      criticality: "medium",
      type: "internal",
      description: "Email and push notifications.",
      repo: "https://example.com/notify",
    },
    {
      name: "Postgres Primary",
      status: "active",
      criticality: "critical",
      type: "infrastructure",
      description: "Primary relational database.",
      repo: null,
    },
    {
      name: "Email Provider",
      status: "active",
      criticality: "high",
      type: "third_party",
      description: "External SMTP provider.",
      repo: null,
    },
    {
      name: "Analytics Pipeline",
      status: "planned",
      criticality: "low",
      type: "internal",
      description: "Batch analytics ingestion.",
      repo: "https://example.com/analytics",
    },
  ].map(
    (s, index): IService => ({
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      name: s.name,
      description: s.description,
      description_html: `<p>${s.description}</p>`,
      status: s.status as IService["status"],
      criticality: s.criticality as IService["criticality"],
      type: s.type as IService["type"],
      owner_id: null,
      repository_url: s.repo,
      documentation_url: null,
      position: null,
      sort_order: index * 65535,
      created_at: nowIso(),
      updated_at: nowIso(),
      created_by: null,
      updated_by: null,
    })
  );

export class ServiceMockRepository {
  storage: TStorageLike;

  constructor(storage: TStorageLike = createDefaultStorage()) {
    this.storage = storage;
  }

  private read(key: string): TServiceStoreData {
    const raw = this.storage.getItem(key);
    if (!raw) return EMPTY_DATA();
    try {
      const parsed = JSON.parse(raw) as TServiceStoreData;
      if (!parsed || parsed.version !== 1) return EMPTY_DATA();
      return {
        version: 1,
        services: Array.isArray(parsed.services) ? parsed.services : [],
        dependencies: Array.isArray(parsed.dependencies) ? parsed.dependencies : [],
        links: Array.isArray(parsed.links) ? parsed.links : [],
      };
    } catch {
      return EMPTY_DATA();
    }
  }

  private write(key: string, data: TServiceStoreData): TServiceStoreData {
    this.storage.setItem(key, JSON.stringify(data));
    return data;
  }

  seedIfEmpty(workspaceSlug: string, workspaceId: string, projectId: string): TServiceStoreData {
    const key = serviceStorageKey(workspaceSlug, projectId);
    // Key presence (not array length) decides seeding, so an emptied store stays empty.
    if (this.storage.getItem(key) !== null) return this.read(key);

    const services = seedServices(workspaceId, projectId);
    const byName = (name: string) => services.find((s) => s.name === name)?.id as string;
    const makeDep = (from: string, to: string): IServiceDependency => ({
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      from_service_id: from,
      to_service_id: to,
      created_at: nowIso(),
    });
    const dependencies: IServiceDependency[] = [
      makeDep(byName("Payment Gateway"), byName("Auth Service")),
      makeDep(byName("Payment Gateway"), byName("Postgres Primary")),
      makeDep(byName("Auth Service"), byName("Postgres Primary")),
      makeDep(byName("Notification Service"), byName("Email Provider")),
    ];
    const links: TServiceWorkItemLink[] = [
      {
        id: uid(),
        service_id: byName("Payment Gateway"),
        issue_id: "sample-issue-1",
        project_id: projectId,
        workspace_id: workspaceId,
        issue_identifier: "SAMPLE-1",
        issue_name: "Add 3DS support",
      },
      {
        id: uid(),
        service_id: byName("Auth Service"),
        issue_id: "sample-issue-2",
        project_id: projectId,
        workspace_id: workspaceId,
        issue_identifier: "SAMPLE-2",
        issue_name: "Rotate signing keys",
      },
    ];

    return this.write(key, { version: 1, services, dependencies, links });
  }

  getServices(workspaceSlug: string, workspaceId: string, projectId: string): IService[] {
    return this.seedIfEmpty(workspaceSlug, workspaceId, projectId).services;
  }

  getDependencies(workspaceSlug: string, workspaceId: string, projectId: string): IServiceDependency[] {
    return this.seedIfEmpty(workspaceSlug, workspaceId, projectId).dependencies;
  }

  getLinks(workspaceSlug: string, workspaceId: string, projectId: string): TServiceWorkItemLink[] {
    return this.seedIfEmpty(workspaceSlug, workspaceId, projectId).links;
  }

  createService(workspaceSlug: string, workspaceId: string, projectId: string, data: Partial<IService>): IService {
    const key = serviceStorageKey(workspaceSlug, projectId);
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    const service: IService = {
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      name: data.name ?? "Untitled service",
      description: data.description ?? "",
      description_html: data.description_html ?? "",
      status: data.status ?? "planned",
      criticality: data.criticality ?? "medium",
      type: data.type ?? "internal",
      owner_id: data.owner_id ?? null,
      repository_url: data.repository_url ?? null,
      documentation_url: data.documentation_url ?? null,
      position: null,
      sort_order: Math.max(0, ...stored.services.map((s) => s.sort_order)) + 65535,
      created_at: nowIso(),
      updated_at: nowIso(),
      created_by: data.created_by ?? null,
      updated_by: data.updated_by ?? null,
    };
    this.write(key, { ...stored, services: [...stored.services, service] });
    return service;
  }

  updateService(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ): IService | null {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    const current = stored.services.find((s) => s.id === serviceId);
    if (!current) return null;
    // Never allow identity/timestamp fields to be overwritten.
    const safe: Partial<IService> = { ...data };
    delete safe.id;
    delete safe.workspace_id;
    delete safe.project_id;
    delete safe.created_at;
    const updated: IService = { ...current, ...safe, updated_at: nowIso() };
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      services: stored.services.map((s) => (s.id === serviceId ? updated : s)),
    });
    return updated;
  }

  deleteService(workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string): void {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      services: stored.services.filter((s) => s.id !== serviceId),
      dependencies: stored.dependencies.filter((d) => d.from_service_id !== serviceId && d.to_service_id !== serviceId),
      links: stored.links.filter((l) => l.service_id !== serviceId),
    });
  }

  createDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ): IServiceDependency {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    if (!stored.services.some((s) => s.id === fromServiceId)) throw new Error("Source service not found.");
    if (!stored.services.some((s) => s.id === toServiceId)) throw new Error("Target service not found.");
    const error = validateDependency(stored.dependencies, fromServiceId, toServiceId);
    if (error) throw new Error(error);
    const dependency: IServiceDependency = {
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      from_service_id: fromServiceId,
      to_service_id: toServiceId,
      created_at: nowIso(),
    };
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      dependencies: [...stored.dependencies, dependency],
    });
    return dependency;
  }

  deleteDependency(workspaceSlug: string, workspaceId: string, projectId: string, dependencyId: string): void {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      dependencies: stored.dependencies.filter((d) => d.id !== dependencyId),
    });
  }

  linkWorkItem(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ): TServiceWorkItemLink {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    if (!stored.services.some((s) => s.id === serviceId)) throw new Error("Service not found.");
    const existing = stored.links.find((l) => l.service_id === serviceId && l.issue_id === issue.id);
    if (existing) return existing;
    const link: TServiceWorkItemLink = {
      id: uid(),
      service_id: serviceId,
      issue_id: issue.id,
      project_id: projectId,
      workspace_id: workspaceId,
      issue_identifier: issue.identifier,
      issue_name: issue.name,
    };
    this.write(serviceStorageKey(workspaceSlug, projectId), { ...stored, links: [...stored.links, link] });
    return link;
  }

  unlinkWorkItem(workspaceSlug: string, workspaceId: string, projectId: string, linkId: string): void {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      links: stored.links.filter((l) => l.id !== linkId),
    });
  }
}
