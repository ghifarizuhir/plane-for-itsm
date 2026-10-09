# Cleanse Release Packages + RCB/TCB (CAB Review + Technical Review) — Design

Date: 2026-10-10
Status: Approved (brainstorming 2026-10-10)
Scope: full removal — web + api-rs + DB. Docs disimpan sebagai history.
Supersedes: `2026-10-09-hide-release-review-ui-design.md` (hide → delete).

## 1. Background

Fitur yang dibangun 2026-10-01..03:

- Releases / release packages (workspace-level bundling work items)
- RCB = Release Control Board / "CAB review" (`/release-control`)
- TCB = Testing Control Board / "Technical review" (`/projects/:projectId/testing-control`)
- Pendukung: review requests/sessions/participants/items, AI review briefing, notifikasi review, Submit-to-TCB di issue detail.

Pada 2026-10-09 UI-nya sudah di-hide (nav + route 404; lihat spec hide). Semua file
masih ada di disk sebagai dead code, dan backend api-rs + 6 tabel DB masih hidup.
Keputusan user 2026-10-10: **cleansing all** — hapus seluruh kode, route backend,
tes, tipe, constants, i18n, dan tabel DB (tabel kosong 0 baris).

Istilah user: "CAB" = RCB/release-control, "Technical review" = TCB/testing-control.

## 2. Keputusan yang disetujui

1. Full removal di tiga lapis: web, api-rs, DB.
2. Dokumentasi (`docs/superpowers/specs|plans`, `docs/features/*`) tetap sebagai history.
3. Migration strategi: file `0012`/`0013` **tidak disentuh** (applied migration immutable;
   `sqlx::migrate!` cek checksum — lihat `apps/api-rs/migrations/README.md`).
   Tabel di-drop lewat migration baru `0018_drop_release_review.sql` (versi tertinggi saat
   ini `0017`; guard tes duplikasi versi: `crates/common/tests/migration_versions_test.rs`).
4. Tidak ada feature flag baru.
5. Yang tetap hidup: War rooms, AI scheduler, Services/CMDB, `scripts/release.sh`
   (build tooling offline bundle — bukan fitur ITSM).

## 3. DB

Migration baru `apps/api-rs/migrations/0018_drop_release_review.sql` (idempotent, guarded):

```sql
-- Cleanse release packages + RCB/TCB (review control) — tabel kosong per 2026-10-10.
DROP TABLE IF EXISTS public.review_session_items CASCADE;
DROP TABLE IF EXISTS public.review_session_participants CASCADE;
DROP TABLE IF EXISTS public.review_sessions CASCADE;
DROP TABLE IF EXISTS public.review_requests CASCADE;
DROP TABLE IF EXISTS public.release_changes CASCADE;
DROP TABLE IF EXISTS public.releases CASCADE;

-- Notifikasi in-app entity review (2 baris di live DB per 2026-10-10).
DELETE FROM public.notifications WHERE entity_name IN ('review_request', 'review_session');
```

Dijalankan otomatis saat api boot (`common::db::migrate`). Drop tabel ikut menghapus
index/RLS/constraint miliknya. FK antar tabel review teratasi oleh CASCADE.

Data saat ini: 6 tabel = 0 baris; `notifications` punya 1 `review_request` + 1 `review_session`.
Tidak ada data hilang yang berarti; war_room/ai_schedule_run/issue notification tidak tersentuh.

## 4. Backend api-rs

Hapus file:

- `apps/api-rs/crates/api/src/routes/release.rs` (570 baris)
- `apps/api-rs/crates/api/src/routes/review.rs` (1739)
- `apps/api-rs/crates/api/src/routes/review_briefing.rs` (560)
- `apps/api-rs/crates/api/tests/release_review_test.rs` (1804)
- `apps/api-rs/crates/api/tests/review_briefing_test.rs` (600)

Edit:

- `routes/mod.rs`: hapus `pub mod release; pub mod review; pub mod review_briefing;` (42-44).
- `main.rs`: hapus blok route Releases (4 route), Review requests (3), Review sessions (8),
  Briefing (1) + komentar spec (775-847), sehingga blok war rooms langsung diikuti
  komentar `// Parity with StateViewSet.mark_as_default`.
- `routes/notification.rs`: whitelist `n.entity_name IN (...)` → sisakan
  `('issue', 'ai_schedule_run', 'war_room')`.
- `routes/v1/workspace.rs`: hapus key `"releases": false` dari `v1_workspace_features_json()`
  (tidak ada konsumen di web/types).
- `tests/notification_test.rs`: hapus fixture tuple `review_request` + `review_session`,
  assertion positif keduanya (~205-210), dan assertion negatif mention (~252-257).

## 5. Web

Hapus file:

- `apps/web/core/components/releases/**` (16 file)
- `apps/web/core/components/reviews/**` (25 file)
- `apps/web/core/components/workspace-notifications/review-detail.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/releases/**` (4)
- `apps/web/app/(all)/[workspaceSlug]/(projects)/release-control/**` (4)
- `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control/**` (4)
- `apps/web/core/store/release.store.ts`, `review.store.ts`, `review.store.test.ts`
- `apps/web/core/services/release.{service,helpers}.ts` + `release.helpers.test.ts`,
  `review.{service,helpers}.ts` + `review.helpers.test.ts`
- `apps/web/core/lib/review-notification.ts` + `.test.ts`
- `apps/web/core/hooks/store/use-release.ts`, `use-review.ts`

Edit:

- `apps/web/core/store/root.store.ts`: hapus import `IReleaseStore/ReleaseStore/IReviewStore/ReviewStore`
  (62-65), field interface `release`/`review` (105-106), dan dua instansiasi (151-152, 195-196).

Catatan: route `core.ts`, nav sidebar/project, tombol Submit-to-TCB, dan render notifikasi
sudah dibersihkan pada 2026-10-09 — tidak ada yang perlu diubah lagi di sana.

## 6. Packages (types, constants, i18n)

Types:

- Hapus `packages/types/src/release/**` (core.ts, filters.ts, index.ts) dan
  `packages/types/src/review/**` (core.ts, index.ts).
- `packages/types/src/index.ts`: hapus `export * from "./release"` + `"./review"` (42-43).
- `packages/types/src/workspace-notifications.ts`: hapus import `TReviewBoardType/TReviewOutcome` (8),
  tipe `TNotificationReviewRequest`/`TNotificationReviewSession` (48-68), field
  `review_request`/`review_session` (83-84).

Constants:

- Hapus `packages/constants/src/release.ts`, `review.ts`.
- `packages/constants/src/index.ts`: hapus kedua export (34-35).
- `packages/constants/src/workspace.ts`: hapus object `releases` + `release_control` (223-236).

i18n:

- Hapus `packages/i18n/src/locales/*/release.json` + `review.json` (20 locale × 2 = 40 file).
- `packages/i18n/src/constants/namespaces.ts`: hapus `"release"`, `"review"` (25-26).
- Hapus key `releases`, `release_control`, `testing_control` dari objek `sidebar` di
  20 `locales/*/navigation.json`.
- Regenerate `packages/i18n/src/types/keys.generated.ts`:
  `pnpm --filter=@plane/i18n generate:types`.

## 7. Non-goals / tetap

- `scripts/release.sh`, offline bundle docs (build tooling).
- `docs/superpowers/**`, `docs/features/**` (history).
- `changelogs.release_date`/`is_release_candidate` (fitur upstream changelogs, bukan release packages).
- War rooms, incident bridge removal docs, AI scheduler.
- Tidak ada rename/refactor lain di luar daftar di atas.

## 8. Testing & verifikasi

1. `cargo check -p api --tests` di `apps/api-rs` → 0 error (compile semua tes sisa,
   termasuk `migration_versions_test`).
2. `pnpm check` (format + lint + types) → 0 error (key i18n yang dihapus harus tidak
   direferensikan lagi; regenerasi types memastikan).
3. Setelah deploy backend: `SELECT to_regclass('public.releases');` → NULL untuk 6 tabel,
   `SELECT count(*) FROM notifications WHERE entity_name IN ('review_request','review_session');` → 0.
4. `curl http://localhost:8000/health` → 200; `curl http://localhost:3100/live/health/` → 200
   (live di-restart karena koneksi Redis ikut ter-recreate; lihat AGENTS.md).
5. Web build + prod restart; manual: `/:ws/releases`, `/:ws/release-control`,
   `/projects/:p/testing-control` → 404 (sudah), tidak ada nav, inbox tidak error.

## 9. Rollout & rollback

Rollout (urutan):

1. Backend: `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker
   > /tmp/plane-api-build.log 2>&1 < /dev/null &` — build Rust LTO bisa 10+ menit tanpa output
   > (jangan abort). Migration 0018 jalan saat api boot.
2. Verifikasi §8.3-8.4, restart `plane-live.service`.
3. Web: `pnpm --filter=web build` + `systemctl --user restart plane-web-prod.service`.

Rollback:

- Kode: `git revert <commit>` + rebuild web/backend yang sama.
- DB: migration `0018` tidak punya down-migration. Untuk restore skema, jalankan ulang SQL
  `0012` + `0013` (keduanya `CREATE TABLE IF NOT EXISTS`) secara manual, atau reset DB.
  Data hilang tidak bisa dikembalikan (6 tabel kosong + 2 notifikasi review — inconsequential).

## 10. File inventory (exact)

Hapus (kode): 3 file src + 2 file tes api-rs; 67 file web (16 releases + 25 reviews +
1 review-detail + 12 page/layout + 3 store + 6 service/helper + 2 lib + 2 hook);
5 file types; 2 file constants; 40 file i18n json.

Edit: `routes/mod.rs`, `main.rs`, `notification.rs`, `notification_test.rs`, `v1/workspace.rs`,
`apps/web/core/store/root.store.ts`, `packages/types/src/{index.ts,workspace-notifications.ts}`,
`packages/constants/src/{index.ts,workspace.ts}`, `packages/i18n/src/constants/namespaces.ts`,
20× `navigation.json`, regenerated `keys.generated.ts`.

Tambah: `apps/api-rs/migrations/0018_drop_release_review.sql`.
