import type { IIssueFilterOptions, IIssueFilters, TWorkItemFilterExpression } from "@plane/types";

type TLegacyIssueFilterBag = { filters?: IIssueFilterOptions | null };
type TRichIssueFilterBag = { richFilters?: TWorkItemFilterExpression };

const collectTypeIds = (node: unknown, out: Set<string>): void => {
  if (!node || typeof node !== "object") return;
  const record = node as Record<string, unknown>;
  const andChildren = record.and;
  if (Array.isArray(andChildren)) {
    andChildren.forEach((child) => collectTypeIds(child, out));
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

/** Type tunggal efektif dari filter board (rich atau legacy). */
export const getSingleWorkItemTypeId = (
  issueFilters: TLegacyIssueFilterBag | TRichIssueFilterBag | IIssueFilters | null | undefined
): string | null => {
  if (!issueFilters) return null;
  if ("filters" in issueFilters) {
    const legacyTypeIds = issueFilters.filters?.issue_type;
    return legacyTypeIds?.length === 1 ? (legacyTypeIds[0] ?? null) : null;
  }
  const ids = new Set<string>();
  collectTypeIds((issueFilters as TRichIssueFilterBag).richFilters, ids);
  return ids.size === 1 ? ([...ids][0] ?? null) : null;
};

/** Semua type id yang sedang difilter (rich maupun legacy). */
export const getWorkItemTypeIds = (
  issueFilters: TLegacyIssueFilterBag | TRichIssueFilterBag | IIssueFilters | null | undefined
): string[] => {
  if (!issueFilters) return [];
  const ids = new Set<string>();
  if ("filters" in issueFilters) {
    (issueFilters.filters?.issue_type ?? []).forEach((typeId) => ids.add(typeId));
    return [...ids];
  }
  collectTypeIds((issueFilters as TRichIssueFilterBag).richFilters, ids);
  return [...ids];
};
