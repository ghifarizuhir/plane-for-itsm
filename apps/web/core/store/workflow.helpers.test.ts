import { describe, expect, it } from "vitest";
import type { IIssueFilters, IState, TWorkflowMap, TWorkflowMapType } from "@plane/types";
import {
  allowedTargetStateIds,
  buildTransitionMatrix,
  findWorkflowMapType,
  getSingleWorkItemTypeId,
  getTypeDefaultStateId,
  getWorkItemTypeIds,
  isTypedState,
  MAX_WORKFLOW_MAP_FETCH_RETRIES,
  pruneStateFilterValues,
  resolveEffectiveDisplayFilters,
  resolveSelectableStateIds,
  resolveStateColumns,
  resolveWorkflowStateColumns,
  scopeStateIdsForTypes,
  shouldRetryWorkflowMapFetch,
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

// mirror state bertipe punya `type_id`/`workflow_state_id`; state legacy tidak
const stateById = (stateId: string): IState | undefined => {
  const typed = ["s-new", "s-progress", "s-closed"].includes(stateId);
  return {
    id: stateId,
    type_id: typed ? "type-1" : null,
    workflow_state_id: typed ? `ws-${stateId}` : null,
  } as IState;
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

describe("resolveSelectableStateIds", () => {
  it.each([
    {
      label: "map absen saat edit → stateIds apa adanya, state bertipe tetap tampil",
      map: undefined,
      stateIds: ["legacy", "s-new"],
      currentStateId: "legacy",
      isForWorkItemCreation: false,
      expected: ["legacy", "s-new"],
    },
    {
      label: "create tanpa map → hanya state untyped",
      map: undefined,
      stateIds: ["legacy", "s-new"],
      currentStateId: null,
      isForWorkItemCreation: true,
      expected: ["legacy"],
    },
    {
      label: "create tanpa map dan semua state bertipe → kosong",
      map: undefined,
      stateIds: ["s-new", "s-progress"],
      currentStateId: null,
      isForWorkItemCreation: true,
      expected: [],
    },
    {
      label: "create tanpa map dengan current type terpilih → tidak dipaksa tampil bila current bertipe",
      map: undefined,
      stateIds: ["legacy", "s-new"],
      currentStateId: "s-new",
      isForWorkItemCreation: true,
      expected: ["legacy"],
    },
    {
      label: "create dengan map → irisan state type, urut stateIds",
      map: mapType,
      stateIds: ["legacy", "s-closed", "s-new", "s-progress"],
      currentStateId: null,
      isForWorkItemCreation: true,
      expected: ["s-closed", "s-new", "s-progress"],
    },
    {
      label: "create dengan current legacy yang ada di stateIds tetap terlihat",
      map: mapType,
      stateIds: ["legacy", "s-new"],
      currentStateId: "legacy",
      isForWorkItemCreation: true,
      expected: ["legacy", "s-new"],
    },
    {
      label: "edit dari New → New + In Progress",
      map: mapType,
      stateIds: ["s-new", "s-progress", "s-closed"],
      currentStateId: "s-new",
      isForWorkItemCreation: false,
      expected: ["s-new", "s-progress"],
    },
    {
      label: "edit dari In Progress → In Progress + Closed",
      map: mapType,
      stateIds: ["s-new", "s-progress", "s-closed"],
      currentStateId: "s-progress",
      isForWorkItemCreation: false,
      expected: ["s-progress", "s-closed"],
    },
    {
      label: "edit dari Closed tanpa transisi keluar → state sekarang saja",
      map: mapType,
      stateIds: ["s-new", "s-progress", "s-closed"],
      currentStateId: "s-closed",
      isForWorkItemCreation: false,
      expected: ["s-closed"],
    },
    {
      label: "edit tanpa current state → default saja",
      map: mapType,
      stateIds: ["s-new", "s-progress", "s-closed"],
      currentStateId: null,
      isForWorkItemCreation: false,
      expected: ["s-new"],
    },
    {
      label: "edit dari state legacy di luar workflow → current + default",
      map: mapType,
      stateIds: ["legacy", "s-new", "s-closed"],
      currentStateId: "legacy",
      isForWorkItemCreation: false,
      expected: ["legacy", "s-new"],
    },
    {
      label: "edit current tidak ada di stateIds → default dari stateIds",
      map: mapType,
      stateIds: ["s-new", "s-closed"],
      currentStateId: "legacy",
      isForWorkItemCreation: false,
      expected: ["s-new"],
    },
    {
      label: "stateIds kosong → kosong",
      map: mapType,
      stateIds: [],
      currentStateId: "s-new",
      isForWorkItemCreation: false,
      expected: [],
    },
  ])("$label", ({ map, stateIds, currentStateId, isForWorkItemCreation, expected }) => {
    expect(
      resolveSelectableStateIds(map, { stateIds, currentStateId, getStateById: stateById, isForWorkItemCreation })
    ).toEqual(expected);
  });

  it("type tidak ada di map → stateIds apa adanya", () => {
    const resolvedMapType = findWorkflowMapType({ types: [mapType] }, "type-lain");
    const stateIds = ["legacy", "s-new"];
    expect(
      resolveSelectableStateIds(resolvedMapType, { stateIds, currentStateId: "legacy", getStateById: stateById })
    ).toEqual(stateIds);
  });

  it("tanpa map mengembalikan referensi stateIds yang sama", () => {
    const stateIds = ["s-new"];
    expect(resolveSelectableStateIds(undefined, { stateIds })).toBe(stateIds);
  });

  it("create tanpa resolver state mengembalikan stateIds apa adanya", () => {
    const stateIds = ["legacy", "s-new"];
    expect(resolveSelectableStateIds(undefined, { stateIds, isForWorkItemCreation: true })).toBe(stateIds);
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

describe("getTypeDefaultStateId", () => {
  it("mengembalikan default_state_id saat type ditemukan", () => {
    expect(getTypeDefaultStateId({ types: [mapType] }, "type-1")).toBe("s-new");
  });

  it("null saat type ditemukan tapi default_state_id null", () => {
    const noDefault: TWorkflowMapType = { ...mapType, default_state_id: null };
    expect(getTypeDefaultStateId({ types: [noDefault] }, "type-1")).toBeNull();
  });

  it("null saat type tidak ada di map", () => {
    expect(getTypeDefaultStateId({ types: [mapType] }, "type-lain")).toBeNull();
    expect(getTypeDefaultStateId({ types: [mapType] }, null)).toBeNull();
    expect(getTypeDefaultStateId({ types: [mapType] }, undefined)).toBeNull();
  });

  it("null saat map absen", () => {
    expect(getTypeDefaultStateId(undefined, "type-1")).toBeNull();
  });
});

describe("getSingleWorkItemTypeId", () => {
  it("membaca satu type dari richFilters bentuk type_id__in", () => {
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__in: "type-1" }] } })).toBe("type-1");
  });

  it("membaca type_id__exact dan nilai comma tunggal", () => {
    expect(getSingleWorkItemTypeId({ richFilters: { type_id__exact: "type-2" } })).toBe("type-2");
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__in: "type-3," }] } })).toBe("type-3");
  });

  it("null saat tidak ada / lebih dari satu type", () => {
    expect(getSingleWorkItemTypeId({ richFilters: {} })).toBeNull();
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__in: "t-1,t-2" }] } })).toBeNull();
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ priority__in: "urgent" }] } })).toBeNull();
  });

  it("tetap membaca bentuk legacy filters.issue_type", () => {
    expect(getSingleWorkItemTypeId({ filters: { issue_type: ["type-1"] } })).toBe("type-1");
    expect(getSingleWorkItemTypeId({ filters: { issue_type: ["t-1", "t-2"] } })).toBeNull();
    expect(getSingleWorkItemTypeId({ filters: { issue_type: [] } })).toBeNull();
    expect(getSingleWorkItemTypeId({ filters: { issue_type: null } })).toBeNull();
    expect(getSingleWorkItemTypeId({ filters: {} })).toBeNull();
  });

  it("null untuk input kosong atau board tanpa filter type", () => {
    expect(getSingleWorkItemTypeId(undefined)).toBeNull();
    expect(getSingleWorkItemTypeId(null)).toBeNull();
    const currentBoardFilters: IIssueFilters = {
      richFilters: {},
      displayFilters: { group_by: "state" },
      displayProperties: {},
      kanbanFilters: { group_by: [], sub_group_by: [] },
    };
    expect(getSingleWorkItemTypeId(currentBoardFilters)).toBeNull();
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

describe("shouldRetryWorkflowMapFetch", () => {
  it("mengizinkan retry selama di bawah budget, lalu berhenti", () => {
    expect(shouldRetryWorkflowMapFetch(0)).toBe(true);
    expect(shouldRetryWorkflowMapFetch(1)).toBe(true);
    expect(shouldRetryWorkflowMapFetch(MAX_WORKFLOW_MAP_FETCH_RETRIES)).toBe(false);
    expect(shouldRetryWorkflowMapFetch(MAX_WORKFLOW_MAP_FETCH_RETRIES + 1)).toBe(false);
  });
});

const typedMapType = (typeId: string, typeName: string, stateId: string, name: string): TWorkflowMapType => ({
  type_id: typeId,
  type_name: typeName,
  workflow_id: `wf-${typeId}`,
  default_state_id: stateId,
  states: [{ id: stateId, name, color: "#000000", group: "backlog", sequence: 1, is_default: true }],
  transitions: [],
});

const compositeWorkflowMap: TWorkflowMap = {
  types: [
    {
      ...typedMapType("type-p", "Problem", "p-1", "Baru"),
      states: [
        { id: "p-1", name: "Baru", color: "#111111", group: "backlog", sequence: 1, is_default: true },
        { id: "p-2", name: "Investigasi", color: "#222222", group: "started", sequence: 2, is_default: false },
      ],
    },
    {
      ...typedMapType("type-c", "Change", "c-1", "Baru"),
      states: [{ id: "c-1", name: "Baru", color: "#333333", group: "backlog", sequence: 1, is_default: true }],
    },
  ],
};

const compositeProjectStates: IState[] = [
  {
    id: "l-1",
    name: "Baru",
    color: "#000000",
    group: "backlog",
    description: "",
    sequence: 5,
    workspace_id: "w",
    project_id: "p",
  },
  {
    id: "p-1",
    name: "Baru",
    color: "#111111",
    group: "backlog",
    description: "",
    sequence: 1,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-p",
    workflow_state_id: "ws-p-1",
  },
  {
    id: "p-2",
    name: "Investigasi",
    color: "#222222",
    group: "started",
    description: "",
    sequence: 2,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-p",
    workflow_state_id: "ws-p-2",
  },
  {
    id: "c-1",
    name: "Baru",
    color: "#333333",
    group: "backlog",
    description: "",
    sequence: 1,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-c",
    workflow_state_id: "ws-c-1",
  },
  {
    id: "z-1",
    name: "Ghost",
    color: "#444444",
    group: "started",
    description: "",
    sequence: 9,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-z",
    workflow_state_id: "ws-z-1",
  },
] as IState[];

describe("resolveEffectiveDisplayFilters", () => {
  it("project tanpa typed workflow tidak berubah", () => {
    const filters = { group_by: "state" as const, sub_group_by: null };
    expect(resolveEffectiveDisplayFilters(filters, undefined, null)).toEqual(filters);
    expect(resolveEffectiveDisplayFilters(filters, { types: [] }, "type-p")).toEqual(filters);
  });

  it("mixed typed workflow: state → state_detail.group", () => {
    const result = resolveEffectiveDisplayFilters(
      { group_by: "state", sub_group_by: null },
      compositeWorkflowMap,
      null
    );
    expect(result?.group_by).toBe("state_detail.group");
  });

  it("satu type: state → workflow_state", () => {
    const result = resolveEffectiveDisplayFilters(
      { group_by: "state", sub_group_by: null },
      compositeWorkflowMap,
      "type-p"
    );
    expect(result?.group_by).toBe("workflow_state");
  });

  it("nilai workflow_state dan state_detail.group eksplisit dipertahankan", () => {
    expect(
      resolveEffectiveDisplayFilters({ group_by: "workflow_state", sub_group_by: null }, compositeWorkflowMap, null)
        ?.group_by
    ).toBe("workflow_state");
    expect(
      resolveEffectiveDisplayFilters({ group_by: "state_detail.group", sub_group_by: null }, compositeWorkflowMap, null)
        ?.group_by
    ).toBe("state_detail.group");
  });

  it("menghapus sub_group_by yang menjadi sama dengan group_by efektif", () => {
    const result = resolveEffectiveDisplayFilters(
      { group_by: "state", sub_group_by: "state_detail.group" },
      compositeWorkflowMap,
      null
    );
    expect(result?.group_by).toBe("state_detail.group");
    expect(result?.sub_group_by).toBeNull();
  });

  it("undefined tetap undefined", () => {
    expect(resolveEffectiveDisplayFilters(undefined, compositeWorkflowMap, null)).toBeUndefined();
  });
});

describe("resolveWorkflowStateColumns", () => {
  it("single type memakai kolom type itu dengan label polos", () => {
    const columns = resolveWorkflowStateColumns(
      compositeProjectStates,
      compositeWorkflowMap.types[0],
      compositeWorkflowMap
    );
    expect(columns.map((column) => [column.state.id, column.label])).toEqual([
      ["p-1", "Baru"],
      ["p-2", "Investigasi"],
    ]);
  });

  it("mixed: legacy dulu, lalu per type urut sequence, label komposit", () => {
    const columns = resolveWorkflowStateColumns(compositeProjectStates, undefined, compositeWorkflowMap);
    expect(columns.map((column) => [column.state.id, column.label])).toEqual([
      ["l-1", "Baru"],
      ["p-1", "Problem · Baru"],
      ["p-2", "Problem · Investigasi"],
      ["c-1", "Change · Baru"],
      ["z-1", "Ghost"],
    ]);
  });

  it("tanpa typed workflow mengembalikan label polos apa adanya", () => {
    const columns = resolveWorkflowStateColumns(compositeProjectStates, undefined, { types: [] });
    expect(columns.map((column) => column.label)).toEqual(["Baru", "Baru", "Investigasi", "Baru", "Ghost"]);
  });
});

describe("getWorkItemTypeIds", () => {
  it("mengumpulkan semua type id dari richFilters", () => {
    expect(new Set(getWorkItemTypeIds({ richFilters: { and: [{ type_id__in: "t-1,t-2" }] } }))).toEqual(
      new Set(["t-1", "t-2"])
    );
  });

  it("membaca bentuk legacy filters.issue_type dan input kosong", () => {
    expect(getWorkItemTypeIds({ filters: { issue_type: ["t-9"] } })).toEqual(["t-9"]);
    expect(getWorkItemTypeIds(undefined)).toEqual([]);
    expect(getWorkItemTypeIds({ richFilters: {} })).toEqual([]);
  });
});

const compositeStateTypeById = (stateId: string) =>
  ({
    "l-1": { type_id: null },
    "p-1": { type_id: "type-p" },
    "c-1": { type_id: "type-c" },
  })[stateId];

describe("scopeStateIdsForTypes", () => {
  it("tanpa selected type mengembalikan apa adanya", () => {
    expect(scopeStateIdsForTypes(["l-1", "p-1"], [], compositeStateTypeById)).toEqual(["l-1", "p-1"]);
  });

  it("menyaring ke state milik type terpilih dan membuang legacy", () => {
    expect(scopeStateIdsForTypes(["l-1", "p-1", "c-1"], ["type-p"], compositeStateTypeById)).toEqual(["p-1"]);
  });

  it("undefined tetap undefined", () => {
    expect(scopeStateIdsForTypes(undefined, ["type-p"], compositeStateTypeById)).toBeUndefined();
  });
});

describe("pruneStateFilterValues", () => {
  it("membuang state yang tidak diizinkan dari state_id__in", () => {
    expect(
      pruneStateFilterValues({ and: [{ state_id__in: "s-1,s-2" }, { priority__in: "urgent" }] }, new Set(["s-2"]))
    ).toEqual({ and: [{ state_id__in: "s-2" }, { priority__in: "urgent" }] });
  });

  it("menghapus condition state yang kosong dan menormalkan grup kosong", () => {
    expect(pruneStateFilterValues({ and: [{ state_id__in: "s-1" }] }, new Set(["s-9"]))).toEqual({});
  });

  it("menangani state_id__exact dan field lain yang tidak disentuh", () => {
    expect(
      pruneStateFilterValues({ and: [{ state_id__exact: "s-1" }, { state_group__in: "backlog" }] }, new Set([]))
    ).toEqual({ and: [{ state_group__in: "backlog" }] });
  });

  it("undefined tetap undefined", () => {
    expect(pruneStateFilterValues(undefined, new Set(["s-1"]))).toBeUndefined();
  });
});
