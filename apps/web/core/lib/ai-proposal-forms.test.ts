import { describe, expect, it } from "vitest";
import {
  buildServiceWrite,
  buildSprintWrite,
  buildTrackWrite,
  initialDraftForProposal,
  PROPOSAL_FORM_FIELDS,
  validateFormDraft,
  visibleFields,
} from "./ai-proposal-forms";

describe("PROPOSAL_FORM_FIELDS", () => {
  it("defines a required name for create kinds and update sprint/track", () => {
    for (const kind of ["create_service", "create_sprint", "create_track", "update_sprint", "update_track"] as const) {
      expect(PROPOSAL_FORM_FIELDS[kind].some((field) => field.key === "name" && field.required)).toBe(true);
    }
    // update_service has no name field: the service is a resolve target, not an editable field.
    expect(PROPOSAL_FORM_FIELDS.update_service.some((field) => field.key === "name")).toBe(false);
  });
});

describe("visibleFields", () => {
  it("shows every field for create and only changed fields for update", () => {
    expect(visibleFields("create_service", { project: "LTS", name: "Email" })).toHaveLength(
      PROPOSAL_FORM_FIELDS.create_service.length
    );
    const fields = visibleFields("update_service", {
      service: "Email",
      changes: { status: "deprecated" },
    });
    expect(fields.map((field) => field.key)).toEqual(["status"]);
  });
});

describe("initialDraftForProposal", () => {
  it("fills create drafts from the proposal", () => {
    const draft = initialDraftForProposal("create_track", {
      project: "LTS",
      name: "Onboarding",
      members: ["Budi"],
    });
    expect(draft.name).toBe("Onboarding");
    expect(draft.members).toEqual(["Budi"]);
    expect(draft.lead).toBe("");
  });

  it("fills update drafts from changes only", () => {
    const draft = initialDraftForProposal("update_sprint", {
      sprint: "Sprint 4",
      changes: { end_date: "2026-11-21" },
    });
    expect(draft).toEqual({ end_date: "2026-11-21" });
  });
});

describe("validateFormDraft", () => {
  it("validates create-service drafts", () => {
    expect(validateFormDraft("create_service", { name: "Email", status: "active" })).toBeNull();
    expect(validateFormDraft("create_service", { name: "  " })).toBe("Name is required.");
    expect(validateFormDraft("create_service", { name: "Email", repository_url: "nope" })).toBe(
      "Repository URL must start with http:// or https://."
    );
  });

  it("validates sprint drafts", () => {
    expect(validateFormDraft("create_sprint", { name: "Sprint 4", start_date: "2026-11-01" })).toBe(
      "Provide both dates or neither."
    );
    expect(validateFormDraft("update_sprint", { end_date: "" })).toBe(
      "Sprint dates cannot be cleared; pick a new date."
    );
    expect(validateFormDraft("update_service", { status: "" })).toBe("At least one change is required.");
  });

  it("allows clearing track fields", () => {
    expect(validateFormDraft("update_track", { description: "" })).toBeNull();
    expect(validateFormDraft("update_track", { members: [] })).toBeNull();
  });
});

describe("buildServiceWrite", () => {
  it("builds a create payload with html and owner", () => {
    const data = buildServiceWrite("create_service", { name: "Email", description: "Handles mail" }, "u1");
    expect(data.name).toBe("Email");
    expect(data.description).toBe("Handles mail");
    expect(data.description_html).toBe("<p>Handles mail</p>");
    expect(data.owner_id).toBe("u1");
  });

  it("builds an update payload with clears", () => {
    const changes = buildServiceWrite("update_service", { owner: "", repository_url: "" }, null);
    expect(changes.owner_id).toBeNull();
    expect(changes.repository_url).toBeNull();
    expect(changes.name).toBeUndefined();
  });
});

describe("buildSprintWrite", () => {
  it("keeps single update dates and omits blank ones", () => {
    expect(buildSprintWrite("update_sprint", { end_date: "2026-11-21" })).toEqual({ end_date: "2026-11-21" });
    expect(buildSprintWrite("update_sprint", { end_date: "" })).toEqual({});
  });
});

describe("buildTrackWrite", () => {
  it("builds create payloads and update clears", () => {
    expect(buildTrackWrite("create_track", { name: "Onboarding", members: [] }, null, ["u1", "u2"])).toEqual({
      name: "Onboarding",
      member_ids: ["u1", "u2"],
    });
    expect(buildTrackWrite("update_track", { lead: "", start_date: "" }, null, [])).toEqual({
      lead_id: null,
      start_date: null,
    });
  });
});
