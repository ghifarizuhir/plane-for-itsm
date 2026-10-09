# Hide Release Packages + RCB + TCB UI — Design

Date: 2026-10-09
Approach: A — Surgical hide (approved)
Scope: web entry-points only; backend + DB untouched; files stay on disk.

## 1. Background

Fitur yang dibangun sebelumnya:

- Releases / release packages (workspace-level bundling)
- RCB = Release Control Board (`/release-control`, workspace-level)
- TCB = Testing Control Board (`/projects/:projectId/testing-control`, project-level)
- Pendukung: review requests/sessions, review_briefing, notifikasi review, tombol Submit-to-TCB di change detail.

Keputusan user (brainstorming 2026-10-09):

1. Backend + tabel tetap (migrasi `0012_releases_review_control.sql`, `0013_review_briefing.sql`, `routes/release.rs`, `routes/review.rs`, `routes/review_briefing.rs`, tests tetap).
2. Web: hanya sembunyikan nav + route, semua file komponen/page/store/service tetap ada.
3. Sembunyikan semua entry: nav Releases, nav Release Control, nav Testing Control (sidebar + tabbed), tombol Submit-to-TCB, render notifikasi review.
4. URL langsung → 404 (route dihapus dari `core.ts`).

Istilah user "CAB Review" = RCB, "Technical Review" = TCB.

## 2. Non-goals

- Tidak ada perubahan `apps/api-rs` (handler, migrasi, notifikasi whitelist, tests).
- Tidak ada penghapusan file: `components/releases/*`, `components/reviews/*`, `app/.../releases|release-control|testing-control/*`, `store/release.store.ts`, `store/review.store.ts`, `services/release.*`, `services/review.*`, `lib/review-notification.*`, `packages/types/src/release|review`, `packages/constants/src/release|review`, `packages/i18n/*/release.json|review.json` tetap di disk.
- Tidak ada perubahan `scripts/release.sh` / offline-bundle docs (itu build tooling, bukan fitur ITSM).
- Tidak ada feature-flag baru.

## 3. Changes

### 3.1 Route removal — `apps/web/app/routes/core.ts`

Hapus 6 route (file page/layout tetap):

- Workspace: `:workspaceSlug/releases` (list layout 81-83), `:workspaceSlug/releases/:releaseId` (detail 86-91), `:workspaceSlug/release-control` (list 94-96), `:workspaceSlug/release-control/sessions/:sessionId` (detail 99-104).
- Project: `:workspaceSlug/projects/:projectId/testing-control` (list 222-227), `:workspaceSlug/projects/:projectId/testing-control/sessions/:sessionId` (detail 230-238).

Hasil: React Router tidak match → 404. Deep-link lama (mis. dari notifikasi/bookmark) ikut 404.

### 3.2 Workspace nav — `packages/constants/src/workspace.ts` + `sidebar/helper.tsx`

- `workspace.ts`: hapus `releases` + `release_control` dari `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS` (baris 243-244). Definisi object `releases` (223-229) dan `release_control` (230-236) dibiarkan agar tidak merusak import lain.
- `sidebar/helper.tsx`: hapus `case "releases"` + `case "release_control"` (45-47) atau jadikan unreachable default. Tidak ada ikon baru.

### 3.3 Project nav (TCB) — `project-navigation.tsx` + `use-navigation-items.ts`

- `project-navigation.tsx`: hapus item `{ key: "testing-control", name: "Technical review", href: .../testing-control }` (149-151). Berlaku untuk sidebar mode.
- `use-navigation-items.ts`: hapus item yang sama (112-114). Berlaku untuk tabbed mode.
- Tidak ada pengganti; urutan nav project lain tidak berubah.

### 3.4 Issue detail — `issue-detail-quick-actions.tsx`

- Hapus baris import `ChangeTcbControl` (28) dan render `<ChangeTcbControl ... />` (155).
- File `components/reviews/change-tcb-control.tsx` tetap. Tidak ada tombol pengganti; `IssueWarRoomButton` dan quick actions lain tidak berubah.

### 3.5 Notifications — `workspace-notifications/`

- `sidebar/notification-card/item.tsx`: hapus branch review (deep-link 96-103, render 147-181, label 218-219) → notifikasi `review_request`/`review_session` jatuh ke render generik (judul + waktu, tanpa deep-link ke session/board). Hapus juga import `isReviewRequestNotification`/`isReviewSessionNotification`, `getReviewSessionLink`, `REVIEW_OUTCOME_CONFIG`, `REVIEW_BOARD_LABEL_KEYS` yang menjadi unused agar lolos `check:lint`.
- `root.tsx`: hapus render `<ReviewInboxDetail>` (126-130) dan guard `selectedReviewRequest`/`selectedReviewSession` (59-63) beserta import terkait (`ReviewInboxDetail`, guard review) → panel kanan menampilkan fallback generik untuk notifikasi review.
- `review-detail.tsx`: tetap di disk tak terpakai.
- API tetap me-list `review_request`/`review_session` (whitelist `notification.rs` tidak diubah); hanya render web yang generik.

## 4. Data flow

- Sebelum: nav → route → page → store → service → api-rs → tabel releases/review\_\*.
- Sesudah: nav hilang; route hilang → 404 sebelum store/service dipanggil. Tidak ada request baru ke `/releases/` atau `/review-*` dari UI yang disembunyikan. Request lama (retry, bookmark) 404 di router, tidak sampai backend.
- Notifikasi: `GET /notifications` tetap mengembalikan entity review; `item.tsx`/`root.tsx` tidak lagi branching ke `getReviewSessionLink`.

## 5. Error handling

- Tidak ada error baru yang diperkenalkan. 404 adalah perilaku standar React Router untuk route yang dihapus.
- Notifikasi review lama: tidak error, hanya kehilangan deep-link + label board spesifik; judul/body generik tetap tampil.
- Tidak ada migrasi, tidak ada perubahan status data; release/review yang sudah ada di DB tetap konsisten untuk rollback.

## 6. Testing

- `pnpm --filter=web check:types` → 0 error (pastikan tidak ada import unused yang gagal lint; hapus import `ChangeTcbControl`, `ReviewInboxDetail`, guard review bila perlu).
- `pnpm --filter=web check:lint` → 0 error.
- Manual:
  1. Sidebar workspace: tidak ada Releases / Release Control.
  2. Project sidebar + tab: tidak ada Technical review / Testing Control.
  3. Buka langsung `/ws/releases`, `/ws/release-control`, `/ws/projects/p/testing-control` → 404.
  4. Issue detail: tidak ada tombol Submit-to-TCB.
  5. Notifikasi: item review tampil generik, tidak crash, tidak deep-link.
- Tidak ada test backend yang dijalankan (tidak ada perubahan api-rs).

## 7. Rollout & rollback

- Rollout: edit 7 file web → check → `pnpm --filter=web build` → `systemctl --user restart plane-web-prod.service` (prod port 3000; lihat AGENTS.md). Verifikasi `curl` health bila perlu.
- Rollback: `git revert <commit>` + rebuild + restart yang sama. Tidak ada rollback DB karena tidak ada migrasi.

## 8. Files touched (exact)

Edit (8, satu commit):

1. `apps/web/app/routes/core.ts`
2. `packages/constants/src/workspace.ts`
3. `apps/web/core/components/workspace/sidebar/helper.tsx`
4. `apps/web/core/components/workspace/sidebar/project-navigation.tsx`
5. `apps/web/core/components/navigation/use-navigation-items.ts`
6. `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`
7. `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`
8. `apps/web/core/components/workspace-notifications/root.tsx`

Untouched (tetap di disk, tidak dihapus): seluruh `components/releases/*`, `components/reviews/*`, `app/.../releases|release-control|testing-control/*`, store/service/helpers/lib review+release, types, constants release/review, i18n release/review, seluruh `apps/api-rs`.
