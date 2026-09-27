import type {
  IIssueFilterOptions,
  IIssueFilters,
  IState,
  TWorkItemFilterExpressionData,
  TWorkflowMap,
  TWorkflowMapType,
} from "@plane/types";

export const findWorkflowMapType = (
  map: TWorkflowMap | undefined,
  typeId: string | null | undefined
): TWorkflowMapType | undefined => (typeId ? map?.types.find((type) => type.type_id === typeId) : undefined);

/** State default satu type dari map, atau `null` bila type/default tidak ada. */
export const getTypeDefaultStateId = (
  map: TWorkflowMap | undefined,
  typeId: string | null | undefined
): string | null => findWorkflowMapType(map, typeId)?.default_state_id ?? null;

type TLegacyIssueFilterBag = { filters?: IIssueFilterOptions | null };

/** Kumpulkan semua nilai type dari kondisi `type_id*` (rekursif pada grup `and`). */
const collectTypeIds = (node: TWorkItemFilterExpressionData | undefined, out: Set<string>): void => {
  if (!node) return;
  const record = node as Record<string, unknown>;
  const andChildren = record.and;
  if (Array.isArray(andChildren)) {
    andChildren.forEach((child) => collectTypeIds(child as TWorkItemFilterExpressionData, out));
    return;
  }
  for (const key of ["type_id", "type_id__exact", "type_id__in"] as const) {
    const raw = record[key];
    if (raw === undefined || raw === null) continue;
    const value = Array.isArray(raw) ? raw.join(",") : String(raw);
    value
      .split(",")
      .map((part) => part.trim())
      .filter((part) => part.length > 0)
      .forEach((part) => out.add(part));
  }
};

/**
 * Type tunggal efektif dari filter board: utamanya `richFilters`
 * (`type_id__in` / `type_id__exact`, boleh di dalam grup `and`), fallback ke
 * bentuk legacy `filters.issue_type`. Dipakai `getStateColumns` untuk memilih
 * kolom state milik type tersebut.
 *
 * Parameter menerima bentuk legacy maupun `IIssueFilters` supaya call site
 * tidak perlu cast; probe `"filters" in ...` yang menentukan bentuknya.
 */
export const getSingleWorkItemTypeId = (
  issueFilters: TLegacyIssueFilterBag | IIssueFilters | null | undefined
): string | null => {
  if (!issueFilters) return null;
  if ("filters" in issueFilters) {
    const legacyTypeIds = issueFilters.filters?.issue_type;
    return legacyTypeIds?.length === 1 ? (legacyTypeIds[0] ?? null) : null;
  }
  const ids = new Set<string>();
  collectTypeIds(issueFilters.richFilters, ids);
  return ids.size === 1 ? ([...ids][0] ?? null) : null;
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

type TSelectableStateIdsOptions = {
  stateIds: string[];
  currentStateId?: string | null;
  /** Dipakai saat create tanpa type untuk menyaring state mirror bertipe. */
  getStateById?: (stateId: string) => IState | undefined;
  isForWorkItemCreation?: boolean;
};

/**
 * State ids yang boleh dipilih di dropdown untuk `mapType`.
 *
 * - Tanpa `mapType` saat edit (type legacy / map belum termuat) → `stateIds` apa adanya.
 * - Tanpa `mapType` saat create (belum ada type terpilih) → hanya state legacy/untyped;
 *   state mirror bertipe akan ditolak backend bila dikirim tanpa `type_id` (`validate_create_refs`).
 * - Create dengan `mapType` → irisan state type dengan `stateIds`, tanpa filter
 *   transisi (state sekarang belum ada; urutan mengikuti `stateIds`).
 * - Edit dengan `mapType` → hanya state sekarang + tujuan transisi valid; state sekarang selalu tampil.
 */
export const resolveSelectableStateIds = (
  mapType: TWorkflowMapType | undefined,
  { stateIds, currentStateId, getStateById, isForWorkItemCreation }: TSelectableStateIdsOptions
): string[] => {
  if (!mapType) {
    if (isForWorkItemCreation && getStateById)
      return stateIds.filter((stateId) => !isTypedState(getStateById(stateId) ?? {}));
    return stateIds;
  }
  const allowedStateIds = isForWorkItemCreation
    ? mapType.states.map((state) => state.id)
    : allowedTargetStateIds(mapType, currentStateId);
  return stateIds.filter((stateId) => stateId === currentStateId || allowedStateIds.includes(stateId));
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
