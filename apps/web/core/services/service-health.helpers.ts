/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type {
  IService,
  IServiceHealthSnapshot,
  IServiceIncident,
  TServiceCriticality,
  TServiceHealth,
  TServiceIncidentSeverity,
} from "@plane/types";

const FNV_OFFSET_BASIS = 2166136261;
const FNV_PRIME = 16777619;
const DAY_MS = 24 * 60 * 60 * 1000;

/** FNV-1a 32-bit hash — small, dependency-free, stable across runs. */
export const fnv1aHash = (value: string): number => {
  let hash = FNV_OFFSET_BASIS;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, FNV_PRIME);
  }
  return hash >>> 0;
};

/** mulberry32 PRNG — deterministic sequence from a 32-bit seed. */
export const mulberry32 = (seed: number): (() => number) => {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

/** down/degraded probability ceilings, weighted by criticality. */
const HEALTH_THRESHOLDS: Record<TServiceCriticality, { down: number; degraded: number }> = {
  critical: { down: 0.12, degraded: 0.34 },
  high: { down: 0.1, degraded: 0.3 },
  medium: { down: 0.08, degraded: 0.26 },
  low: { down: 0.06, degraded: 0.22 },
};

export const HEALTH_WEIGHT: Record<TServiceHealth, number> = {
  down: 0,
  degraded: 1,
  unknown: 2,
  healthy: 3,
};

export const INCIDENT_SEVERITY_RANK: Record<TServiceIncidentSeverity, number> = {
  sev1: 0,
  sev2: 1,
  sev3: 2,
  sev4: 3,
};

const makeIncident = (
  serviceId: string,
  severity: TServiceIncidentSeverity,
  index: number,
  updatedAt: number,
  random: () => number
): IServiceIncident => ({
  id: `${serviceId}-incident-${index}`,
  service_id: serviceId,
  severity,
  opened_at: new Date(updatedAt - Math.floor(random() * 5 * DAY_MS)).toISOString(),
});

/**
 * Derives a stable health snapshot from a service. The PRNG is seeded from the
 * service id and consumed in a fixed order (health, incidents, deploy offset),
 * so the result is identical across reloads, list order and filter changes.
 */
export const buildHealthSnapshot = (service: IService): IServiceHealthSnapshot => {
  const random = mulberry32(fnv1aHash(service.id));
  const parsedUpdatedAt = new Date(service.updated_at).getTime();
  const updatedAt = Number.isNaN(parsedUpdatedAt) ? Date.now() : parsedUpdatedAt;

  // Lifecycle states with no runtime signal.
  if (service.status === "planned" || service.status === "retired") {
    return { service_id: service.id, health: "unknown", incidents: [], last_deployed_at: null };
  }

  const thresholds = HEALTH_THRESHOLDS[service.criticality] ?? HEALTH_THRESHOLDS.medium;
  const roll = random();
  const health: TServiceHealth = roll < thresholds.down ? "down" : roll < thresholds.degraded ? "degraded" : "healthy";

  const incidents: IServiceIncident[] = [];
  if (health === "down") {
    incidents.push(makeIncident(service.id, "sev1", 0, updatedAt, random));
  } else if (health === "degraded") {
    const count = random() < 0.5 ? 1 : 2;
    for (let index = 0; index < count; index += 1) {
      const severity: TServiceIncidentSeverity = index === 0 && random() < 0.7 ? "sev2" : "sev3";
      incidents.push(makeIncident(service.id, severity, index, updatedAt, random));
    }
  }

  const deployOffsetDays = Math.floor(random() * 15);
  const lastDeployedAt = new Date(updatedAt - deployOffsetDays * DAY_MS).toISOString();

  return { service_id: service.id, health, incidents, last_deployed_at: lastDeployedAt };
};

/** Highest-severity incident in a list (`sev1` is highest). */
export const getHighestSeverityIncident = (incidents: IServiceIncident[]): IServiceIncident | null =>
  incidents.reduce<IServiceIncident | null>(
    (highest, incident) =>
      !highest || INCIDENT_SEVERITY_RANK[incident.severity] < INCIDENT_SEVERITY_RANK[highest.severity]
        ? incident
        : highest,
    null
  );
