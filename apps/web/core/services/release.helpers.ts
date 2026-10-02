import type { IRelease, TReleaseFilters, TReleaseOrderByOptions, TReleaseStatus } from "@plane/types";

const STATUS_ORDER: Record<TReleaseStatus, number> = {
  draft: 0,
  planned: 1,
  in_review: 2,
  approved: 3,
  released: 4,
  cancelled: 5,
};

export const filterReleases = (releases: IRelease[], filters: TReleaseFilters, searchQuery: string): IRelease[] => {
  const query = searchQuery.trim().toLowerCase();
  return releases.filter((release) => {
    if (filters.status && filters.status.length > 0 && !filters.status.includes(release.status)) return false;
    if (!query) return true;
    return release.name.toLowerCase().includes(query) || (release.version ?? "").toLowerCase().includes(query);
  });
};

export const orderReleases = (releases: IRelease[], orderBy: TReleaseOrderByOptions = "-created_at"): IRelease[] => {
  const sorted = [...releases];
  switch (orderBy) {
    case "name":
      // oxlint-disable-next-line unicorn/no-array-sort
      return sorted.sort((a, b) => a.name.localeCompare(b.name));
    case "target_date":
      // oxlint-disable-next-line unicorn/no-array-sort
      return sorted.sort((a, b) => (a.target_date ?? "9999-12-31").localeCompare(b.target_date ?? "9999-12-31"));
    case "-created_at":
    default:
      // oxlint-disable-next-line unicorn/no-array-sort
      return sorted.sort((a, b) => b.created_at.localeCompare(a.created_at));
  }
};

export const releaseStatusOrder = (status: TReleaseStatus): number => STATUS_ORDER[status] ?? 99;
