import type { TReleaseStatus } from "@plane/types";

export type TReleaseToneConfig = {
  /** i18n key label */
  label_key: string;
  /** pill className */
  pill: string;
  /** left rail / dot className */
  rail: string;
};

export const RELEASE_STATUSES: TReleaseStatus[] = [
  "draft",
  "planned",
  "in_review",
  "approved",
  "released",
  "cancelled",
];

export const RELEASE_STATUS_CONFIG: Record<TReleaseStatus, TReleaseToneConfig> = {
  draft: {
    label_key: "release.status_values.draft",
    pill: "bg-layer-2 text-secondary",
    rail: "bg-layer-3",
  },
  planned: {
    label_key: "release.status_values.planned",
    pill: "bg-layer-2 text-secondary",
    rail: "bg-layer-3",
  },
  in_review: {
    label_key: "release.status_values.in_review",
    pill: "bg-warning-subtle text-warning-primary",
    rail: "bg-warning-primary",
  },
  approved: {
    label_key: "release.status_values.approved",
    pill: "bg-success-subtle text-success-primary",
    rail: "bg-success-primary",
  },
  released: {
    label_key: "release.status_values.released",
    pill: "bg-success-subtle text-success-primary",
    rail: "bg-success-primary",
  },
  cancelled: {
    label_key: "release.status_values.cancelled",
    pill: "bg-danger-subtle text-danger-primary",
    rail: "bg-danger-primary",
  },
};

export const RELEASE_STATUSES_ALLOWING_SCOPE_EDIT: TReleaseStatus[] = ["draft", "planned", "in_review"];

export const getReleaseLink = (workspaceSlug: string, releaseId: string) => `/${workspaceSlug}/releases/${releaseId}`;
