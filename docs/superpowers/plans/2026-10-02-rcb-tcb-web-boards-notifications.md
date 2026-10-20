# RCB/TCB Boards & Notifications Implementation Plan (Plan 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the RCB/TCB feature: board pages (queue + sessions) for Release Control (workspace) and Testing Control (project), a shared session detail (agenda/outcome/minutes/participants), Submit-to-TCB in the Change work item detail, and web rendering for `review_request`/`review_session` notifications (plus the api-rs list whitelist).

**Architecture:** All review UI is built once in `apps/web/core/components/reviews/` and parameterized by `board_type` (`tcb` | `rcb`) + optional `project_id`. The `ReviewStore` is extended with session/item/participant state and actions on top of the existing request state. Pages are thin route shells in `apps/web/app/`; nav items link to them. Notification rendering follows the existing war-room/ai-schedule pattern (`data` shape guards, not `entity_name`).

**Tech Stack:** React Router (data routes), MobX stores, react-hook-form, `@plane/ui` + `@makeplane/propel`, Tailwind tokens, vitest, Rust/axum + sqlx.

**Prior plans (done):** backend `release.rs` + `review.rs` and web foundations/releases (types, services, helpers, stores for requests, releases list/detail). This plan assumes those are merged.

---

## File Structure

**Backend (Rust):**

- Modify: `apps/api-rs/crates/api/src/routes/notification.rs` (list whitelist)
- Modify: `apps/api-rs/crates/api/tests/notification_test.rs` (whitelist test)

**Types / constants / i18n:**

- Modify: `packages/types/src/workspace-notifications.ts`
- Modify: `packages/i18n/src/locales/*/review.json` + `packages/i18n/src/types/keys.generated.ts` (generated)

**Web lib/services/store:**

- Create: `apps/web/core/lib/review-notification.ts` + `.test.ts`
- Modify: `apps/web/core/services/review.helpers.ts` + `.test.ts`
- Modify: `apps/web/core/store/review.store.ts`

**Web components (`apps/web/core/components/reviews/`):**

- Create: `session-status-pill.tsx`
- Create: `submit-review-modal.tsx` (shared, board-parameterized)
- Create: `change-tcb-control.tsx` (work item detail integration)
- Create: `board/review-board-root.tsx`, `board/review-queue-board.tsx`, `board/review-request-row.tsx`, `board/review-sessions-board.tsx`, `board/review-session-row.tsx`, `board/session-form-modal.tsx`
- Create: `session/review-session-detail-root.tsx`, `session/session-detail-header.tsx`, `session/session-minutes.tsx`, `session/session-agenda.tsx`, `session/add-agenda-items-modal.tsx`, `session/record-outcome-modal.tsx`, `session/session-participants.tsx`, `session/workspace-member-select.tsx`, `session/session-detail-breadcrumbs.tsx`
- Create: `release-control/release-control-header.tsx`, `testing-control/testing-control-header.tsx`
- Modify: `index.ts` (re-exports)
- Delete: `apps/web/core/components/releases/detail/submit-review-modal.tsx` (replaced by shared modal)

**Web work item integration:**

- Modify: `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`

**Web notifications:**

- Create: `apps/web/core/components/workspace-notifications/review-detail.tsx`
- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`
- Modify: `apps/web/core/components/workspace-notifications/root.tsx`

**Web routes/pages/nav:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(list)/layout.tsx` + `page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(detail)/layout.tsx` + `(detail)/sessions/[sessionId]/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/layout.tsx` + `page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(detail)/layout.tsx` + `(detail)/sessions/[sessionId]/page.tsx`
- Modify: `apps/web/app/routes/core.ts`
- Modify: `packages/constants/src/workspace.ts` (workspace nav `release_control`)
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx` (icon case)
- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx` (TCB nav, sidebar mode)
- Modify: `apps/web/core/components/navigation/use-navigation-items.ts` (TCB nav, tabbed mode)

**Conventions reminder (from prior plans):**

- `Input` requires `size="2xl"`; target is ES2022 → no `toSorted` (use `[...x].sort()` + `// oxlint-disable-next-line unicorn/no-array-sort`).
- `RichTextEditor` is only used in release form; this plan uses plain `TextArea` for session minutes, so no editor wiring needed.
- Pre-commit hook runs oxfmt + `oxlint --fix --deny-warnings`.

---

### Task 1: Backend — include review entity types in the notification list

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/notification.rs:262-269`
- Test: `apps/api-rs/crates/api/tests/notification_test.rs:79-245`

- [ ] **Step 1: Update the test first (it must fail)**

In `apps/api-rs/crates/api/tests/notification_test.rs`, inside `list_includes_known_entity_types`, extend the fixture array. Replace this block:

```rust
    let run_id = Uuid::new_v4();
    for (entity_name, title, sender, data) in [
        (
            "issue",
            "Issue notification",
            "in_app:issue_activities:created",
            json!({}),
        ),
        (
            "ai_schedule_run",
            "Daily report",
            "in_app:ai_schedule:run",
            json!({"ai_schedule": {"schedule_id": schedule_id.to_string(), "run_id": run_id.to_string()}}),
        ),
        (
            "war_room",
            "Checkout down",
            "in_app:war_room:mentioned",
            json!({"war_room": {
                "id": Uuid::new_v4().to_string(),
                "project_id": Uuid::new_v4().to_string(),
                "workspace_slug": slug.clone(),
                "name": "Checkout down",
                "sequence_id": 1,
            }}),
        ),
        ("mystery", "Hidden", "in_app:other", json!({})),
    ] {
```

with:

```rust
    let run_id = Uuid::new_v4();
    for (entity_name, title, sender, data) in [
        (
            "issue",
            "Issue notification",
            "in_app:issue_activities:created",
            json!({}),
        ),
        (
            "ai_schedule_run",
            "Daily report",
            "in_app:ai_schedule:run",
            json!({"ai_schedule": {"schedule_id": schedule_id.to_string(), "run_id": run_id.to_string()}}),
        ),
        (
            "war_room",
            "Checkout down",
            "in_app:war_room:mentioned",
            json!({"war_room": {
                "id": Uuid::new_v4().to_string(),
                "project_id": Uuid::new_v4().to_string(),
                "workspace_slug": slug.clone(),
                "name": "Checkout down",
                "sequence_id": 1,
            }}),
        ),
        (
            "review_request",
            "Review requested",
            "in_app:review:agenda_added",
            json!({"review_request": {
                "id": Uuid::new_v4().to_string(),
                "board_type": "tcb",
                "project_id": Uuid::new_v4().to_string(),
                "workspace_slug": slug.clone(),
                "status": "scheduled",
                "subject_label": "CHG-1 Fix checkout",
                "session_id": Uuid::new_v4().to_string(),
                "session_title": "Weekly TCB",
                "scheduled_at": "2026-10-05T09:00:00Z",
            }}),
        ),
        (
            "review_session",
            "Review scheduled",
            "in_app:review:session_scheduled",
            json!({"review_session": {
                "id": Uuid::new_v4().to_string(),
                "board_type": "tcb",
                "project_id": Uuid::new_v4().to_string(),
                "workspace_slug": slug.clone(),
                "title": "Weekly TCB",
                "scheduled_at": "2026-10-05T09:00:00Z",
            }}),
        ),
        ("mystery", "Hidden", "in_app:other", json!({})),
    ] {
```

Then extend the default-list assertions. Replace:

```rust
    assert!(names.contains(&"issue"), "issue notifications still listed");
    assert!(
        names.contains(&"ai_schedule_run"),
        "schedule run notifications must be listed"
    );
```

with:

```rust
    assert!(names.contains(&"issue"), "issue notifications still listed");
    assert!(
        names.contains(&"ai_schedule_run"),
        "schedule run notifications must be listed"
    );
    assert!(
        names.contains(&"review_request"),
        "review request notifications must be listed"
    );
    assert!(
        names.contains(&"review_session"),
        "review session notifications must be listed"
    );
```

In the `mentioned=true` block, after the `ai_schedule_run` assertion add:

```rust
    assert!(
        !names.contains(&"review_request"),
        "review requests are not mention notifications"
    );
    assert!(
        !names.contains(&"review_session"),
        "review sessions are not mention notifications"
    );
```

Update the unread counts (two extra non-mention rows):

```rust
    assert_eq!(counts["total_unread_notifications_count"], json!(5));
    assert_eq!(counts["mention_unread_notifications_count"], json!(1));
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test notification_test -- --test-threads=1
```

Expected: FAIL — `review request notifications must be listed` (the list filter currently excludes them).

- [ ] **Step 3: Update the whitelist**

In `apps/api-rs/crates/api/src/routes/notification.rs`, change the `base_where` line:

```rust
          AND n.entity_name IN ('issue', 'ai_schedule_run', 'war_room') \
```

to:

```rust
          AND n.entity_name IN ('issue', 'ai_schedule_run', 'war_room', 'review_request', 'review_session') \
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test notification_test -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 5: Format only the touched files**

```bash
cd apps/api-rs
rustfmt --edition 2021 crates/api/src/routes/notification.rs crates/api/tests/notification_test.rs
git diff --stat crates/api/src/routes/notification.rs crates/api/tests/notification_test.rs
```

(Do NOT run a global `cargo fmt`; repo baseline is dirty from toolchain drift.)

- [ ] **Step 6: Rebuild the backend and verify**

Per `AGENTS.md`, detached build (link stage can take 10+ min with no output):

```bash
cd /home/ghifari/plane-for-itsm
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker \
  > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll until healthy:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8000/health
systemctl --user restart plane-live.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `200` and `200`.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/notification.rs apps/api-rs/crates/api/tests/notification_test.rs
git commit -m "feat(api-rs): list review notifications"
```

---

### Task 2: i18n — add the new review board/session keys

**Files:**

- Modify: `packages/i18n/src/locales/en/review.json`
- Modify: every other locale `packages/i18n/src/locales/<locale>/review.json` (19 locales)
- Generated: `packages/i18n/src/types/keys.generated.ts`

**REQUIRED before touching any locale file other than `en`:** read and follow `.claude/skills/translate/SKILL.md`. Do not machine-translate ad hoc.

- [ ] **Step 1: Add keys to the English source**

In `packages/i18n/src/locales/en/review.json`, add these entries (`review` root object). Insert `all_statuses` after `sessions_tab`, `queue` after `no_matches`, and the rest at their alphabetical-ish neighbours:

```json
    "all_statuses": "All statuses",
    "queue": {
      "subject": "Subject",
      "status": "Status",
      "submitted_at": "Submitted",
      "session": "Session"
    },
```

Inside `"sessions"`, add:

```json
      "status": "Status",
      "items_count": "Agenda items",
      "participants_count": "Participants",
      "complete_confirm": "Complete this session? Pending decisions will be recorded as deferred.",
      "cancel_confirm": "Cancel this session? Sessions with final decisions cannot be cancelled.",
```

Inside `"items"`, add:

```json
      "record": "Record decision",
```

Inside `"participants"`, add:

```json
      "select_member": "Select a member",
```

Inside `"request"`, add:

```json
      "submit": "Submit",
      "note_placeholder": "Summarize the change, risks and test evidence",
```

Add a new `"change"` object (sibling of `"request"`):

```json
    "change": {
      "submit": "Submit to TCB",
      "resubmit": "Submit again",
      "withdraw": "Withdraw"
    },
```

- [ ] **Step 2: Regenerate typed keys**

```bash
pnpm --filter @plane/i18n run generate:types
```

Expected: `packages/i18n/src/types/keys.generated.ts` now contains `review.all_statuses`, `review.queue.subject`, `review.change.submit`, etc.

- [ ] **Step 3: Translate into the other 19 locales**

For each locale directory under `packages/i18n/src/locales/` except `en` — `cs, de, es, fr, id, it, ja, ka-ge, ko, pl, pt-BR, ro, ru, sk, tr-TR, ua, vi-VN, zh-CN, zh-TW` — add the same keys to `review.json` with translations produced per the translate skill (placeholder/tag preservation, no translation of product terms `TCB`/`RCB`).

- [ ] **Step 4: Verify sync has no new review gaps**

```bash
pnpm --filter @plane/i18n run sync:check
```

Expected: the only missing/stale keys remain the pre-existing `auth.*` baseline (18 missing / 32 stale, same numbers as before this task). `review.*` must be fully translated in all locales.

- [ ] **Step 5: Commit**

```bash
git add packages/i18n
git commit -m "i18n(web): review board + session keys"
```

---

### Task 3: Web types + notification guards

**Files:**

- Modify: `packages/types/src/workspace-notifications.ts`
- Create: `apps/web/core/lib/review-notification.ts`
- Test: `apps/web/core/lib/review-notification.test.ts`

- [ ] **Step 1: Write the failing guard test**

Create `apps/web/core/lib/review-notification.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { TNotificationData } from "@plane/types";
// local imports
import { isReviewRequestNotification, isReviewSessionNotification } from "./review-notification";

describe("review notification guards", () => {
  it("detects review_request payloads", () => {
    const data = {
      review_request: {
        id: "req-1",
        board_type: "tcb",
        project_id: "p1",
        workspace_slug: "ws",
        status: "decided",
        subject_label: "CHG-1 Fix",
        session_id: "s1",
        session_title: "Weekly",
      },
    } as unknown as TNotificationData;
    expect(isReviewRequestNotification(data)).toBe(true);
    expect(isReviewSessionNotification(data)).toBe(false);
  });

  it("detects review_session payloads", () => {
    const data = {
      review_session: {
        id: "s1",
        board_type: "rcb",
        project_id: null,
        workspace_slug: "ws",
        title: "Release board",
        scheduled_at: "2026-10-05T09:00:00Z",
      },
    } as unknown as TNotificationData;
    expect(isReviewSessionNotification(data)).toBe(true);
    expect(isReviewRequestNotification(data)).toBe(false);
  });

  it("rejects payloads without an id", () => {
    expect(isReviewRequestNotification({ review_request: { id: "" } } as unknown as TNotificationData)).toBe(false);
    expect(isReviewSessionNotification(undefined)).toBe(false);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
pnpm --filter=web test -- review-notification
```

Expected: FAIL — module `./review-notification` not found.

- [ ] **Step 3: Add the notification payload types**

In `packages/types/src/workspace-notifications.ts`, add the import at the top:

```ts
import type { TReviewBoardType, TReviewOutcome } from "./review";
```

Add these types after `TNotificationWarRoom`:

```ts
export type TNotificationReviewRequest = {
  id: string;
  board_type: TReviewBoardType;
  project_id: string | null;
  workspace_slug: string;
  status: "pending" | "scheduled" | "decided";
  subject_label: string;
  session_id?: string;
  session_title?: string;
  scheduled_at?: string;
  outcome?: TReviewOutcome;
};

export type TNotificationReviewSession = {
  id: string;
  board_type: TReviewBoardType;
  project_id: string | null;
  workspace_slug: string;
  title: string;
  scheduled_at: string;
};
```

Extend `TNotificationData`:

```ts
  review_request?: TNotificationReviewRequest | undefined;
  review_session?: TNotificationReviewSession | undefined;
```

- [ ] **Step 4: Create the guards**

Create `apps/web/core/lib/review-notification.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TNotificationData, TNotificationReviewRequest, TNotificationReviewSession } from "@plane/types";

export const isReviewRequestNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { review_request: TNotificationReviewRequest } =>
  typeof data?.review_request?.id === "string" && data.review_request.id.length > 0;

export const isReviewSessionNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { review_session: TNotificationReviewSession } =>
  typeof data?.review_session?.id === "string" && data.review_session.id.length > 0;
```

- [ ] **Step 5: Rebuild the types package (web resolves `dist`)**

```bash
pnpm --filter=@plane/types build
```

- [ ] **Step 6: Run the guard test + typecheck**

```bash
pnpm --filter=web test -- review-notification
pnpm --filter=web check:types
```

Expected: tests PASS; 0 TS errors.

- [ ] **Step 7: Commit**

```bash
git add packages/types/src/workspace-notifications.ts packages/types/dist apps/web/core/lib/review-notification.ts apps/web/core/lib/review-notification.test.ts
git commit -m "feat(web): review notification payload types + guards"
```

---

### Task 4: Review helpers (datetime) + store session/agenda/participant state

**Files:**

- Modify: `apps/web/core/services/review.helpers.ts`
- Test: `apps/web/core/services/review.helpers.test.ts`
- Modify: `apps/web/core/store/review.store.ts` (full rewrite below)

- [ ] **Step 1: Write the failing datetime helper tests**

Append to `apps/web/core/services/review.helpers.test.ts` (and add the imports to the local import list):

```ts
import { fromDateTimeLocal, toDateTimeLocal } from "./review.helpers";

describe("toDateTimeLocal", () => {
  it("formats an ISO string for a datetime-local input", () => {
    const iso = new Date(2026, 9, 5, 9, 30).toISOString();
    expect(toDateTimeLocal(iso)).toBe("2026-10-05T09:30");
  });

  it("returns empty string for null/invalid input", () => {
    expect(toDateTimeLocal(null)).toBe("");
    expect(toDateTimeLocal("not-a-date")).toBe("");
  });
});

describe("fromDateTimeLocal", () => {
  it("converts a datetime-local value back to ISO", () => {
    const iso = fromDateTimeLocal("2026-10-05T09:30");
    expect(iso).not.toBeNull();
    expect(new Date(iso as string).getFullYear()).toBe(2026);
  });

  it("returns null for empty/invalid input", () => {
    expect(fromDateTimeLocal("")).toBeNull();
    expect(fromDateTimeLocal("nope")).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
pnpm --filter=web test -- review.helpers
```

Expected: FAIL — `toDateTimeLocal is not a function`.

- [ ] **Step 3: Implement the helpers**

In `apps/web/core/services/review.helpers.ts`, append:

```ts
const pad = (value: number) => String(value).padStart(2, "0");

export const toDateTimeLocal = (value: string | null | undefined): string => {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(
    date.getMinutes()
  )}`;
};

export const fromDateTimeLocal = (value: string): string | null => {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
};
```

- [ ] **Step 4: Run to verify it passes**

```bash
pnpm --filter=web test -- review.helpers
```

Expected: PASS.

- [ ] **Step 5: Extend the store with sessions, agenda items, participants**

Replace the whole `apps/web/core/store/review.store.ts` with:

```ts
import { set } from "lodash-es";
import { action, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
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
import { latestRequestForSubject, reviewRequestsKey, reviewSessionsKey } from "@/services/review.helpers";
import { ReviewService } from "@/services/review.service";
// store
import type { CoreRootStore } from "./root.store";

const sessionListKey = (params: TReviewSessionListParams) => `sessions:${reviewSessionsKey(params)}`;

export interface IReviewStore {
  loader: boolean;
  fetchedKeys: Record<string, boolean>;
  requestMap: Record<string, IReviewRequest>;
  requestIdsMap: Record<string, string[]>;
  requestDetailMap: Record<string, IReviewRequestDetail>;
  sessionMap: Record<string, IReviewSession>;
  sessionIdsMap: Record<string, string[]>;
  sessionDetailMap: Record<string, IReviewSessionDetail>;
  errorMap: Record<string, boolean>;
  getRequestById: (requestId: string) => IReviewRequest | null;
  getRequestIds: (params: TReviewRequestListParams) => string[] | null;
  getRequestDetailById: (requestId: string) => IReviewRequestDetail | null;
  getRequestsForChange: (changeIssueId: string) => IReviewRequest[];
  getRequestsForRelease: (releaseId: string) => IReviewRequest[];
  getLatestRequestForChange: (changeIssueId: string) => IReviewRequest | null;
  getLatestRequestForRelease: (releaseId: string) => IReviewRequest | null;
  getSessionById: (sessionId: string) => IReviewSession | null;
  getSessions: (params: TReviewSessionListParams) => IReviewSession[] | null;
  getSessionDetailById: (sessionId: string) => IReviewSessionDetail | null;
  fetchRequests: (workspaceSlug: string, params: TReviewRequestListParams) => Promise<IReviewRequest[] | undefined>;
  submitRequest: (workspaceSlug: string, data: TReviewRequestCreatePayload) => Promise<IReviewRequest>;
  withdrawRequest: (workspaceSlug: string, requestId: string) => Promise<IReviewRequest>;
  fetchRequestDetail: (workspaceSlug: string, requestId: string) => Promise<IReviewRequestDetail | undefined>;
  fetchSessions: (workspaceSlug: string, params: TReviewSessionListParams) => Promise<IReviewSession[] | undefined>;
  createSession: (workspaceSlug: string, data: TReviewSessionCreatePayload) => Promise<IReviewSession>;
  fetchSessionDetail: (workspaceSlug: string, sessionId: string) => Promise<IReviewSessionDetail | undefined>;
  updateSession: (
    workspaceSlug: string,
    sessionId: string,
    data: TReviewSessionUpdatePayload
  ) => Promise<IReviewSession>;
  completeSession: (workspaceSlug: string, sessionId: string) => Promise<IReviewSession>;
  cancelSession: (workspaceSlug: string, sessionId: string) => Promise<IReviewSession>;
  addSessionItems: (workspaceSlug: string, sessionId: string, requestIds: string[]) => Promise<IReviewSessionItem[]>;
  updateSessionItem: (
    workspaceSlug: string,
    sessionId: string,
    itemId: string,
    data: TReviewItemUpdatePayload
  ) => Promise<IReviewSessionItem>;
  removeSessionItem: (workspaceSlug: string, sessionId: string, itemId: string) => Promise<void>;
  addSessionParticipant: (
    workspaceSlug: string,
    sessionId: string,
    data: TReviewParticipantCreatePayload
  ) => Promise<IReviewParticipant>;
  updateSessionParticipant: (
    workspaceSlug: string,
    sessionId: string,
    participantId: string,
    data: TReviewParticipantUpdatePayload
  ) => Promise<IReviewParticipant>;
  removeSessionParticipant: (workspaceSlug: string, sessionId: string, participantId: string) => Promise<void>;
}

export class ReviewStore implements IReviewStore {
  loader: boolean = false;
  fetchedKeys: Record<string, boolean> = {};
  requestMap: Record<string, IReviewRequest> = {};
  requestIdsMap: Record<string, string[]> = {};
  requestDetailMap: Record<string, IReviewRequestDetail> = {};
  sessionMap: Record<string, IReviewSession> = {};
  sessionIdsMap: Record<string, string[]> = {};
  sessionDetailMap: Record<string, IReviewSessionDetail> = {};
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
      sessionMap: observable,
      sessionIdsMap: observable,
      sessionDetailMap: observable,
      errorMap: observable,
      fetchRequests: action,
      submitRequest: action,
      withdrawRequest: action,
      fetchRequestDetail: action,
      fetchSessions: action,
      createSession: action,
      fetchSessionDetail: action,
      updateSession: action,
      completeSession: action,
      cancelSession: action,
      addSessionItems: action,
      updateSessionItem: action,
      removeSessionItem: action,
      addSessionParticipant: action,
      updateSessionParticipant: action,
      removeSessionParticipant: action,
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

  getSessionById = computedFn((sessionId: string) => this.sessionMap[sessionId] ?? null);

  getSessions = computedFn((params: TReviewSessionListParams) => {
    const key = sessionListKey(params);
    if (!this.fetchedKeys[key]) return null;
    return (this.sessionIdsMap[key] ?? [])
      .map((sessionId) => this.sessionMap[sessionId])
      .filter((session): session is IReviewSession => Boolean(session));
  });

  getSessionDetailById = computedFn((sessionId: string) => this.sessionDetailMap[sessionId] ?? null);

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

  fetchSessions = async (workspaceSlug: string, params: TReviewSessionListParams) => {
    const key = sessionListKey(params);
    try {
      runInAction(() => {
        set(this.errorMap, key, false);
      });
      this.loader = true;
      const sessions = await this.reviewService.getReviewSessions(workspaceSlug, params);
      runInAction(() => {
        sessions.forEach((session) => set(this.sessionMap, [session.id], session));
        set(
          this.sessionIdsMap,
          [key],
          sessions.map((session) => session.id)
        );
        set(this.fetchedKeys, [key], true);
        this.loader = false;
      });
      return sessions;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, [key], true);
      });
      return undefined;
    }
  };

  createSession = async (workspaceSlug: string, data: TReviewSessionCreatePayload) => {
    const session = await this.reviewService.createReviewSession(workspaceSlug, data);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
    });
    return session;
  };

  fetchSessionDetail = async (workspaceSlug: string, sessionId: string) => {
    try {
      const session = await this.reviewService.getReviewSession(workspaceSlug, sessionId);
      runInAction(() => {
        set(this.sessionMap, [session.id], session);
        set(this.sessionDetailMap, [sessionId], session);
      });
      return session;
    } catch {
      return undefined;
    }
  };

  updateSession = async (workspaceSlug: string, sessionId: string, data: TReviewSessionUpdatePayload) => {
    const session = await this.reviewService.updateReviewSession(workspaceSlug, sessionId, data);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
      const detail = this.sessionDetailMap[sessionId];
      if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, ...session });
    });
    return session;
  };

  completeSession = async (workspaceSlug: string, sessionId: string) => {
    const session = await this.reviewService.completeReviewSession(workspaceSlug, sessionId);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
      const detail = this.sessionDetailMap[sessionId];
      if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, ...session });
    });
    return session;
  };

  cancelSession = async (workspaceSlug: string, sessionId: string) => {
    const session = await this.reviewService.cancelReviewSession(workspaceSlug, sessionId);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
      const detail = this.sessionDetailMap[sessionId];
      if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, ...session });
    });
    return session;
  };

  addSessionItems = async (workspaceSlug: string, sessionId: string, requestIds: string[]) => {
    const items = await this.reviewService.addSessionItems(workspaceSlug, sessionId, requestIds);
    runInAction(() => {
      items.forEach((item) =>
        set(this.requestMap, [item.review_request_id], {
          ...(this.requestMap[item.review_request_id] ?? {}),
          status: item.request_status,
        } as IReviewRequest)
      );
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        const existing = new Set(detail.items.map((item) => item.id));
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          items: [...detail.items, ...items.filter((item) => !existing.has(item.id))],
        });
      }
    });
    return items;
  };

  updateSessionItem = async (
    workspaceSlug: string,
    sessionId: string,
    itemId: string,
    data: TReviewItemUpdatePayload
  ) => {
    const item = await this.reviewService.updateSessionItem(workspaceSlug, sessionId, itemId, data);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          items: detail.items.map((existing) => (existing.id === item.id ? item : existing)),
        });
      }
    });
    return item;
  };

  removeSessionItem = async (workspaceSlug: string, sessionId: string, itemId: string) => {
    await this.reviewService.removeSessionItem(workspaceSlug, sessionId, itemId);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          items: detail.items.filter((item) => item.id !== itemId),
        });
      }
    });
  };

  addSessionParticipant = async (workspaceSlug: string, sessionId: string, data: TReviewParticipantCreatePayload) => {
    const participant = await this.reviewService.addSessionParticipant(workspaceSlug, sessionId, data);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        const others = detail.participants.filter((existing) => existing.id !== participant.id);
        set(this.sessionDetailMap, [sessionId], { ...detail, participants: [...others, participant] });
      }
    });
    return participant;
  };

  updateSessionParticipant = async (
    workspaceSlug: string,
    sessionId: string,
    participantId: string,
    data: TReviewParticipantUpdatePayload
  ) => {
    const participant = await this.reviewService.updateSessionParticipant(
      workspaceSlug,
      sessionId,
      participantId,
      data
    );
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          participants: detail.participants.map((existing) =>
            existing.id === participant.id ? participant : existing
          ),
        });
      }
    });
    return participant;
  };

  removeSessionParticipant = async (workspaceSlug: string, sessionId: string, participantId: string) => {
    await this.reviewService.removeSessionParticipant(workspaceSlug, sessionId, participantId);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          participants: detail.participants.filter((participant) => participant.id !== participantId),
        });
      }
    });
  };
}
```

Note: `addSessionItems` writes a partial request (`{ status }`) into `requestMap` — acceptable because the queue will be refetched; if TypeScript complains, cast as `IReviewRequest` via `{ ...(this.requestMap[item.review_request_id] ?? {}), status: item.request_status } as IReviewRequest`. Prefer that form:

```ts
items.forEach((item) =>
  set(this.requestMap, [item.review_request_id], {
    ...(this.requestMap[item.review_request_id] ?? {}),
    status: item.request_status,
  } as IReviewRequest)
);
```

- [ ] **Step 6: Typecheck + tests**

```bash
pnpm --filter=web check:types
pnpm --filter=web test -- review
```

Expected: 0 TS errors; all review tests PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/services/review.helpers.ts apps/web/core/services/review.helpers.test.ts apps/web/core/store/review.store.ts
git commit -m "feat(web): review store sessions + agenda + participants"
```

---

### Task 5: Shared Submit-to-review modal (refactor release detail to use it)

**Files:**

- Create: `apps/web/core/components/reviews/submit-review-modal.tsx`
- Modify: `apps/web/core/components/reviews/index.ts`
- Modify: `apps/web/core/components/releases/detail/review-history.tsx`
- Delete: `apps/web/core/components/releases/detail/submit-review-modal.tsx`
- Modify: `apps/web/core/components/releases/detail/index.ts`

- [ ] **Step 1: Create the shared modal**

Create `apps/web/core/components/reviews/submit-review-modal.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewRequest, TReviewBoardType, TReviewRequestCreatePayload } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  boardType: TReviewBoardType;
  subjectId: string;
  title: string;
  submitLabel: string;
  onSubmitted?: (request: IReviewRequest) => void | Promise<void>;
};

export const SubmitReviewModal = observer(function SubmitReviewModal({
  isOpen,
  onClose,
  workspaceSlug,
  boardType,
  subjectId,
  title,
  submitLabel,
  onSubmitted,
}: Props) {
  const { t } = useTranslation();
  const { submitRequest } = useReview();
  const [note, setNote] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    setIsSubmitting(true);
    try {
      const payload: TReviewRequestCreatePayload =
        boardType === "rcb"
          ? { board_type: "rcb", release_id: subjectId, submission_note: note.trim() }
          : { board_type: "tcb", change_issue_id: subjectId, submission_note: note.trim() };
      const request = await submitRequest(workspaceSlug, payload);
      if (onSubmitted) await onSubmitted(request);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.request.submitted") });
      setNote("");
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.request.submit_failed"),
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{title}</h3>
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("review.request.submission_note")}</label>
          <TextArea
            value={note}
            onChange={(event) => setNote(event.target.value)}
            placeholder={t("review.request.note_placeholder")}
            rows={4}
          />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isSubmitting} onClick={() => void handleSubmit()}>
            {submitLabel}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

- [ ] **Step 2: Export it**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./submit-review-modal";
```

- [ ] **Step 3: Refactor the release history to use it**

In `apps/web/core/components/releases/detail/review-history.tsx`:

1. Replace the local modal import and add `useRelease`:

```tsx
// components
import { ReviewOutcomePill, ReviewRequestStatusPill, SubmitReviewModal } from "@/components/reviews";
// hooks
import { useRelease } from "@/hooks/store/use-release";
import { useReview } from "@/hooks/store/use-review";
```

2. Inside the component add the store:

```tsx
const { updateRelease } = useRelease();
```

3. Replace the render of `<SubmitReleaseReviewModal ... />` at the bottom with:

```tsx
<SubmitReviewModal
  isOpen={isSubmitOpen}
  onClose={() => setIsSubmitOpen(false)}
  workspaceSlug={workspaceSlug}
  boardType="rcb"
  subjectId={releaseId}
  title={t("release.review.submit")}
  submitLabel={t("release.review.submit")}
  onSubmitted={async () => {
    await updateRelease(workspaceSlug, releaseId, { status: "in_review" });
  }}
/>
```

4. Delete the old modal import line (`import { SubmitReleaseReviewModal } from "./submit-review-modal";`).

- [ ] **Step 4: Delete the old release-specific modal + export**

```bash
rm apps/web/core/components/releases/detail/submit-review-modal.tsx
```

In `apps/web/core/components/releases/detail/index.ts`, remove the line `export * from "./submit-review-modal";`.

- [ ] **Step 5: Typecheck + lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: 0 errors.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/reviews apps/web/core/components/releases/detail
git commit -m "refactor(web): shared submit-to-review modal"
```

---

### Task 6: Shared board components (tabs + queue + sessions)

**Files:**

- Create: `apps/web/core/components/reviews/session-status-pill.tsx`
- Create: `apps/web/core/components/reviews/board/review-request-row.tsx`
- Create: `apps/web/core/components/reviews/board/review-queue-board.tsx`
- Create: `apps/web/core/components/reviews/board/review-session-row.tsx`
- Create: `apps/web/core/components/reviews/board/review-sessions-board.tsx`
- Create: `apps/web/core/components/reviews/board/session-form-modal.tsx`
- Create: `apps/web/core/components/reviews/board/review-board-root.tsx`
- Modify: `apps/web/core/components/reviews/index.ts`

- [ ] **Step 1: Session status pill**

Create `apps/web/core/components/reviews/session-status-pill.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { REVIEW_SESSION_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewSessionStatus } from "@plane/types";
import { cn } from "@plane/utils";

type Props = {
  status: TReviewSessionStatus;
  className?: string;
};

export const ReviewSessionStatusPill = observer(function ReviewSessionStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  const config = REVIEW_SESSION_STATUS_CONFIG[status];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
```

- [ ] **Step 2: Queue row**

Create `apps/web/core/components/reviews/board/review-request-row.tsx`:

```tsx
import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { getReleaseLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { renderFormattedDate } from "@plane/utils";
// components
import { ReviewRequestStatusPill } from "../request-status-pill";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  requestId: string;
};

export const ReviewRequestRow = observer(function ReviewRequestRow({ workspaceSlug, requestId }: Props) {
  const { t } = useTranslation();
  const { getRequestById } = useReview();

  const request = getRequestById(requestId);
  if (!request) return null;

  const issue = request.subject.issue;
  const release = request.subject.release;
  const subjectLabel =
    request.board_type === "tcb" ? [issue?.identifier, issue?.name].filter(Boolean).join(" ") : (release?.name ?? "");
  const href =
    request.board_type === "tcb" && issue?.identifier
      ? `/${workspaceSlug}/browse/${issue.identifier}`
      : request.release_id
        ? getReleaseLink(workspaceSlug, request.release_id)
        : null;

  const body = (
    <>
      <span className="w-16 shrink-0 font-code text-11 text-tertiary">
        {request.board_type === "tcb" ? (issue?.identifier ?? "—") : `REL-${release?.sequence_id ?? "—"}`}
      </span>
      <span className="min-w-0 flex-1 truncate text-13 font-medium text-primary">
        {subjectLabel || t("review.queue.subject")}
      </span>
      <span className="flex w-28 justify-center">
        <ReviewRequestStatusPill status={request.status} />
      </span>
      <span className="w-40 truncate text-12 text-secondary">{request.session?.title ?? "—"}</span>
      <span className="w-28 text-right text-12 text-secondary">{renderFormattedDate(request.submitted_at)}</span>
    </>
  );

  if (!href) {
    return <div className="flex items-center gap-3 border-b border-subtle px-4 py-3">{body}</div>;
  }

  return (
    <Link href={href} className="group flex items-center gap-3 border-b border-subtle px-4 py-3 hover:bg-layer-1">
      {body}
    </Link>
  );
});
```

- [ ] **Step 3: Queue board**

Create `apps/web/core/components/reviews/board/review-queue-board.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { REVIEW_REQUEST_STATUS_CONFIG, REVIEW_REQUEST_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewBoardType, TReviewRequestStatus } from "@plane/types";
import { CustomSelect } from "@plane/ui";
// components
import { ReviewRequestRow } from "./review-request-row";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
};

export const ReviewQueueBoard = observer(function ReviewQueueBoard({ workspaceSlug, boardType, projectId }: Props) {
  const { t } = useTranslation();
  const { getRequestById, getRequestIds, fetchRequests, errorMap } = useReview();
  const [status, setStatus] = useState("");

  const params = { board_type: boardType, project_id: projectId };
  const fetched = getRequestIds(params);
  const hasError = errorMap[`${boardType}:${projectId ?? "ws"}:all:-:-`] ?? false;

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: boardType, project_id: projectId });
  }, [fetched, fetchRequests, boardType, projectId, workspaceSlug]);

  const requestIds =
    fetched?.filter((requestId) => {
      const request = getRequestById(requestId);
      return Boolean(request) && (!status || request?.status === status);
    }) ?? [];

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-end gap-2 border-b border-subtle px-4 py-2">
        <CustomSelect
          value={status}
          onChange={setStatus}
          label={
            status
              ? t(REVIEW_REQUEST_STATUS_CONFIG[status as TReviewRequestStatus].label_key)
              : t("review.all_statuses")
          }
        >
          <CustomSelect.Option value="">{t("review.all_statuses")}</CustomSelect.Option>
          {REVIEW_REQUEST_STATUSES.map((value) => (
            <CustomSelect.Option key={value} value={value}>
              {t(REVIEW_REQUEST_STATUS_CONFIG[value].label_key)}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto">
        <div className="sticky top-0 z-10 flex items-center gap-3 border-b border-subtle bg-surface-1 px-4 py-2 text-11 font-medium text-tertiary">
          <span className="w-16 shrink-0">&nbsp;</span>
          <span className="flex-1">{t("review.queue.subject")}</span>
          <span className="w-28 text-center">{t("review.queue.status")}</span>
          <span className="w-40">{t("review.queue.session")}</span>
          <span className="w-28 text-right">{t("review.queue.submitted_at")}</span>
        </div>
        {hasError ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.load_error.title")}</p>
            <p className="text-xs text-secondary">{t("review.load_error.description")}</p>
            <button
              type="button"
              className="text-12 text-accent-primary underline"
              onClick={() => void fetchRequests(workspaceSlug, params)}
            >
              {t("review.load_error.retry")}
            </button>
          </div>
        ) : fetched === null ? (
          <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>
        ) : fetched.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.empty_queue.title")}</p>
            <p className="text-xs text-secondary">{t("review.empty_queue.description")}</p>
          </div>
        ) : requestIds.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.no_matches.title")}</p>
            <p className="text-xs text-secondary">{t("review.no_matches.description")}</p>
          </div>
        ) : (
          requestIds.map((requestId) => (
            <ReviewRequestRow key={requestId} workspaceSlug={workspaceSlug} requestId={requestId} />
          ))
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 4: Session row + sessions board**

Create `apps/web/core/components/reviews/board/review-session-row.tsx`:

```tsx
import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { getReviewSessionLink } from "@plane/constants";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// components
import { ReviewSessionStatusPill } from "../session-status-pill";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  sessionId: string;
};

export const ReviewSessionRow = observer(function ReviewSessionRow({ workspaceSlug, sessionId }: Props) {
  const { getSessionById } = useReview();

  const session = getSessionById(sessionId);
  if (!session) return null;

  return (
    <Link
      href={getReviewSessionLink(workspaceSlug, session.id, session.board_type, session.project_id ?? undefined)}
      className="group flex items-center gap-3 border-b border-subtle px-4 py-3 hover:bg-layer-1"
    >
      <span className="min-w-0 flex-1 truncate text-13 font-medium text-primary">{session.title}</span>
      <span className="flex w-28 justify-center">
        <ReviewSessionStatusPill status={session.status} />
      </span>
      <span className="w-40 text-12 text-secondary">
        {renderFormattedDate(session.scheduled_at)} {renderFormattedTime(session.scheduled_at)}
      </span>
      <span className="w-24 text-center text-12 text-secondary">{session.counts?.items ?? 0}</span>
      <span className="w-24 text-center text-12 text-secondary">{session.counts?.participants ?? 0}</span>
    </Link>
  );
});
```

Note: `session.counts` — the list endpoint returns counts per Plan 2; if a session item lacks `counts`, guard with `session.counts?.items ?? 0`. Use the guarded form in both cells.

Create `apps/web/core/components/reviews/board/review-sessions-board.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TReviewBoardType } from "@plane/types";
// components
import { ReviewSessionRow } from "./review-session-row";
import { SessionFormModal } from "./session-form-modal";
// helpers
import { sortSessionsByScheduledAt } from "@/services/review.helpers";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
};

export const ReviewSessionsBoard = observer(function ReviewSessionsBoard({
  workspaceSlug,
  boardType,
  projectId,
}: Props) {
  const { t } = useTranslation();
  const { getSessions, fetchSessions, errorMap } = useReview();
  const { allowPermissions } = useUserPermissions();
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  const params = { board_type: boardType, project_id: projectId };
  const sessions = getSessions(params);
  const hasError = errorMap[`sessions:${boardType}:${projectId ?? "ws"}:all`] ?? false;
  const canCreate =
    boardType === "rcb"
      ? allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          workspaceSlug,
          projectId ?? ""
        );

  useEffect(() => {
    if (sessions !== null) return;
    void fetchSessions(workspaceSlug, { board_type: boardType, project_id: projectId });
  }, [sessions, fetchSessions, boardType, projectId, workspaceSlug]);

  const ordered = sessions ? sortSessionsByScheduledAt(sessions) : null;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-end border-b border-subtle px-4 py-2">
        {canCreate && (
          <Button variant="primary" size="sm" onClick={() => setIsCreateOpen(true)}>
            {t("review.sessions.add")}
          </Button>
        )}
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto">
        <div className="sticky top-0 z-10 flex items-center gap-3 border-b border-subtle bg-surface-1 px-4 py-2 text-11 font-medium text-tertiary">
          <span className="flex-1">{t("review.sessions.title_label")}</span>
          <span className="w-28 text-center">{t("review.sessions.status")}</span>
          <span className="w-40">{t("review.sessions.scheduled_at")}</span>
          <span className="w-24 text-center">{t("review.sessions.items_count")}</span>
          <span className="w-24 text-center">{t("review.sessions.participants_count")}</span>
        </div>
        {hasError ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.load_error.title")}</p>
            <p className="text-xs text-secondary">{t("review.load_error.description")}</p>
            <button
              type="button"
              className="text-12 text-accent-primary underline"
              onClick={() => void fetchSessions(workspaceSlug, params)}
            >
              {t("review.load_error.retry")}
            </button>
          </div>
        ) : ordered === null ? (
          <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>
        ) : ordered.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.empty_sessions.title")}</p>
            <p className="text-xs text-secondary">{t("review.empty_sessions.description")}</p>
          </div>
        ) : (
          ordered.map((session) => (
            <ReviewSessionRow key={session.id} workspaceSlug={workspaceSlug} sessionId={session.id} />
          ))
        )}
      </div>
      <SessionFormModal
        isOpen={isCreateOpen}
        onClose={() => setIsCreateOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType={boardType}
        projectId={projectId}
        onSaved={async () => {
          await fetchSessions(workspaceSlug, params);
        }}
      />
    </div>
  );
});
```

Unused import check: `TOAST_TYPE, setToast` are NOT used here — remove them from the import list in the final file. (They are used in `session-form-modal.tsx`.)

- [ ] **Step 5: Session create/edit modal**

Create `apps/web/core/components/reviews/board/session-form-modal.tsx`:

```tsx
import { useEffect } from "react";
import { Controller, useForm } from "react-hook-form";
import { observer } from "mobx-react";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSession, TReviewBoardType } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// helpers
import { fromDateTimeLocal, toDateTimeLocal } from "@/services/review.helpers";
// hooks
import { useReview } from "@/hooks/store/use-review";

type FormValues = {
  title: string;
  scheduled_at: string;
  location: string;
};

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
  data?: IReviewSession;
  onSaved?: () => void | Promise<void>;
};

export const SessionFormModal = observer(function SessionFormModal({
  isOpen,
  onClose,
  workspaceSlug,
  boardType,
  projectId,
  data,
  onSaved,
}: Props) {
  const { t } = useTranslation();
  const { createSession, updateSession } = useReview();
  const {
    formState: { errors, isSubmitting },
    handleSubmit,
    control,
    reset,
  } = useForm<FormValues>({
    defaultValues: { title: "", scheduled_at: "", location: "" },
  });

  useEffect(() => {
    reset({
      title: data?.title ?? "",
      scheduled_at: toDateTimeLocal(data?.scheduled_at),
      location: data?.location ?? "",
    });
  }, [data, reset, isOpen]);

  const handleFormSubmit = async (values: FormValues) => {
    const scheduledAt = fromDateTimeLocal(values.scheduled_at);
    if (!scheduledAt) return;
    try {
      if (data) {
        await updateSession(workspaceSlug, data.id, {
          title: values.title.trim(),
          scheduled_at: scheduledAt,
          location: values.location.trim() || null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      } else {
        await createSession(workspaceSlug, {
          board_type: boardType,
          project_id: projectId ?? null,
          title: values.title.trim(),
          scheduled_at: scheduledAt,
          location: values.location.trim() || null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.created") });
      }
      if (onSaved) await onSaved();
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          apiError?.detail ??
          apiError?.error ??
          (data ? t("review.sessions.update_failed") : t("review.sessions.create_failed")),
      });
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <form onSubmit={handleSubmit(handleFormSubmit)}>
        <div className="space-y-5 p-5">
          <div>
            <Controller
              control={control}
              name="title"
              rules={{
                required: t("title_is_required"),
                maxLength: { value: 255, message: t("title_should_be_less_than_255_characters") },
              }}
              render={({ field: { value, onChange } }) => (
                <Field name="title" invalid={Boolean(errors?.title)}>
                  <InputGroup size="2xl">
                    <Input
                      size="2xl"
                      id="title"
                      name="title"
                      type="text"
                      value={value}
                      onChange={onChange}
                      placeholder={t("review.sessions.title_label")}
                    />
                  </InputGroup>
                </Field>
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.title?.message}</span>
          </div>
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("review.sessions.scheduled_at")}</label>
            <Controller
              control={control}
              name="scheduled_at"
              rules={{ required: t("review.sessions.scheduled_at") }}
              render={({ field: { value, onChange } }) => (
                <Input
                  size="2xl"
                  id="scheduled_at"
                  name="scheduled_at"
                  type="datetime-local"
                  value={value}
                  onChange={onChange}
                />
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.scheduled_at?.message}</span>
          </div>
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("review.sessions.location")}</label>
            <Controller
              control={control}
              name="location"
              render={({ field: { value, onChange } }) => (
                <Input
                  size="2xl"
                  id="location"
                  name="location"
                  type="text"
                  value={value}
                  onChange={onChange}
                  placeholder={t("review.sessions.location")}
                />
              )}
            />
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" type="submit" loading={isSubmitting}>
            {data ? t("save") : t("review.sessions.add")}
          </Button>
        </div>
      </form>
    </ModalCore>
  );
});
```

- [ ] **Step 6: Board root**

Create `apps/web/core/components/reviews/board/review-board-root.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TReviewBoardType } from "@plane/types";
import { cn } from "@plane/utils";
// components
import { ReviewQueueBoard } from "./review-queue-board";
import { ReviewSessionsBoard } from "./review-sessions-board";

type Props = {
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
};

export const ReviewBoardRoot = observer(function ReviewBoardRoot({ workspaceSlug, boardType, projectId }: Props) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"queue" | "sessions">("queue");

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <div className="flex items-center gap-4 border-b border-subtle px-4 pt-2 text-13">
        {(["queue", "sessions"] as const).map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => setTab(key)}
            className={cn(
              "border-b-2 pb-2 font-medium",
              tab === key
                ? "border-accent-primary text-primary"
                : "border-transparent text-secondary hover:text-primary"
            )}
          >
            {key === "queue" ? t("review.queue_tab") : t("review.sessions_tab")}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        {tab === "queue" ? (
          <ReviewQueueBoard workspaceSlug={workspaceSlug} boardType={boardType} projectId={projectId} />
        ) : (
          <ReviewSessionsBoard workspaceSlug={workspaceSlug} boardType={boardType} projectId={projectId} />
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 7: Exports**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./board/review-board-root";
export * from "./session-status-pill";
```

- [ ] **Step 8: Typecheck + lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: 0 errors. Fix the `hasError` key mismatch if the `reviewRequestsKey` shape differs — the canonical keys are built by `reviewRequestsKey`/`reviewSessionsKey` in `apps/web/core/services/review.helpers.ts`; verify the literals in `ReviewQueueBoard`/`ReviewSessionsBoard` match (`board:project:status:change:release`).

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/components/reviews
git commit -m "feat(web): shared review board components"
```

---

### Task 7: Session detail — root, header, minutes, complete/cancel

**Files:**

- Create: `apps/web/core/components/reviews/session/review-session-detail-root.tsx`
- Create: `apps/web/core/components/reviews/session/session-detail-header.tsx`
- Create: `apps/web/core/components/reviews/session/session-minutes.tsx`
- Modify: `apps/web/core/components/reviews/index.ts`

- [ ] **Step 1: Session minutes**

Create `apps/web/core/components/reviews/session/session-minutes.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail } from "@plane/types";
import { TextArea } from "@plane/ui";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionMinutes = observer(function SessionMinutes({ workspaceSlug, session, canManage }: Props) {
  const { t } = useTranslation();
  const { updateSession } = useReview();
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [value, setValue] = useState(session.minutes ?? "");

  useEffect(() => {
    setValue(session.minutes ?? "");
  }, [session.id, session.minutes]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await updateSession(workspaceSlug, session.id, { minutes: value });
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      setIsEditing(false);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.sessions.update_failed"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("review.sessions.minutes")}</h3>
        {canManage && session.status !== "cancelled" && !isEditing && (
          <Button variant="secondary" size="sm" onClick={() => setIsEditing(true)}>
            {t("edit")}
          </Button>
        )}
      </div>
      {isEditing ? (
        <div className="space-y-2">
          <TextArea
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder={t("review.sessions.minutes_placeholder")}
            rows={6}
          />
          <div className="flex items-center justify-end gap-2">
            <Button variant="secondary" size="sm" onClick={() => setIsEditing(false)}>
              {t("cancel")}
            </Button>
            <Button variant="primary" size="sm" loading={isSaving} onClick={() => void handleSave()}>
              {t("save")}
            </Button>
          </div>
        </div>
      ) : session.minutes ? (
        <p className="rounded-md border border-subtle px-3 py-2 text-12 whitespace-pre-wrap text-secondary">
          {session.minutes}
        </p>
      ) : (
        <p className="text-12 text-secondary">{t("review.sessions.minutes_placeholder")}</p>
      )}
    </section>
  );
});
```

- [ ] **Step 2: Header with complete/cancel**

Create `apps/web/core/components/reviews/session/session-detail-header.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail } from "@plane/types";
import { AlertModalCore, Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { ReviewSessionStatusPill } from "../session-status-pill";
import { SessionFormModal } from "../board/session-form-modal";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  boardLabel: string;
  boardHref: string;
  canManage: boolean;
};

export const SessionDetailHeader = observer(function SessionDetailHeader({
  workspaceSlug,
  session,
  boardLabel,
  boardHref,
  canManage,
}: Props) {
  const { t } = useTranslation();
  const { completeSession, cancelSession, fetchSessionDetail } = useReview();
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [confirmAction, setConfirmAction] = useState<"complete" | "cancel" | null>(null);

  const handleConfirm = async () => {
    if (!confirmAction) return;
    try {
      if (confirmAction === "complete") {
        await completeSession(workspaceSlug, session.id);
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.completed") });
      } else {
        await cancelSession(workspaceSlug, session.id);
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.cancelled") });
      }
      await fetchSessionDetail(workspaceSlug, session.id);
      setConfirmAction(null);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          apiError?.detail ??
          apiError?.error ??
          (confirmAction === "complete" ? t("review.sessions.complete_failed") : t("review.sessions.cancel_failed")),
      });
    }
  };

  return (
    <>
      <Header>
        <Header.LeftItem>
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={<BreadcrumbLink label={boardLabel} href={boardHref} isLast={false} />}
              isLast={false}
            />
            <Breadcrumbs.Item component={<BreadcrumbLink label={session.title} href="#" isLast />} isLast />
          </Breadcrumbs>
        </Header.LeftItem>
        <Header.RightItem>
          {canManage && (
            <div className="flex items-center gap-2">
              <Button variant="secondary" size="lg" onClick={() => setIsEditOpen(true)}>
                {t("review.sessions.edit")}
              </Button>
              {session.status === "scheduled" && (
                <>
                  <Button variant="secondary" size="lg" onClick={() => setConfirmAction("complete")}>
                    {t("review.sessions.complete")}
                  </Button>
                  <Button
                    variant="secondary"
                    size="lg"
                    className="text-danger-primary"
                    onClick={() => setConfirmAction("cancel")}
                  >
                    {t("review.sessions.cancel")}
                  </Button>
                </>
              )}
            </div>
          )}
        </Header.RightItem>
      </Header>
      <SessionFormModal
        isOpen={isEditOpen}
        onClose={() => setIsEditOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType={session.board_type}
        projectId={session.project_id ?? undefined}
        data={session}
        onSaved={async () => {
          await fetchSessionDetail(workspaceSlug, session.id);
        }}
      />
      <AlertModalCore
        isOpen={confirmAction !== null}
        handleClose={() => setConfirmAction(null)}
        handleSubmit={() => void handleConfirm()}
        isSubmitting={false}
        title={confirmAction === "complete" ? t("review.sessions.complete") : t("review.sessions.cancel")}
        content={
          confirmAction === "complete" ? t("review.sessions.complete_confirm") : t("review.sessions.cancel_confirm")
        }
      />
    </>
  );
});
```

Remove the unused `GRID_COLUMNS` import and the unused `renderFormattedDate/renderFormattedTime` imports from the final file — they belong on the page/root, not this header. (Already applied above: the header shows no date itself; the root renders the schedule line.)

- [ ] **Step 3: Detail root**

Create `apps/web/core/components/reviews/session/review-session-detail-root.tsx`:

```tsx
import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import {
  EUserPermissions,
  EUserPermissionsLevel,
  getReleaseControlLink,
  getTestingControlLink,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// components
import { ReviewSessionStatusPill } from "../session-status-pill";
import { SessionAgenda } from "./session-agenda";
import { SessionMinutes } from "./session-minutes";
import { SessionParticipants } from "./session-participants";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  sessionId: string;
};

export const ReviewSessionDetailRoot = observer(function ReviewSessionDetailRoot({ workspaceSlug, sessionId }: Props) {
  const { t } = useTranslation();
  const { getSessionDetailById, fetchSessionDetail } = useReview();
  const { allowPermissions } = useUserPermissions();

  const session = getSessionDetailById(sessionId);

  useEffect(() => {
    if (session) return;
    void fetchSessionDetail(workspaceSlug, sessionId);
  }, [session, fetchSessionDetail, sessionId, workspaceSlug]);

  if (!session) {
    return <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>;
  }

  const canManage =
    session.board_type === "rcb"
      ? allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          workspaceSlug,
          session.project_id ?? ""
        );
  const boardLabel = t(session.board_type === "rcb" ? "review.board_values.rcb" : "review.board_values.tcb");
  const boardHref =
    session.board_type === "rcb"
      ? getReleaseControlLink(workspaceSlug)
      : getTestingControlLink(workspaceSlug, session.project_id ?? "");

  return (
    <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
      <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-16 font-semibold text-primary">{session.title}</h1>
            <ReviewSessionStatusPill status={session.status} />
          </div>
          <p className="mt-1 text-12 text-secondary">
            {renderFormattedDate(session.scheduled_at)} {renderFormattedTime(session.scheduled_at)}
            {session.location ? ` · ${session.location}` : ""}
          </p>
        </div>
        <SessionAgenda workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
        <SessionMinutes workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
      </div>
      <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
        <SessionParticipants workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
      </div>
    </div>
  );
});
```

The `ReviewSessionStatusPill` import is already at the top of the file (Step 1 of this task's correction). Do not add wrapper components or imports after declarations.

- [ ] **Step 4: The layout owns the header — the root does not mount it**

The root is mounted by the detail **page**, while the detail **layout** renders `<SessionDetailBreadcrumbs />` in the `AppHeader` (Task 10). `SessionDetailBreadcrumbs` reads the store and renders `SessionDetailHeader`. Confirm the root imports only: `ReviewSessionStatusPill`, `SessionAgenda`, `SessionMinutes`, `SessionParticipants`, store/perms hooks, constants, utils.

- [ ] **Step 5: Exports**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./session/review-session-detail-root";
export * from "./session/session-detail-header";
```

- [ ] **Step 6: Typecheck + lint (will fail until Tasks 8-9 land)**

`SessionAgenda` and `SessionParticipants` are created in Tasks 8 and 9. Run typecheck after those tasks; for now verify the two files above compile in isolation by temporarily stubbing? Do NOT stub. Complete Tasks 8 and 9 first, then run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

- [ ] **Step 7: Commit after Tasks 8 and 9 (single commit for the session detail stack)**

---

### Task 8: Session detail — agenda (items, add-to-agenda, record decision)

**Files:**

- Create: `apps/web/core/components/reviews/session/session-agenda.tsx`
- Create: `apps/web/core/components/reviews/session/add-agenda-items-modal.tsx`
- Create: `apps/web/core/components/reviews/session/record-outcome-modal.tsx`

- [ ] **Step 1: Record decision modal**

Create `apps/web/core/components/reviews/session/record-outcome-modal.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { REVIEW_OUTCOME_CONFIG, REVIEW_OUTCOMES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionItem, TReviewOutcome } from "@plane/types";
import { CustomSelect, EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  sessionId: string;
  item: IReviewSessionItem | null;
  onSaved?: () => void | Promise<void>;
};

export const RecordOutcomeModal = observer(function RecordOutcomeModal({
  isOpen,
  onClose,
  workspaceSlug,
  sessionId,
  item,
  onSaved,
}: Props) {
  const { t } = useTranslation();
  const { updateSessionItem } = useReview();
  const [outcome, setOutcome] = useState<TReviewOutcome>("approved");
  const [note, setNote] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (!item) return;
    setOutcome(item.outcome ?? "approved");
    setNote(item.outcome_note ?? "");
  }, [item]);

  const handleSave = async () => {
    if (!item) return;
    setIsSaving(true);
    try {
      await updateSessionItem(workspaceSlug, sessionId, item.id, { outcome, outcome_note: note.trim() });
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      if (onSaved) await onSaved();
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.items.save_failed"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{t("review.items.record")}</h3>
        {item && (
          <p className="text-12 text-secondary">
            {[item.subject.identifier, item.subject.name].filter(Boolean).join(" ")}
          </p>
        )}
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("review.items.outcome_label")}</label>
          <CustomSelect
            value={outcome}
            onChange={(value: TReviewOutcome) => setOutcome(value)}
            label={t(REVIEW_OUTCOME_CONFIG[outcome].label_key)}
          >
            {REVIEW_OUTCOMES.map((value) => (
              <CustomSelect.Option key={value} value={value}>
                {t(REVIEW_OUTCOME_CONFIG[value].label_key)}
              </CustomSelect.Option>
            ))}
          </CustomSelect>
        </div>
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("review.items.note_label")}</label>
          <TextArea value={note} onChange={(event) => setNote(event.target.value)} rows={3} />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isSaving} onClick={() => void handleSave()}>
            {t("review.items.save")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

- [ ] **Step 2: Add-to-agenda modal**

Create `apps/web/core/components/reviews/session/add-agenda-items-modal.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TReviewBoardType } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  sessionId: string;
  boardType: TReviewBoardType;
  projectId?: string;
  excludeRequestIds: string[];
  onAdded?: () => void | Promise<void>;
};

export const AddAgendaItemsModal = observer(function AddAgendaItemsModal({
  isOpen,
  onClose,
  workspaceSlug,
  sessionId,
  boardType,
  projectId,
  excludeRequestIds,
  onAdded,
}: Props) {
  const { t } = useTranslation();
  const { getRequestById, getRequestIds, fetchRequests, addSessionItems } = useReview();
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [isSaving, setIsSaving] = useState(false);

  const params = { board_type: boardType, project_id: projectId, status: "pending" as const };
  const fetched = getRequestIds(params);

  useEffect(() => {
    if (!isOpen) return;
    setSelectedIds([]);
    void fetchRequests(workspaceSlug, { board_type: boardType, project_id: projectId, status: "pending" });
  }, [isOpen, fetchRequests, boardType, projectId, workspaceSlug]);

  const candidates = (fetched ?? []).filter((requestId) => !excludeRequestIds.includes(requestId));

  const toggle = (requestId: string) => {
    setSelectedIds((current) =>
      current.includes(requestId) ? current.filter((id) => id !== requestId) : [...current, requestId]
    );
  };

  const handleAdd = async () => {
    if (selectedIds.length === 0) return;
    setIsSaving(true);
    try {
      await addSessionItems(workspaceSlug, sessionId, selectedIds);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      if (onAdded) await onAdded();
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.items.add_failed"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{t("review.items.add")}</h3>
        <div className="max-h-80 space-y-1 overflow-y-auto">
          {candidates.length === 0 ? (
            <p className="text-12 text-secondary">{t("review.sessions.empty_items")}</p>
          ) : (
            candidates.map((requestId) => {
              const request = getRequestById(requestId);
              if (!request) return null;
              const label =
                request.board_type === "tcb"
                  ? [request.subject.issue?.identifier, request.subject.issue?.name].filter(Boolean).join(" ")
                  : (request.subject.release?.name ?? "");
              return (
                <button
                  key={requestId}
                  type="button"
                  onClick={() => toggle(requestId)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md border border-subtle px-3 py-2 text-left text-12",
                    selectedIds.includes(requestId) ? "bg-accent-primary/10" : "hover:bg-layer-1"
                  )}
                >
                  <input type="checkbox" checked={selectedIds.includes(requestId)} readOnly />
                  <span className="truncate text-primary">{label}</span>
                </button>
              );
            })
          )}
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            loading={isSaving}
            disabled={selectedIds.length === 0}
            onClick={() => void handleAdd()}
          >
            {t("review.items.add_confirm")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

- [ ] **Step 3: Agenda section**

Create `apps/web/core/components/reviews/session/session-agenda.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail, IReviewSessionItem } from "@plane/types";
// components
import { ReviewOutcomePill } from "../outcome-pill";
import { ReviewRequestStatusPill } from "../request-status-pill";
import { AddAgendaItemsModal } from "./add-agenda-items-modal";
import { RecordOutcomeModal } from "./record-outcome-modal";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionAgenda = observer(function SessionAgenda({ workspaceSlug, session, canManage }: Props) {
  const { t } = useTranslation();
  const { removeSessionItem, fetchSessionDetail } = useReview();
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [outcomeItem, setOutcomeItem] = useState<IReviewSessionItem | null>(null);

  const refresh = async () => {
    await fetchSessionDetail(workspaceSlug, session.id);
  };

  const handleRemove = async (itemId: string) => {
    try {
      await removeSessionItem(workspaceSlug, session.id, itemId);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.items.remove_failed"),
      });
    }
  };

  // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; sorting a fresh copied list
  const items = [...session.items].sort((a, b) => a.position - b.position);

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("review.sessions.agenda")}</h3>
        {canManage && session.status === "scheduled" && (
          <Button variant="secondary" size="sm" onClick={() => setIsAddOpen(true)}>
            {t("review.items.add")}
          </Button>
        )}
      </div>
      {items.length === 0 ? (
        <p className="text-12 text-secondary">{t("review.sessions.empty_items")}</p>
      ) : (
        <ul className="space-y-2">
          {items.map((item) => {
            const label = [item.subject.identifier, item.subject.name].filter(Boolean).join(" ");
            const canRemove = !item.outcome || item.outcome === "deferred";
            return (
              <li key={item.id} className="flex items-center gap-3 rounded-md border border-subtle px-3 py-2">
                <span className="w-6 text-center font-code text-11 text-tertiary">{item.position + 1}</span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-13 text-primary">{label}</p>
                  {item.outcome_note && <p className="truncate text-11 text-tertiary">{item.outcome_note}</p>}
                </div>
                <ReviewRequestStatusPill status={item.request_status} />
                {item.outcome && <ReviewOutcomePill outcome={item.outcome} />}
                {canManage && session.status === "scheduled" && (
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      className="text-11 text-accent-primary hover:underline"
                      onClick={() => setOutcomeItem(item)}
                    >
                      {t("review.items.record")}
                    </button>
                    {canRemove && (
                      <button
                        type="button"
                        className="text-11 text-tertiary hover:text-danger-primary"
                        onClick={() => void handleRemove(item.id)}
                      >
                        {t("review.items.remove")}
                      </button>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <AddAgendaItemsModal
        isOpen={isAddOpen}
        onClose={() => setIsAddOpen(false)}
        workspaceSlug={workspaceSlug}
        sessionId={session.id}
        boardType={session.board_type}
        projectId={session.project_id ?? undefined}
        excludeRequestIds={items.map((item) => item.review_request_id)}
        onAdded={refresh}
      />
      <RecordOutcomeModal
        isOpen={outcomeItem !== null}
        onClose={() => setOutcomeItem(null)}
        workspaceSlug={workspaceSlug}
        sessionId={session.id}
        item={outcomeItem}
        onSaved={refresh}
      />
    </section>
  );
});
```

- [ ] **Step 4: Confirm no directive comments**

Grep the files you created in this task for `eslint-disable` — there should be none. The repo lints with oxlint and unknown directives can trip `--deny-warnings`. (The effect already uses primitive deps.)

- [ ] **Step 5: Commit (with Task 9)**

---

### Task 9: Session detail — participants

**Files:**

- Create: `apps/web/core/components/reviews/session/workspace-member-select.tsx`
- Create: `apps/web/core/components/reviews/session/session-participants.tsx`

- [ ] **Step 1: Workspace member select**

Create `apps/web/core/components/reviews/session/workspace-member-select.tsx`:

```tsx
import { useEffect } from "react";
import { observer } from "mobx-react";
import { Avatar } from "@makeplane/propel/components/avatar";
// plane imports
import { CustomSearchSelect } from "@plane/ui";
import { getFileURL } from "@plane/utils";
// hooks
import { useMember } from "@/hooks/store/use-member";

type Props = {
  workspaceSlug: string;
  value: string;
  onChange: (userId: string) => void;
  excludeUserIds: string[];
  isDisabled?: boolean;
  placeholder: string;
};

export const WorkspaceMemberSelect = observer(function WorkspaceMemberSelect({
  workspaceSlug,
  value,
  onChange,
  excludeUserIds,
  isDisabled = false,
  placeholder,
}: Props) {
  const {
    workspace: { workspaceMemberIds, getWorkspaceMemberDetails, fetchWorkspaceMembers },
  } = useMember();

  useEffect(() => {
    if (workspaceMemberIds) return;
    void fetchWorkspaceMembers(workspaceSlug).catch(() => undefined);
  }, [workspaceMemberIds, fetchWorkspaceMembers, workspaceSlug]);

  const options =
    workspaceMemberIds
      ?.map((userId) => {
        const details = getWorkspaceMemberDetails(userId);
        if (!details?.member) return null;
        return {
          value: userId,
          query: details.member.display_name,
          content: (
            <div className="flex items-center gap-2">
              <Avatar
                alt={details.member.display_name}
                fallback={details.member.display_name?.[0]?.toUpperCase()}
                src={getFileURL(details.member.avatar_url)}
                size="xs"
              />
              {details.member.display_name}
            </div>
          ),
        };
      })
      .filter((option): option is NonNullable<typeof option> => Boolean(option))
      .filter((option) => !excludeUserIds.includes(option.value)) ?? [];

  return (
    <CustomSearchSelect
      value={value}
      onChange={onChange}
      options={options}
      label={value || placeholder}
      disabled={isDisabled}
      buttonClassName="!px-3 !py-2 bg-surface-1"
    />
  );
});
```

If `IUserLite` lacks `avatar_url`/`display_name`, mirror `MemberSelect` (`apps/web/core/components/project/member-select.tsx`) exactly and run typecheck before continuing.

- [ ] **Step 2: Participants section**

Create `apps/web/core/components/reviews/session/session-participants.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import {
  REVIEW_ATTENDANCE_LABEL_KEYS,
  REVIEW_ATTENDANCE_VALUES,
  REVIEW_PARTICIPANT_ROLE_LABEL_KEYS,
  REVIEW_PARTICIPANT_ROLES,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewParticipant, IReviewSessionDetail, TReviewAttendance, TReviewParticipantRole } from "@plane/types";
import { CustomSelect } from "@plane/ui";
// components
import { WorkspaceMemberSelect } from "./workspace-member-select";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionParticipants = observer(function SessionParticipants({ workspaceSlug, session, canManage }: Props) {
  const { t } = useTranslation();
  const { addSessionParticipant, updateSessionParticipant, removeSessionParticipant } = useReview();
  const [selectedUserId, setSelectedUserId] = useState("");
  const [isAdding, setIsAdding] = useState(false);

  const handleError = (error: unknown, fallback: string) => {
    const apiError = error as { detail?: string; error?: string };
    setToast({
      type: TOAST_TYPE.ERROR,
      title: "Error!",
      message: apiError?.detail ?? apiError?.error ?? fallback,
    });
  };

  const handleAdd = async () => {
    if (!selectedUserId) return;
    setIsAdding(true);
    try {
      await addSessionParticipant(workspaceSlug, session.id, { user_id: selectedUserId, role: "member" });
      setSelectedUserId("");
    } catch (error) {
      handleError(error, t("review.participants.add_failed"));
    } finally {
      setIsAdding(false);
    }
  };

  const handleUpdate = async (
    participant: IReviewParticipant,
    data: Partial<{ role: TReviewParticipantRole; attendance: TReviewAttendance }>
  ) => {
    try {
      await updateSessionParticipant(workspaceSlug, session.id, participant.id, data);
    } catch (error) {
      handleError(error, t("review.participants.update_failed"));
    }
  };

  const handleRemove = async (participantId: string) => {
    try {
      await removeSessionParticipant(workspaceSlug, session.id, participantId);
    } catch (error) {
      handleError(error, t("review.participants.remove_failed"));
    }
  };

  return (
    <div className="space-y-4 p-6">
      <h3 className="text-13 font-semibold text-primary">{t("review.sessions.participants_count")}</h3>
      {canManage && session.status === "scheduled" && (
        <div className="space-y-2">
          <WorkspaceMemberSelect
            workspaceSlug={workspaceSlug}
            value={selectedUserId}
            onChange={setSelectedUserId}
            excludeUserIds={session.participants.map((participant) => participant.user_id)}
            placeholder={t("review.participants.select_member")}
          />
          <Button
            variant="secondary"
            size="sm"
            className="w-full"
            loading={isAdding}
            disabled={!selectedUserId}
            onClick={() => void handleAdd()}
          >
            {t("review.participants.add")}
          </Button>
        </div>
      )}
      {session.participants.length === 0 ? (
        <p className="text-12 text-secondary">{t("review.participants.empty")}</p>
      ) : (
        <ul className="space-y-3">
          {session.participants.map((participant) => (
            <li key={participant.id} className="space-y-2 rounded-md border border-subtle px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-12 font-medium text-primary">
                  {participant.display_name || participant.email || participant.user_id}
                </span>
                {canManage && session.status === "scheduled" && (
                  <button
                    type="button"
                    className="text-11 text-tertiary hover:text-danger-primary"
                    onClick={() => void handleRemove(participant.id)}
                  >
                    {t("review.participants.remove")}
                  </button>
                )}
              </div>
              <div className="grid grid-cols-2 gap-2">
                <CustomSelect
                  value={participant.role}
                  onChange={(value: TReviewParticipantRole) => void handleUpdate(participant, { role: value })}
                  label={t(REVIEW_PARTICIPANT_ROLE_LABEL_KEYS[participant.role])}
                  disabled={!canManage || session.status !== "scheduled"}
                >
                  {REVIEW_PARTICIPANT_ROLES.map((role) => (
                    <CustomSelect.Option key={role} value={role}>
                      {t(REVIEW_PARTICIPANT_ROLE_LABEL_KEYS[role])}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
                <CustomSelect
                  value={participant.attendance}
                  onChange={(value: TReviewAttendance) => void handleUpdate(participant, { attendance: value })}
                  label={t(REVIEW_ATTENDANCE_LABEL_KEYS[participant.attendance])}
                  disabled={!canManage || session.status !== "scheduled"}
                >
                  {REVIEW_ATTENDANCE_VALUES.map((attendance) => (
                    <CustomSelect.Option key={attendance} value={attendance}>
                      {t(REVIEW_ATTENDANCE_LABEL_KEYS[attendance])}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
});
```

- [ ] **Step 3: Exports**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./session/add-agenda-items-modal";
export * from "./session/record-outcome-modal";
export * from "./session/session-agenda";
export * from "./session/session-participants";
export * from "./session/workspace-member-select";
```

- [ ] **Step 4: Typecheck + lint + test**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web test
```

Expected: 0 errors; all tests PASS.

- [ ] **Step 5: Commit the whole session detail stack (Tasks 7-9)**

```bash
git add apps/web/core/components/reviews
git commit -m "feat(web): review session detail (agenda, minutes, participants)"
```

---

### Task 10: RCB pages + routes + workspace nav

**Files:**

- Create: `apps/web/core/components/reviews/release-control/release-control-header.tsx`
- Create: `apps/web/core/components/reviews/session/session-detail-breadcrumbs.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(list)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(list)/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(detail)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(detail)/sessions/[sessionId]/page.tsx`
- Modify: `apps/web/app/routes/core.ts` (insert after the Release Detail block)
- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`
- Modify: `apps/web/core/components/reviews/index.ts`

- [ ] **Step 1: Shared session detail breadcrumbs (mounts the header)**

Create `apps/web/core/components/reviews/session/session-detail-breadcrumbs.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import {
  EUserPermissions,
  EUserPermissionsLevel,
  getReleaseControlLink,
  getTestingControlLink,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
// components
import { SessionDetailHeader } from "./session-detail-header";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";

export const SessionDetailBreadcrumbs = observer(function SessionDetailBreadcrumbs() {
  const { workspaceSlug, sessionId } = useParams();
  const { t } = useTranslation();
  const { getSessionDetailById } = useReview();
  const { allowPermissions } = useUserPermissions();

  const slug = workspaceSlug?.toString() ?? "";
  const id = sessionId?.toString() ?? "";
  const session = id ? getSessionDetailById(id) : null;

  if (!session) return null;

  const canManage =
    session.board_type === "rcb"
      ? allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          slug,
          session.project_id ?? ""
        );
  const boardLabel = t(session.board_type === "rcb" ? "review.board_values.rcb" : "review.board_values.tcb");
  const boardHref =
    session.board_type === "rcb" ? getReleaseControlLink(slug) : getTestingControlLink(slug, session.project_id ?? "");

  return (
    <SessionDetailHeader
      workspaceSlug={slug}
      session={session}
      boardLabel={boardLabel}
      boardHref={boardHref}
      canManage={canManage}
    />
  );
});
```

Note: the detail root (`ReviewSessionDetailRoot`) already fetches the detail on mount; this header appears once the store has it. `SessionDetailHeader` imported from `./session-detail-header` (created in Task 7).

- [ ] **Step 2: RCB list header**

Create `apps/web/core/components/reviews/release-control/release-control-header.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { ShieldOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";

export const ReleaseControlHeader = observer(function ReleaseControlHeader() {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.release_control")}
                href={`/${workspaceSlug}/release-control`}
                icon={<ShieldOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});
```

- [ ] **Step 3: RCB pages**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(list)/layout.tsx`:

```tsx
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ReleaseControlHeader } from "@/components/reviews";

export default function WorkspaceReleaseControlLayout() {
  return (
    <>
      <AppHeader header={<ReleaseControlHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(list)/page.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewBoardRoot } from "@/components/reviews";

function WorkspaceReleaseControlPage() {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug) return null;

  return (
    <>
      <PageHead title={t("sidebar.release_control")} />
      <div className="relative h-full w-full overflow-hidden">
        <ReviewBoardRoot workspaceSlug={workspaceSlug.toString()} boardType="rcb" />
      </div>
    </>
  );
}

export default observer(WorkspaceReleaseControlPage);
```

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(detail)/layout.tsx`:

```tsx
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { SessionDetailBreadcrumbs } from "@/components/reviews";

export default function WorkspaceReleaseControlDetailLayout() {
  return (
    <>
      <AppHeader header={<SessionDetailBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/(detail)/sessions/[sessionId]/page.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewSessionDetailRoot } from "@/components/reviews";

function WorkspaceReleaseSessionPage() {
  const { workspaceSlug, sessionId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !sessionId) return null;

  return (
    <>
      <PageHead title={t("sidebar.release_control")} />
      <ReviewSessionDetailRoot workspaceSlug={workspaceSlug.toString()} sessionId={sessionId.toString()} />
    </>
  );
}

export default observer(WorkspaceReleaseSessionPage);
```

- [ ] **Step 4: Register RCB routes**

In `apps/web/app/routes/core.ts`, insert immediately after the Release Detail layout block (the one ending `:workspaceSlug/releases/:releaseId`), before `// Notifications`:

```ts
        // Release Control (RCB board)
        layout("./(all)/[workspaceSlug]/(projects)/release-control/(list)/layout.tsx", [
          route(
            ":workspaceSlug/release-control",
            "./(all)/[workspaceSlug]/(projects)/release-control/(list)/page.tsx"
          ),
        ]),

        // Release Control Session Detail
        layout("./(all)/[workspaceSlug]/(projects)/release-control/(detail)/layout.tsx", [
          route(
            ":workspaceSlug/release-control/sessions/:sessionId",
            "./(all)/[workspaceSlug]/(projects)/release-control/(detail)/sessions/[sessionId]/page.tsx"
          ),
        ]),
```

- [ ] **Step 5: Workspace nav entry**

In `packages/constants/src/workspace.ts`, add to `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS` after the `releases` entry:

```ts
  release_control: {
    key: "release_control",
    labelTranslationKey: "sidebar.release_control",
    href: `/release-control/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname, url) => pathname.includes(url),
  },
```

Add to `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS` after the `releases` entry:

```ts
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["release_control"],
```

- [ ] **Step 6: Sidebar icon**

In `apps/web/core/components/workspace/sidebar/helper.tsx`:

1. Add `ShieldOutline,` to the `@makeplane/propel/icons` import list (alphabetical position after `ProjectsOutline`).
2. Add at the end of the switch, after the `releases` case:

```tsx
    case "release_control":
      return <ShieldOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 7: Exports**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./release-control/release-control-header";
export * from "./session/session-detail-breadcrumbs";
```

- [ ] **Step 8: Typecheck + lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: 0 errors.

- [ ] **Step 9: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/release-control" apps/web/app/routes/core.ts \
  packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/helper.tsx \
  apps/web/core/components/reviews
git commit -m "feat(web): release control board pages + navigation"
```

---

### Task 11: TCB pages + routes + conditional project nav

**Files:**

- Create: `apps/web/core/components/reviews/testing-control/testing-control-header.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(detail)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(detail)/sessions/[sessionId]/page.tsx`
- Modify: `apps/web/app/routes/core.ts` (insert after the Services List block, inside the `[projectId]` layout)
- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx`
- Modify: `apps/web/core/components/navigation/use-navigation-items.ts`
- Modify: `apps/web/core/components/reviews/index.ts`

- [ ] **Step 1: TCB list header**

Create `apps/web/core/components/reviews/testing-control/testing-control-header.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CheckDoneOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";

export const TestingControlHeader = observer(function TestingControlHeader() {
  const { workspaceSlug, projectId } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.testing_control")}
                href={`/${workspaceSlug}/projects/${projectId}/testing-control`}
                icon={<CheckDoneOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});
```

- [ ] **Step 2: TCB pages**

Create the list layout at `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/layout.tsx`:

```tsx
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { TestingControlHeader } from "@/components/reviews";

export default function TestingControlLayout() {
  return (
    <>
      <AppHeader header={<TestingControlHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Create the list page at the same directory `page.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewBoardRoot } from "@/components/reviews";

function TestingControlPage() {
  const { workspaceSlug, projectId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !projectId) return null;

  return (
    <>
      <PageHead title={t("sidebar.testing_control")} />
      <div className="relative h-full w-full overflow-hidden">
        <ReviewBoardRoot workspaceSlug={workspaceSlug.toString()} boardType="tcb" projectId={projectId.toString()} />
      </div>
    </>
  );
}

export default observer(TestingControlPage);
```

Create the detail layout at `.../testing-control/(detail)/layout.tsx`:

```tsx
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { SessionDetailBreadcrumbs } from "@/components/reviews";

export default function TestingControlDetailLayout() {
  return (
    <>
      <AppHeader header={<SessionDetailBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Create the detail page at `.../testing-control/(detail)/sessions/[sessionId]/page.tsx`:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewSessionDetailRoot } from "@/components/reviews";

function TestingControlSessionPage() {
  const { workspaceSlug, sessionId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !sessionId) return null;

  return (
    <>
      <PageHead title={t("sidebar.testing_control")} />
      <ReviewSessionDetailRoot workspaceSlug={workspaceSlug.toString()} sessionId={sessionId.toString()} />
    </>
  );
}

export default observer(TestingControlSessionPage);
```

- [ ] **Step 3: Register TCB routes**

In `apps/web/app/routes/core.ts`, inside the `[projectId]` layout children, insert immediately after the `// Services List` layout block (the one ending `:workspaceSlug/projects/:projectId/services`), before `// War Room Detail`:

```ts
          // Testing Control (TCB board)
          layout(
            "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/layout.tsx",
            [
              route(
                ":workspaceSlug/projects/:projectId/testing-control",
                "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/page.tsx"
              ),
            ]
          ),

          // Testing Control Session Detail
          layout(
            "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(detail)/layout.tsx",
            [
              route(
                ":workspaceSlug/projects/:projectId/testing-control/sessions/:sessionId",
                "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(detail)/sessions/[sessionId]/page.tsx"
              ),
            ]
          ),
```

- [ ] **Step 4: Sidebar-mode project nav (conditional on Change type)**

In `apps/web/core/components/workspace/sidebar/project-navigation.tsx`:

1. Add `useEffect` to the React import.
2. Add `CheckDoneOutline,` to the `@makeplane/propel/icons` import list.
3. Add the workflow hook import:

```ts
import { useWorkflow } from "@/hooks/store/use-workflow";
```

4. Inside `ProjectNavigation`, after `const project = getPartialProjectById(projectId);`:

```ts
const { workItemTypes, fetchWorkItemTypes } = useWorkflow();

useEffect(() => {
  if (workItemTypes) return;
  void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
}, [workItemTypes, fetchWorkItemTypes, workspaceSlug]);

const hasChangeType =
  workItemTypes?.some((type) => type.name.toLowerCase() === "change" && type.project_ids?.includes(projectId)) ?? false;
```

5. Add the nav item to `baseNavigation` after the `war-rooms` entry:

```ts
      {
        i18n_key: "sidebar.testing_control",
        key: "testing-control",
        name: "Testing control",
        href: `/${workspaceSlug}/projects/${projectId}/testing-control`,
        icon: CheckDoneOutline,
        access: [EUserPermissions.ADMIN, EUserPermissions.MEMBER, EUserPermissions.GUEST],
        shouldRender: hasChangeType,
        sortOrder: 4.75,
      },
```

6. Add `hasChangeType` to the `useCallback` dependency array (`[project]` → `[project, hasChangeType]`).

- [ ] **Step 5: Tabbed-mode project nav**

In `apps/web/core/components/navigation/use-navigation-items.ts`:

1. Add `useEffect` to the React import (currently `useMemo, useCallback`).
2. Add `CheckDoneOutline,` to the `@makeplane/propel/icons` import list.
3. Add `import { useWorkflow } from "@/hooks/store/use-workflow";`.
4. Inside `useNavigationItems`, after the destructure:

```ts
const { workItemTypes, fetchWorkItemTypes } = useWorkflow();

useEffect(() => {
  if (workItemTypes) return;
  void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
}, [workItemTypes, fetchWorkItemTypes, workspaceSlug]);

const hasChangeType =
  workItemTypes?.some((type) => type.name.toLowerCase() === "change" && type.project_ids?.includes(projectId)) ?? false;
```

5. Add the same nav item after the `war-rooms` entry and extend the `baseNavigation` deps `[project]` → `[project, hasChangeType]`.

- [ ] **Step 6: Exports**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./testing-control/testing-control-header";
```

- [ ] **Step 7: Typecheck + lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: 0 errors.

- [ ] **Step 8: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control" \
  apps/web/app/routes/core.ts \
  apps/web/core/components/workspace/sidebar/project-navigation.tsx \
  apps/web/core/components/navigation/use-navigation-items.ts \
  apps/web/core/components/reviews
git commit -m "feat(web): testing control board pages + conditional nav"
```

---

### Task 12: Submit to TCB in the Change work item detail

**Files:**

- Create: `apps/web/core/components/reviews/change-tcb-control.tsx`
- Modify: `apps/web/core/components/reviews/index.ts`
- Modify: `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`

- [ ] **Step 1: Create the control**

Create `apps/web/core/components/reviews/change-tcb-control.tsx`:

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// components
import { SubmitReviewModal } from "./submit-review-modal";
import { TcbStatusBadge } from "./tcb-status-badge";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  projectId: string;
  issueId: string;
};

export const ChangeTcbControl = observer(function ChangeTcbControl({ workspaceSlug, projectId, issueId }: Props) {
  const { t } = useTranslation();
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  const { workItemTypes, fetchWorkItemTypes } = useWorkflow();
  const { allowPermissions } = useUserPermissions();
  const { getRequestIds, getLatestRequestForChange, fetchRequests, withdrawRequest } = useReview();
  const [isSubmitOpen, setIsSubmitOpen] = useState(false);

  const issue = getIssueById(issueId);

  useEffect(() => {
    if (workItemTypes) return;
    void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
  }, [workItemTypes, fetchWorkItemTypes, workspaceSlug]);

  const fetched = getRequestIds({ board_type: "tcb", project_id: projectId });

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: "tcb", project_id: projectId });
  }, [fetched, fetchRequests, projectId, workspaceSlug]);

  const issueType = workItemTypes?.find((type) => type.id === issue?.type_id);
  if (!issue || issue.archived_at || issueType?.name.toLowerCase() !== "change") return <></>;

  const canWrite = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    projectId
  );
  const latest = getLatestRequestForChange(issueId);
  const hasActive = latest !== null && (latest.status === "pending" || latest.status === "scheduled");

  const handleWithdraw = async () => {
    if (!latest) return;
    try {
      await withdrawRequest(workspaceSlug, latest.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.request.withdrawn") });
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.request.withdraw_failed"),
      });
    }
  };

  return (
    <>
      <TcbStatusBadge changeIssueId={issueId} projectId={projectId} workspaceSlug={workspaceSlug} />
      {canWrite && !hasActive && (
        <Button variant="secondary" size="lg" onClick={() => setIsSubmitOpen(true)}>
          {latest ? t("review.change.resubmit") : t("review.change.submit")}
        </Button>
      )}
      {canWrite && hasActive && (
        <Button variant="secondary" size="lg" onClick={() => void handleWithdraw()}>
          {t("review.change.withdraw")}
        </Button>
      )}
      <SubmitReviewModal
        isOpen={isSubmitOpen}
        onClose={() => setIsSubmitOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType="tcb"
        subjectId={issueId}
        title={t("review.change.submit")}
        submitLabel={t("review.request.submit")}
      />
    </>
  );
});
```

- [ ] **Step 2: Export it**

In `apps/web/core/components/reviews/index.ts`, add:

```ts
export * from "./change-tcb-control";
```

- [ ] **Step 3: Mount it in the work item quick actions**

In `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`:

1. Add the import next to the war room button import:

```ts
import { ChangeTcbControl } from "@/components/reviews/change-tcb-control";
```

2. In the render, replace:

```tsx
<IssueWarRoomButton workspaceSlug={workspaceSlug} projectId={projectId} issueId={issueId} />
```

with:

```tsx
            <ChangeTcbControl workspaceSlug={workspaceSlug} projectId={projectId} issueId={issueId} />
            <IssueWarRoomButton workspaceSlug={workspaceSlug} projectId={projectId} issueId={issueId} />
```

(The component renders an empty fragment unless the work item type is `Change`.)

- [ ] **Step 4: Typecheck + lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/reviews/change-tcb-control.tsx apps/web/core/components/reviews/index.ts \
  apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx
git commit -m "feat(web): submit to TCB from change detail"
```

---

### Task 13: Render review notifications (card + right pane)

**Files:**

- Create: `apps/web/core/components/workspace-notifications/review-detail.tsx`
- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`
- Modify: `apps/web/core/components/workspace-notifications/root.tsx`

- [ ] **Step 1: Right pane detail component**

Create `apps/web/core/components/workspace-notifications/review-detail.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRouter } from "next/navigation";
// plane imports
import {
  getReleaseControlLink,
  getReviewSessionLink,
  getTestingControlLink,
  REVIEW_BOARD_LABEL_KEYS,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TNotificationReviewRequest, TNotificationReviewSession } from "@plane/types";

type Props = {
  workspaceSlug: string;
  reviewRequest?: TNotificationReviewRequest;
  reviewSession?: TNotificationReviewSession;
  embedRemoveCurrentNotification: () => void;
};

export function ReviewInboxDetail({
  workspaceSlug,
  reviewRequest,
  reviewSession,
  embedRemoveCurrentNotification,
}: Props) {
  const router = useRouter();
  const { t } = useTranslation();

  const boardType = reviewRequest?.board_type ?? reviewSession?.board_type ?? "tcb";
  const projectId = reviewRequest?.project_id ?? reviewSession?.project_id ?? undefined;
  const sessionId = reviewRequest?.session_id ?? reviewSession?.id;

  const open = () => {
    if (sessionId) {
      router.push(getReviewSessionLink(workspaceSlug, sessionId, boardType, projectId ?? undefined));
      return;
    }
    router.push(
      boardType === "rcb" ? getReleaseControlLink(workspaceSlug) : getTestingControlLink(workspaceSlug, projectId ?? "")
    );
  };

  return (
    <div className="h-full w-full overflow-y-auto p-4">
      <div className="mx-auto max-w-2xl space-y-3">
        <h3 className="text-base font-semibold break-words text-primary">
          {reviewRequest?.subject_label ?? reviewSession?.title ?? ""}
        </h3>
        <p className="text-xs text-tertiary">{t(REVIEW_BOARD_LABEL_KEYS[boardType])}</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="secondary" onClick={open}>
            {t("review.notification.open")}
          </Button>
          <Button size="sm" variant="ghost" onClick={embedRemoveCurrentNotification}>
            {t("review.notification.dismiss")}
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Notification card branches**

In `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`:

1. Replace the imports:

```tsx
import { CalendarOutline, CheckDoneOutline, ClockOutline } from "@makeplane/propel/icons";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import {
  getReleaseControlLink,
  getReviewSessionLink,
  getTestingControlLink,
  getWarRoomLink,
  REVIEW_BOARD_LABEL_KEYS,
  REVIEW_OUTCOME_CONFIG,
} from "@plane/constants";
```

2. Add the guard import next to `isWarRoomNotification`:

```ts
import { isReviewRequestNotification, isReviewSessionNotification } from "@/lib/review-notification";
```

3. After the `warRoom` derived value, add:

```tsx
const reviewRequest = isReviewRequestNotification(notification?.data) ? notification?.data?.review_request : undefined;
const reviewSession = isReviewSessionNotification(notification?.data) ? notification?.data?.review_session : undefined;
```

4. Replace the early-return guard:

```tsx
if (!scheduleRun && !warRoom && (!notificationField || !projectId)) return <></>;
```

with:

```tsx
if (!scheduleRun && !warRoom && !reviewRequest && !reviewSession && (!notificationField || !projectId)) return <></>;
```

5. In `handleNotificationClick`, after the `scheduleRun` early return, add:

```tsx
// review notifications deep-link into the session (or the board)
if (reviewRequest || reviewSession) {
  const boardType = reviewRequest?.board_type ?? reviewSession?.board_type ?? "tcb";
  const boardProjectId = reviewRequest?.project_id ?? reviewSession?.project_id ?? undefined;
  const sessionId = reviewRequest?.session_id ?? reviewSession?.id;
  router.push(
    sessionId
      ? getReviewSessionLink(workspaceSlug, sessionId, boardType, boardProjectId ?? undefined)
      : boardType === "rcb"
        ? getReleaseControlLink(workspaceSlug)
        : getTestingControlLink(workspaceSlug, boardProjectId ?? "")
  );
  return;
}
```

6. Icon branch: replace

```tsx
          ) : scheduleRun ? (
            <CalendarOutline className="h-5 w-5 text-tertiary" />
          ) : null}
```

with

```tsx
          ) : scheduleRun ? (
            <CalendarOutline className="h-5 w-5 text-tertiary" />
          ) : reviewRequest || reviewSession ? (
            <CheckDoneOutline className="h-5 w-5 text-tertiary" />
          ) : null}
```

7. Title branch: replace

```tsx
              ) : warRoom ? (
                <span className="font-medium text-primary">
                  {t("war_room.notification.mention", {
                    name: notificationTriggeredBy?.is_bot
                      ? notificationTriggeredBy.first_name
                      : notificationTriggeredBy?.display_name ||
                        notificationTriggeredBy?.first_name ||
                        t("war_room.activity.actor_unknown"),
                    room: warRoom.name,
                  })}
                </span>
```

with

```tsx
              ) : warRoom ? (
                <span className="font-medium text-primary">
                  {t("war_room.notification.mention", {
                    name: notificationTriggeredBy?.is_bot
                      ? notificationTriggeredBy.first_name
                      : notificationTriggeredBy?.display_name ||
                        notificationTriggeredBy?.first_name ||
                        t("war_room.activity.actor_unknown"),
                    room: warRoom.name,
                  })}
                </span>
              ) : reviewSession ? (
                <span className="font-medium text-primary">
                  {t("review.notification.session_scheduled", { session: reviewSession.title })}
                </span>
              ) : reviewRequest ? (
                <span className="font-medium text-primary">
                  {reviewRequest.outcome
                    ? t("review.notification.decided", {
                        subject: reviewRequest.subject_label,
                        outcome: t(REVIEW_OUTCOME_CONFIG[reviewRequest.outcome].label_key),
                      })
                    : t("review.notification.agenda_added", {
                        subject: reviewRequest.subject_label,
                        session: reviewRequest.session_title ?? "",
                      })}
                </span>
```

8. Subtitle branch: replace

```tsx
              ) : warRoom ? (
                <span>WR-{warRoom.sequence_id}</span>
```

with

```tsx
              ) : warRoom ? (
                <span>WR-{warRoom.sequence_id}</span>
              ) : reviewRequest || reviewSession ? (
                <span>{t(REVIEW_BOARD_LABEL_KEYS[(reviewRequest ?? reviewSession)!.board_type])}</span>
```

- [ ] **Step 3: Right pane branch in root**

In `apps/web/core/components/workspace-notifications/root.tsx`:

1. Add imports:

```tsx
import { isReviewRequestNotification, isReviewSessionNotification } from "@/lib/review-notification";
import { ReviewInboxDetail } from "./review-detail";
```

2. After `selectedWarRoom`, add:

```tsx
const selectedReviewRequest = isReviewRequestNotification(selectedNotification?.data)
  ? selectedNotification?.data?.review_request
  : undefined;
const selectedReviewSession = isReviewSessionNotification(selectedNotification?.data)
  ? selectedNotification?.data?.review_session
  : undefined;
```

3. In the render, insert a branch after the `selectedWarRoom` branch and before the `is_inbox_issue` branch:

```tsx
          ) : (selectedReviewRequest || selectedReviewSession) && workspace_slug ? (
            <ReviewInboxDetail
              workspaceSlug={workspace_slug}
              reviewRequest={selectedReviewRequest}
              reviewSession={selectedReviewSession}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
```

- [ ] **Step 4: Typecheck + lint + build the types package**

```bash
pnpm --filter=@plane/types build
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: 0 errors.

- [ ] **Step 5: Manual click-through (dev server)**

Start the dev server on `:3000` per `AGENTS.md` (stop prod first), then trigger a TCB submit with a second account:

1. `systemctl --user stop plane-web-prod.service && systemctl --user start plane-web.service`
2. As user A: open a `Change` work item in a project whose Change type came from the TCB seeding, click **Submit to TCB**.
3. As user B (reviewer/participant or the submitter of an agenda item), open the notification inbox: the card renders the review title (no blank card), clicking it opens the session or board page.

If the card renders blank, re-check the `item.tsx` guard in Step 2.4 — the `data` payload contract is `review_request` / `review_session`.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/workspace-notifications
git commit -m "feat(web): render review notifications"
```

---

### Task 14: Verification wrap-up

**Files:** none (verification only; commit only if fixes are needed).

- [ ] **Step 1: Full web checks**

```bash
pnpm --filter=web test
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: all tests PASS; 0 TS errors; lint `0 errors` (warnings allowed at baseline).

- [ ] **Step 2: i18n sync**

```bash
pnpm --filter @plane/i18n run sync:check
```

Expected: no `release.*`/`review.*` missing keys; only the pre-existing `auth.*` baseline (18 missing / 32 stale).

- [ ] **Step 3: Rust notification test (already green in Task 1)**

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test notification_test -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 4: Build web prod + restart (required by `AGENTS.md`)**

```bash
cd /home/ghifari/plane-for-itsm
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
systemctl --user is-active plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/
```

Expected: build succeeds; service `active`; `200`.

- [ ] **Step 5: Manual E2E smoke on the tunnel**

Open `https://<tunnel>/<workspaceSlug>/`:

1. Sidebar shows **Releases** and **Release control**; a project with a `Change` type shows **Testing control** (a project without Change does not).
2. `/release-control` renders Queue + Sessions tabs; `?tab` state switches; empty states show.
3. `/projects/<id>/testing-control` renders the same board scoped to the project.
4. Create a session (Sessions tab) → open detail → add a pending request to the agenda → record a decision → edit minutes → add a participant → complete the session.
5. Change detail shows the TCB badge + **Submit to TCB**; submitting updates the badge and notifies the reviewer account.
6. Release detail Submit-to-RCB still works (shared modal refactor).

- [ ] **Step 6: Commit any fixes**

```bash
git add -A apps packages
git commit -m "chore(web): rcb/tcb polish"
```

(Skip when there are no changes.)

---

## Self-review notes

- **Spec coverage:**
  - RCB board `/[workspaceSlug]/release-control/` (Queue + Sessions tabs): Tasks 6, 10.
  - RCB session detail `.../sessions/[sessionId]/`: Tasks 7-9, 10.
  - TCB board `/projects/[projectId]/testing-control/` + session detail: Tasks 6-9, 11.
  - Nav muncul hanya bila type `Change` di-import ke project: Task 11 (`hasChangeType`, match case-insensitive).
  - Detail Change: tombol Submit + badge status TCB terakhir: Task 12 (`ChangeTcbControl`).
  - Satu komponen di-parameterisasi `board_type`: `ReviewBoardRoot`, `ReviewQueueBoard`, `ReviewSessionsBoard`, `SessionFormModal`, `SubmitReviewModal`, `SessionDetailBreadcrumbs`.
  - Notifikasi `review_request`/`review_session`: backend whitelist Task 1; web card + right pane Task 13.
  - Semua string baru lewat 20 locale + translate skill: Task 2.
- **Deviation (perubahan sadar):** release detail's own submit modal digantikan modal bersama (Task 5); status release tetap di-update ke `in_review` via callback `onSubmitted`.
- **Dependencies:** Task 2 (i18n/generate types) must precede Tasks 5-13. Tasks 3 and 4 are independent. Tasks 8-9 must land before the combined commit in Task 9.
- **Risk:** `hasError` key literals in Task 6 must match `review.helpers.ts` exactly; Step 8 of Task 6 calls this out.
