# In-App Documentation Page — Design

Date: 2026-10-16
Status: Approved in brainstorming (2026-10-16); awaiting written-spec review.
Scope: web only (`apps/web`, `packages/constants`, `packages/i18n`). No backend, no migrations, no changes to `apps/live`.
Related: workspace sidebar registry (`packages/constants/src/workspace.ts`), navigation labels (`packages/i18n/src/locales/*/navigation.json`, `.../common.json`), help menu (`apps/web/core/components/workspace/sidebar/help-section/root.tsx`), settings section layout as the two-pane precedent (`apps/web/app/(all)/[workspaceSlug]/(settings)/layout.tsx`), route table (`apps/web/app/routes/core.ts`), product docs (`docs/PRODUCT.md`, `docs/features/`).

## Background

Fork Plane untuk ITSM (Terraline) belum punya dokumentasi in-app. Bantuan yang tersedia hanya Help menu (contact sales, keyboard shortcuts, what's new) dan tur onboarding yang berhenti setelah beberapa langkah. User baru — terutama agent helpdesk L1/L2 — perlu penjelasan: platform ini apa, tiap orang punya fitur apa, dan tiap project berisi apa. Dokumentasi repo (`docs/`) kaya tapi tidak pernah dilihat user non-dev.

## Locked decisions (brainstorming 2026-10-16)

1. **Lokasi:** in-app, level workspace, route `/:workspaceSlug/docs/:topicSlug`. Semua user login bisa akses (Admin/Member/Guest).
2. **Struktur:** multi-halaman + sidebar kiri berisi daftar topik; tiap topik punya URL shareable.
3. **Bahasa konten:** Bahasa Indonesia, hardcoded di file TSX (bukan i18n). Label navigasi (sidebar & help menu) tetap via i18n (en + id).
4. **Cakupan:** overview ("apa ini") + fitur level member + fitur level project + pengaturan project.
5. **Pendekatan A:** konten sebagai komponen TSX, registry sebagai sumber tunggal topik. Tidak ada pipeline markdown, tidak ada CMS/editor in-app.
6. **Titik akses:** item sidebar workspace "Docs" + item Help menu "Documentation". Docs tidak masuk `RESTRICTED_URLS`.
7. **Slug bahasa Inggris**, diselaraskan dengan label sidebar (Tickets, Sprints, Tracks, Services, Queues, Knowledge base, Intake, Home, Inbox, Your work, Reports, Drafts, Stickies, Scheduler).
8. **Layout:** mengikuti pola section settings — app rail tetap tampil (dari parent `[workspaceSlug]/layout.tsx`), sidebar workspace/project disembunyikan, docs punya canvas penuh dengan sidebar topik sendiri.

## Non-goals (fase ini)

- Markdown pipeline / file `.md` / CMS — konten adalah TSX.
- Pencarian dalam docs, TOC "on this page" per topik, feedback widget, analytics docs.
- I18n konten (selain label). Versi Inggris konten tidak dibuat sekarang.
- Screenshot/gambar produk; grafis dibatasi primitif CSS (kartu, callout, tabel).
- Dokumentasi admin workspace & instance (billing, ekspor, webhooks, autentikasi).
- Deep-link langsung ke menu di dalam app (mis. tombol "buka Settings → States"); cross-link hanya antar topik docs.
- Perubahan backend/API.

## Design

### 1. Routes & layout

Grup route baru `(docs)` di `apps/web/app/routes/core.ts`, di dalam layout `[workspaceSlug]`, mengikuti preseden `(settings)`:

```ts
layout("./(all)/[workspaceSlug]/(docs)/layout.tsx", [
  route(":workspaceSlug/docs", "./(all)/[workspaceSlug]/(docs)/docs/page.tsx"),
  route(":workspaceSlug/docs/:topicSlug", "./(all)/[workspaceSlug]/(docs)/docs/[topicSlug]/page.tsx"),
]),
```

| File                                                         | Tanggung jawab                                                                                                      |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| `app/(all)/[workspaceSlug]/(docs)/layout.tsx`                | Shell: `ContentWrapper` (pola sama dengan settings layout) + `DocsLayout` yang merender sidebar topik dan `Outlet`. |
| `app/(all)/[workspaceSlug]/(docs)/docs/page.tsx`             | Redirect (`<Navigate replace>`) ke `/:workspaceSlug/docs/overview`.                                                 |
| `app/(all)/[workspaceSlug]/(docs)/docs/[topicSlug]/page.tsx` | Resolve slug dari registry via `useParams`; slug tidak dikenal → redirect `overview`; render `DocsTopicPage`.       |

Layout dokumentasi tidak memuat `ProjectAppSidebar`/`ExtendedProjectSidebar` — hanya app rail bawaan parent layout, lalu dua panel:

```
┌─ app rail ─┬────────────── docs shell ──────────────┐
│            │  docs sidebar   │  konten topik        │
│            │  (grouped nav)  │  (+ prev/next)       │
└────────────┴─────────────────────────────────────────┘
```

### 2. Structure & module layout

Semua komponen baru di bawah `apps/web/core/components/docs/`:

| File                  | Tanggung jawab                                                                                                         |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `docs-layout.tsx`     | Shell dua panel; mengelola state sidebar mobile (drawer/disclosure).                                                   |
| `docs-sidebar.tsx`    | Daftar topik per grup, highlight aktif dari `useParams().topicSlug`, sticky + scroll sendiri.                          |
| `docs-topic-page.tsx` | Header topik (label grup, judul, deskripsi), konten (`Suspense`), navigasi Previous/Next.                              |
| `registry.ts`         | Sumber tunggal: grup, topik, helper resolusi.                                                                          |
| `blocks.tsx`          | Primitif konten: `DocsCallout`, `DocsStepList`, `DocsFeatureGrid`/`DocsFeatureCard`, `DocsTermTable`, `DocsTopicLink`. |
| `content/*.tsx`       | Satu file per topik (21 file), default export komponen.                                                                |

**Registry** — sumber kebenaran untuk sidebar, prev/next, dan validasi route:

```ts
type TDocTopic = {
  slug: string;
  title: string;
  description: string;
  group: TDocGroupKey;
  icon: React.ElementType;
  component: React.LazyExoticComponent<React.ComponentType>;
};

const DOC_TOPIC_GROUPS: { key; label }[]; // urutan: Memulai, Untuk Member, Untuk Project, Pengaturan
const DOC_TOPICS: TDocTopic[];             // urutan tampil & urutan prev/next
getTopicBySlug(slug): TDocTopic | undefined
getTopicNeighbors(slug): { prev?: TDocTopic; next?: TDocTopic }
```

Konten topik di-load dengan `React.lazy(() => import("./content/<slug>"))` supaya bundle awal halaman docs tetap kecil; `Suspense` fallback skeleton sederhana.

### 3. Content primitives (`blocks.tsx`)

| Primitive                             | Kegunaan                                                   |
| ------------------------------------- | ---------------------------------------------------------- |
| `DocsCallout`                         | Kotak info/tip/warning (varian + ikon).                    |
| `DocsStepList`                        | Langkah bernomor untuk alur (mis. alur intake → ticket).   |
| `DocsFeatureGrid` + `DocsFeatureCard` | Grid kartu ringkas "yang bisa kamu lakukan".               |
| `DocsTermTable`                       | Tabel dua kolom untuk pemetaan istilah ITSM ↔ nama di app. |
| `DocsTopicLink`                       | Link antar topik lewat slug (menjaga konsistensi URL).     |

Tipografi dan warna memakai token yang ada (`text-primary`, `text-secondary`, `bg-surface-1`, dst.), mengikuti gaya halaman settings. Semua primitif statis, tanpa akses store/API — konten murni presentasional.

### 4. Daftar topik

**Grup: Memulai**

| Slug       | Judul                 | Isi singkat                                                                                                                                                                                                                                                                                                                   |
| ---------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `overview` | Apa itu platform ini? | Fork Plane → platform ITSM; alur incident → problem → change → knowledge; konsep Workspace & Project; peran Admin/Member/Guest (dari `ROLE_DETAILS`); `DocsTermTable` pemetaan istilah (Ticket = work item, Sprint = cycle, Track = module, Queue = view, Knowledge base = page, Inbox = notifications, Reports = analytics). |

**Grup: Untuk Member** (level workspace)

| Slug                | Judul                       | Sumber fitur                                                       |
| ------------------- | --------------------------- | ------------------------------------------------------------------ |
| `home`              | Home                        | Home dashboard + widgets                                           |
| `inbox`             | Inbox                       | Notifikasi workspace                                               |
| `your-work`         | Your work                   | Profil: assigned/created/subscribed + activity                     |
| `reports`           | Reports                     | Analytics workspace (admin/member)                                 |
| `workspace-views`   | Workspace Views             | View lintas project (all-issues, assigned, created, subscribed)    |
| `search`            | Pencarian & command palette | Power-K (cmd+k)                                                    |
| `drafts`            | Drafts                      | Draft work item                                                    |
| `stickies`          | Stickies                    | Catatan cepat                                                      |
| `scheduler`         | Scheduler                   | AI scheduler                                                       |
| `galileo`           | Galileo                     | Asisten AI di sidebar kanan                                        |
| `personal-settings` | Profil & preferensi         | Tema, notifikasi, keamanan, API token (`settings/profile.ts` tabs) |

**Grup: Untuk Project**

| Slug             | Judul          | Sumber fitur                                                                                                             |
| ---------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `tickets`        | Tickets        | Properti (status, assignee, priority, dates, labels, estimate), komentar, attachments, links, sub-items, toggle War Room |
| `sprints`        | Sprints        | Cycles: planning, progress, burndown                                                                                     |
| `tracks`         | Tracks         | Modules: pengelompokan per fase/inisiatif                                                                                |
| `services`       | Services       | Katalog layanan, health, dependency graph                                                                                |
| `war-rooms`      | War Rooms      | Incident bridge: blast radius, chat realtime, runbook, participants                                                      |
| `queues`         | Queues         | Views: filter + layout list/kanban/calendar/spreadsheet, share                                                           |
| `knowledge-base` | Knowledge base | Pages: editor kolaboratif, sub-halaman, publish publik via space                                                         |
| `intake`         | Intake         | Service desk intake: triage, sumber intake (email/webhook), accept/decline                                               |

**Grup: Pengaturan**

| Slug               | Judul              | Sumber fitur                                                                                                                                |
| ------------------ | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `project-settings` | Pengaturan project | General, members, toggle fitur (cycles/modules/views/pages/intake), states, work item types, labels, estimates, intake sources, automations |

Struktur seragam tiap topik: paragraf pembuka → blok "Yang bisa kamu lakukan" (grid/bullets) → detail alur bila ada (`DocsStepList`) → callout tips → `DocsTopicLink` ke topik terkait (mis. `intake` → `tickets`, `services` → `war-rooms`, `knowledge-base`).

### 5. Judul dokumen & tema

Halaman topik memakai komponen `PageHead` yang sudah ada (`apps/web/core/components/core/page-title.tsx`) dengan `title = "<Judul Topik> · Docs"`. Warna mengikuti tema aktif via token CSS yang ada; tidak ada mekanisme baru.

### 6. Entry points

1. **Sidebar workspace** — item statis baru `docs` di `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS` + dimasukkan `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS` setelah `ai_scheduler` (`packages/constants/src/workspace.ts`):

   ```ts
   docs: {
     key: "docs",
     labelTranslationKey: "sidebar.docs",
     href: `/docs/`,
     access: [ADMIN, MEMBER, GUEST],
     highlight: (pathname, url) => pathname.includes(url),
   }
   ```

   Ikon sidebar: `PagesOutline` dari `@makeplane/propel/icons` (pola impor yang sama dengan `project-navigation.tsx`). Label `sidebar.docs` ditambahkan ke `navigation.json` en (`"Docs"`) dan id (`"Dokumentasi"`).

2. **Help menu** (`help-section/root.tsx`) — `CustomMenu.MenuItem` baru di atas "Keyboard shortcuts", navigasi client-side ke `/${workspaceSlug}/docs` (workspaceSlug dari `useParams`). Label `documentation` ditambahkan ke `common.json` en (`"Documentation"`) dan id (`"Dokumentasi"`).

3. **`RESTRICTED_URLS`** — `docs` tidak ditambahkan (bukan reserved workspace slug).

### 7. Error & edge handling

- Slug tidak dikenal / `topicSlug` kosong → `<Navigate replace to=".../docs/overview">`.
- Chunk konten gagal dimuat (lazy import error) → jadi error boundary router yang sudah ada; tidak ada mekanisme retry baru.
- Guest: semua topik terlihat; konten informasi, bukan operasi.
- Mobile: sidebar topik jadi disclosure/dropdown di atas konten (lebar < `md`), topik aktif terlihat di trigger.

### 8. i18n

- Konten: hardcoded Bahasa Indonesia (keputusan #3).
- Label baru: `sidebar.docs` (navigation.json) + `documentation` (common.json), ditambahkan ke `en` dan `id` sesuai praktik translate workflow.
- Judul topik & deskripsi di registry adalah string Indonesia langsung (bukan key i18n).

### 9. Testing & verification

- **Unit test** (`apps/web/core/lib/` konvensi vitest yang ada, mis. `docs-registry.test.ts`): slug unik, semua topik punya grup valid, `getTopicBySlug`, `getTopicNeighbors` benar di tepi (topik pertama tidak punya prev, terakhir tidak punya next).
- **Typecheck & lint:** `pnpm check:types`, `pnpm check:lint`.
- **Build:** `pnpm --filter=web build`.
- **Manual:** akses dari sidebar & help menu; deep-link tiap topik; prev/next dari `overview` sampai topik terakhir; slug invalid redirect ke `overview`; `/docs` tanpa slug redirect; tampilan light/dark; mobile drawer.
- **Deploy lokal:** setelah web berubah dan harus tampil di tunnel, rebuild + restart `plane-web-prod.service` (per AGENTS.md).

## Implementation order (indikatif)

1. Registry + primitives + shell layout + routes + unit test registry.
2. Entry points (sidebar item, help menu, i18n keys).
3. Konten grup Memulai + Member.
4. Konten grup Project + Pengaturan.
5. Polish responsive + verifikasi menyeluruh.
