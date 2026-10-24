/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IServiceIncident, TServiceHealth, TServiceIncidentSeverity } from "@plane/types";

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

/** Highest-severity incident in a list (`sev1` is highest; ties break newest first). */
export const getHighestSeverityIncident = (incidents: IServiceIncident[]): IServiceIncident | null =>
  incidents.reduce<IServiceIncident | null>(
    (highest, incident) =>
      !highest ||
      INCIDENT_SEVERITY_RANK[incident.severity] < INCIDENT_SEVERITY_RANK[highest.severity] ||
      (INCIDENT_SEVERITY_RANK[incident.severity] === INCIDENT_SEVERITY_RANK[highest.severity] &&
        new Date(incident.opened_at).getTime() > new Date(highest.opened_at).getTime())
        ? incident
        : highest,
    null
  );

/** Severity first (`sev1`), then most recently opened. */
export const sortIncidentsBySeverity = (incidents: IServiceIncident[]): IServiceIncident[] => {
  const sorted: IServiceIncident[] = [...incidents];
  sorted.sort((a, b) => {
    const rank = INCIDENT_SEVERITY_RANK[a.severity] - INCIDENT_SEVERITY_RANK[b.severity];
    if (rank !== 0) return rank;
    return new Date(b.opened_at).getTime() - new Date(a.opened_at).getTime();
  });
  return sorted;
};

export const getIncidentTooltipWindow = (incidents: IServiceIncident[], limit = 5) => {
  const sorted = sortIncidentsBySeverity(incidents);
  return {
    visible: sorted.slice(0, limit),
    remaining: Math.max(0, sorted.length - limit),
  };
};
