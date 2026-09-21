# ITSM Copy Kit — Wave 2 (Seed Demo, AI, Penghapusan Surface Jualan) — Design

Tanggal: 2026-09-22

## Context

Wave 1 (spec: `docs/superpowers/specs/2026-09-21-itsm-copy-kit-wave1-design.md`) selesai dan live di tunnel: empty state, onboarding/tour, metadata & branding. Voice principles dan term map wave 1 tetap berlaku (operator-first, dual-vocabulary, honest by architecture, label fitur delivery dipertahankan).

Wave 2 dibentuk dengan prinsip **dampak user saja** — hanya surface yang dilihat user:

1. **Seed demo** — di-seed otomatis untuk setiap workspace baru (`workspace_seed.delay`, `apps/api/plane/app/views/workspace/base.py:137`); first impression. Isinya sekarang tutorial PM ("Terraline Demo Project", 7 work item tutorial, 2 page).
2. **AI assistant** — copy assistant sidebar + editor AI; sudah setengah ITSM (`AI_ASSISTANT_TASK` di `apps/web/core/lib/ai-context.ts` menyebut "ITSM work-item assistant") tapi ada inkonsistensi branding.
3. **Penghapusan surface jualan** — keputusan produk: platform tidak dijual, jadi billing/upsell dihapus, bukan ditulis ulang.

Email templates, label analitik/integrasi, 19 locale, dan docs/README tetap ditunda (lihat Out of scope).

## Keputusan yang sudah diambil

1. **Seed rewrite penuh ke skenario ITSM** — struktur JSON, ID, mapping (`labels`, `cycle_id`, `module_ids`) dan loader Python tidak berubah; hanya value teks/HTML yang ditulis ulang.
2. **Project demo bernama "Terraline Service Management"** (identifier `PDP` → `TSM`) — dipilih "service management" alih-alih "service desk" karena lebih luas dan sejalan dengan positioning platform.
3. **Nama asisten diseragamkan ke "Galileo"** — konsisten dengan copy wave 1 yang sudah live; "Pi" (2 tempat) di-rename.
4. **Surface jualan dihapus semua** (bukan disembunyikan): billing, license/upgrade modal, plan constants, badge "Pro", upsell active-cycles, banner bulk-ops, dan dead code yang menempel.
5. **Backend license app tetap** (`apps/api/plane/license/**`) — infrastruktur instance/edition, bukan surface jualan web.
6. **Locale `en` saja** — key yang dihapus dari `en` menjadi _stale_ di 19 locale lain sampai wave translate (perlakuan yang sama dengan wave 1; `check:sync` hanya gagal pada _missing_, bukan _stale_).

## Voice principles (inherit wave 1)

Operator-first, operational verbs (run, resolve, triage, maintain, document, track), honest by architecture, dual-vocabulary, ringkas/sentence case. Contoh dunia memakai konteks layanan (identity provider, payroll service, HR portal), bukan product/agile.

---

## A. Seed demo → skenario ITSM

File: `apps/api/plane/seeds/data/*.json` (8 file). Tidak ada perubahan schema, ID, atau loader (`apps/api/plane/bgtasks/workspace_seed_task.py`, `apps/api/plane/db/management/commands/create_dummy_data.py`).

### A1. Project, Cycle, Module, Label, View

| Entitas      | Sebelum                                                                                            | Sesudah                                                                                                                                                                                                                                                                                                                          |
| ------------ | -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Project name | Terraline Demo Project                                                                             | Terraline Service Management                                                                                                                                                                                                                                                                                                     |
| Identifier   | PDP                                                                                                | TSM                                                                                                                                                                                                                                                                                                                              |
| Description  | "...work management software... startup hungry to scale or an enterprise sharpening efficiency..." | "Welcome to Terraline. This demo project shows how a team runs services — the services you support, the requests and incidents that come in, and the knowledge your team works from. Every card here is a work item: read them in order or jump to what you need. When you're ready, create your own project and make it yours." |
| Cover image  | Unsplash existing                                                                                  | Dipertahankan                                                                                                                                                                                                                                                                                                                    |
| Cycle 1      | Cycle 1: Getting Started with Terraline                                                            | Week 1: Set up your first service                                                                                                                                                                                                                                                                                                |
| Cycle 2      | Cycle 2: Collaboration & Customization                                                             | Week 2: Triage and resolve                                                                                                                                                                                                                                                                                                       |
| Module 1     | Core Workflow (System)                                                                             | Service Catalog (System) — "The services your team supports and the work that keeps them healthy."                                                                                                                                                                                                                               |
| Module 2     | Onboarding Flow (Feature)                                                                          | Request Fulfilment (Process) — "Intake, triage, and assignment for incoming service requests."                                                                                                                                                                                                                                   |
| Module 3     | Workspace Setup (Area)                                                                             | Knowledge Base (Area) — "Runbooks, SOPs, and postmortems your team works from."                                                                                                                                                                                                                                                  |
| Label 1      | admin (#0693e3)                                                                                    | incident (#EF4444)                                                                                                                                                                                                                                                                                                               |
| Label 2      | concepts (#9900ef)                                                                                 | service-request (#0693e3)                                                                                                                                                                                                                                                                                                        |
| View         | Project Urgent Tasks                                                                               | Urgent requests — "Urgent priority work across this service." (filter `priority__in: urgent` dipertahankan)                                                                                                                                                                                                                      |

Catatan: label 1 tidak dipakai work item mana pun, label 2 dipakai 3 work item tutorial — assignment dipertahankan apa adanya.

### A2. Work items (7, jumlah & urutan tetap)

Semua `description_html` (2–4 KB per item) ditulis ulang melalui lensa layanan; tag/tabel HTML dan gaya emoji judul dipertahankan.

| #   | Judul sebelum                      | Judul sesudah                           | Yang diajarkan                                                                          |
| --- | ---------------------------------- | --------------------------------------- | --------------------------------------------------------------------------------------- |
| 1   | Welcome to Terraline 👋            | Welcome to Terraline 👋                 | Apa isi demo ini; setiap kartu adalah work item; cara membacanya                        |
| 2   | 1. Create Projects 🎯              | 1. Create a project for your service 🎯 | Project sebagai rumah untuk sebuah layanan; langkah membuat project                     |
| 3   | 2. Invite your team 🤜🤛           | 2. Invite your team 🤜🤛                | Member, role, dan level akses                                                           |
| 4   | 3. Create and assign Work Items ✏️ | 3. Log requests and incidents ✏️        | Work item sebagai request/incident/task; properti (priority, assignee, label) dan state |
| 5   | 4. Visualize your work 🔮          | 4. Visualize your work 🔮               | Layout (list, kanban, calendar, spreadsheet, gantt), filter, dan view                   |
| 6   | 5. Use Cycles to time box tasks 🗓️ | 5. Timebox work with cycles 🗓️          | Cycle sebagai window mingguan/maintenance; Module untuk effort panjang                  |
| 7   | 6. Customize your settings ⚙️      | 6. Customize your settings ⚙️           | State, label, estimate, dan feature project                                             |

State (`Backlog/Todo/In Progress/Done/Cancelled`), `cycle_id`, dan `module_ids` per item tidak berubah.

### A3. Pages (2)

| Page | Sebelum                | Sesudah                        | Outline konten                                                                                                                                                                           |
| ---- | ---------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | Project Design Spec    | Service Runbook                | Ringkasan layanan; tabel metadata (nama layanan, owner, status, on-call, dependensi, module/cycle terkait); checklist kesehatan layanan; jalur eskalasi; ajakan menyesuaikan isi halaman |
| 2    | Project Draft proposal | Incident Postmortem (Template) | Ringkasan insiden; dampak; timeline (tabel); root cause; apa yang berjalan baik; action item + owner (tabel)                                                                             |

Field yang dibaca loader (`name`, `description_html`, `type`, `access`, `logo_props`) dipertahankan; `type: PROJECT` dan `access` tidak berubah.

### A4. Verifikasi seed

1. Semua JSON valid (`node -e`/`python -m json.tool`).
2. Jalankan seed di backend lokal (`python manage.py create_dummy_data` atau buat workspace baru) dan inspeksi hasilnya.
3. Tidak ada perubahan pada `workspace_seed_task.py` dan test backend yang ada.

---

## B. AI assistant → Galileo

Prompt (`apps/web/core/lib/ai-context.ts`) sudah ITSM; tidak diubah. Hanya copy user-visible:

| File                                                             | Sebelum                                                                                                  | Sesudah                                                                                                                                                                                            |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `packages/constants/src/ai.ts` (`LOADING_TEXTS`)                 | Pi is generating response                                                                                | Galileo is generating response                                                                                                                                                                     |
| `apps/web/core/components/pages/editor/ai/menu.tsx`              | Pi is writing                                                                                            | Galileo is writing                                                                                                                                                                                 |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`         | AI Assistant (header)                                                                                    | Galileo                                                                                                                                                                                            |
|                                                                  | Suggest acceptance criteria for this work item                                                           | Suggest resolution steps for this work item                                                                                                                                                        |
|                                                                  | No issue in view — general answers                                                                       | No work item in view — general answers                                                                                                                                                             |
|                                                                  | Summaries, descriptions, comment drafts — grounded in the issue on screen.                               | Summaries, descriptions, comment drafts — grounded in the work item on screen.                                                                                                                     |
| `apps/web/core/components/core/modals/gpt-assistant-popover.tsx` | "Please enter some task to get AI assistance.", "Tell AI what action to perform on this content...", dll | Polish ringkas dengan voice wave 1; makna tidak berubah                                                                                                                                            |
|                                                                  | "You have reached the maximum number of requests of 50 requests per month per user."                     | Diganti fallback generik ("Something went wrong. Please try again.") — backend `GPTIntegrationEndpoint`/`WorkspaceGPTIntegrationEndpoint` tidak menegakkan kuota 50/bulan; pesan lama tidak akurat |

Chip "Summarize this work item in 3 bullets" dan "Draft a status comment for this work item" sudah selaras, tidak diubah.

---

## C. Penghapusan surface jualan

### C1. Yang dihapus (hidup)

| Surface                      | Titik pemakaian                                                                                                                                                                                                                                                                                                                                                                                                   | Aksi                                             |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| Billing & Plans settings     | `packages/constants/src/settings/workspace.ts:44-49,75` (`WORKSPACE_SETTINGS`, `GROUPED_WORKSPACE_SETTINGS`), `packages/types/src/settings.ts:13` (`TWorkspaceSettingsTabs`), `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx:23`, route `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/billing/`, `apps/web/core/components/workspace/billing/**` (7 file, ~1.720 baris) | Hapus semua                                      |
| i18n block                   | `packages/i18n/src/locales/en/workspace-settings.json` (`workspace_settings.settings.billing_and_plans.*`)                                                                                                                                                                                                                                                                                                        | Hapus dari `en`                                  |
| License/upgrade modal        | `apps/web/core/components/license/**` (10 file) — satu-satunya konsumen adalah billing                                                                                                                                                                                                                                                                                                                            | Hapus                                            |
| Plan constants               | `packages/constants/src/payment.ts`, `packages/constants/src/subscription.ts`, `packages/types/src/payment.ts`, ekspor di `packages/constants/src/index.ts:32,42` dan `packages/types/src/index.ts:40`                                                                                                                                                                                                            | Hapus (tidak ada konsumen lain)                  |
| UpgradeBadge                 | `apps/web/core/components/workspace/upgrade-badge.tsx`; pemakaian: `sidebar/extended-sidebar-item.tsx:199-203`, `project/settings/features-list.tsx:122-126` (`isPro` semua `false` → field dihapus), `estimates/create/stage-one.tsx:53-56` (branch unreachable — `if (!isEnabled) return null`), `active-cycles/header.tsx:32` (ikut terhapus)                                                                  | Hapus komponen, key `sidebar.pro`, dan pemakaian |
| Upsell active-cycles         | route `apps/web/app/(all)/[workspaceSlug]/(projects)/active-cycles/**`, `apps/web/core/components/active-cycles/**`, method `workspaceActiveCycles` di `apps/web/core/services/cycle.service.ts:64` (tak terpakai), `MARKETING_PRICING_PAGE_LINK` di `packages/constants/src/endpoints.ts:31`                                                                                                                     | Hapus                                            |
| Sidebar workspace menu yatim | `sidebar/workspace-menu.tsx`, `workspace-menu-item.tsx`, `workspace-menu-header.tsx` — tak pernah dirender; menempelkan badge "Pro" ke semua item dan memuat link active-cycles                                                                                                                                                                                                                                   | Hapus                                            |
| Banner upsell bulk-ops       | `issues/bulk-operations/upgrade-banner.tsx`, `issues/bulk-operations/root.tsx`; pemakaian di `issue-layouts/list/default.tsx:30,175` dan `issue-layouts/spreadsheet/spreadsheet-view.tsx:16,122`                                                                                                                                                                                                                  | Hapus                                            |
| Key i18n sisa                | `packages/i18n/src/locales/en/navigation.json` (`pro`), `common.json` (`active_cycles`, `active_cycles_description`), `empty-state.json` (`workspace_empty_state.active_cycles` — tak terpakai)                                                                                                                                                                                                                   | Hapus dari `en`                                  |

### C2. Yang tetap

- Backend `apps/api/plane/license/**` dan model instance/edition — infrastruktur, bukan surface jualan.
- Infrastruktur multiple-select/bulk-ops (`use-bulk-operation-status.ts` yang hardcoded `false`, `MultipleSelectGroup`, dsb.) — di luar scope; hanya banner upsell yang dihapus.
- Mekanisme gating fitur — tidak ada fitur yang dibuka/dikunci ulang; hanya UI upsell yang hilang.
- `apps/space` (field `billing_address*` di profile store) — bukan copy user-visible.

### C3. Konsekuensi

- `/settings/billing` dan `/active-cycles` menjadi 404; tidak ada link tersisa yang mengarah ke sana.
- `TWorkspaceSettingsTabs` berubah → error kompilasi akan menangkap referensi yang tertinggal (power-k workspace settings menu memakai `WORKSPACE_SETTINGS_ICONS` yang di-key oleh union ini).
- Key yang dihapus dari `en` menjadi stale di 19 locale (diterima; wave translate akan membersihkan).

---

## Out of scope (wave 3+)

- Email templates: `apps/api/templates/emails/**`.
- Label analitik & integrasi: burndown/burnup, estimate systems (Fibonacci/T-shirt), Gantt.
- 19 locale (id, ja, ka-ge, …) via skill `translate`, termasuk membersihkan stale key.
- Sisa README + `docs/` internal.
- Backend license app dan detail komersial non-user-facing.
- Infrastruktur dead code multiple-select/bulk-ops.

## Verification

1. Audit teks: `rg` untuk `Upgrade|Talk to Sales|pricing|billing|subscription|payment|Pro` pada string user-facing di `apps/web` — sisa hit hanya yang sah (mis. versi aplikasi).
2. `pnpm check:lint` dan `pnpm --filter=web check:types`.
3. `pnpm --filter=@plane/i18n check:types` (key generated) — `check:sync` tetap gagal karena _missing_ pra-existing; wave 2 tidak boleh menambah _missing_ baru.
4. Validasi JSON seed + jalankan seed di backend lokal; inspeksi project, work item, page, view.
5. `pnpm --filter=web build` lalu `systemctl --user restart plane-web-prod.service`; inspeksi visual: settings sidebar tanpa Billing, sidebar tanpa badge "Pro", `/active-cycles` hilang dari UI, editor AI + panel Galileo, workspace baru menampilkan demo Service Management.

## Risks

- Penghapusan menyentuh file konstanta/type bersama (`packages/constants`, `packages/types`) — mitigasi: typecheck menangkap referensi tersisa.
- Rewrite seed adalah tugas konten terbesar — mitigasi: schema/ID utuh, verifikasi JSON + seed nyata.
- 404 pada dua path lama — diterima; tidak ada link tersisa.
