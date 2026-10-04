import { describe, expect, it } from "vitest";
import {
  buildWebhookUrl,
  configFromForm,
  serviceRowsFromConfig,
  severityRowsFromConfig,
  validateIntakeSourceForm,
} from "./intake-source.helpers";

describe("intake-source helpers", () => {
  it("builds the webhook url", () => {
    expect(buildWebhookUrl("https://api.example.com/", "plane_is_abc")).toBe(
      "https://api.example.com/api/inbound/alertmanager/plane_is_abc/"
    );
  });

  it("round-trips config rows", () => {
    const config = configFromForm({
      serviceLabelKey: "service",
      serviceRows: [{ id: "a", labelValue: "payment", serviceId: "s1" }],
      fallbackServiceId: "s1",
      severityLabelKey: "severity",
      severityRows: [{ id: "b", labelValue: "critical", priority: "urgent" }],
      defaultPriority: "none",
    });
    expect(serviceRowsFromConfig(config).map(({ labelValue, serviceId }) => ({ labelValue, serviceId }))).toEqual([
      { labelValue: "payment", serviceId: "s1" },
    ]);
    expect(severityRowsFromConfig(config).map(({ labelValue, priority }) => ({ labelValue, priority }))).toEqual([
      { labelValue: "critical", priority: "urgent" },
    ]);
  });

  it("rejects duplicate label values and unknown services", () => {
    const base = {
      name: "Prometheus",
      typeId: "t1",
      autoAccept: false,
      serviceLabelKey: "service",
      serviceRows: [
        { id: "a", labelValue: "payment", serviceId: "s1" },
        { id: "b", labelValue: "payment", serviceId: "s1" },
      ],
      fallbackServiceId: "s1",
      severityLabelKey: "severity",
      severityRows: [{ id: "c", labelValue: "critical", priority: "urgent" as const }],
      defaultPriority: "none" as const,
    };
    expect(validateIntakeSourceForm(base, ["s1", "t1"])).not.toBeNull();
    const unknown = { ...base, serviceRows: [{ id: "a", labelValue: "payment", serviceId: "nope" }] };
    expect(validateIntakeSourceForm(unknown, ["s1", "t1"])).not.toBeNull();
    const valid = { ...base, serviceRows: [{ id: "a", labelValue: "payment", serviceId: "s1" }] };
    expect(validateIntakeSourceForm(valid, ["s1", "t1"])).toBeNull();
  });
});
