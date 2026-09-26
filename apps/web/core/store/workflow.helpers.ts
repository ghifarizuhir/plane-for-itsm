import type { IIssueFilterOptions, IState, TIssueGroupByOptions, TWorkflowMap, TWorkflowMapType } from "@plane/types";

export const findWorkflowMapType = (
  map: TWorkflowMap | undefined,
  typeId: string | null | undefined
): TWorkflowMapType | undefined => (typeId ? map?.types.find((type) => type.type_id === typeId) : undefined);

/**
 * Type tunggal aktif dari filter store. Filter board saat ini memakai rich
 * filters (tanpa key type), jadi pembacaan legacy `filters.issue_type` tetap
 * dipertahankan supaya kolom hybrid menyala begitu filter type tersedia.
 */
export const getSingleWorkItemTypeId = (issueFilters: unknown): string | null => {
  const filters = (issueFilters as { filters?: IIssueFilterOptions | null } | null | undefined)?.filters;
  const typeIds = filters?.issue_type;
  return typeIds?.length === 1 ? (typeIds[0] ?? null) : null;
};

/**
 * Default group_by board project bertipe. Campuran type memakai kolom group
 * (5 kolom) supaya state tiap type tidak menumpuk. `"state"`/null adalah
 * default lama yang boleh ditimpa; pilihan user lain dipertahankan.
 */
export const resolveTypedWorkflowGroupBy = (
  groupBy: TIssueGroupByOptions | undefined,
  workflowMap: TWorkflowMap | undefined,
  singleTypeId?: string | null
): TIssueGroupByOptions => {
  if (singleTypeId) return groupBy ?? null;
  const hasTypedWorkflow = (workflowMap?.types.length ?? 0) > 0;
  if (!hasTypedWorkflow) return groupBy ?? null;
  const isDefaultGroupBy = groupBy === undefined || groupBy === null || groupBy === "state";
  return isDefaultGroupBy ? "state_detail.group" : groupBy;
};

/** State tujuan yang diizinkan dari `currentStateId` (mirror ids). */
export const allowedTargetStateIds = (
  mapType: TWorkflowMapType | undefined,
  currentStateId: string | null | undefined
): string[] => {
  if (!mapType) return [];
  const defaultIds = mapType.default_state_id ? [mapType.default_state_id] : [];
  if (!currentStateId) return defaultIds;
  const current = mapType.states.find((state) => state.id === currentStateId);
  if (!current) return defaultIds;
  const allowed = new Set(
    mapType.transitions.filter((transition) => transition.from_state_id === currentStateId).map((t) => t.to_state_id)
  );
  return mapType.states.filter((state) => allowed.has(state.id)).map((state) => state.id);
};

/** State mirror typed (workflow-owned) bila punya `type_id` atau `workflow_state_id`. */
export const isTypedState = (state: Partial<Pick<IState, "type_id" | "workflow_state_id">>): boolean =>
  Boolean(state.type_id || state.workflow_state_id);

/** Kolom kanban untuk satu type: urut `sequence` mirror, metadata mirror. */
export const resolveStateColumns = (projectStates: IState[], mapType: TWorkflowMapType | undefined): IState[] => {
  if (!mapType) return projectStates;
  return (
    [...mapType.states]
      // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread already copies the array
      .sort((a, b) => a.sequence - b.sequence)
      .map((mirror) => {
        const state = projectStates.find((candidate) => candidate.id === mirror.id);
        return state
          ? Object.assign({}, state, { name: mirror.name, color: mirror.color, group: mirror.group })
          : undefined;
      })
      .filter((state): state is IState => Boolean(state))
  );
};

/** Pasangan from → to (tanpa self) + status ada/tidak, untuk matriks transisi. */
export const buildTransitionMatrix = (
  states: { id: string }[],
  transitions: { from_state_id: string; to_state_id: string }[]
): { from_state_id: string; to_state_id: string; exists: boolean }[] => {
  const existing = new Set(transitions.map((transition) => `${transition.from_state_id}:${transition.to_state_id}`));
  return states.flatMap((from) =>
    states
      .filter((to) => to.id !== from.id)
      .map((to) => ({
        from_state_id: from.id,
        to_state_id: to.id,
        exists: existing.has(`${from.id}:${to.id}`),
      }))
  );
};
