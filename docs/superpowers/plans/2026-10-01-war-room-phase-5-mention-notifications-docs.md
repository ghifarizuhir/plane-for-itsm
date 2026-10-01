# War Room Phase 5 — Mention Notifications & Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `@mention` war-room notifications visible and clickable in the Inbox (list, card, right pane) and document the shipped feature in `docs/features/`.

**Architecture:** The api-rs mention insert already writes `notifications` rows with `entity_name = 'war_room'` (`war_room.rs:1897-1934`), but the notification list query only allows `('issue', 'ai_schedule_run')` (`notification.rs:266`) and the web card hides rows without `issue_activity`. This phase opens the allowlist, adds a typed `war_room` payload + type guard, adds a card branch that opens the room, and adds a minimal right-pane detail so a selected war-room notification never falls through to the work-item peek. Docs close the spec's rollout step 4.

**Tech Stack:** Rust/Axum/sqlx (api-rs), React 19 + MobX (`mobx-react`), `next/navigation` compat router, `@plane/types`, `@plane/constants`, `@plane/i18n`, Vitest, OxLint/oxfmt.

**Spec:** `docs/superpowers/specs/2026-09-30-war-room-design.md` — §4 "Notifikasi mention", §6 Rollout 4, §7 item 5.

---

## Pre-flight (read once before Task 1)

- Branch: `preview` (all Phase 1–4 work is committed there).
- Working dir for Rust commands: `apps/api-rs`. DB-backed tests need `DATABASE_URL` (default `postgres://plane:plane@localhost:5432/plane`).
- Web consumes **built** `dist/` of `@plane/types`, `@plane/i18n`, `@plane/constants`. After touching their `src/`, run their build before `pnpm --filter=web check:types` / `pnpm --filter=web build`:
  - `pnpm --filter @plane/types build`
  - `pnpm --filter @plane/i18n build` (stale `dist/` shipped raw i18n keys in Phase 3 — always rebuild)
  - `pnpm --filter @plane/constants build` (only if constants source changes; Phase 5 does not change it)
- `packages/*/dist` is gitignored — never stage it.
- i18n: before touching any `packages/i18n/src/locales/**/*.json`, read and follow the `translate` skill at `/home/ghifari/plane-for-itsm/.claude/skills/translate/SKILL.md`. Baseline drift in other namespaces is `18 missing, 32 stale` — the war-room namespace must end at **0 missing / 0 stale** and the baseline must not grow.
- Lint is strict: `oxlint --deny-warnings` runs on pre-commit, so any new warning blocks the commit. Do not use `.sort()` (use `orderBy` / `.toSorted()` with an oxlint disable only if TS lib rejects it, as in `ai-schedule.ts:192-193`).
- Commit after every task. Never commit `dist/`.
- Production rollout is Task 8 only; nothing in Tasks 1–7 changes runtime behavior on the tunnel.

## File Structure

| File                                                                                  | Task | Responsibility                                                            |
| ------------------------------------------------------------------------------------- | ---- | ------------------------------------------------------------------------- |
| `apps/api-rs/crates/api/src/routes/notification.rs`                                   | 1    | Allow `war_room` rows in the notification list query                      |
| `apps/api-rs/crates/api/tests/notification_test.rs`                                   | 1    | Regression test: list includes `war_room` (and still hides unknown types) |
| `packages/types/src/workspace-notifications.ts`                                       | 2    | `TNotificationWarRoom` + `TNotificationData.war_room`                     |
| `apps/web/core/lib/war-room-notification.ts`                                          | 3    | `isWarRoomNotification` type guard                                        |
| `apps/web/core/lib/war-room-notification.test.ts`                                     | 3    | Vitest for the guard                                                      |
| `packages/i18n/src/locales/*/war-room.json`                                           | 4    | `notification.mention` + `notification.dismiss` in 20 locales             |
| `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx` | 5    | Card branch: render + click opens the room                                |
| `apps/web/core/components/workspace-notifications/war-room-detail.tsx`                | 6    | Right-pane detail for a selected war-room notification                    |
| `apps/web/core/components/workspace-notifications/root.tsx`                           | 6    | Render `WarRoomInboxDetail` for war-room notifications                    |
| `docs/features/war-rooms.md`                                                          | 7    | Page doc (Approved)                                                       |
| `docs/features/README.md`, `docs/features/_backlog.md`, `docs/features/work-items.md` | 7    | Inventory + backlog sync                                                  |

---

### Task 1: Backend — include `war_room` in the notification list filter

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/notification.rs:266`
- Modify: `apps/api-rs/crates/api/tests/notification_test.rs:79-192`

- [ ] **Step 0: Commit this plan file**

```bash
git add docs/superpowers/plans/2026-10-01-war-room-phase-5-mention-notifications-docs.md
git commit -m "docs(plans): war room phase 5 mention notifications and docs plan"
```

- [ ] **Step 1: Extend the failing test**

In `apps/api-rs/crates/api/tests/notification_test.rs`, rename `list_includes_schedule_run_notifications` to `list_includes_known_entity_types` and add a `war_room` fixture to the existing loop:

```rust
#[tokio::test]
async fn list_includes_known_entity_types() {
```

Inside the fixture array (after the `ai_schedule_run` entry, before `mystery`):

```rust
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
```

Add the assertion after the `ai_schedule_run` assertion:

```rust
    assert!(
        names.contains(&"war_room"),
        "war room mention notifications must be listed"
    );
```

Update the unread-count expectation from `3` to `4`:

```rust
    assert_eq!(counts["total_unread_notifications_count"], json!(4));
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test notification_test list_includes_known_entity_types
```

Expected: FAIL with `war room mention notifications must be listed`.

- [ ] **Step 3: Widen the allowlist**

In `apps/api-rs/crates/api/src/routes/notification.rs`, change the `base_where` entity filter (line 266):

```rust
          AND n.entity_name IN ('issue', 'ai_schedule_run', 'war_room') \
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test notification_test
```

Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/notification.rs apps/api-rs/crates/api/tests/notification_test.rs
git commit -m "feat(api): list war room mention notifications in inbox"
```

---

### Task 2: Shared types — `TNotificationWarRoom`

**Files:**

- Modify: `packages/types/src/workspace-notifications.ts:39-51`

- [ ] **Step 1: Add the payload type and field**

In `packages/types/src/workspace-notifications.ts`, add above `TNotificationData`:

```ts
export type TNotificationWarRoom = {
  id: string;
  project_id: string;
  workspace_slug: string;
  name: string;
  sequence_id: number;
};
```

and add the optional field to `TNotificationData` (after `ai_schedule`):

```ts
export type TNotificationData = {
  issue?: TNotificationIssueLite | undefined;
  issue_activity?: {
    id: string | undefined;
    actor: string | undefined;
    field: string | undefined;
    issue_comment: string | undefined;
    verb: "created" | "updated" | "deleted";
    new_value: string | undefined;
    old_value: string | undefined;
  };
  ai_schedule?: TNotificationScheduleRun | undefined;
  war_room?: TNotificationWarRoom | undefined;
};
```

- [ ] **Step 2: Typecheck + build (web reads `dist/`)**

```bash
pnpm --filter @plane/types check:types
pnpm --filter @plane/types build
```

Expected: both exit 0.

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/workspace-notifications.ts
git commit -m "feat(types): war room notification payload"
```

---

### Task 3: Web helper — `isWarRoomNotification` (TDD)

**Files:**

- Create: `apps/web/core/lib/war-room-notification.ts`
- Test: `apps/web/core/lib/war-room-notification.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/war-room-notification.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { TNotificationData, TNotificationWarRoom } from "@plane/types";
import { isWarRoomNotification } from "./war-room-notification";

const warRoom: TNotificationWarRoom = {
  id: "8b0e6c3a-6f4f-4c2a-9a3f-0b1c2d3e4f50",
  project_id: "1c2d3e4f-5a6b-7c8d-9e0f-1a2b3c4d5e6f",
  workspace_slug: "acme",
  name: "Checkout down",
  sequence_id: 3,
};

describe("isWarRoomNotification", () => {
  it("accepts data carrying a war_room payload", () => {
    expect(isWarRoomNotification({ war_room: warRoom })).toBe(true);
  });

  it("rejects missing data, empty ids, and other notification kinds", () => {
    expect(isWarRoomNotification(undefined)).toBe(false);
    expect(isWarRoomNotification({})).toBe(false);
    expect(isWarRoomNotification({ war_room: { ...warRoom, id: "" } })).toBe(false);
    const scheduleRun: TNotificationData = {
      ai_schedule: {
        schedule_id: "schedule-1",
        run_id: "run-1",
        name: "Daily report",
        status: "success",
        finished_at: null,
      },
    };
    expect(isWarRoomNotification(scheduleRun)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
pnpm --filter=web test -- war-room-notification
```

Expected: FAIL — `Failed to resolve import "./war-room-notification"`.

- [ ] **Step 3: Write the minimal implementation**

Create `apps/web/core/lib/war-room-notification.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TNotificationData, TNotificationWarRoom } from "@plane/types";

export const isWarRoomNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { war_room: TNotificationWarRoom } =>
  typeof data?.war_room?.id === "string" && data.war_room.id.length > 0;
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
pnpm --filter=web test -- war-room-notification
```

Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/war-room-notification.ts apps/web/core/lib/war-room-notification.test.ts
git commit -m "feat(web): war room notification type guard"
```

---

### Task 4: i18n — notification strings for 20 locales

**Files:**

- Modify: `packages/i18n/src/locales/en/war-room.json` (source of truth)
- Modify: `packages/i18n/src/locales/{cs,de,es,fr,id,it,ja,ka-ge,ko,pl,pt-BR,ro,ru,sk,tr-TR,ua,vi-VN,zh-CN,zh-TW}/war-room.json`
- No change to `packages/i18n/src/constants/namespaces.ts` (namespace already exists)

- [ ] **Step 1: Add the English keys**

In `packages/i18n/src/locales/en/war-room.json`, insert after the top-level `"open": "Open war room",` line:

```json
    "notification": {
      "mention": "{name} mentioned you in {room}",
      "dismiss": "Dismiss"
    },
```

The card renders this as one ICU string on purpose (word order differs across locales); `{name}` and `{room}` are the only placeholders.

- [ ] **Step 2: Translate the two keys into the 19 target locales**

Follow the `translate` skill exactly (`.claude/skills/translate/SKILL.md`). For each locale file: keep every existing key/value untouched, add the `notification` section with `mention` and `dismiss`. Rules for these two keys:

- `mention` must keep **both** ICU placeholders `{name}` and `{room}`, in the grammatically correct position for the locale; do not reorder placeholders into English word order if the locale differs (e.g. Japanese/Korean/Turkish).
- `dismiss` = the locale's standard UI verb for closing a detail pane (not "delete", not "archive").
- Never leave English in a non-English file. `WR-` is a do-not-translate token (not present in these two keys, but keep the rule in mind).
- Keep the file's existing JSON formatting (2-space indent, same key order as `en`).

- [ ] **Step 3: Generate types, build, and check sync**

```bash
pnpm --filter @plane/i18n generate:types
pnpm --filter @plane/i18n build
pnpm --filter @plane/i18n run sync:check
```

Expected: `generate:types` + `build` exit 0; `sync:check` reports **0 missing / 0 stale for war-room.json in every locale**. Pre-existing drift in other namespaces (`18 missing, 32 stale`) is unrelated and must not grow.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "i18n(web): war room mention notification strings for all locales"
```

---

### Task 5: Notification card — war-room branch

**Files:**

- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`

- [ ] **Step 1: Add imports**

Replace the import block (lines 7-24) with:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
import { useRouter } from "next/navigation";
import { CalendarOutline, ClockOutline } from "@makeplane/propel/icons";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Badge } from "@plane/propel/badge";
import { Row } from "@plane/ui";
import { cn, calculateTimeAgo, renderFormattedDate, renderFormattedTime, getFileURL } from "@plane/utils";
// hooks
import { useWorkspaceNotifications } from "@/hooks/store/notifications";
import { useNotification } from "@/hooks/store/notifications/use-notification";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { isScheduleRunNotification, scheduleRunNotificationText, scheduleStatusLabel } from "@/lib/ai-schedule";
import { isWarRoomNotification } from "@/lib/war-room-notification";
// local imports
import { NotificationContent } from "./content";
import { NotificationOption } from "./options";
```

- [ ] **Step 2: Add hooks and derived value**

After `const { getWorkspaceBySlug } = useWorkspace();` add:

```tsx
// router
const router = useRouter();
const { t } = useTranslation();
```

After the `scheduleRun` derived value (line 46) add:

```tsx
const warRoom = isWarRoomNotification(notification?.data) ? notification?.data?.war_room : undefined;
```

- [ ] **Step 3: Open the room on click**

In `handleNotificationClick`, insert before the schedule-run early return (line 65-66):

```tsx
// war room mentions open the room page itself
if (warRoom) {
  router.push(getWarRoomLink(workspaceSlug, warRoom.project_id, warRoom.id));
  return;
}
```

- [ ] **Step 4: Let war-room rows render**

Change the guard (line 75) from:

```tsx
if (!scheduleRun && (!notificationField || !projectId)) return <></>;
```

to:

```tsx
if (!scheduleRun && !warRoom && (!notificationField || !projectId)) return <></>;
```

- [ ] **Step 5: Render the mention title**

Change the title ternary from:

```tsx
              {scheduleRun ? (
                <span className="font-medium text-primary">{scheduleRun.name}</span>
              ) : (
                <NotificationContent
```

to:

```tsx
              {scheduleRun ? (
                <span className="font-medium text-primary">{scheduleRun.name}</span>
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
              ) : (
                <NotificationContent
```

- [ ] **Step 6: Render the room identifier subtitle**

Change the subtitle ternary from:

```tsx
              ) : (
                <>
                  {notification?.data?.issue?.identifier}-{notification?.data?.issue?.sequence_id}&nbsp;
                  {notification?.data?.issue?.name}
                </>
              )}
```

to:

```tsx
              ) : warRoom ? (
                <span>WR-{warRoom.sequence_id}</span>
              ) : (
                <>
                  {notification?.data?.issue?.identifier}-{notification?.data?.issue?.sequence_id}&nbsp;
                  {notification?.data?.issue?.name}
                </>
              )}
```

- [ ] **Step 7: Typecheck**

```bash
pnpm --filter @plane/types build && pnpm --filter @plane/i18n build && pnpm --filter=web check:types
```

Expected: exit 0. (The i18n key is not in `TTranslationKeys` until Task 4's `generate:types` has run — if `check:types` fails on the key, run `pnpm --filter @plane/i18n generate:types` first.)

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx
git commit -m "feat(web): open war room from mention notification card"
```

---

### Task 6: Right pane — `WarRoomInboxDetail`

**Files:**

- Create: `apps/web/core/components/workspace-notifications/war-room-detail.tsx`
- Modify: `apps/web/core/components/workspace-notifications/root.tsx`

- [ ] **Step 1: Create the detail component**

Create `apps/web/core/components/workspace-notifications/war-room-detail.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRouter } from "next/navigation";
// plane imports
import { getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TNotificationWarRoom } from "@plane/types";

type Props = {
  workspaceSlug: string;
  warRoom: TNotificationWarRoom;
  embedRemoveCurrentNotification: () => void;
};

export function WarRoomInboxDetail({ workspaceSlug, warRoom, embedRemoveCurrentNotification }: Props) {
  // router
  const router = useRouter();
  const { t } = useTranslation();

  const openWarRoom = () => {
    router.push(getWarRoomLink(workspaceSlug, warRoom.project_id, warRoom.id));
  };

  return (
    <div className="h-full w-full overflow-y-auto p-4">
      <div className="mx-auto max-w-2xl space-y-3">
        <h3 className="text-base font-semibold break-words text-primary">{warRoom.name}</h3>
        <p className="text-xs text-tertiary">WR-{warRoom.sequence_id}</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="secondary" onClick={openWarRoom}>
            {t("war_room.open")}
          </Button>
          <Button size="sm" variant="ghost" onClick={embedRemoveCurrentNotification}>
            {t("war_room.notification.dismiss")}
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Wire the right pane branch**

In `apps/web/core/components/workspace-notifications/root.tsx`, add the import next to the schedule-run import:

```tsx
import { WarRoomInboxDetail } from "./war-room-detail";
```

and the lib import after `isScheduleRunNotification`:

```tsx
import { isWarRoomNotification } from "@/lib/war-room-notification";
```

Add the derived value after `selectedScheduleRun` (line 49-51):

```tsx
const selectedWarRoom = isWarRoomNotification(selectedNotification?.data)
  ? selectedNotification.data.war_room
  : undefined;
```

Add the render branch between the schedule-run branch and the inbox branch:

```tsx
          {selectedScheduleRun && workspace_slug ? (
            <ScheduleRunInboxDetail
              workspaceSlug={workspace_slug}
              scheduleRun={selectedScheduleRun}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
          ) : selectedWarRoom && workspace_slug ? (
            <WarRoomInboxDetail
              workspaceSlug={workspace_slug}
              warRoom={selectedWarRoom}
              embedRemoveCurrentNotification={embedRemoveCurrentNotification}
            />
          ) : is_inbox_issue === true && workspace_slug && project_id && issue_id ? (
```

- [ ] **Step 3: Typecheck**

```bash
pnpm --filter=web check:types
```

Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/workspace-notifications/war-room-detail.tsx apps/web/core/components/workspace-notifications/root.tsx
git commit -m "feat(web): war room notification detail pane"
```

---

### Task 7: Docs — `docs/features/war-rooms.md` + inventory sync

**Files:**

- Create: `docs/features/war-rooms.md`
- Modify: `docs/features/README.md:47-54` (ITSM inventory table + note) and its changelog
- Modify: `docs/features/_backlog.md:7` (Incident Management bullet)
- Modify: `docs/features/work-items.md:20` (Missing note)

- [ ] **Step 1: Write the page doc**

Create `docs/features/war-rooms.md`:

```markdown
# War Rooms

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/war-rooms` (list) · `:workspaceSlug/projects/:projectId/war-rooms/:warRoomId` (detail) — lihat `apps/web/app/routes/core.ts:196-207`
Share: CORE

## Intent

Ruang koordinasi insiden per project: peta blast radius, chat realtime, runbook, work item terkait, catatan, dan aktivitas dalam satu halaman — supaya respons insiden tidak tersebar di chat dan issue.

## Current State (snapshot kode)

- Halaman: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/**` (list + detail).
- Komponen: `apps/web/core/components/war-rooms/**` — list/board + summary, create modal, room header (lifecycle + join/leave), `service-map` (blast radius), chat, context panel (Notes/Work items/Runbook/Activity/People), modal resolve/archive/delete/edit details.
- Store/service/hook: `apps/web/core/store/war-room.store.ts`, `apps/web/core/services/war-room.service.ts`, `apps/web/core/hooks/use-war-room-socket.ts`.
- Backend: `apps/api-rs/crates/api/src/routes/war_room.rs`; migrasi `apps/api-rs/migrations/0011_war_rooms.sql`.
- Realtime: controller `war-rooms` di `apps/live`, relay event lewat Redis `war-room:events`.
- Working: list + tab status + search + summary; create (primary incident wajib, 409 bila sudah ada room aktif untuk incident itu); detail map-first; chat (kirim/edit/hapus, mention `@{user_id}`, typing, presence, reconnect); runbook; notes autosave; work items; activity; participants + role; lifecycle transitions; notifikasi mention in-app.
- Stub: —
- Missing (ITSM fork): — (Problem/Change/Request/Knowledge terpisah belum ada; ide diparkir di [`_backlog.md`](./_backlog.md)).

## Primary View

- Layout B (map first): panel kiri peta blast radius read-only (canvas `service-graph-canvas.tsx`; node affected + 1-hop neighbor + legenda); panel kanan chat + tab konteks.
- Header: identifier `WR-{sequence_id}`, nama, badge severity/status, timer elapsed, aksi lifecycle (Resolve/Reopen/Archive), edit details, join/leave, menu delete.
- List: board row per room (identifier, nama, severity, status, incident utama, peserta, waktu) + summary chip (active / SEV1–2 / resolved 7d).

## Actions

| Action                         | Trigger               | Permission                      | State required                                 |
| ------------------------------ | --------------------- | ------------------------------- | ---------------------------------------------- |
| Create                         | Toolbar / empty state | Project member                  | —                                              |
| Open room                      | Row click             | Project member; guest read-only | —                                              |
| Send message                   | Composer Enter        | Project member                  | status != archived                             |
| Edit/delete message            | Hover pesan sendiri   | Author                          | status != archived                             |
| Mention                        | `@` di composer       | Project member                  | Mentioned harus workspace member               |
| Change status                  | Header lifecycle menu | Project member                  | transisi valid (`WAR_ROOM_STATUS_TRANSITIONS`) |
| Resolve                        | Header                | Project member                  | status active/monitoring                       |
| Reopen                         | Header                | Project member                  | status resolved                                |
| Archive                        | Header/menu           | Project member                  | status != archived                             |
| Delete                         | Header menu           | Project member                  | konfirmasi modal                               |
| Edit details                   | Header menu           | Project member                  | status != archived                             |
| Link/unlink services           | Header map            | Project member                  | status != archived                             |
| Add participant / change role  | People tab            | Project member                  | status != archived                             |
| Join/Leave                     | Header / People tab   | Project member                  | status != archived                             |
| Edit notes                     | Notes tab             | Project member                  | status != archived                             |
| Runbook toggle/add/edit/delete | Runbook tab           | Project member                  | status != archived                             |
| Link/unlink work items         | Work items tab        | Project member                  | status != archived                             |

## Filters / Sort / Search

- Tabs: Active (`active,monitoring`), Resolved (`resolved`), All.
- Search: nama room / identifier + nama incident (server-side `search`).
- Sort: terbaru dulu (`-created_at`); summary chip dari endpoint `summary/`.

## Detail View

- Panel kiri: peta blast radius (affected + 1-hop neighbor, dua arah dependency).
- Panel kanan: chat + tab Notes (rich-text autosave 1 detik), Work items (primary incident pinned), Runbook (progress + checklist), Activity (feed kronologis live), People (peserta + role + indikator online).
- Chat: pagination `before_id`, optimistic send + `client_id`, edit/delete author-only, typing/presence, banner reconnect.

## Permissions

| Role           | Create | Read | Update | Delete                   |
| -------------- | ------ | ---- | ------ | ------------------------ |
| Project member | ✅     | ✅   | ✅     | ✅ (room; pesan sendiri) |
| Guest          | ❌     | ✅   | ❌     | ❌                       |
| Non-member     | ❌     | ❌   | ❌     | ❌                       |

## Empty / Loading / Error

- Empty: list → pesan + CTA create; map tanpa services → placeholder; chat/runbook/activity/notes/work items punya empty state masing-masing.
- Loading: skeleton list/room; spinner saat load pesan lama.
- Error: banner + retry (`war_room.load_error`, `chat.load_failed`, `activity.load_failed`, dst.); 409 duplicate active room → tawaran buka room existing; aksi saat archived ditolak (`errors.room_archived`).

## Realtime & Notifikasi

- Socket: `ws(s)://<live>/war-rooms/<roomId>?workspaceSlug=&projectId=&token=`; event `message.created|updated|deleted`, `room.changed`, `activity.created`, `typing`, `presence.joined|left`; publish best-effort via Redis `war-room:events`.
- Status koneksi: `connecting|connected|reconnecting|disabled`; banner `chat.reconnect_banner` hanya saat gagal jaringan; auth gagal (close 4403) → `disabled` tanpa banner.
- Mention: server mem-parse `@{user_id}` (workspace member saja), mengisi `mentions`, dan menulis row `notifications` (`entity_name = 'war_room'`, `sender = 'in_app:war_room:mentioned'`) kecuali author; klik notifikasi membuka room.

## Edge Cases

- Incident dihapus/archived: room tetap; chip incident menampilkan state archived.
- Maksimum satu commander per room (partial unique); room tanpa commander valid.
- Service dihapus: link soft-delete; peta kehilangan node (fallback empty state).
- Dua create untuk incident sama: unique index partial → 409 `active_war_room_exists`.
- Kirim pesan saat archived: 409 `room_archived`.
- WS putus saat kirim: REST tetap sukses; pesan ditandai terkirim + banner reconnect.
- Presence multi-instance: best-effort, bukan locking.
- Rate limit pesan memakai middleware existing; typing di-throttle client (maks 1 event/2 detik).
- Name > 255 karakter: 400 server; UI `maxLength`.

## API Touchpoints

Base `/api/workspaces/:slug/projects/:project_id/war-rooms/`: list/create, `summary/`, `:pk/` (GET/PATCH/DELETE), `links/`, `services/`, `issues/`, `participants/`, `runbook/`, `messages/`, `events/` — lihat `apps/api-rs/crates/api/src/routes/war_room.rs` dan `docs/superpowers/specs/2026-09-30-war-room-design.md`.

## Changelog

| Date       | Change                                                                                                                              |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| 2026-10-01 | Doc dibuat — Fase 1–4 sudah shipped (backend, chat realtime, list+create, room page); Fase 5 menambah notifikasi mention + doc ini. |
```

- [ ] **Step 2: Add the inventory row and update the note**

In `docs/features/README.md`, add to the ITSM table (after the CMDB row, before the backlog row):

```markdown
| War Rooms | [`war-rooms.md`](./war-rooms.md) | ✅ Doc done | Project page `:projectId/war-rooms`; realtime via `apps/live`; tabel `war_rooms` (api-rs) |
```

Replace the blockquote at line 54 with:

```markdown
> `cmdb.md` adalah proposal pertama (sebelum ada kode). `war-rooms.md` mengikuti aturan normal: ditulis setelah Fase 1–4 diimplementasikan. Proposal ITSM lain tetap diparkir di [`_backlog.md`](./_backlog.md).
```

Add a changelog row at the bottom of the file:

```markdown
| 2026-10-01 | tambah `war-rooms.md` ke inventory ITSM (Fase 1–4 shipped) |
```

- [ ] **Step 3: Update the backlog bullet**

In `docs/features/_backlog.md`, append to the Incident Management bullet (line 7):

```markdown
— **War room sudah diimplementasikan** (route `:workspaceSlug/projects/:projectId/war-rooms`, tabel `war_rooms`, chat realtime; lihat [`war-rooms.md`](./war-rooms.md)).
```

- [ ] **Step 4: Update the work-items missing note**

In `docs/features/work-items.md`, replace line 20:

```markdown
- Missing (ITSM fork): tidak ada — overlay ITSM (incident/priority SLA, war room) belum ada di kode; ide diparkir di [`_backlog.md`](./_backlog.md).
```

with:

```markdown
- Missing (ITSM fork): incident/priority SLA belum ada di kode; **war room sudah ada** — lihat [`war-rooms.md`](./war-rooms.md); ide incident diparkir di [`_backlog.md`](./_backlog.md).
```

- [ ] **Step 5: Commit**

```bash
git add docs/features/war-rooms.md docs/features/README.md docs/features/_backlog.md docs/features/work-items.md
git commit -m "docs(features): war rooms page doc and inventory sync"
```

---

### Task 8: Full verification + production rollout

**Files:** none (verification only)

- [ ] **Step 1: Run the full check suite**

```bash
pnpm --filter @plane/types build
pnpm --filter @plane/i18n build
pnpm --filter=web test
pnpm --filter=web check:types
pnpm check:lint
pnpm check:format
pnpm --filter @plane/i18n run sync:check
```

Expected: tests pass (Phase 4 baseline `14 passed, 262 passed` plus the 2 new helper tests); types/lint/format exit 0; `sync:check` shows 0 war-room drift and the pre-existing `18 missing, 32 stale` baseline unchanged.

- [ ] **Step 2: Run the Rust tests**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test notification_test
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1
```

Working dir: `apps/api-rs`. Expected: all PASS.

- [ ] **Step 3: Rebuild and restart api-rs (detached — LTO link takes 10+ min with no output)**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll the log until the build finishes, then verify:

```bash
curl -s -o /dev/null -w "api:%{http_code}\n" http://localhost:8000/health
```

Expected: `api:200`.

- [ ] **Step 4: Restart live (it holds the Redis connection that is recreated with the stack)**

```bash
systemctl --user restart plane-live.service && sleep 8 && curl -s -o /dev/null -w "live:%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `live:200`.

- [ ] **Step 5: Rebuild web prod and restart**

```bash
VITE_API_BASE_URL=https://api.terraline.space VITE_LIVE_BASE_URL=https://api.terraline.space pnpm --filter=web build
systemctl --user restart plane-web-prod.service && sleep 8 && curl -s -o /dev/null -w "prod:%{http_code}\n" http://localhost:3000/
```

Expected: build exit 0; `prod:200`.

- [ ] **Step 6: Manual smoke via the tunnel**

1. Open a war room, send `@{teammate}` in chat from account A.
2. Log in as the teammate (account B) and open Inbox → **Mentions** tab.
3. Expected: the card shows "<A> mentioned you in <room>", subtitle `WR-<n>`, unread dot; clicking it opens the room page; the notification is marked read.
4. Select the notification without navigating (or navigate back) — the right pane shows the room name, `WR-<n>`, "Open war room" and "Dismiss".
5. Confirm no "Reconnecting…" banner appears (live socket still `connected`).

- [ ] **Step 7: Final commit if anything was fixed during verification**

```bash
git status
# stage only the intended files, then:
git commit -m "fix(web): <describe the fix>"
```
