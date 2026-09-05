# Settings (Workspace / Project / Profile / Admin)

Status: **Approved**
Route: lihat `apps/web/app/routes/core.ts:255-363` (ringkas di bawah)
Share: PLATFORM

## Intent

Pusat konfigurasi tiga level: workspace (anggota, billing, exports, webhooks), project (fitur, states, labels, estimates, automations), dan profil pribadi (general, security, preferences, notifications, api-tokens) — plus panel god-mode `apps/admin` di level instance. Dari sudut user: "atur yang perlu diatur, sesuai peranku."

## Current State (snapshot kode)

### Workspace settings (`:workspaceSlug/settings/*`, `core.ts:263-285`)

| Sub-halaman | Route                                | Komponen                                                                                                                                                 |
| ----------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| General     | `settings`                           | `settings/(workspace)/page.tsx:19-33` (`GeneralWorkspaceSettingsPage` → `WorkspaceDetails`, `workspace/settings/workspace-details.tsx` — nama/slug/icon) |
| Members     | `settings/members`                   | `members-list.tsx` + `members-list-item.tsx` + `invitations-list-item.tsx` (members + invites + roles)                                                   |
| Billing     | `settings/billing`                   | `billing/page.tsx:20-28` (`BillingRoot`, guard `EUserPermissions.ADMIN` else `NotAuthorizedView`)                                                        |
| Exports     | `settings/exports`                   | `exporter/export-form.tsx` + `prev-exports.tsx` + `single-export.tsx`                                                                                    |
| Webhooks    | `settings/webhooks` (+ `:webhookId`) | `web-hooks/webhooks-list.tsx` + `create-webhook-modal.tsx` + `delete-webhook-modal.tsx`                                                                  |

Catatan: `settings/(workspace)/integrations/page.tsx` ada di disk tapi **tidak terdaftar** di `core.ts:263-285` (dead route).

### Project settings (`:workspaceSlug/settings/projects/*`, `core.ts:291-349`)

| Sub-halaman | Route                                              | Komponen                                                                                                                                                                      |
| ----------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| List        | `settings/projects`                                | —                                                                                                                                                                             |
| General     | `settings/projects/:projectId`                     | `[projectId]/page.tsx:39-48` (`ProjectDetailsForm` + `GeneralProjectSettingsControlSection`, guard admin `useUserPermissions:30`)                                             |
| Members     | `.../members`                                      | —                                                                                                                                                                             |
| Features    | `.../features/{cycles,modules,views,pages,intake}` | `project/settings/features-list.tsx:30-75` — flag `cycle_view`, `module_view`, `issue_views_view`, `page_view`, `inbox_view` (key `intake`); toggle via `updateProject:86-93` |
| States      | `.../states`                                       | `project-states/state-list.tsx` + `state-item.tsx` + `state-delete-modal.tsx` (CRUD state + groups `backlog/unstarted/started/completed/cancelled` via `state.group`)         |
| Labels      | `.../labels`                                       | `labels/project-setting-label-list.tsx` + `project-setting-label-item.tsx` + `create-update-label-inline.tsx` + `label-drag-n-drop-HOC.tsx` (CRUD + `color`)                  |
| Estimates   | `.../estimates`                                    | `estimates/estimate-list.tsx` + `estimate-list-item.tsx` + `estimate-disable-switch.tsx`; store `store/estimates/project-estimate.store.ts` + `estimate-point.ts`             |
| Automations | `.../automations`                                  | `page.tsx` + `automation/auto-close-automation.tsx` + `auto-archive-automation.tsx` (auto-close / auto-archive)                                                               |

Redirect legacy: `:workspaceSlug/projects/:projectId/settings/*` → path baru (`core.ts:376`).

### Feature flags (persimpangan semua feature docs)

`features-list.tsx:30-75` adalah sumber tunggal flag yang di-gate di tiap halaman: `cycle_view` (cycles), `module_view` (modules), `issue_views_view` (views), `page_view` (pages), `inbox_view` (intake, key `intake`). Pola gate: empty-state + link ke features.

### Profile settings (`settings/profile/:profileTabId`, `core.ts:360-362`)

- Layout: `layout.tsx:13-28` (`AuthenticationWrapper`); page `:32-56` validasi `PROFILE_SETTINGS_TABS`, split `ProfileSettingsSidebarRoot` + `ProfileSettingsContent`.
- Tab: `packages/constants/src/settings/profile.ts:32-52` — `general`, `security`, `preferences`, `notifications`, `api-tokens`; grup `:60-67` (YOUR_PROFILE vs DEVELOPER).
- API tokens: `settings/profile/content/pages/api-tokens.tsx:25-29` (list via `APITokenService.list()` + `CreateApiTokenModal`); redirect lama `:workspaceSlug/settings/api-tokens` → `/settings/profile/api-tokens` (`core.ts:381-383`).
- Notifications: `.../notifications/email-notification-form.tsx` (lihat [`inbox.md`](./inbox.md)).

### Admin god-mode (`apps/admin`, port 3001)

- Routes: `apps/admin/app/routes.ts:11-24` — `general`, `workspace`, `workspace/create`, `email`, `authentication{,/github,/gitlab,/google,/gitea}`, `ai`, `image`; catch-all `*:26` → `components/404.tsx`.
- Level instance (bukan workspace): kelola workspace, email/instance, auth provider, AI/image config.

### Working / Stub / Missing

- Working: semua sub-halaman di atas + guards admin + redirects legacy.
- Stub: route `integrations` mati (tidak terdaftar).
- Missing (ITSM fork): tidak ada — tidak ada settings ITSM (SLA policies, on-call, CMDB) di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Model Django + Store FE (peta)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- Model: `workspace.py:198` (`WorkspaceMember`), `project.py:210` (`ProjectMember`), `state.py:79` (`State`), `label.py:11` (`Label`), `estimate.py:18` (`Estimate`), `webhook.py:34` (`Webhook`).
- Store FE: `store/member/workspace/workspace-member.store.ts`, `member/project/base-project-member.store.ts`, `state.store.ts`, `label.store.ts`, `estimates/project-estimate.store.ts`, `workspace/webhook.store.ts`, `user/settings.store.ts` + `profile.store.ts`.

## Primary View

- Layout: settings shell (sidebar sub-halaman + content form/list) per level.
- Data visible: form general, tabel members/labels/states/estimates/webhooks/tokens, toggles features, aturan automations.
- Interaction: edit inline / modal CRUD, toggle, guard `NotAuthorizedView` bila bukan admin.
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action            | Trigger            | Permission      | State required |
| ----------------- | ------------------ | --------------- | -------------- |
| Edit general      | Form               | Admin           | —              |
| Invite member     | Members list       | Admin           | —              |
| Ubah role         | Member item        | Admin           | —              |
| Toggle feature    | Features list      | Project admin+  | —              |
| CRUD states       | State list + modal | Project admin+  | —              |
| CRUD labels       | Label list inline  | Project admin+  | —              |
| Config estimates  | Estimate list      | Project admin+  | —              |
| Toggle automation | Auto-close/archive | Project admin+  | —              |
| CRUD webhooks     | Webhooks + modal   | Workspace admin | —              |
| Export data       | Export form        | Workspace admin | —              |
| Create API token  | Modal              | Self            | —              |
| Email prefs       | Notifications form | Self            | —              |

## Filters / Sort / Search

- Filters: per tabel (members by role, labels/states by group).
- Sort: manual (labels drag-n-drop) + default.
- Search: members search; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Webhook `:webhookId` punya detail; sisanya list/form tanpa detail page.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md).

## Permissions

| Role            | Workspace settings | Project settings | Profile (self) | Admin app |
| --------------- | ------------------ | ---------------- | -------------- | --------- |
| Member          | ❌                 | ❌               | ✅             | ❌        |
| Project admin   | ❌                 | ✅ (own project) | ✅             | ❌        |
| Workspace admin | ✅                 | ✅               | ✅             | ❌        |
| Instance admin  | ✅                 | ✅               | ✅             | ✅        |

Billing + beberapa aksi workspace guard `EUserPermissions.ADMIN` eksplisit.

## Empty / Loading / Error

- Empty: list kosong → message + CTA (invite/label/state pertama).
- Loading: skeleton per section.
- Error/unauthorized: `NotAuthorizedView` + banner + retry.

---

## Changelog

| Date       | Change                                                                                |
| ---------- | ------------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts:255-363`, `features-list.tsx`, `routes.ts` admin |
