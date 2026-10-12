/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { resolvePlaceholders } from "./resolve-placeholders";

const values = {
  workspace_name: "Acme",
  document_title: "Server down",
  document_id: "WORK-1",
  export_date: "2026-10-12",
  exported_by: "Budi",
};

describe("resolvePlaceholders", () => {
  it("replaces known placeholders", () => {
    expect(resolvePlaceholders("PT {workspace_name} — {document_id}", values)).toBe("PT Acme — WORK-1");
  });

  it("leaves unknown placeholders literal", () => {
    expect(resolvePlaceholders("{org_name} report", values)).toBe("{org_name} report");
  });

  it("handles text without placeholders", () => {
    expect(resolvePlaceholders("Confidential", values)).toBe("Confidential");
  });
});
