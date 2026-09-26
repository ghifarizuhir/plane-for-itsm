import { describe, expect, it } from "vitest";
import type { IState, TWorkflowMapType } from "@plane/types";
import {
  allowedTargetStateIds,
  buildTransitionMatrix,
  findWorkflowMapType,
  isTypedState,
  resolveStateColumns,
} from "./workflow.helpers";

const mapType: TWorkflowMapType = {
  type_id: "type-1",
  type_name: "Incident",
  workflow_id: "wf-1",
  default_state_id: "s-new",
  states: [
    { id: "s-new", name: "New", color: "#60646C", group: "backlog", sequence: 1, is_default: true },
    { id: "s-progress", name: "In Progress", color: "#F59E0B", group: "started", sequence: 2, is_default: false },
    { id: "s-closed", name: "Closed", color: "#46A758", group: "completed", sequence: 3, is_default: false },
  ],
  transitions: [
    { from_state_id: "s-new", to_state_id: "s-progress" },
    { from_state_id: "s-progress", to_state_id: "s-closed" },
  ],
};

describe("allowedTargetStateIds", () => {
  it("mengikuti transisi dari state sekarang", () => {
    expect(allowedTargetStateIds(mapType, "s-new")).toEqual(["s-progress"]);
    expect(allowedTargetStateIds(mapType, "s-progress")).toEqual(["s-closed"]);
    expect(allowedTargetStateIds(mapType, "s-closed")).toEqual([]);
  });

  it("state di luar workflow hanya boleh pindah ke default", () => {
    expect(allowedTargetStateIds(mapType, "legacy-state")).toEqual(["s-new"]);
  });

  it("state sekarang null memakai default", () => {
    expect(allowedTargetStateIds(mapType, null)).toEqual(["s-new"]);
  });

  it("tanpa map mengembalikan kosong", () => {
    expect(allowedTargetStateIds(undefined, "s-new")).toEqual([]);
  });

  it("tanpa default state mengembalikan kosong untuk state tak dikenal", () => {
    const noDefault: TWorkflowMapType = { ...mapType, default_state_id: null };
    expect(allowedTargetStateIds(noDefault, "legacy-state")).toEqual([]);
    expect(allowedTargetStateIds(noDefault, null)).toEqual([]);
  });
});

describe("resolveStateColumns", () => {
  it("memakai urutan + metadata mirror saat map ada", () => {
    const projectStates: IState[] = [
      {
        id: "s-closed",
        name: "Closed",
        color: "#000000",
        group: "completed",
        description: "",
        sequence: 30,
        workspace_id: "w",
        project_id: "p",
      } as IState,
      {
        id: "s-new",
        name: "New",
        color: "#000000",
        group: "backlog",
        description: "",
        sequence: 10,
        workspace_id: "w",
        project_id: "p",
      } as IState,
    ];
    const shuffledMapType: TWorkflowMapType = {
      ...mapType,
      states: [mapType.states[2], mapType.states[1], mapType.states[0]],
    };
    const columns = resolveStateColumns(projectStates, shuffledMapType);
    expect(columns.map((c) => c.id)).toEqual(["s-new", "s-closed"]);
    expect(columns[0].name).toBe("New");
    expect(columns[0].group).toBe("backlog");
    expect(columns[0].color).toBe("#60646C");
    expect(columns[1].name).toBe("Closed");
    expect(columns[1].group).toBe("completed");
    expect(columns[1].color).toBe("#46A758");
  });

  it("tanpa map mengembalikan state apa adanya", () => {
    const projectStates = [{ id: "s-1" } as IState];
    expect(resolveStateColumns(projectStates, undefined)).toBe(projectStates);
  });
});

describe("buildTransitionMatrix", () => {
  it("membuat semua pasangan kecuali self dan menandai yang ada", () => {
    const matrix = buildTransitionMatrix([{ id: "a" }, { id: "b" }], [{ from_state_id: "a", to_state_id: "b" }]);
    expect(matrix).toEqual([
      { from_state_id: "a", to_state_id: "b", exists: true },
      { from_state_id: "b", to_state_id: "a", exists: false },
    ]);
  });

  it("transisi ke state tak dikenal tidak menandai pasangan apa pun", () => {
    const matrix = buildTransitionMatrix(
      [{ id: "a" }, { id: "b" }],
      [
        { from_state_id: "a", to_state_id: "b" },
        { from_state_id: "a", to_state_id: "ghost" },
      ]
    );
    expect(matrix).toEqual([
      { from_state_id: "a", to_state_id: "b", exists: true },
      { from_state_id: "b", to_state_id: "a", exists: false },
    ]);
  });
});

describe("findWorkflowMapType", () => {
  it("menemukan type dari map", () => {
    expect(findWorkflowMapType({ types: [mapType] }, "type-1")?.type_name).toBe("Incident");
    expect(findWorkflowMapType({ types: [mapType] }, null)).toBeUndefined();
  });
});

describe("isTypedState", () => {
  it("true bila type_id terisi", () => {
    expect(isTypedState({ type_id: "type-1", workflow_state_id: null })).toBe(true);
  });

  it("true bila hanya workflow_state_id terisi", () => {
    expect(isTypedState({ type_id: null, workflow_state_id: "ws-1" })).toBe(true);
  });

  it("false untuk state legacy", () => {
    expect(isTypedState({ type_id: null, workflow_state_id: null })).toBe(false);
  });

  it("false bila field mapping tidak ada", () => {
    expect(isTypedState({})).toBe(false);
  });
});
