import type { IIssueFilterOptions, IIssueFilters, IState, TWorkflowMap, TWorkflowMapType } from "@plane/types";

export const findWorkflowMapType = (
  map: TWorkflowMap | undefined,
  typeId: string | null | undefined
): TWorkflowMapType | undefined => (typeId ? map?.types.find((type) => type.type_id === typeId) : undefined);

type TLegacyIssueFilterBag = { filters?: IIssueFilterOptions | null };

/**
 * Type tunggal aktif dari bentuk **legacy** `filters.issue_type`. Revamp rich
 * filters menghapus filter type dari board, jadi tidak ada jalur web yang
 * mengisi bentuk ini saat ini: semua pemanggil board mengirim `IIssueFilters`
 * (tanpa field `filters`) dan helper selalu mengembalikan `null`. Follow-up
 * type filter harus mengarahkan pembacaan ini ke `richFilters`.
 *
 * Parameter menerima bentuk legacy maupun `IIssueFilters` supaya call site
 * tidak perlu cast; probe `"filters" in ...` yang menentukan bentuknya.
 */
export const getSingleWorkItemTypeId = (
  issueFilters: TLegacyIssueFilterBag | IIssueFilters | null | undefined
): string | null => {
  if (!issueFilters || !("filters" in issueFilters)) return null;
  const typeIds = issueFilters.filters?.issue_type;
  return typeIds?.length === 1 ? (typeIds[0] ?? null) : null;
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
