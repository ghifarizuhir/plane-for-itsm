import { describe, expect, it } from "vitest";
import { activeFlowFromMessages, INCIDENT_FLOW_ID, isIncidentCommand, parseIncidentCommand } from "./ai-flows";
import { toAiMessage } from "./ai-conversations";

describe("isIncidentCommand", () => {
  it("matches only leading /incident commands", () => {
    expect(isIncidentCommand("/incident OPS-123")).toBe(true);
    expect(isIncidentCommand("  /incident OPS-123")).toBe(true);
    expect(isIncidentCommand("/INCIDENT off")).toBe(true);
    expect(isIncidentCommand("ask about /incident")).toBe(false);
    expect(isIncidentCommand("/incidents OPS-1")).toBe(false);
  });
});

describe("parseIncidentCommand", () => {
  it("parses a start command with a reference", () => {
    expect(parseIncidentCommand("/incident OPS-123")).toEqual({ kind: "start", ref: "OPS-123" });
    expect(parseIncidentCommand("/incident lts-7 please")).toEqual({ kind: "start", ref: "lts-7" });
  });

  it("parses the off command case-insensitively", () => {
    expect(parseIncidentCommand("/incident off")).toEqual({ kind: "off" });
    expect(parseIncidentCommand("/incident OFF")).toEqual({ kind: "off" });
  });

  it("reports missing and malformed references", () => {
    expect(parseIncidentCommand("/incident")).toEqual({ kind: "missing" });
    expect(parseIncidentCommand("/incident   ")).toEqual({ kind: "missing" });
    expect(parseIncidentCommand("/incident nothing")).toEqual({ kind: "invalid" });
    expect(parseIncidentCommand("/incident OPS-")).toEqual({ kind: "invalid" });
  });

  it("returns null for non-commands", () => {
    expect(parseIncidentCommand("hello")).toBeNull();
    expect(parseIncidentCommand("/schedule daily")).toBeNull();
  });
});

describe("activeFlowFromMessages", () => {
  it("returns the newest non-null flow across messages", () => {
    const messages = [
      { flow: "incident" as const, flowRef: "OPS-1" },
      { flow: "incident" as const, flowRef: "OPS-2" },
    ];
    expect(activeFlowFromMessages(messages)).toEqual({ id: INCIDENT_FLOW_ID, ref: "OPS-2" });
  });

  it("treats an explicit null as ended even with older active messages", () => {
    const messages = [
      { flow: "incident" as const, flowRef: "OPS-1" },
      { flow: null, flowRef: null },
    ];
    expect(activeFlowFromMessages(messages)).toBeUndefined();
  });

  it("returns undefined when no message carries a flow key", () => {
    expect(activeFlowFromMessages([{ content: "hi" }, { flow: undefined }])).toBeUndefined();
  });

  it("ignores an active flow without a reference", () => {
    expect(activeFlowFromMessages([{ flow: "incident", flowRef: null }])).toBeUndefined();
  });
});

describe("toAiMessage flow mapping", () => {
  it("maps flow metadata onto the message model", () => {
    const mapped = toAiMessage({
      id: "m1",
      role: "assistant",
      content: "ok",
      metadata: { flow: "incident", flow_ref: "OPS-9" },
      created_at: "2026-10-15T00:00:00Z",
    });
    expect(mapped.flow).toBe("incident");
    expect(mapped.flowRef).toBe("OPS-9");
  });
});
