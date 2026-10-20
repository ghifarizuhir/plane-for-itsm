# RCB/TCB Web — Phase 3: Foundations + Releases — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisite:** Phase 1 & 2 backend (`2026-10-02-rcb-tcb-backend-releases.md`, `2026-10-02-rcb-tcb-backend-review-engine.md`) sudah dieksekusi — endpoint `/api/workspaces/:slug/releases/` dan `/review-requests/` live.

**Goal:** Halaman **Releases** workspace-level yang berfungsi end-to-end (list, create/edit/delete, link change lintas project dengan badge TCB, riwayat review, submit/withdraw ke RCB) plus seluruh fondasi bersama (types, constants, API services, stores, hooks) yang dipakai board RCB/TCB di Phase 4.

**Architecture:** Mengikuti pola Services/War Rooms: `packages/types` domain barrel, class service berbasis `APIService` dengan normalisasi error, MobX store + hook di root store, komponen di `apps/web/core/components/releases`, halaman workspace-level di `(projects)` layout dengan entri di `app/routes/core.ts`, nav sidebar workspace via `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS`. Semua string UI lewat `@plane/i18n` (namespace baru `release` + `review`).

**Tech Stack:** React Router 8, React 19, MobX 6, axios `APIService`, react-hook-form, Tailwind 4, `@plane/ui`, `@makeplane/propel`, vitest.

**Spec:** `docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md`

**Conventions (verified 2026-10-02):**

- Service: `class X extends APIService { constructor() { super(API_BASE_URL); } }`; tiap method `.then((response) => response?.data).catch((error) => { throw toXError(error); })` (`core/services/service.service.ts`).
- Store: `makeObservable` + `computedFn` + `runInAction` + lodash `set`; `fetchedMap` keyed per scope (`core/store/service.store.ts`); registrasi di `root.store.ts` (class field, constructor, `resetOnSignOut`).
- Hook: `useContext(StoreContext)` + throw bila undefined (`core/hooks/store/use-service.ts`).
- Vitest: `apps/web/core/services/*.helpers.test.ts` (`import { describe, expect, it } from "vitest"`).
- Halaman workspace-level: `(projects)` layout; contoh `stickies/` & `drafts/`; registrasi `layout(path, [route(...)])` di `app/routes/core.ts`.
- Nav workspace: `packages/constants/src/workspace.ts` (`WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS` + `..._LINKS`), ikon via `getSidebarNavigationItemIcon` (`core/components/workspace/sidebar/helper.tsx`).
- Permission UI: `useUserPermissions().allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)`.
- Ikon tersedia & sudah dipakai: `RocketOutline`, `ClipboardOutline`, `AlertOctagonOutline`, `ArchiveOutline`, `ClockOutline`.
- i18n: English source-of-truth di `packages/i18n/src/locales/en/<namespace>.json`, namespace didaftarkan di `packages/i18n/src/constants/namespaces.ts`, regenerate `pnpm --filter @plane/i18n run generate:types`, lalu **wajib ikuti skill `translate`** untuk 19 locale lain + `sync:check`.

---

### Task 1: Types — release + review

**Files:**

- Create: `packages/types/src/release/core.ts`
- Create: `packages/types/src/release/filters.ts`
- Create: `packages/types/src/release/index.ts`
- Create: `packages/types/src/review/core.ts`
- Create: `packages/types/src/review/index.ts`
- Modify: `packages/types/src/index.ts`

- [ ] **Step 1: Create release types**

`packages/types/src/release/core.ts`:

```ts
import type { IReviewRequestSummary } from "../review";

export type TReleaseStatus = "draft" | "planned" | "in_review" | "approved" | "released" | "cancelled";

export interface IRelease {
  id: string;
  workspace_id: string;
  sequence_id: number;
  name: string;
  version: string | null;
  description_html: string;
  status: TReleaseStatus;
  target_date: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  updated_by: string | null;
}

export interface IReleaseChange {
  id: string;
  issue_id: string;
  project_id: string;
  issue_name: string | null;
  issue_identifier: string | null;
  project_identifier: string | null;
}

export interface IReleaseDetail extends IRelease {
  changes: IReleaseChange[];
  review_requests: IReviewRequestSummary[];
}

export type TReleaseCreatePayload = {
  name: string;
  version?: string | null;
  description_html?: string;
  status?: TReleaseStatus;
  target_date?: string | null;
};

export type TReleaseUpdatePayload = Partial<TReleaseCreatePayload>;
```

`packages/types/src/release/filters.ts`:

```ts
import type { TReleaseStatus } from "./core";

export type TReleaseOrderByOptions = "-created_at" | "target_date" | "name";

export type TReleaseFilters = {
  status?: TReleaseStatus[];
};
```

`packages/types/src/release/index.ts`:

```ts
export * from "./core";
export * from "./filters";
```

- [ ] **Step 2: Create review types**

`packages/types/src/review/core.ts`:

```ts
export type TReviewBoardType = "tcb" | "rcb";
export type TReviewRequestStatus = "pending" | "scheduled" | "decided" | "withdrawn";
export type TReviewSessionStatus = "scheduled" | "completed" | "cancelled";
export type TReviewOutcome = "approved" | "rejected" | "approved_with_notes" | "deferred";
export type TReviewParticipantRole = "chair" | "secretary" | "member";
export type TReviewAttendance = "invited" | "present" | "absent";
export type TReviewSubjectKind = "change" | "release";

export interface IReviewSubjectIssue {
  id: string | null;
  identifier: string | null;
  name: string | null;
  project_id: string | null;
  project_identifier: string | null;
}

export interface IReviewSubjectRelease {
  id: string | null;
  sequence_id: number | null;
  name: string | null;
  version: string | null;
  status: string | null;
}

export interface IReviewSubject {
  kind: TReviewSubjectKind;
  id: string | null;
  issue: IReviewSubjectIssue | null;
  release: IReviewSubjectRelease | null;
}

export interface IReviewSessionRef {
  id: string;
  title: string | null;
  scheduled_at: string | null;
}

export interface IReviewRequestSummary {
  id: string;
  board_type: TReviewBoardType;
  status: TReviewRequestStatus;
  submission_note: string;
  submitted_by: string | null;
  submitted_at: string;
}

export interface IReviewRequest extends IReviewRequestSummary {
  workspace_id: string;
  change_issue_id: string | null;
  release_id: string | null;
  project_id: string | null;
  created_at: string;
  updated_at: string;
  subject: IReviewSubject;
  session: IReviewSessionRef | null;
}

export interface IReviewHistoryEntry {
  session_id: string;
  title: string;
  scheduled_at: string;
  session_status: TReviewSessionStatus;
  outcome: TReviewOutcome | null;
  outcome_note: string;
  decided_at: string | null;
}

export interface IReviewRequestDetail extends IReviewRequest {
  history: IReviewHistoryEntry[];
}

export interface IReviewSessionCounts {
  items: number;
  participants: number;
  pending_outcome: number;
}

export interface IReviewSession {
  id: string;
  workspace_id: string;
  board_type: TReviewBoardType;
  project_id: string | null;
  title: string;
  scheduled_at: string;
  status: TReviewSessionStatus;
  minutes: string;
  location: string | null;
  completed_at: string | null;
  cancelled_at: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  counts: IReviewSessionCounts;
}

export interface IReviewItemSubject {
  kind: TReviewSubjectKind;
  identifier: string | null;
  name: string | null;
  version: string | null;
}

export interface IReviewSessionItem {
  id: string;
  session_id: string;
  review_request_id: string;
  position: number;
  outcome: TReviewOutcome | null;
  outcome_note: string;
  decided_by: string | null;
  decided_at: string | null;
  created_at: string;
  request_status: TReviewRequestStatus;
  submission_note: string;
  subject: IReviewItemSubject;
}

export interface IReviewParticipant {
  id: string;
  user_id: string;
  role: TReviewParticipantRole;
  attendance: TReviewAttendance;
  created_at: string;
  display_name: string;
  email: string | null;
}

export interface IReviewSessionDetail extends IReviewSession {
  items: IReviewSessionItem[];
  participants: IReviewParticipant[];
}

export type TReviewRequestCreatePayload = {
  board_type: TReviewBoardType;
  change_issue_id?: string | null;
  release_id?: string | null;
  submission_note?: string;
};

export type TReviewRequestListParams = {
  board_type: TReviewBoardType;
  project_id?: string;
  status?: TReviewRequestStatus;
  change_issue_id?: string;
  release_id?: string;
};

export type TReviewSessionCreatePayload = {
  board_type: TReviewBoardType;
  project_id?: string | null;
  title: string;
  scheduled_at: string;
  location?: string | null;
  minutes?: string;
};

export type TReviewSessionUpdatePayload = {
  title?: string;
  scheduled_at?: string;
  location?: string | null;
  minutes?: string;
};

export type TReviewSessionListParams = {
  board_type: TReviewBoardType;
  project_id?: string;
  status?: TReviewSessionStatus;
};

export type TReviewItemUpdatePayload = {
  outcome: TReviewOutcome;
  outcome_note?: string;
};

export type TReviewParticipantCreatePayload = {
  user_id: string;
  role?: TReviewParticipantRole;
  attendance?: TReviewAttendance;
};

export type TReviewParticipantUpdatePayload = {
  role?: TReviewParticipantRole;
  attendance?: TReviewAttendance;
};
```

`packages/types/src/review/index.ts`:

```ts
export * from "./core";
```

- [ ] **Step 3: Export from the types root**

In `packages/types/src/index.ts`, add after `export * from "./project";`:

```ts
export * from "./release";
export * from "./review";
```

- [ ] **Step 4: Verify types compile**

Run:

```bash
pnpm --filter=@plane/types check:types 2>/dev/null || pnpm check:types
```

Expected: PASS (tidak ada error `IReviewRequestSummary`/`TReleaseStatus`).

- [ ] **Step 5: Commit**

```bash
git add packages/types/src/release packages/types/src/review packages/types/src/index.ts
git commit -m "feat(types): release + review domain types"
```

---

### Task 2: Constants — release + review config

**Files:**

- Create: `packages/constants/src/release.ts`
- Create: `packages/constants/src/review.ts`
- Modify: `packages/constants/src/index.ts`

- [ ] **Step 1: Create release constants**

`packages/constants/src/release.ts`:

```ts
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
```

- [ ] **Step 2: Create review constants**

`packages/constants/src/review.ts`:

```ts
import type {
  TReviewAttendance,
  TReviewBoardType,
  TReviewOutcome,
  TReviewParticipantRole,
  TReviewRequestStatus,
  TReviewSessionStatus,
} from "@plane/types";

export type TReviewToneConfig = {
  label_key: string;
  pill: string;
};

export const REVIEW_BOARD_TYPES: TReviewBoardType[] = ["tcb", "rcb"];

export const REVIEW_BOARD_LABEL_KEYS: Record<TReviewBoardType, string> = {
  tcb: "review.board_values.tcb",
  rcb: "review.board_values.rcb",
};

export const REVIEW_REQUEST_STATUSES: TReviewRequestStatus[] = ["pending", "scheduled", "decided", "withdrawn"];

export const REVIEW_REQUEST_STATUS_CONFIG: Record<TReviewRequestStatus, TReviewToneConfig> = {
  pending: { label_key: "review.request_status_values.pending", pill: "bg-layer-2 text-secondary" },
  scheduled: { label_key: "review.request_status_values.scheduled", pill: "bg-warning-subtle text-warning-primary" },
  decided: { label_key: "review.request_status_values.decided", pill: "bg-success-subtle text-success-primary" },
  withdrawn: { label_key: "review.request_status_values.withdrawn", pill: "bg-layer-2 text-tertiary" },
};

export const REVIEW_SESSION_STATUSES: TReviewSessionStatus[] = ["scheduled", "completed", "cancelled"];

export const REVIEW_SESSION_STATUS_CONFIG: Record<TReviewSessionStatus, TReviewToneConfig> = {
  scheduled: { label_key: "review.session_status_values.scheduled", pill: "bg-warning-subtle text-warning-primary" },
  completed: { label_key: "review.session_status_values.completed", pill: "bg-success-subtle text-success-primary" },
  cancelled: { label_key: "review.session_status_values.cancelled", pill: "bg-layer-2 text-tertiary" },
};

export const REVIEW_OUTCOMES: TReviewOutcome[] = ["approved", "rejected", "approved_with_notes", "deferred"];

export const REVIEW_OUTCOME_CONFIG: Record<TReviewOutcome, TReviewToneConfig> = {
  approved: { label_key: "review.outcome_values.approved", pill: "bg-success-subtle text-success-primary" },
  rejected: { label_key: "review.outcome_values.rejected", pill: "bg-danger-subtle text-danger-primary" },
  approved_with_notes: {
    label_key: "review.outcome_values.approved_with_notes",
    pill: "bg-warning-subtle text-warning-primary",
  },
  deferred: { label_key: "review.outcome_values.deferred", pill: "bg-layer-2 text-secondary" },
};

export const REVIEW_PARTICIPANT_ROLES: TReviewParticipantRole[] = ["chair", "secretary", "member"];

export const REVIEW_PARTICIPANT_ROLE_LABEL_KEYS: Record<TReviewParticipantRole, string> = {
  chair: "review.participant_role_values.chair",
  secretary: "review.participant_role_values.secretary",
  member: "review.participant_role_values.member",
};

export const REVIEW_ATTENDANCE_VALUES: TReviewAttendance[] = ["invited", "present", "absent"];

export const REVIEW_ATTENDANCE_LABEL_KEYS: Record<TReviewAttendance, string> = {
  invited: "review.attendance_values.invited",
  present: "review.attendance_values.present",
  absent: "review.attendance_values.absent",
};

export const getReleaseControlLink = (workspaceSlug: string) => `/${workspaceSlug}/release-control`;

export const getTestingControlLink = (workspaceSlug: string, projectId: string) =>
  `/${workspaceSlug}/projects/${projectId}/testing-control`;

export const getReviewSessionLink = (
  workspaceSlug: string,
  sessionId: string,
  board: TReviewBoardType,
  projectId?: string
) =>
  board === "rcb"
    ? `${getReleaseControlLink(workspaceSlug)}/sessions/${sessionId}`
    : `${getTestingControlLink(workspaceSlug, projectId ?? "")}/sessions/${sessionId}`;
```

- [ ] **Step 3: Register constants exports**

In `packages/constants/src/index.ts`, add (alphabetical with existing `export * from "./..."` lines):

```ts
export * from "./release";
export * from "./review";
```

- [ ] **Step 4: Verify types compile**

Run:

```bash
pnpm --filter=web check:types
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/constants/src/release.ts packages/constants/src/review.ts packages/constants/src/index.ts
git commit -m "feat(constants): release + review config"
```

---

### Task 3: API services — ReleaseService + ReviewService

**Files:**

- Create: `apps/web/core/services/release.service.ts`
- Create: `apps/web/core/services/review.service.ts`

- [ ] **Step 1: Create `release.service.ts`**

`apps/web/core/services/release.service.ts`:

```ts
import { API_BASE_URL } from "@plane/constants";
import type {
  IRelease,
  IReleaseChange,
  IReleaseDetail,
  TReleaseCreatePayload,
  TReleaseUpdatePayload,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TReleaseErrorBody = { detail?: string; error?: string; [key: string]: unknown };

const toReleaseError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TReleaseErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  return normalized;
};

export class ReleaseService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private basePath(workspaceSlug: string): string {
    return `/api/workspaces/${workspaceSlug}/releases`;
  }

  async getReleases(
    workspaceSlug: string,
    params?: { status?: string; target_date_from?: string; target_date_to?: string }
  ): Promise<IRelease[]> {
    return this.get(`${this.basePath(workspaceSlug)}/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async getRelease(workspaceSlug: string, releaseId: string): Promise<IReleaseDetail> {
    return this.get(`${this.basePath(workspaceSlug)}/${releaseId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async createRelease(workspaceSlug: string, data: TReleaseCreatePayload): Promise<IRelease> {
    return this.post(`${this.basePath(workspaceSlug)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async updateRelease(workspaceSlug: string, releaseId: string, data: TReleaseUpdatePayload): Promise<IRelease> {
    return this.patch(`${this.basePath(workspaceSlug)}/${releaseId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async deleteRelease(workspaceSlug: string, releaseId: string): Promise<void> {
    await this.delete(`${this.basePath(workspaceSlug)}/${releaseId}/`).catch((error) => {
      throw toReleaseError(error);
    });
  }

  async getReleaseChanges(workspaceSlug: string, releaseId: string): Promise<IReleaseChange[]> {
    return this.get(`${this.basePath(workspaceSlug)}/${releaseId}/changes/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async linkReleaseChanges(workspaceSlug: string, releaseId: string, issueIds: string[]): Promise<IReleaseChange[]> {
    return this.post(`${this.basePath(workspaceSlug)}/${releaseId}/changes/`, { issue_ids: issueIds })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async unlinkReleaseChange(workspaceSlug: string, releaseId: string, issueId: string): Promise<void> {
    await this.delete(`${this.basePath(workspaceSlug)}/${releaseId}/changes/${issueId}/`).catch((error) => {
      throw toReleaseError(error);
    });
  }
}
```

- [ ] **Step 2: Create `review.service.ts`**

`apps/web/core/services/review.service.ts`:

```ts
import { API_BASE_URL } from "@plane/constants";
import type {
  IReviewParticipant,
  IReviewRequest,
  IReviewRequestDetail,
  IReviewSession,
  IReviewSessionDetail,
  IReviewSessionItem,
  TReviewItemUpdatePayload,
  TReviewParticipantCreatePayload,
  TReviewParticipantUpdatePayload,
  TReviewRequestCreatePayload,
  TReviewRequestListParams,
  TReviewSessionCreatePayload,
  TReviewSessionListParams,
  TReviewSessionUpdatePayload,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TReviewErrorBody = { detail?: string; error?: string; [key: string]: unknown };

const toReviewError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TReviewErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  return normalized;
};

export class ReviewService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private workspacePath(workspaceSlug: string): string {
    return `/api/workspaces/${workspaceSlug}`;
  }

  async getReviewRequests(workspaceSlug: string, params: TReviewRequestListParams): Promise<IReviewRequest[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-requests/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async submitReviewRequest(workspaceSlug: string, data: TReviewRequestCreatePayload): Promise<IReviewRequest> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-requests/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getReviewRequest(workspaceSlug: string, requestId: string): Promise<IReviewRequestDetail> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-requests/${requestId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async withdrawReviewRequest(workspaceSlug: string, requestId: string): Promise<IReviewRequest> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-requests/${requestId}/withdraw/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getReviewSessions(workspaceSlug: string, params: TReviewSessionListParams): Promise<IReviewSession[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async createReviewSession(workspaceSlug: string, data: TReviewSessionCreatePayload): Promise<IReviewSession> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getReviewSession(workspaceSlug: string, sessionId: string): Promise<IReviewSessionDetail> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async updateReviewSession(
    workspaceSlug: string,
    sessionId: string,
    data: TReviewSessionUpdatePayload
  ): Promise<IReviewSession> {
    return this.patch(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async completeReviewSession(workspaceSlug: string, sessionId: string): Promise<IReviewSession> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/complete/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async cancelReviewSession(workspaceSlug: string, sessionId: string): Promise<IReviewSession> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/cancel/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getSessionItems(workspaceSlug: string, sessionId: string): Promise<IReviewSessionItem[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async addSessionItems(workspaceSlug: string, sessionId: string, requestIds: string[]): Promise<IReviewSessionItem[]> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/`, {
      request_ids: requestIds,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async updateSessionItem(
    workspaceSlug: string,
    sessionId: string,
    itemId: string,
    data: TReviewItemUpdatePayload
  ): Promise<IReviewSessionItem> {
    return this.patch(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/${itemId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async removeSessionItem(workspaceSlug: string, sessionId: string, itemId: string): Promise<void> {
    await this.delete(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/${itemId}/`).catch(
      (error) => {
        throw toReviewError(error);
      }
    );
  }

  async getSessionParticipants(workspaceSlug: string, sessionId: string): Promise<IReviewParticipant[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async addSessionParticipant(
    workspaceSlug: string,
    sessionId: string,
    data: TReviewParticipantCreatePayload
  ): Promise<IReviewParticipant> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async updateSessionParticipant(
    workspaceSlug: string,
    sessionId: string,
    participantId: string,
    data: TReviewParticipantUpdatePayload
  ): Promise<IReviewParticipant> {
    return this.patch(
      `${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/${participantId}/`,
      data
    )
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async removeSessionParticipant(workspaceSlug: string, sessionId: string, participantId: string): Promise<void> {
    await this.delete(
      `${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/${participantId}/`
    ).catch((error) => {
      throw toReviewError(error);
    });
  }
}
```

- [ ] **Step 3: Typecheck**

Run:

```bash
pnpm --filter=web check:types
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/services/release.service.ts apps/web/core/services/review.service.ts
git commit -m "feat(web): release + review api services"
```

---

### Task 4: Web helpers + unit tests

**Files:**

- Create: `apps/web/core/services/release.helpers.ts`
- Create: `apps/web/core/services/review.helpers.ts`
- Test: `apps/web/core/services/release.helpers.test.ts`
- Test: `apps/web/core/services/review.helpers.test.ts`

- [ ] **Step 1: Write the failing tests**

`apps/web/core/services/release.helpers.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { IRelease } from "@plane/types";
// local imports
import { filterReleases, orderReleases, releaseStatusOrder } from "./release.helpers";

const base = (overrides: Partial<IRelease>): IRelease => ({
  id: "r1",
  workspace_id: "w1",
  sequence_id: 1,
  name: "Rilis",
  version: null,
  description_html: "",
  status: "draft",
  target_date: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  created_by: null,
  updated_by: null,
  ...overrides,
});

describe("filterReleases", () => {
  const releases = [
    base({ id: "a", name: "Rilis Januari", status: "planned", version: "v1.0" }),
    base({ id: "b", name: "Rilis Februari", status: "released", version: "v2.0" }),
  ];

  it("returns all releases when no filters are applied", () => {
    expect(filterReleases(releases, {}, "")).toHaveLength(2);
  });

  it("filters by status", () => {
    const result = filterReleases(releases, { status: ["planned"] }, "");
    expect(result.map((release) => release.id)).toEqual(["a"]);
  });

  it("searches name and version case-insensitively", () => {
    expect(filterReleases(releases, {}, "februari").map((release) => release.id)).toEqual(["b"]);
    expect(filterReleases(releases, {}, "V1.0").map((release) => release.id)).toEqual(["a"]);
  });
});

describe("orderReleases", () => {
  it("orders by created_at descending by default", () => {
    const older = base({ id: "old", created_at: "2026-01-01T00:00:00Z" });
    const newer = base({ id: "new", created_at: "2026-02-01T00:00:00Z" });
    expect(orderReleases([older, newer]).map((release) => release.id)).toEqual(["new", "old"]);
  });

  it("orders by target date with nulls last", () => {
    const noDate = base({ id: "none", target_date: null });
    const later = base({ id: "later", target_date: "2026-12-01" });
    const sooner = base({ id: "sooner", target_date: "2026-10-01" });
    expect(orderReleases([noDate, later, sooner], "target_date").map((release) => release.id)).toEqual([
      "sooner",
      "later",
      "none",
    ]);
  });
});

describe("releaseStatusOrder", () => {
  it("ranks lifecycle statuses", () => {
    expect(releaseStatusOrder("draft")).toBeLessThan(releaseStatusOrder("approved"));
    expect(releaseStatusOrder("approved")).toBeLessThan(releaseStatusOrder("cancelled"));
  });
});
```

`apps/web/core/services/review.helpers.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { IReviewSession } from "@plane/types";
// local imports
import {
  activeRequestForSubject,
  latestRequestForSubject,
  reviewRequestsKey,
  reviewSessionsKey,
  sortSessionsByScheduledAt,
} from "./review.helpers";

describe("reviewRequestsKey", () => {
  it("is stable for equivalent params and distinguishes board/scope", () => {
    const base = { board_type: "tcb" as const, project_id: "p1" };
    expect(reviewRequestsKey(base)).toBe(reviewRequestsKey({ ...base }));
    expect(reviewRequestsKey(base)).not.toBe(reviewRequestsKey({ board_type: "rcb" }));
  });
});

describe("reviewSessionsKey", () => {
  it("distinguishes board and project scope", () => {
    expect(reviewSessionsKey({ board_type: "tcb", project_id: "p1" })).not.toBe(
      reviewSessionsKey({ board_type: "rcb" })
    );
  });
});

describe("latestRequestForSubject", () => {
  it("returns the most recent request", () => {
    const requests = [
      { submitted_at: "2026-01-01T00:00:00Z", status: "withdrawn" },
      { submitted_at: "2026-02-01T00:00:00Z", status: "pending" },
    ];
    expect(latestRequestForSubject(requests)?.status).toBe("pending");
  });

  it("returns null for an empty list", () => {
    expect(latestRequestForSubject([])).toBeNull();
  });
});

describe("activeRequestForSubject", () => {
  it("only considers pending and scheduled requests", () => {
    const requests = [
      { submitted_at: "2026-02-01T00:00:00Z", status: "decided" },
      { submitted_at: "2026-01-01T00:00:00Z", status: "scheduled" },
    ];
    expect(activeRequestForSubject(requests)?.status).toBe("scheduled");
  });
});

describe("sortSessionsByScheduledAt", () => {
  it("orders newest first", () => {
    const sessions = [
      { id: "old", scheduled_at: "2026-01-01T00:00:00Z" },
      { id: "new", scheduled_at: "2026-03-01T00:00:00Z" },
    ] as IReviewSession[];
    expect(sortSessionsByScheduledAt(sessions).map((session) => session.id)).toEqual(["new", "old"]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
pnpm --filter=web test -- release.helpers review.helpers
```

Expected: FAIL — `release.helpers` / `review.helpers` not found (atau `reviewSessionsKey` belum ada).

- [ ] **Step 3: Create the helpers**

`apps/web/core/services/release.helpers.ts`:

```ts
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
      return sorted.sort((a, b) => a.name.localeCompare(b.name));
    case "target_date":
      return sorted.sort((a, b) => (a.target_date ?? "9999-12-31").localeCompare(b.target_date ?? "9999-12-31"));
    case "-created_at":
    default:
      return sorted.sort((a, b) => b.created_at.localeCompare(a.created_at));
  }
};

export const releaseStatusOrder = (status: TReleaseStatus): number => STATUS_ORDER[status] ?? 99;
```

`apps/web/core/services/review.helpers.ts`:

```ts
import type { IReviewSession, TReviewRequestListParams, TReviewSessionListParams } from "@plane/types";

export const reviewRequestsKey = (params: TReviewRequestListParams): string =>
  [
    params.board_type,
    params.project_id ?? "ws",
    params.status ?? "all",
    params.change_issue_id ?? "-",
    params.release_id ?? "-",
  ].join(":");

export const reviewSessionsKey = (params: TReviewSessionListParams): string =>
  [params.board_type, params.project_id ?? "ws", params.status ?? "all"].join(":");

export const latestRequestForSubject = <T extends { submitted_at: string }>(requests: T[]): T | null => {
  if (requests.length === 0) return null;
  return [...requests].sort((a, b) => b.submitted_at.localeCompare(a.submitted_at))[0];
};

export const activeRequestForSubject = <T extends { status: string; submitted_at: string }>(requests: T[]): T | null =>
  latestRequestForSubject(requests.filter((request) => request.status === "pending" || request.status === "scheduled"));

export const sortSessionsByScheduledAt = (sessions: IReviewSession[]): IReviewSession[] =>
  [...sessions].sort((a, b) => b.scheduled_at.localeCompare(a.scheduled_at));
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
pnpm --filter=web test -- release.helpers review.helpers
```

Expected: PASS (13 test).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/release.helpers.ts apps/web/core/services/review.helpers.ts \
  apps/web/core/services/release.helpers.test.ts apps/web/core/services/review.helpers.test.ts
git commit -m "feat(web): release + review helpers"
```

---

### Task 5: Stores + hooks + root registration

**Files:**

- Create: `apps/web/core/store/release.store.ts`
- Create: `apps/web/core/store/review.store.ts`
- Create: `apps/web/core/hooks/store/use-release.ts`
- Create: `apps/web/core/hooks/store/use-review.ts`
- Modify: `apps/web/core/store/root.store.ts`

- [ ] **Step 1: Create `release.store.ts`**

`apps/web/core/store/release.store.ts`:

```ts
import { set } from "lodash-es";
import { action, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
import type {
  IRelease,
  IReleaseChange,
  IReleaseDetail,
  TReleaseCreatePayload,
  TReleaseUpdatePayload,
} from "@plane/types";
// services
import { ReleaseService } from "@/services/release.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IReleaseStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  releaseMap: Record<string, IRelease>;
  releaseIdsMap: Record<string, string[]>;
  detailMap: Record<string, IReleaseDetail>;
  errorMap: Record<string, boolean>;
  getReleaseById: (releaseId: string) => IRelease | null;
  getWorkspaceReleaseIds: (workspaceSlug: string) => string[] | null;
  getReleaseDetailById: (releaseId: string) => IReleaseDetail | null;
  getReleaseChanges: (releaseId: string) => IReleaseChange[] | null;
  fetchReleases: (workspaceSlug: string, params?: { status?: string }) => Promise<IRelease[] | undefined>;
  fetchReleaseDetail: (workspaceSlug: string, releaseId: string) => Promise<IReleaseDetail | undefined>;
  createRelease: (workspaceSlug: string, data: TReleaseCreatePayload) => Promise<IRelease>;
  updateRelease: (workspaceSlug: string, releaseId: string, data: TReleaseUpdatePayload) => Promise<IRelease>;
  deleteRelease: (workspaceSlug: string, releaseId: string) => Promise<void>;
  linkChanges: (workspaceSlug: string, releaseId: string, issueIds: string[]) => Promise<IReleaseChange[]>;
  unlinkChange: (workspaceSlug: string, releaseId: string, issueId: string) => Promise<void>;
}

export class ReleaseStore implements IReleaseStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  releaseMap: Record<string, IRelease> = {};
  releaseIdsMap: Record<string, string[]> = {};
  detailMap: Record<string, IReleaseDetail> = {};
  errorMap: Record<string, boolean> = {};
  rootStore: CoreRootStore;
  releaseService: ReleaseService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      releaseMap: observable,
      releaseIdsMap: observable,
      detailMap: observable,
      errorMap: observable,
      fetchReleases: action,
      fetchReleaseDetail: action,
      createRelease: action,
      updateRelease: action,
      deleteRelease: action,
      linkChanges: action,
      unlinkChange: action,
    });
    this.rootStore = _rootStore;
    this.releaseService = new ReleaseService();
  }

  getReleaseById = computedFn((releaseId: string) => this.releaseMap[releaseId] ?? null);

  getWorkspaceReleaseIds = computedFn((workspaceSlug: string) => {
    if (!this.fetchedMap[workspaceSlug]) return null;
    return this.releaseIdsMap[workspaceSlug] ?? [];
  });

  getReleaseDetailById = computedFn((releaseId: string) => this.detailMap[releaseId] ?? null);

  getReleaseChanges = computedFn((releaseId: string) => this.detailMap[releaseId]?.changes ?? null);

  fetchReleases = async (workspaceSlug: string, params?: { status?: string }) => {
    try {
      runInAction(() => {
        set(this.errorMap, workspaceSlug, false);
      });
      this.loader = true;
      const releases = await this.releaseService.getReleases(workspaceSlug, params);
      runInAction(() => {
        releases.forEach((release) => set(this.releaseMap, [release.id], release));
        set(
          this.releaseIdsMap,
          [workspaceSlug],
          releases.map((release) => release.id)
        );
        set(this.fetchedMap, [workspaceSlug], true);
        this.loader = false;
      });
      return releases;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, workspaceSlug, true);
      });
      return undefined;
    }
  };

  fetchReleaseDetail = async (workspaceSlug: string, releaseId: string) => {
    try {
      const release = await this.releaseService.getRelease(workspaceSlug, releaseId);
      runInAction(() => {
        set(this.releaseMap, [release.id], release);
        set(this.detailMap, [releaseId], release);
      });
      return release;
    } catch {
      return undefined;
    }
  };

  createRelease = async (workspaceSlug: string, data: TReleaseCreatePayload) => {
    const release = await this.releaseService.createRelease(workspaceSlug, data);
    runInAction(() => {
      set(this.releaseMap, [release.id], release);
      set(this.releaseIdsMap, [workspaceSlug], [release.id, ...(this.releaseIdsMap[workspaceSlug] ?? [])]);
    });
    return release;
  };

  updateRelease = async (workspaceSlug: string, releaseId: string, data: TReleaseUpdatePayload) => {
    const original = this.getReleaseById(releaseId);
    if (!original) throw new Error("Release not found");
    try {
      runInAction(() => {
        set(this.releaseMap, [releaseId], { ...original, ...data });
        const detail = this.detailMap[releaseId];
        if (detail) set(this.detailMap, [releaseId], { ...detail, ...data });
      });
      const response = await this.releaseService.updateRelease(workspaceSlug, releaseId, data);
      runInAction(() => {
        set(this.releaseMap, [releaseId], response);
        const detail = this.detailMap[releaseId];
        if (detail) set(this.detailMap, [releaseId], { ...detail, ...response });
      });
      return response;
    } catch (error) {
      runInAction(() => {
        set(this.releaseMap, [releaseId], original);
      });
      throw error;
    }
  };

  deleteRelease = async (workspaceSlug: string, releaseId: string) => {
    await this.releaseService.deleteRelease(workspaceSlug, releaseId);
    runInAction(() => {
      delete this.releaseMap[releaseId];
      delete this.detailMap[releaseId];
      Object.keys(this.releaseIdsMap).forEach((key) => {
        this.releaseIdsMap[key] = (this.releaseIdsMap[key] ?? []).filter((id) => id !== releaseId);
      });
    });
  };

  linkChanges = async (workspaceSlug: string, releaseId: string, issueIds: string[]) => {
    const changes = await this.releaseService.linkReleaseChanges(workspaceSlug, releaseId, issueIds);
    runInAction(() => {
      const detail = this.detailMap[releaseId];
      if (detail) set(this.detailMap, [releaseId], { ...detail, changes });
    });
    return changes;
  };

  unlinkChange = async (workspaceSlug: string, releaseId: string, issueId: string) => {
    await this.releaseService.unlinkReleaseChange(workspaceSlug, releaseId, issueId);
    runInAction(() => {
      const detail = this.detailMap[releaseId];
      if (detail)
        set(this.detailMap, [releaseId], {
          ...detail,
          changes: detail.changes.filter((change) => change.issue_id !== issueId),
        });
    });
  };
}
```

- [ ] **Step 2: Create `review.store.ts` (requests slice; sessions menyusul di Phase 4)**

`apps/web/core/store/review.store.ts`:

```ts
import { set } from "lodash-es";
import { action, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
import type {
  IReviewRequest,
  IReviewRequestDetail,
  TReviewRequestCreatePayload,
  TReviewRequestListParams,
} from "@plane/types";
// services
import { latestRequestForSubject, reviewRequestsKey } from "@/services/review.helpers";
import { ReviewService } from "@/services/review.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IReviewStore {
  loader: boolean;
  fetchedKeys: Record<string, boolean>;
  requestMap: Record<string, IReviewRequest>;
  requestIdsMap: Record<string, string[]>;
  requestDetailMap: Record<string, IReviewRequestDetail>;
  errorMap: Record<string, boolean>;
  getRequestById: (requestId: string) => IReviewRequest | null;
  getRequestIds: (params: TReviewRequestListParams) => string[] | null;
  getRequestDetailById: (requestId: string) => IReviewRequestDetail | null;
  getRequestsForChange: (changeIssueId: string) => IReviewRequest[];
  getRequestsForRelease: (releaseId: string) => IReviewRequest[];
  getLatestRequestForChange: (changeIssueId: string) => IReviewRequest | null;
  getLatestRequestForRelease: (releaseId: string) => IReviewRequest | null;
  fetchRequests: (workspaceSlug: string, params: TReviewRequestListParams) => Promise<IReviewRequest[] | undefined>;
  submitRequest: (workspaceSlug: string, data: TReviewRequestCreatePayload) => Promise<IReviewRequest>;
  withdrawRequest: (workspaceSlug: string, requestId: string) => Promise<IReviewRequest>;
  fetchRequestDetail: (workspaceSlug: string, requestId: string) => Promise<IReviewRequestDetail | undefined>;
}

export class ReviewStore implements IReviewStore {
  loader: boolean = false;
  fetchedKeys: Record<string, boolean> = {};
  requestMap: Record<string, IReviewRequest> = {};
  requestIdsMap: Record<string, string[]> = {};
  requestDetailMap: Record<string, IReviewRequestDetail> = {};
  errorMap: Record<string, boolean> = {};
  rootStore: CoreRootStore;
  reviewService: ReviewService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedKeys: observable,
      requestMap: observable,
      requestIdsMap: observable,
      requestDetailMap: observable,
      errorMap: observable,
      fetchRequests: action,
      submitRequest: action,
      withdrawRequest: action,
      fetchRequestDetail: action,
    });
    this.rootStore = _rootStore;
    this.reviewService = new ReviewService();
  }

  getRequestById = computedFn((requestId: string) => this.requestMap[requestId] ?? null);

  getRequestIds = computedFn((params: TReviewRequestListParams) => {
    const key = reviewRequestsKey(params);
    if (!this.fetchedKeys[key]) return null;
    return this.requestIdsMap[key] ?? [];
  });

  getRequestDetailById = computedFn((requestId: string) => this.requestDetailMap[requestId] ?? null);

  getRequestsForChange = computedFn((changeIssueId: string) =>
    Object.values(this.requestMap).filter((request) => request.change_issue_id === changeIssueId)
  );

  getRequestsForRelease = computedFn((releaseId: string) =>
    Object.values(this.requestMap).filter((request) => request.release_id === releaseId)
  );

  getLatestRequestForChange = computedFn((changeIssueId: string) =>
    latestRequestForSubject(
      this.getRequestsForChange(changeIssueId).filter((request) => request.status !== "withdrawn")
    )
  );

  getLatestRequestForRelease = computedFn((releaseId: string) =>
    latestRequestForSubject(this.getRequestsForRelease(releaseId).filter((request) => request.status !== "withdrawn"))
  );

  fetchRequests = async (workspaceSlug: string, params: TReviewRequestListParams) => {
    const key = reviewRequestsKey(params);
    try {
      runInAction(() => {
        set(this.errorMap, key, false);
      });
      this.loader = true;
      const requests = await this.reviewService.getReviewRequests(workspaceSlug, params);
      runInAction(() => {
        requests.forEach((request) => set(this.requestMap, [request.id], request));
        set(
          this.requestIdsMap,
          [key],
          requests.map((request) => request.id)
        );
        set(this.fetchedKeys, [key], true);
        this.loader = false;
      });
      return requests;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, key, true);
      });
      return undefined;
    }
  };

  submitRequest = async (workspaceSlug: string, data: TReviewRequestCreatePayload) => {
    const request = await this.reviewService.submitReviewRequest(workspaceSlug, data);
    runInAction(() => {
      set(this.requestMap, [request.id], request);
    });
    return request;
  };

  withdrawRequest = async (workspaceSlug: string, requestId: string) => {
    const request = await this.reviewService.withdrawReviewRequest(workspaceSlug, requestId);
    runInAction(() => {
      set(this.requestMap, [request.id], request);
      if (this.requestDetailMap[requestId])
        set(this.requestDetailMap, [requestId], { ...this.requestDetailMap[requestId], ...request });
    });
    return request;
  };

  fetchRequestDetail = async (workspaceSlug: string, requestId: string) => {
    try {
      const request = await this.reviewService.getReviewRequest(workspaceSlug, requestId);
      runInAction(() => {
        set(this.requestMap, [request.id], request);
        set(this.requestDetailMap, [requestId], request);
      });
      return request;
    } catch {
      return undefined;
    }
  };
}
```

- [ ] **Step 3: Create hooks**

`apps/web/core/hooks/store/use-release.ts`:

```ts
import { useContext } from "react";
// store
import { StoreContext } from "@/lib/store-context";
import type { IReleaseStore } from "@/store/release.store";

export const useRelease = (): IReleaseStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useRelease must be used within StoreProvider");
  return context.release;
};
```

`apps/web/core/hooks/store/use-review.ts`:

```ts
import { useContext } from "react";
// store
import { StoreContext } from "@/lib/store-context";
import type { IReviewStore } from "@/store/review.store";

export const useReview = (): IReviewStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useReview must be used within StoreProvider");
  return context.review;
};
```

- [ ] **Step 4: Register in the root store**

In `apps/web/core/store/root.store.ts`:

After the war-room filter imports:

```ts
import type { IReleaseStore } from "./release.store";
import { ReleaseStore } from "./release.store";
import type { IReviewStore } from "./review.store";
import { ReviewStore } from "./review.store";
```

After `warRoomFilter: IWarRoomFilterStore;` in the class fields:

```ts
release: IReleaseStore;
review: IReviewStore;
```

After `this.warRoomFilter = new WarRoomFilterStore(this);` in the constructor **and** in `resetOnSignOut()`:

```ts
this.release = new ReleaseStore(this);
this.review = new ReviewStore(this);
```

- [ ] **Step 5: Typecheck**

Run:

```bash
pnpm --filter=web check:types
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/store/release.store.ts apps/web/core/store/review.store.ts \
  apps/web/core/hooks/store/use-release.ts apps/web/core/hooks/store/use-review.ts \
  apps/web/core/store/root.store.ts
git commit -m "feat(web): release + review stores"
```

---

### Task 6: i18n — namespace `release` + `review` (semua locale)

**Files:**

- Create: `packages/i18n/src/locales/en/release.json`
- Create: `packages/i18n/src/locales/en/review.json`
- Modify: `packages/i18n/src/constants/namespaces.ts`
- Modify: `packages/i18n/src/locales/en/navigation.json`
- Modify: `packages/i18n/src/locales/<19 locale lain>/{release,review,navigation}.json`
- Generated: `packages/i18n/src/types/keys.generated.ts`

- [ ] **Step 1: Create the English locale files**

`packages/i18n/src/locales/en/release.json`:

```json
{
  "release": {
    "title": "Releases",
    "add": "Add release",
    "edit": "Edit release",
    "delete": "Delete release",
    "search_placeholder": "Search releases",
    "all_statuses": "All statuses",
    "actions": {
      "create": "Create",
      "save": "Save"
    },
    "empty_state": {
      "title": "No releases yet",
      "description": "Create a release to bundle changes for RCB review.",
      "no_matches": {
        "title": "No matching releases",
        "description": "Try a different search or clear the filters."
      }
    },
    "load_error": {
      "title": "Couldn't load releases",
      "description": "Something went wrong while fetching releases.",
      "retry": "Retry"
    },
    "fields": {
      "name": "Release name",
      "version": "Version",
      "status": "Status",
      "target_date": "Target date",
      "description": "Description",
      "changes": "Changes",
      "review_history": "Review history"
    },
    "status_values": {
      "draft": "Draft",
      "planned": "Planned",
      "in_review": "In review",
      "approved": "Approved",
      "released": "Released",
      "cancelled": "Cancelled"
    },
    "changes": {
      "add": "Add changes",
      "empty": "No changes linked yet.",
      "unlink": "Remove",
      "link_failed": "Could not link changes. Please try again.",
      "unlink_failed": "Could not remove the change. Please try again.",
      "scope_warning": "This release is under review. Scope changes are visible to the board.",
      "tcb_badge": {
        "pending": "TCB pending",
        "scheduled": "TCB scheduled",
        "decided": "TCB decided",
        "withdrawn": "TCB withdrawn"
      }
    },
    "review": {
      "submit": "Submit to RCB",
      "resubmit": "Submit again",
      "withdraw": "Withdraw",
      "note_label": "Submission note",
      "note_placeholder": "Summarize readiness, risks and rollback plan",
      "empty_history": "No review requests yet.",
      "submitted": "Release submitted to RCB.",
      "withdrawn": "Review request withdrawn.",
      "submit_failed": "Could not submit the release. Please try again.",
      "withdraw_failed": "Could not withdraw the request. Please try again."
    },
    "delete_modal": {
      "title": "Delete release",
      "description": "Are you sure you want to delete this release? Bundled changes will be unlinked."
    },
    "toast": {
      "created": "Release created successfully.",
      "updated": "Release updated successfully.",
      "deleted": "Release deleted successfully."
    },
    "errors": {
      "create_failed": "Release could not be created. Please try again.",
      "update_failed": "Release could not be updated. Please try again.",
      "delete_failed": "Release could not be deleted. Please try again.",
      "active_review": "This release has an active review request. Withdraw it first.",
      "version_too_long": "Ensure version has no more than 100 characters."
    }
  }
}
```

`packages/i18n/src/locales/en/review.json`:

```json
{
  "review": {
    "board_values": {
      "tcb": "Testing Control Board",
      "rcb": "Release Control Board"
    },
    "request_status_values": {
      "pending": "Pending",
      "scheduled": "Scheduled",
      "decided": "Decided",
      "withdrawn": "Withdrawn"
    },
    "session_status_values": {
      "scheduled": "Scheduled",
      "completed": "Completed",
      "cancelled": "Cancelled"
    },
    "outcome_values": {
      "approved": "Approved",
      "rejected": "Rejected",
      "approved_with_notes": "Approved with notes",
      "deferred": "Deferred"
    },
    "participant_role_values": {
      "chair": "Chair",
      "secretary": "Secretary",
      "member": "Member"
    },
    "attendance_values": {
      "invited": "Invited",
      "present": "Present",
      "absent": "Absent"
    },
    "queue_tab": "Queue",
    "sessions_tab": "Sessions",
    "load_error": {
      "title": "Couldn't load the board",
      "description": "Something went wrong while fetching review data.",
      "retry": "Retry"
    },
    "empty_queue": {
      "title": "Nothing waiting for review",
      "description": "Submitted requests will appear here."
    },
    "empty_sessions": {
      "title": "No sessions yet",
      "description": "Schedule a session to review pending requests."
    },
    "no_matches": {
      "title": "No matching requests",
      "description": "Try a different search or clear the filters."
    },
    "request": {
      "submitted": "Change submitted to TCB.",
      "withdrawn": "Request withdrawn.",
      "submit_failed": "Could not submit the request. Please try again.",
      "withdraw_failed": "Could not withdraw the request. Please try again.",
      "submission_note": "Submission note"
    },
    "tcb": {
      "badge_pending": "TCB pending",
      "badge_scheduled": "TCB scheduled",
      "badge_decided": "TCB decided",
      "badge_withdrawn": "TCB withdrawn"
    },
    "sessions": {
      "add": "Schedule session",
      "edit": "Edit session",
      "title_label": "Session title",
      "scheduled_at": "Date and time",
      "location": "Location",
      "minutes": "Minutes",
      "minutes_placeholder": "Write the meeting minutes here",
      "complete": "Complete session",
      "cancel": "Cancel session",
      "complete_failed": "Could not complete the session. Please try again.",
      "cancel_failed": "Could not cancel the session. Please try again.",
      "created": "Session scheduled.",
      "updated": "Session updated.",
      "completed": "Session completed.",
      "cancelled": "Session cancelled.",
      "create_failed": "Could not schedule the session. Please try again.",
      "update_failed": "Could not update the session. Please try again.",
      "empty_items": "The agenda is empty. Add pending requests to review them.",
      "agenda": "Agenda",
      "pending_outcome": "Pending decisions: {count}"
    },
    "items": {
      "add": "Add to agenda",
      "add_confirm": "Add requests",
      "remove": "Remove from agenda",
      "remove_failed": "Could not remove the request. Please try again.",
      "add_failed": "Could not add the requests. Please try again.",
      "outcome_label": "Decision",
      "note_label": "Decision note",
      "save": "Record decision",
      "save_failed": "Could not record the decision. Please try again."
    },
    "participants": {
      "add": "Add participant",
      "role": "Role",
      "attendance": "Attendance",
      "remove": "Remove",
      "add_failed": "Could not add the participant. Please try again.",
      "update_failed": "Could not update the participant. Please try again.",
      "remove_failed": "Could not remove the participant. Please try again.",
      "empty": "No participants yet."
    },
    "notification": {
      "agenda_added": "{subject} was added to {session}",
      "decided": "{subject} review decision: {outcome}",
      "session_scheduled": "You are scheduled for {session}",
      "open": "Open board",
      "dismiss": "Dismiss"
    }
  }
}
```

- [ ] **Step 2: Register the namespaces**

In `packages/i18n/src/constants/namespaces.ts`, add to `NAMESPACES` between `"project-settings"` and `"service"`:

```ts
  "release",
  "review",
```

- [ ] **Step 3: Add sidebar labels**

In `packages/i18n/src/locales/en/navigation.json`, inside the `sidebar` object after `"services": "Services",`:

```json
    "releases": "Releases",
    "release_control": "Release control",
    "testing_control": "Testing control",
```

- [ ] **Step 4: Translate to every locale (REQUIRED SUB-SKILL)**

Invoke the **translate** skill and follow it exactly: copy the three new/updated keys into **all 19 non-English locales** (`cs de es fr id it ja ka-ge ko pl pt-BR ro ru sk tr-TR ua vi-VN zh-CN zh-TW`) — never copy English as a shortcut, preserve `{subject}`, `{session}`, `{outcome}`, `{count}` placeholders and do-not-translate acronyms (TCB, RCB, RCB/TCB, Plane).

- [ ] **Step 5: Regenerate key types + sync check**

Run:

```bash
pnpm --filter @plane/i18n run generate:types
pnpm --filter @plane/i18n run sync:check
```

Expected: `keys.generated.ts` bertambah key `release.*`/`review.*`; `0 missing, 0 stale, 0 collisions`.

- [ ] **Step 6: Commit**

```bash
git add packages/i18n
git commit -m "i18n(web): release + review namespaces"
```

---

### Task 7: Releases list UI

**Files:**

- Create: `apps/web/core/components/releases/release-status-pill.tsx`
- Create: `apps/web/core/components/releases/release-load-error-state.tsx`
- Create: `apps/web/core/components/releases/release-view-header.tsx`
- Create: `apps/web/core/components/releases/release-board-row.tsx`
- Create: `apps/web/core/components/releases/releases-board.tsx`
- Create: `apps/web/core/components/releases/release-form.tsx`
- Create: `apps/web/core/components/releases/create-release-modal.tsx`
- Create: `apps/web/core/components/releases/delete-release-modal.tsx`
- Create: `apps/web/core/components/releases/releases-list-view.tsx`
- Create: `apps/web/core/components/releases/index.ts`

- [ ] **Step 1: Status pill + load error state**

`apps/web/core/components/releases/release-status-pill.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { RELEASE_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReleaseStatus } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type Props = {
  status: TReleaseStatus;
  className?: string;
};

export const ReleaseStatusPill = observer(function ReleaseStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  const config = RELEASE_STATUS_CONFIG[status];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
```

`apps/web/core/components/releases/release-load-error-state.tsx`:

```tsx
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { WarningTriangleOutline } from "@makeplane/propel/icons";

type Props = {
  onRetry: () => void;
};

export function ReleaseLoadErrorState({ onRetry }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <WarningTriangleOutline className="size-8 text-tertiary" />
      <p className="text-sm font-medium text-primary">{t("release.load_error.title")}</p>
      <p className="text-xs text-secondary">{t("release.load_error.description")}</p>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        {t("release.load_error.retry")}
      </Button>
    </div>
  );
}
```

- [ ] **Step 2: List header**

`apps/web/core/components/releases/release-view-header.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel, RELEASE_STATUS_CONFIG, RELEASE_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { CloseOutline, RocketOutline, SearchOutline } from "@makeplane/propel/icons";
import type { TReleaseStatus } from "@plane/types";
import { Breadcrumbs, CustomSelect, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CreateReleaseModal } from "./create-release-modal";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useUserPermissions } from "@/hooks/store/user";

export const ReleaseViewHeader = observer(function ReleaseViewHeader() {
  const { workspaceSlug } = useParams();
  const router = useAppRouter();
  const searchParams = useSearchParams();
  const { t } = useTranslation();
  const { allowPermissions } = useUserPermissions();
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const canCreateRelease = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.WORKSPACE
  );
  const query = searchParams.get("q") ?? "";
  const status = searchParams.get("status") ?? "";

  const updateParams = (updates: { q?: string; status?: string }) => {
    const params = new URLSearchParams(searchParams.toString());
    Object.entries(updates).forEach(([key, value]) => {
      if (value) params.set(key, value);
      else params.delete(key);
    });
    const queryString = params.toString();
    router.replace(`/${workspaceSlug}/releases${queryString ? `?${queryString}` : ""}`);
  };

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("release.title")}
                href={`/${workspaceSlug}/releases`}
                icon={<RocketOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
      <Header.RightItem>
        <div className="flex h-full items-center gap-2 self-end">
          <div className="flex items-center gap-1 rounded-md border border-subtle px-2">
            <SearchOutline className="h-3.5 w-3.5 text-tertiary" />
            <input
              value={query}
              onChange={(event) => updateParams({ q: event.target.value })}
              placeholder={t("release.search_placeholder")}
              className="h-7 w-40 bg-transparent text-13 outline-none"
            />
            {query && (
              <button type="button" onClick={() => updateParams({ q: "" })}>
                <CloseOutline className="h-3.5 w-3.5 text-tertiary" />
              </button>
            )}
          </div>
          <CustomSelect
            value={status}
            onChange={(value: string) => updateParams({ status: value })}
            label={status ? t(RELEASE_STATUS_CONFIG[status as TReleaseStatus].label_key) : t("release.all_statuses")}
          >
            <CustomSelect.Option value="">{t("release.all_statuses")}</CustomSelect.Option>
            {RELEASE_STATUSES.map((value) => (
              <CustomSelect.Option key={value} value={value}>
                {t(RELEASE_STATUS_CONFIG[value].label_key)}
              </CustomSelect.Option>
            ))}
          </CustomSelect>
        </div>
        {canCreateRelease && (
          <Button variant="primary" onClick={() => setIsCreateModalOpen(true)} size="lg">
            {t("release.add")}
          </Button>
        )}
      </Header.RightItem>
      {workspaceSlug && (
        <CreateReleaseModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
        />
      )}
    </Header>
  );
});
```

- [ ] **Step 3: Board + row**

`apps/web/core/components/releases/release-board-row.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { getReleaseLink } from "@plane/constants";
import { ArrowExpandOutline } from "@makeplane/propel/icons";
import { renderFormattedDate } from "@plane/utils";
// components
import { ReleaseStatusPill } from "./release-status-pill";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  releaseId: string;
  workspaceSlug: string;
};

export const ReleaseBoardRow = observer(function ReleaseBoardRow({ releaseId, workspaceSlug }: Props) {
  const router = useAppRouter();
  const { getReleaseById } = useRelease();
  const release = getReleaseById(releaseId);

  if (!release) return null;
  const href = getReleaseLink(workspaceSlug, release.id);

  return (
    <div
      role="link"
      tabIndex={0}
      onClick={() => router.push(href)}
      onKeyDown={(event) => {
        if (event.key === "Enter") router.push(href);
      }}
      className="group flex cursor-pointer items-center gap-3 border-b border-subtle px-4 py-3 hover:bg-layer-1"
    >
      <span className="w-16 shrink-0 font-code text-11 text-tertiary">REL-{release.sequence_id}</span>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-13 font-medium text-primary">{release.name}</span>
        {release.version && <span className="text-11 text-tertiary">{release.version}</span>}
      </div>
      <span className="flex w-24 justify-center">
        <ReleaseStatusPill status={release.status} />
      </span>
      <span className="w-28 text-right text-12 text-secondary">
        {release.target_date ? renderFormattedDate(release.target_date) : "—"}
      </span>
      <ArrowExpandOutline className="h-3.5 w-3.5 text-tertiary opacity-0 group-hover:opacity-100" />
    </div>
  );
});
```

`apps/web/core/components/releases/releases-board.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { ReleaseBoardRow } from "./release-board-row";

type Props = {
  workspaceSlug: string;
  releaseIds: string[];
};

export const ReleasesBoard = observer(function ReleasesBoard({ workspaceSlug, releaseIds }: Props) {
  const { t } = useTranslation();

  return (
    <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto">
      <div className="sticky top-0 z-10 flex items-center gap-3 border-b border-subtle bg-surface-1 px-4 py-2 text-11 font-medium text-tertiary">
        <span className="w-16 shrink-0">&nbsp;</span>
        <span className="flex-1">{t("release.fields.name")}</span>
        <span className="w-24 text-center">{t("release.fields.status")}</span>
        <span className="w-28 text-right">{t("release.fields.target_date")}</span>
        <span className="w-3.5" />
      </div>
      {releaseIds.map((releaseId) => (
        <ReleaseBoardRow key={releaseId} releaseId={releaseId} workspaceSlug={workspaceSlug} />
      ))}
    </div>
  );
});
```

- [ ] **Step 4: Form + modals**

`apps/web/core/components/releases/release-form.tsx`:

```tsx
import { useEffect, useState } from "react";
import { Controller, useForm } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { RELEASE_STATUS_CONFIG, RELEASE_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IRelease, TReleaseStatus } from "@plane/types";
import { CustomSelect } from "@plane/ui";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  handleFormSubmit: (values: Partial<IRelease>) => Promise<void>;
  handleClose: () => void;
  status: boolean;
  workspaceSlug: string;
  data?: IRelease;
};

const defaultValues: Partial<IRelease> = {
  name: "",
  version: "",
  status: "draft",
  target_date: null,
  description_html: "<p></p>",
};

export function ReleaseForm(props: Props) {
  const { handleFormSubmit, handleClose, status, workspaceSlug, data } = props;
  const { t } = useTranslation();
  const { getWorkspaceBySlug } = useWorkspace();
  const [stableDescriptionHtml] = useState(data?.description_html ?? "<p></p>");
  const {
    formState: { errors, isSubmitting },
    handleSubmit,
    control,
    reset,
  } = useForm<IRelease>({
    defaultValues: {
      ...defaultValues,
      ...data,
      version: data?.version ?? "",
      target_date: data?.target_date ?? null,
      description_html: data?.description_html ?? "<p></p>",
    },
  });

  useEffect(() => {
    reset({
      ...defaultValues,
      ...data,
      version: data?.version ?? "",
      target_date: data?.target_date ?? null,
      description_html: data?.description_html ?? "<p></p>",
    });
  }, [data, reset]);

  const handleCreateUpdateRelease = async (formData: Partial<IRelease>) => {
    await handleFormSubmit({
      ...formData,
      version: formData.version?.trim() ? formData.version.trim() : null,
      target_date: formData.target_date || null,
      description_html: formData.description_html ?? "<p></p>",
    });
  };

  return (
    <form onSubmit={handleSubmit(handleCreateUpdateRelease)}>
      <div className="space-y-5 p-5">
        <div>
          <Controller
            control={control}
            name="name"
            rules={{
              required: t("title_is_required"),
              maxLength: { value: 255, message: t("title_should_be_less_than_255_characters") },
            }}
            render={({ field: { value, onChange } }) => (
              <Field name="name" invalid={Boolean(errors?.name)}>
                <InputGroup size="2xl">
                  <Input
                    size="2xl"
                    id="name"
                    name="name"
                    type="text"
                    value={value}
                    onChange={onChange}
                    placeholder={t("release.fields.name")}
                  />
                </InputGroup>
              </Field>
            )}
          />
          <span className="text-11 text-danger-primary">{errors?.name?.message}</span>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("release.fields.version")}</label>
            <Controller
              control={control}
              name="version"
              rules={{ maxLength: { value: 100, message: t("release.errors.version_too_long") } }}
              render={({ field: { value, onChange } }) => (
                <Input
                  id="version"
                  name="version"
                  type="text"
                  value={value ?? ""}
                  onChange={onChange}
                  placeholder="v1.0.0"
                />
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.version?.message}</span>
          </div>
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("release.fields.status")}</label>
            <Controller
              control={control}
              name="status"
              render={({ field: { value, onChange } }) => (
                <CustomSelect
                  value={value}
                  onChange={(next: TReleaseStatus) => onChange(next)}
                  label={t(RELEASE_STATUS_CONFIG[value as TReleaseStatus].label_key)}
                >
                  {RELEASE_STATUSES.map((releaseStatus) => (
                    <CustomSelect.Option key={releaseStatus} value={releaseStatus}>
                      {t(RELEASE_STATUS_CONFIG[releaseStatus].label_key)}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
              )}
            />
          </div>
        </div>

        <div>
          <label className="mb-1 block text-12 text-secondary">{t("release.fields.target_date")}</label>
          <Controller
            control={control}
            name="target_date"
            render={({ field: { value, onChange } }) => (
              <Input
                id="target_date"
                name="target_date"
                type="date"
                value={value ?? ""}
                onChange={(event) => onChange(event.target.value || null)}
              />
            )}
          />
        </div>

        <div>
          <label className="mb-1 block text-12 text-secondary">{t("release.fields.description")}</label>
          <Controller
            name="description_html"
            control={control}
            render={({ field: { onChange } }) => (
              <RichTextEditor
                editable
                key={data?.id ?? "release-create"}
                id="release-description-editor"
                initialValue={stableDescriptionHtml}
                value={stableDescriptionHtml}
                workspaceSlug={workspaceSlug}
                workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
                onChange={(_descriptionJson: object, descriptionHtml: string) => onChange(descriptionHtml)}
                placeholder={t("release.fields.description")}
                containerClassName="min-h-24 rounded-md border border-subtle"
              />
            )}
          />
        </div>
      </div>
      <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
        <Button variant="secondary" size="lg" onClick={handleClose}>
          {t("cancel")}
        </Button>
        <Button variant="primary" size="lg" type="submit" loading={isSubmitting}>
          {status ? t("release.actions.save") : t("release.actions.create")}
        </Button>
      </div>
    </form>
  );
}
```

`apps/web/core/components/releases/create-release-modal.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IRelease } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ReleaseForm } from "./release-form";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  data?: IRelease;
};

export const CreateReleaseModal = observer(function CreateReleaseModal({
  isOpen,
  onClose,
  workspaceSlug,
  data,
}: Props) {
  const { t } = useTranslation();
  const { createRelease, updateRelease } = useRelease();

  const handleFormSubmit = async (payload: Partial<IRelease>) => {
    try {
      if (data) {
        await updateRelease(workspaceSlug, data.id, {
          name: payload.name,
          version: payload.version ?? null,
          description_html: payload.description_html,
          status: payload.status,
          target_date: payload.target_date ?? null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.toast.updated") });
      } else {
        await createRelease(workspaceSlug, {
          name: payload.name ?? "",
          version: payload.version ?? null,
          description_html: payload.description_html ?? "<p></p>",
          status: payload.status,
          target_date: payload.target_date ?? null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.toast.created") });
      }
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          apiError?.detail ??
          apiError?.error ??
          (data ? t("release.errors.update_failed") : t("release.errors.create_failed")),
      });
      throw error;
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XL}>
      <ReleaseForm
        handleFormSubmit={handleFormSubmit}
        handleClose={onClose}
        status={!!data}
        workspaceSlug={workspaceSlug}
        data={data}
      />
    </ModalCore>
  );
});
```

`apps/web/core/components/releases/delete-release-modal.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IRelease } from "@plane/types";
import { AlertModalCore } from "@plane/ui";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  release: IRelease;
};

export const DeleteReleaseModal = observer(function DeleteReleaseModal({
  isOpen,
  onClose,
  workspaceSlug,
  release,
}: Props) {
  const { t } = useTranslation();
  const router = useAppRouter();
  const { deleteRelease } = useRelease();
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    setIsSubmitting(true);
    try {
      await deleteRelease(workspaceSlug, release.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.toast.deleted") });
      onClose();
      router.push(`/${workspaceSlug}/releases`);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.errors.delete_failed"),
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <AlertModalCore
      handleClose={onClose}
      handleSubmit={handleSubmit}
      isSubmitting={isSubmitting}
      isOpen={isOpen}
      title={t("release.delete_modal.title")}
      content={t("release.delete_modal.description")}
    />
  );
});
```

- [ ] **Step 5: List view + barrel**

`apps/web/core/components/releases/releases-list-view.tsx`:

```tsx
import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IRelease, TReleaseStatus } from "@plane/types";
// components
import { ReleaseLoadErrorState } from "./release-load-error-state";
import { ReleasesBoard } from "./releases-board";
// helpers
import { filterReleases, orderReleases } from "@/services/release.helpers";
// hooks
import { useRelease } from "@/hooks/store/use-release";

export const ReleasesListView = observer(function ReleasesListView() {
  const { workspaceSlug } = useParams();
  const searchParams = useSearchParams();
  const { t } = useTranslation();
  const { loader, errorMap, fetchedMap, releaseMap, getWorkspaceReleaseIds, fetchReleases } = useRelease();

  const slug = workspaceSlug?.toString() ?? "";
  const query = searchParams.get("q") ?? "";
  const status = searchParams.get("status") ?? "";
  const releaseIds = getWorkspaceReleaseIds(slug);
  const hasError = errorMap[slug];

  useEffect(() => {
    if (!slug || fetchedMap[slug]) return;
    void fetchReleases(slug);
  }, [slug, fetchedMap, fetchReleases]);

  if (hasError) {
    return (
      <ReleaseLoadErrorState
        onRetry={() => {
          void fetchReleases(slug);
        }}
      />
    );
  }
  if (loader || releaseIds === null) {
    return <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>;
  }

  const releases = releaseIds
    .map((releaseId) => releaseMap[releaseId])
    .filter((release): release is IRelease => Boolean(release));
  const filtered = orderReleases(filterReleases(releases, status ? { status: [status as TReleaseStatus] } : {}, query));

  if (releaseIds.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
        <p className="text-sm font-medium text-primary">{t("release.empty_state.title")}</p>
        <p className="text-xs text-secondary">{t("release.empty_state.description")}</p>
      </div>
    );
  }
  if (filtered.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
        <p className="text-sm font-medium text-primary">{t("release.empty_state.no_matches.title")}</p>
        <p className="text-xs text-secondary">{t("release.empty_state.no_matches.description")}</p>
      </div>
    );
  }
  return <ReleasesBoard workspaceSlug={slug} releaseIds={filtered.map((release) => release.id)} />;
});
```

`apps/web/core/components/releases/index.ts`:

```ts
export * from "./create-release-modal";
export * from "./delete-release-modal";
export * from "./release-board-row";
export * from "./release-form";
export * from "./release-load-error-state";
export * from "./release-status-pill";
export * from "./release-view-header";
export * from "./releases-board";
export * from "./releases-list-view";
```

- [ ] **Step 6: Typecheck + lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: PASS (tidak ada error import/tipe baru).

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/releases
git commit -m "feat(web): releases list ui"
```

---

### Task 8: Release detail UI (changes + review history + submit RCB)

**Files:**

- Create: `apps/web/core/components/reviews/request-status-pill.tsx`
- Create: `apps/web/core/components/reviews/outcome-pill.tsx`
- Create: `apps/web/core/components/reviews/tcb-status-badge.tsx`
- Create: `apps/web/core/components/reviews/index.ts`
- Create: `apps/web/core/components/releases/detail/breadcrumbs.tsx`
- Create: `apps/web/core/components/releases/detail/properties.tsx`
- Create: `apps/web/core/components/releases/detail/changes.tsx`
- Create: `apps/web/core/components/releases/detail/submit-review-modal.tsx`
- Create: `apps/web/core/components/releases/detail/review-history.tsx`
- Create: `apps/web/core/components/releases/detail/root.tsx`
- Create: `apps/web/core/components/releases/detail/index.ts`
- Modify: `apps/web/core/components/releases/index.ts`

- [ ] **Step 1: Shared review pills + TCB badge**

`apps/web/core/components/reviews/request-status-pill.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { REVIEW_REQUEST_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewRequestStatus } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type Props = {
  status: TReviewRequestStatus;
  className?: string;
};

export const ReviewRequestStatusPill = observer(function ReviewRequestStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  const config = REVIEW_REQUEST_STATUS_CONFIG[status];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
```

`apps/web/core/components/reviews/outcome-pill.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { REVIEW_OUTCOME_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewOutcome } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type Props = {
  outcome: TReviewOutcome;
  className?: string;
};

export const ReviewOutcomePill = observer(function ReviewOutcomePill({ outcome, className }: Props) {
  const { t } = useTranslation();
  const config = REVIEW_OUTCOME_CONFIG[outcome];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
```

`apps/web/core/components/reviews/tcb-status-badge.tsx`:

```tsx
import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TReviewRequestStatus } from "@plane/types";
// helpers
import { cn } from "@plane/utils";
// hooks
import { useReview } from "@/hooks/store/use-review";

const TCB_BADGE_KEYS: Record<TReviewRequestStatus, string> = {
  pending: "review.tcb.badge_pending",
  scheduled: "review.tcb.badge_scheduled",
  decided: "review.tcb.badge_decided",
  withdrawn: "review.tcb.badge_withdrawn",
};

const TCB_BADGE_CLASSES: Record<TReviewRequestStatus, string> = {
  pending: "bg-layer-2 text-secondary",
  scheduled: "bg-warning-subtle text-warning-primary",
  decided: "bg-success-subtle text-success-primary",
  withdrawn: "bg-layer-2 text-tertiary",
};

type Props = {
  changeIssueId: string;
  projectId: string;
  workspaceSlug: string;
  className?: string;
};

export const TcbStatusBadge = observer(function TcbStatusBadge({
  changeIssueId,
  projectId,
  workspaceSlug,
  className,
}: Props) {
  const { t } = useTranslation();
  const { getRequestIds, getLatestRequestForChange, fetchRequests } = useReview();

  const fetched = getRequestIds({ board_type: "tcb", project_id: projectId });

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: "tcb", project_id: projectId });
  }, [fetched, fetchRequests, projectId, workspaceSlug]);

  const request = getLatestRequestForChange(changeIssueId);
  if (!request) return null;

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", TCB_BADGE_CLASSES[request.status], className)}>
      {t(TCB_BADGE_KEYS[request.status])}
    </span>
  );
});
```

`apps/web/core/components/reviews/index.ts`:

```ts
export * from "./outcome-pill";
export * from "./request-status-pill";
export * from "./tcb-status-badge";
```

- [ ] **Step 2: Detail breadcrumbs (header + quick actions)**

`apps/web/core/components/releases/detail/breadcrumbs.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { getReleaseLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { RocketOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CreateReleaseModal } from "../create-release-modal";
import { DeleteReleaseModal } from "../delete-release-modal";
// hooks
import { useRelease } from "@/hooks/store/use-release";

export const ReleaseDetailBreadcrumbs = observer(function ReleaseDetailBreadcrumbs() {
  const { workspaceSlug, releaseId } = useParams();
  const { t } = useTranslation();
  const { getReleaseById } = useRelease();
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);

  const slug = workspaceSlug?.toString() ?? "";
  const release = releaseId ? getReleaseById(releaseId.toString()) : null;

  return (
    <>
      <Header>
        <Header.LeftItem>
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("release.title")}
                  href={`/${slug}/releases`}
                  icon={<RocketOutline className="h-4 w-4 text-tertiary" />}
                  isLast={!release}
                />
              }
              isLast={!release}
            />
            {release && (
              <Breadcrumbs.Item
                component={<BreadcrumbLink label={release.name} href={getReleaseLink(slug, release.id)} isLast />}
                isLast
              />
            )}
          </Breadcrumbs>
        </Header.LeftItem>
        <Header.RightItem>
          {release && (
            <div className="flex items-center gap-2">
              <Button variant="secondary" size="lg" onClick={() => setIsEditOpen(true)}>
                {t("release.edit")}
              </Button>
              <Button
                variant="secondary"
                size="lg"
                className="text-danger-primary"
                onClick={() => setIsDeleteOpen(true)}
              >
                {t("release.delete")}
              </Button>
            </div>
          )}
        </Header.RightItem>
      </Header>
      {release && (
        <>
          <CreateReleaseModal
            isOpen={isEditOpen}
            onClose={() => setIsEditOpen(false)}
            workspaceSlug={slug}
            data={release}
          />
          <DeleteReleaseModal
            isOpen={isDeleteOpen}
            onClose={() => setIsDeleteOpen(false)}
            workspaceSlug={slug}
            release={release}
          />
        </>
      )}
    </>
  );
});
```

- [ ] **Step 3: Properties sidebar**

`apps/web/core/components/releases/detail/properties.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { RELEASE_STATUS_CONFIG, RELEASE_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IRelease, TReleaseStatus } from "@plane/types";
import { CustomSelect, Input } from "@plane/ui";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  workspaceSlug: string;
  release: IRelease;
};

export const ReleaseDetailProperties = observer(function ReleaseDetailProperties({ workspaceSlug, release }: Props) {
  const { t } = useTranslation();
  const { updateRelease } = useRelease();
  const [version, setVersion] = useState(release.version ?? "");
  const [targetDate, setTargetDate] = useState(release.target_date ?? "");

  const patchRelease = async (data: Partial<IRelease>) => {
    try {
      await updateRelease(workspaceSlug, release.id, data);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.errors.update_failed"),
      });
    }
  };

  return (
    <div className="space-y-4 p-4">
      <div>
        <p className="mb-1 text-11 font-medium text-tertiary">{t("release.fields.status")}</p>
        <CustomSelect
          value={release.status}
          onChange={(value: TReleaseStatus) => void patchRelease({ status: value })}
          label={t(RELEASE_STATUS_CONFIG[release.status].label_key)}
        >
          {RELEASE_STATUSES.map((status) => (
            <CustomSelect.Option key={status} value={status}>
              {t(RELEASE_STATUS_CONFIG[status].label_key)}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      </div>
      <div>
        <p className="mb-1 text-11 font-medium text-tertiary">{t("release.fields.version")}</p>
        <Input
          value={version}
          onChange={(event) => setVersion(event.target.value)}
          onBlur={() => void patchRelease({ version: version.trim() ? version.trim() : null })}
          placeholder="v1.0.0"
        />
      </div>
      <div>
        <p className="mb-1 text-11 font-medium text-tertiary">{t("release.fields.target_date")}</p>
        <Input
          type="date"
          value={targetDate}
          onChange={(event) => setTargetDate(event.target.value)}
          onBlur={() => void patchRelease({ target_date: targetDate || null })}
        />
      </div>
      <div className="border-t border-subtle pt-3 text-11 text-tertiary">
        <p>REL-{release.sequence_id}</p>
      </div>
    </div>
  );
});
```

- [ ] **Step 4: Changes section**

`apps/web/core/components/releases/detail/changes.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse } from "@plane/types";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
import { TcbStatusBadge } from "@/components/reviews";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  workspaceSlug: string;
  releaseId: string;
  canWrite: boolean;
  isUnderReview: boolean;
};

export const ReleaseDetailChanges = observer(function ReleaseDetailChanges({
  workspaceSlug,
  releaseId,
  canWrite,
  isUnderReview,
}: Props) {
  const { t } = useTranslation();
  const { getReleaseChanges, linkChanges, unlinkChange } = useRelease();
  const [isPickerOpen, setIsPickerOpen] = useState(false);

  const changes = getReleaseChanges(releaseId) ?? [];
  const linkedIssueIds = changes.map((change) => change.issue_id);

  const handleLink = async (issues: ISearchIssueResponse[]) => {
    try {
      await linkChanges(
        workspaceSlug,
        releaseId,
        issues.map((issue) => issue.id)
      );
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Error!", message: t("release.changes.link_failed") });
    }
  };

  const handleUnlink = async (issueId: string) => {
    try {
      await unlinkChange(workspaceSlug, releaseId, issueId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Error!", message: t("release.changes.unlink_failed") });
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("release.fields.changes")}</h3>
        {canWrite && (
          <Button variant="secondary" size="sm" onClick={() => setIsPickerOpen(true)}>
            {t("release.changes.add")}
          </Button>
        )}
      </div>
      {isUnderReview && (
        <p className="rounded-md bg-warning-subtle px-3 py-2 text-12 text-warning-primary">
          {t("release.changes.scope_warning")}
        </p>
      )}
      {changes.length === 0 ? (
        <p className="text-12 text-secondary">{t("release.changes.empty")}</p>
      ) : (
        <ul className="divide-y divide-subtle rounded-md border border-subtle">
          {changes.map((change) => (
            <li key={change.id} className="flex items-center gap-2 px-3 py-2">
              <a
                href={`/${workspaceSlug}/browse/${change.issue_identifier}/`}
                className="flex min-w-0 flex-1 items-center gap-2 text-13 text-primary hover:underline"
              >
                <span className="font-code text-11 text-tertiary">{change.issue_identifier}</span>
                <span className="truncate">{change.issue_name}</span>
              </a>
              <TcbStatusBadge
                changeIssueId={change.issue_id}
                projectId={change.project_id}
                workspaceSlug={workspaceSlug}
              />
              {canWrite && (
                <button
                  type="button"
                  className="text-11 text-tertiary hover:text-danger-primary"
                  onClick={() => void handleUnlink(change.issue_id)}
                >
                  {t("release.changes.unlink")}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <ExistingIssuesListModal
        isOpen={isPickerOpen}
        handleClose={() => setIsPickerOpen(false)}
        workspaceSlug={workspaceSlug}
        searchParams={{}}
        workspaceLevelToggle
        selectedWorkItemIds={linkedIssueIds}
        shouldHideIssue={(issue) => linkedIssueIds.includes(issue.id)}
        handleOnSubmit={handleLink}
      />
    </section>
  );
});
```

- [ ] **Step 5: Submit-review modal + review history**

`apps/web/core/components/releases/detail/submit-review-modal.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useRelease } from "@/hooks/store/use-release";
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  releaseId: string;
};

export const SubmitReleaseReviewModal = observer(function SubmitReleaseReviewModal({
  isOpen,
  onClose,
  workspaceSlug,
  releaseId,
}: Props) {
  const { t } = useTranslation();
  const { updateRelease } = useRelease();
  const { submitRequest } = useReview();
  const [note, setNote] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    setIsSubmitting(true);
    try {
      await submitRequest(workspaceSlug, {
        board_type: "rcb",
        release_id: releaseId,
        submission_note: note.trim(),
      });
      // Konvensi: submit menandai release in_review (bukan gate).
      await updateRelease(workspaceSlug, releaseId, { status: "in_review" });
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.review.submitted") });
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.review.submit_failed"),
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{t("release.review.submit")}</h3>
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("release.review.note_label")}</label>
          <TextArea
            value={note}
            onChange={(event) => setNote(event.target.value)}
            placeholder={t("release.review.note_placeholder")}
            rows={4}
          />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isSubmitting} onClick={() => void handleSubmit()}>
            {t("release.review.submit")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

`apps/web/core/components/releases/detail/review-history.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// components
import { ReviewOutcomePill, ReviewRequestStatusPill } from "@/components/reviews";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { SubmitReleaseReviewModal } from "./submit-review-modal";

type Props = {
  workspaceSlug: string;
  releaseId: string;
  canWrite: boolean;
};

export const ReleaseReviewHistory = observer(function ReleaseReviewHistory({
  workspaceSlug,
  releaseId,
  canWrite,
}: Props) {
  const { t } = useTranslation();
  const {
    fetchRequests,
    fetchRequestDetail,
    getRequestIds,
    getRequestDetailById,
    getRequestsForRelease,
    getLatestRequestForRelease,
    withdrawRequest,
  } = useReview();
  const [isSubmitOpen, setIsSubmitOpen] = useState(false);

  const fetched = getRequestIds({ board_type: "rcb", release_id: releaseId });

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: "rcb", release_id: releaseId });
  }, [fetched, fetchRequests, releaseId, workspaceSlug]);

  const requests = getRequestsForRelease(releaseId).sort((a, b) => b.submitted_at.localeCompare(a.submitted_at));
  const latest = getLatestRequestForRelease(releaseId);
  const hasActive = latest !== null && (latest.status === "pending" || latest.status === "scheduled");

  // Decided requests need their cycle history to show the latest outcome.
  useEffect(() => {
    requests
      .filter((request) => request.status === "decided" && !getRequestDetailById(request.id))
      .forEach((request) => {
        void fetchRequestDetail(workspaceSlug, request.id);
      });
  }, [requests, fetchRequestDetail, getRequestDetailById, workspaceSlug]);

  const handleWithdraw = async () => {
    if (!latest) return;
    try {
      await withdrawRequest(workspaceSlug, latest.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.review.withdrawn") });
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.review.withdraw_failed"),
      });
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("release.fields.review_history")}</h3>
        {canWrite && !hasActive && (
          <Button variant="secondary" size="sm" onClick={() => setIsSubmitOpen(true)}>
            {requests.length === 0 ? t("release.review.submit") : t("release.review.resubmit")}
          </Button>
        )}
        {canWrite && hasActive && (
          <Button variant="secondary" size="sm" onClick={() => void handleWithdraw()}>
            {t("release.review.withdraw")}
          </Button>
        )}
      </div>
      {requests.length === 0 ? (
        <p className="text-12 text-secondary">{t("release.review.empty_history")}</p>
      ) : (
        <ul className="space-y-2">
          {requests.map((request) => {
            const detail = getRequestDetailById(request.id);
            const latestOutcome = detail?.history
              .filter((entry) => entry.outcome)
              .sort((a, b) => (b.decided_at ?? "").localeCompare(a.decided_at ?? ""))[0]?.outcome;
            return (
              <li
                key={request.id}
                className="flex flex-col gap-1 rounded-md border border-subtle px-3 py-2 text-12 text-secondary"
              >
                <div className="flex items-center gap-2">
                  <ReviewRequestStatusPill status={request.status} />
                  {latestOutcome && <ReviewOutcomePill outcome={latestOutcome} />}
                  <span className="ml-auto text-11 text-tertiary">
                    {renderFormattedDate(request.submitted_at)} {renderFormattedTime(request.submitted_at)}
                  </span>
                </div>
                {request.submission_note && <p className="text-12 text-secondary">{request.submission_note}</p>}
              </li>
            );
          })}
        </ul>
      )}
      <SubmitReleaseReviewModal
        isOpen={isSubmitOpen}
        onClose={() => setIsSubmitOpen(false)}
        workspaceSlug={workspaceSlug}
        releaseId={releaseId}
      />
    </section>
  );
});
```

- [ ] **Step 6: Detail root + barrel**

`apps/web/core/components/releases/detail/root.tsx`:

```tsx
import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { sanitizeHTML } from "@plane/utils";
// components
import { ReleaseDetailChanges } from "./changes";
import { ReleaseDetailProperties } from "./properties";
import { ReleaseReviewHistory } from "./review-history";
// hooks
import { useRelease } from "@/hooks/store/use-release";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  releaseId: string;
};

export const ReleaseDetailRoot = observer(function ReleaseDetailRoot({ workspaceSlug, releaseId }: Props) {
  const { t } = useTranslation();
  const { getReleaseDetailById, fetchReleaseDetail } = useRelease();
  const { allowPermissions } = useUserPermissions();

  const canWrite = allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE);
  const release = getReleaseDetailById(releaseId);

  useEffect(() => {
    if (release) return;
    void fetchReleaseDetail(workspaceSlug, releaseId);
  }, [release, fetchReleaseDetail, releaseId, workspaceSlug]);

  if (!release) {
    return <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>;
  }

  return (
    <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
      <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
        <div>
          <h1 className="text-16 font-semibold text-primary">{release.name}</h1>
          <div
            className="mt-2 text-13 text-secondary"
            dangerouslySetInnerHTML={{ __html: sanitizeHTML(release.description_html || "<p></p>") }}
          />
        </div>
        <ReleaseDetailChanges
          workspaceSlug={workspaceSlug}
          releaseId={releaseId}
          canWrite={canWrite}
          isUnderReview={release.status === "in_review"}
        />
        <ReleaseReviewHistory workspaceSlug={workspaceSlug} releaseId={releaseId} canWrite={canWrite} />
      </div>
      <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
        <ReleaseDetailProperties workspaceSlug={workspaceSlug} release={release} />
      </div>
    </div>
  );
});
```

`apps/web/core/components/releases/detail/index.ts`:

```ts
export * from "./breadcrumbs";
export * from "./changes";
export * from "./properties";
export * from "./review-history";
export * from "./root";
export * from "./submit-review-modal";
```

Update `apps/web/core/components/releases/index.ts` to append:

```ts
export * from "./detail";
```

- [ ] **Step 7: Typecheck + lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: PASS (`TextArea`, `AlertModalCore`, `sanitizeHTML`, `ReviewOutcomePill` terverifikasi ada di paket masing-masing).

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/reviews apps/web/core/components/releases/detail \
  apps/web/core/components/releases/index.ts
git commit -m "feat(web): release detail changes + rcb review history"
```

---

### Task 9: Routes, pages, workspace navigation

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(list)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(list)/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(detail)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(detail)/[releaseId]/page.tsx`
- Modify: `apps/web/app/routes/core.ts`
- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`

- [ ] **Step 1: Create page + layout files**

`apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(list)/layout.tsx`:

```tsx
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ReleaseViewHeader } from "@/components/releases";

export default function WorkspaceReleasesListLayout() {
  return (
    <>
      <AppHeader header={<ReleaseViewHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

`apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(list)/page.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReleasesListView } from "@/components/releases";

function WorkspaceReleasesPage() {
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("release.title")} />
      <div className="relative h-full w-full overflow-hidden">
        <ReleasesListView />
      </div>
    </>
  );
}

export default observer(WorkspaceReleasesPage);
```

`apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(detail)/layout.tsx`:

```tsx
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ReleaseDetailBreadcrumbs } from "@/components/releases";

export default function WorkspaceReleaseDetailLayout() {
  return (
    <>
      <AppHeader header={<ReleaseDetailBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

`apps/web/app/(all)/[workspaceSlug]/(projects)/releases/(detail)/[releaseId]/page.tsx`:

```tsx
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReleaseDetailRoot } from "@/components/releases";

export default function WorkspaceReleaseDetailPage() {
  const { workspaceSlug, releaseId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !releaseId) return null;

  return (
    <>
      <PageHead title={t("release.title")} />
      <ReleaseDetailRoot workspaceSlug={workspaceSlug.toString()} releaseId={releaseId.toString()} />
    </>
  );
}
```

- [ ] **Step 2: Register routes**

In `apps/web/app/routes/core.ts`, immediately after the Drafts block (`:workspaceSlug/drafts`), add:

```ts
          // Releases (RCB scope)
          layout("./(all)/[workspaceSlug]/(projects)/releases/(list)/layout.tsx", [
            route(
              ":workspaceSlug/releases",
              "./(all)/[workspaceSlug]/(projects)/releases/(list)/page.tsx"
            ),
          ]),

          // Release Detail
          layout("./(all)/[workspaceSlug]/(projects)/releases/(detail)/layout.tsx", [
            route(
              ":workspaceSlug/releases/:releaseId",
              "./(all)/[workspaceSlug]/(projects)/releases/(detail)/[releaseId]/page.tsx"
            ),
          ]),
```

- [ ] **Step 3: Add the workspace nav item**

In `packages/constants/src/workspace.ts`, add to `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS` after the `archives` entry:

```ts
  releases: {
    key: "releases",
    labelTranslationKey: "sidebar.releases",
    href: `/releases/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname, url) => pathname.includes(url),
  },
```

And to `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS` after the archives entry:

```ts
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["releases"],
```

- [ ] **Step 4: Add the sidebar icon**

In `apps/web/core/components/workspace/sidebar/helper.tsx`, add `RocketOutline` to the `@makeplane/propel/icons` import list, then add a case at the end of the switch:

```tsx
    case "releases":
      return <RocketOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 5: Typecheck + lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/releases" apps/web/app/routes/core.ts \
  packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/helper.tsx
git commit -m "feat(web): releases routes + workspace navigation"
```

---

### Task 10: Verification wrap-up

**Files:** none (verification only).

- [ ] **Step 1: Unit tests + typecheck + lint (full web)**

Run:

```bash
pnpm --filter=web test
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: semua PASS.

- [ ] **Step 2: i18n sync**

Run:

```bash
pnpm --filter @plane/i18n run sync:check
```

Expected: `0 missing, 0 stale, 0 collisions`.

- [ ] **Step 3: Build + serve prod (wajib sesuai `AGENTS.md`)**

Run:

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build sukses; service restart tanpa error. Buka `https://<tunnel>/<workspaceSlug>/releases` — halaman tampil, nav "Releases" muncul, create/list/detail/submit RCB berfungsi.

- [ ] **Step 4: Commit (jika ada perbaikan)**

```bash
git add -A apps/web packages
git commit -m "chore(web): releases polish"
```

(Skip jika tidak ada perubahan.)

---

## Self-review notes

- **Spec coverage Phase 3:** halaman Releases workspace-level ✔; create/edit/delete dengan validasi ✔; bundling change lintas project + picker + unlink ✔; badge status TCB per change ✔; riwayat review + outcome ✔; submit/withdraw RCB ✔; permission UI (workspace ADMIN/MEMBER) ✔; nav workspace ✔; i18n ketiga bahasa (en source + 19 locale via translate skill) ✔.
- **Phase 4 (plan terpisah):** halaman RCB (queue + sessions) & TCB (queue + sessions), session detail (agenda, outcome, notulen, peserta), tombol + badge TCB di detail Change, render notifikasi `review_request`/`review_session`, **dan perpanjangan whitelist `entity_name` di `apps/api-rs/crates/api/src/routes/notification.rs:266`** (`'issue' | 'ai_schedule_run' | 'war_room'` + `'review_request' | 'review_session'`), plus test notifikasi Rust.
- **Deviasi sadar:** badge TCB menampilkan status request (bukan outcome) — outcome tampil di riwayat review (fetch detail per request, N kecil); submit RCB meng-update status release ke `in_review` sebagai konvensi.
- **Urutan i18n:** Task 6 harus selesai sebelum Task 7-9 agar `TTranslationKeys` sudah memuat key baru.
- **Urutan komponen:** Task 7 (list) sebelum Task 8 (detail memakai `release` store yang sama) dan Task 9 (routes mengimpor keduanya).
