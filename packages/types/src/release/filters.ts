import type { TReleaseStatus } from "./core";

export type TReleaseOrderByOptions = "-created_at" | "target_date" | "name";

export type TReleaseFilters = {
  status?: TReleaseStatus[];
};
