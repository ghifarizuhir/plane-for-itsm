/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { IServiceIncident } from "@plane/types";
import { getHighestSeverityIncident, getIncidentTooltipWindow } from "./service-health.helpers";

const incident = (
  id: string,
  severity: IServiceIncident["severity"],
  openedAt: string,
  overrides: Partial<IServiceIncident> = {}
): IServiceIncident => ({
  id,
  service_id: "service",
  severity,
  opened_at: openedAt,
  ...overrides,
});

describe("service health helpers", () => {
  it("sorts by severity, then newest first", () => {
    const incidents = [
      incident("sev4", "sev4", "2026-10-24T01:00:00Z"),
      incident("sev1-old", "sev1", "2026-10-24T01:00:00Z"),
      incident("sev1-new", "sev1", "2026-10-24T02:00:00Z"),
    ];

    expect(getHighestSeverityIncident(incidents)?.id).toBe("sev1-new");
    expect(getIncidentTooltipWindow(incidents).visible.map((item) => item.id)).toEqual([
      "sev1-new",
      "sev1-old",
      "sev4",
    ]);
  });

  it("caps the tooltip at five incidents and reports the remainder", () => {
    const incidents = Array.from({ length: 7 }, (_, index) =>
      incident(`incident-${index}`, "sev3", "2026-10-24T01:00:00Z")
    );

    const window = getIncidentTooltipWindow(incidents);
    expect(window.visible).toHaveLength(5);
    expect(window.remaining).toBe(2);
  });
});
