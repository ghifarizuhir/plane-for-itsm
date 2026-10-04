/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TIntakeSourceConfig, TIntakeSourcePriority } from "@plane/types";

export const INTAKE_PRIORITIES: TIntakeSourcePriority[] = ["urgent", "high", "medium", "low", "none"];

export type TServiceRow = { labelValue: string; serviceId: string };
export type TSeverityRow = { labelValue: string; priority: TIntakeSourcePriority };

export type TIntakeSourceForm = {
  name: string;
  typeId: string;
  autoAccept: boolean;
  serviceLabelKey: string;
  serviceRows: TServiceRow[];
  fallbackServiceId: string | null;
  severityLabelKey: string;
  severityRows: TSeverityRow[];
  defaultPriority: TIntakeSourcePriority;
};

export const buildWebhookUrl = (apiBaseUrl: string, token: string): string =>
  `${apiBaseUrl.replace(/\/+$/, "")}/api/inbound/alertmanager/${token}/`;

export const serviceRowsFromConfig = (config?: TIntakeSourceConfig): TServiceRow[] =>
  Object.entries(config?.service_map ?? {}).map(([labelValue, serviceId]) => ({ labelValue, serviceId }));

export const severityRowsFromConfig = (config?: TIntakeSourceConfig): TSeverityRow[] =>
  Object.entries(config?.severity_map ?? {}).map(([labelValue, priority]) => ({
    labelValue,
    priority: priority as TIntakeSourcePriority,
  }));

export const configFromForm = (
  form: Pick<
    TIntakeSourceForm,
    "serviceLabelKey" | "serviceRows" | "fallbackServiceId" | "severityLabelKey" | "severityRows" | "defaultPriority"
  >
): TIntakeSourceConfig => ({
  service_label_key: form.serviceLabelKey.trim() || "service",
  service_map: Object.fromEntries(
    form.serviceRows
      .filter((row) => row.labelValue.trim() && row.serviceId)
      .map((row) => [row.labelValue.trim(), row.serviceId])
  ),
  fallback_service_id: form.fallbackServiceId || null,
  severity_label_key: form.severityLabelKey.trim() || "severity",
  severity_map: Object.fromEntries(
    form.severityRows
      .filter((row) => row.labelValue.trim() && row.priority)
      .map((row) => [row.labelValue.trim(), row.priority])
  ),
  default_priority: form.defaultPriority,
});

export const validateIntakeSourceForm = (form: TIntakeSourceForm, validServiceIds: string[]): string | null => {
  if (!form.name.trim()) return "Name is required";
  if (!form.typeId) return "Select a work item type";
  const labels = new Set<string>();
  for (const row of form.serviceRows) {
    const label = row.labelValue.trim();
    if (!label) return "Service label value is required";
    if (labels.has(label)) return `Duplicate service label value: ${label}`;
    labels.add(label);
    if (!validServiceIds.includes(row.serviceId)) return "Select a service for every mapping row";
  }
  if (form.fallbackServiceId && !validServiceIds.includes(form.fallbackServiceId)) return "Invalid fallback service";
  const severityLabels = new Set<string>();
  for (const row of form.severityRows) {
    const label = row.labelValue.trim();
    if (!label) return "Severity label value is required";
    if (severityLabels.has(label)) return `Duplicate severity label value: ${label}`;
    severityLabels.add(label);
    if (!INTAKE_PRIORITIES.includes(row.priority)) return "Invalid priority";
  }
  if (!INTAKE_PRIORITIES.includes(form.defaultPriority)) return "Invalid priority";
  return null;
};
