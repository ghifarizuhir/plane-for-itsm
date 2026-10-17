# Grup Dokumentasi ITIL4 & Sidebar Docs Buka-Tutup — Design

Date: 2026-10-17
Status: Approved in brainstorming (2026-10-17); awaiting written-spec review.
Scope: web only (`apps/web`). Tidak ada backend, migrasi, route baru, atau perubahan i18n.
Related: `docs/superpowers/specs/2026-10-16-in-app-docs-page-design.md` (sistem docs existing), `apps/web/core/components/docs/*`, `packages/ui/src/collapsible/collapsible.tsx`, `apps/web/core/components/docs/registry.test.ts`.

## Background

Docs in-app sudah live: 4 grup, 21 topik, `registry.ts` sebagai sumber tunggal, konten TSX hardcoded Bahasa Indonesia, sidebar selalu terbuka penuh (`docs-sidebar.tsx`). Dua kebutuhan baru dari brainstorming 2026-10-17:

1. **Dokumentasi penerapan ITIL4** — bagaimana organisasi menjalankan praktik ITIL4 memakai fitur platform yang saling terkait: jenis ticket (Incident/Problem/Change/Improvement + Request), Intake, Queue, Service, Sprint, Track, War Room, dan Knowledge base.
2. **Sidebar docs bisa buka-tutup** — grup di sidebar saat ini selalu terbuka; dengan bertambahnya topik, grup perlu collapsible (hidden expand).

Fakta produk yang mengikat isi dokumentasi (hasil verifikasi kode):

- Jenis ticket ter-seed: **Incident, Problem, Change, Improvement** (`apps/api-rs/crates/api/src/seed.rs:296`). "Request"/"Service Request" tidak ter-seed di build ini; dibuat sebagai jenis baru dan didukung kolom `requires_service` (`apps/api-rs/migrations/0014_intake_service_gate.sql`).
- State milik **project** dan dipakai semua jenis ticket (workflow per jenis sudah dihapus; `apps/api-rs/migrations/0016_remove_workflows.sql`).
- Gate accept Intake (`apps/api-rs/crates/api/src/routes/intake.rs:1379-1425`): jenis non-epic wajib bila project punya jenis, dan wajib link Service bila `requires_service=true`.
- "Health" service saat ini **mock client-side** (`apps/web/core/services/service-health.service.ts:11-26`) — docs tidak boleh menjanjikannya sebagai monitoring nyata.
- Tidak ada fitur SLA/CAB/review board di produk; perencanaan change lewat Sprint/Track dan prioritas.
- Pemetaan istilah: Queue = saved view (`issue_views`), Track = module, Sprint = cycle, Knowledge base = pages, War Room = war rooms.

## Locked decisions (brainstorming 2026-10-17)

1. Grup sidebar baru **ITIL4** berisi 6 topik: `itil4-overview`, `itil4-incident`, `itil4-problem`, `itil4-service-request`, `itil4-change-improvement`, `itil4-flow`.
2. Posisi grup: **setelah "Untuk Project", sebelum "Pengaturan"**.
3. Diagram keterkaitan utama: **matrix praktik × fitur** (bukan flow diagram atau hub-spoke), sebagai primitif baru `DocsPracticeMatrix`.
4. Sidebar: semua grup **collapsible**; default **hanya grup aktif terbuka**; toggle manual **diingat** (localStorage); **keputusan manual menang** atas auto-open; berlaku desktop & mobile.
5. Konten Bahasa Indonesia sebagai TSX di `content/`; tanpa markdown/CMS (mengikuti keputusan #5 spec docs sebelumnya).
6. Tanpa perubahan route, backend, atau i18n. Judul topik tetap string langsung di registry.

## Non-goals

- Diagram alur siklus / hub-and-spoke (dipilih matrix; alur tetap disampaikan lewat `DocsStepList`).
- Mengubah konten 21 topik existing; cross-link hanya dari halaman ITIL4 ke topik fitur.
- Menambah jenis "Request" ke seed backend, atau fitur produk baru (SLA, CAB/review board, monitoring health nyata).
- Pencarian dalam docs, TOC per halaman, versi Inggris konten.
- Perubahan entry point docs (sidebar workspace, help menu) dan route.

## Design

### 1. Registry: grup & topik baru

`apps/web/core/components/docs/registry.ts`:

- `TDocGroupKey` += `"itil4"`.
- `DOC_TOPIC_GROUPS` urutan final: `getting-started`, `member`, `project`, `itil4` (label `"ITIL4"`), `settings`.
- Enam entri baru disisipkan ke `DOC_TOPICS` **setelah topik `intake`** (topik terakhir grup project) dan **sebelum `project-settings`**, karena urutan array menentukan urutan sidebar sekaligus prev/next.

| Slug                       | Judul                      | Deskripsi (header & sidebar)                                                   | Ikon (`@makeplane/propel/icons`) |
| -------------------------- | -------------------------- | ------------------------------------------------------------------------------ | -------------------------------- |
| `itil4-overview`           | ITIL4 di platform ini      | Peta praktik ITIL4 ke fitur platform dan bagaimana semuanya saling terhubung.  | `ConnectionsOutline`             |
| `itil4-incident`           | Incident management        | Alur insiden dari laporan masuk sampai pemulihan, termasuk major incident.     | `WarningCircleOutline`           |
| `itil4-problem`            | Problem management         | Menemukan akar masalah dari insiden berulang dan mengubahnya jadi pengetahuan. | `RelationsOutline`               |
| `itil4-service-request`    | Service request            | Melayani permintaan standar lewat Intake dan katalog layanan.                  | `MessageSupportOutline`          |
| `itil4-change-improvement` | Change & improvement       | Mengelola perubahan secara terencana dan perbaikan berkelanjutan.              | `UpgradeOutline`                 |
| `itil4-flow`               | Alur organisasi end-to-end | Contoh penerapan dari nol: struktur, ritual, dan siklus kerja organisasi.      | `WorkflowsOutline`               |

Semua ikon di atas sudah terverifikasi ada di `@makeplane/propel@0.3.0`. Komponen topik di-load lazy: `lazy(() => import("./content/itil4-<slug>"))`.

`registry.test.ts` existing bersifat list-driven (slug unik, grup valid, default pertama, prev/next tepi) sehingga otomatis memvalidasi topik baru **tanpa perubahan test**.

### 2. Konten halaman ITIL4

Semua file baru di `apps/web/core/components/docs/content/`, default export, memakai primitif `blocks.tsx` (plus `DocsPracticeMatrix` baru). Struktur seragam tiap halaman praktik: paragraf pembuka (konteks ITIL4) → **"Di platform ini"** (`DocsFeatureGrid`) → **alur langkah** (`DocsStepList`) → **peran & ritme** (grid/term table) → callout (jebakan/tips) → `DocsTopicLink` ke topik fitur terkait.

**`itil4-overview`** — ITIL4 dalam satu paragraf (value stream; output vs outcome); `DocsPracticeMatrix` (baris: Incident, Problem, Service request, Change, Improvement; kolom: Intake, Queue, Service, Sprint, Track, KB, War Room, Reports); `DocsTermTable` istilah ITIL4 ↔ istilah di app (Incident, Problem, Known error, Service request, Change, Continual improvement, Knowledge management); callout "mulai dari satu project, satu layanan"; `DocsTopicLink` ke kelima halaman praktik dan alur organisasi.

**`itil4-incident`** — Tujuan: pulihkan layanan secepat mungkin, catat pengetahuan. Alur langkah:

1. Laporan masuk — Intake (form/email/webhook) atau alert otomatis via Intake source (mis. Alertmanager) yang melakukan dedup (`occurrence_count`).
2. Triage — tentukan jenis Incident, prioritas (urgent–low), dan tautkan Service (wajib untuk jenis Incident lewat gate Intake).
3. Kerjakan — Queue per layanan untuk antrean tim; Sprint bila butuh perencanaan; relasi `duplicate`/`blocking` untuk kaitan antar-insiden.
4. Major incident — nyalakan War Room pada ticket Incident: severity otomatis dari prioritas (`urgent→sev1`), runbook per jenis, peran commander/comms/scribe/responder, chat & timeline.
5. Resolve & close — catat solusi di komentar/deskripsi, buat/tautkan artikel Knowledge base, tandai ticket selesai.
6. Review — buat ticket Problem bila akar masalah belum diketahui (link ke `itil4-problem`).

Peran & ritme: L1 triage via Queue; L2 mengerjakan; incident commander saat major; saran queue "Need Triage" dan "Active Incidents". Callout: kesehatan service di halaman Services adalah indikator katalog (belum monitoring nyata) — jangan dijadikan satu-satunya alarm. Links: tickets, intake, war-rooms, queues, services, knowledge-base.

**`itil4-problem`** — Tujuan: temukan akar masalah, cegah insiden berulang. Alur: deteksi pola (relasi `relates_to`/`duplicate`, queue filter insiden berulang); buat ticket jenis **Problem** dan hubungkan ke insiden terkait; investigasi (RCA) memakai Track khusus investigasi + sub-items; hasil jadi artikel **Known error** di Knowledge base yang ditautkan ke ticket; perbaikan permanen jadi ticket **Improvement**. Peran: problem manager (member yang ditunjuk) + SME; ritme: review mingguan queue problem. Callout: tidak semua insiden perlu RCA — mulai dari yang berulang atau berdampak besar. Links: tickets, tracks, knowledge-base, services, queues.

**`itil4-service-request`** — Tujuan: penuhi permintaan standar dengan alur konsisten. Setup jenis "Request": Workspace settings → Work item types → buat jenis "Request" (aktifkan **requires service**), lalu enable di project. Alur: permintaan masuk via Intake (form publik, email, webhook/intake source) → triage memilih jenis Request + Service (gate accept) → accept → fulfillment di queue per layanan (pakai sub-items sebagai checklist bila perlu) → selesai dan perbarui KB bila pertanyaan berulang. Peran: helpdesk L1/L2; permintaan terjadwal (mis. onboarding) direncanakan lewat Sprint. Callout: "Request" tidak ter-seed — halaman ini memuat langkah pembuatannya; jangan aktifkan requires_service untuk jenis di luar permintaan layanan. Links: intake, tickets, services, queues, sprints, knowledge-base.

**`itil4-change-improvement`** — Dua praktik, satu halaman:

- _Change enablement_: buat ticket jenis **Change**; rencanakan dalam **Track** (kumpulan perubahan/inisiatif) dan **Sprint** (kapan dikerjakan); nilai risiko lewat prioritas + deskripsi (rencana, rollback) + tautkan Service yang terdampak; koordinasi lewat komentar dan relasi ke insiden.
- _Continual improvement_: ticket jenis **Improvement** sebagai backlog perbaikan (dari review insiden/problem, retrospektif sprint, atau ide tim); kelola dalam Track; ukur dampak lewat **Reports**.

Peran: change owner, improvement lead; ritme: review backlog improvement di awal sprint. Callout: tidak ada board persetujuan (CAB) di platform — kesepakatan dicatat di komentar ticket. Links: tickets, sprints, tracks, services, reports.

**`itil4-flow`** — Skenario contoh end-to-end "dari nol" untuk satu organisasi (mis. perusahaan dengan 3 layanan internal):

1. Struktur — satu workspace; satu project per layanan besar, atau satu project operasional dengan Service catalog untuk rincian layanan.
2. Service catalog — daftarkan Services + dependency; tetapkan owner dan criticality.
3. Jenis ticket — aktifkan Incident/Problem/Change/Improvement di project; buat jenis Request + requires_service bila perlu.
4. Pintu masuk — aktifkan Intake; sambungkan channel (form/email/webhook); buat Intake source untuk alert monitoring bila ada.
5. Queue standar — "Triage", "Incident aktif", "Problem", "Request per layanan", "Change mendatang"; jadikan public agar dipakai tim.
6. Ritme — Sprint dua minggu untuk pekerjaan operasional terjadwal + Improvement; Track untuk investigasi problem dan inisiatif besar; War Room saat major incident.
7. Loop pengetahuan — setelah insiden/problem, perbarui Knowledge base dan tautkan artikel ke ticket; review bulanan lewat Reports.
8. Perbaikan berulang — temuan review dijadikan ticket Improvement; ukur tren di Reports.

Callout penutup: mulai kecil (satu layanan, satu queue, satu sprint), tambah praktik saat tim nyaman. Ditutup dengan matrix recap + links.

Semua konten memakai istilah menu app persis (Tickets, Sprints, Tracks, Services, Queues, Knowledge base, Intake, War Rooms, Reports) dengan padanan ITIL4 di tabel.

### 3. Primitif `DocsPracticeMatrix` (blocks.tsx)

```ts
export type TPracticeUsage = "primary" | "secondary" | "none";

export function DocsPracticeMatrix({
  columns, // nama kolom fitur, mis. ["Intake", "Queue", "Service", ...]
  rows, // { practice: string; cells: TPracticeUsage[] }
  legend, // teks opsional; default "● sering dipakai · ○ kadang dipakai"
}: {
  columns: string[];
  rows: { practice: string; cells: TPracticeUsage[] }[];
  legend?: string;
});
```

- `<table>` aksesibel menyatu dengan gaya `DocsTermTable` (border-subtle, header `bg-layer-1`): nama kolom di `th`, nama praktik di `th scope="row"`.
- Sel: `primary` → `●` (`text-accent-primary`), `secondary` → `○` (`text-tertiary`), `none` → `–` (`text-placeholder`). Simbol `aria-hidden` + `sr-only` label ("sering dipakai" / "kadang dipakai" / "tidak dipakai").
- Responsif: wrapper `overflow-x-auto`; tanpa kolom sticky (keputusan final, menjaga implementasi sederhana).
- `legend` ditampilkan sebagai `<p>` kecil di bawah tabel.

### 4. Sidebar collapsible

**Perilaku final:**

- Tiap grup = header tombol berisi label uppercase existing + chevron (`ChevronDownOutline`, diputar −90° saat tertutup).
- Status buka grup dihitung: `overrides[group] ?? (group === activeGroup)`; `activeGroup` dari `getTopicBySlug(activeSlug)?.group` (fallback grup topik default).
- Klik header → toggle manual → disimpan sebagai override `{ [groupKey]: boolean }` di localStorage, key `plane-docs-sidebar-groups`.
- Keputusan manual menang: grup aktif yang pernah ditutup manual tetap tertutup sampai dibuka lagi.
- Berlaku sama untuk desktop & mobile (`DocsSidebar` yang sama dipakai di drawer mobile); tidak ada persist terpisah.

**File:**

| File                                | Perubahan                                                                                                                                                                                                                                                                                                                                                                                                              |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs-sidebar-state.ts` (baru)      | Fungsi murni dengan tipe `TGroupOverrides = Partial<Record<TDocGroupKey, boolean>>`: `parseGroupOverrides(raw: string \| null): TGroupOverrides` (JSON parse + validasi boolean per grup; invalid → `{}`), `serializeGroupOverrides(overrides)`, `resolveGroupOpen(group, activeGroup, overrides)`, `toggleGroupOverrides(overrides, group, open)`; konstanta `DOCS_SIDEBAR_GROUPS_KEY = "plane-docs-sidebar-groups"`. |
| `use-docs-sidebar-groups.ts` (baru) | Hook: `activeSlug` dari `useParams().topicSlug` (fallback `DEFAULT_DOC_TOPIC_SLUG`); `overrides` lazy-init dari localStorage; `isGroupOpen(key)`; `toggleGroup(key)` menulis state + localStorage; `activeGroup` via `useMemo`.                                                                                                                                                                                        |
| `docs-sidebar.tsx`                  | Render grup statis → `Collapsible` dari `@plane/ui` (mode terkontrol `isOpen` + `onToggle`, animasi grid-rows bawaan) dengan `className`/`buttonClassName` full-width + chevron; daftar topik per grup tidak berubah.                                                                                                                                                                                                  |
| `docs-sidebar-state.test.ts` (baru) | Unit test vitest untuk modul state.                                                                                                                                                                                                                                                                                                                                                                                    |

Catatan implementasi `Collapsible` (`packages/ui/src/collapsible/collapsible.tsx`): `title` diisi konten header (label + chevron), `isOpen` dikontrol penuh oleh hook, `onToggle` dari hook. Chevron memakai state `isOpen` kita (bukan render-prop `Disclosure`), karena `Collapsible` sudah ada di `@plane/ui` dan dipakai luas.

**Edge & error:**

- localStorage tidak tersedia / corrupt / nilai bukan boolean → fallback default; tidak pernah throw (try/catch di parse).
- Slug tak dikenal → halaman redirect ke `overview` (existing); `activeGroup` jatuh ke grup getting-started.
- Deep-link ke topik pada grup yang pernah ditutup manual → grup tetap tertutup, topik aktif tetap tersorot saat dibuka.
- Prev/next di tepi: `next` dari `itil4-flow` = `project-settings`; `prev` dari `project-settings` = `itil4-flow` — konsisten dengan urutan array registry.

### 5. Testing & verifikasi

- Unit test (`pnpm --filter=web test`):
  - `docs-sidebar-state.test.ts`: parse valid/invalid/JSON rusak, resolve default vs override, toggle, serialize roundtrip.
  - `registry.test.ts` existing tetap hijau tanpa perubahan (validasi list-driven mencakup topik baru).
- `pnpm check:types`, `pnpm check:lint`.
- `pnpm --filter=web build`.
- Manual: deep-link keenam topik ITIL4; auto-open grup aktif saat pindah topik; toggle manual + reload (persist); keputusan manual menang atas auto-open; drawer mobile; light/dark; prev/next di batas grup; halaman existing tidak berubah.
- Deploy (bila diminta): rebuild web prod + restart `plane-web-prod.service` per AGENTS.md.

## Implementation order (indikatif)

1. `docs-sidebar-state.ts` + unit test; `use-docs-sidebar-groups.ts`.
2. Integrasi collapsible di `docs-sidebar.tsx`.
3. Registry: grup `itil4` + 6 entri topik + ikon.
4. `DocsPracticeMatrix` di `blocks.tsx`.
5. Enam file konten ITIL4.
6. Verifikasi menyeluruh (checks, build, manual).
