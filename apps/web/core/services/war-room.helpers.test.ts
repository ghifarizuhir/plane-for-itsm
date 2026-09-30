/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// helpers
import {
  formatElapsed,
  getWarRoomIncidentLink,
  isActiveWarRoomStatus,
  severityFromPriority,
  statusFilterForTab,
} from "./war-room.helpers";

describe("severityFromPriority", () => {
  it("maps every priority to the backend severity default", () => {
    expect(severityFromPriority("urgent")).toBe("sev1");
    expect(severityFromPriority("high")).toBe("sev2");
    expect(severityFromPriority("medium")).toBe("sev3");
    expect(severityFromPriority("low")).toBe("sev4");
    expect(severityFromPriority("none")).toBe("sev4");
  });

  it("returns null for empty or unknown priorities", () => {
    expect(severityFromPriority(null)).toBeNull();
    expect(severityFromPriority(undefined)).toBeNull();
    expect(severityFromPriority("")).toBeNull();
    expect(severityFromPriority("critical")).toBeNull();
  });
});

describe("isActiveWarRoomStatus", () => {
  it("treats active and monitoring as active", () => {
    expect(isActiveWarRoomStatus("active")).toBe(true);
    expect(isActiveWarRoomStatus("monitoring")).toBe(true);
  });

  it("treats resolved and archived as inactive", () => {
    expect(isActiveWarRoomStatus("resolved")).toBe(false);
    expect(isActiveWarRoomStatus("archived")).toBe(false);
  });
});

describe("statusFilterForTab", () => {
  it("maps tabs to the server status csv", () => {
    expect(statusFilterForTab("active")).toBe("active,monitoring");
    expect(statusFilterForTab("resolved")).toBe("resolved");
    expect(statusFilterForTab("all")).toBeUndefined();
  });
});

describe("formatElapsed", () => {
  const startedAt = "2026-01-01T00:00:00.000Z";

  it("formats a running timer with the injected now", () => {
    const now = new Date("2026-01-01T01:02:03.000Z").getTime();
    expect(formatElapsed(startedAt, null, now)).toBe("01:02:03");
  });

  it("uses resolved_at when the room is closed", () => {
    const resolvedAt = "2026-01-01T00:10:00.000Z";
    const now = new Date("2026-01-02T00:00:00.000Z").getTime();
    expect(formatElapsed(startedAt, resolvedAt, now)).toBe("00:10:00");
  });

  it("never returns negative time", () => {
    const now = new Date("2025-12-31T23:59:00.000Z").getTime();
    expect(formatElapsed(startedAt, null, now)).toBe("00:00:00");
  });
});

describe("getWarRoomIncidentLink", () => {
  it("links to the browse work item route", () => {
    expect(getWarRoomIncidentLink("acme", "PROJ-12")).toBe("/acme/browse/PROJ-12/");
  });
});
