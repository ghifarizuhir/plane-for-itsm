# Feature Specifications

Product-level spec per halaman + per konsep cross-cutting. Adaptasi dari `terra/docs/features/README.md:1` — Plane punya Work Items/Cycles/Modules/Views/Pages/Analytics (bukan incidents/changes Terra mentah); ITSM adalah overlay.

**Menjawab pertanyaan:** "Apa yang user lihat dan bisa lakukan di halaman X?"

Bukan:

- Bukan schema DB → lihat [`../design/02-data-model.md`](../design/02-data-model.md)
- Bukan API shape → lihat [`../design/03-api-contract.md`](../design/03-api-contract.md)
- Bukan pola UI generik → lihat [`../design/04-design-system.md`](../design/04-design-system.md)

---

## Organisasi

```
features/
├── README.md                           ← ini
├── _backlog.md                         ← parked ideas (belum commit phase)
├── <page>.md                           ← 1 file per halaman
└── _shared/                            ← cross-cutting, dipakai > 1 halaman
    ├── README.md
    └── <concern>.md
```

---

## Page Inventory — Plane Upstream (existing)

| Page                                           | File                                                                                   | Status      | Catatan                                                       |
| ---------------------------------------------- | -------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------- | ------- | --- |
| Work Items (Issues)                            | [`work-items.md`](./work-items.md)                                                     | ✅ Existing | Core — `apps/web/core/store/issue/*`, `plane.db.models.issue` |
| Cycles (Sprints)                               | [`cycles.md`](./cycles.md)                                                             | ✅ Existing | `plane.db.models.cycle`, burn-down                            |
| Modules                                        | [`modules.md`](./modules.md)                                                           | ✅ Existing | `plane.db.models.module`                                      |
| Views                                          | [`views.md`](./views.md)                                                               | ✅ Existing | Saved filters, `plane.db.models.view`                         |
| Pages                                          | [`pages.md`](./pages.md)                                                               | ✅ Existing | TipTap + Yjs, `apps/live`                                     |
| Analytics                                      | [`analytics.md`](./analytics.md)                                                       | ✅ Existing | `plane.analytics`                                             |
| Inbox / Notifications                          | [`inbox.md`](./inbox.md)                                                               | ✅ Existing | `plane.db.models.notification`                                |
| Intake / Drafts / Stickies / Browse / Archives | \_(todo — actual: `intake`, `drafts`, `stickies`, `browse/:workItem`, `archives/issues | cycles      | modules`)\_                                                   | 📝 Todo | —   |

| Workspace / Project Settings | [`settings.md`](./settings.md) | ✅ Existing | `apps/admin` god-mode + workspace settings |

## Page Inventory — ITSM (future, bukan actual)

Belum ada halaman ITSM di kode (tidak ada `incidents` / `configuration-items` di `apps/web/app/routes/core.ts`, tidak ada model CI di `plane/db/models/`). Semua ide ITSM (Incident, Problem+RCA, Change, Request, Knowledge, Improvement, Asset, Service Map) diparkir di [`_backlog.md`](./_backlog.md) — jangan buat `features/<page>.md` sampai implementasi dimulai.

## Shared Concerns

Lihat [`_shared/README.md`](./_shared/README.md).

| Concern                   | File                                                     | Status   | Dipakai oleh                         |
| ------------------------- | -------------------------------------------------------- | -------- | ------------------------------------ |
| List & filter/sort/export | [`_shared/list.md`](./_shared/list.md)                   | 📝 Draft | Semua list pages (Work Items + ITSM) |
| Detail page               | [`_shared/detail-page.md`](./_shared/detail-page.md)     | 📝 Draft | Issue detail + ITSM detail           |
| Routing                   | [`_shared/routing.md`](./_shared/routing.md)             | 📝 Draft | URL pattern `/:workspaceSlug/...`    |
| Comments & activity       | [`_shared/comments.md`](./_shared/comments.md)           | 📝 Draft | Comments + timeline                  |
| Global search (Cmd+K)     | [`_shared/global-search.md`](./_shared/global-search.md) | 📝 Draft | Cmd+K palette                        |

> Terra `_shared` punya 19 file (`terra/docs/features/_shared:19` — termasuk `entity-linking.md`, `versioning.md`, `reviews.md`, `rail.md`). Plane **ramping** — ITSM fork reuse `list/detail/routing/comments` saja Phase 1; `entity-linking`/`reviews`/`rail` ditambah bila ITSM butuh (jangan pre-build 19 file).

---

## Template — Page Doc

Format baku untuk setiap file di `features/*.md` (contek `terra/features/README.md:80`):

```markdown
# <Page Name>

Status: **Draft** | **Approved**
Route: `:workspaceSlug/projects/:projectId/<page>` (actual — lihat `apps/web/app/routes/core.ts`)
Share: CORE / CONTEXT / PLATFORM

## Intent

Satu-dua kalimat: tujuan halaman dari sudut user.

## Current State (snapshot kode)

- Komponen: `<Component>` di `apps/web/core/...:<range>` atau `apps/web/app/routes/...`
- Working: ...
- Stub: ...
- Missing (ITSM fork): ...

## Primary View

- Layout: table / board / grid / graph
- Data visible: kolom/field utama
- Interaction: klik row, hover, select

## Actions

| Action | Trigger     | Permission | State required |
| ------ | ----------- | ---------- | -------------- |
| Create | Toolbar / C | Member+    | —              |

...

## Filters / Sort / Search

- Filters: field, default, persist di URL `?state=&priority=&sort=`
- Sort: default column
- Search: scope (per-page atau Cmd+K)

## Detail View

- Section: metadata / description / comments / timeline / versions
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md)

## Permissions

| Role            | Create | Read | Update | Delete |
| --------------- | ------ | ---- | ------ | ------ |
| Member          | ✅     | ✅   | own    | ❌     |
| Project admin   | ✅     | ✅   | ✅     | ✅     |
| Workspace admin | ✅     | ✅   | ✅     | ✅     |

## Empty / Loading / Error

- Empty: message + CTA
- Loading: skeleton
- Error: banner + retry
```

---

## Template — Shared Doc

```markdown
# <Concern Name>

Status: **Draft** | **Approved**
Used by: daftar halaman yang pakai

## Purpose

Kenapa ini di-shared.

## Behavior

- Trigger, steps, success/failure, keyboard/a11y

## States

- Default, loading, error, empty

## Edge Cases

## API Touchpoints

Endpoint yang di-hit. Ref ke [`../design/03-api-contract.md`](../design/03-api-contract.md).
```

---

## Conventions

1. **Status eksplisit** di header setiap doc: `Draft` → `Approved`.
2. **Current State dari kode Plane** — referensi `apps/web/core/store/*` atau `plane/db/models/*` + line range.
3. **Phase fork explicit** — Phase 1 fork (Incident+Knowledge+Service Map) vs Phase 2 deferred.
4. **Cross-reference `_shared/*`** kalau dipakai > 1 halaman — jangan duplikasi.
5. **Permissions matrix** per page (inherit Plane workspace/project hierarchy).
6. **Tidak iterasi batch** — satu file per session, diskusi + tulis + approve baru pindah.

---

## Content Boundary (apa taruh di mana)

Berlaku cross-folder (`design/`, `features/`, `ui/`):

| Jenis konten              | Lokasi                            | Lifecycle                       |
| ------------------------- | --------------------------------- | ------------------------------- |
| Living spec               | Body doc yang relevan             | Permanent, update-in-place      |
| Open question aktif       | §Open Items di doc relevan        | Ephemeral — hapus saat resolved |
| Phase 2 deferred          | §Phase 2 Deferred di doc relevan  | Permanent commitment            |
| Parked idea cross-feature | [`_backlog.md`](./_backlog.md)    | Fluid, tanpa deadline           |
| Parked engineering        | `design/README.md` §Open Items    | Fluid                           |
| Review discussion         | Chat / PR                         | Ephemeral — tidak masuk docs    |
| Change history            | Git log + `## Changelog` per file | Immutable                       |

---

## Writing Order (recommended)

**Option Y — Start dari yang ada dulu (recommended untuk fork):** Work Items (`work-items.md`) → Cycles → Service Map (`services.md`) → Incidents (`incidents.md`). Validates template dengan page yang sudah ada, ITSM pakai template matang.

**Option X — Start complex first:** Incidents → Service Map → Work Items.

> Untuk fork, **jangan start dari Terra complex first** — Plane Work Items sudah kompleks; ITSM adapt polanya.

---

---

## Changelog

| Date       | Change                                                 |
| ---------- | ------------------------------------------------------ |
| 2026-09-03 | fork init — adaptasi dari terra `features/README.md:1` |
