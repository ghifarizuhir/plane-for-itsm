# Hide Release Packages + RCB + TCB UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sembunyikan seluruh entry-point UI Releases / Release Control (RCB) / Testing Control (TCB) sehingga nav hilang dan URL langsung 404, tanpa menyentuh backend, DB, atau menghapus file.

**Architecture:** Hapus 6 blok route dari `apps/web/app/routes/core.ts`, hapus 2 entri nav workspace + 1 entri TCB di 2 file nav project, hapus tombol TCB di issue detail, jadikan render notifikasi review sebagai fallback generik. Semua file page/komponen/store/service/types/i18n tetap di disk (dead code yang disengaja, reversible via revert).

**Tech Stack:** React Router (data routes di Next.js app dir), React 19, `@plane/constants` nav, MobX stores (tidak diubah), oxlint + tsc, `pnpm --filter=web build`, `systemctl --user restart plane-web-prod.service`.

**Spec:** `docs/superpowers/specs/2026-10-09-hide-release-review-ui-design.md`

**Non-goals (jangan disentuh):** `apps/api-rs/**` (termasuk `routes/release.rs`, `routes/review.rs`, `routes/review_briefing.rs`, `routes/notification.rs`, migrasi `0012`/`0013`, tests), `packages/types/src/release|review`, `packages/constants/src/release.ts|review.ts`, `packages/i18n/*/release.json|review.json`, `apps/web/core/store/release.store.ts|review.store.ts`, `apps/web/core/services/release.*|review.*`, `apps/web/core/lib/review-notification.*`, `components/releases/*`, `components/reviews/*`, `app/.../releases|release-control|testing-control/*` (file tetap, hanya route-nya yang dihapus), `scripts/release.sh`.

---

## File Structure

Edit (8 file, 1 commit per task di bawah, total 5 commit):

- `apps/web/app/routes/core.ts` — hapus 6 blok route (releases list/detail, release-control list/detail, testing-control list/detail). Tidak ada import yang perlu dibersihkan (file ini deklaratif).
- `packages/constants/src/workspace.ts` — hapus `releases` + `release_control` dari `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS`; definisi object tetap.
- `apps/web/core/components/workspace/sidebar/helper.tsx` — hapus `case releases` + `case release_control` + import ikon `RocketOutline`/`ShieldOutline` yang menjadi unused.
- `apps/web/core/components/workspace/sidebar/project-navigation.tsx` — hapus item `testing-control` + seluruh wiring `hasChangeType`/`useWorkItemType` yang menjadi unused + import `CheckDoneOutline`.
- `apps/web/core/components/navigation/use-navigation-items.ts` — sama seperti di atas untuk tabbed mode.
- `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx` — hapus import + render `ChangeTcbControl`.
- `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx` — hapus branch review (deep-link, ikon, judul, subjudul) + import yang menjadi unused.
- `apps/web/core/components/workspace-notifications/root.tsx` — hapus render `ReviewInboxDetail` + guard review + import terkait.

---

### Task 1: Route removal — `apps/web/app/routes/core.ts`

**Files:**

- Modify: `apps/web/app/routes/core.ts:80-104`
- Modify: `apps/web/app/routes/core.ts:221-238`

- [ ] **Step 1: Verify current state**

Run:

```bash
rg -n "releases|release-control|testing-control" apps/web/app/routes/core.ts
```

Expected: 16 baris — `releases/(list)` di 81-82, `releases/(detail)` di 86-90, `release-control/(list)` di 94-95, `release-control/(detail)` di 99-103, `testing-control/(list)` di 222-226, `testing-control/(detail)` di 230-236.

- [ ] **Step 2: Delete the workspace-level blocks (releases + release-control)**

Delete this exact block (lines 80-104, termasuk komentar dan baris kosong sesudahnya sehingga `// Notifications` langsung mengikuti `// Drafts`):

```ts
        // Releases (RCB scope)
        layout("./(all)/[workspaceSlug]/(projects)/releases/(list)/layout.tsx", [
          route(":workspaceSlug/releases", "./(all)/[workspaceSlug]/(projects)/releases/(list)/page.tsx"),
        ]),

        // Release Detail
        layout("./(all)/[workspaceSlug]/(projects)/releases/(detail)/layout.tsx", [
          route(
            ":workspaceSlug/releases/:releaseId",
            "./(all)/[workspaceSlug]/(projects)/releases/(detail)/[releaseId]/page.tsx"
          ),
        ]),

        // Release Control (RCB board)
        layout("./(all)/[workspaceSlug]/(projects)/release-control/(list)/layout.tsx", [
          route(":workspaceSlug/release-control", "./(all)/[workspaceSlug]/(projects)/release-control/(list)/page.tsx"),
        ]),

        // Release Control Session Detail
        layout("./(all)/[workspaceSlug]/(projects)/release-control/(detail)/layout.tsx", [
          route(
            ":workspaceSlug/release-control/sessions/:sessionId",
            "./(all)/[workspaceSlug]/(projects)/release-control/(detail)/sessions/[sessionId]/page.tsx"
          ),
        ]),

```

After deletion the file must read:

```ts
        // Drafts
        layout("./(all)/[workspaceSlug]/(projects)/drafts/layout.tsx", [
          route(":workspaceSlug/drafts", "./(all)/[workspaceSlug]/(projects)/drafts/page.tsx"),
        ]),

        // Notifications
```

- [ ] **Step 3: Delete the project-level blocks (testing-control list + detail)**

Delete this exact block:

```ts
          // Testing Control (TCB board)
          layout("./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/layout.tsx", [
            route(
              ":workspaceSlug/projects/:projectId/testing-control",
              "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/(list)/page.tsx"
            ),
          ]),

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

After deletion the `// War Room Detail` layout must directly follow the `// Services List` layout.

- [ ] **Step 4: Verify routes are gone**

Run:

```bash
rg -n "releases|release-control|testing-control" apps/web/app/routes/core.ts; echo "exit=$?"
```

Expected: no matches, `exit=1`.

- [ ] **Step 5: Commit**

```bash
git add apps/web/app/routes/core.ts
git commit -m "feat(web): hide releases + RCB + TCB routes (404)"
```

---

### Task 2: Workspace nav — `workspace.ts` + `sidebar/helper.tsx`

**Files:**

- Modify: `packages/constants/src/workspace.ts:239-245`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx:7-49`

- [ ] **Step 1: Remove releases + release_control from the LINKS array**

In `packages/constants/src/workspace.ts`, replace:

```ts
export const WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS: IWorkspaceSidebarNavigationItem[] = [
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["views"],
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["analytics"],
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["archives"],
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["releases"],
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["release_control"],
];
```

with:

```ts
export const WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS: IWorkspaceSidebarNavigationItem[] = [
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["views"],
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["analytics"],
  WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS["archives"],
];
```

Do NOT delete the `releases` (223-229) and `release_control` (230-236) object definitions above — they stay for reversibility.

- [ ] **Step 2: Remove the icon cases + unused icon imports in helper.tsx**

Replace the import block:

```tsx
import {
  AnalyticsOutline,
  ArchiveOutline,
  CalendarOutline,
  DraftsOutline,
  HomeOutline,
  InboxOutline,
  MultipleStickyOutline,
  ProjectsOutline,
  RocketOutline,
  ShieldOutline,
  ViewsOutline,
  YourWorkOutline,
} from "@makeplane/propel/icons";
```

with:

```tsx
import {
  AnalyticsOutline,
  ArchiveOutline,
  CalendarOutline,
  DraftsOutline,
  HomeOutline,
  InboxOutline,
  MultipleStickyOutline,
  ProjectsOutline,
  ViewsOutline,
  YourWorkOutline,
} from "@makeplane/propel/icons";
```

And delete these two cases (lines 45-48):

```tsx
    case "releases":
      return <RocketOutline className={cn("size-4 flex-shrink-0", className)} />;
    case "release_control":
      return <ShieldOutline className={cn("size-4 flex-shrink-0", className)} />;
```

After deletion the switch must end with:

```tsx
    case "ai_scheduler":
      return <CalendarOutline className={cn("size-4 flex-shrink-0", className)} />;
  }
};
```

- [ ] **Step 3: Verify**

Run:

```bash
rg -n "releases|release_control" packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/helper.tsx
```

Expected: only the two object definitions in `workspace.ts:223-236` remain; no matches in `helper.tsx`; no matches for `RocketOutline|ShieldOutline` in `helper.tsx`.

- [ ] **Step 4: Commit**

```bash
git add packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/helper.tsx
git commit -m "feat(web): hide releases + release-control workspace nav"
```

---

### Task 3: Project nav (TCB) — sidebar + tabbed mode

**Files:**

- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx`
- Modify: `apps/web/core/components/navigation/use-navigation-items.ts`

- [ ] **Step 1: Delete the testing-control item in project-navigation.tsx**

Delete this exact object (lines 147-156):

```tsx
      {
        i18n_key: "sidebar.testing_control",
        key: "testing-control",
        name: "Technical review",
        href: `/${workspaceSlug}/projects/${projectId}/testing-control`,
        icon: CheckDoneOutline,
        access: [EUserPermissions.ADMIN, EUserPermissions.MEMBER, EUserPermissions.GUEST],
        shouldRender: hasChangeType,
        sortOrder: 4.75,
      },
```

- [ ] **Step 2: Remove the now-unused change-type wiring in project-navigation.tsx**

Delete the hook + effect + derived value (lines 71-80). Replace:

```tsx
const project = getPartialProjectById(projectId);
const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();

useEffect(() => {
  if (workItemTypes) return;
  void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
}, [workItemTypes, fetchWorkItemTypes, workspaceSlug]);

const hasChangeType =
  workItemTypes?.some((type) => type.name.toLowerCase() === "change" && type.project_ids?.includes(projectId)) ?? false;
// handlers
```

with:

```tsx
const project = getPartialProjectById(projectId);
// handlers
```

Remove `CheckDoneOutline,` from the icon import (line 15) and remove the `useWorkItemType` import (line 33). Change the `baseNavigation` deps (line 188) from `[project, hasChangeType]` to `[project]`. If `useEffect` becomes unused in this file after this deletion, remove it from the `react` import on line 7 (keep `useCallback, useMemo` as they are still used).

- [ ] **Step 3: Delete the testing-control item in use-navigation-items.ts**

Delete this exact object (lines 110-119):

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

- [ ] **Step 4: Remove the now-unused change-type wiring in use-navigation-items.ts**

Replace:

```ts
const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();

useEffect(() => {
  if (workItemTypes) return;
  void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
}, [workItemTypes, fetchWorkItemTypes, workspaceSlug]);

const hasChangeType =
  workItemTypes?.some((type) => type.name.toLowerCase() === "change" && type.project_ids?.includes(projectId)) ?? false;
```

with nothing (delete all 10 lines). Remove the `useWorkItemType` import (line 23), remove `CheckDoneOutline,` from the icon import (line 12), remove `useEffect` from the react import if unused elsewhere in this file (line 7: `import { useEffect, useMemo, useCallback }` → `import { useMemo, useCallback }`). Change the `baseNavigation` deps (line 151) from `[project, hasChangeType]` to `[project]`.

- [ ] **Step 5: Verify**

Run:

```bash
rg -n "testing-control|hasChangeType|CheckDoneOutline|useWorkItemType" apps/web/core/components/workspace/sidebar/project-navigation.tsx apps/web/core/components/navigation/use-navigation-items.ts; echo "exit=$?"
```

Expected: no matches, `exit=1`.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/workspace/sidebar/project-navigation.tsx apps/web/core/components/navigation/use-navigation-items.ts
git commit -m "feat(web): hide testing-control project nav"
```

---

### Task 4: Issue detail — remove Submit-to-TCB button

**Files:**

- Modify: `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx:28,155`

- [ ] **Step 1: Delete the import**

Delete line 28:

```tsx
import { ChangeTcbControl } from "@/components/reviews/change-tcb-control";
```

- [ ] **Step 2: Delete the render**

Delete line 155 (inside the quick-actions row, between the copy-link Tooltip and `IssueWarRoomButton`):

```tsx
<ChangeTcbControl workspaceSlug={workspaceSlug} projectId={projectId} issueId={issueId} />
```

After deletion the row must read:

```tsx
<IssueWarRoomButton workspaceSlug={workspaceSlug} projectId={projectId} issueId={issueId} />
```

directly after the copy-link `Tooltip` block.

- [ ] **Step 3: Verify**

Run:

```bash
rg -n "ChangeTcbControl|change-tcb" apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx; echo "exit=$?"
```

Expected: no matches, `exit=1`. File `apps/web/core/components/reviews/change-tcb-control.tsx` must still exist (do NOT delete it):

```bash
test -f apps/web/core/components/reviews/change-tcb-control.tsx && echo "kept"
```

Expected: `kept`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx
git commit -m "feat(web): hide submit-to-TCB in issue detail"
```

---

### Task 5: Notifications — render review items generically

**Files:**

- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`
- Modify: `apps/web/core/components/workspace-notifications/root.tsx`

- [ ] **Step 1: Clean imports in item.tsx**

Replace:

```tsx
import { CalendarOutline, CheckDoneOutline, ClockOutline } from "@makeplane/propel/icons";
```

with:

```tsx
import { CalendarOutline, ClockOutline } from "@makeplane/propel/icons";
```

Replace:

```tsx
import {
  getReleaseControlLink,
  getReviewSessionLink,
  getTestingControlLink,
  getWarRoomLink,
  REVIEW_BOARD_LABEL_KEYS,
  REVIEW_OUTCOME_CONFIG,
} from "@plane/constants";
```

with:

```tsx
import { getWarRoomLink } from "@plane/constants";
```

Delete:

```tsx
import { isReviewRequestNotification, isReviewSessionNotification } from "@/lib/review-notification";
```

- [ ] **Step 2: Delete review derived values in item.tsx**

Delete (lines 63-68):

```tsx
const reviewRequest = isReviewRequestNotification(notification?.data) ? notification?.data?.review_request : undefined;
const reviewSession = isReviewSessionNotification(notification?.data) ? notification?.data?.review_session : undefined;
```

- [ ] **Step 3: Delete the review deep-link branch in item.tsx**

Delete (lines 96-109):

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

- [ ] **Step 4: Generalize the guard + icon + title + subtitle in item.tsx**

Replace the guard (line 118):

```tsx
if (!scheduleRun && !warRoom && !reviewRequest && !reviewSession && (!notificationField || !projectId)) return <></>;
```

with:

```tsx
if (!scheduleRun && !warRoom && (!notificationField || !projectId)) return <></>;
```

Replace the avatar branch (lines 147-149):

```tsx
          ) : reviewRequest || reviewSession ? (
            <CheckDoneOutline className="h-5 w-5 text-tertiary" />
          ) : null}
```

with:

```tsx
          ) : null}
```

Replace the title branch — delete the two review arms (lines 168-183):

```tsx
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
              ) : (
```

with:

```tsx
              ) : (
```

Replace the subtitle branch — delete (lines 218-219):

```tsx
              ) : reviewRequest || reviewSession ? (
                <span>{t(REVIEW_BOARD_LABEL_KEYS[(reviewRequest ?? reviewSession)!.board_type])}</span>
```

with nothing (so `warRoom` arm falls straight through to the default `NotificationContent` arm).

- [ ] **Step 5: Remove the review detail pane in root.tsx**

Delete the import (line 25):

```tsx
import { isReviewRequestNotification, isReviewSessionNotification } from "@/lib/review-notification";
```

Delete the import (line 29):

```tsx
import { ReviewInboxDetail } from "./review-detail";
```

Delete the derived values (lines 59-64):

```tsx
const selectedReviewRequest = isReviewRequestNotification(selectedNotification?.data)
  ? selectedNotification?.data?.review_request
  : undefined;
const selectedReviewSession = isReviewSessionNotification(selectedNotification?.data)
  ? selectedNotification?.data?.review_session
  : undefined;
```

Delete the pane branch (lines 126-132):

```tsx
          ) : (selectedReviewRequest || selectedReviewSession) && workspace_slug ? (
            <ReviewInboxDetail
              workspaceSlug={workspace_slug}
              reviewRequest={selectedReviewRequest}
              reviewSession={selectedReviewSession}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
```

After deletion the chain must read `selectedWarRoom ... : is_inbox_issue === true ...` directly.

- [ ] **Step 6: Verify no review references remain in the two edited files**

Run:

```bash
rg -n "review|Review|REVIEW|CheckDoneOutline|getReviewSessionLink|getReleaseControlLink|getTestingControlLink" apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx apps/web/core/components/workspace-notifications/root.tsx; echo "exit=$?"
```

Expected: no matches, `exit=1`. And the detail component must still exist:

```bash
test -f apps/web/core/components/workspace-notifications/review-detail.tsx && echo "kept"
```

Expected: `kept`.

- [ ] **Step 7: Typecheck + lint the web app**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: both PASS with 0 errors (unused-import errors mean a Step 1/5 import was missed — go back and delete it).

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx apps/web/core/components/workspace-notifications/root.tsx
git commit -m "feat(web): render review notifications generically"
```

---

### Task 6: Build + deploy prod + manual verification

**Files:** none (ops only).

- [ ] **Step 1: Rebuild web**

Run:

```bash
pnpm --filter=web build
```

Expected: build succeeds.

- [ ] **Step 2: Restart prod**

Run:

```bash
systemctl --user restart plane-web-prod.service
```

Per `AGENTS.md`, never enable `plane-web.service` (dev) and `plane-web-prod.service` at the same time — prod stays, dev stays stopped.

- [ ] **Step 3: Manual checks (5 items, all must hold)**

1. Workspace sidebar has no `Releases` and no `Release Control`.
2. Project sidebar + tabbed nav have no `Technical review` / `Testing control`.
3. Direct URLs `/:ws/releases`, `/:ws/releases/:id`, `/:ws/release-control`, `/:ws/release-control/sessions/:id`, `/:ws/projects/:p/testing-control`, `/:ws/projects/:p/testing-control/sessions/:id` all render 404.
4. Issue detail quick actions show no Submit-to-TCB control.
5. An existing `review_request`/`review_session` notification (if any) renders generically without crash and without deep-linking to a board/session.

- [ ] **Step 4: Rollback note (only if verification fails)**

```bash
git log --oneline -6
git revert <commit>   # revert only the failing task commit, or all 5
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

No DB rollback exists or is needed — no migration was touched.

---

## Self-Review

**1. Spec coverage:** Spec §3.1 → Task 1; §3.2 → Task 2; §3.3 → Task 3 (termasuk cleanup `hasChangeType`/`useWorkItemType` yang spec sebut implisit via "tidak ada import unused"); §3.4 → Task 4; §3.5 → Task 5 (import cleanup dibuat eksplisit); §6 testing → Task 5 Step 7 + Task 6; §7 rollout/rollback → Task 6. Semua terpetakan, tidak ada gap.

**2. Placeholder scan:** Tidak ada TBD/TODO/"mirip Task N"/"handle edge cases" — semua edit menampilkan kode before/after eksak dengan nomor baris terverifikasi 2026-10-09.

**3. Type consistency:** `hasChangeType` dihapus di kedua file nav beserta produsennya (`workItemTypes`, `fetchWorkItemTypes`, `useWorkItemType`, `useEffect` bila unused) dan konsumennya (`shouldRender`, deps array) — tidak ada referensi menggantung. `CheckDoneOutline` dihapus dari ketiga file yang kehilangannya (helper tidak pernah punya; nav x2; notif item). `getReviewSessionLink`/`getReleaseControlLink`/`getTestingControlLink`/`REVIEW_*` hanya dihapus dari `item.tsx` tempat branch-nya dihapus; helper link di `packages/constants/src/review.ts` tetap (non-goal). `ReviewInboxDetail` hanya dilepas dari `root.tsx`; file-nya tetap.
