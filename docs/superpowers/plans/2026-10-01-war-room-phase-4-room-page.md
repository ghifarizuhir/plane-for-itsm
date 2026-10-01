# War Room — Phase 4 (Room Page) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the full war room page: header/lifecycle controls, blast-radius service map, realtime chat with mentions, and the Notes / Work items / Runbook / Activity / People context tabs.

**Architecture:** Pure frontend work in `apps/web` on top of the frozen Phase 1–2 API and Phase 2 live relay. All server state lives in the existing MobX `WarRoomStore`; REST persists through `WarRoomService`; a single `useWarRoomSocket` hook per room reduces live `{kind, data}` events into the store; panels are presentational components reading the store. The services graph canvas is extracted from `service-graph.tsx` into a reusable read-only `ServiceGraphCanvas`.

**Tech Stack:** React 19 + MobX (`mobx-react`, `mobx-utils`), React Router (`next/navigation` compat), `@xyflow/react` + dagre, `@plane/editor` RichTextEditor, `@plane/ui` (ModalCore, CustomSelect, CustomMenu), `@makeplane/propel` (Avatar, AvatarGroup, Input, Button), `@plane/i18n`, Vitest, OxLint, oxfmt.

---

## Context you need before starting

### Frozen backend contract (Phase 1–2, do not change)

Prefix: `/api/workspaces/:slug/projects/:project_id/war-rooms/` — **trailing slash required** on every path.

| Method | Path                                 | Body / Query                                                  | Notes                                                                                              |
| ------ | ------------------------------------ | ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| GET    | `/`                                  | `status` (CSV), `severity` (CSV), `q`                         | list                                                                                               |
| GET    | `/summary/`                          | —                                                             | `{active, sev1_2, resolved_7d}`                                                                    |
| GET    | `/:pk/`                              | —                                                             | full detail                                                                                        |
| PATCH  | `/:pk/`                              | `{name?, severity?, status?, description_html?, notes_html?}` | returns full detail; 409 `room_archived` when archived                                             |
| DELETE | `/:pk/`                              | —                                                             | 204, idempotent                                                                                    |
| POST   | `/:pk/services/`                     | `{service_ids: string[]}`                                     | `201 {linked}`                                                                                     |
| DELETE | `/:pk/services/:service_id/`         | —                                                             | 204                                                                                                |
| POST   | `/:pk/issues/`                       | `{issue_ids: string[]}`                                       | `201 {linked}`; 400 `primary_issue_not_linkable`                                                   |
| DELETE | `/:pk/issues/:issue_id/`             | —                                                             | 204                                                                                                |
| POST   | `/:pk/participants/`                 | `{member_id, role?}`                                          | `201` participant; role default `responder`; promoting `commander` demotes the previous one        |
| PATCH  | `/:pk/participants/:participant_id/` | `{role}`                                                      | `200` participant                                                                                  |
| DELETE | `/:pk/participants/:participant_id/` | —                                                             | 204                                                                                                |
| POST   | `/:pk/runbook-items/`                | `{title}`                                                     | `201` item                                                                                         |
| PATCH  | `/:pk/runbook-items/:item_id/`       | `{title?, is_done?}`                                          | `200` item                                                                                         |
| DELETE | `/:pk/runbook-items/:item_id/`       | —                                                             | 204                                                                                                |
| GET    | `/:pk/messages/`                     | `before_id?`, `limit?` (default 50, max 200)                  | response is **oldest → newest**; pass the **first (oldest)** item's id as `before_id` to page back |
| POST   | `/:pk/messages/`                     | `{body, client_id?}`                                          | `201` message incl. `client_id` echo                                                               |
| PATCH  | `/:pk/messages/:message_id/`         | `{body}`                                                      | author-only, 403 otherwise                                                                         |
| DELETE | `/:pk/messages/:message_id/`         | —                                                             | author-only, 204                                                                                   |
| GET    | `/:pk/events/`                       | `before_id?`, `limit?`                                        | response is **newest → oldest**; pass the first (newest) item's id as `before_id` to page back     |

Status transitions (server-enforced, `archived` terminal):
`active → monitoring|resolved|archived`, `monitoring → active|resolved|archived`, `resolved → active|archived`.

Permissions: reads work for guests (`gate_member`); all writes are `gate_writer` (project Admin/Member or workspace admin) and return `403 {"error":"You don't have the required permissions."}` for guests. Chat edit/delete additionally author-only. Archived rooms reject all writes with `409 {"error":"room_archived"}`.

### Frozen realtime contract (Phase 2, do not change)

Browser URL: `ws(s)://<LIVE_BASE_URL|window.origin><LIVE_BASE_PATH>/war-rooms/<roomId>?workspaceSlug=…&projectId=…&token=<JSON.stringify({id: currentUserId})>`. Cookie rides on the handshake headers; auth failure closes with code `4403`.

Relay envelope: `{ kind, data }`. Kinds the browser can receive:

- `message.created` → full message incl. `client_id`
- `message.updated` → full message (no `client_id`)
- `message.deleted` → `{id, war_room_id}`
- `activity.created` → `{id, actor_id, event_type, payload, created_at}`
- `room.changed` → `{reasons: string[]}` — reasons: `created`, `deleted`, `details`, `notes`, `status`, `severity`, `links`, `participants`, `runbook`
- `typing` → `{user_id, is_typing}`
- `presence.joined` / `presence.left` → `{user_id, name}`
- `pong` → no payload

Client → server: `{"type":"typing","is_typing":true|false}`, `{"type":"ping"}`.

### Conventions & commands (this repo)

- Web tests: `pnpm --filter=web test` (Vitest, files `*.test.ts` next to source).
- Types: `pnpm --filter=web check:types`. Lint: `pnpm check:lint`. Format: `pnpm fix:format` then `pnpm check:format`.
- i18n: after touching `packages/i18n`: `pnpm --filter @plane/i18n generate:types`, then **`pnpm --filter @plane/i18n build`** (web consumes `dist/` — forgetting this build ships raw i18n keys; this bit us in Phase 3), then `pnpm --filter @plane/i18n run sync:check`.
- Web prod build (tunnel): `VITE_API_BASE_URL=https://api.terraline.space pnpm --filter=web build` then `systemctl --user restart plane-web-prod.service`; verify `curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/`.
- Live health: `curl http://localhost:3100/live/health/`; API health: `curl http://localhost:8000/health`.
- Write guest gate on the web side: `const canWrite = allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT) && room.status !== "archived";` (`useUserPermissions` from `@/hooks/store/user`).

### Error mapping used by every panel

All store mutations throw the normalized `Error` from `WarRoomService` (it carries `.error`, `.detail`, `.war_room_id`). Panels show:
`room_archived` → `t("war_room.errors.room_archived")`, `403` → `t("war_room.errors.forbidden")`, otherwise the panel-specific failure key or `t("war_room.errors.generic")`.

### File structure (created/modified by this plan)

```
packages/i18n/src/locales/*/war-room.json                 Task 1 (20 locales)
packages/types/src/war-room/core.ts                       Task 2
packages/constants/src/war-room.ts                        Task 3
apps/web/core/services/war-room.helpers.ts                Task 4
apps/web/core/services/war-room.helpers.test.ts           Task 4
apps/web/core/services/war-room.service.ts                Task 5
apps/web/core/store/war-room.store.ts                     Task 6
apps/web/core/store/war-room.store.test.ts                Task 6
apps/web/core/hooks/use-war-room-socket.ts                Task 7
apps/web/core/components/services/graph/service-graph-canvas.tsx   Task 8 (new)
apps/web/core/components/services/graph/service-graph.tsx          Task 8 (rewrite)
apps/web/core/components/services/graph/service-node.tsx           Task 8 (dimmed)
apps/web/core/components/war-rooms/room/service-map.tsx            Task 9
apps/web/core/components/war-rooms/room/context/notes.tsx          Task 10
apps/web/core/components/war-rooms/room/chat/root.tsx              Task 11
apps/web/core/components/war-rooms/room/chat/message-item.tsx      Task 11
apps/web/core/components/war-rooms/room/chat/composer.tsx          Task 11
apps/web/core/components/war-rooms/room/context/work-items.tsx     Task 12
apps/web/core/components/war-rooms/room/context/runbook.tsx        Task 13
apps/web/core/components/war-rooms/room/context/activity.tsx       Task 14
apps/web/core/components/war-rooms/room/context/people.tsx         Task 15
apps/web/core/components/war-rooms/room/context/root.tsx           Task 16
apps/web/core/components/war-rooms/room/header/root.tsx            Task 17
apps/web/core/components/war-rooms/room/header/status-control.tsx  Task 17
apps/web/core/components/war-rooms/room/header/resolve-modal.tsx   Task 17
apps/web/core/components/war-rooms/room/header/confirm-modal.tsx   Task 17
apps/web/core/components/war-rooms/room/header/edit-details-modal.tsx Task 17
apps/web/core/components/war-rooms/room/root.tsx                   Task 18 (rewrite)
apps/web/core/components/war-rooms/room/room-overview.tsx          Task 18 (delete)
apps/web/core/components/war-rooms/index.ts                        Task 18
```

---

### Task 1: i18n keys for the room page (20 locales)

**Files:**

- Modify: `packages/i18n/src/locales/en/war-room.json` (source of truth)
- Modify: `packages/i18n/src/locales/{cs,de,es,fr,id,it,ja,ka-ge,ko,pl,pt-BR,ro,ru,sk,tr-TR,ua,vi-VN,zh-CN,zh-TW}/war-room.json`
- No changes to `packages/i18n/src/constants/namespaces.ts` (the `war-room` namespace already exists)

- [ ] **Step 1: Replace `packages/i18n/src/locales/en/war-room.json` with this exact content**

```json
{
  "war_room": {
    "title": "War rooms",
    "add": "Create war room",
    "open": "Open war room",
    "load_error": {
      "title": "Couldn't load war rooms",
      "description": "Something went wrong while fetching war rooms. Please try again.",
      "retry": "Retry"
    },
    "tabs": {
      "active": "Active",
      "resolved": "Resolved",
      "all": "All"
    },
    "search": {
      "placeholder": "Search war rooms"
    },
    "summary": {
      "active": "{count} active",
      "sev1_2": "{count} SEV1–2",
      "resolved_7d": "{count} resolved in 7d"
    },
    "status_values": {
      "active": "Active",
      "monitoring": "Monitoring",
      "resolved": "Resolved",
      "archived": "Archived"
    },
    "severity_values": {
      "sev1": "SEV1",
      "sev2": "SEV2",
      "sev3": "SEV3",
      "sev4": "SEV4"
    },
    "roles": {
      "commander": "Incident Commander",
      "comms": "Comms",
      "scribe": "Scribe",
      "responder": "Responder"
    },
    "fields": {
      "primary_incident": "Primary incident",
      "affected_services": "Affected services",
      "participants": "Participants",
      "messages": "Messages",
      "started": "Started",
      "elapsed": "Elapsed"
    },
    "empty_state": {
      "title": "No war rooms yet",
      "description": "Open a war room to coordinate incident response.",
      "no_matches": {
        "title": "No matching war rooms",
        "description": "No war rooms match the current filters. Try adjusting or clearing them."
      }
    },
    "create": {
      "title": "Create war room",
      "incident": "Primary incident",
      "select_incident": "Select incident",
      "incident_selected": "Incident selected",
      "incident_required": "Select the primary incident to continue",
      "name": "Name",
      "name_placeholder": "War room name",
      "severity": "Severity",
      "severity_auto": "From incident priority",
      "services": "Affected services",
      "description": "Description",
      "submit": "Create war room",
      "duplicate_title": "Active war room already exists",
      "duplicate_description": "An active war room already exists for this incident.",
      "open_existing": "Open war room"
    },
    "detail": {
      "not_found_title": "War room does not exist",
      "not_found_description": "The war room you are looking for does not exist or has been deleted.",
      "view_other_rooms": "View other war rooms",
      "back": "Back to war rooms"
    },
    "header": {
      "rename_placeholder": "War room name",
      "more_actions": "More actions",
      "edit_details": "Edit details",
      "archive": "Archive",
      "delete": "Delete",
      "resolve": "Resolve",
      "reopen": "Reopen",
      "join": "Join",
      "leave": "Leave",
      "status_control": "Change status",
      "severity_control": "Change severity",
      "online": "{count} online",
      "you": "You"
    },
    "archived_notice": "This war room is archived and read-only.",
    "resolve_modal": {
      "title": "Resolve war room",
      "description": "Mark this incident as resolved. The timer stops and the room stays available for review.",
      "note_label": "Resolution note",
      "note_placeholder": "Summarize the resolution for the notes (optional)",
      "confirm": "Resolve"
    },
    "archive_modal": {
      "title": "Archive war room",
      "description": "Archived rooms are read-only and terminal. They stay visible in the All tab.",
      "confirm": "Archive"
    },
    "delete_modal": {
      "title": "Delete war room",
      "description": "This permanently removes the war room, its chat, runbook and links. The activity feed is kept.",
      "confirm": "Delete war room"
    },
    "edit_details_modal": {
      "title": "Edit war room details",
      "name": "Name",
      "description": "Description",
      "save": "Save changes"
    },
    "map": {
      "title": "Affected services",
      "empty_title": "No affected services yet",
      "empty_description": "Link services to see the blast radius.",
      "add_services": "Add services",
      "collapse": "Collapse map",
      "expand": "Expand map",
      "affected_legend": "Affected",
      "neighbor_legend": "Dependency",
      "link_failed": "Couldn't update affected services. Please try again."
    },
    "chat": {
      "title": "Chat",
      "load_older": "Load older messages",
      "composer_placeholder": "Message the room. Use @ to mention.",
      "send": "Send",
      "empty_title": "No messages yet",
      "empty_description": "Start coordinating the response here.",
      "typing_single": "{name} is typing…",
      "typing_multiple": "{count} people are typing…",
      "reconnect_banner": "Reconnecting… messages may be delayed.",
      "edited": "edited",
      "deleted": "This message was deleted",
      "edit": "Edit message",
      "delete": "Delete message",
      "save": "Save",
      "cancel": "Cancel",
      "new_messages": "{count} new",
      "send_failed": "Couldn't send the message.",
      "load_failed": "Couldn't load messages. Please try again.",
      "mentions_empty": "No members found"
    },
    "notes": {
      "placeholder": "Capture decisions, timeline, and follow-ups…",
      "saving": "Saving…",
      "saved": "Saved",
      "edited_at": "Edited {time}",
      "read_only": "Notes are read-only for archived rooms.",
      "save_failed": "Couldn't save the notes. Please try again."
    },
    "context_tabs": {
      "notes": "Notes",
      "work_items": "Work items",
      "runbook": "Runbook",
      "activity": "Activity",
      "people": "People"
    },
    "work_items": {
      "primary_badge": "Primary incident",
      "add": "Link work items",
      "unlink": "Unlink",
      "empty_title": "No linked work items",
      "empty_description": "Link related work items to keep context in one place.",
      "link_failed": "Couldn't update linked work items. Please try again.",
      "unlink_failed": "Couldn't unlink the work item. Please try again."
    },
    "runbook": {
      "progress": "{done}/{total} done",
      "add_placeholder": "Add a step and press Enter",
      "empty_title": "No runbook steps",
      "empty_description": "Add steps to track the response checklist.",
      "mark_done": "Mark as done",
      "mark_undone": "Mark as not done",
      "edit": "Edit step",
      "delete": "Delete step",
      "save": "Save",
      "cancel": "Cancel",
      "update_failed": "Couldn't update the runbook. Please try again."
    },
    "activity": {
      "empty_title": "No activity yet",
      "empty_description": "Status changes, links and runbook updates appear here.",
      "load_older": "Load older activity",
      "load_failed": "Couldn't load activity. Please try again.",
      "actor_unknown": "Someone",
      "events": {
        "room_created": "created the war room",
        "status_changed": "changed status from {from} to {to}",
        "severity_changed": "changed severity from {from} to {to}",
        "resolved": "resolved the war room",
        "reopened": "reopened the war room",
        "archived": "archived the war room",
        "participant_joined": "joined the room",
        "participant_left": "left the room",
        "participant_role_changed": "changed {target}'s role from {from} to {to}",
        "service_linked": "linked {services}",
        "service_unlinked": "unlinked {service}",
        "issue_linked": "linked {issues}",
        "issue_unlinked": "unlinked {issue}",
        "runbook_item_done": "completed “{title}”",
        "runbook_item_reopened": "reopened “{title}”",
        "unknown": "updated the room"
      }
    },
    "people": {
      "add": "Add participant",
      "join": "Join",
      "leave": "Leave",
      "role": "Role",
      "online": "Online",
      "empty_title": "No participants yet",
      "empty_description": "Join the room or add responders to coordinate.",
      "update_failed": "Couldn't update participants. Please try again.",
      "join_failed": "Couldn't join the war room. Please try again.",
      "leave_failed": "Couldn't leave the war room. Please try again."
    },
    "errors": {
      "room_archived": "This war room is archived and can't be changed.",
      "forbidden": "You don't have permission to do that.",
      "generic": "Something went wrong. Please try again."
    }
  }
}
```

- [ ] **Step 2: Translate every new key into the 19 target locales**

Follow the `translate` skill (`/home/ghifari/plane-for-itsm/.claude/skills/translate/SKILL.md`) exactly. Per locale: keep all existing keys and their existing values; add the new sections (`header`, `archived_notice`, `resolve_modal`, `archive_modal`, `delete_modal`, `edit_details_modal`, `map`, `chat`, `notes`, `context_tabs`, `work_items`, `runbook`, `activity`, `people`, `errors`). Never leave English in a non-English file. Do not translate `WR`, `SEV1–4`, `SEV1–2`, `SEV1`, `@`, ICU variables (`{count}`, `{name}`, `{done}`, `{total}`, `{time}`, `{from}`, `{to}`, `{target}`, `{services}`, `{service}`, `{issues}`, `{issue}`, `{title}`), or `@` tokens. Respect each locale's plural rules, punctuation and register per the skill.

- [ ] **Step 3: Regenerate types, rebuild the package, and check sync**

Run:

```bash
pnpm --filter @plane/i18n generate:types
pnpm --filter @plane/i18n build
pnpm --filter @plane/i18n run sync:check
```

Expected: `generate:types` succeeds; `build` writes `dist/index.js` containing `war-room`; `sync:check` reports no `war-room.json` entries (pre-existing drift in other namespaces — baseline was `18 missing, 32 stale` — is unrelated and must not grow).

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales packages/i18n/src/types
git commit -m "i18n(web): war room room-page strings for all locales"
```

---

### Task 2: Shared types for room payloads and socket events

**Files:**

- Modify: `packages/types/src/war-room/core.ts` (append; no existing export changes)

- [ ] **Step 1: Append these types to `packages/types/src/war-room/core.ts`**

```ts
export type TWarRoomMessageCreatePayload = {
  body: string;
  client_id?: string;
};

export type TWarRoomMessageUpdatePayload = {
  body: string;
};

export type TWarRoomMessagesParams = {
  before_id?: string;
  limit?: number;
};

export type TWarRoomEventsParams = {
  before_id?: string;
  limit?: number;
};

export type TWarRoomParticipantCreatePayload = {
  member_id: string;
  role?: TWarRoomParticipantRole;
};

export type TWarRoomParticipantUpdatePayload = {
  role: TWarRoomParticipantRole;
};

export type TWarRoomRunbookCreatePayload = {
  title: string;
};

export type TWarRoomRunbookUpdatePayload = {
  title?: string;
  is_done?: boolean;
};

export type TWarRoomLinkServicesPayload = {
  service_ids: string[];
};

export type TWarRoomLinkIssuesPayload = {
  issue_ids: string[];
};

export type TWarRoomLinkResponse = {
  linked: number;
};

/**
 * Browser-facing relay envelope from `apps/live` (`{kind, data}`). Mirrors the
 * frozen Phase 2 contract; `pong` is answered to app-level pings.
 */
export type TWarRoomSocketEvent =
  | { kind: "message.created"; data: IWarRoomMessage }
  | { kind: "message.updated"; data: IWarRoomMessage }
  | { kind: "message.deleted"; data: { id: string; war_room_id: string } }
  | { kind: "activity.created"; data: IWarRoomEvent }
  | { kind: "room.changed"; data: { reasons: string[] } }
  | { kind: "typing"; data: { user_id: string; is_typing: boolean } }
  | { kind: "presence.joined"; data: { user_id: string; name: string } }
  | { kind: "presence.left"; data: { user_id: string; name: string } }
  | { kind: "pong"; data?: undefined };

export type TWarRoomConnectionStatus = "connecting" | "connected" | "reconnecting" | "disabled";
```

- [ ] **Step 2: Typecheck the package**

Run:

```bash
pnpm --filter @plane/types build
pnpm --filter=web check:types
```

Expected: no new errors (only pre-existing warnings, if any). The `build` is required because `apps/web` resolves `@plane/types` from `dist/` — without it, the new exports are invisible to `tsc`.

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/war-room/core.ts
git commit -m "feat(types): war room room payload and socket event types"
```

---

### Task 3: Status transition map constant

**Files:**

- Modify: `packages/constants/src/war-room.ts` (append)

- [ ] **Step 1: Append the transition map**

```ts
/** Server-enforced transition map (`war_room.rs::transitions_allowed`); `archived` is terminal. */
export const WAR_ROOM_STATUS_TRANSITIONS: Record<TWarRoomStatus, TWarRoomStatus[]> = {
  active: ["monitoring", "resolved", "archived"],
  monitoring: ["active", "resolved", "archived"],
  resolved: ["active", "archived"],
  archived: [],
};
```

- [ ] **Step 2: Verify the constant is exported**

Run: `rg -n "WAR_ROOM_STATUS_TRANSITIONS" packages/constants/src/index.ts packages/constants/src/war-room.ts`
Expected: one match in `war-room.ts`; `index.ts` already does `export * from "./war-room";` (line ~46) so no edit needed.

- [ ] **Step 3: Commit**

```bash
git add packages/constants/src/war-room.ts
git commit -m "feat(constants): war room status transition map"
```

---

### Task 4: Room helpers (blast radius, mentions, notes append, message merge, typing)

**Files:**

- Modify: `apps/web/core/services/war-room.helpers.ts` (append)
- Modify: `apps/web/core/services/war-room.helpers.test.ts` (append)

- [ ] **Step 1: Write the failing tests (append to `war-room.helpers.test.ts`)**

First update the import block at the top of the file to include the new helpers:

```ts
import {
  appendNoteToHtml,
  formatElapsed,
  getBlastRadius,
  getWarRoomIncidentLink,
  isActiveWarRoomStatus,
  isTypingActive,
  parseMessageSegments,
  serializeMentionTokens,
  severityFromPriority,
  shouldShowMessageHeader,
  statusFilterForTab,
  upsertMessageInList,
} from "./war-room.helpers";
import type { IWarRoomMessage, IServiceDependency } from "@plane/types";
```

Then append:

```ts
const makeMessage = (overrides: Partial<IWarRoomMessage> = {}): IWarRoomMessage =>
  ({
    id: "message-1",
    war_room_id: "room-1",
    author_id: "user-1",
    author: null,
    body: "hello",
    mentions: [],
    edited_at: null,
    created_at: "2026-01-01T00:00:00.000Z",
    client_id: null,
    ...overrides,
  }) as IWarRoomMessage;

const makeDependency = (from: string, to: string): IServiceDependency =>
  ({
    id: `${from}-${to}`,
    workspace_id: "ws-1",
    project_id: "project-1",
    from_service_id: from,
    to_service_id: to,
    created_at: "2026-01-01T00:00:00.000Z",
  }) as IServiceDependency;

describe("getBlastRadius", () => {
  it("includes affected services plus both directions of one hop", () => {
    const dependencies = [makeDependency("a", "b"), makeDependency("c", "a"), makeDependency("b", "x")];
    const result = getBlastRadius(["a"], dependencies);
    expect(result.affectedIds).toEqual(["a"]);
    expect(result.neighborIds.sort()).toEqual(["b", "c"]);
    expect(result.includedIds.sort()).toEqual(["a", "b", "c"]);
  });

  it("does not mark an affected service as its own neighbor", () => {
    const result = getBlastRadius(["a", "b"], [makeDependency("a", "b")]);
    expect(result.neighborIds).toEqual([]);
    expect(result.includedIds.sort()).toEqual(["a", "b"]);
  });
});

describe("upsertMessageInList", () => {
  it("appends a message that is not present", () => {
    const result = upsertMessageInList([makeMessage()], makeMessage({ id: "message-2" }));
    expect(result.map((message) => message.id)).toEqual(["message-1", "message-2"]);
  });

  it("replaces by id", () => {
    const result = upsertMessageInList([makeMessage()], makeMessage({ body: "edited" }));
    expect(result).toHaveLength(1);
    expect(result[0].body).toBe("edited");
  });

  it("reconciles an optimistic message by client_id", () => {
    const optimistic = makeMessage({ id: "optimistic-1", client_id: "client-1" });
    const result = upsertMessageInList([optimistic], makeMessage({ id: "server-1", client_id: "client-1" }));
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("server-1");
  });

  it("dedupes a websocket echo by id", () => {
    const result = upsertMessageInList([makeMessage()], makeMessage());
    expect(result).toHaveLength(1);
  });
});

describe("appendNoteToHtml", () => {
  it("appends an escaped paragraph", () => {
    expect(appendNoteToHtml("<p>existing</p>", "rolled back & verified")).toBe(
      "<p>existing</p><p>rolled back &amp; verified</p>"
    );
  });

  it("converts newlines to breaks and returns the original when blank", () => {
    expect(appendNoteToHtml("", "line 1\nline 2")).toBe("<p>line 1<br>line 2</p>");
    expect(appendNoteToHtml("<p>x</p>", "   ")).toBe("<p>x</p>");
  });
});

describe("serializeMentionTokens", () => {
  it("replaces display names with uuid tokens", () => {
    const body = "@Ada Lovelace please check @Grace Hopper";
    const mentions = [
      { id: "uuid-ada", display_name: "Ada Lovelace" },
      { id: "uuid-grace", display_name: "Grace Hopper" },
    ];
    expect(serializeMentionTokens(body, mentions)).toBe("@{uuid-ada} please check @{uuid-grace}");
  });

  it("prefers the longest display name when names overlap", () => {
    const body = "@Ada Lovelace and @Ada";
    const mentions = [
      { id: "uuid-short", display_name: "Ada" },
      { id: "uuid-long", display_name: "Ada Lovelace" },
    ];
    expect(serializeMentionTokens(body, mentions)).toBe("@{uuid-long} and @{uuid-short}");
  });
});

describe("parseMessageSegments", () => {
  it("splits text and mention tokens", () => {
    const segments = parseMessageSegments("ping @{123e4567-e89b-12d3-a456-426614174000} now");
    expect(segments).toEqual([
      { type: "text", value: "ping " },
      { type: "mention", user_id: "123e4567-e89b-12d3-a456-426614174000" },
      { type: "text", value: " now" },
    ]);
  });

  it("returns a single text segment when there are no tokens", () => {
    expect(parseMessageSegments("plain body")).toEqual([{ type: "text", value: "plain body" }]);
  });
});

describe("shouldShowMessageHeader", () => {
  it("shows a header for the first message and on author change", () => {
    const first = makeMessage();
    expect(shouldShowMessageHeader(undefined, first)).toBe(true);
    expect(shouldShowMessageHeader(first, makeMessage({ id: "m2", author_id: "user-2" }))).toBe(true);
  });

  it("groups consecutive messages by the same author within five minutes", () => {
    const first = makeMessage();
    const sameGroup = makeMessage({ id: "m2", created_at: "2026-01-01T00:04:00.000Z" });
    const newGroup = makeMessage({ id: "m3", created_at: "2026-01-01T00:06:00.000Z" });
    expect(shouldShowMessageHeader(first, sameGroup)).toBe(false);
    expect(shouldShowMessageHeader(first, newGroup)).toBe(true);
  });
});

describe("isTypingActive", () => {
  it("is true only before the expiry timestamp", () => {
    expect(isTypingActive(1500, 1000)).toBe(true);
    expect(isTypingActive(1000, 1000)).toBe(false);
    expect(isTypingActive(undefined, 1000)).toBe(false);
    expect(isTypingActive(0, 1000)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --filter=web test -- war-room.helpers`
Expected: FAIL — the new exports do not exist yet.

- [ ] **Step 3: Implement the helpers (append to `war-room.helpers.ts`)**

Update the top import to add the new types:

```ts
import type {
  IWarRoomMessage,
  IServiceDependency,
  TWarRoomSeverity,
  TWarRoomStatus,
  TWarRoomStatusTab,
} from "@plane/types";
```

Then append:

```ts
export type TWarRoomBlastRadius = {
  affectedIds: string[];
  neighborIds: string[];
  includedIds: string[];
};

/** Affected services + every 1-hop neighbor in both dependency directions. */
export const getBlastRadius = (
  affectedServiceIds: string[],
  dependencies: IServiceDependency[]
): TWarRoomBlastRadius => {
  const affected = new Set(affectedServiceIds);
  const neighbors = new Set<string>();
  for (const dependency of dependencies) {
    if (affected.has(dependency.from_service_id) && !affected.has(dependency.to_service_id))
      neighbors.add(dependency.to_service_id);
    if (affected.has(dependency.to_service_id) && !affected.has(dependency.from_service_id))
      neighbors.add(dependency.from_service_id);
  }
  return {
    affectedIds: [...affected],
    neighborIds: [...neighbors],
    includedIds: [...affected, ...neighbors],
  };
};

/** Insert or reconcile a message by `id`, then by `client_id` (optimistic send / WS echo). */
export const upsertMessageInList = (messages: IWarRoomMessage[], message: IWarRoomMessage): IWarRoomMessage[] => {
  const byId = messages.findIndex((candidate) => candidate.id === message.id);
  const index =
    byId !== -1
      ? byId
      : message.client_id
        ? messages.findIndex((candidate) => candidate.client_id === message.client_id)
        : -1;
  if (index === -1) return [...messages, message];
  const next = [...messages];
  next[index] = message;
  return next;
};

/** Append a plain-text note to a notes HTML document, escaped. */
export const appendNoteToHtml = (notesHtml: string, note: string): string => {
  const trimmed = note.trim();
  if (trimmed === "") return notesHtml;
  const escaped = trimmed
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/\n/g, "<br>");
  return `${notesHtml}<p>${escaped}</p>`;
};

export type TWarRoomMention = {
  id: string;
  display_name: string;
};

/** Turn the composer's human-readable `@Display Name` text into server `@{uuid}` tokens. */
export const serializeMentionTokens = (body: string, mentions: TWarRoomMention[]): string => {
  let output = body;
  const ordered = [...mentions]
    .filter((mention) => mention.display_name !== "")
    .sort((a, b) => b.display_name.length - a.display_name.length);
  for (const mention of ordered) {
    output = output.split(`@${mention.display_name}`).join(`@{${mention.id}}`);
  }
  return output;
};

export type TWarRoomMessageSegment = { type: "text"; value: string } | { type: "mention"; user_id: string };

const MENTION_TOKEN_PATTERN = /@\{([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\}/gi;

/** Split a stored message body into renderable text/mention segments. */
export const parseMessageSegments = (body: string): TWarRoomMessageSegment[] => {
  const segments: TWarRoomMessageSegment[] = [];
  let lastIndex = 0;
  for (const match of body.matchAll(MENTION_TOKEN_PATTERN)) {
    const index = match.index ?? 0;
    if (index > lastIndex) segments.push({ type: "text", value: body.slice(lastIndex, index) });
    segments.push({ type: "mention", user_id: match[1] });
    lastIndex = index + match[0].length;
  }
  if (lastIndex < body.length) segments.push({ type: "text", value: body.slice(lastIndex) });
  return segments;
};

/** Chat grouping: new header on author change or a >5 minute gap. */
export const shouldShowMessageHeader = (previous: IWarRoomMessage | undefined, current: IWarRoomMessage): boolean => {
  if (!previous) return true;
  if (previous.author_id !== current.author_id) return true;
  return new Date(current.created_at).getTime() - new Date(previous.created_at).getTime() > 5 * 60 * 1000;
};

export const isTypingActive = (expiresAt: number | undefined, now: number = Date.now()): boolean =>
  typeof expiresAt === "number" && expiresAt > now;
```

Note: `apps/web` targets ES2022, so `.sort()` on a copied array must carry `// oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread already copies the array` (see `workflow.helpers.ts:133`). Without the comment the pre-commit `oxlint --fix` rewrites it to `.toSorted()`, which `tsc` rejects.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm --filter=web test -- war-room.helpers`
Expected: PASS — all suites in `war-room.helpers.test.ts`.

- [ ] **Step 5: Format and commit**

```bash
pnpm fix:format
git add apps/web/core/services/war-room.helpers.ts apps/web/core/services/war-room.helpers.test.ts
git commit -m "feat(web): war room room helpers with unit tests"
```

---

### Task 5: WarRoomService — full REST surface

**Files:**

- Modify: `apps/web/core/services/war-room.service.ts` (replace whole file)

- [ ] **Step 1: Replace `apps/web/core/services/war-room.service.ts` with this content**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IWarRoom,
  IWarRoomEvent,
  IWarRoomListItem,
  IWarRoomMessage,
  IWarRoomParticipant,
  IWarRoomRunbookItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomEventsParams,
  TWarRoomLinkIssuesPayload,
  TWarRoomLinkResponse,
  TWarRoomLinkServicesPayload,
  TWarRoomListParams,
  TWarRoomMessageCreatePayload,
  TWarRoomMessageUpdatePayload,
  TWarRoomMessagesParams,
  TWarRoomParticipantCreatePayload,
  TWarRoomParticipantUpdatePayload,
  TWarRoomRunbookCreatePayload,
  TWarRoomRunbookUpdatePayload,
  TWarRoomUpdatePayload,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TWarRoomErrorBody = { detail?: string; error?: string; war_room_id?: string; [key: string]: unknown };

/**
 * Normalizes axios errors into a real Error carrying `.detail`, `.error`, and
 * `.war_room_id` (the 409 duplicate payload used by the create modal banner).
 */
const toWarRoomError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TWarRoomErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string; war_room_id?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  normalized.war_room_id = body?.war_room_id;
  return normalized;
};

export class WarRoomService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private basePath(workspaceSlug: string, projectId: string): string {
    return `/api/workspaces/${workspaceSlug}/projects/${projectId}/war-rooms`;
  }

  private roomPath(workspaceSlug: string, projectId: string, warRoomId: string): string {
    return `${this.basePath(workspaceSlug, projectId)}/${warRoomId}`;
  }

  async getWarRooms(
    workspaceSlug: string,
    projectId: string,
    params?: TWarRoomListParams
  ): Promise<IWarRoomListItem[]> {
    return this.get(`${this.basePath(workspaceSlug, projectId)}/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async getWarRoomSummary(workspaceSlug: string, projectId: string): Promise<IWarRoomSummary> {
    return this.get(`${this.basePath(workspaceSlug, projectId)}/summary/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async getWarRoom(workspaceSlug: string, projectId: string, warRoomId: string): Promise<IWarRoom> {
    return this.get(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async createWarRoom(workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload): Promise<IWarRoom> {
    return this.post(`${this.basePath(workspaceSlug, projectId)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateWarRoom(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomUpdatePayload
  ): Promise<IWarRoom> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteWarRoom(workspaceSlug: string, projectId: string, warRoomId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async addServices(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomLinkServicesPayload
  ): Promise<TWarRoomLinkResponse> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/services/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async removeService(workspaceSlug: string, projectId: string, warRoomId: string, serviceId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/services/${serviceId}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async addIssues(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomLinkIssuesPayload
  ): Promise<TWarRoomLinkResponse> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/issues/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async removeIssue(workspaceSlug: string, projectId: string, warRoomId: string, issueId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/issues/${issueId}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async createParticipant(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomParticipantCreatePayload
  ): Promise<IWarRoomParticipant> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/participants/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateParticipant(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string,
    data: TWarRoomParticipantUpdatePayload
  ): Promise<IWarRoomParticipant> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/participants/${participantId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteParticipant(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string
  ): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/participants/${participantId}/`).catch(
      (error) => {
        throw toWarRoomError(error);
      }
    );
  }

  async createRunbookItem(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomRunbookCreatePayload
  ): Promise<IWarRoomRunbookItem> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/runbook-items/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateRunbookItem(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    itemId: string,
    data: TWarRoomRunbookUpdatePayload
  ): Promise<IWarRoomRunbookItem> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/runbook-items/${itemId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteRunbookItem(workspaceSlug: string, projectId: string, warRoomId: string, itemId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/runbook-items/${itemId}/`).catch(
      (error) => {
        throw toWarRoomError(error);
      }
    );
  }

  async getMessages(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomMessagesParams
  ): Promise<IWarRoomMessage[]> {
    return this.get(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async createMessage(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomMessageCreatePayload
  ): Promise<IWarRoomMessage> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateMessage(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    messageId: string,
    data: TWarRoomMessageUpdatePayload
  ): Promise<IWarRoomMessage> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/${messageId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteMessage(workspaceSlug: string, projectId: string, warRoomId: string, messageId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/${messageId}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async getEvents(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomEventsParams
  ): Promise<IWarRoomEvent[]> {
    return this.get(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/events/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/war-room.service.ts
git commit -m "feat(web): war room room REST client methods"
```

---

### Task 6: WarRoomStore — messages, events, presence, typing, room mutations

**Files:**

- Modify: `apps/web/core/store/war-room.store.ts` (replace whole file)
- Modify: `apps/web/core/store/war-room.store.test.ts` (update mock + append tests)

- [ ] **Step 1: Write the failing tests first**

In `war-room.store.test.ts`, replace the `makeStore` helper with:

```ts
const makeStore = () => {
  const store = new WarRoomStore({} as never);
  const warRoomService = {
    getWarRooms: vi.fn(async () => [makeRoom()]),
    getWarRoomSummary: vi.fn(async (): Promise<IWarRoomSummary> => ({ active: 1, sev1_2: 0, resolved_7d: 0 })),
    getWarRoom: vi.fn(async () => makeDetail()),
    createWarRoom: vi.fn(async () => makeDetail({ id: "room-2", sequence_id: 2 })),
    updateWarRoom: vi.fn(async () => makeDetail({ name: "Updated room" })),
    deleteWarRoom: vi.fn(async () => undefined),
    addServices: vi.fn(async () => ({ linked: 1 })),
    removeService: vi.fn(async () => undefined),
    addIssues: vi.fn(async () => ({ linked: 1 })),
    removeIssue: vi.fn(async () => undefined),
    createParticipant: vi.fn(async () => makeParticipant({ id: "participant-2", member_id: "user-2" })),
    updateParticipant: vi.fn(async () => makeParticipant({ id: "participant-1", role: "comms" })),
    deleteParticipant: vi.fn(async () => undefined),
    createRunbookItem: vi.fn(async () => makeRunbookItem({ id: "item-2" })),
    updateRunbookItem: vi.fn(async () => makeRunbookItem({ id: "item-1", is_done: true })),
    deleteRunbookItem: vi.fn(async () => undefined),
    getMessages: vi.fn(async () => [makeMessage()]),
    createMessage: vi.fn(async () => makeMessage()),
    updateMessage: vi.fn(async () => makeMessage({ body: "edited" })),
    deleteMessage: vi.fn(async () => undefined),
    getEvents: vi.fn(async () => [makeEvent()]),
  };
  (store as unknown as { warRoomService: typeof warRoomService }).warRoomService = warRoomService;
  return { store, warRoomService };
};
```

Add these fixtures before `makeStore`:

```ts
const makeParticipant = (overrides: Partial<IWarRoomParticipant> = {}): IWarRoomParticipant =>
  ({
    id: "participant-1",
    member_id: "user-1",
    role: "commander",
    joined_at: "2026-01-01T00:00:00.000Z",
    display_name: "User One",
    avatar_url: null,
    ...overrides,
  }) as IWarRoomParticipant;

const makeRunbookItem = (overrides: Partial<IWarRoomRunbookItem> = {}): IWarRoomRunbookItem =>
  ({
    id: "item-1",
    title: "Triage",
    sort_order: 65535,
    is_done: false,
    done_by_id: null,
    done_at: null,
    template_key: "triage",
    ...overrides,
  }) as IWarRoomRunbookItem;

const makeMessage = (overrides: Partial<IWarRoomMessage> = {}): IWarRoomMessage =>
  ({
    id: "message-1",
    war_room_id: "room-1",
    author_id: "user-1",
    author: null,
    body: "hello",
    mentions: [],
    edited_at: null,
    created_at: "2026-01-01T00:00:00.000Z",
    client_id: null,
    ...overrides,
  }) as IWarRoomMessage;

const makeEvent = (overrides: Partial<IWarRoomEvent> = {}): IWarRoomEvent =>
  ({
    id: "event-1",
    actor_id: "user-1",
    event_type: "room.status_changed",
    payload: { from: "active", to: "resolved" },
    created_at: "2026-01-01T00:00:00.000Z",
    ...overrides,
  }) as IWarRoomEvent;
```

Update the type import at the top to include the new types:

```ts
import type {
  IWarRoom,
  IWarRoomEvent,
  IWarRoomListItem,
  IWarRoomMessage,
  IWarRoomParticipant,
  IWarRoomRunbookItem,
  IWarRoomSummary,
} from "@plane/types";
```

Also update the existing `makeDetail` fixture to seed one participant and one runbook item (the mutation tests below need a baseline to update):

```ts
const makeDetail = (overrides: Partial<IWarRoom> = {}): IWarRoom =>
  ({
    ...makeRoom(),
    issues: [],
    participants: [makeParticipant()],
    runbook_items: [makeRunbookItem()],
    counts: { messages: 0 },
    ...overrides,
  }) as unknown as IWarRoom;
```

Append these describes:

```ts
describe("WarRoomStore messages", () => {
  it("replaces the first page and tracks hasMore", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getMessages.mockResolvedValueOnce([makeMessage()]);

    await store.fetchMessages("acme", "project-1", "room-1");

    expect(store.getMessages("room-1").map((message) => message.id)).toEqual(["message-1"]);
    expect(store.hasMoreMessages("room-1")).toBe(false);
  });

  it("prepends older pages without duplicating ids", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getMessages.mockResolvedValueOnce([makeMessage({ id: "m2" })]);
    await store.fetchMessages("acme", "project-1", "room-1");

    warRoomService.getMessages.mockResolvedValueOnce([makeMessage({ id: "m0" }), makeMessage({ id: "m1" })]);
    await store.fetchMessages("acme", "project-1", "room-1", { before_id: "m2" });

    expect(store.getMessages("room-1").map((message) => message.id)).toEqual(["m0", "m1", "m2"]);
  });

  it("reconciles the optimistic message by client_id", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.createMessage.mockResolvedValueOnce(makeMessage({ id: "server-1", client_id: "client-1" }));

    await store.sendMessage("acme", "project-1", "room-1", "hello", "client-1");

    expect(store.getMessages("room-1")).toHaveLength(1);
    expect(store.getMessages("room-1")[0].id).toBe("server-1");
  });

  it("removes the optimistic message when the send fails", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.createMessage.mockRejectedValueOnce(new Error("boom"));

    await expect(store.sendMessage("acme", "project-1", "room-1", "hello", "client-1")).rejects.toThrow("boom");
    expect(store.getMessages("room-1")).toHaveLength(0);
  });

  it("updates and deletes messages", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchMessages("acme", "project-1", "room-1");
    warRoomService.updateMessage.mockResolvedValueOnce(makeMessage({ body: "edited" }));

    await store.updateMessage("acme", "project-1", "room-1", "message-1", "edited");
    expect(store.getMessages("room-1")[0].body).toBe("edited");

    await store.deleteMessage("acme", "project-1", "room-1", "message-1");
    expect(store.getMessages("room-1")).toHaveLength(0);
  });
});

describe("WarRoomStore events", () => {
  it("replaces the first page and appends older pages", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getEvents.mockResolvedValueOnce([makeEvent({ id: "e2" })]);
    await store.fetchEvents("acme", "project-1", "room-1");

    warRoomService.getEvents.mockResolvedValueOnce([makeEvent({ id: "e0" }), makeEvent({ id: "e1" })]);
    await store.fetchEvents("acme", "project-1", "room-1", { before_id: "e2" });

    expect(store.getEvents("room-1").map((event) => event.id)).toEqual(["e2", "e0", "e1"]);
  });
});

describe("WarRoomStore.applySocketEvent", () => {
  it("dedupes message.created and applies update/delete", () => {
    const { store } = makeStore();

    store.applySocketEvent("acme", "project-1", "room-1", { kind: "message.created", data: makeMessage() });
    store.applySocketEvent("acme", "project-1", "room-1", { kind: "message.created", data: makeMessage() });
    expect(store.getMessages("room-1")).toHaveLength(1);

    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "message.updated",
      data: makeMessage({ body: "edited" }),
    });
    expect(store.getMessages("room-1")[0].body).toBe("edited");

    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "message.deleted",
      data: { id: "message-1", war_room_id: "room-1" },
    });
    expect(store.getMessages("room-1")).toHaveLength(0);
  });

  it("prepends activity.created exactly once", () => {
    const { store } = makeStore();
    store.applySocketEvent("acme", "project-1", "room-1", { kind: "activity.created", data: makeEvent() });
    store.applySocketEvent("acme", "project-1", "room-1", { kind: "activity.created", data: makeEvent() });
    expect(store.getEvents("room-1")).toHaveLength(1);
  });

  it("refetches detail on room.changed and drops state on deleted", async () => {
    const { store, warRoomService } = makeStore();
    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "room.changed",
      data: { reasons: ["notes"] },
    });
    await vi.waitFor(() => expect(warRoomService.getWarRoom).toHaveBeenCalled());

    await store.fetchWarRoomDetail("acme", "project-1", "room-1");
    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "room.changed",
      data: { reasons: ["deleted"] },
    });
    expect(store.getWarRoomDetailById("room-1")).toBeNull();
  });

  it("tracks typing expiry, presence and unread", () => {
    const { store } = makeStore();
    store.applyTyping("room-1", "user-2", true);
    expect(store.getTypingUserIds("room-1", Date.now())).toContain("user-2");
    store.applyTyping("room-1", "user-2", false);
    expect(store.getTypingUserIds("room-1", Date.now())).toEqual([]);

    store.applyPresence("room-1", "user-2", true);
    store.applyPresence("room-1", "user-3", true);
    store.applyPresence("room-1", "user-2", false);
    expect(store.getOnlineUserIds("room-1")).toEqual(["user-3"]);

    store.incrementUnread("room-1");
    store.incrementUnread("room-1");
    expect(store.getUnreadCount("room-1")).toBe(2);
    store.clearUnread("room-1");
    expect(store.getUnreadCount("room-1")).toBe(0);
  });
});

describe("WarRoomStore room mutations", () => {
  it("updates detail and mirrors the list item", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRooms("acme", "project-1");
    warRoomService.updateWarRoom.mockResolvedValueOnce(makeDetail({ name: "Updated room", status: "resolved" }));

    await store.updateWarRoom("acme", "project-1", "room-1", { status: "resolved" });

    expect(store.getWarRoomDetailById("room-1")?.name).toBe("Updated room");
    expect(store.getWarRoomById("room-1")?.status).toBe("resolved");
  });

  it("applies runbook mutations locally", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRoomDetail("acme", "project-1", "room-1");

    await store.createRunbookItem("acme", "project-1", "room-1", "New step");
    expect(store.getWarRoomDetailById("room-1")?.runbook_items.map((item) => item.id)).toContain("item-2");

    warRoomService.updateRunbookItem.mockResolvedValueOnce(makeRunbookItem({ id: "item-1", is_done: true }));
    await store.updateRunbookItem("acme", "project-1", "room-1", "item-1", { is_done: true });
    expect(store.getWarRoomDetailById("room-1")?.runbook_items.find((item) => item.id === "item-1")?.is_done).toBe(
      true
    );

    await store.deleteRunbookItem("acme", "project-1", "room-1", "item-2");
    expect(store.getWarRoomDetailById("room-1")?.runbook_items.map((item) => item.id)).toEqual(["item-1"]);
  });

  it("applies participant mutations and demotes other commanders", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRoomDetail("acme", "project-1", "room-1");

    warRoomService.createParticipant.mockResolvedValueOnce(
      makeParticipant({ id: "participant-2", member_id: "user-2", role: "commander" })
    );
    await store.addParticipant("acme", "project-1", "room-1", { member_id: "user-2", role: "commander" });

    const participants = store.getWarRoomDetailById("room-1")?.participants ?? [];
    expect(participants.find((p) => p.id === "participant-2")?.role).toBe("commander");
    expect(participants.find((p) => p.id === "participant-1")?.role).toBe("responder");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --filter=web test -- war-room.store`
Expected: FAIL — store methods do not exist yet.

- [ ] **Step 3: Replace `apps/web/core/store/war-room.store.ts` with this content**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type {
  IWarRoom,
  IWarRoomEvent,
  IWarRoomListItem,
  IWarRoomMessage,
  IWarRoomParticipant,
  IWarRoomRunbookItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomEventsParams,
  TWarRoomListParams,
  TWarRoomMessagesParams,
  TWarRoomParticipantCreatePayload,
  TWarRoomParticipantUpdatePayload,
  TWarRoomRunbookCreatePayload,
  TWarRoomRunbookUpdatePayload,
  TWarRoomSocketEvent,
  TWarRoomUpdatePayload,
} from "@plane/types";
// helpers
import { isActiveWarRoomStatus, isTypingActive, upsertMessageInList } from "@/services/war-room.helpers";
// services
import { WarRoomService } from "@/services/war-room.service";
// store
import type { CoreRootStore } from "./root.store";

const TYPING_TIMEOUT_MS = 3000;
const DEFAULT_MESSAGE_PAGE_SIZE = 50;
const DEFAULT_EVENT_PAGE_SIZE = 50;

export interface IWarRoomStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  warRoomMap: Record<string, IWarRoomListItem>;
  warRoomIdsMap: Record<string, string[]>;
  detailMap: Record<string, IWarRoom>;
  summaryMap: Record<string, IWarRoomSummary>;
  errorMap: Record<string, boolean>;
  detailErrorMap: Record<string, boolean>;
  messagesMap: Record<string, IWarRoomMessage[]>;
  messagesHasMoreMap: Record<string, boolean>;
  messagesLoaderMap: Record<string, boolean>;
  eventsMap: Record<string, IWarRoomEvent[]>;
  eventsHasMoreMap: Record<string, boolean>;
  eventsLoaderMap: Record<string, boolean>;
  unreadMap: Record<string, number>;
  typingMap: Record<string, Record<string, number>>;
  onlineUsersMap: Record<string, string[]>;
  getWarRoomById: (warRoomId: string) => IWarRoomListItem | null;
  getProjectWarRoomIds: (projectId: string) => string[] | null;
  getWarRoomDetailById: (warRoomId: string) => IWarRoom | null;
  getProjectSummary: (projectId: string) => IWarRoomSummary | null;
  getActiveWarRoomByIssue: (projectId: string, issueId: string) => IWarRoomListItem | null;
  getMessages: (warRoomId: string) => IWarRoomMessage[];
  hasMoreMessages: (warRoomId: string) => boolean;
  getEvents: (warRoomId: string) => IWarRoomEvent[];
  hasMoreEvents: (warRoomId: string) => boolean;
  getUnreadCount: (warRoomId: string) => number;
  getTypingUserIds: (warRoomId: string, now?: number) => string[];
  getOnlineUserIds: (warRoomId: string) => string[];
  fetchWarRooms: (
    workspaceSlug: string,
    projectId: string,
    params?: TWarRoomListParams
  ) => Promise<IWarRoomListItem[] | undefined>;
  fetchWarRoomSummary: (workspaceSlug: string, projectId: string) => Promise<IWarRoomSummary | undefined>;
  fetchWarRoomDetail: (workspaceSlug: string, projectId: string, warRoomId: string) => Promise<IWarRoom | undefined>;
  createWarRoom: (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => Promise<IWarRoom>;
  updateWarRoom: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomUpdatePayload
  ) => Promise<IWarRoom>;
  deleteWarRoom: (workspaceSlug: string, projectId: string, warRoomId: string) => Promise<void>;
  addServices: (workspaceSlug: string, projectId: string, warRoomId: string, serviceIds: string[]) => Promise<number>;
  removeService: (workspaceSlug: string, projectId: string, warRoomId: string, serviceId: string) => Promise<void>;
  addIssues: (workspaceSlug: string, projectId: string, warRoomId: string, issueIds: string[]) => Promise<number>;
  removeIssue: (workspaceSlug: string, projectId: string, warRoomId: string, issueId: string) => Promise<void>;
  addParticipant: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomParticipantCreatePayload
  ) => Promise<IWarRoomParticipant>;
  updateParticipantRole: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string,
    data: TWarRoomParticipantUpdatePayload
  ) => Promise<IWarRoomParticipant>;
  removeParticipant: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string
  ) => Promise<void>;
  createRunbookItem: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    title: string
  ) => Promise<IWarRoomRunbookItem>;
  updateRunbookItem: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    itemId: string,
    data: TWarRoomRunbookUpdatePayload
  ) => Promise<IWarRoomRunbookItem>;
  deleteRunbookItem: (workspaceSlug: string, projectId: string, warRoomId: string, itemId: string) => Promise<void>;
  fetchMessages: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomMessagesParams
  ) => Promise<IWarRoomMessage[] | undefined>;
  sendMessage: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    body: string,
    clientId: string
  ) => Promise<IWarRoomMessage>;
  updateMessage: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    messageId: string,
    body: string
  ) => Promise<IWarRoomMessage>;
  deleteMessage: (workspaceSlug: string, projectId: string, warRoomId: string, messageId: string) => Promise<void>;
  fetchEvents: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomEventsParams
  ) => Promise<IWarRoomEvent[] | undefined>;
  applySocketEvent: (workspaceSlug: string, projectId: string, warRoomId: string, event: TWarRoomSocketEvent) => void;
  incrementUnread: (warRoomId: string) => void;
  clearUnread: (warRoomId: string) => void;
}

export class WarRoomStore implements IWarRoomStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  warRoomMap: Record<string, IWarRoomListItem> = {};
  warRoomIdsMap: Record<string, string[]> = {};
  detailMap: Record<string, IWarRoom> = {};
  summaryMap: Record<string, IWarRoomSummary> = {};
  errorMap: Record<string, boolean> = {};
  detailErrorMap: Record<string, boolean> = {};
  messagesMap: Record<string, IWarRoomMessage[]> = {};
  messagesHasMoreMap: Record<string, boolean> = {};
  messagesLoaderMap: Record<string, boolean> = {};
  eventsMap: Record<string, IWarRoomEvent[]> = {};
  eventsHasMoreMap: Record<string, boolean> = {};
  eventsLoaderMap: Record<string, boolean> = {};
  unreadMap: Record<string, number> = {};
  typingMap: Record<string, Record<string, number>> = {};
  onlineUsersMap: Record<string, string[]> = {};
  rootStore: CoreRootStore;
  warRoomService: WarRoomService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      warRoomMap: observable,
      warRoomIdsMap: observable,
      detailMap: observable,
      summaryMap: observable,
      errorMap: observable,
      detailErrorMap: observable,
      messagesMap: observable,
      messagesHasMoreMap: observable,
      messagesLoaderMap: observable,
      eventsMap: observable,
      eventsHasMoreMap: observable,
      eventsLoaderMap: observable,
      unreadMap: observable,
      typingMap: observable,
      onlineUsersMap: observable,
      fetchWarRooms: action,
      fetchWarRoomSummary: action,
      fetchWarRoomDetail: action,
      createWarRoom: action,
      updateWarRoom: action,
      deleteWarRoom: action,
      addServices: action,
      removeService: action,
      addIssues: action,
      removeIssue: action,
      addParticipant: action,
      updateParticipantRole: action,
      removeParticipant: action,
      createRunbookItem: action,
      updateRunbookItem: action,
      deleteRunbookItem: action,
      fetchMessages: action,
      sendMessage: action,
      updateMessage: action,
      deleteMessage: action,
      fetchEvents: action,
      applySocketEvent: action,
      applyTyping: action,
      applyPresence: action,
      incrementUnread: action,
      clearUnread: action,
    });
    this.rootStore = _rootStore;
    this.warRoomService = new WarRoomService();
  }

  getWarRoomById = computedFn((warRoomId: string) => this.warRoomMap[warRoomId] ?? null);

  getProjectWarRoomIds = computedFn((projectId: string) =>
    this.fetchedMap[projectId] ? (this.warRoomIdsMap[projectId] ?? []) : null
  );

  getWarRoomDetailById = computedFn((warRoomId: string) => this.detailMap[warRoomId] ?? null);

  getProjectSummary = computedFn((projectId: string) => this.summaryMap[projectId] ?? null);

  getActiveWarRoomByIssue = computedFn((projectId: string, issueId: string) => {
    const room = Object.values(this.warRoomMap).find(
      (candidate) =>
        candidate.project_id === projectId &&
        candidate.primary_issue_id === issueId &&
        isActiveWarRoomStatus(candidate.status)
    );
    return room ?? null;
  });

  getMessages = (warRoomId: string): IWarRoomMessage[] => this.messagesMap[warRoomId] ?? [];

  hasMoreMessages = (warRoomId: string): boolean => this.messagesHasMoreMap[warRoomId] ?? false;

  getEvents = (warRoomId: string): IWarRoomEvent[] => this.eventsMap[warRoomId] ?? [];

  hasMoreEvents = (warRoomId: string): boolean => this.eventsHasMoreMap[warRoomId] ?? false;

  getUnreadCount = (warRoomId: string): number => this.unreadMap[warRoomId] ?? 0;

  getTypingUserIds = (warRoomId: string, now: number = Date.now()): string[] =>
    Object.entries(this.typingMap[warRoomId] ?? {})
      .filter(([, expiresAt]) => isTypingActive(expiresAt, now))
      .map(([userId]) => userId);

  getOnlineUserIds = (warRoomId: string): string[] => this.onlineUsersMap[warRoomId] ?? [];

  fetchWarRooms = async (workspaceSlug: string, projectId: string, params?: TWarRoomListParams) => {
    try {
      runInAction(() => {
        set(this.errorMap, projectId, false);
        this.loader = true;
      });
      const rooms = await this.warRoomService.getWarRooms(workspaceSlug, projectId, params);
      runInAction(() => {
        rooms.forEach((room) => set(this.warRoomMap, [room.id], room));
        set(
          this.warRoomIdsMap,
          projectId,
          rooms.map((room) => room.id)
        );
        set(this.fetchedMap, projectId, true);
        this.loader = false;
      });
      return rooms;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, projectId, true);
      });
      return undefined;
    }
  };

  fetchWarRoomSummary = async (workspaceSlug: string, projectId: string) => {
    try {
      const summary = await this.warRoomService.getWarRoomSummary(workspaceSlug, projectId);
      runInAction(() => {
        set(this.summaryMap, projectId, summary);
      });
      return summary;
    } catch {
      return undefined;
    }
  };

  fetchWarRoomDetail = async (workspaceSlug: string, projectId: string, warRoomId: string) => {
    try {
      runInAction(() => {
        set(this.detailErrorMap, warRoomId, false);
      });
      const room = await this.warRoomService.getWarRoom(workspaceSlug, projectId, warRoomId);
      runInAction(() => {
        set(this.detailMap, [warRoomId], room);
      });
      return room;
    } catch {
      runInAction(() => {
        set(this.detailErrorMap, warRoomId, true);
      });
      return undefined;
    }
  };

  createWarRoom = async (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => {
    const room = await this.warRoomService.createWarRoom(workspaceSlug, projectId, data);
    runInAction(() => {
      set(this.detailMap, [room.id], room);
      const currentIds = this.warRoomIdsMap[projectId] ?? [];
      set(this.warRoomIdsMap, projectId, [room.id, ...currentIds.filter((id) => id !== room.id)]);
      set(this.fetchedMap, projectId, true);
    });
    return room;
  };

  updateWarRoom = async (workspaceSlug: string, projectId: string, warRoomId: string, data: TWarRoomUpdatePayload) => {
    const room = await this.warRoomService.updateWarRoom(workspaceSlug, projectId, warRoomId, data);
    runInAction(() => {
      set(this.detailMap, [warRoomId], room);
      const listItem = this.warRoomMap[warRoomId];
      if (listItem) {
        set(this.warRoomMap, [warRoomId], {
          ...listItem,
          name: room.name,
          description_html: room.description_html,
          notes_html: room.notes_html,
          severity: room.severity,
          status: room.status,
          resolved_at: room.resolved_at,
          updated_at: room.updated_at,
        });
      }
    });
    return room;
  };

  deleteWarRoom = async (workspaceSlug: string, projectId: string, warRoomId: string) => {
    await this.warRoomService.deleteWarRoom(workspaceSlug, projectId, warRoomId);
    runInAction(() => {
      delete this.detailMap[warRoomId];
      delete this.warRoomMap[warRoomId];
      delete this.messagesMap[warRoomId];
      delete this.messagesHasMoreMap[warRoomId];
      delete this.eventsMap[warRoomId];
      delete this.eventsHasMoreMap[warRoomId];
      const currentIds = this.warRoomIdsMap[projectId];
      if (currentIds)
        set(
          this.warRoomIdsMap,
          projectId,
          currentIds.filter((id) => id !== warRoomId)
        );
    });
  };

  addServices = async (workspaceSlug: string, projectId: string, warRoomId: string, serviceIds: string[]) => {
    const response = await this.warRoomService.addServices(workspaceSlug, projectId, warRoomId, {
      service_ids: serviceIds,
    });
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
    return response.linked;
  };

  removeService = async (workspaceSlug: string, projectId: string, warRoomId: string, serviceId: string) => {
    await this.warRoomService.removeService(workspaceSlug, projectId, warRoomId, serviceId);
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  };

  addIssues = async (workspaceSlug: string, projectId: string, warRoomId: string, issueIds: string[]) => {
    const response = await this.warRoomService.addIssues(workspaceSlug, projectId, warRoomId, {
      issue_ids: issueIds,
    });
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
    return response.linked;
  };

  removeIssue = async (workspaceSlug: string, projectId: string, warRoomId: string, issueId: string) => {
    await this.warRoomService.removeIssue(workspaceSlug, projectId, warRoomId, issueId);
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  };

  addParticipant = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomParticipantCreatePayload
  ) => {
    const participant = await this.warRoomService.createParticipant(workspaceSlug, projectId, warRoomId, data);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      const existing = room.participants.some((candidate) => candidate.id === participant.id);
      const participants = existing
        ? room.participants.map((candidate) => (candidate.id === participant.id ? participant : candidate))
        : [...room.participants, participant];
      set(this.detailMap, [warRoomId], {
        ...room,
        participants:
          participant.role === "commander"
            ? participants.map((candidate) =>
                candidate.id !== participant.id && candidate.role === "commander"
                  ? { ...candidate, role: "responder" as const }
                  : candidate
              )
            : participants,
      });
    });
    return participant;
  };

  updateParticipantRole = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string,
    data: TWarRoomParticipantUpdatePayload
  ) => {
    const participant = await this.warRoomService.updateParticipant(
      workspaceSlug,
      projectId,
      warRoomId,
      participantId,
      data
    );
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        participants: room.participants.map((candidate) => {
          if (candidate.id === participant.id) return participant;
          if (data.role === "commander" && candidate.role === "commander")
            return { ...candidate, role: "responder" as const };
          return candidate;
        }),
      });
    });
    return participant;
  };

  removeParticipant = async (workspaceSlug: string, projectId: string, warRoomId: string, participantId: string) => {
    await this.warRoomService.deleteParticipant(workspaceSlug, projectId, warRoomId, participantId);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        participants: room.participants.filter((candidate) => candidate.id !== participantId),
      });
    });
  };

  createRunbookItem = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    title: string
  ): Promise<IWarRoomRunbookItem> => {
    const item = await this.warRoomService.createRunbookItem(workspaceSlug, projectId, warRoomId, { title });
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], { ...room, runbook_items: [...room.runbook_items, item] });
    });
    return item;
  };

  updateRunbookItem = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    itemId: string,
    data: TWarRoomRunbookUpdatePayload
  ): Promise<IWarRoomRunbookItem> => {
    const item = await this.warRoomService.updateRunbookItem(workspaceSlug, projectId, warRoomId, itemId, data);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        runbook_items: room.runbook_items.map((candidate) => (candidate.id === item.id ? item : candidate)),
      });
    });
    return item;
  };

  deleteRunbookItem = async (workspaceSlug: string, projectId: string, warRoomId: string, itemId: string) => {
    await this.warRoomService.deleteRunbookItem(workspaceSlug, projectId, warRoomId, itemId);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        runbook_items: room.runbook_items.filter((candidate) => candidate.id !== itemId),
      });
    });
  };

  fetchMessages = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomMessagesParams
  ) => {
    const isPaging = Boolean(params?.before_id);
    runInAction(() => {
      set(this.messagesLoaderMap, warRoomId, true);
    });
    try {
      const page = await this.warRoomService.getMessages(workspaceSlug, projectId, warRoomId, params);
      runInAction(() => {
        const existing = this.messagesMap[warRoomId] ?? [];
        if (isPaging) {
          const existingIds = new Set(existing.map((message) => message.id));
          set(this.messagesMap, warRoomId, [...page.filter((message) => !existingIds.has(message.id)), ...existing]);
        } else {
          set(this.messagesMap, warRoomId, page);
        }
        set(this.messagesHasMoreMap, warRoomId, page.length >= (params?.limit ?? DEFAULT_MESSAGE_PAGE_SIZE));
        set(this.messagesLoaderMap, warRoomId, false);
      });
      return page;
    } catch (error) {
      runInAction(() => {
        set(this.messagesLoaderMap, warRoomId, false);
      });
      throw error;
    }
  };

  sendMessage = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    body: string,
    clientId: string
  ): Promise<IWarRoomMessage> => {
    const user = this.rootStore.user?.data;
    const optimistic: IWarRoomMessage = {
      id: `optimistic-${clientId}`,
      war_room_id: warRoomId,
      author_id: user?.id ?? null,
      author: user
        ? { id: user.id, display_name: user.display_name ?? null, avatar_url: user.avatar_url ?? null }
        : null,
      body,
      mentions: [],
      edited_at: null,
      created_at: new Date().toISOString(),
      client_id: clientId,
    };
    runInAction(() => {
      set(this.messagesMap, warRoomId, [...(this.messagesMap[warRoomId] ?? []), optimistic]);
    });
    try {
      const message = await this.warRoomService.createMessage(workspaceSlug, projectId, warRoomId, {
        body,
        client_id: clientId,
      });
      runInAction(() => {
        set(this.messagesMap, warRoomId, upsertMessageInList(this.messagesMap[warRoomId] ?? [], message));
      });
      return message;
    } catch (error) {
      runInAction(() => {
        set(
          this.messagesMap,
          warRoomId,
          (this.messagesMap[warRoomId] ?? []).filter((message) => message.id !== optimistic.id)
        );
      });
      throw error;
    }
  };

  updateMessage = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    messageId: string,
    body: string
  ): Promise<IWarRoomMessage> => {
    const message = await this.warRoomService.updateMessage(workspaceSlug, projectId, warRoomId, messageId, { body });
    runInAction(() => {
      set(this.messagesMap, warRoomId, upsertMessageInList(this.messagesMap[warRoomId] ?? [], message));
    });
    return message;
  };

  deleteMessage = async (workspaceSlug: string, projectId: string, warRoomId: string, messageId: string) => {
    await this.warRoomService.deleteMessage(workspaceSlug, projectId, warRoomId, messageId);
    runInAction(() => {
      set(
        this.messagesMap,
        warRoomId,
        (this.messagesMap[warRoomId] ?? []).filter((message) => message.id !== messageId)
      );
    });
  };

  fetchEvents = async (workspaceSlug: string, projectId: string, warRoomId: string, params?: TWarRoomEventsParams) => {
    const isPaging = Boolean(params?.before_id);
    runInAction(() => {
      set(this.eventsLoaderMap, warRoomId, true);
    });
    try {
      const page = await this.warRoomService.getEvents(workspaceSlug, projectId, warRoomId, params);
      runInAction(() => {
        const existing = this.eventsMap[warRoomId] ?? [];
        if (isPaging) {
          const existingIds = new Set(existing.map((event) => event.id));
          set(this.eventsMap, warRoomId, [...existing, ...page.filter((event) => !existingIds.has(event.id))]);
        } else {
          set(this.eventsMap, warRoomId, page);
        }
        set(this.eventsHasMoreMap, warRoomId, page.length >= (params?.limit ?? DEFAULT_EVENT_PAGE_SIZE));
        set(this.eventsLoaderMap, warRoomId, false);
      });
      return page;
    } catch (error) {
      runInAction(() => {
        set(this.eventsLoaderMap, warRoomId, false);
      });
      throw error;
    }
  };

  applySocketEvent = (workspaceSlug: string, projectId: string, warRoomId: string, event: TWarRoomSocketEvent) => {
    switch (event.kind) {
      case "message.created":
      case "message.updated":
        runInAction(() => {
          set(this.messagesMap, warRoomId, upsertMessageInList(this.messagesMap[warRoomId] ?? [], event.data));
        });
        break;
      case "message.deleted":
        runInAction(() => {
          set(
            this.messagesMap,
            warRoomId,
            (this.messagesMap[warRoomId] ?? []).filter((message) => message.id !== event.data.id)
          );
        });
        break;
      case "activity.created":
        runInAction(() => {
          const existing = this.eventsMap[warRoomId] ?? [];
          if (!existing.some((candidate) => candidate.id === event.data.id)) {
            set(this.eventsMap, warRoomId, [event.data, ...existing]);
          }
        });
        break;
      case "room.changed":
        if (event.data.reasons.includes("deleted")) {
          runInAction(() => {
            delete this.detailMap[warRoomId];
            delete this.warRoomMap[warRoomId];
            delete this.messagesMap[warRoomId];
            delete this.eventsMap[warRoomId];
          });
        } else {
          void this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
        }
        break;
      case "typing":
        this.applyTyping(warRoomId, event.data.user_id, event.data.is_typing);
        break;
      case "presence.joined":
        this.applyPresence(warRoomId, event.data.user_id, true);
        break;
      case "presence.left":
        this.applyPresence(warRoomId, event.data.user_id, false);
        break;
      default:
        break;
    }
  };

  applyTyping = (warRoomId: string, userId: string, isTyping: boolean) => {
    runInAction(() => {
      set(this.typingMap, warRoomId, {
        ...(this.typingMap[warRoomId] ?? {}),
        [userId]: isTyping ? Date.now() + TYPING_TIMEOUT_MS : 0,
      });
    });
  };

  applyPresence = (warRoomId: string, userId: string, isOnline: boolean) => {
    runInAction(() => {
      const current = this.onlineUsersMap[warRoomId] ?? [];
      set(
        this.onlineUsersMap,
        warRoomId,
        isOnline ? [...new Set([...current, userId])] : current.filter((id) => id !== userId)
      );
    });
  };

  incrementUnread = (warRoomId: string) => {
    runInAction(() => {
      set(this.unreadMap, warRoomId, (this.unreadMap[warRoomId] ?? 0) + 1);
    });
  };

  clearUnread = (warRoomId: string) => {
    runInAction(() => {
      set(this.unreadMap, warRoomId, 0);
    });
  };
}
```

- [ ] **Step 4: Run the store tests**

Run: `pnpm --filter=web test -- war-room.store`
Expected: PASS — all `WarRoomStore` suites.

- [ ] **Step 5: Commit**

```bash
pnpm fix:format
git add apps/web/core/store/war-room.store.ts apps/web/core/store/war-room.store.test.ts
git commit -m "feat(web): war room store for messages, events, presence and room mutations"
```

---

### Task 7: `useWarRoomSocket` hook

**Files:**

- Create: `apps/web/core/hooks/use-war-room-socket.ts`

- [ ] **Step 1: Create the hook**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
// plane imports
import { LIVE_BASE_PATH, LIVE_BASE_URL } from "@plane/constants";
import type { TWarRoomConnectionStatus, TWarRoomSocketEvent } from "@plane/types";
// hooks
import { useUser } from "@/hooks/store/user";

const HEARTBEAT_INTERVAL_MS = 30_000;
const MAX_RECONNECT_DELAY_MS = 30_000;
const AUTH_CLOSE_CODE = 4403;

type TUseWarRoomSocketArgs = {
  workspaceSlug: string;
  projectId: string;
  warRoomId: string;
  enabled?: boolean;
  onEvent: (event: TWarRoomSocketEvent) => void;
};

/**
 * One socket per war room. Connects to the Phase 2 live relay
 * (`/live/war-rooms/:roomId`), answers heartbeats, reconnects with exponential
 * backoff and stops permanently on the 4403 auth close.
 */
export const useWarRoomSocket = ({
  workspaceSlug,
  projectId,
  warRoomId,
  enabled = true,
  onEvent,
}: TUseWarRoomSocketArgs) => {
  // store hooks
  const { data: currentUser } = useUser();
  // states
  const [status, setStatus] = useState<TWarRoomConnectionStatus>("connecting");
  // refs
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const heartbeatTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const attemptRef = useRef(0);
  const onEventRef = useRef(onEvent);
  const currentUserId = currentUser?.id;

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  const buildUrl = useCallback(() => {
    if (typeof window === "undefined" || !currentUserId) return null;
    try {
      const base = LIVE_BASE_URL?.trim() || window.location.origin;
      const url = new URL(base);
      url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      url.pathname = `${LIVE_BASE_PATH}/war-rooms/${warRoomId}`;
      url.searchParams.set("workspaceSlug", workspaceSlug);
      url.searchParams.set("projectId", projectId);
      url.searchParams.set("token", JSON.stringify({ id: currentUserId }));
      return url.toString();
    } catch {
      return null;
    }
  }, [currentUserId, projectId, warRoomId, workspaceSlug]);

  const clearTimers = useCallback(() => {
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    if (heartbeatTimerRef.current) {
      clearInterval(heartbeatTimerRef.current);
      heartbeatTimerRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!enabled) {
      setStatus("disabled");
      return;
    }
    let disposed = false;

    const connect = () => {
      if (disposed) return;
      const url = buildUrl();
      if (!url) {
        setStatus("connecting");
        return;
      }
      setStatus(attemptRef.current === 0 ? "connecting" : "reconnecting");
      const socket = new WebSocket(url);
      socketRef.current = socket;

      const handleOpen = () => {
        if (disposed) {
          socket.close();
          return;
        }
        attemptRef.current = 0;
        setStatus("connected");
        heartbeatTimerRef.current = setInterval(() => {
          if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: "ping" }));
        }, HEARTBEAT_INTERVAL_MS);
      };

      const handleMessage = (message: MessageEvent) => {
        try {
          const event = JSON.parse(message.data as string) as TWarRoomSocketEvent;
          if (event?.kind) onEventRef.current(event);
        } catch {
          // ignore malformed payloads
        }
      };

      const handleClose = (event: CloseEvent) => {
        clearTimers();
        socketRef.current = null;
        if (disposed) return;
        if (event.code === AUTH_CLOSE_CODE) {
          setStatus("disabled");
          return;
        }
        setStatus("reconnecting");
        const delay = Math.min(MAX_RECONNECT_DELAY_MS, 1000 * 2 ** attemptRef.current);
        attemptRef.current += 1;
        reconnectTimerRef.current = setTimeout(connect, delay);
      };

      const handleError = () => {
        socket.close();
      };

      socket.addEventListener("open", handleOpen);
      socket.addEventListener("message", handleMessage);
      socket.addEventListener("close", handleClose);
      socket.addEventListener("error", handleError);
    };

    connect();

    return () => {
      disposed = true;
      clearTimers();
      const socket = socketRef.current;
      socketRef.current = null;
      if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING))
        socket.close();
    };
  }, [buildUrl, clearTimers, enabled]);

  const sendTyping = useCallback((isTyping: boolean) => {
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN) return;
    socket.send(JSON.stringify({ type: "typing", is_typing: isTyping }));
  }, []);

  return useMemo(() => ({ status, sendTyping }), [sendTyping, status]);
};
```

- [ ] **Step 2: Typecheck and lint**

Run: `pnpm --filter=web check:types && pnpm check:lint`
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
pnpm fix:format
git add apps/web/core/hooks/use-war-room-socket.ts
git commit -m "feat(web): war room websocket hook with reconnect and heartbeat"
```

---

### Task 8: Extract `ServiceGraphCanvas` (behavior-preserving refactor)

**Files:**

- Create: `apps/web/core/components/services/graph/service-graph-canvas.tsx`
- Modify: `apps/web/core/components/services/graph/service-graph.tsx` (rewrite)
- Modify: `apps/web/core/components/services/graph/service-node.tsx` (add `dimmed`)

- [ ] **Step 1: Create `service-graph-canvas.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef } from "react";
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
  type ReactFlowInstance,
} from "@xyflow/react";
// oxlint-disable-next-line import/no-unassigned-import
import "@xyflow/react/dist/style.css";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IService, TServiceGraphData } from "@plane/types";
// components
import { ServiceNode } from "./service-node";
import { getLayoutedElements } from "./use-graph-layout";

const nodeTypes = { service: ServiceNode };

export type TServiceGraphCanvasProps = {
  graphData: TServiceGraphData;
  readOnly?: boolean;
  /** When provided, non-listed services are dimmed (war room blast radius). */
  highlightedServiceIds?: string[];
  onNodeClick?: (serviceId: string) => void;
  onConnect?: (sourceServiceId: string, targetServiceId: string) => void;
  onEdgeDelete?: (dependencyId: string) => void;
  onNodeDragStop?: (serviceId: string, position: { x: number; y: number }) => void;
};

/**
 * Presentational React Flow canvas. Owns layout + i18n labels + viewport state;
 * mutation callbacks are optional so the same canvas serves the editable
 * Services page and the read-only war room map.
 */
export function ServiceGraphCanvas({
  graphData,
  readOnly = false,
  highlightedServiceIds,
  onNodeClick,
  onConnect,
  onEdgeDelete,
  onNodeDragStop,
}: TServiceGraphCanvasProps) {
  // plane hooks
  const { t, currentLocale } = useTranslation();
  // refs
  const fitDone = useRef(false);
  // derived values
  const highlighted = useMemo(() => new Set(highlightedServiceIds ?? []), [highlightedServiceIds]);

  const { nodes: layoutNodes, edges: layoutEdges } = useMemo(
    () => getLayoutedElements(graphData.services, graphData.dependencies),
    [graphData.dependencies, graphData.services]
  );

  const translatedNodes = useMemo(
    () =>
      layoutNodes.map((node) => {
        const service = (node.data as { service?: IService } | undefined)?.service;
        if (!service) return node;
        const health = graphData.health[service.id];
        return {
          ...node,
          data: {
            ...(node.data as Record<string, unknown>),
            service,
            statusLabel: t(`service.status_values.${service.status}`),
            criticalityLabel: t(`service.criticality_values.${service.criticality}`),
            health: health?.health ?? "unknown",
            incidents: health?.incidents ?? [],
            dimmed: highlighted.size > 0 && !highlighted.has(service.id),
          },
        };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
    [layoutNodes, currentLocale, graphData.health, highlighted]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(translatedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutEdges);

  useEffect(() => {
    setNodes((prev) => {
      if (
        prev.length === translatedNodes.length &&
        prev.every((node, i) => {
          const next = translatedNodes[i];
          if (node.id !== next?.id) return false;
          const prevData = node.data as
            | {
                statusLabel?: string;
                criticalityLabel?: string;
                health?: string;
                incidents?: unknown[];
                dimmed?: boolean;
              }
            | undefined;
          const nextData = next?.data as
            | {
                statusLabel?: string;
                criticalityLabel?: string;
                health?: string;
                incidents?: unknown[];
                dimmed?: boolean;
              }
            | undefined;
          return (
            prevData?.statusLabel === nextData?.statusLabel &&
            prevData?.criticalityLabel === nextData?.criticalityLabel &&
            prevData?.health === nextData?.health &&
            prevData?.dimmed === nextData?.dimmed &&
            (prevData?.incidents?.length ?? 0) === (nextData?.incidents?.length ?? 0)
          );
        })
      )
        return prev;
      const selectedById = new Map(prev.map((node) => [node.id, node.selected]));
      return translatedNodes.map((node) =>
        selectedById.has(node.id) ? { ...node, selected: selectedById.get(node.id) } : node
      );
    });
    setEdges((prev) => {
      if (prev.length === layoutEdges.length && prev.every((edge, i) => edge.id === layoutEdges[i]?.id)) return prev;
      const selectedById = new Map(prev.map((edge) => [edge.id, edge.selected]));
      return layoutEdges.map((edge) =>
        selectedById.has(edge.id) ? { ...edge, selected: selectedById.get(edge.id) } : edge
      );
    });
  }, [translatedNodes, layoutEdges, setNodes, setEdges]);

  const handleConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      onConnect?.(connection.source, connection.target);
    },
    [onConnect]
  );

  const handleEdgesDelete = useCallback(
    (deleted: Edge[]) => {
      deleted.forEach((edge) => onEdgeDelete?.(edge.id));
    },
    [onEdgeDelete]
  );

  const handleNodeDragStop = useCallback(
    (_event: unknown, node: Node) => {
      onNodeDragStop?.(node.id, node.position);
    },
    [onNodeDragStop]
  );

  const handleNodeClick = useCallback(
    (_event: unknown, node: Node) => {
      onNodeClick?.(node.id);
    },
    [onNodeClick]
  );

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onConnect={readOnly ? undefined : handleConnect}
      onEdgesDelete={readOnly ? undefined : handleEdgesDelete}
      onNodeDragStop={readOnly ? undefined : handleNodeDragStop}
      onNodeClick={handleNodeClick}
      nodesDraggable={!readOnly}
      nodesConnectable={!readOnly}
      edgesFocusable={!readOnly}
      elementsSelectable={!readOnly}
      deleteKeyCode={readOnly ? null : ["Backspace", "Delete"]}
      onInit={(instance: ReactFlowInstance) => {
        if (!fitDone.current) {
          fitDone.current = true;
          instance.fitView();
        }
      }}
    >
      <Background />
      <Controls />
      {!readOnly && <MiniMap />}
    </ReactFlow>
  );
}
```

- [ ] **Step 2: Rewrite `service-graph.tsx` to wrap the canvas**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TServiceGraphData } from "@plane/types";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
// components
import { ServiceGraphCanvas } from "./service-graph-canvas";

export const ServiceGraph = observer(function ServiceGraph() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getGraphData, addDependency, removeDependency, updateNodePosition, updateService } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString();
  const workspaceId = currentWorkspace?.id;
  // states
  const [isReLayouting, setIsReLayouting] = useState(false);

  const graphData: TServiceGraphData = pid ? getGraphData(pid) : { services: [], dependencies: [], health: {} };

  const handleConnect = useCallback(
    async (source: string, target: string) => {
      if (!slug || !workspaceId || !pid) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not create dependency. Please try again.",
        });
        return;
      }
      try {
        await addDependency(slug, workspaceId, pid, source, target);
      } catch (error) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: error instanceof Error ? error.message : "Could not create dependency. Please try again.",
        });
      }
    },
    [addDependency, pid, slug, workspaceId]
  );

  const handleEdgeDelete = useCallback(
    (dependencyId: string) => {
      if (!slug || !workspaceId || !pid) return;
      void removeDependency(slug, workspaceId, pid, dependencyId).catch(() => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not delete dependency. Please try again.",
        });
      });
    },
    [pid, removeDependency, slug, workspaceId]
  );

  const handleNodeDragStop = useCallback(
    (serviceId: string, position: { x: number; y: number }) => {
      if (!slug || !workspaceId || !pid) return;
      void updateNodePosition(slug, workspaceId, pid, serviceId, position).catch(() => {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not save node position. Please try again.",
        });
      });
    },
    [pid, slug, updateNodePosition, workspaceId]
  );

  const handleNodeClick = useCallback(
    (serviceId: string) => {
      if (!slug || !pid) return;
      router.push(`/${slug}/projects/${pid}/services/${serviceId}`);
    },
    [pid, router, slug]
  );

  const handleReLayout = useCallback(async () => {
    if (!slug || !workspaceId || !pid) return;
    const positioned = graphData.services.filter((service) => service.position !== null);
    if (positioned.length === 0) return;
    setIsReLayouting(true);
    try {
      await Promise.all(
        positioned.map((service) => updateService(slug, workspaceId, pid, service.id, { position: null }))
      );
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: "Could not reset layout. Please try again.",
      });
    } finally {
      setIsReLayouting(false);
    }
  }, [graphData.services, pid, slug, updateService, workspaceId]);

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between gap-2 px-2 py-1.5">
        <p className="text-xs text-secondary">{t("service.graph.connect_hint")}</p>
        <Button variant="secondary" size="sm" onClick={handleReLayout} loading={isReLayouting}>
          {t("service.graph.re_layout")}
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <ServiceGraphCanvas
          graphData={graphData}
          onConnect={handleConnect}
          onEdgeDelete={handleEdgeDelete}
          onNodeDragStop={handleNodeDragStop}
          onNodeClick={handleNodeClick}
        />
      </div>
    </div>
  );
});
```

- [ ] **Step 3: Add `dimmed` to the node renderer**

Replace `apps/web/core/components/services/graph/service-node.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
// plane imports
import type { IService, IServiceIncident, TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceHealthDot } from "../health/service-health-dot";

export type TServiceNodeData = {
  service: IService;
  statusLabel: string;
  criticalityLabel: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
  /** Blast-radius map: dependency neighbors are dimmed. */
  dimmed?: boolean;
};

export function ServiceNode({ data }: NodeProps<Node<TServiceNodeData>>) {
  const service = (data as TServiceNodeData | undefined)?.service;
  if (!service) return null;
  const { statusLabel, criticalityLabel, health, incidents, dimmed } = data as TServiceNodeData;
  const state = health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];

  return (
    <div
      className={cn(
        "shadow-sm relative w-[220px] overflow-hidden rounded-md border border-subtle bg-surface-1 px-3 py-2",
        dimmed && "opacity-60"
      )}
    >
      <Handle type="target" position={Position.Left} className="!bg-surface-2" />
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", config.rail)} />
      {incidents.length > 0 && (
        <span className="absolute -top-1.5 -right-1.5 grid h-4 min-w-4 place-items-center rounded-full bg-danger-primary px-1 text-10 font-medium text-on-color">
          {incidents.length}
        </span>
      )}
      <div className="flex min-w-0 items-center gap-2 pl-1">
        <ServiceHealthDot health={state} />
        <span className="text-sm min-w-0 flex-1 truncate font-medium text-primary" title={service.name}>
          {service.name}
        </span>
      </div>
      <div className="text-xs mt-1 pl-1 text-secondary capitalize">
        {statusLabel} · {criticalityLabel}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-surface-2" />
    </div>
  );
}
```

- [ ] **Step 4: Verify the Services page still compiles and behaves**

Run:

```bash
pnpm --filter=web check:types
pnpm check:lint
```

Expected: no new errors. The Services page still renders the same graph (same layout, hint, Re-layout button, connect/delete/drag callbacks); the only intentional change is the canvas internals.

- [ ] **Step 5: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/services/graph
git commit -m "refactor(web): extract read-only capable service graph canvas"
```

---

### Task 9: Blast-radius service map panel

**Files:**

- Create: `apps/web/core/components/war-rooms/room/service-map.tsx`

- [ ] **Step 1: Create `service-map.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService, IServiceHealthSnapshot, IWarRoom, TServiceGraphData } from "@plane/types";
// components
import { ServiceGraphCanvas } from "@/components/services/graph/service-graph-canvas";
import { ServiceMultiSelect } from "@/components/services/select/service-multi-select";
// helpers
import { getBlastRadius } from "@/services/war-room.helpers";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomServiceMap = observer(function WarRoomServiceMap({
  workspaceSlug,
  projectId,
  room,
  canWrite,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { fetchedMap, loader, fetchServices, getServiceById, getServiceHealth, getDependenciesByProject } =
    useService();
  const { addServices, removeService } = useWarRoom();
  const { currentWorkspace } = useWorkspace();
  // states
  const [isCollapsed, setIsCollapsed] = useState(false);
  // derived values
  const workspaceId = currentWorkspace?.id;
  const affectedServiceIds = useMemo(() => room.services.map((service) => service.id), [room.services]);
  const dependencies = getDependenciesByProject(projectId);
  const blastRadius = useMemo(
    () => getBlastRadius(affectedServiceIds, dependencies),
    [affectedServiceIds, dependencies]
  );
  const graphData: TServiceGraphData = useMemo(() => {
    const includedIds = new Set(blastRadius.includedIds);
    const services = blastRadius.includedIds
      .map((serviceId) => getServiceById(serviceId))
      .filter((service): service is IService => Boolean(service));
    const health: Record<string, IServiceHealthSnapshot> = {};
    services.forEach((service) => {
      const snapshot = getServiceHealth(service.id);
      if (snapshot) health[service.id] = snapshot;
    });
    return {
      services,
      dependencies: dependencies.filter(
        (dependency) => includedIds.has(dependency.from_service_id) && includedIds.has(dependency.to_service_id)
      ),
      health,
    };
  }, [blastRadius, dependencies, getServiceById, getServiceHealth]);

  useEffect(() => {
    if (!workspaceId || fetchedMap[projectId] || loader) return;
    void fetchServices(workspaceSlug, workspaceId, projectId);
  }, [fetchedMap, fetchServices, loader, projectId, workspaceId, workspaceSlug]);

  const handleServicesChange = async (nextIds: string[]) => {
    const current = new Set(affectedServiceIds);
    const next = new Set(nextIds);
    const toAdd = nextIds.filter((serviceId) => !current.has(serviceId));
    const toRemove = affectedServiceIds.filter((serviceId) => !next.has(serviceId));
    try {
      if (toAdd.length > 0) await addServices(workspaceSlug, projectId, room.id, toAdd);
      await Promise.all(toRemove.map((serviceId) => removeService(workspaceSlug, projectId, room.id, serviceId)));
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("toast.error"),
        message: t("war_room.map.link_failed"),
      });
    }
  };

  const handleNodeClick = (serviceId: string) => {
    window.open(`/${workspaceSlug}/projects/${projectId}/services/${serviceId}`, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex items-center justify-between gap-2 border-b border-subtle px-3 py-1.5">
        <div className="flex min-w-0 items-center gap-3">
          <span className="text-11 shrink-0 font-medium tracking-wide text-tertiary uppercase">
            {t("war_room.map.title")}
          </span>
          <span className="hidden items-center gap-1 text-10 text-secondary sm:flex">
            <span className="h-2 w-2 rounded-full bg-danger-primary" />
            {t("war_room.map.affected_legend")}
          </span>
          <span className="hidden items-center gap-1 text-10 text-tertiary sm:flex">
            <span className="h-2 w-2 rounded-full bg-layer-3" />
            {t("war_room.map.neighbor_legend")}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {canWrite && (
            <ServiceMultiSelect
              workspaceSlug={workspaceSlug}
              projectId={projectId}
              value={affectedServiceIds}
              onChange={(serviceIds) => void handleServicesChange(serviceIds)}
            />
          )}
          <Button variant="secondary" size="sm" onClick={() => setIsCollapsed((value) => !value)}>
            {isCollapsed ? t("war_room.map.expand") : t("war_room.map.collapse")}
          </Button>
        </div>
      </div>
      {!isCollapsed && (
        <div className="min-h-0 flex-1">
          {graphData.services.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
              <p className="text-13 font-medium text-primary">{t("war_room.map.empty_title")}</p>
              <p className="text-12 text-secondary">{t("war_room.map.empty_description")}</p>
            </div>
          ) : (
            <ServiceGraphCanvas
              graphData={graphData}
              readOnly
              highlightedServiceIds={blastRadius.affectedIds}
              onNodeClick={handleNodeClick}
            />
          )}
        </div>
      )}
    </div>
  );
});
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/service-map.tsx
git commit -m "feat(web): war room blast radius service map"
```

---

### Task 10: Notes panel with autosave

**Files:**

- Create: `apps/web/core/components/war-rooms/room/context/notes.tsx`

- [ ] **Step 1: Create `context/notes.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { debounce } from "lodash-es";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom } from "@plane/types";
import { calculateTimeAgo } from "@plane/utils";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS } from "@/services/service.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

type TSavingState = "idle" | "saving" | "saved";

export const WarRoomNotes = observer(function WarRoomNotes({ workspaceSlug, projectId, room, canWrite }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateWarRoom } = useWarRoom();
  const { getWorkspaceBySlug } = useWorkspace();
  // states
  const [savingState, setSavingState] = useState<TSavingState>("idle");
  // refs
  const saveRef = useRef<(html: string) => void>(() => undefined);
  const workspaceId = getWorkspaceBySlug(workspaceSlug)?.id ?? "";

  saveRef.current = (html: string) => {
    setSavingState("saving");
    void updateWarRoom(workspaceSlug, projectId, room.id, { notes_html: html })
      .then(() => setSavingState("saved"))
      .catch(() => {
        setSavingState("idle");
        setToast({
          type: TOAST_TYPE.ERROR,
          title: t("toast.error"),
          message: t("war_room.notes.save_failed"),
        });
      });
  };

  const debouncedSave = useMemo(
    () => debounce((html: string) => saveRef.current(html), 1000),
    // oxlint-disable-next-line exhaustive-deps -- debounce instance must survive re-renders
    []
  );

  useEffect(() => () => debouncedSave.flush(), [debouncedSave]);

  const handleChange = (_json: object, html: string) => {
    setSavingState("saving");
    debouncedSave(html);
  };

  const statusLabel =
    savingState === "saving"
      ? t("war_room.notes.saving")
      : savingState === "saved"
        ? t("war_room.notes.saved")
        : t("war_room.notes.edited_at", { time: calculateTimeAgo(room.updated_at) });

  const editorProps = {
    id: "war-room-notes-editor",
    initialValue: room.notes_html,
    workspaceSlug,
    workspaceId,
    projectId,
    disabledExtensions: SERVICE_DESCRIPTION_DISABLED_EXTENSIONS,
    onChange: handleChange,
    placeholder: t("war_room.notes.placeholder"),
    containerClassName: "min-h-48",
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.notes")}
        </span>
        <span className="text-10 text-tertiary">{statusLabel}</span>
      </div>
      {!canWrite && (
        <p className="border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 text-secondary">
          {t("war_room.notes.read_only")}
        </p>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {canWrite ? (
          <RichTextEditor
            {...editorProps}
            key={room.id}
            editable
            searchMentionCallback={async () => ({})}
            uploadFile={async () => {
              throw new Error("File upload is disabled for war room notes.");
            }}
            duplicateFile={async () => {
              throw new Error("File upload is disabled for war room notes.");
            }}
          />
        ) : (
          <RichTextEditor {...editorProps} key={room.id} editable={false} />
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors. `TSearchResponse` is an all-optional object, so `async () => ({})` is the valid empty response.

- [ ] **Step 3: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/context/notes.tsx
git commit -m "feat(web): war room notes panel with autosave"
```

---

### Task 11: Chat panel (list, composer with mentions, typing, reconnect)

**Files:**

- Create: `apps/web/core/components/war-rooms/room/chat/message-item.tsx`
- Create: `apps/web/core/components/war-rooms/room/chat/composer.tsx`
- Create: `apps/web/core/components/war-rooms/room/chat/root.tsx`

- [ ] **Step 1: Create `chat/message-item.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoomMessage } from "@plane/types";
import { CustomMenu } from "@plane/ui";
import { cn, getFileURL } from "@plane/utils";
// helpers
import {
  parseMessageSegments,
  shouldShowMessageHeader,
  type TWarRoomMessageSegment,
} from "@/services/war-room.helpers";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  message: IWarRoomMessage;
  previousMessage?: IWarRoomMessage;
  workspaceSlug: string;
  projectId: string;
  roomId: string;
  canWrite: boolean;
  currentUserId?: string;
};

export const WarRoomMessageItem = observer(function WarRoomMessageItem({
  message,
  previousMessage,
  workspaceSlug,
  projectId,
  roomId,
  canWrite,
  currentUserId,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateMessage, deleteMessage } = useWarRoom();
  const { getUserDetails } = useMember();
  // states
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState(message.body);
  // derived values
  const showHeader = shouldShowMessageHeader(previousMessage, message);
  const isOwn = Boolean(currentUserId) && message.author_id === currentUserId;
  const authorName =
    message.author?.display_name ?? (message.author_id ? getUserDetails(message.author_id)?.display_name : null);
  const segments = parseMessageSegments(message.body);
  // Content-based keys: stable across renders and unique within one message
  // (`no-array-index-key` forbids the map index in keys).
  const seenKeys = new Map<string, number>();
  const keyForSegment = (segment: TWarRoomMessageSegment): string => {
    const base = segment.type === "mention" ? `mention-${segment.user_id}` : `text-${segment.value}`;
    const count = (seenKeys.get(base) ?? 0) + 1;
    seenKeys.set(base, count);
    return `${base}-${count}`;
  };

  const handleSave = async () => {
    const body = editValue.trim();
    if (body === "" || body === message.body) {
      setIsEditing(false);
      return;
    }
    try {
      await updateMessage(workspaceSlug, projectId, roomId, message.id, body);
      setIsEditing(false);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.errors.generic") });
    }
  };

  const handleDelete = async () => {
    try {
      await deleteMessage(workspaceSlug, projectId, roomId, message.id);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.errors.generic") });
    }
  };

  return (
    <div className={cn("group/message flex gap-2 px-1", showHeader ? "mt-3" : "mt-0.5")}>
      <div className="w-6 shrink-0">
        {showHeader && (
          <Avatar
            size="sm"
            src={message.author?.avatar_url ? getFileURL(message.author.avatar_url) : undefined}
            alt={authorName ?? ""}
            fallback={authorName?.[0]?.toUpperCase()}
          />
        )}
      </div>
      <div className="min-w-0 flex-1">
        {showHeader && (
          <div className="flex items-center gap-2">
            <span className="text-12 font-medium text-primary">
              {authorName ?? t("war_room.activity.actor_unknown")}
            </span>
            <span className="text-10 text-tertiary">
              {new Date(message.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </span>
            {isOwn && canWrite && !isEditing && (
              <span className="opacity-0 transition-opacity group-hover/message:opacity-100">
                <CustomMenu ellipsis className="h-4" placement="bottom-end">
                  <CustomMenu.MenuItem
                    onClick={() => {
                      setEditValue(message.body);
                      setIsEditing(true);
                    }}
                  >
                    {t("war_room.chat.edit")}
                  </CustomMenu.MenuItem>
                  <CustomMenu.MenuItem onClick={() => void handleDelete()}>
                    {t("war_room.chat.delete")}
                  </CustomMenu.MenuItem>
                </CustomMenu>
              </span>
            )}
          </div>
        )}
        {isEditing ? (
          <div className="mt-1 space-y-1">
            <textarea
              value={editValue}
              onChange={(event) => setEditValue(event.target.value)}
              className="w-full resize-none rounded-sm border border-subtle bg-surface-1 px-2 py-1 text-13 text-primary outline-none focus:border-strong"
              rows={2}
            />
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" onClick={() => void handleSave()}>
                {t("war_room.chat.save")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setIsEditing(false)}>
                {t("war_room.chat.cancel")}
              </Button>
            </div>
          </div>
        ) : (
          <p className="text-13 break-words whitespace-pre-wrap text-primary">
            {segments.map((segment) =>
              segment.type === "mention" ? (
                <span key={keyForSegment(segment)} className="rounded-xs bg-accent-subtle px-1 text-accent-primary">
                  @{getUserDetails(segment.user_id)?.display_name ?? segment.user_id}
                </span>
              ) : (
                <span key={keyForSegment(segment)}>{segment.value}</span>
              )
            )}
            {message.edited_at && <span className="ml-1 text-10 text-tertiary">({t("war_room.chat.edited")})</span>}
          </p>
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Create `chat/composer.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { getFileURL } from "@plane/utils";
// helpers
import { serializeMentionTokens, type TWarRoomMention } from "@/services/war-room.helpers";
// hooks
import { useMember } from "@/hooks/store/use-member";

const TYPING_THROTTLE_MS = 2000;
const MAX_SUGGESTIONS = 6;

type Props = {
  workspaceSlug: string;
  projectId: string;
  disabled?: boolean;
  onSend: (body: string) => Promise<boolean>;
  onTyping: (isTyping: boolean) => void;
};

export const WarRoomChatComposer = observer(function WarRoomChatComposer({
  workspaceSlug,
  projectId,
  disabled = false,
  onSend,
  onTyping,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getUserDetails,
    project: { getProjectMemberIds, fetchProjectMembers },
  } = useMember();
  // states
  const [value, setValue] = useState("");
  const [mentions, setMentions] = useState<TWarRoomMention[]>([]);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [isSending, setIsSending] = useState(false);
  // refs
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lastTypingRef = useRef(0);
  // derived values
  const memberIds = getProjectMemberIds(projectId, true) ?? [];
  const candidates =
    mentionQuery === null
      ? []
      : memberIds
          .map((memberId) => ({ id: memberId, display_name: getUserDetails(memberId)?.display_name ?? "" }))
          .filter((member) => member.display_name.toLowerCase().includes(mentionQuery.toLowerCase()))
          .slice(0, MAX_SUGGESTIONS);

  useEffect(() => {
    if (memberIds.length === 0) void fetchProjectMembers(workspaceSlug, projectId);
    // oxlint-disable-next-line exhaustive-deps -- hydrate once per project
  }, [projectId, workspaceSlug]);

  const handleChange = (event: ChangeEvent<HTMLTextAreaElement>) => {
    const nextValue = event.target.value;
    setValue(nextValue);
    const cursor = event.target.selectionStart ?? nextValue.length;
    const match = nextValue.slice(0, cursor).match(/(?:^|\s)@([^\s@]*)$/);
    setMentionQuery(match ? match[1] : null);
    setActiveIndex(0);
    const now = Date.now();
    if (now - lastTypingRef.current > TYPING_THROTTLE_MS) {
      lastTypingRef.current = now;
      onTyping(true);
    }
  };

  const insertMention = (member: TWarRoomMention) => {
    const textarea = textareaRef.current;
    const cursor = textarea?.selectionStart ?? value.length;
    const before = value.slice(0, cursor).replace(/@([^\s@]*)$/, "");
    const after = value.slice(cursor);
    setValue(`${before}@${member.display_name} ${after}`);
    setMentions((current) => [...current.filter((candidate) => candidate.id !== member.id), member]);
    setMentionQuery(null);
    requestAnimationFrame(() => textarea?.focus());
  };

  const handleSubmit = async () => {
    if (disabled || isSending) return;
    const body = serializeMentionTokens(value, mentions).trim();
    if (body === "") return;
    setIsSending(true);
    const sent = await onSend(body);
    setIsSending(false);
    if (sent) {
      setValue("");
      setMentions([]);
      setMentionQuery(null);
      onTyping(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (mentionQuery !== null && candidates.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((index) => (index + 1) % candidates.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((index) => (index - 1 + candidates.length) % candidates.length);
        return;
      }
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        insertMention(candidates[activeIndex]);
        return;
      }
      if (event.key === "Escape") {
        setMentionQuery(null);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void handleSubmit();
    }
  };

  return (
    <div className="relative border-t border-subtle p-2">
      {mentionQuery !== null && candidates.length > 0 && (
        <div className="absolute right-2 bottom-full left-2 z-10 mb-1 max-h-56 overflow-y-auto rounded-md border border-subtle bg-surface-1 py-1 shadow-lg">
          {candidates.map((candidate, index) => (
            <button
              key={candidate.id}
              type="button"
              onClick={() => insertMention(candidate)}
              className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-12 hover:bg-layer-transparent-hover ${
                index === activeIndex ? "bg-layer-transparent-hover" : ""
              }`}
            >
              <Avatar
                size="xs"
                src={
                  getUserDetails(candidate.id)?.avatar_url
                    ? getFileURL(getUserDetails(candidate.id)?.avatar_url ?? "")
                    : undefined
                }
                alt={candidate.display_name}
                fallback={candidate.display_name[0]?.toUpperCase()}
              />
              <span className="truncate text-primary">{candidate.display_name}</span>
            </button>
          ))}
        </div>
      )}
      {mentionQuery !== null && candidates.length === 0 && (
        <div className="absolute right-2 bottom-full left-2 z-10 mb-1 rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-12 text-tertiary shadow-lg">
          {t("war_room.chat.mentions_empty")}
        </div>
      )}
      <div className="flex items-end gap-2">
        <textarea
          ref={textareaRef}
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onBlur={() => onTyping(false)}
          disabled={disabled}
          rows={2}
          placeholder={disabled ? t("war_room.archived_notice") : t("war_room.chat.composer_placeholder")}
          className="min-h-9 flex-1 resize-none rounded-sm border border-subtle bg-surface-1 px-2 py-1.5 text-13 text-primary outline-none focus:border-strong disabled:cursor-not-allowed disabled:opacity-60"
        />
        <Button
          variant="primary"
          size="sm"
          disabled={disabled || isSending || value.trim() === ""}
          onClick={() => void handleSubmit()}
        >
          {t("war_room.chat.send")}
        </Button>
      </div>
    </div>
  );
});
```

- [ ] **Step 3: Create `chat/root.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, TWarRoomConnectionStatus } from "@plane/types";
// components
import { WarRoomChatComposer } from "./composer";
import { WarRoomMessageItem } from "./message-item";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUser } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
  connectionStatus: TWarRoomConnectionStatus;
  sendTyping: (isTyping: boolean) => void;
};

const AT_BOTTOM_THRESHOLD_PX = 80;
const TYPING_RENDER_INTERVAL_MS = 1000;

export const WarRoomChat = observer(function WarRoomChat({
  workspaceSlug,
  projectId,
  room,
  canWrite,
  connectionStatus,
  sendTyping,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getMessages,
    hasMoreMessages,
    fetchMessages,
    sendMessage,
    getUnreadCount,
    incrementUnread,
    clearUnread,
    getTypingUserIds,
  } = useWarRoom();
  const { getUserDetails } = useMember();
  const { data: currentUser } = useUser();
  // states
  const [, setTypingTick] = useState(0);
  // refs
  const scrollRef = useRef<HTMLDivElement>(null);
  const isAtBottomRef = useRef(true);
  const previousLengthRef = useRef(0);
  // derived values
  const messages = getMessages(room.id);
  const hasMore = hasMoreMessages(room.id);
  const unreadCount = getUnreadCount(room.id);
  const currentUserId = currentUser?.id;
  const typingUserIds = getTypingUserIds(room.id).filter((userId) => userId !== currentUserId);
  const typingNames = typingUserIds.map((userId) => getUserDetails(userId)?.display_name).filter(Boolean) as string[];

  useEffect(() => {
    const timer = setInterval(() => setTypingTick((tick) => tick + 1), TYPING_RENDER_INTERVAL_MS);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    clearUnread(room.id);
    previousLengthRef.current = 0;
    void fetchMessages(workspaceSlug, projectId, room.id).catch(() => {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.chat.load_failed") });
    });
    // oxlint-disable-next-line exhaustive-deps -- fetch once per room
  }, [room.id, workspaceSlug, projectId]);

  const handleScroll = () => {
    const node = scrollRef.current;
    if (!node) return;
    const distance = node.scrollHeight - node.scrollTop - node.clientHeight;
    isAtBottomRef.current = distance < AT_BOTTOM_THRESHOLD_PX;
    if (isAtBottomRef.current) clearUnread(room.id);
  };

  useEffect(() => {
    const length = messages.length;
    const grew = length > previousLengthRef.current;
    previousLengthRef.current = length;
    if (!grew) return;
    const node = scrollRef.current;
    if (!node) return;
    const lastMessage = messages[messages.length - 1];
    if (isAtBottomRef.current) {
      node.scrollTop = node.scrollHeight;
    } else if (lastMessage && lastMessage.author_id !== currentUserId) {
      incrementUnread(room.id);
    }
  }, [messages, room.id, currentUserId, incrementUnread]);

  const handleLoadOlder = async () => {
    const oldest = messages[0];
    if (!oldest) return;
    try {
      await fetchMessages(workspaceSlug, projectId, room.id, { before_id: oldest.id });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.chat.load_failed") });
    }
  };

  const handleSend = useCallback(
    async (body: string): Promise<boolean> => {
      try {
        await sendMessage(workspaceSlug, projectId, room.id, body, crypto.randomUUID());
        return true;
      } catch {
        setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.chat.send_failed") });
        return false;
      }
    },
    [projectId, room.id, sendMessage, t, workspaceSlug]
  );

  const scrollToBottom = () => {
    const node = scrollRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    isAtBottomRef.current = true;
    clearUnread(room.id);
  };

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">{t("war_room.chat.title")}</span>
        <span className="truncate text-10 text-tertiary">
          {typingNames.length === 1
            ? t("war_room.chat.typing_single", { name: typingNames[0] })
            : typingNames.length > 1
              ? t("war_room.chat.typing_multiple", { count: typingNames.length })
              : ""}
        </span>
      </div>
      {connectionStatus === "reconnecting" && (
        <div className="border-b border-subtle bg-warning-subtle px-3 py-1 text-11 text-warning-primary">
          {t("war_room.chat.reconnect_banner")}
        </div>
      )}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto px-2 py-2"
      >
        {hasMore && (
          <div className="flex justify-center py-1">
            <Button variant="secondary" size="sm" onClick={() => void handleLoadOlder()}>
              {t("war_room.chat.load_older")}
            </Button>
          </div>
        )}
        {messages.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.chat.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.chat.empty_description")}</p>
          </div>
        ) : (
          messages.map((message, index) => (
            <WarRoomMessageItem
              key={message.id}
              message={message}
              previousMessage={messages[index - 1]}
              workspaceSlug={workspaceSlug}
              projectId={projectId}
              roomId={room.id}
              canWrite={canWrite}
              currentUserId={currentUserId}
            />
          ))
        )}
      </div>
      {unreadCount > 0 && (
        <div className="flex justify-center">
          <button
            type="button"
            onClick={scrollToBottom}
            className="rounded-full bg-accent-primary px-3 py-1 text-11 font-medium text-on-color shadow-md"
          >
            {t("war_room.chat.new_messages", { count: unreadCount })}
          </button>
        </div>
      )}
      <WarRoomChatComposer
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        disabled={!canWrite}
        onSend={handleSend}
        onTyping={sendTyping}
      />
    </div>
  );
});
```

- [ ] **Step 4: Typecheck and lint**

Run: `pnpm --filter=web check:types && pnpm check:lint`
Expected: no new errors. `getFileURL` accepts `string`; if `undefined` slips through, keep the ternary guard.

- [ ] **Step 5: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/chat
git commit -m "feat(web): war room chat panel with mentions and realtime"
```

---

### Task 12: Work items panel

**Files:**

- Create: `apps/web/core/components/war-rooms/room/context/work-items.tsx`

- [ ] **Step 1: Create `context/work-items.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse, IWarRoom } from "@plane/types";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
// helpers
import { getWarRoomIncidentLink } from "@/services/war-room.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomWorkItems = observer(function WarRoomWorkItems({
  workspaceSlug,
  projectId,
  room,
  canWrite,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { addIssues, removeIssue } = useWarRoom();
  // states
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  // derived values
  const hiddenIssueIds = new Set([room.primary_issue_id, ...room.issues.map((issue) => issue.id)]);
  const isEmpty = !room.primary_issue && room.issues.length === 0;

  const handleAdd = async (issues: ISearchIssueResponse[]) => {
    const issueIds = issues.map((issue) => issue.id).filter((issueId) => !hiddenIssueIds.has(issueId));
    if (issueIds.length === 0) {
      setIsPickerOpen(false);
      return;
    }
    try {
      await addIssues(workspaceSlug, projectId, room.id, issueIds);
      setIsPickerOpen(false);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.work_items.link_failed") });
    }
  };

  const handleUnlink = async (issueId: string) => {
    try {
      await removeIssue(workspaceSlug, projectId, room.id, issueId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.work_items.unlink_failed") });
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.work_items")}
        </span>
        {canWrite && (
          <Button variant="secondary" size="sm" onClick={() => setIsPickerOpen(true)}>
            {t("war_room.work_items.add")}
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-3">
        {isEmpty && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.work_items.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.work_items.empty_description")}</p>
          </div>
        )}
        {room.primary_issue && (
          <div className="rounded-md border border-subtle bg-layer-1 p-2">
            <span className="rounded-full bg-accent-subtle px-1.5 py-0.5 text-10 font-medium text-accent-primary">
              {t("war_room.work_items.primary_badge")}
            </span>
            <Link
              href={getWarRoomIncidentLink(workspaceSlug, room.primary_issue.identifier)}
              className="mt-1 flex items-center gap-2 text-13 text-primary hover:underline"
            >
              <span className="shrink-0 font-medium text-tertiary">{room.primary_issue.identifier}</span>
              <span className="truncate">{room.primary_issue.name}</span>
            </Link>
          </div>
        )}
        {room.issues.map((issue) => (
          <div
            key={issue.id}
            className="flex items-center justify-between gap-2 rounded-md border border-subtle p-2 hover:bg-layer-transparent-hover"
          >
            <Link
              href={getWarRoomIncidentLink(workspaceSlug, issue.identifier)}
              className="flex min-w-0 flex-1 items-center gap-2 text-13 text-primary hover:underline"
            >
              <span className="shrink-0 font-medium text-tertiary">{issue.identifier}</span>
              <span className="truncate">{issue.name}</span>
            </Link>
            {canWrite && (
              <Button variant="tertiary" size="sm" onClick={() => void handleUnlink(issue.id)}>
                {t("war_room.work_items.unlink")}
              </Button>
            )}
          </div>
        ))}
      </div>
      <ExistingIssuesListModal
        isOpen={isPickerOpen}
        handleClose={() => setIsPickerOpen(false)}
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        searchParams={{ workspace_search: false }}
        shouldHideIssue={(issue) => hiddenIssueIds.has(issue.id)}
        handleOnSubmit={handleAdd}
      />
    </div>
  );
});
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/context/work-items.tsx
git commit -m "feat(web): war room work items panel"
```

---

### Task 13: Runbook panel

**Files:**

- Create: `apps/web/core/components/war-rooms/room/context/runbook.tsx`

- [ ] **Step 1: Create `context/runbook.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, IWarRoomRunbookItem } from "@plane/types";
import { cn } from "@plane/utils";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomRunbook = observer(function WarRoomRunbook({ workspaceSlug, projectId, room, canWrite }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { createRunbookItem, updateRunbookItem, deleteRunbookItem } = useWarRoom();
  // states
  const [newTitle, setNewTitle] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState("");
  // derived values
  const doneCount = room.runbook_items.filter((item) => item.is_done).length;
  const totalCount = room.runbook_items.length;
  const progress = totalCount === 0 ? 0 : Math.round((doneCount / totalCount) * 100);

  const showError = () =>
    setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.runbook.update_failed") });

  const handleCreate = async () => {
    const title = newTitle.trim();
    if (title === "") return;
    try {
      await createRunbookItem(workspaceSlug, projectId, room.id, title);
      setNewTitle("");
    } catch {
      showError();
    }
  };

  const handleToggle = async (item: IWarRoomRunbookItem) => {
    try {
      await updateRunbookItem(workspaceSlug, projectId, room.id, item.id, { is_done: !item.is_done });
    } catch {
      showError();
    }
  };

  const handleSaveTitle = async (itemId: string) => {
    const title = editingTitle.trim();
    if (title === "") return;
    try {
      await updateRunbookItem(workspaceSlug, projectId, room.id, itemId, { title });
      setEditingId(null);
    } catch {
      showError();
    }
  };

  const handleDelete = async (itemId: string) => {
    try {
      await deleteRunbookItem(workspaceSlug, projectId, room.id, itemId);
    } catch {
      showError();
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="border-b border-subtle px-3 py-1.5">
        <div className="flex items-center justify-between">
          <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
            {t("war_room.context_tabs.runbook")}
          </span>
          <span className="text-10 text-tertiary">
            {t("war_room.runbook.progress", { done: doneCount, total: totalCount })}
          </span>
        </div>
        <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-layer-2">
          <div className="h-full rounded-full bg-success-primary transition-all" style={{ width: `${progress}%` }} />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {room.runbook_items.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.runbook.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.runbook.empty_description")}</p>
          </div>
        )}
        <div className="space-y-1">
          {room.runbook_items.map((item) => (
            <div
              key={item.id}
              className="group/runbook flex items-start gap-2 rounded-md px-1 py-1.5 hover:bg-layer-transparent-hover"
            >
              <input
                type="checkbox"
                checked={item.is_done}
                disabled={!canWrite}
                onChange={() => void handleToggle(item)}
                aria-label={item.is_done ? t("war_room.runbook.mark_undone") : t("war_room.runbook.mark_done")}
                className="mt-0.5 h-3.5 w-3.5 shrink-0 accent-success-primary"
              />
              {editingId === item.id ? (
                <div className="flex min-w-0 flex-1 items-center gap-1">
                  <input
                    value={editingTitle}
                    onChange={(event) => setEditingTitle(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void handleSaveTitle(item.id);
                      if (event.key === "Escape") setEditingId(null);
                    }}
                    // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus -- inline edit input should take focus
                    autoFocus
                    className="min-w-0 flex-1 rounded-sm border border-subtle bg-surface-1 px-1.5 py-0.5 text-13 text-primary outline-none focus:border-strong"
                  />
                  <Button variant="primary" size="sm" onClick={() => void handleSaveTitle(item.id)}>
                    {t("war_room.runbook.save")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => setEditingId(null)}>
                    {t("war_room.runbook.cancel")}
                  </Button>
                </div>
              ) : (
                <>
                  <span
                    className={cn("min-w-0 flex-1 text-13 text-primary", item.is_done && "text-tertiary line-through")}
                  >
                    {item.title}
                  </span>
                  {canWrite && (
                    <span className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover/runbook:opacity-100">
                      <button
                        type="button"
                        onClick={() => {
                          setEditingId(item.id);
                          setEditingTitle(item.title);
                        }}
                        className="text-10 text-secondary hover:text-primary"
                      >
                        {t("war_room.runbook.edit")}
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleDelete(item.id)}
                        className="text-10 text-danger-primary hover:opacity-80"
                      >
                        {t("war_room.runbook.delete")}
                      </button>
                    </span>
                  )}
                </>
              )}
            </div>
          ))}
        </div>
      </div>
      {canWrite && (
        <div className="border-t border-subtle p-2">
          <input
            value={newTitle}
            onChange={(event) => setNewTitle(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void handleCreate();
            }}
            placeholder={t("war_room.runbook.add_placeholder")}
            className="w-full rounded-sm border border-subtle bg-surface-1 px-2 py-1.5 text-13 text-primary outline-none focus:border-strong"
          />
        </div>
      )}
    </div>
  );
});
```

- [ ] **Step 2: Typecheck and commit**

Run: `pnpm --filter=web check:types`
Expected: no errors.

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/context/runbook.tsx
git commit -m "feat(web): war room runbook panel"
```

---

### Task 14: Activity panel

**Files:**

- Create: `apps/web/core/components/war-rooms/room/context/activity.tsx`

- [ ] **Step 1: Create `context/activity.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { WAR_ROOM_ROLE_LABEL_KEYS, WAR_ROOM_SEVERITY_CONFIG, WAR_ROOM_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, IWarRoomEvent, TWarRoomParticipantRole, TWarRoomSeverity, TWarRoomStatus } from "@plane/types";
import { calculateTimeAgo, getFileURL } from "@plane/utils";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
};

export const WarRoomActivity = observer(function WarRoomActivity({ workspaceSlug, projectId, room }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getEvents, hasMoreEvents, fetchEvents } = useWarRoom();
  const { getUserDetails } = useMember();
  // derived values
  const events = getEvents(room.id);
  const hasMore = hasMoreEvents(room.id);

  useEffect(() => {
    void fetchEvents(workspaceSlug, projectId, room.id).catch(() => {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.activity.load_failed") });
    });
    // oxlint-disable-next-line exhaustive-deps -- fetch once per room
  }, [room.id, workspaceSlug, projectId]);

  const describeEvent = (event: IWarRoomEvent): string => {
    const payload = event.payload ?? {};
    const asString = (key: string): string => (typeof payload[key] === "string" ? (payload[key] as string) : "");
    const asArray = (key: string): string[] => (Array.isArray(payload[key]) ? (payload[key] as string[]) : []);
    const statusLabel = (value: string) =>
      value in WAR_ROOM_STATUS_CONFIG ? t(WAR_ROOM_STATUS_CONFIG[value as TWarRoomStatus].label_key) : value;
    const severityLabel = (value: string) =>
      value in WAR_ROOM_SEVERITY_CONFIG ? t(WAR_ROOM_SEVERITY_CONFIG[value as TWarRoomSeverity].label_key) : value;
    const roleLabel = (value: string) =>
      value in WAR_ROOM_ROLE_LABEL_KEYS ? t(WAR_ROOM_ROLE_LABEL_KEYS[value as TWarRoomParticipantRole]) : value;
    const memberName = (memberId: string) =>
      getUserDetails(memberId)?.display_name ?? t("war_room.activity.actor_unknown");
    const serviceNames = (serviceIds: string[]) =>
      serviceIds
        .map((serviceId) => room.services.find((service) => service.id === serviceId)?.name ?? serviceId)
        .join(", ");
    const issueNames = (issueIds: string[]) =>
      issueIds.map((issueId) => room.issues.find((issue) => issue.id === issueId)?.identifier ?? issueId).join(", ");

    switch (event.event_type) {
      case "room.created":
        return t("war_room.activity.events.room_created");
      case "room.status_changed":
        return t("war_room.activity.events.status_changed", {
          from: statusLabel(asString("from")),
          to: statusLabel(asString("to")),
        });
      case "room.severity_changed":
        return t("war_room.activity.events.severity_changed", {
          from: severityLabel(asString("from")),
          to: severityLabel(asString("to")),
        });
      case "room.resolved":
        return t("war_room.activity.events.resolved");
      case "room.reopened":
        return t("war_room.activity.events.reopened");
      case "room.archived":
        return t("war_room.activity.events.archived");
      case "participant.joined":
        return t("war_room.activity.events.participant_joined");
      case "participant.left":
        return t("war_room.activity.events.participant_left");
      case "participant.role_changed":
        return t("war_room.activity.events.participant_role_changed", {
          target: memberName(asString("member_id")),
          from: roleLabel(asString("from")),
          to: roleLabel(asString("to")),
        });
      case "service.linked":
        return t("war_room.activity.events.service_linked", { services: serviceNames(asArray("service_ids")) });
      case "service.unlinked":
        return t("war_room.activity.events.service_unlinked", { service: serviceNames([asString("service_id")]) });
      case "issue.linked":
        return t("war_room.activity.events.issue_linked", { issues: issueNames(asArray("issue_ids")) });
      case "issue.unlinked":
        return t("war_room.activity.events.issue_unlinked", { issue: issueNames([asString("issue_id")]) });
      case "runbook.item_done":
        return t("war_room.activity.events.runbook_item_done", { title: asString("title") });
      case "runbook.item_reopened":
        return t("war_room.activity.events.runbook_item_reopened", { title: asString("title") });
      default:
        return t("war_room.activity.events.unknown");
    }
  };

  const handleLoadOlder = async () => {
    const oldest = events[events.length - 1];
    if (!oldest) return;
    try {
      await fetchEvents(workspaceSlug, projectId, room.id, { before_id: oldest.id });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.activity.load_failed") });
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.activity")}
        </span>
        {hasMore && (
          <Button variant="tertiary" size="sm" onClick={() => void handleLoadOlder()}>
            {t("war_room.activity.load_older")}
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {events.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.activity.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.activity.empty_description")}</p>
          </div>
        )}
        <div className="space-y-3">
          {events.map((event) => {
            const actor = event.actor_id ? getUserDetails(event.actor_id) : null;
            const actorName = actor?.display_name ?? t("war_room.activity.actor_unknown");
            return (
              <div key={event.id} className="flex items-start gap-2">
                <Avatar
                  size="xs"
                  src={actor?.avatar_url ? getFileURL(actor.avatar_url) : undefined}
                  alt={actorName}
                  fallback={actorName[0]?.toUpperCase()}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-12 text-primary">
                    <span className="font-medium">{actorName}</span>{" "}
                    <span className="text-secondary">{describeEvent(event)}</span>
                  </p>
                  <p className="text-10 text-tertiary">{calculateTimeAgo(event.created_at)}</p>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Typecheck and commit**

Run: `pnpm --filter=web check:types`
Expected: no errors.

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/context/activity.tsx
git commit -m "feat(web): war room activity panel"
```

---

### Task 15: People panel

**Files:**

- Create: `apps/web/core/components/war-rooms/room/context/people.tsx`

- [ ] **Step 1: Create `context/people.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { WAR_ROOM_PARTICIPANT_ROLES, WAR_ROOM_ROLE_LABEL_KEYS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, IWarRoomParticipant, TWarRoomParticipantRole } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { getFileURL } from "@plane/utils";
// components
import { MemberDropdown } from "@/components/dropdowns/member/dropdown";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUser } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomPeople = observer(function WarRoomPeople({ workspaceSlug, projectId, room, canWrite }: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { addParticipant, updateParticipantRole, removeParticipant, getOnlineUserIds } = useWarRoom();
  const {
    getUserDetails,
    project: { getProjectMemberIds, fetchProjectMembers },
  } = useMember();
  const { data: currentUser } = useUser();
  // derived values
  const currentUserId = currentUser?.id;
  const onlineUserIds = getOnlineUserIds(room.id);
  const participantMemberIds = new Set(room.participants.map((participant) => participant.member_id));
  const selfParticipant = room.participants.find((participant) => participant.member_id === currentUserId);
  const availableMemberIds = (getProjectMemberIds(projectId, true) ?? []).filter(
    (memberId) => !participantMemberIds.has(memberId)
  );

  useEffect(() => {
    if (getProjectMemberIds(projectId, true) === null) void fetchProjectMembers(workspaceSlug, projectId);
    // oxlint-disable-next-line exhaustive-deps -- hydrate once per project
  }, [projectId, workspaceSlug]);

  const handleAdd = async (memberId: string) => {
    try {
      await addParticipant(workspaceSlug, projectId, room.id, { member_id: memberId });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.update_failed") });
    }
  };

  const handleRoleChange = async (participant: IWarRoomParticipant, role: TWarRoomParticipantRole) => {
    if (participant.role === role) return;
    try {
      await updateParticipantRole(workspaceSlug, projectId, room.id, participant.id, { role });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.update_failed") });
    }
  };

  const handleJoin = async () => {
    if (!currentUserId) return;
    try {
      await addParticipant(workspaceSlug, projectId, room.id, { member_id: currentUserId });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.join_failed") });
    }
  };

  const handleLeave = async () => {
    if (!selfParticipant) return;
    try {
      await removeParticipant(workspaceSlug, projectId, room.id, selfParticipant.id);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.people.leave_failed") });
    }
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-1.5">
        <span className="text-11 font-medium tracking-wide text-tertiary uppercase">
          {t("war_room.context_tabs.people")}
        </span>
        {canWrite && (
          <div className="flex items-center gap-2">
            {availableMemberIds.length > 0 && (
              <MemberDropdown
                buttonVariant="border-with-text"
                multiple={false}
                value={null}
                memberIds={availableMemberIds}
                projectId={projectId}
                placeholder={t("war_room.people.add")}
                onChange={(memberId) => {
                  if (memberId) void handleAdd(memberId);
                }}
              />
            )}
            {!selfParticipant ? (
              <Button variant="secondary" size="sm" onClick={() => void handleJoin()}>
                {t("war_room.people.join")}
              </Button>
            ) : (
              <Button variant="secondary" size="sm" onClick={() => void handleLeave()}>
                {t("war_room.people.leave")}
              </Button>
            )}
          </div>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {room.participants.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.people.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.people.empty_description")}</p>
          </div>
        )}
        <div className="space-y-1">
          {room.participants.map((participant) => {
            const member = getUserDetails(participant.member_id);
            const displayName =
              participant.display_name ?? member?.display_name ?? t("war_room.activity.actor_unknown");
            const isOnline = onlineUserIds.includes(participant.member_id);
            return (
              <div
                key={participant.id}
                className="flex items-center gap-2 rounded-md px-1 py-1.5 hover:bg-layer-transparent-hover"
              >
                <div className="relative shrink-0">
                  <Avatar
                    size="sm"
                    src={participant.avatar_url ? getFileURL(participant.avatar_url) : undefined}
                    alt={displayName}
                    fallback={displayName[0]?.toUpperCase()}
                  />
                  {isOnline && (
                    <span className="absolute -right-0.5 -bottom-0.5 h-2 w-2 rounded-full border border-surface-1 bg-success-primary" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-13 text-primary">
                    {displayName}
                    {participant.member_id === currentUserId && (
                      <span className="ml-1 text-10 text-tertiary">({t("war_room.header.you")})</span>
                    )}
                  </p>
                </div>
                {canWrite ? (
                  <CustomSelect
                    value={participant.role}
                    label={
                      <span className="text-11 text-secondary">{t(WAR_ROOM_ROLE_LABEL_KEYS[participant.role])}</span>
                    }
                    onChange={(role: TWarRoomParticipantRole) => void handleRoleChange(participant, role)}
                    noChevron
                  >
                    {WAR_ROOM_PARTICIPANT_ROLES.map((role) => (
                      <CustomSelect.Option key={role} value={role}>
                        {t(WAR_ROOM_ROLE_LABEL_KEYS[role])}
                      </CustomSelect.Option>
                    ))}
                  </CustomSelect>
                ) : (
                  <span className="text-11 text-secondary">{t(WAR_ROOM_ROLE_LABEL_KEYS[participant.role])}</span>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Typecheck and commit**

Run: `pnpm --filter=web check:types`
Expected: no errors.

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/context/people.tsx
git commit -m "feat(web): war room people panel"
```

---

### Task 16: Context panel shell with remembered tab

**Files:**

- Create: `apps/web/core/components/war-rooms/room/context/root.tsx`

- [ ] **Step 1: Create `context/root.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IWarRoom } from "@plane/types";
import { cn } from "@plane/utils";
// components
import { WarRoomActivity } from "./activity";
import { WarRoomNotes } from "./notes";
import { WarRoomPeople } from "./people";
import { WarRoomRunbook } from "./runbook";
import { WarRoomWorkItems } from "./work-items";
// hooks
import useLocalStorage from "@/hooks/use-local-storage";

type TContextTab = "notes" | "work_items" | "runbook" | "activity" | "people";

const CONTEXT_TABS: { key: TContextTab; label_key: string }[] = [
  { key: "notes", label_key: "war_room.context_tabs.notes" },
  { key: "work_items", label_key: "war_room.context_tabs.work_items" },
  { key: "runbook", label_key: "war_room.context_tabs.runbook" },
  { key: "activity", label_key: "war_room.context_tabs.activity" },
  { key: "people", label_key: "war_room.context_tabs.people" },
];

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomContextPanel = observer(function WarRoomContextPanel({
  workspaceSlug,
  projectId,
  room,
  canWrite,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // local storage
  const { storedValue, setValue } = useLocalStorage<TContextTab>(`war-room-context-tab-${room.id}`, "notes");
  // derived values
  const activeTab = CONTEXT_TABS.some((tab) => tab.key === storedValue) ? storedValue : "notes";

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex shrink-0 items-center gap-1 border-b border-subtle px-2 pt-1.5">
        {CONTEXT_TABS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => setValue(tab.key)}
            className={cn(
              "rounded-t-sm border-b-2 px-2 py-1 text-11 font-medium transition-colors",
              activeTab === tab.key
                ? "border-accent-primary text-primary"
                : "border-transparent text-tertiary hover:text-secondary"
            )}
          >
            {t(tab.label_key)}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1">
        {activeTab === "notes" && (
          <WarRoomNotes workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
        {activeTab === "work_items" && (
          <WarRoomWorkItems workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
        {activeTab === "runbook" && (
          <WarRoomRunbook workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
        {activeTab === "activity" && (
          <WarRoomActivity workspaceSlug={workspaceSlug} projectId={projectId} room={room} />
        )}
        {activeTab === "people" && (
          <WarRoomPeople workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Typecheck and commit**

Run: `pnpm --filter=web check:types`
Expected: no errors.

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/context/root.tsx
git commit -m "feat(web): war room context panel with remembered tab"
```

---

### Task 17: Room header and lifecycle modals

**Files:**

- Create: `apps/web/core/components/war-rooms/room/header/status-control.tsx`
- Create: `apps/web/core/components/war-rooms/room/header/confirm-modal.tsx`
- Create: `apps/web/core/components/war-rooms/room/header/resolve-modal.tsx`
- Create: `apps/web/core/components/war-rooms/room/header/edit-details-modal.tsx`
- Create: `apps/web/core/components/war-rooms/room/header/root.tsx`

- [ ] **Step 1: Create `header/status-control.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { WAR_ROOM_STATUS_CONFIG, WAR_ROOM_STATUS_TRANSITIONS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TWarRoomStatus } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";

type Props = {
  status: TWarRoomStatus;
  disabled?: boolean;
  onChange: (status: TWarRoomStatus) => void;
};

export const WarRoomStatusControl = observer(function WarRoomStatusControl({
  status,
  disabled = false,
  onChange,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const config = WAR_ROOM_STATUS_CONFIG[status];
  const transitions = WAR_ROOM_STATUS_TRANSITIONS[status];
  const pill = (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill)}>{t(config.label_key)}</span>
  );

  if (transitions.length === 0 || disabled) return pill;

  return (
    <CustomSelect value={status} label={pill} onChange={(value) => onChange(value as TWarRoomStatus)} noChevron>
      {transitions.map((nextStatus) => (
        <CustomSelect.Option key={nextStatus} value={nextStatus}>
          {t(WAR_ROOM_STATUS_CONFIG[nextStatus].label_key)}
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
});
```

- [ ] **Step 2: Create `header/confirm-modal.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { Button } from "@plane/propel/button";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  description: string;
  cancelLabel: string;
  confirmLabel: string;
  isDestructive?: boolean;
  isSubmitting?: boolean;
  onConfirm: () => void;
};

export const WarRoomConfirmModal = observer(function WarRoomConfirmModal({
  isOpen,
  onClose,
  title,
  description,
  cancelLabel,
  confirmLabel,
  isDestructive = false,
  isSubmitting = false,
  onConfirm,
}: Props) {
  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.CENTER} width={EModalWidth.XL}>
      <div className="space-y-3 p-5">
        <h3 className="text-16 font-medium text-primary">{title}</h3>
        <p className="text-13 text-secondary">{description}</p>
        <div className="flex items-center justify-end gap-2 pt-1">
          <Button variant="secondary" size="lg" onClick={onClose} disabled={isSubmitting}>
            {cancelLabel}
          </Button>
          <Button
            variant={isDestructive ? "error-fill" : "primary"}
            size="lg"
            onClick={onConfirm}
            loading={isSubmitting}
            disabled={isSubmitting}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

Use `t("common.cancel")` for `cancelLabel` at every call site.

- [ ] **Step 3: Create `header/resolve-modal.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  isSubmitting?: boolean;
  onConfirm: (note: string) => void;
};

export const WarRoomResolveModal = observer(function WarRoomResolveModal({
  isOpen,
  onClose,
  isSubmitting = false,
  onConfirm,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // states
  const [note, setNote] = useState("");

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.CENTER} width={EModalWidth.XL}>
      <div className="space-y-3 p-5">
        <h3 className="text-16 font-medium text-primary">{t("war_room.resolve_modal.title")}</h3>
        <p className="text-13 text-secondary">{t("war_room.resolve_modal.description")}</p>
        <div className="space-y-1">
          <label className="text-12 text-secondary">{t("war_room.resolve_modal.note_label")}</label>
          <textarea
            value={note}
            onChange={(event) => setNote(event.target.value)}
            rows={3}
            placeholder={t("war_room.resolve_modal.note_placeholder")}
            className="w-full resize-none rounded-sm border border-subtle bg-surface-1 px-2 py-1.5 text-13 text-primary outline-none focus:border-strong"
          />
        </div>
        <div className="flex items-center justify-end gap-2 pt-1">
          <Button variant="secondary" size="lg" onClick={onClose} disabled={isSubmitting}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            onClick={() => onConfirm(note)}
            loading={isSubmitting}
            disabled={isSubmitting}
          >
            {t("war_room.resolve_modal.confirm")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

- [ ] **Step 4: Create `header/edit-details-modal.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IWarRoom } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS } from "@/services/service.helpers";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";
// services
import { WorkspaceService } from "@/services/workspace.service";

const workspaceService = new WorkspaceService();

type Props = {
  isOpen: boolean;
  onClose: () => void;
  room: IWarRoom;
  workspaceSlug: string;
  projectId: string;
  isSubmitting?: boolean;
  onSave: (data: { name: string; description_html: string }) => void;
};

export const WarRoomEditDetailsModal = observer(function WarRoomEditDetailsModal({
  isOpen,
  onClose,
  room,
  workspaceSlug,
  projectId,
  isSubmitting = false,
  onSave,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWorkspaceBySlug } = useWorkspace();
  // states
  const [name, setName] = useState(room.name);
  const [descriptionHtml, setDescriptionHtml] = useState(room.description_html);

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <div className="space-y-4 p-5">
        <h3 className="text-18 font-medium text-secondary">{t("war_room.edit_details_modal.title")}</h3>
        <Field name="name">
          <InputGroup size="2xl">
            <Input
              size="2xl"
              id="war-room-edit-name"
              name="name"
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t("war_room.header.rename_placeholder")}
              maxLength={255}
            />
          </InputGroup>
        </Field>
        <div>
          <RichTextEditor
            editable
            key={room.id}
            id="war-room-edit-description-editor"
            initialValue={room.description_html}
            value={descriptionHtml}
            workspaceSlug={workspaceSlug}
            workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
            projectId={projectId}
            disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
            onChange={(_json: object, html: string) => setDescriptionHtml(html)}
            placeholder={t("war_room.edit_details_modal.description")}
            containerClassName="min-h-24 rounded-md border border-subtle"
            searchMentionCallback={async (payload) =>
              await workspaceService.searchEntity(workspaceSlug, {
                ...payload,
                project_id: projectId,
              })
            }
            uploadFile={async () => {
              throw new Error("File upload is disabled for war room descriptions.");
            }}
            duplicateFile={async () => {
              throw new Error("File upload is disabled for war room descriptions.");
            }}
          />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose} disabled={isSubmitting}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            onClick={() => onSave({ name: name.trim(), description_html: descriptionHtml })}
            loading={isSubmitting}
            disabled={isSubmitting || name.trim() === ""}
          >
            {t("war_room.edit_details_modal.save")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

- [ ] **Step 5: Create `header/root.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { AvatarGroup } from "@makeplane/propel/components/avatar-group";
import {
  EUserPermissions,
  EUserPermissionsLevel,
  WAR_ROOM_SEVERITIES,
  WAR_ROOM_SEVERITY_CONFIG,
  getWarRoomLink,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, TWarRoomSeverity, TWarRoomStatus } from "@plane/types";
import { CustomMenu, CustomSelect } from "@plane/ui";
import { getFileURL } from "@plane/utils";
// components
import { WarRoomConfirmModal } from "./confirm-modal";
import { WarRoomEditDetailsModal } from "./edit-details-modal";
import { WarRoomResolveModal } from "./resolve-modal";
import { WarRoomStatusControl } from "./status-control";
// helpers
import { appendNoteToHtml, formatElapsed } from "@/services/war-room.helpers";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

type TDialog = "resolve" | "archive" | "delete" | "edit" | null;

export const WarRoomHeader = observer(function WarRoomHeader({ workspaceSlug, projectId, room, canWrite }: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateWarRoom, deleteWarRoom, addParticipant, removeParticipant, getOnlineUserIds } = useWarRoom();
  const { data: currentUser } = useUser();
  const { allowPermissions } = useUserPermissions();
  // states
  const [now, setNow] = useState(() => Date.now());
  const [isEditingName, setIsEditingName] = useState(false);
  const [nameValue, setNameValue] = useState(room.name);
  const [dialog, setDialog] = useState<TDialog>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  // derived values
  const currentUserId = currentUser?.id;
  const isRunning = room.status === "active" || room.status === "monitoring";
  const elapsed = formatElapsed(room.started_at, room.resolved_at, now);
  const onlineUserIds = getOnlineUserIds(room.id);
  const onlineParticipants = room.participants.filter((participant) => onlineUserIds.includes(participant.member_id));
  const visibleOnline = onlineParticipants.slice(0, 4);
  const overflowOnline = onlineParticipants.length - visibleOnline.length;
  const selfParticipant = room.participants.find((participant) => participant.member_id === currentUserId);
  const canManage =
    canWrite && allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT);

  useEffect(() => {
    if (!isRunning) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [isRunning]);

  useEffect(() => {
    setNameValue(room.name);
  }, [room.name]);

  const showError = (message: string) => setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message });

  const handleNameSave = async () => {
    const name = nameValue.trim();
    setIsEditingName(false);
    if (name === "" || name === room.name) {
      setNameValue(room.name);
      return;
    }
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { name });
    } catch {
      setNameValue(room.name);
      showError(t("war_room.errors.generic"));
    }
  };

  const handleStatusChange = async (status: TWarRoomStatus) => {
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { status });
    } catch {
      showError(t("war_room.errors.generic"));
    }
  };

  const handleSeverityChange = async (severity: TWarRoomSeverity) => {
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { severity });
    } catch {
      showError(t("war_room.errors.generic"));
    }
  };

  const handleResolve = async (note: string) => {
    setIsSubmitting(true);
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, {
        status: "resolved",
        notes_html: appendNoteToHtml(room.notes_html, note),
      });
      setDialog(null);
    } catch {
      showError(t("war_room.errors.generic"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleArchive = async () => {
    setIsSubmitting(true);
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, { status: "archived" });
      setDialog(null);
    } catch {
      showError(t("war_room.errors.generic"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDelete = async () => {
    setIsSubmitting(true);
    try {
      await deleteWarRoom(workspaceSlug, projectId, room.id);
      setDialog(null);
      router.push(getWarRoomLink(workspaceSlug, projectId));
    } catch {
      showError(t("war_room.errors.generic"));
      setIsSubmitting(false);
    }
  };

  const handleEditDetails = async (data: { name: string; description_html: string }) => {
    setIsSubmitting(true);
    try {
      await updateWarRoom(workspaceSlug, projectId, room.id, data);
      setDialog(null);
    } catch {
      showError(t("war_room.errors.generic"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleJoin = async () => {
    if (!currentUserId) return;
    try {
      await addParticipant(workspaceSlug, projectId, room.id, { member_id: currentUserId });
    } catch {
      showError(t("war_room.people.join_failed"));
    }
  };

  const handleLeave = async () => {
    if (!selfParticipant) return;
    try {
      await removeParticipant(workspaceSlug, projectId, room.id, selfParticipant.id);
    } catch {
      showError(t("war_room.people.leave_failed"));
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-subtle bg-surface-1 px-3 py-2">
      <span className="shrink-0 text-12 font-medium text-tertiary">WR-{room.sequence_id}</span>
      {isEditingName ? (
        <input
          value={nameValue}
          onChange={(event) => setNameValue(event.target.value)}
          onBlur={() => void handleNameSave()}
          onKeyDown={(event) => {
            if (event.key === "Enter") void handleNameSave();
            if (event.key === "Escape") {
              setNameValue(room.name);
              setIsEditingName(false);
            }
          }}
          // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus -- inline name edit should take focus
          autoFocus
          maxLength={255}
          className="min-w-40 max-w-72 flex-1 rounded-sm border border-subtle bg-surface-1 px-1.5 py-0.5 text-14 font-medium text-primary outline-none focus:border-strong"
        />
      ) : (
        <button
          type="button"
          disabled={!canManage}
          onClick={() => setIsEditingName(true)}
          className="min-w-0 max-w-72 truncate text-left text-14 font-medium text-primary hover:underline disabled:hover:no-underline"
          title={room.name}
        >
          {room.name}
        </button>
      )}
      {canManage ? (
        <CustomSelect
          value={room.severity}
          label={
            <span
              className={`rounded-full px-2 py-0.5 text-11 font-medium ${WAR_ROOM_SEVERITY_CONFIG[room.severity].pill}`}
            >
              {t(WAR_ROOM_SEVERITY_CONFIG[room.severity].label_key)}
            </span>
          }
          onChange={(severity: TWarRoomSeverity) => void handleSeverityChange(severity)}
          noChevron
        >
          {WAR_ROOM_SEVERITIES.map((severity) => (
            <CustomSelect.Option key={severity} value={severity}>
              {t(WAR_ROOM_SEVERITY_CONFIG[severity].label_key)}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      ) : (
        <span
          className={`rounded-full px-2 py-0.5 text-11 font-medium ${WAR_ROOM_SEVERITY_CONFIG[room.severity].pill}`}
        >
          {t(WAR_ROOM_SEVERITY_CONFIG[room.severity].label_key)}
        </span>
      )}
      <WarRoomStatusControl
        status={room.status}
        disabled={!canManage}
        onChange={(status) => void handleStatusChange(status)}
      />
      <span className="text-12 tabular-nums text-tertiary">{elapsed}</span>
      <div className="flex items-center gap-1">
        {onlineParticipants.length > 0 && (
          <AvatarGroup size="xs">
            {visibleOnline.map((participant) => (
              <Avatar
                key={participant.id}
                src={participant.avatar_url ? getFileURL(participant.avatar_url) : undefined}
                alt={participant.display_name ?? ""}
                fallback={participant.display_name?.[0]?.toUpperCase()}
              />
            ))}
            {overflowOnline > 0 && <Avatar alt={`+${overflowOnline}`} fallback={`+${overflowOnline}`} />}
          </AvatarGroup>
        )}
        <span className="text-10 text-tertiary">
          {t("war_room.header.online", { count: onlineParticipants.length })}
        </span>
      </div>
      <div className="ml-auto flex items-center gap-2">
        {canManage && !selfParticipant && (
          <Button variant="secondary" size="sm" onClick={() => void handleJoin()}>
            {t("war_room.header.join")}
          </Button>
        )}
        {canManage && selfParticipant && (
          <Button variant="tertiary" size="sm" onClick={() => void handleLeave()}>
            {t("war_room.header.leave")}
          </Button>
        )}
        {canManage && isRunning && (
          <Button variant="primary" size="sm" onClick={() => setDialog("resolve")}>
            {t("war_room.header.resolve")}
          </Button>
        )}
        {canManage && (
          <CustomMenu ellipsis placement="bottom-end">
            <CustomMenu.MenuItem onClick={() => setDialog("edit")}>
              {t("war_room.header.edit_details")}
            </CustomMenu.MenuItem>
            <CustomMenu.MenuItem onClick={() => setDialog("archive")}>
              {t("war_room.header.archive")}
            </CustomMenu.MenuItem>
            <CustomMenu.MenuItem onClick={() => setDialog("delete")}>{t("war_room.header.delete")}</CustomMenu.MenuItem>
          </CustomMenu>
        )}
      </div>

      <WarRoomResolveModal
        isOpen={dialog === "resolve"}
        onClose={() => setDialog(null)}
        isSubmitting={isSubmitting}
        onConfirm={(note) => void handleResolve(note)}
      />
      <WarRoomConfirmModal
        isOpen={dialog === "archive"}
        onClose={() => setDialog(null)}
        title={t("war_room.archive_modal.title")}
        description={t("war_room.archive_modal.description")}
        cancelLabel={t("common.cancel")}
        confirmLabel={t("war_room.archive_modal.confirm")}
        isSubmitting={isSubmitting}
        onConfirm={() => void handleArchive()}
      />
      <WarRoomConfirmModal
        isOpen={dialog === "delete"}
        onClose={() => setDialog(null)}
        title={t("war_room.delete_modal.title")}
        description={t("war_room.delete_modal.description")}
        cancelLabel={t("common.cancel")}
        confirmLabel={t("war_room.delete_modal.confirm")}
        isDestructive
        isSubmitting={isSubmitting}
        onConfirm={() => void handleDelete()}
      />
      {dialog === "edit" && (
        <WarRoomEditDetailsModal
          isOpen
          onClose={() => setDialog(null)}
          room={room}
          workspaceSlug={workspaceSlug}
          projectId={projectId}
          isSubmitting={isSubmitting}
          onSave={(data) => void handleEditDetails(data)}
        />
      )}
    </div>
  );
});
```

Note: `WAR_ROOM_STATUS_CONFIG` is intentionally not imported in the header — `WarRoomStatusControl` owns status labels.

- [ ] **Step 6: Typecheck and lint**

Run: `pnpm --filter=web check:types && pnpm check:lint`
Expected: no errors. `CustomMenu` needs the `ellipsis` prop (it does; matches `services/detail/quick-actions.tsx:44-52`).

- [ ] **Step 7: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms/room/header
git commit -m "feat(web): war room header, lifecycle and management modals"
```

---

### Task 18: Assemble the room page and remove the old overview

**Files:**

- Modify: `apps/web/core/components/war-rooms/room/root.tsx` (rewrite)
- Delete: `apps/web/core/components/war-rooms/room/room-overview.tsx`
- Modify: `apps/web/core/components/war-rooms/index.ts`

- [ ] **Step 1: Rewrite `room/root.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TWarRoomSocketEvent } from "@plane/types";
// components
import { WarRoomChat } from "./chat/root";
import { WarRoomContextPanel } from "./context/root";
import { WarRoomHeader } from "./header/root";
import { WarRoomServiceMap } from "./service-map";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUserPermissions } from "@/hooks/store/user";
import { useWarRoomSocket } from "@/hooks/use-war-room-socket";

type Props = {
  workspaceSlug: string;
  projectId: string;
  warRoomId: string;
};

export const WarRoomRoot = observer(function WarRoomRoot({ workspaceSlug, projectId, warRoomId }: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomDetailById, detailErrorMap, fetchWarRoomDetail, applySocketEvent } = useWarRoom();
  const { allowPermissions } = useUserPermissions();
  // derived values
  const room = getWarRoomDetailById(warRoomId);
  const hasError = detailErrorMap[warRoomId];

  useEffect(() => {
    if (room || hasError) return;
    void fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  }, [room, hasError, workspaceSlug, projectId, warRoomId, fetchWarRoomDetail]);

  const handleSocketEvent = useCallback(
    (event: TWarRoomSocketEvent) => {
      applySocketEvent(workspaceSlug, projectId, warRoomId, event);
    },
    [applySocketEvent, projectId, warRoomId, workspaceSlug]
  );

  const { status, sendTyping } = useWarRoomSocket({
    workspaceSlug,
    projectId,
    warRoomId,
    enabled: Boolean(room),
    onEvent: handleSocketEvent,
  });

  const canWrite =
    allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT) &&
    room?.status !== "archived";

  if (hasError) {
    return (
      <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
        <p className="text-sm font-medium text-primary">{t("war_room.detail.not_found_title")}</p>
        <p className="text-xs text-secondary">{t("war_room.detail.not_found_description")}</p>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => router.push(`/${workspaceSlug}/projects/${projectId}/war-rooms`)}
        >
          {t("war_room.detail.view_other_rooms")}
        </Button>
      </div>
    );
  }

  if (!room) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col overflow-hidden">
      <WarRoomHeader workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
      {canWrite === false && room.status === "archived" && (
        <div className="border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 text-secondary">
          {t("war_room.archived_notice")}
        </div>
      )}
      <div className="h-[42%] min-h-[200px] shrink-0 border-b border-subtle">
        <WarRoomServiceMap workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
        <div className="min-h-[240px] flex-1 border-b border-subtle lg:min-h-0 lg:border-r lg:border-b-0">
          <WarRoomChat
            workspaceSlug={workspaceSlug}
            projectId={projectId}
            room={room}
            canWrite={canWrite}
            connectionStatus={status}
            sendTyping={sendTyping}
          />
        </div>
        <div className="min-h-[240px] flex-1 lg:min-h-0 lg:w-[420px] lg:flex-none">
          <WarRoomContextPanel workspaceSlug={workspaceSlug} projectId={projectId} room={room} canWrite={canWrite} />
        </div>
      </div>
    </div>
  );
});
```

Note: `canWrite` is `boolean | undefined` when `room` is undefined; inside the guard `room` is defined, so TS narrows to boolean — if it complains, wrap with `Boolean(...)`.

- [ ] **Step 2: Delete `room-overview.tsx` and drop its barrel export**

```bash
rm apps/web/core/components/war-rooms/room/room-overview.tsx
```

In `apps/web/core/components/war-rooms/index.ts`, remove the line:

```ts
export * from "./room/room-overview";
```

Keep the remaining exports unchanged.

- [ ] **Step 3: Run the full web checks**

Run:

```bash
pnpm --filter=web test
pnpm --filter=web check:types
pnpm check:lint
pnpm check:format
```

Expected: all pass. If a stale import of `WarRoomOverview` exists anywhere, `rg -n "WarRoomOverview|room-overview" apps/web` must return nothing.

- [ ] **Step 4: Commit**

```bash
pnpm fix:format
git add apps/web/core/components/war-rooms
git commit -m "feat(web): assemble war room page with map, chat and context panels"
```

---

### Task 19: Verification, prod build and manual smoke

**Files:** none (verification only)

- [ ] **Step 1: Run the full automated suite**

```bash
pnpm --filter=web test
pnpm --filter=web check:types
pnpm check:lint
pnpm check:format
pnpm --filter @plane/i18n run sync:check
```

Expected: web tests pass (existing 14 files + the new helper/store suites); types/lint/format clean; `sync:check` shows no `war-room` drift.

- [ ] **Step 2: Build and restart the live + web services**

The backend was frozen in Phase 1–2; no api-rs rebuild is needed. Verify health first, then build web (the i18n `dist` was already rebuilt in Task 1):

```bash
curl -s -o /dev/null -w "api:%{http_code}\n" http://localhost:8000/health
curl -s -o /dev/null -w "live:%{http_code}\n" --max-time 15 http://localhost:3100/live/health/
VITE_API_BASE_URL=https://api.terraline.space pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 8
curl -s -o /dev/null -w "prod:%{http_code}\n" --max-time 15 http://localhost:3000/
```

Expected: `api:200`, `live:200`, `prod:200`.

- [ ] **Step 3: Manual smoke checklist (two browser sessions, same project)**

1. Open a war room from the list → header shows `WR-n`, name, severity pill, status pill, ticking timer.
2. Rename inline (click name → type → Enter); severity and status controls change and persist on reload.
3. Map: affected services are highlighted, 1-hop neighbors dimmed; clicking a node opens the service detail in a new tab; "Add services" adds/removes links and the canvas updates.
4. Chat: send a message in session A; it appears in session B without reload; optimistic echo does not duplicate.
5. Mentions: type `@`, pick a member, send; the message renders a mention chip and the stored `mentions` array includes the member (the notification card itself is Phase 5).
6. Typing indicator appears for <3s; closing a session removes its presence dot; kill the live service and confirm the reconnect banner appears and REST sends still succeed.
7. Notes: type, wait ~1s, reload → content persisted; "Edited …" label updates.
8. Work items: link an extra work item, unlink it; the primary incident is pinned with a badge and cannot be unlinked.
9. Runbook: toggle items (progress bar updates), add, rename, delete.
10. Activity: status/severity changes and runbook toggles appear live; "Load older" pages back.
11. People: join/leave, role dropdown changes roles (commander demotion), add a participant, online dots follow presence.
12. Lifecycle: Resolve with a note → status resolved, timer stops, note appended in Notes; Reopen; Archive → write controls disappear and the archived notice shows; Delete → redirected to the list and the room disappears.
13. Guest session (project Guest role): every write affordance is hidden; reads work.

- [ ] **Step 4: Final commit (if any fixes came out of smoke)**

```bash
git status --short
git add -A
git commit -m "fix(web): war room room page smoke fixes"
```

Only commit if there are changes; otherwise skip.

---

## Self-review notes (for the planner)

- Spec coverage: header/lifecycle (Task 17), graph refactor + blast radius (Tasks 8–9), chat + socket (Tasks 7, 11), Notes/Work items/Runbook/Activity/People (Tasks 10, 12–15), tab persistence (Task 16), i18n (Task 1), testing (Tasks 4, 6, 19). Mention _notification card_ is Phase 5 by design — Phase 4 only produces the mention tokens and relies on the existing notification row.
- Known deliberate limitation: notes/chat are last-write-wins and the notes editor ignores remote `notes_html` updates while mounted; documented in the spec (keputusan #13) and acceptable for MVP.
- If `@plane/ui` `CustomMenu` renders without the ellipsis icon prop name used in the plan, mirror `services/detail/quick-actions.tsx:44-52` exactly.
