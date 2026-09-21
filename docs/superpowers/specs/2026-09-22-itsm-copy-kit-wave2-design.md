# ITSM Copy Kit — Wave 2 (Seed Demo api-rs, AI, Penghapusan Surface Jualan) — Design

Tanggal: 2026-09-22

## Context

Wave 1 (spec: `docs/superpowers/specs/2026-09-21-itsm-copy-kit-wave1-design.md`) selesai dan live di tunnel: empty state, onboarding/tour, metadata & branding. Voice principles dan term map wave 1 tetap berlaku (operator-first, dual-vocabulary, honest by architecture, label fitur delivery dipertahankan).

Wave 2 dibentuk dengan prinsip **dampak user saja**:

1. **Seed demo** — first impression setiap workspace baru. Temuan: backend produksi **fully Rust** (`apps/api-rs`: api/worker/beat lewat `docker-compose-local.yml`; Django tidak jalan). Seed hanya ada di Django (`workspace_seed_task.py`) dan **di-skip oleh Rust** (`apps/api-rs/crates/api/src/routes/workspace.rs:232` — "`workspace_seed` celery skipped"); DB saat ini 0 project demo. Bagian A karena itu mencakup **port seeding ke api-rs** sekaligus rewrite konten ke ITSM.
2. **AI assistant** — copy assistant sidebar + editor AI; prompt sudah ITSM (`AI_ASSISTANT_TASK` di `apps/web/core/lib/ai-context.ts`), endpoint di Rust (`apps/api-rs/crates/api/src/routes/ai.rs`), tapi ada inkonsistensi branding "Pi" vs "Galileo" dan satu pesan error frontend yang tidak akurat.
3. **Penghapusan surface jualan** — keputusan produk: platform tidak dijual. Surface jualan semuanya di frontend; endpoint license/instance di api-rs (`instance.rs`, `instance_admin.rs`) adalah infrastruktur admin, tetap.

Email templates, label analitik/integrasi, 19 locale, dan docs/README tetap ditunda (lihat Out of scope).

## Keputusan yang sudah diambil

1. **Seed: port ke api-rs + rewrite konten ITSM.** Port mengikuti `workspace_seed_task.py` langkah demi langkah; konten JSON ditulis ulang ke skenario layanan.
2. **Eksekusi sinkron setelah commit** di handler `POST /api/workspaces/`: demo langsung ada saat workspace dibuat; gagal seed di-log dan workspace tetap berdiri (tidak rollback). Tidak ada plumbing job baru.
3. **Data seed kanonik pindah ke api-rs**: `apps/api-rs/crates/api/assets/seeds/data/*.json`, dimuat compile-time via `include_str!` (build context Docker api-rs hanya `./apps/api-rs`, jadi file Django tidak terjangkau). Django `SEED_DIR` (`apps/api/plane/settings/common.py:559`) di-repoint ke lokasi baru supaya `create_dummy_data` tetap jalan di checkout.
4. **Project demo mengikuti nama workspace** (parity Django): nama = `workspace.name`, identifier = `alnum(workspace.name)[:5]`; field `name`/`identifier` di `projects.json` dihapus sebagai field mati. Usulan nama "Terraline Service Management" tidak dipakai karena setiap workspace memakai namanya sendiri.
5. **Nama asisten diseragamkan ke "Galileo"** — konsisten dengan copy wave 1 yang sudah live; "Pi" (2 tempat) di-rename.
6. **Surface jualan dihapus semua** (bukan disembunyikan): billing, license/upgrade modal, plan constants, badge "Pro", upsell active-cycles, banner bulk-ops, dead code yang menempel.
7. **Locale `en` saja** — key yang dihapus dari `en` menjadi _stale_ di 19 locale lain sampai wave translate (perlakuan yang sama dengan wave 1; `check:sync` hanya gagal pada _missing_, bukan _stale_).

## Voice principles (inherit wave 1)

Operator-first, operational verbs (run, resolve, triage, maintain, document, track), honest by architecture, dual-vocabulary, ringkas/sentence case. Contoh dunia memakai konteks layanan (identity provider, payroll service, HR portal), bukan product/agile.

---

## A. Seed demo → port api-rs + konten ITSM

### A0. Desain port

Lokasi & pemuatan:

- JSON kanonik: `apps/api-rs/crates/api/assets/seeds/data/*.json` (8 file), dibaca compile-time dengan `include_str!` dari modul seed baru `apps/api-rs/crates/api/src/seed.rs`.
- Django `SEED_DIR` di-repoint ke `apps/api-rs/crates/api/assets/seeds`; `workspace_seed_task.py` (legacy, tidak dijalankan) dibiarkan utuh sebagai referensi parity.

Urutan seeding (mirror `workspace_seed_task.py`):

1. **Bot user** — `is_bot=true`, `bot_type=WORKSPACE_SEED`, `display_name/first_name="Terraline"`, email `bot_user_{workspace_id}@{host WEB_URL}`, password acak ter-hash; ditambahkan sebagai `workspace_members` role 20.
2. **Project** — nama = nama workspace, identifier turunan, `description`/`network`/`cover_image`/`logo_props` dari JSON, `cycle_view=true`, `module_view=true`, `issue_views_view=true`, created/updated by bot.
3. **Project members + user properties** — `project_members` untuk **semua** workspace member (termasuk bot), plus `project_user_properties` per member dengan `display_filters`/`display_properties` persis Django.
4. **States → Labels → Cycles → Modules** — cycles: `CURRENT` = now..+14 hari, `UPCOMING` = mulai setelah cycle terakhir; modules: start = now + index×2 hari, target = +14 hari.
5. **Issues** — `issues` + `issue_sequences` + `issue_activities` (verb `created`, comment "created the issue", actor bot) + `issue_labels` + `cycle_issues` + `module_issues`, mengikuti mapping `labels`/`cycle_id`/`module_ids` di JSON.
6. **Views** — `issue_views` dari `views.json`.
7. **Pages** — `pages` + `project_pages` untuk page `type=PROJECT`.

Invocation:

- Dipanggil di `routes/workspace.rs::create` **setelah transaksi create commit**; error di-log (`tracing::warn/error`) dan tidak menggagalkan response 201.
- SQL menyusun ulang pola yang sudah ada di api-rs (`project.rs` create + default states, `draft.rs` issue+sequence+labels+cycle/module links, `label.rs`, `view.rs`, `cycle.rs`, `module.rs`) — tidak ada perubahan skema/migrasi.
- `parity-inventory.json` entri `POST /api/workspaces/` diperbarui: seed tidak lagi skipped.

Non-goals port: tidak retroaktif untuk workspace lama; tidak ada background job; tidak mengubah route/handler lain.

### A1. Konten: Cycle, Module, Label, View

| Entitas                     | Sebelum                                                                                            | Sesudah                                                                                                                                                                                                                                                                                                                          |
| --------------------------- | -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Description project         | "...work management software... startup hungry to scale or an enterprise sharpening efficiency..." | "Welcome to Terraline. This demo project shows how a team runs services — the services you support, the requests and incidents that come in, and the knowledge your team works from. Every card here is a work item: read them in order or jump to what you need. When you're ready, create your own project and make it yours." |
| `name`/`identifier` project | Terraline Demo Project / PDP                                                                       | Dihapus dari JSON (dipakai nama workspace, parity Django)                                                                                                                                                                                                                                                                        |
| Cover image                 | Unsplash existing                                                                                  | Dipertahankan                                                                                                                                                                                                                                                                                                                    |
| Cycle 1                     | Cycle 1: Getting Started with Terraline                                                            | Week 1: Set up your first service                                                                                                                                                                                                                                                                                                |
| Cycle 2                     | Cycle 2: Collaboration & Customization                                                             | Week 2: Triage and resolve                                                                                                                                                                                                                                                                                                       |
| Module 1                    | Core Workflow (System)                                                                             | Service Catalog (System) — "The services your team supports and the work that keeps them healthy."                                                                                                                                                                                                                               |
| Module 2                    | Onboarding Flow (Feature)                                                                          | Request Fulfilment (Process) — "Intake, triage, and assignment for incoming service requests."                                                                                                                                                                                                                                   |
| Module 3                    | Workspace Setup (Area)                                                                             | Knowledge Base (Area) — "Runbooks, SOPs, and postmortems your team works from."                                                                                                                                                                                                                                                  |
| Label 1                     | admin (#0693e3)                                                                                    | incident (#EF4444)                                                                                                                                                                                                                                                                                                               |
| Label 2                     | concepts (#9900ef)                                                                                 | service-request (#0693e3)                                                                                                                                                                                                                                                                                                        |
| View                        | Project Urgent Tasks                                                                               | Urgent requests — "Urgent priority work across this service." (filter `priority__in: urgent` dipertahankan)                                                                                                                                                                                                                      |

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

1. Unit test: parsing & mapping JSON (identifier turunan, tanggal cycle/module relatif, mapping label/cycle/module).
2. Integration test (`#[tokio::test]`, butuh `DATABASE_URL` — lokal `postgres://plane:plane@localhost:5432/plane`, port 5432 ter-mapping): buat workspace lewat handler, lalu assert baris: project (nama = workspace), 5 states, 2 labels, 2 cycles, 3 modules, 7 issues + `issue_sequences` + `issue_activities` + relasi label/cycle/module, 1 view, 2 pages + `project_pages`, bot user + keanggotaan.
3. Manual: buat workspace baru di UI, demo muncul langsung tanpa refresh; cek isi project, work item, page, view.

---

## B. AI assistant → Galileo

Prompt (`apps/web/core/lib/ai-context.ts`) sudah ITSM; tidak diubah. Hanya copy user-visible:

| File                                                             | Sebelum                                                                                                  | Sesudah                                                                                                                      |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `packages/constants/src/ai.ts` (`LOADING_TEXTS`)                 | Pi is generating response                                                                                | Galileo is generating response                                                                                               |
| `apps/web/core/components/pages/editor/ai/menu.tsx`              | Pi is writing                                                                                            | Galileo is writing                                                                                                           |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`         | AI Assistant (header)                                                                                    | Galileo                                                                                                                      |
|                                                                  | Suggest acceptance criteria for this work item                                                           | Suggest resolution steps for this work item                                                                                  |
|                                                                  | No issue in view — general answers                                                                       | No work item in view — general answers                                                                                       |
|                                                                  | Summaries, descriptions, comment drafts — grounded in the issue on screen.                               | Summaries, descriptions, comment drafts — grounded in the work item on screen.                                               |
| `apps/web/core/components/core/modals/gpt-assistant-popover.tsx` | "Please enter some task to get AI assistance.", "Tell AI what action to perform on this content...", dll | Polish ringkas dengan voice wave 1; makna tidak berubah                                                                      |
|                                                                  | "You have reached the maximum number of requests of 50 requests per month per user."                     | Diganti fallback generik ("Something went wrong. Please try again.") — endpoint Rust `ai.rs` tidak menegakkan kuota 50/bulan |
| `apps/api-rs/crates/api/src/routes/ai.rs`                        | "LLM provider API key and model are required" (tampil ke user)                                           | "AI is not configured for this workspace."                                                                                   |

"Rate limit exceeded for {host}" dan "An internal error has occurred." sudah wajar, tidak diubah. Chip "Summarize this work item in 3 bullets" dan "Draft a status comment for this work item" sudah selaras, tidak diubah.

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

- Endpoint license/instance di api-rs (`apps/api-rs/crates/api/src/routes/instance.rs`, `instance_admin.rs`) — infrastruktur admin/edition, bukan surface jualan.
- Django `apps/api/plane/license/**` — legacy, tidak dijalankan.
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
- Seeding retroaktif untuk workspace lama; background job seeding.
- Infrastruktur dead code multiple-select/bulk-ops.
- Penghapusan `workspace_seed_task.py` Django (dibiarkan sebagai referensi parity).

## Verification

1. **Rust**: `cargo fmt --check` + `cargo test -p api -p common` (unit); integration test seed dengan `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.
2. **Build & deploy api-rs**: `docker compose -f docker-compose-local.yml up -d --build api` (image sama dipakai worker/beat; worker/beat tidak berubah); `systemctl --user reload plane-backend.service` atau perintah compose yang setara.
3. **Frontend**: `pnpm check:lint`, `pnpm --filter=web check:types`, `pnpm --filter=@plane/i18n check:types`; audit `rg` untuk `Upgrade|Talk to Sales|pricing|billing|subscription|payment` pada string user-facing di `apps/web` (sisa hit hanya yang sah).
4. **Seed manual**: buat workspace baru di UI → demo muncul langsung; inspeksi project/work item/page/view; cek DB via `docker exec` bila perlu.
5. **Visual**: `pnpm --filter=web build` + restart `plane-web-prod.service`; settings sidebar tanpa Billing, sidebar tanpa badge "Pro", editor AI + panel Galileo.

## Risks

- **Perubahan api-rs butuh rebuild image** (bukan sekadar restart) — langkah deploy eksplisit di Verification.
- **Seed sinkron menambah latensi** create workspace (puluhan ms) — gagal seed hanya di-log, workspace tetap dibuat.
- **Bot user** harus memenuhi kolom NOT NULL `users` — ikuti daftar kolom Django; password hash acak (tidak pernah login).
- **Identifier kosong** bila nama workspace tanpa karakter alfanumerik — mirror Django (seed gagal & ter-log); catat sebagai edge case.
- **Data kanonik pindah** ke api-rs: Django `SEED_DIR` di-repoint; image Django (bila dibangun) tidak memuat folder itu — seed legacy akan log warning dan skip, tidak crash.
- **Inventory/tripwire tests** (`parity-inventory.json`, `route_inventory_test`, `fe_tripwire_test`) — perbarui notes `POST /api/workspaces/`; jalankan test gate.
- **Non-retroaktif**: workspace yang sudah ada tidak mendapat demo; hanya workspace baru.
