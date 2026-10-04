import { describe, expect, it } from "vitest";
import { getIntakeAcceptGate } from "./accept-gate";

describe("getIntakeAcceptGate", () => {
  it("requires a type when the project has types", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: null,
      typeRequiresService: false,
      hasLinkedService: false,
    });
    expect(gate).toEqual({ missingType: true, missingService: false, ready: false });
  });

  it("requires a service when the type needs one", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: "type-1",
      typeRequiresService: true,
      hasLinkedService: false,
    });
    expect(gate).toEqual({ missingType: false, missingService: true, ready: false });
  });

  it("is ready when the required service is linked", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: "type-1",
      typeRequiresService: true,
      hasLinkedService: true,
    });
    expect(gate.ready).toBe(true);
  });

  it("does not require a service for a non-flagged type", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: "type-1",
      typeRequiresService: false,
      hasLinkedService: false,
    });
    expect(gate.ready).toBe(true);
  });

  it("turns the type gate off when the project has no types", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: false,
      issueTypeId: null,
      typeRequiresService: false,
      hasLinkedService: false,
    });
    expect(gate.ready).toBe(true);
  });
});
