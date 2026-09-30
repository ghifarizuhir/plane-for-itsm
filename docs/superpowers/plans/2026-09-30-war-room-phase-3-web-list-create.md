# War Room Phase 3 (Web List + Create) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the war room frontend entry surface: shared types/constants/i18n, API service + MobX store, the project list page (summary chips, status tabs, search, rows), the create modal (incident picker single-select, severity/services/description defaults, 409 duplicate banner), the sidebar/tab-navigation entry, and the "Open war room" entry point on Incident work items. A minimal read-only room overview route is included so list rows and create redirects land on a real page; Phase 4 replaces that overview with the full room (map/chat/panels).

**Architecture:** Frontend-only phase, mirroring the Services feature layering: `packages/types/src/war-room` → `packages/constants/src/war-room.ts` → `apps/web/core/services/war-room.service.ts` → `apps/web/core/store/war-room.store.ts` + `war-room_filter.store.ts` → components under `apps/web/core/components/war-rooms/**` → routes under `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/**`. All REST calls go to the Phase 1–2 api-rs endpoints; list filtering is server-side (`status` CSV + `q`), with the tab/search state held in the filter store.

**Tech Stack:** React Router v7 (route config in `app/routes/core.ts`), MobX (+ `mobx-utils.computedFn`), react-hook-form, `@plane/types`, `@plane/constants`, `@plane/i18n`, `@plane/ui`, `@makeplane/propel`, vitest (node env, `core/**/*.test.ts`).

**Preconditions:** Phase 1 + Phase 2 merged on this branch (api-rs war room routes + `apps/live` relay live). Backend dev stack running (`curl http://localhost:8000/health` → 200). `pnpm install` up to date. No existing `war-room*` artifacts in `apps/web`, `packages/types`, `packages/constants`, or `packages/i18n`.

**Scope deltas from the design spec (approved by plan):**

- Room page in this phase is a read-only overview (`room-overview.tsx`) with header data, primary incident link, and counts. The blast-radius map, chat panel, socket hook, and context tabs ship in Phase 4 and will replace `room-overview.tsx`.
- Severity autofill from priority happens client-side only when the issue is already in the issue store (entry point path). When an incident is picked from the search modal the backend default applies (`severity` omitted) because `ISearchIssueResponse` has no `priority` field. The severity select shows "From incident priority" when unset.
- Web vitest runs in node environment and has no jsdom/testing-library; component behavior is verified by `check:types`, lint, and the manual smoke checklist in Task 15. Pure helpers and the store get unit tests.
- `ExistingIssuesListModal` gains `selectionMode?: "multiple" | "single"` (default `"multiple"`, backward compatible). No other consumer changes.

---

## File Structure

| File                                                                                                                  | Aksi   | Tanggung jawab                                                       |
| --------------------------------------------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------- |
| `packages/i18n/src/constants/namespaces.ts`                                                                           | modify | daftarkan namespace `war-room`                                       |
| `packages/i18n/src/locales/en/war-room.json`                                                                          | create | sumber string EN fitur war room                                      |
| `packages/i18n/src/locales/*/war-room.json`                                                                           | create | terjemahan 19 locale (skill translate)                               |
| `packages/i18n/src/locales/*/navigation.json`                                                                         | modify | key `sidebar.war_rooms` di 20 locale                                 |
| `packages/types/src/war-room/core.ts`                                                                                 | create | tipe domain war room + payload                                       |
| `packages/types/src/war-room/filters.ts`                                                                              | create | tipe tab/list params                                                 |
| `packages/types/src/war-room/index.ts`                                                                                | create | barrel folder                                                        |
| `packages/types/src/index.ts`                                                                                         | modify | export barrel war-room                                               |
| `packages/constants/src/war-room.ts`                                                                                  | create | link helper + config severity/status/role/tab                        |
| `packages/constants/src/index.ts`                                                                                     | modify | export barrel war-room                                               |
| `apps/web/core/services/war-room.helpers.ts`                                                                          | create | helper murni (severity mapping, tab filter, elapsed, link insiden)   |
| `apps/web/core/services/war-room.helpers.test.ts`                                                                     | create | unit test helper                                                     |
| `apps/web/core/services/war-room.service.ts`                                                                          | create | REST client + normalisasi error (termasuk `war_room_id`)             |
| `apps/web/core/store/war-room.store.ts`                                                                               | create | data room (list/detail/summary)                                      |
| `apps/web/core/store/war-room.store.test.ts`                                                                          | create | unit test store                                                      |
| `apps/web/core/store/war-room_filter.store.ts`                                                                        | create | state UI tab status + search                                         |
| `apps/web/core/store/root.store.ts`                                                                                   | modify | registrasi dua store                                                 |
| `apps/web/core/hooks/store/use-war-room.ts`                                                                           | create | hook store data                                                      |
| `apps/web/core/hooks/store/use-war-room-filter.ts`                                                                    | create | hook store filter                                                    |
| `apps/web/core/components/core/modals/existing-issues-list-modal.tsx`                                                 | modify | mode `selectionMode="single"`                                        |
| `apps/web/core/components/war-rooms/index.ts`                                                                         | create | barrel komponen                                                      |
| `apps/web/core/components/war-rooms/create/create-war-room-modal.tsx`                                                 | create | modal create (picker insiden, form, banner 409)                      |
| `apps/web/core/components/war-rooms/list/war-room-view-header.tsx`                                                    | create | breadcrumbs + search + tombol create                                 |
| `apps/web/core/components/war-rooms/list/war-room-search-input.tsx`                                                   | create | input search debounce ke filter store                                |
| `apps/web/core/components/war-rooms/list/war-room-summary-chips.tsx`                                                  | create | chips `{active, sev1_2, resolved_7d}`                                |
| `apps/web/core/components/war-rooms/list/war-room-load-error-state.tsx`                                               | create | state error + retry                                                  |
| `apps/web/core/components/war-rooms/list/war-rooms-list-view.tsx`                                                     | create | tabs + fetch + empty/loading/error + prefill `?primary_issue_id=`    |
| `apps/web/core/components/war-rooms/list/war-rooms-board.tsx`                                                         | create | daftar baris                                                         |
| `apps/web/core/components/war-rooms/list/war-rooms-board-row.tsx`                                                     | create | baris ops-board (severity rail, WR-n, chips, durasi ticking, avatar) |
| `apps/web/core/components/war-rooms/room/detail-header.tsx`                                                           | create | breadcrumbs room                                                     |
| `apps/web/core/components/war-rooms/room/root.tsx`                                                                    | create | fetch detail + state loading/error/not-found                         |
| `apps/web/core/components/war-rooms/room/room-overview.tsx`                                                           | create | overview read-only (diganti Fase 4)                                  |
| `apps/web/core/components/war-rooms/issue-war-room-button.tsx`                                                        | create | tombol "Open war room" di quick actions work item tipe Incident      |
| `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`                                         | modify | render `IssueWarRoomButton`                                          |
| `apps/web/core/components/workspace/sidebar/project-navigation.tsx`                                                   | modify | item nav War rooms                                                   |
| `apps/web/core/components/navigation/use-navigation-items.ts`                                                         | modify | item nav War rooms (mode tab)                                        |
| `apps/web/core/components/navigation/tab-navigation-utils.ts`                                                         | modify | `getTabUrl` war-rooms                                                |
| `apps/web/app/routes/core.ts`                                                                                         | modify | route list + detail war rooms                                        |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/layout.tsx`             | create | AppHeader + ContentWrapper list                                      |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/page.tsx`               | create | halaman list + baca `?primary_issue_id=`                             |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)/layout.tsx`           | create | AppHeader + ContentWrapper detail                                    |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)/[warRoomId]/page.tsx` | create | halaman room shell                                                   |

---

## Catatan kontrak (dari Fase 1–2, jangan diubah)

- Routes: `GET|POST /api/workspaces/:slug/projects/:project_id/war-rooms/`, `GET .../war-rooms/summary/`, `GET|PATCH|DELETE .../war-rooms/:pk/`.
- List item keys: `id, workspace_id, project_id, sequence_id, name, description_html, notes_html, severity, status, primary_issue_id, started_at, resolved_at, created_at, updated_at, created_by, primary_issue, services, participants, service_count, participant_count, message_count, last_activity_at`.
- `primary_issue` = `{id, identifier, name, priority, state_group}` (identifier berbentuk `PROJ-123`), atau `null`.
- `services[]` = `{id, name, status}`; `participants[]` = `{id, member_id, role, joined_at, display_name, avatar_url}` (`avatar_url` relatif `/api/assets/v2/static/<asset-id>/` atau URL lama/null).
- Detail keys: semua `room_base_json` + `primary_issue, services, issues, participants, runbook_items, counts:{messages}`.
- Summary = `{active, sev1_2, resolved_7d}` (angka).
- List query params: `status` (csv, nilai `active|monitoring|resolved|archived`), `severity` (csv), `q`.
- Create body: `{name?, primary_issue_id, severity?, description_html?, service_ids?}`; sukses 201 detail; insiden invalid 400; duplikat aktif 409 `{"error":"active_war_room_exists","war_room_id":"..."}`; `service_ids` harus se-project.
- Status transitions (relevan Fase 4): active↔monitoring, active|monitoring→resolved, resolved→active (reopen), semua→archived (terminal).
- Enum: status `active|monitoring|resolved|archived`, severity `sev1..sev4`, role `commander|comms|scribe|responder`.
- Fixture/verifikasi backend: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1` (dari `apps/api-rs`).

### Pola file yang disalin

- Route/page/list: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/**`.
- Store: `apps/web/core/store/service.store.ts`; filter store: `apps/web/core/store/service_filter.store.ts`.
- Header list: `apps/web/core/components/services/service-view-header.tsx`; search: `.../services/search-input.tsx`.
- Baris board: `.../services/board/services-board-row.tsx`; chips summary: `.../services/health/service-health-summary.tsx`.
- Form/modal: `.../services/modal.tsx`, `.../services/service-form.tsx`; multi-select: `.../services/select/service-multi-select.tsx`.
- Picker: `apps/web/core/components/core/modals/existing-issues-list-modal.tsx`.
- Test store: `apps/web/core/store/workflow.store.test.ts`; test helper: `apps/web/core/services/service.helpers.test.ts`.

---

### Task 1: i18n — namespace `war-room`, key EN, terjemahan semua locale

**Files:**

- Modify: `packages/i18n/src/constants/namespaces.ts`
- Create: `packages/i18n/src/locales/en/war-room.json`
- Create: `packages/i18n/src/locales/<19 locale lain>/war-room.json`
- Modify: `packages/i18n/src/locales/<semua 20 locale>/navigation.json`

- [ ] **Step 1: Muat skill translate**

Baca `/home/ghifari/plane-for-itsm/.claude/skills/translate/SKILL.md` dan ikuti aturannya untuk seluruh langkah task ini (DNT glossary, register per locale, plural CLDR, workflow review terjemahan). Catatan relevan: `SEV` adalah akronim → tidak diterjemahkan; "war room" dan nama role adalah kata umum → diterjemahkan; jangan menyalin EN ke locale non-Latin.

- [ ] **Step 2: Daftarkan namespace**

Di `packages/i18n/src/constants/namespaces.ts`, sisipkan `"war-room"` di antara `"update"` (baris 30) dan `"wiki"`:

```ts
  "update",
  "war-room",
  "wiki",
```

- [ ] **Step 3: Tulis sumber EN**

Buat `packages/i18n/src/locales/en/war-room.json` dengan isi persis berikut:

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
    }
  }
}
```

- [ ] **Step 4: Tambah key sidebar di navigation.json**

Di `packages/i18n/src/locales/en/navigation.json`, tambahkan setelah baris `"services": "Services",` (baris 24):

```json
    "war_rooms": "War rooms",
```

Terjemahkan key `sidebar.war_rooms` yang sama ke 19 locale lain di `packages/i18n/src/locales/` (cs, de, es, fr, id, it, ja, ka-ge, ko, pl, pt-BR, ro, ru, sk, tr-TR, ua, vi-VN, zh-CN, zh-TW) mengikuti skill translate.

- [ ] **Step 5: Terjemahkan war-room.json ke 19 locale**

Untuk tiap locale target, buat `packages/i18n/src/locales/<locale>/war-room.json` dengan struktur key identik dengan EN dan terjemahan yang direview sesuai skill translate. Jangan menyalin nilai EN ke locale non-Latin (kecuali `SEV1..SEV4`, `WR-n`, dan placeholder `{count}` yang dipertahankan persis).

- [ ] **Step 6: Generate types + sync check**

Run: `pnpm --filter @plane/i18n run generate:types && pnpm --filter @plane/i18n run check:sync`
Expected: `0 missing, 0 stale, 0 collisions` (exit 0).

- [ ] **Step 7: Commit**

```bash
git add packages/i18n
git commit -m "i18n(web): war room namespace and sidebar entry"
```

---

### Task 2: Paket types — `packages/types/src/war-room`

**Files:**

- Create: `packages/types/src/war-room/core.ts`
- Create: `packages/types/src/war-room/filters.ts`
- Create: `packages/types/src/war-room/index.ts`
- Modify: `packages/types/src/index.ts:56`

- [ ] **Step 1: Tulis `core.ts`**

Buat `packages/types/src/war-room/core.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWarRoomStatus = "active" | "monitoring" | "resolved" | "archived";

export type TWarRoomSeverity = "sev1" | "sev2" | "sev3" | "sev4";

export type TWarRoomParticipantRole = "commander" | "comms" | "scribe" | "responder";

export interface IWarRoomPrimaryIssue {
  id: string;
  identifier: string;
  name: string;
  priority: string | null;
  state_group: string | null;
}

export interface IWarRoomService {
  id: string;
  name: string;
  status: string;
}

export interface IWarRoomLinkedIssue {
  id: string;
  identifier: string;
  name: string;
  priority: string | null;
}

export interface IWarRoomParticipant {
  id: string;
  member_id: string;
  role: TWarRoomParticipantRole;
  joined_at: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface IWarRoomRunbookItem {
  id: string;
  title: string;
  sort_order: number;
  is_done: boolean;
  done_by_id: string | null;
  done_at: string | null;
  template_key: string | null;
}

export interface IWarRoomBase {
  id: string;
  workspace_id: string;
  project_id: string;
  sequence_id: number;
  name: string;
  description_html: string;
  notes_html: string;
  severity: TWarRoomSeverity;
  status: TWarRoomStatus;
  primary_issue_id: string;
  started_at: string;
  resolved_at: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
}

export interface IWarRoomListItem extends IWarRoomBase {
  primary_issue: IWarRoomPrimaryIssue | null;
  services: IWarRoomService[];
  participants: IWarRoomParticipant[];
  service_count: number;
  participant_count: number;
  message_count: number;
  last_activity_at: string | null;
}

export interface IWarRoom extends IWarRoomBase {
  primary_issue: IWarRoomPrimaryIssue | null;
  services: IWarRoomService[];
  issues: IWarRoomLinkedIssue[];
  participants: IWarRoomParticipant[];
  runbook_items: IWarRoomRunbookItem[];
  counts: {
    messages: number;
  };
}

export interface IWarRoomSummary {
  active: number;
  sev1_2: number;
  resolved_7d: number;
}

export interface IWarRoomEvent {
  id: string;
  actor_id: string | null;
  event_type: string;
  payload: Record<string, unknown>;
  created_at: string;
}

export interface IWarRoomMessageAuthor {
  id: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface IWarRoomMessage {
  id: string;
  war_room_id: string;
  author_id: string | null;
  author: IWarRoomMessageAuthor | null;
  body: string;
  mentions: string[];
  edited_at: string | null;
  created_at: string;
  client_id?: string | null;
}

export type TWarRoomCreatePayload = {
  name?: string;
  primary_issue_id: string;
  severity?: TWarRoomSeverity;
  description_html?: string;
  service_ids?: string[];
};

export type TWarRoomUpdatePayload = {
  name?: string;
  severity?: TWarRoomSeverity;
  status?: TWarRoomStatus;
  description_html?: string;
  notes_html?: string;
};
```

- [ ] **Step 2: Tulis `filters.ts`**

Buat `packages/types/src/war-room/filters.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWarRoomStatusTab = "active" | "resolved" | "all";

export type TWarRoomListParams = {
  status?: string;
  q?: string;
};
```

- [ ] **Step 3: Tulis barrel + export root**

Buat `packages/types/src/war-room/index.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./core";
export * from "./filters";
```

Di `packages/types/src/index.ts`, tambahkan tepat setelah `export * from "./waitlist";` (baris 56):

```ts
export * from "./war-room";
```

- [ ] **Step 4: Build paket types**

Run: `pnpm --filter=@plane/types build`
Expected: sukses tanpa error.

- [ ] **Step 5: Commit**

```bash
git add packages/types
git commit -m "feat(types): war room domain and payload types"
```

---

### Task 3: Paket constants — link helper + config severity/status/role/tab

**Files:**

- Create: `packages/constants/src/war-room.ts`
- Modify: `packages/constants/src/index.ts` (setelah `export * from "./views";`)

- [ ] **Step 1: Tulis `war-room.ts`**

Buat `packages/constants/src/war-room.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TWarRoomParticipantRole, TWarRoomSeverity, TWarRoomStatus, TWarRoomStatusTab } from "@plane/types";

export type TWarRoomToneConfig = {
  /** i18n key label */
  label_key: string;
  /** pill className */
  pill: string;
  /** left rail / dot className */
  rail: string;
};

export const WAR_ROOM_SEVERITIES: TWarRoomSeverity[] = ["sev1", "sev2", "sev3", "sev4"];

export const WAR_ROOM_SEVERITY_CONFIG: Record<TWarRoomSeverity, TWarRoomToneConfig> = {
  sev1: {
    label_key: "war_room.severity_values.sev1",
    pill: "bg-danger-subtle text-danger-primary",
    rail: "bg-danger-primary",
  },
  sev2: {
    label_key: "war_room.severity_values.sev2",
    pill: "bg-warning-subtle text-warning-primary",
    rail: "bg-warning-primary",
  },
  sev3: {
    label_key: "war_room.severity_values.sev3",
    pill: "bg-layer-2 text-secondary",
    rail: "bg-layer-3",
  },
  sev4: {
    label_key: "war_room.severity_values.sev4",
    pill: "bg-layer-2 text-tertiary",
    rail: "bg-layer-2",
  },
};

export const WAR_ROOM_STATUSES: TWarRoomStatus[] = ["active", "monitoring", "resolved", "archived"];

export const WAR_ROOM_STATUS_CONFIG: Record<TWarRoomStatus, TWarRoomToneConfig> = {
  active: {
    label_key: "war_room.status_values.active",
    pill: "bg-danger-subtle text-danger-primary",
    rail: "bg-danger-primary",
  },
  monitoring: {
    label_key: "war_room.status_values.monitoring",
    pill: "bg-warning-subtle text-warning-primary",
    rail: "bg-warning-primary",
  },
  resolved: {
    label_key: "war_room.status_values.resolved",
    pill: "bg-success-subtle text-success-primary",
    rail: "bg-success-primary",
  },
  archived: {
    label_key: "war_room.status_values.archived",
    pill: "bg-layer-2 text-tertiary",
    rail: "bg-layer-2",
  },
};

export const WAR_ROOM_PARTICIPANT_ROLES: TWarRoomParticipantRole[] = ["commander", "comms", "scribe", "responder"];

export const WAR_ROOM_ROLE_LABEL_KEYS: Record<TWarRoomParticipantRole, string> = {
  commander: "war_room.roles.commander",
  comms: "war_room.roles.comms",
  scribe: "war_room.roles.scribe",
  responder: "war_room.roles.responder",
};

/** Tab status → CSV `status` query param (kosong = tanpa filter). */
export const WAR_ROOM_STATUS_TABS: { key: TWarRoomStatusTab; label_key: string; status: string }[] = [
  { key: "active", label_key: "war_room.tabs.active", status: "active,monitoring" },
  { key: "resolved", label_key: "war_room.tabs.resolved", status: "resolved" },
  { key: "all", label_key: "war_room.tabs.all", status: "" },
];

export const getWarRoomLink = (workspaceSlug: string, projectId: string, warRoomId?: string): string =>
  warRoomId
    ? `/${workspaceSlug}/projects/${projectId}/war-rooms/${warRoomId}`
    : `/${workspaceSlug}/projects/${projectId}/war-rooms`;
```

- [ ] **Step 2: Export dari barrel**

Di `packages/constants/src/index.ts`, sisipkan setelah `export * from "./views";`:

```ts
export * from "./war-room";
```

- [ ] **Step 3: Build + typecheck**

Run: `pnpm --filter=@plane/constants check:types && pnpm --filter=@plane/constants build`
Expected: sukses tanpa error.

- [ ] **Step 4: Commit**

```bash
git add packages/constants
git commit -m "feat(constants): war room link helper and display config"
```

---

### Task 4: Helper murni + unit test

**Files:**

- Create: `apps/web/core/services/war-room.helpers.ts`
- Test: `apps/web/core/services/war-room.helpers.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/web/core/services/war-room.helpers.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// helpers
import {
  formatElapsed,
  getWarRoomIncidentLink,
  isActiveWarRoomStatus,
  severityFromPriority,
  statusFilterForTab,
} from "./war-room.helpers";

describe("severityFromPriority", () => {
  it("maps every priority to the backend severity default", () => {
    expect(severityFromPriority("urgent")).toBe("sev1");
    expect(severityFromPriority("high")).toBe("sev2");
    expect(severityFromPriority("medium")).toBe("sev3");
    expect(severityFromPriority("low")).toBe("sev4");
    expect(severityFromPriority("none")).toBe("sev4");
  });

  it("returns null for empty or unknown priorities", () => {
    expect(severityFromPriority(null)).toBeNull();
    expect(severityFromPriority(undefined)).toBeNull();
    expect(severityFromPriority("")).toBeNull();
    expect(severityFromPriority("critical")).toBeNull();
  });
});

describe("isActiveWarRoomStatus", () => {
  it("treats active and monitoring as active", () => {
    expect(isActiveWarRoomStatus("active")).toBe(true);
    expect(isActiveWarRoomStatus("monitoring")).toBe(true);
  });

  it("treats resolved and archived as inactive", () => {
    expect(isActiveWarRoomStatus("resolved")).toBe(false);
    expect(isActiveWarRoomStatus("archived")).toBe(false);
  });
});

describe("statusFilterForTab", () => {
  it("maps tabs to the server status csv", () => {
    expect(statusFilterForTab("active")).toBe("active,monitoring");
    expect(statusFilterForTab("resolved")).toBe("resolved");
    expect(statusFilterForTab("all")).toBeUndefined();
  });
});

describe("formatElapsed", () => {
  const startedAt = "2026-01-01T00:00:00.000Z";

  it("formats a running timer with the injected now", () => {
    const now = new Date("2026-01-01T01:02:03.000Z").getTime();
    expect(formatElapsed(startedAt, null, now)).toBe("01:02:03");
  });

  it("uses resolved_at when the room is closed", () => {
    const resolvedAt = "2026-01-01T00:10:00.000Z";
    const now = new Date("2026-01-02T00:00:00.000Z").getTime();
    expect(formatElapsed(startedAt, resolvedAt, now)).toBe("00:10:00");
  });

  it("never returns negative time", () => {
    const now = new Date("2025-12-31T23:59:00.000Z").getTime();
    expect(formatElapsed(startedAt, null, now)).toBe("00:00:00");
  });
});

describe("getWarRoomIncidentLink", () => {
  it("links to the browse work item route", () => {
    expect(getWarRoomIncidentLink("acme", "PROJ-12")).toBe("/acme/browse/PROJ-12/");
  });
});
```

- [ ] **Step 2: Run test untuk memastikan gagal**

Run: `pnpm --filter=web test -- war-room.helpers`
Expected: FAIL — modul `./war-room.helpers` belum ada.

- [ ] **Step 3: Implementasi helper**

Buat `apps/web/core/services/war-room.helpers.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TWarRoomSeverity, TWarRoomStatus, TWarRoomStatusTab } from "@plane/types";

/** Mirrors the server-side `severity_from_priority` mapping. */
export const severityFromPriority = (priority: string | null | undefined): TWarRoomSeverity | null => {
  switch (priority) {
    case "urgent":
      return "sev1";
    case "high":
      return "sev2";
    case "medium":
      return "sev3";
    case "low":
    case "none":
      return "sev4";
    default:
      return null;
  }
};

export const isActiveWarRoomStatus = (status: TWarRoomStatus): boolean =>
  status === "active" || status === "monitoring";

export const statusFilterForTab = (tab: TWarRoomStatusTab): string | undefined => {
  switch (tab) {
    case "active":
      return "active,monitoring";
    case "resolved":
      return "resolved";
    case "all":
      return undefined;
  }
};

/** `HH:MM:SS`; `endAt` kosong = timer berjalan memakai `now`. */
export const formatElapsed = (startedAt: string, endAt: string | null, now: number = Date.now()): string => {
  const startMs = new Date(startedAt).getTime();
  const endMs = endAt ? new Date(endAt).getTime() : now;
  const totalSeconds = Number.isNaN(startMs) ? 0 : Math.max(0, Math.floor((endMs - startMs) / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":");
};

export const getWarRoomIncidentLink = (workspaceSlug: string, identifier: string): string =>
  `/${workspaceSlug}/browse/${identifier}/`;
```

- [ ] **Step 4: Run test untuk memastikan lulus**

Run: `pnpm --filter=web test -- war-room.helpers`
Expected: PASS (5 describe, 9 test).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/war-room.helpers.ts apps/web/core/services/war-room.helpers.test.ts
git commit -m "feat(web): war room list helpers with unit tests"
```

---

### Task 5: API service `war-room.service.ts`

**Files:**

- Create: `apps/web/core/services/war-room.service.ts`

- [ ] **Step 1: Tulis service**

Buat `apps/web/core/services/war-room.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IWarRoom,
  IWarRoomListItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomListParams,
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
    return this.get(`${this.basePath(workspaceSlug, projectId)}/${warRoomId}/`)
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
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: sukses (route typegen ikut jalan).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/war-room.service.ts
git commit -m "feat(web): war room API service"
```

---

### Task 6: Data store `war-room.store.ts` + unit test

**Files:**

- Create: `apps/web/core/store/war-room.store.ts`
- Test: `apps/web/core/store/war-room.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/web/core/store/war-room.store.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import type { IWarRoom, IWarRoomListItem, IWarRoomSummary } from "@plane/types";
// store
import { WarRoomStore } from "./war-room.store";

const makeRoom = (overrides: Partial<IWarRoomListItem> = {}): IWarRoomListItem =>
  ({
    id: "room-1",
    workspace_id: "ws-1",
    project_id: "project-1",
    sequence_id: 1,
    name: "Room 1",
    description_html: "<p></p>",
    notes_html: "",
    severity: "sev3",
    status: "active",
    primary_issue_id: "issue-1",
    started_at: "2026-01-01T00:00:00.000Z",
    resolved_at: null,
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    created_by: "user-1",
    primary_issue: null,
    services: [],
    participants: [],
    service_count: 0,
    participant_count: 0,
    message_count: 0,
    last_activity_at: null,
    ...overrides,
  }) as IWarRoomListItem;

const makeDetail = (overrides: Partial<IWarRoom> = {}): IWarRoom =>
  ({
    ...makeRoom(),
    issues: [],
    runbook_items: [],
    counts: { messages: 0 },
    ...overrides,
  }) as unknown as IWarRoom;

const makeStore = () => {
  const store = new WarRoomStore({} as never);
  const warRoomService = {
    getWarRooms: vi.fn(async () => [makeRoom()]),
    getWarRoomSummary: vi.fn(async (): Promise<IWarRoomSummary> => ({ active: 1, sev1_2: 0, resolved_7d: 0 })),
    getWarRoom: vi.fn(async () => makeDetail()),
    createWarRoom: vi.fn(async () => makeDetail({ id: "room-2", sequence_id: 2 })),
  };
  (store as unknown as { warRoomService: typeof warRoomService }).warRoomService = warRoomService;
  return { store, warRoomService };
};

describe("WarRoomStore.fetchWarRooms", () => {
  it("stores list items, order, and fetched flag", async () => {
    const { store, warRoomService } = makeStore();

    await store.fetchWarRooms("acme", "project-1", { status: "active,monitoring" });

    expect(warRoomService.getWarRooms).toHaveBeenCalledWith("acme", "project-1", { status: "active,monitoring" });
    expect(store.getProjectWarRoomIds("project-1")).toEqual(["room-1"]);
    expect(store.getWarRoomById("room-1")?.name).toBe("Room 1");
    expect(store.fetchedMap["project-1"]).toBe(true);
    expect(store.errorMap["project-1"]).toBe(false);
  });

  it("flags errors and returns undefined", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getWarRooms.mockRejectedValueOnce(new Error("boom"));

    const result = await store.fetchWarRooms("acme", "project-1");

    expect(result).toBeUndefined();
    expect(store.errorMap["project-1"]).toBe(true);
    expect(store.loader).toBe(false);
  });
});

describe("WarRoomStore.getActiveWarRoomByIssue", () => {
  it("matches only active or monitoring rooms for the incident", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getWarRooms.mockResolvedValueOnce([
      makeRoom({ id: "room-resolved", status: "resolved", primary_issue_id: "issue-1" }),
      makeRoom({ id: "room-monitoring", status: "monitoring", primary_issue_id: "issue-1" }),
      makeRoom({ id: "room-other", status: "active", primary_issue_id: "issue-2" }),
    ]);

    await store.fetchWarRooms("acme", "project-1");

    expect(store.getActiveWarRoomByIssue("project-1", "issue-1")?.id).toBe("room-monitoring");
    expect(store.getActiveWarRoomByIssue("project-1", "issue-2")?.id).toBe("room-other");
    expect(store.getActiveWarRoomByIssue("project-1", "issue-3")).toBeNull();
  });
});

describe("WarRoomStore.fetchWarRoomSummary", () => {
  it("stores the summary per project", async () => {
    const { store } = makeStore();

    await store.fetchWarRoomSummary("acme", "project-1");

    expect(store.getProjectSummary("project-1")).toEqual({ active: 1, sev1_2: 0, resolved_7d: 0 });
  });
});

describe("WarRoomStore.fetchWarRoomDetail", () => {
  it("stores detail and flags failures per room", async () => {
    const { store, warRoomService } = makeStore();

    await store.fetchWarRoomDetail("acme", "project-1", "room-1");
    expect(store.getWarRoomDetailById("room-1")?.counts.messages).toBe(0);
    expect(store.detailErrorMap["room-1"]).toBe(false);

    warRoomService.getWarRoom.mockRejectedValueOnce(new Error("boom"));
    const result = await store.fetchWarRoomDetail("acme", "project-1", "room-missing");
    expect(result).toBeUndefined();
    expect(store.detailErrorMap["room-missing"]).toBe(true);
  });
});

describe("WarRoomStore.createWarRoom", () => {
  it("stores the created detail and prepends its id", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRooms("acme", "project-1");

    const room = await store.createWarRoom("acme", "project-1", { primary_issue_id: "issue-2" });

    expect(warRoomService.createWarRoom).toHaveBeenCalledWith("acme", "project-1", { primary_issue_id: "issue-2" });
    expect(room.id).toBe("room-2");
    expect(store.getWarRoomDetailById("room-2")?.sequence_id).toBe(2);
    expect(store.getProjectWarRoomIds("project-1")).toEqual(["room-2", "room-1"]);
  });
});
```

- [ ] **Step 2: Run test untuk memastikan gagal**

Run: `pnpm --filter=web test -- war-room.store`
Expected: FAIL — modul `./war-room.store` belum ada.

- [ ] **Step 3: Implementasi store**

Buat `apps/web/core/store/war-room.store.ts`:

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
  IWarRoomListItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomListParams,
} from "@plane/types";
// helpers
import { isActiveWarRoomStatus } from "@/services/war-room.helpers";
// services
import { WarRoomService } from "@/services/war-room.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IWarRoomStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  warRoomMap: Record<string, IWarRoomListItem>;
  warRoomIdsMap: Record<string, string[]>;
  detailMap: Record<string, IWarRoom>;
  summaryMap: Record<string, IWarRoomSummary>;
  errorMap: Record<string, boolean>;
  detailErrorMap: Record<string, boolean>;
  getWarRoomById: (warRoomId: string) => IWarRoomListItem | null;
  getProjectWarRoomIds: (projectId: string) => string[] | null;
  getWarRoomDetailById: (warRoomId: string) => IWarRoom | null;
  getProjectSummary: (projectId: string) => IWarRoomSummary | null;
  getActiveWarRoomByIssue: (projectId: string, issueId: string) => IWarRoomListItem | null;
  fetchWarRooms: (
    workspaceSlug: string,
    projectId: string,
    params?: TWarRoomListParams
  ) => Promise<IWarRoomListItem[] | undefined>;
  fetchWarRoomSummary: (workspaceSlug: string, projectId: string) => Promise<IWarRoomSummary | undefined>;
  fetchWarRoomDetail: (workspaceSlug: string, projectId: string, warRoomId: string) => Promise<IWarRoom | undefined>;
  createWarRoom: (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => Promise<IWarRoom>;
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
      fetchWarRooms: action,
      fetchWarRoomSummary: action,
      fetchWarRoomDetail: action,
      createWarRoom: action,
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
}
```

- [ ] **Step 4: Run test untuk memastikan lulus**

Run: `pnpm --filter=web test -- war-room.store`
Expected: PASS (5 describe, 6 test).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/war-room.store.ts apps/web/core/store/war-room.store.test.ts
git commit -m "feat(web): war room store with list, detail and summary"
```

---

### Task 7: Filter store, hooks, registrasi root store

**Files:**

- Create: `apps/web/core/store/war-room_filter.store.ts`
- Create: `apps/web/core/hooks/store/use-war-room.ts`
- Create: `apps/web/core/hooks/store/use-war-room-filter.ts`
- Modify: `apps/web/core/store/root.store.ts` (imports ~baris 53, fields ~baris 93, constructor ~baris 135, `resetOnSignOut` ~baris 175)

- [ ] **Step 1: Tulis filter store**

Buat `apps/web/core/store/war-room_filter.store.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, observable, makeObservable, reaction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { TWarRoomStatusTab } from "@plane/types";
// store
import type { CoreRootStore } from "./root.store";

export interface IWarRoomFilterStore {
  statusTabs: Record<string, TWarRoomStatusTab>;
  searchQuery: string;
  getStatusTab: (projectId: string) => TWarRoomStatusTab;
  setStatusTab: (projectId: string, tab: TWarRoomStatusTab) => void;
  updateSearchQuery: (query: string) => void;
}

export class WarRoomFilterStore implements IWarRoomFilterStore {
  statusTabs: Record<string, TWarRoomStatusTab> = {};
  searchQuery: string = "";
  rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      statusTabs: observable,
      searchQuery: observable.ref,
      setStatusTab: action,
      updateSearchQuery: action,
    });
    this.rootStore = _rootStore;

    reaction(
      () => this.rootStore.router.projectId,
      () => {
        this.searchQuery = "";
      }
    );
  }

  getStatusTab = computedFn((projectId: string): TWarRoomStatusTab => this.statusTabs[projectId] ?? "active");

  setStatusTab = (projectId: string, tab: TWarRoomStatusTab) => {
    this.statusTabs[projectId] = tab;
  };

  updateSearchQuery = (query: string) => {
    this.searchQuery = query;
  };
}
```

- [ ] **Step 2: Tulis hook**

Buat `apps/web/core/hooks/store/use-war-room.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// mobx store
import { StoreContext } from "@/lib/store-context";
// types
import type { IWarRoomStore } from "@/store/war-room.store";

export const useWarRoom = (): IWarRoomStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useWarRoom must be used within StoreProvider");
  return context.warRoom;
};
```

Buat `apps/web/core/hooks/store/use-war-room-filter.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// mobx store
import { StoreContext } from "@/lib/store-context";
// types
import type { IWarRoomFilterStore } from "@/store/war-room_filter.store";

export const useWarRoomFilter = (): IWarRoomFilterStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useWarRoomFilter must be used within StoreProvider");
  return context.warRoomFilter;
};
```

- [ ] **Step 3: Registrasi di root store (4 titik)**

Di `apps/web/core/store/root.store.ts`:

1. Import (setelah baris import `ServiceFilterStore`, sekitar baris 56):

```ts
import type { IWarRoomStore } from "./war-room.store";
import { WarRoomStore } from "./war-room.store";
import type { IWarRoomFilterStore } from "./war-room_filter.store";
import { WarRoomFilterStore } from "./war-room_filter.store";
```

2. Field class (setelah `serviceFilter: IServiceFilterStore;` baris 94):

```ts
warRoom: IWarRoomStore;
warRoomFilter: IWarRoomFilterStore;
```

3. Constructor (setelah `this.serviceFilter = new ServiceFilterStore(this);` baris 136):

```ts
this.warRoom = new WarRoomStore(this);
this.warRoomFilter = new WarRoomFilterStore(this);
```

4. `resetOnSignOut` (setelah `this.serviceFilter = new ServiceFilterStore(this);` di blok reset, sekitar baris 176):

```ts
this.warRoom = new WarRoomStore(this);
this.warRoomFilter = new WarRoomFilterStore(this);
```

- [ ] **Step 4: Typecheck + test**

Run: `pnpm --filter=web check:types && pnpm --filter=web test -- war-room`
Expected: typecheck sukses; test war-room helper + store lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/war-room_filter.store.ts apps/web/core/hooks/store/use-war-room.ts apps/web/core/hooks/store/use-war-room-filter.ts apps/web/core/store/root.store.ts
git commit -m "feat(web): war room filter store and hooks"
```

---

### Task 8: `ExistingIssuesListModal` — mode single-select

**Files:**

- Modify: `apps/web/core/components/core/modals/existing-issues-list-modal.tsx`

- [ ] **Step 1: Tambah prop + handler**

Di `existing-issues-list-modal.tsx`:

1. Tambahkan prop pada `type Props` (setelah `selectedWorkItemIds`):

```ts
  selectionMode?: "multiple" | "single";
```

2. Tambahkan ke destructuring props (setelah `selectedWorkItemIds`):

```ts
    selectionMode = "multiple",
```

3. Ganti blok `onSubmit` (baris 82-98) dengan versi berikut:

```tsx
const submitIssues = async (issuesToSubmit: ISearchIssueResponse[]) => {
  setIsSubmitting(true);

  await handleOnSubmit(issuesToSubmit).finally(() => setIsSubmitting(false));

  handleClose();
};

const onSubmit = async () => {
  if (selectedIssues.length === 0) {
    setToast({
      type: TOAST_TYPE.ERROR,
      title: t("toast.error"),
      message: t("issue.select.error"),
    });

    return;
  }

  await submitIssues(selectedIssues);
};

const handleSelectIssue = (issue: ISearchIssueResponse | null) => {
  if (issue === null) return;
  if (selectionMode === "single") {
    if (isSubmitting) return;
    void submitIssues([issue]);
    return;
  }
  if (selectedIssues.some((i) => i.id === issue.id))
    setSelectedIssues((prevData) => prevData.filter((i) => i.id !== issue.id));
  else setSelectedIssues((prevData) => [...prevData, issue]);
};
```

4. Ganti isi `onChange` pada `<Combobox ...>` (baris 142-147) menjadi:

```tsx
onChange = { handleSelectIssue };
```

- [ ] **Step 2: Sembunyikan elemen multi-select saat single mode**

Masih di file yang sama:

1. Bungkus blok chips (`{selectedIssues.length > 0 ? ... : ...}` baris 164-193) dengan guard single mode:

```tsx
{
  selectionMode === "multiple" &&
    (selectedIssues.length > 0 ? (
      <div className="mt-1 flex flex-wrap items-center gap-2">
        {selectedIssues.map((issue) => (
          <div
            key={issue.id}
            className="flex items-center gap-1 rounded-md border border-subtle bg-layer-1 py-1 pl-2 text-11 whitespace-nowrap text-primary"
          >
            <IssueIdentifier
              projectId={issue.project_id}
              issueTypeId={issue.type_id}
              projectIdentifier={issue.project__identifier}
              issueSequenceId={issue.sequence_id}
              size="xs"
              variant="secondary"
            />
            <button
              type="button"
              className="group p-1"
              onClick={() => setSelectedIssues((prevData) => prevData.filter((i) => i.id !== issue.id))}
            >
              <CloseOutline className="h-3 w-3 text-secondary group-hover:text-primary" />
            </button>
          </div>
        ))}
      </div>
    ) : (
      <div className="w-min rounded-md border border-subtle bg-layer-1 p-2 text-11 whitespace-nowrap">
        {t("issue.select.empty")}
      </div>
    ));
}
```

2. Di dalam `filteredIssues.map`, sembunyikan checkbox saat single mode — ganti baris `<input type="checkbox" checked={selected} readOnly />` menjadi:

```tsx
{
  selectionMode === "multiple" && <input type="checkbox" checked={selected} readOnly />;
}
```

3. Footer (baris 314-337): sembunyikan tombol select-all + tombol submit pada single mode, dan rapikan alignment:

```tsx
<div className="flex items-center justify-between p-3">
  {selectionMode === "multiple" ? (
    <Button
      variant="link"
      onClick={handleSelectIssues}
      disabled={filteredIssues.length === 0}
      className={filteredIssues.length === 0 ? "p-0" : ""}
    >
      {selectedIssues.length === issues.length ? t("issue.select.deselect_all") : t("issue.select.select_all")}
    </Button>
  ) : (
    <span />
  )}
  <div className="flex items-center justify-end gap-2">
    <Button variant="secondary" size="lg" onClick={handleClose}>
      {t("common.cancel")}
    </Button>
    {selectionMode === "multiple" && (
      <Button
        variant="primary"
        size="lg"
        onClick={onSubmit}
        loading={isSubmitting}
        disabled={isSubmitting || selectedIssues.length === 0}
      >
        {isSubmitting ? t("common.adding") : t("issue.select.add_selected")}
      </Button>
    )}
  </div>
</div>
```

- [ ] **Step 3: Typecheck + pastikan consumer lama tidak berubah**

Run: `pnpm --filter=web check:types`
Expected: sukses. Semua caller lama (services/detail/work-items.tsx, relation-select.tsx, dll.) tidak berubah karena default `"multiple"`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/core/modals/existing-issues-list-modal.tsx
git commit -m "feat(web): single-select mode for existing issues modal"
```

---

### Task 9: Modal create war room

**Files:**

- Create: `apps/web/core/components/war-rooms/create/create-war-room-modal.tsx`

- [ ] **Step 1: Tulis komponen modal**

Buat `apps/web/core/components/war-rooms/create/create-war-room-modal.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { Controller, useForm } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { AlertOctagonOutline, CloseOutline, InfoOutline } from "@makeplane/propel/icons";
import { WAR_ROOM_SEVERITIES, WAR_ROOM_SEVERITY_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse, TWarRoomSeverity } from "@plane/types";
import { CustomSelect, EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
import { RichTextEditor } from "@/components/editor/rich-text";
import { ServiceMultiSelect } from "@/components/services/select/service-multi-select";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS } from "@/services/service.helpers";
import { severityFromPriority } from "@/services/war-room.helpers";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useService } from "@/hooks/store/use-service";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkspace } from "@/hooks/store/use-workspace";
// services
import { WorkspaceService } from "@/services/workspace.service";

const workspaceService = new WorkspaceService();

export type TWarRoomIncidentOption = {
  id: string;
  name: string;
  priority: string | null;
};

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  projectId: string;
  initialIssue?: TWarRoomIncidentOption;
  initialIssueId?: string;
};

type TWarRoomCreateFormValues = {
  name: string;
  severity: TWarRoomSeverity | "";
  description_html: string;
  service_ids: string[];
};

export const CreateWarRoomModal = observer(function CreateWarRoomModal(props: Props) {
  const { isOpen, onClose, workspaceSlug, projectId, initialIssue, initialIssueId } = props;
  // plane hooks
  const { t } = useTranslation();
  // router
  const router = useAppRouter();
  // store hooks
  const { createWarRoom, getWarRoomById } = useWarRoom();
  const { getWorkspaceBySlug } = useWorkspace();
  const { fetchedMap, workItemLinkMap } = useService();
  // states
  const [selectedIssue, setSelectedIssue] = useState<TWarRoomIncidentOption | null>(null);
  const [isIncidentPickerOpen, setIsIncidentPickerOpen] = useState(false);
  const [duplicateRoomId, setDuplicateRoomId] = useState<string | null>(null);
  // refs
  const seededIssueId = useRef<string | null>(null);
  // form
  const {
    control,
    formState: { errors, isSubmitting },
    handleSubmit,
    reset,
    setValue,
  } = useForm<TWarRoomCreateFormValues>({
    defaultValues: {
      name: "",
      severity: "",
      description_html: "<p></p>",
      service_ids: [],
    },
  });

  useEffect(() => {
    if (!isOpen) return;
    seededIssueId.current = null;
    setDuplicateRoomId(null);
    setIsIncidentPickerOpen(false);
    const issue = initialIssue ?? (initialIssueId ? { id: initialIssueId, name: "", priority: null } : null);
    setSelectedIssue(issue);
    reset({
      name: initialIssue?.name ?? "",
      severity: severityFromPriority(initialIssue?.priority ?? null) ?? "",
      description_html: "<p></p>",
      service_ids: [],
    });
  }, [isOpen, initialIssue, initialIssueId, reset]);

  // Default affected services from the incident's service-issues links once the
  // service store is hydrated. Runs once per selected incident.
  useEffect(() => {
    if (!isOpen || !selectedIssue || seededIssueId.current === selectedIssue.id) return;
    if (!fetchedMap[projectId]) return;
    seededIssueId.current = selectedIssue.id;
    const linkedServiceIds = Object.values(workItemLinkMap)
      .filter((link) => link.issue_id === selectedIssue.id && link.project_id === projectId)
      .map((link) => link.service_id);
    if (linkedServiceIds.length > 0) setValue("service_ids", linkedServiceIds);
  }, [isOpen, selectedIssue, fetchedMap, workItemLinkMap, projectId, setValue]);

  const handleIncidentSelect = async (issues: ISearchIssueResponse[]) => {
    const issue = issues[0];
    if (!issue) return;
    seededIssueId.current = null;
    setSelectedIssue({ id: issue.id, name: issue.name, priority: null });
    setValue("name", issue.name);
    setIsIncidentPickerOpen(false);
  };

  const handleCreateWarRoom = async (formData: TWarRoomCreateFormValues) => {
    if (!selectedIssue) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("toast.error"),
        message: t("war_room.create.incident_required"),
      });
      return;
    }
    const descriptionHtml = formData.description_html.trim() !== "" ? formData.description_html : "<p></p>";
    try {
      const room = await createWarRoom(workspaceSlug, projectId, {
        name: formData.name.trim() !== "" ? formData.name.trim() : undefined,
        primary_issue_id: selectedIssue.id,
        severity: formData.severity === "" ? undefined : formData.severity,
        description_html: descriptionHtml,
        service_ids: formData.service_ids,
      });
      onClose();
      router.push(getWarRoomLink(workspaceSlug, projectId, room.id));
    } catch (error) {
      const apiError = error as { error?: string; war_room_id?: string; message?: string };
      if (apiError?.error === "active_war_room_exists" && apiError.war_room_id) {
        setDuplicateRoomId(apiError.war_room_id);
        return;
      }
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("toast.error"),
        message: apiError?.message ?? t("war_room.load_error.description"),
      });
    }
  };

  const duplicateRoom = duplicateRoomId ? getWarRoomById(duplicateRoomId) : null;
  const duplicateLabel = duplicateRoom ? `WR-${duplicateRoom.sequence_id}` : t("war_room.create.open_existing");
  const stableDescriptionHtml = "<p></p>";

  return (
    <>
      <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
        <form onSubmit={handleSubmit(handleCreateWarRoom)}>
          <div className="space-y-5 p-5">
            <div className="flex items-center gap-x-3">
              <h3 className="text-18 font-medium text-secondary">{t("war_room.create.title")}</h3>
            </div>

            {duplicateRoomId && (
              <div className="relative flex items-center gap-2 rounded-md border border-danger-strong/50 bg-danger-subtle p-2">
                <InfoOutline width={16} height={16} className="text-danger-primary" />
                <div className="w-full text-13 font-medium text-danger-primary">
                  {t("war_room.create.duplicate_title")}
                  <p className="text-12 font-normal">{t("war_room.create.duplicate_description")}</p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    onClose();
                    router.push(getWarRoomLink(workspaceSlug, projectId, duplicateRoomId));
                  }}
                >
                  {duplicateLabel}
                </Button>
                <button type="button" onClick={() => setDuplicateRoomId(null)}>
                  <CloseOutline className="h-3.5 w-3.5 text-secondary hover:text-primary" />
                </button>
              </div>
            )}

            <div className="space-y-1">
              <label className="text-12 text-secondary">{t("war_room.create.incident")}</label>
              <button
                type="button"
                onClick={() => setIsIncidentPickerOpen(true)}
                className="flex w-full items-center gap-2 rounded-sm border-[0.5px] border-strong px-2 py-2 text-left hover:bg-layer-1"
              >
                <AlertOctagonOutline className="h-4 w-4 shrink-0 text-tertiary" />
                <span className="truncate text-13 text-primary">
                  {selectedIssue
                    ? selectedIssue.name || t("war_room.create.incident_selected")
                    : t("war_room.create.select_incident")}
                </span>
              </button>
            </div>

            <div className="space-y-1">
              <Controller
                control={control}
                name="name"
                rules={{
                  required: t("title_is_required"),
                  maxLength: {
                    value: 255,
                    message: t("title_should_be_less_than_255_characters"),
                  },
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
                        placeholder={t("war_room.create.name_placeholder")}
                      />
                    </InputGroup>
                  </Field>
                )}
              />
              <span className="text-11 text-danger-primary">{errors?.name?.message}</span>
            </div>

            <div>
              <Controller
                name="description_html"
                control={control}
                render={({ field: { onChange } }) => (
                  <RichTextEditor
                    editable
                    key={selectedIssue?.id ?? "war-room-create"}
                    id="war-room-description-editor"
                    initialValue={stableDescriptionHtml}
                    value={stableDescriptionHtml}
                    workspaceSlug={workspaceSlug}
                    workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
                    projectId={projectId}
                    disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
                    onChange={(_descriptionJson: object, descriptionHtml: string) => {
                      onChange(descriptionHtml);
                    }}
                    placeholder={t("war_room.create.description")}
                    searchMentionCallback={async (payload) =>
                      await workspaceService.searchEntity(workspaceSlug, {
                        ...payload,
                        project_id: projectId,
                      })
                    }
                    containerClassName="min-h-24 rounded-md border border-subtle"
                    uploadFile={async () => {
                      throw new Error("File upload is disabled for war room descriptions.");
                    }}
                    duplicateFile={async () => {
                      throw new Error("File upload is disabled for war room descriptions.");
                    }}
                  />
                )}
              />
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <div className="h-7">
                <Controller
                  control={control}
                  name="severity"
                  render={({ field: { value, onChange } }) => (
                    <CustomSelect
                      value={value}
                      label={
                        <span className="flex items-center gap-2 py-0.5 text-11">
                          {value ? (
                            t(WAR_ROOM_SEVERITY_CONFIG[value].label_key)
                          ) : (
                            <span className="text-secondary">{t("war_room.create.severity")}</span>
                          )}
                        </span>
                      }
                      onChange={onChange}
                      noChevron
                    >
                      <CustomSelect.Option value="">
                        <span>{t("war_room.create.severity_auto")}</span>
                      </CustomSelect.Option>
                      {WAR_ROOM_SEVERITIES.map((severity) => (
                        <CustomSelect.Option key={severity} value={severity}>
                          <span>{t(WAR_ROOM_SEVERITY_CONFIG[severity].label_key)}</span>
                        </CustomSelect.Option>
                      ))}
                    </CustomSelect>
                  )}
                />
              </div>
              <div className="h-7">
                <Controller
                  control={control}
                  name="service_ids"
                  render={({ field: { value, onChange } }) => (
                    <ServiceMultiSelect
                      workspaceSlug={workspaceSlug}
                      projectId={projectId}
                      value={value}
                      onChange={onChange}
                    />
                  )}
                />
              </div>
            </div>
          </div>
          <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
            <Button variant="secondary" size="lg" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              size="lg"
              type="submit"
              loading={isSubmitting}
              disabled={isSubmitting || !selectedIssue}
            >
              {t("war_room.create.submit")}
            </Button>
          </div>
        </form>
      </ModalCore>

      <ExistingIssuesListModal
        isOpen={isIncidentPickerOpen}
        handleClose={() => setIsIncidentPickerOpen(false)}
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        searchParams={{ workspace_search: false }}
        selectionMode="single"
        handleOnSubmit={handleIncidentSelect}
      />
    </>
  );
});
```

- [ ] **Step 2: Lint file baru**

Run: `pnpm exec oxlint apps/web/core/components/war-rooms/create/create-war-room-modal.tsx`
Expected: 0 error (warning baseline tidak bertambah).

- [ ] **Step 3: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: sukses.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/war-rooms/create/create-war-room-modal.tsx
git commit -m "feat(web): create war room modal"
```

---

### Task 10: Komponen list (chips, error, search, baris, board)

**Files:**

- Create: `apps/web/core/components/war-rooms/list/war-room-summary-chips.tsx`
- Create: `apps/web/core/components/war-rooms/list/war-room-load-error-state.tsx`
- Create: `apps/web/core/components/war-rooms/list/war-room-search-input.tsx`
- Create: `apps/web/core/components/war-rooms/list/war-rooms-board-row.tsx`
- Create: `apps/web/core/components/war-rooms/list/war-rooms-board.tsx`
- Create: `apps/web/core/components/war-rooms/list/war-room-view-header.tsx`

- [ ] **Step 1: Summary chips**

Buat `war-room-summary-chips.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { IWarRoomSummary } from "@plane/types";
import { cn } from "@plane/utils";

type Props = {
  summary: IWarRoomSummary;
};

const SUMMARY_CHIPS: { key: keyof IWarRoomSummary; className: string; label_key: string }[] = [
  { key: "active", className: "bg-danger-subtle text-danger-primary", label_key: "war_room.summary.active" },
  { key: "sev1_2", className: "bg-warning-subtle text-warning-primary", label_key: "war_room.summary.sev1_2" },
  {
    key: "resolved_7d",
    className: "bg-success-subtle text-success-primary",
    label_key: "war_room.summary.resolved_7d",
  },
];

export function WarRoomSummaryChips({ summary }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-subtle px-3 py-2">
      {SUMMARY_CHIPS.map((chip) => (
        <span key={chip.key} className={cn("rounded-full px-2 py-0.5 text-11 font-medium", chip.className)}>
          {t(chip.label_key, { count: summary[chip.key] })}
        </span>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Load error state**

Buat `war-room-load-error-state.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";

type Props = {
  onRetry: () => void;
};

export function WarRoomLoadErrorState({ onRetry }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <WarningTriangleOutline className="size-8 text-tertiary" />
      <p className="text-sm font-medium text-primary">{t("war_room.load_error.title")}</p>
      <p className="text-xs text-secondary">{t("war_room.load_error.description")}</p>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        {t("war_room.load_error.retry")}
      </Button>
    </div>
  );
}
```

- [ ] **Step 3: Search input**

Buat `war-room-search-input.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { CloseOutline, SearchOutline } from "@makeplane/propel/icons";
import { useOutsideClickDetector } from "@plane/hooks";
import { useTranslation } from "@plane/i18n";
import { IconButton } from "@plane/propel/icon-button";
// helpers
import { cn } from "@plane/utils";
// hooks
import { useWarRoomFilter } from "@/hooks/store/use-war-room-filter";

export const WarRoomSearchInput = observer(function WarRoomSearchInput() {
  // refs
  const inputRef = useRef<HTMLInputElement>(null);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { searchQuery, updateSearchQuery } = useWarRoomFilter();
  // states
  const [isSearchOpen, setIsSearchOpen] = useState(searchQuery !== "");

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      if (searchQuery && searchQuery.trim() !== "") updateSearchQuery("");
      else {
        setIsSearchOpen(false);
        inputRef.current?.blur();
      }
    }
  };

  useOutsideClickDetector(inputRef, () => {
    if (isSearchOpen && searchQuery.trim() === "") setIsSearchOpen(false);
  });

  useEffect(() => {
    if (searchQuery.trim() !== "") setIsSearchOpen(true);
  }, [searchQuery]);

  return (
    <div className="flex items-center">
      {!isSearchOpen && (
        <IconButton
          variant="ghost"
          size="lg"
          className="-mr-1"
          onClick={() => {
            setIsSearchOpen(true);
            inputRef.current?.focus();
          }}
          icon={SearchOutline}
        />
      )}
      <div
        className={cn(
          "ml-auto flex w-0 items-center justify-start gap-1 overflow-hidden rounded-md border border-transparent bg-surface-1 text-placeholder opacity-0 transition-[width] ease-linear",
          {
            "w-64 border-subtle px-2.5 py-1.5 opacity-100": isSearchOpen,
          }
        )}
      >
        <SearchOutline className="h-3.5 w-3.5" />
        <input
          ref={inputRef}
          className="w-full max-w-[234px] border-none bg-transparent text-13 text-primary placeholder:text-placeholder focus:outline-none"
          placeholder={t("war_room.search.placeholder")}
          value={searchQuery}
          onChange={(e) => updateSearchQuery(e.target.value)}
          onKeyDown={handleInputKeyDown}
        />
        {isSearchOpen && (
          <button
            type="button"
            className="grid place-items-center"
            onClick={() => {
              updateSearchQuery("");
              setIsSearchOpen(false);
            }}
          >
            <CloseOutline className="h-3 w-3" />
          </button>
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 4: Baris board**

Buat `war-rooms-board-row.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { AvatarGroup } from "@makeplane/propel/components/avatar-group";
import { WAR_ROOM_SEVERITY_CONFIG, WAR_ROOM_STATUS_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { cn, getFileURL } from "@plane/utils";
// helpers
import { formatElapsed, getWarRoomIncidentLink, isActiveWarRoomStatus } from "@/services/war-room.helpers";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  warRoomId: string;
};

export const WarRoomsBoardRow = observer(function WarRoomsBoardRow({ warRoomId }: Props) {
  // router
  const { workspaceSlug } = useParams();
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomById } = useWarRoom();
  // states
  const [now, setNow] = useState(() => Date.now());
  // derived values
  const room = getWarRoomById(warRoomId);
  const isRunning = room ? isActiveWarRoomStatus(room.status) : false;

  useEffect(() => {
    if (!isRunning) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [isRunning]);

  if (!room) return null;

  const severityConfig = WAR_ROOM_SEVERITY_CONFIG[room.severity];
  const statusConfig = WAR_ROOM_STATUS_CONFIG[room.status];
  const warRoomLink = getWarRoomLink(workspaceSlug?.toString() ?? "", room.project_id, room.id);
  const elapsed = formatElapsed(room.started_at, room.resolved_at, now);
  const visibleParticipants = room.participants.slice(0, 3);
  const overflowParticipants = room.participants.length - visibleParticipants.length;
  const visibleServices = room.services.slice(0, 3);
  const overflowServices = room.services.length - visibleServices.length;

  return (
    <div
      role="link"
      tabIndex={0}
      onClick={() => router.push(warRoomLink)}
      onKeyDown={(e) => {
        if (e.key === "Enter") router.push(warRoomLink);
      }}
      className="relative flex cursor-pointer items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover"
    >
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", severityConfig.rail)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className="shrink-0 text-11 font-medium text-tertiary">WR-{room.sequence_id}</span>
        <span className="truncate text-13 font-medium text-primary">{room.name}</span>
        {room.primary_issue && workspaceSlug && (
          <Link
            href={getWarRoomIncidentLink(workspaceSlug.toString(), room.primary_issue.identifier)}
            onClick={(e) => e.stopPropagation()}
            className="hidden shrink-0 rounded-xs border border-subtle px-1.5 py-0.5 text-10 text-secondary hover:text-primary sm:block"
          >
            {room.primary_issue.identifier}
          </Link>
        )}
      </div>
      <span
        className={cn("hidden shrink-0 rounded-full px-2 py-0.5 text-10 font-medium sm:block", severityConfig.pill)}
      >
        {t(severityConfig.label_key)}
      </span>
      <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-10 font-medium", statusConfig.pill)}>
        {t(statusConfig.label_key)}
      </span>
      <div className="hidden items-center gap-1 lg:flex">
        {visibleServices.map((service) => (
          <span
            key={service.id}
            className="max-w-[120px] truncate rounded-xs border border-subtle px-1.5 py-0.5 text-10 text-secondary"
          >
            {service.name}
          </span>
        ))}
        {overflowServices > 0 && <span className="text-10 text-tertiary">+{overflowServices}</span>}
      </div>
      <div className="hidden w-[72px] shrink-0 items-center justify-end md:flex">
        {room.participants.length > 0 && (
          <AvatarGroup size="xs">
            {visibleParticipants.map((participant) => (
              <Avatar
                key={participant.id}
                src={participant.avatar_url ? getFileURL(participant.avatar_url) : undefined}
                alt={participant.display_name ?? ""}
                fallback={participant.display_name?.[0]?.toUpperCase()}
              />
            ))}
            {overflowParticipants > 0 && (
              <Avatar alt={`${overflowParticipants} more`} fallback={`+${overflowParticipants}`} />
            )}
          </AvatarGroup>
        )}
      </div>
      <span className="hidden w-[72px] shrink-0 text-right text-11 tabular-nums text-secondary xl:block">
        {elapsed}
      </span>
      <span className="hidden w-[48px] shrink-0 text-right text-11 text-tertiary xl:block">{room.message_count}</span>
    </div>
  );
});
```

- [ ] **Step 5: Board**

Buat `war-rooms-board.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
// local imports
import { WarRoomsBoardRow } from "./war-rooms-board-row";

export const WarRoomsBoard = observer(function WarRoomsBoard() {
  // router
  const { projectId } = useParams();
  // store hooks
  const { getProjectWarRoomIds } = useWarRoom();
  // derived values
  const roomIds = projectId ? (getProjectWarRoomIds(projectId.toString()) ?? []) : [];

  return (
    <div className="vertical-scrollbar min-h-0 flex-1">
      {roomIds.map((id) => (
        <WarRoomsBoardRow key={id} warRoomId={id} />
      ))}
    </div>
  );
});
```

- [ ] **Step 6: Header**

Buat `war-room-view-header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { CommonProjectBreadcrumbs } from "@/components/breadcrumbs/common";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CreateWarRoomModal } from "../create/create-war-room-modal";
import { WarRoomSearchInput } from "./war-room-search-input";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";

export const WarRoomViewHeader = observer(function WarRoomViewHeader() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { loader } = useProject();
  const { allowPermissions } = useUserPermissions();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // derived values
  const canCreateWarRoom = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.PROJECT
  );

  return (
    <Header>
      <Header.LeftItem>
        <div>
          <Breadcrumbs isLoading={loader === "init-loader"}>
            <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("war_room.title")}
                  href={getWarRoomLink(workspaceSlug?.toString() ?? "", projectId?.toString() ?? "")}
                  icon={<AlertOctagonOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        <div className="flex h-full items-center gap-2 self-end">
          <WarRoomSearchInput />
        </div>
        {canCreateWarRoom && (
          <Button variant="primary" onClick={() => setIsCreateModalOpen(true)} size="lg">
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("war_room.add")}</div>
          </Button>
        )}
      </Header.RightItem>
      {workspaceSlug && projectId && (
        <CreateWarRoomModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
          projectId={projectId.toString()}
        />
      )}
    </Header>
  );
});
```

- [ ] **Step 7: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: sukses.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/war-rooms/list
git commit -m "feat(web): war room list board, chips, search and header"
```

---

### Task 11: List view + route + halaman list

**Files:**

- Create: `apps/web/core/components/war-rooms/list/war-rooms-list-view.tsx`
- Create: `apps/web/core/components/war-rooms/index.ts`
- Modify: `apps/web/app/routes/core.ts:193`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/page.tsx`

- [ ] **Step 1: List view**

Buat `war-rooms-list-view.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { WAR_ROOM_STATUS_TABS, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { cn } from "@plane/utils";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import useDebounce from "@/hooks/use-debounce";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWarRoomFilter } from "@/hooks/store/use-war-room-filter";
// helpers
import { statusFilterForTab } from "@/services/war-room.helpers";
// components
import { CreateWarRoomModal } from "../create/create-war-room-modal";
import { WarRoomLoadErrorState } from "./war-room-load-error-state";
import { WarRoomSummaryChips } from "./war-room-summary-chips";
import { WarRoomsBoard } from "./war-rooms-board";

type Props = {
  prefillIssueId?: string;
};

export const WarRoomsListView = observer(function WarRoomsListView({ prefillIssueId }: Props) {
  // router
  const { workspaceSlug, projectId } = useParams();
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    fetchedMap,
    loader,
    errorMap,
    fetchWarRooms,
    fetchWarRoomSummary,
    getProjectWarRoomIds,
    getProjectSummary,
    getActiveWarRoomByIssue,
  } = useWarRoom();
  const { searchQuery, updateSearchQuery, getStatusTab, setStatusTab } = useWarRoomFilter();
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);
  // refs
  const prefillHandled = useRef(false);

  // derived values
  const workspaceSlugString = workspaceSlug?.toString();
  const projectIdString = projectId?.toString();
  const statusTab = projectIdString ? getStatusTab(projectIdString) : "active";
  const debouncedQuery = useDebounce(searchQuery, 500);
  const roomIds = projectIdString ? getProjectWarRoomIds(projectIdString) : null;
  const summary = projectIdString ? getProjectSummary(projectIdString) : null;
  const hasError = projectIdString ? errorMap[projectIdString] : false;
  const prefillIssue = prefillIssueId ? getIssueById(prefillIssueId) : undefined;

  useEffect(() => {
    if (!workspaceSlugString || !projectIdString) return;
    void fetchWarRooms(workspaceSlugString, projectIdString, {
      status: statusFilterForTab(statusTab),
      q: debouncedQuery.trim() !== "" ? debouncedQuery.trim() : undefined,
    });
  }, [workspaceSlugString, projectIdString, statusTab, debouncedQuery, fetchWarRooms]);

  useEffect(() => {
    if (!workspaceSlugString || !projectIdString) return;
    void fetchWarRoomSummary(workspaceSlugString, projectIdString);
  }, [workspaceSlugString, projectIdString, fetchWarRoomSummary]);

  // `?primary_issue_id=` entry point: open the active room if one exists, otherwise
  // open the create modal prefilled with that incident.
  useEffect(() => {
    if (!prefillIssueId || !workspaceSlugString || !projectIdString || prefillHandled.current) return;
    if (roomIds === null) return;
    prefillHandled.current = true;
    const existingRoom = getActiveWarRoomByIssue(projectIdString, prefillIssueId);
    if (existingRoom) {
      router.replace(getWarRoomLink(workspaceSlugString, projectIdString, existingRoom.id));
      return;
    }
    setIsCreateModalOpen(true);
  }, [prefillIssueId, roomIds, workspaceSlugString, projectIdString, getActiveWarRoomByIssue, router]);

  const closeCreateModal = () => {
    setIsCreateModalOpen(false);
    if (prefillIssueId && workspaceSlugString && projectIdString) {
      router.replace(getWarRoomLink(workspaceSlugString, projectIdString));
    }
  };

  const handleRetry = () => {
    if (!workspaceSlugString || !projectIdString) return;
    void fetchWarRooms(workspaceSlugString, projectIdString, {
      status: statusFilterForTab(statusTab),
      q: debouncedQuery.trim() !== "" ? debouncedQuery.trim() : undefined,
    });
  };

  const handleClearFilters = () => {
    if (!projectIdString) return;
    setStatusTab(projectIdString, "active");
    updateSearchQuery("");
  };

  const renderContent = () => {
    if (hasError) {
      return <WarRoomLoadErrorState onRetry={handleRetry} />;
    }
    if (loader || roomIds === null) {
      return <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>;
    }
    const isFiltered = statusTab !== "active" || debouncedQuery.trim() !== "";
    if (roomIds.length === 0 && isFiltered) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("war_room.empty_state.no_matches.title")}</p>
          <p className="text-xs text-secondary">{t("war_room.empty_state.no_matches.description")}</p>
          <Button variant="secondary" size="sm" onClick={handleClearFilters}>
            {t("common.clear_all")}
          </Button>
        </div>
      );
    }
    if (roomIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("war_room.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("war_room.empty_state.description")}</p>
          <Button variant="primary" size="sm" onClick={() => setIsCreateModalOpen(true)}>
            {t("war_room.add")}
          </Button>
        </div>
      );
    }
    return (
      <>
        {summary && <WarRoomSummaryChips summary={summary} />}
        <WarRoomsBoard />
      </>
    );
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center gap-1 border-b border-subtle px-3 py-1.5">
        {WAR_ROOM_STATUS_TABS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => projectIdString && setStatusTab(projectIdString, tab.key)}
            className={cn(
              "rounded-sm px-2 py-1 text-12 font-medium transition-colors",
              statusTab === tab.key ? "bg-layer-2 text-primary" : "text-secondary hover:bg-layer-1"
            )}
          >
            {t(tab.label_key)}
          </button>
        ))}
      </div>
      {renderContent()}
      {workspaceSlugString && projectIdString && (
        <CreateWarRoomModal
          isOpen={isCreateModalOpen}
          onClose={closeCreateModal}
          workspaceSlug={workspaceSlugString}
          projectId={projectIdString}
          initialIssueId={prefillIssueId}
          initialIssue={
            prefillIssue
              ? { id: prefillIssue.id, name: prefillIssue.name, priority: prefillIssue.priority ?? null }
              : undefined
          }
        />
      )}
    </div>
  );
});
```

- [ ] **Step 2: Barrel komponen**

Buat `apps/web/core/components/war-rooms/index.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./create/create-war-room-modal";
export * from "./list/war-room-load-error-state";
export * from "./list/war-room-search-input";
export * from "./list/war-room-summary-chips";
export * from "./list/war-room-view-header";
export * from "./list/war-rooms-board";
export * from "./list/war-rooms-board-row";
export * from "./list/war-rooms-list-view";
```

- [ ] **Step 3: Registrasi route list**

Di `apps/web/app/routes/core.ts`, sisipkan setelah blok "Services List" (setelah baris 193):

```ts
          // War Rooms List
          layout("./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/layout.tsx", [
            route(
              ":workspaceSlug/projects/:projectId/war-rooms",
              "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/page.tsx"
            ),
          ]),
```

Catatan: route detail didaftarkan di Task 12 (butuh file `(detail)/**` yang belum ada di task ini).

- [ ] **Step 4: Layout + halaman list**

Buat `.../war-rooms/(list)/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { WarRoomViewHeader } from "@/components/war-rooms";

export default function ProjectWarRoomsListLayout() {
  return (
    <>
      <AppHeader header={<WarRoomViewHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Buat `.../war-rooms/(list)/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WarRoomsListView } from "@/components/war-rooms";
// hooks
import { useProject } from "@/hooks/store/use-project";
import type { Route } from "./+types/page";

function ProjectWarRoomsPage({ params }: Route.ComponentProps) {
  const { projectId } = params;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  // query params (entry point from an Incident work item)
  const searchParams = useSearchParams();
  const prefillIssueId = searchParams.get("primary_issue_id") ?? undefined;
  // derived values
  const project = getProjectById(projectId);
  const pageTitle = project?.name ? `${project.name} - ${t("war_room.title")}` : undefined;

  return (
    <>
      <PageHead title={pageTitle} />
      <WarRoomsListView prefillIssueId={prefillIssueId} />
    </>
  );
}

export default observer(ProjectWarRoomsPage);
```

- [ ] **Step 5: Typecheck (route typegen)**

Run: `pnpm --filter=web check:types`
Expected: sukses; React Router typegen menghasilkan `+types` untuk route list yang baru.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/war-rooms apps/web/app/routes/core.ts "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)"
git commit -m "feat(web): war room list page and routes"
```

---

### Task 12: Room shell minimal (route detail + overview)

**Files:**

- Create: `apps/web/core/components/war-rooms/room/root.tsx`
- Create: `apps/web/core/components/war-rooms/room/room-overview.tsx`
- Create: `apps/web/core/components/war-rooms/room/detail-header.tsx`
- Modify: `apps/web/core/components/war-rooms/index.ts`
- Modify: `apps/web/app/routes/core.ts`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)/[warRoomId]/page.tsx`

- [ ] **Step 1: Root (fetch + state)**

Buat `room/root.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// components
import { WarRoomOverview } from "./room-overview";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useWarRoom } from "@/hooks/store/use-war-room";

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
  const { getWarRoomDetailById, detailErrorMap, fetchWarRoomDetail } = useWarRoom();
  // derived values
  const room = getWarRoomDetailById(warRoomId);
  const hasError = detailErrorMap[warRoomId];

  useEffect(() => {
    if (room || hasError) return;
    void fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  }, [room, hasError, workspaceSlug, projectId, warRoomId, fetchWarRoomDetail]);

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
    return <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>;
  }

  return <WarRoomOverview room={room} />;
});
```

- [ ] **Step 2: Overview read-only**

Buat `room/room-overview.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// plane imports
import { WAR_ROOM_STATUS_CONFIG, WAR_ROOM_SEVERITY_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { IWarRoom } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { formatElapsed, getWarRoomIncidentLink } from "@/services/war-room.helpers";

type Props = {
  room: IWarRoom;
};

export const WarRoomOverview = observer(function WarRoomOverview({ room }: Props) {
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const severityConfig = WAR_ROOM_SEVERITY_CONFIG[room.severity];
  const statusConfig = WAR_ROOM_STATUS_CONFIG[room.status];
  const elapsed = formatElapsed(room.started_at, room.resolved_at);

  return (
    <div className="flex h-full w-full flex-col gap-4 overflow-y-auto p-4 sm:p-6">
      <div className="rounded-lg border border-subtle bg-surface-1 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", severityConfig.pill)}>
            {t(severityConfig.label_key)}
          </span>
          <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", statusConfig.pill)}>
            {t(statusConfig.label_key)}
          </span>
          <span className="text-12 font-medium text-tertiary">WR-{room.sequence_id}</span>
          <span className="text-12 tabular-nums text-tertiary">{elapsed}</span>
        </div>
        <h2 className="mt-2 text-18 font-medium text-primary">{room.name}</h2>
        {room.primary_issue && workspaceSlug && (
          <Link
            href={getWarRoomIncidentLink(workspaceSlug.toString(), room.primary_issue.identifier)}
            className="mt-2 inline-flex items-center gap-2 rounded-xs border border-subtle px-2 py-1 text-12 text-secondary hover:text-primary"
          >
            <span className="font-medium">{room.primary_issue.identifier}</span>
            <span className="truncate">{room.primary_issue.name}</span>
          </Link>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-4 text-12 text-secondary">
          <span>
            {t("war_room.fields.affected_services")}: {room.services.length}
          </span>
          <span>
            {t("war_room.fields.participants")}: {room.participants.length}
          </span>
          <span>
            {t("war_room.fields.messages")}: {room.counts.messages}
          </span>
        </div>
      </div>
    </div>
  );
});
```

Catatan: breadcrumb di `detail-header.tsx` memakai `t("war_room.title")` sebagai link kembali ke list; key `war_room.detail.back` disiapkan untuk header Fase 4. Timer di overview bernilai statis saat render (timer ticking ada di baris list); header timer penuh menyusul di Fase 4.

- [ ] **Step 3: Detail header**

Buat `room/detail-header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { CommonProjectBreadcrumbs } from "@/components/breadcrumbs/common";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useAppRouter } from "@/hooks/use-app-router";

export const WarRoomDetailHeader = observer(function WarRoomDetailHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId, warRoomId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomDetailById } = useWarRoom();
  // derived values
  const room = warRoomId ? getWarRoomDetailById(warRoomId.toString()) : null;

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("war_room.title")}
                href={getWarRoomLink(workspaceSlug?.toString() ?? "", projectId?.toString() ?? "")}
                icon={<AlertOctagonOutline className="h-4 w-4 text-tertiary" />}
              />
            }
          />
          <Breadcrumbs.Item
            component={<BreadcrumbLink label={room ? `WR-${room.sequence_id} · ${room.name}` : ""} isLast />}
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});
```

- [ ] **Step 4: Layout + halaman detail**

Buat `.../war-rooms/(detail)/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { WarRoomDetailHeader } from "@/components/war-rooms";

export default function ProjectWarRoomDetailLayout() {
  return (
    <>
      <AppHeader header={<WarRoomDetailHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Buat `.../war-rooms/(detail)/[warRoomId]/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WarRoomRoot } from "@/components/war-rooms";
// hooks
import { useProject } from "@/hooks/store/use-project";
import type { Route } from "./+types/page";

function ProjectWarRoomDetailPage({ params }: Route.ComponentProps) {
  const { workspaceSlug, projectId, warRoomId } = params;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  // derived values
  const project = getProjectById(projectId);
  const pageTitle = project?.name ? `${project.name} - ${t("war_room.title")}` : undefined;

  return (
    <>
      <PageHead title={pageTitle} />
      <WarRoomRoot workspaceSlug={workspaceSlug} projectId={projectId} warRoomId={warRoomId} />
    </>
  );
}

export default observer(ProjectWarRoomDetailPage);
```

- [ ] **Step 5: Export room dari barrel**

Di `apps/web/core/components/war-rooms/index.ts`, tambahkan:

```ts
export * from "./room/detail-header";
export * from "./room/room-overview";
export * from "./room/root";
```

- [ ] **Step 6: Registrasi route detail**

Di `apps/web/app/routes/core.ts`, sisipkan **sebelum** blok "War Rooms List" (yang ditambahkan di Task 11):

```ts
          // War Room Detail
          layout("./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)/layout.tsx", [
            route(
              ":workspaceSlug/projects/:projectId/war-rooms/:warRoomId",
              "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)/[warRoomId]/page.tsx"
            ),
          ]),

```

- [ ] **Step 7: Typecheck penuh (route typegen)**

Run: `pnpm --filter=web check:types`
Expected: sukses, tidak ada error `+types`.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/war-rooms/room apps/web/core/components/war-rooms/index.ts apps/web/app/routes/core.ts "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(detail)"
git commit -m "feat(web): minimal war room detail shell"
```

---

### Task 13: Sidebar + tab navigation

**Files:**

- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx` (import ikon ~baris 13-21, item setelah services ~baris 122)
- Modify: `apps/web/core/components/navigation/use-navigation-items.ts` (import ~baris 6-17, item setelah services ~baris 84)
- Modify: `apps/web/core/components/navigation/tab-navigation-utils.ts` (map `getTabUrl` ~baris 66-80)

- [ ] **Step 1: Item sidebar klasik**

Di `project-navigation.tsx`:

1. Tambahkan `AlertOctagonOutline` ke import ikon:

```tsx
import {
  AlertOctagonOutline,
  CyclesOutline,
  IntakeOutline,
  ModuleOutline,
  PagesOutline,
  ServerOutline,
  ViewsOutline,
  WorkItemsOutline,
} from "@makeplane/propel/icons";
```

2. Tambahkan item tepat setelah item `services` (setelah baris 122):

```tsx
      {
        i18n_key: "sidebar.war_rooms",
        key: "war-rooms",
        name: "War rooms",
        href: `/${workspaceSlug}/projects/${projectId}/war-rooms`,
        icon: AlertOctagonOutline,
        // guest hanya baca; item tetap tampil untuk guest
        access: [EUserPermissions.ADMIN, EUserPermissions.MEMBER, EUserPermissions.GUEST],
        shouldRender: true,
        sortOrder: 4.5,
      },
```

- [ ] **Step 2: Item tab navigation**

Di `use-navigation-items.ts`:

1. Tambahkan `AlertOctagonOutline` ke import ikon:

```tsx
import {
  AlertOctagonOutline,
  CyclesOutline,
  IntakeOutline,
  ModuleOutline,
  PagesOutline,
  ServerOutline,
  ViewsOutline,
  WorkItemsOutline,
} from "@makeplane/propel/icons";
```

2. Tambahkan item yang sama tepat setelah item `services` (setelah baris 84):

```tsx
      {
        i18n_key: "sidebar.war_rooms",
        key: "war-rooms",
        name: "War rooms",
        href: `/${workspaceSlug}/projects/${projectId}/war-rooms`,
        icon: AlertOctagonOutline,
        // guest hanya baca; item tetap tampil untuk guest
        access: [EUserPermissions.ADMIN, EUserPermissions.MEMBER, EUserPermissions.GUEST],
        shouldRender: true,
        sortOrder: 4.5,
      },
```

- [ ] **Step 3: Tab URL**

Di `tab-navigation-utils.ts`, tambahkan entri ke `tabUrlMap` (setelah `services`):

```ts
    war_rooms: `${baseUrl}/war-rooms`,
```

- [ ] **Step 4: Typecheck + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: sukses.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/workspace/sidebar/project-navigation.tsx apps/web/core/components/navigation/use-navigation-items.ts apps/web/core/components/navigation/tab-navigation-utils.ts
git commit -m "feat(web): war rooms in project sidebar and tab navigation"
```

---

### Task 14: Entry point "Open war room" di work item tipe Incident

**Files:**

- Create: `apps/web/core/components/war-rooms/issue-war-room-button.tsx`
- Modify: `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx` (render tombol ~baris 149-152)
- Modify: `apps/web/core/components/war-rooms/index.ts` (export komponen baru)

- [ ] **Step 1: Komponen tombol**

Buat `apps/web/core/components/war-rooms/issue-war-room-button.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { useTranslation } from "@plane/i18n";
import { IconButton } from "@plane/propel/icon-button";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { usePlatformOS } from "@/hooks/use-platform-os";
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  projectId: string;
  issueId: string;
};

export const IssueWarRoomButton = observer(function IssueWarRoomButton({ workspaceSlug, projectId, issueId }: Props) {
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  const { isMobile } = usePlatformOS();
  // store hooks
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  const { workItemTypes, fetchWorkItemTypes } = useWorkflow();
  // derived values
  const issue = getIssueById(issueId);

  useEffect(() => {
    if (workItemTypes) return;
    void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
  }, [workItemTypes, workspaceSlug, fetchWorkItemTypes]);

  const issueType = workItemTypes?.find((type) => type.id === issue?.type_id);
  if (!issue || issue.archived_at || issueType?.name.toLowerCase() !== "incident") return <></>;

  const handleOpenWarRoom = () => {
    router.push(`/${workspaceSlug}/projects/${projectId}/war-rooms?primary_issue_id=${issueId}`);
  };

  return (
    <Tooltip label={t("war_room.open")} disabled={isMobile}>
      <IconButton variant="secondary" size="lg" onClick={handleOpenWarRoom} icon={AlertOctagonOutline} />
    </Tooltip>
  );
});
```

- [ ] **Step 2: Render di quick actions**

Di `issue-detail-quick-actions.tsx`:

1. Tambahkan import:

```tsx
import { IssueWarRoomButton } from "@/components/war-rooms/issue-war-room-button";
```

2. Sisipkan tombol sebelum `WorkItemDetailQuickActions` (setelah tombol copy link, baris 151):

```tsx
<IssueWarRoomButton workspaceSlug={workspaceSlug} projectId={projectId} issueId={issueId} />
```

- [ ] **Step 3: Export dari barrel**

Di `apps/web/core/components/war-rooms/index.ts`, tambahkan:

```ts
export * from "./issue-war-room-button";
```

- [ ] **Step 4: Typecheck + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: sukses.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/war-rooms/issue-war-room-button.tsx apps/web/core/components/war-rooms/index.ts apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx
git commit -m "feat(web): open war room entry point on incident work items"
```

---

### Task 15: Verifikasi penuh + rollout smoke

**Files:** tidak ada file baru; perbaikan format/lint bila perlu.

- [ ] **Step 1: Unit test web**

Run: `pnpm --filter=web test`
Expected: seluruh suite lulus, termasuk `war-room.helpers.test.ts` dan `war-room.store.test.ts` (0 failed).

- [ ] **Step 2: Typecheck + lint + format**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: sukses. Jika ada file baru yang belum diformat:

```bash
pnpm exec oxfmt apps/web/core/components/war-rooms apps/web/core/services/war-room.* apps/web/core/store/war-room* apps/web/core/hooks/store/use-war-room* "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms" apps/web/app/routes/core.ts packages/types/src/war-room packages/constants/src/war-room.ts
pnpm --filter=web check:format
```

- [ ] **Step 3: i18n sync check**

Run: `pnpm --filter @plane/i18n run check:sync`
Expected: exit 0, tidak ada key hilang.

- [ ] **Step 4: Build web**

Run (mengikuti AGENTS.md, env build prod):

```bash
VITE_API_BASE_URL=https://api.terraline.space pnpm --filter=web build
```

Expected: build sukses tanpa error tipe.

- [ ] **Step 5: Commit perbaikan (bila ada)**

```bash
git add apps/web packages
git commit -m "chore(web): war room list phase formatting and lint fixes"
```

- [ ] **Step 6: Rollout smoke (lokal/prod tunnel)**

1. `systemctl --user restart plane-web-prod.service` (setelah build di Step 4).
2. Buka `https://<host>/<workspaceSlug>/projects/<projectId>/war-rooms`:
   - Sidebar "War rooms" tampil (juga di mode tab), item aktif saat route dibuka.
   - Summary chips memuat angka; tab Active default; pindah Resolved/All memicu refetch; search `WR-`/nama menyaring.
   - Baris menampilkan severity rail, `WR-n`, incident chip (klik membuka work item), chips service, avatar peserta, durasi berjalan.
3. Klik **Create war room**:
   - Picker insiden single-select: klik baris langsung submit; form muncul dengan nama terisi.
   - Ubah severity/services/deskripsi, submit → redirect ke halaman room (overview menampilkan `WR-n`).
   - Buka create lagi untuk insiden yang sama → banner 409 "Active war room already exists" + tombol `WR-n` menuju room aktif.
4. Dari work item tipe **Incident**: tombol ikon "Open war room" tampil → klik membuka list dengan `?primary_issue_id=`; bila room aktif ada, langsung redirect; bila tidak, modal create terbuka dengan insiden terpilih.
5. Non-Incident work item: tombol tidak tampil.
6. Guest: tombol create tidak tampil, akses list/room read-only.

- [ ] **Step 7: Catat status fase**

Fase 3 selesai bila Step 1-4 hijau dan smoke Step 6 lolos. Lanjut menulis plan Fase 4 (Room page: header/lifecycle, graph refactor + peta blast radius, chat panel + socket hook, tab Notes/Work items/Runbook/Activity/People).

---

## Catatan self-review

- **Cakupan spec** (bagian "Frontend (web)"): routes list/detail ✅ (Task 11/12), types ✅ (Task 2), constants + `getWarRoomLink` ✅ (Task 3), service + store + hook ✅ (Task 5-7), halaman list (header/chips/tabs/search/skeleton/empty/filtered-empty) ✅ (Task 10/11), modal create + single-select picker + default services + 409 banner ✅ (Task 8/9), entry point work item ✅ (Task 14), sidebar ✅ (Task 13), i18n ✅ (Task 1). Refactor graph, peta blast radius, chat/socket, tab Notes/Work items/Runbook/Activity/People, dan notifikasi mention → Fase 4/5 sesuai urutan spec.
- **Deviasi yang disengaja**: room page masih overview read-only (diganti Fase 4); severity client-side hanya saat priority tersedia, selain itu default server; label tombol 409 `WR-n` tampil bila room ada di store (kalau tidak, teks generik); tidak ada component test (vitest node env tanpa jsdom).
- **Konsistensi tipe**: `IWarRoom` (detail) vs `IWarRoomListItem` dipisah; store memakai `warRoomService` agar test bisa inject; helper `statusFilterForTab` mengembalikan `undefined` untuk tab All; `getWarRoomLink` dipakai konsisten di komponen, modal, dan routes.
- **Tidak ada placeholder**: semua langkah berisi kode final; Task 11 mendaftarkan route list, Task 12 mendaftarkan route detail + menambah export barrel, sehingga tiap task bisa typecheck sendiri.

## Changelog

| Date       | Change                                 |
| ---------- | -------------------------------------- |
| 2026-09-30 | init — plan Fase 3 (web list + create) |
