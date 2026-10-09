# Cleanse Release Packages + RCB/TCB Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hapus total fitur Release packages, RCB ("CAB review"), dan TCB ("Technical review") dari web, backend api-rs, dan DB (6 tabel kosong + 2 notifikasi review).

**Architecture:** Tiga lapis removal. DB lewat migration aditif `0018` (file `0012`/`0013` immutable — checksum sqlx). Backend: hapus 3 route module + 2 file tes, prune registrasi/whitelist. Web/packages: hapus semua file dead code, edit root store, prune types/constants/i18n. Verifikasi: scoped turbo checks + `cargo check -p api --tests`, lalu deploy backend (LTO build) dan web prod.

**Tech Stack:** Rust/axum/sqlx (api-rs), React Router 7 + MobX (web), TypeScript packages (`@plane/types`, `@plane/constants`, `@plane/i18n`), turbo, oxlint/oxfmt, Docker compose `docker-compose-local.yml`.

**Spec:** `docs/superpowers/specs/2026-10-10-cleanse-release-review-design.md`

**Aturan preflight (WAJIB):**

- Workspace ini punya banyak perubahan uncommitted yang TIDAK berhubungan (mis. `apps/admin/**`, `docs/superpowers/specs/2026-10-04-intake-itsm-type-service-gate-design.md`). **JANGAN pernah `git add -A` / `git add .`** — stage hanya path yang disebut di task. Untuk file baru/edit gunakan `git add <path>` eksplisit; untuk hapus gunakan `git rm`.
- Jangan edit migration yang sudah applied (`0012`, `0013`). Migration baru = `0018` (versi tertinggi saat ini `0017`; lihat `apps/api-rs/migrations/README.md`).
- Commit hook menjalankan oxfmt pada file staged — abaikan reformat otomatis, jangan stage file lain.
- Container saat ini: `plane-for-itsm-api-1`, `plane-for-itsm-plane-db-1`; DB `plane`, user `plane`.

---

## File Structure

**Create (1):** `apps/api-rs/migrations/0018_drop_release_review.sql`

**Delete (kode, 77 file):**

- api-rs (5): `crates/api/src/routes/{release,review,review_briefing}.rs`, `crates/api/tests/{release_review_test,review_briefing_test}.rs`
- web (67): `apps/web/core/components/releases/**` (16), `apps/web/core/components/reviews/**` (25), `apps/web/core/components/workspace-notifications/review-detail.tsx`, 12 page/layout di `app/(all)/[workspaceSlug]/(projects)/{releases,release-control}/**` + `.../[projectId]/testing-control/**` (route sudah dihapus dari `core.ts` 2026-10-09), `core/store/{release.store.ts,review.store.ts,review.store.test.ts}`, `core/services/{release.service.ts,release.helpers.ts,release.helpers.test.ts,review.service.ts,review.helpers.ts,review.helpers.test.ts}`, `core/lib/{review-notification.ts,review-notification.test.ts}`, `core/hooks/store/{use-release.ts,use-review.ts}`
- types (5): `packages/types/src/release/{core,filters,index}.ts`, `packages/types/src/review/{core,index}.ts`
- constants (2): `packages/constants/src/{release,review}.ts`
- i18n (40): `packages/i18n/src/locales/*/{release,review}.json` (20 locale)

**Modify (28):** `crates/api/src/{routes/mod.rs,main.rs}`, `crates/api/src/routes/notification.rs`, `crates/api/src/routes/v1/workspace.rs`, `crates/api/tests/notification_test.rs`, `apps/web/core/store/root.store.ts`, `packages/types/src/{index.ts,workspace-notifications.ts}`, `packages/constants/src/{index.ts,workspace.ts}`, `packages/i18n/src/constants/namespaces.ts`, 20× `packages/i18n/src/locales/*/navigation.json`, regenerated `packages/i18n/src/types/keys.generated.ts`.

---

### Task 1: Migration 0018

**Files:**

- Create: `apps/api-rs/migrations/0018_drop_release_review.sql`

- [ ] **Step 1: Tulis file migration**

Isi persis:

```sql
-- Cleanse release packages + RCB/TCB (review control) — fitur dihapus 2026-10-10.
-- Tabel kosong saat drop; index/RLS/constraint ikut terhapus via CASCADE.
DROP TABLE IF EXISTS public.review_session_items CASCADE;
DROP TABLE IF EXISTS public.review_session_participants CASCADE;
DROP TABLE IF EXISTS public.review_sessions CASCADE;
DROP TABLE IF EXISTS public.review_requests CASCADE;
DROP TABLE IF EXISTS public.release_changes CASCADE;
DROP TABLE IF EXISTS public.releases CASCADE;

-- Notifikasi in-app entity review (2 baris di live DB per 2026-10-10).
DELETE FROM public.notifications WHERE entity_name IN ('review_request', 'review_session');
```

- [ ] **Step 2: Verifikasi versi unik**

Run:

```bash
ls apps/api-rs/migrations/ | rg '^0018'
```

Expected: persis satu baris `0018_drop_release_review.sql`.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/0018_drop_release_review.sql
git commit -m "feat(db): drop release + review tables and notifications (0018)"
```

---

### Task 2: Backend api-rs removal

**Files:**

- Delete: `apps/api-rs/crates/api/src/routes/release.rs`, `apps/api-rs/crates/api/src/routes/review.rs`, `apps/api-rs/crates/api/src/routes/review_briefing.rs`, `apps/api-rs/crates/api/tests/release_review_test.rs`, `apps/api-rs/crates/api/tests/review_briefing_test.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`, `apps/api-rs/crates/api/src/main.rs:775-847`, `apps/api-rs/crates/api/src/routes/notification.rs:266`, `apps/api-rs/crates/api/src/routes/v1/workspace.rs:27`, `apps/api-rs/crates/api/tests/notification_test.rs`

- [ ] **Step 1: Hapus 3 modul route dari `routes/mod.rs`**

Hapus 3 baris:

```rust
pub mod release;
pub mod review;
pub mod review_briefing;
```

Setelah itu `pub mod reactions;` langsung diikuti `pub mod s3proxy;`. `pub mod validation;` tetap (bukan fitur ini).

- [ ] **Step 2: Hapus blok route di `main.rs` (baris 775-847)**

Hapus dari baris komentar `// Releases (RCB scope): workspace-level bundles of work items.` sampai `),` penutup route briefing. Sebelum dan sesudahnya harus menjadi:

```rust
            patch(routes::war_room::messages_patch).delete(routes::war_room::messages_destroy),
        )
        // Parity with `StateViewSet.mark_as_default`
```

Total yang dihapus: 4 route releases, 3 route review-requests, 8 route review-sessions, 1 route briefing (termasuk 2 komentar spec).

- [ ] **Step 3: Prune whitelist entity di `notification.rs:266`**

Edit:

```rust
         AND n.entity_name IN ('issue', 'ai_schedule_run', 'war_room', 'review_request', 'review_session') \
```

menjadi:

```rust
         AND n.entity_name IN ('issue', 'ai_schedule_run', 'war_room') \
```

- [ ] **Step 4: Hapus key `releases` di `v1/workspace.rs`**

Edit blok JSON `v1_workspace_features_json()`:

```rust
        "work_item_types": false,
        "releases": false,
        "states_owned_by_workspace": false,
```

menjadi:

```rust
        "work_item_types": false,
        "states_owned_by_workspace": false,
```

- [ ] **Step 5: Hapus fixture review di `notification_test.rs`**

Edit A — hapus 2 tuple review, sisakan tuple `mystery`:

Dari:

```rust
        (
            "review_request",
            "Review requested",
            "in_app:review:agenda_added",
            json!({"review_request": {
                "id": Uuid::new_v4().to_string(),
                "board_type": "tcb",
                "project_id": Uuid::new_v4().to_string(),
                "workspace_slug": slug.clone(),
                "status": "scheduled",
                "subject_label": "CHG-1 Fix checkout",
                "session_id": Uuid::new_v4().to_string(),
                "session_title": "Weekly TCB",
                "scheduled_at": "2026-10-05T09:00:00Z",
            }}),
        ),
        (
            "review_session",
            "Review scheduled",
            "in_app:review:session_scheduled",
            json!({"review_session": {
                "id": Uuid::new_v4().to_string(),
                "board_type": "tcb",
                "project_id": Uuid::new_v4().to_string(),
                "workspace_slug": slug.clone(),
                "title": "Weekly TCB",
                "scheduled_at": "2026-10-05T09:00:00Z",
            }}),
        ),
        ("mystery", "Hidden", "in_app:other", json!({})),
```

Menjadi:

```rust
        ("mystery", "Hidden", "in_app:other", json!({})),
```

Edit B — hapus assertion positif:

Dari:

```rust
    assert!(
        names.contains(&"review_request"),
        "review request notifications must be listed"
    );
    assert!(
        names.contains(&"review_session"),
        "review session notifications must be listed"
    );
    // Mention-carrying senders surface under `mentioned=true`, not the default list.
```

Menjadi:

```rust
    // Mention-carrying senders surface under `mentioned=true`, not the default list.
```

Edit C — hapus assertion negatif mention:

Dari:

```rust
    assert!(
        !names.contains(&"review_request"),
        "review requests are not mention notifications"
    );
    assert!(
        !names.contains(&"review_session"),
        "review sessions are not mention notifications"
    );
    assert!(
        !names.contains(&"mystery"),
```

Menjadi:

```rust
    assert!(
        !names.contains(&"mystery"),
```

- [ ] **Step 6: Hapus 5 file**

```bash
git rm apps/api-rs/crates/api/src/routes/release.rs \
       apps/api-rs/crates/api/src/routes/review.rs \
       apps/api-rs/crates/api/src/routes/review_briefing.rs \
       apps/api-rs/crates/api/tests/release_review_test.rs \
       apps/api-rs/crates/api/tests/review_briefing_test.rs
```

- [ ] **Step 7: Verifikasi tidak ada referensi tersisa**

Run:

```bash
rg -n "routes::(release|review)::|review_briefing|review-requests|review-sessions|/releases/" \
  apps/api-rs/crates/api/src apps/api-rs/crates/api/tests; echo "exit=$?"
```

Expected: tidak ada match (`exit=1`). Satu-satunya `release`/`review` yang boleh tersisa adalah kata bahasa Inggris di komentar/dependency upstream (`changelogs.release_date` di migration lama — tidak di-scope ini).

- [ ] **Step 8: Compile semua test target**

Run (dari `apps/api-rs`):

```bash
cargo check -p api --tests
```

Expected: `Finished` tanpa error dan tanpa warning `unused import`.

- [ ] **Step 9: Jalankan guard migration + test notifikasi**

Run (dari `apps/api-rs`):

```bash
cargo test -p common --test migration_versions_test
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test notification_test
```

Expected: keduanya `test result: ok`. `notification_test` membuat scratch workspace sendiri dan membersihkannya di akhir — aman terhadap DB lokal.

- [ ] **Step 10: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/mod.rs \
        apps/api-rs/crates/api/src/main.rs \
        apps/api-rs/crates/api/src/routes/notification.rs \
        apps/api-rs/crates/api/src/routes/v1/workspace.rs \
        apps/api-rs/crates/api/tests/notification_test.rs
git commit -m "feat(api-rs): remove release packages + review control backend"
```

---

### Task 3: Web removal

**Files:**

- Delete: 67 file web (lihat daftar di Step 1)
- Modify: `apps/web/core/store/root.store.ts`

- [ ] **Step 1: Hapus file web**

```bash
git rm -r apps/web/core/components/releases apps/web/core/components/reviews
git rm apps/web/core/components/workspace-notifications/review-detail.tsx
git rm -r "apps/web/app/(all)/[workspaceSlug]/(projects)/releases" \
          "apps/web/app/(all)/[workspaceSlug]/(projects)/release-control" \
          "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/testing-control"
git rm apps/web/core/store/release.store.ts apps/web/core/store/review.store.ts apps/web/core/store/review.store.test.ts
git rm apps/web/core/services/release.service.ts apps/web/core/services/release.helpers.ts apps/web/core/services/release.helpers.test.ts \
       apps/web/core/services/review.service.ts apps/web/core/services/review.helpers.ts apps/web/core/services/review.helpers.test.ts
git rm apps/web/core/lib/review-notification.ts apps/web/core/lib/review-notification.test.ts
git rm apps/web/core/hooks/store/use-release.ts apps/web/core/hooks/store/use-review.ts
```

Expected: `git status --short` menunjukkan semua path di atas deleted (`D`).

- [ ] **Step 2: Edit import di `root.store.ts`**

Dari:

```ts
import type { IWarRoomStore } from "./war-room.store";
import { WarRoomStore } from "./war-room.store";

import type { IReleaseStore } from "./release.store";
import { ReleaseStore } from "./release.store";
import type { IReviewStore } from "./review.store";
import { ReviewStore } from "./review.store";
import type { IMultipleSelectStore } from "./multiple_select.store";
```

Menjadi:

```ts
import type { IWarRoomStore } from "./war-room.store";
import { WarRoomStore } from "./war-room.store";

import type { IMultipleSelectStore } from "./multiple_select.store";
```

- [ ] **Step 3: Edit field interface root store**

Dari:

```ts
warRoom: IWarRoomStore;
release: IReleaseStore;
review: IReviewStore;
projectView: IProjectViewStore;
```

Menjadi:

```ts
warRoom: IWarRoomStore;
projectView: IProjectViewStore;
```

- [ ] **Step 4: Hapus instansiasi di dua constructor**

Blok berikut muncul **2×** (baris ~151-152 dan ~195-196, satu di `constructor()`, satu di re-hydrate). Hapus di kedua lokasi:

```ts
this.release = new ReleaseStore(this);
this.review = new ReviewStore(this);
```

Setelah kedua edit: `this.warRoom = new WarRoomStore(this);` langsung diikuti `this.projectView = new ProjectViewStore(this);` di kedua tempat.

- [ ] **Step 5: Verifikasi tidak ada dangling import**

Run:

```bash
rg -n "release\.store|review\.store|use-release|use-review|review-notification|components/(releases|reviews)" \
  apps/web --glob '!**/node_modules/**'; echo "exit=$?"
```

Expected: tidak ada match (`exit=1`).

- [ ] **Step 6: Typecheck + lint web**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: keduanya PASS, 0 error.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/store/root.store.ts
git commit -m "feat(web): remove release + review control code"
```

(`git rm` sudah men-stage semua file deleted.)

---

### Task 4: Types + constants prune

**Files:**

- Delete: `packages/types/src/release/**`, `packages/types/src/review/**`, `packages/constants/src/release.ts`, `packages/constants/src/review.ts`
- Modify: `packages/types/src/index.ts`, `packages/types/src/workspace-notifications.ts`, `packages/constants/src/index.ts`, `packages/constants/src/workspace.ts`

- [ ] **Step 1: Hapus file types + constants**

```bash
git rm -r packages/types/src/release packages/types/src/review
git rm packages/constants/src/release.ts packages/constants/src/review.ts
```

- [ ] **Step 2: Edit `packages/types/src/index.ts`**

Dari:

```ts
export * from "./release";
export * from "./review";
export * from "./publish";
```

Menjadi:

```ts
export * from "./publish";
```

- [ ] **Step 3: Edit `packages/types/src/workspace-notifications.ts`**

Edit A — import:

Dari:

```ts
import type { ENotificationFilterType } from "./enums";
import type { TReviewBoardType, TReviewOutcome } from "./review";
import type { IUserLite } from "./users";
```

Menjadi:

```ts
import type { ENotificationFilterType } from "./enums";
import type { IUserLite } from "./users";
```

Edit B — hapus dua tipe review. Dari:

```ts
export type TNotificationReviewRequest = {
  id: string;
  board_type: TReviewBoardType;
  project_id: string | null;
  workspace_slug: string;
  status: "pending" | "scheduled" | "decided";
  subject_label: string;
  session_id?: string;
  session_title?: string;
  scheduled_at?: string;
  outcome?: TReviewOutcome;
};

export type TNotificationReviewSession = {
  id: string;
  board_type: TReviewBoardType;
  project_id: string | null;
  workspace_slug: string;
  title: string;
  scheduled_at: string;
};

export type TNotificationData = {
```

Menjadi:

```ts
export type TNotificationData = {
```

Edit C — hapus field review. Dari:

```ts
  war_room?: TNotificationWarRoom | undefined;
  review_request?: TNotificationReviewRequest | undefined;
  review_session?: TNotificationReviewSession | undefined;
};
```

Menjadi:

```ts
  war_room?: TNotificationWarRoom | undefined;
};
```

- [ ] **Step 4: Edit `packages/constants/src/index.ts`**

Dari:

```ts
export * from "./release";
export * from "./review";
export * from "./rich-filters";
```

Menjadi:

```ts
export * from "./rich-filters";
```

- [ ] **Step 5: Edit `packages/constants/src/workspace.ts`**

Hapus 2 object berikut (setelah object `archives`, sebelum penutup):

```ts
  releases: {
    key: "releases",
    labelTranslationKey: "sidebar.releases",
    href: `/releases/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname, url) => pathname.includes(url),
  },
  release_control: {
    key: "release_control",
    labelTranslationKey: "sidebar.release_control",
    href: `/release-control/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname, url) => pathname.includes(url),
  },
};
```

Sehingga object `archives` langsung diikuti `};`. `WORKSPACE_SIDEBAR_DYNAMIC_NAVIGATION_ITEMS_LINKS` (views/analytics/archives) tidak berubah — sudah dipruning 2026-10-09.

- [ ] **Step 6: Verifikasi + check package**

Run:

```bash
rg -n "\brelease|\breview" packages/types/src packages/constants/src; echo "exit=$?"
pnpm --filter=@plane/types check:types
pnpm --filter=@plane/constants check:types
```

Expected: tidak ada match (`exit=1`; pattern word-boundary tidak match `prerelease`/`preview`); kedua check PASS.

- [ ] **Step 7: Commit**

```bash
git add packages/types/src/index.ts packages/types/src/workspace-notifications.ts \
        packages/constants/src/index.ts packages/constants/src/workspace.ts
git commit -m "refactor(types,constants): drop release + review surfaces"
```

---

### Task 5: i18n prune + regenerate

**Files:**

- Delete: 40 file `packages/i18n/src/locales/*/{release,review}.json`
- Modify: `packages/i18n/src/constants/namespaces.ts`, 20× `packages/i18n/src/locales/*/navigation.json`, `packages/i18n/src/types/keys.generated.ts` (regenerated)

- [ ] **Step 1: Hapus namespace files (20 locale × 2)**

```bash
git rm packages/i18n/src/locales/*/release.json packages/i18n/src/locales/*/review.json
```

Expected: 40 file `D` di `git status --short`.

- [ ] **Step 2: Edit `namespaces.ts`**

Dari:

```ts
  "release",
  "review",
  "service",
```

Menjadi:

```ts
  "service",
```

- [ ] **Step 3: Hapus 3 key sidebar dari 20 `navigation.json`**

Jalankan (semua locale punya key ini di `sidebar`, masing-masing tepat 1×; script memverifikasi):

```bash
for f in packages/i18n/src/locales/*/navigation.json; do
  node -e '
    const fs = require("fs");
    const p = process.argv[1];
    const j = JSON.parse(fs.readFileSync(p, "utf8"));
    for (const k of ["releases", "release_control", "testing_control"]) {
      if (!(k in (j.sidebar ?? {}))) throw new Error(`${p}: missing sidebar.${k}`);
      delete j.sidebar[k];
    }
    fs.writeFileSync(p, JSON.stringify(j, null, 2) + "\n");
  ' "$f"
done
```

- [ ] **Step 4: Verifikasi key hilang**

Run:

```bash
rg -n '"releases":|"release_control":|"testing_control":' packages/i18n/src/locales; echo "exit=$?"
ls packages/i18n/src/locales/*/release.json packages/i18n/src/locales/*/review.json 2>&1 | head -1
```

Expected: `exit=1` untuk rg; `ls` gagal (`No such file or directory`).

- [ ] **Step 5: Regenerate `keys.generated.ts`**

Run:

```bash
pnpm --filter=@plane/i18n generate:types
```

Expected: file `packages/i18n/src/types/keys.generated.ts` berubah tanpa baris `release.*`, `review.*`, `sidebar.releases`, `sidebar.release_control`, `sidebar.testing_control`.

- [ ] **Step 6: Verifikasi generated + sync**

Run:

```bash
rg -n '"(release|review)\.|sidebar\.(releases|release_control|testing_control)"' packages/i18n/src/types/keys.generated.ts; echo "exit=$?"
pnpm --filter=@plane/i18n check:types
pnpm --filter=@plane/i18n check:sync
```

Expected: rg `exit=1`; kedua check PASS.

- [ ] **Step 7: Commit**

```bash
git add packages/i18n/src/constants/namespaces.ts \
        packages/i18n/src/types/keys.generated.ts \
        packages/i18n/src/locales/*/navigation.json
git commit -m "refactor(i18n): drop release + review namespaces and nav keys"
```

(`git rm` Step 1 sudah men-stage 40 file deleted.)

---

### Task 6: Scoped verification (affected packages)

**Files:** none (verification only).

- [ ] **Step 1: Turbo check dengan dependency build**

Run (root):

```bash
pnpm check:types --filter=@plane/types --filter=@plane/constants --filter=@plane/i18n --filter=web
```

Expected: PASS. Task `check:types` turbo melakukan `^build` dulu, jadi dist package di-rebuild tanpa surface release/review dan tsc memvalidasi terhadap dist fresh.

- [ ] **Step 2: Lint + format affected**

Run:

```bash
pnpm check:lint --filter=@plane/types --filter=@plane/constants --filter=@plane/i18n --filter=web
pnpm check:format --filter=@plane/types --filter=@plane/constants --filter=@plane/i18n --filter=web
```

Expected: keduanya PASS.

- [ ] **Step 3: Sanity grep repo-wide**

Run:

```bash
rg -n "testing-control|release-control|review-requests|review-sessions|TNotificationReview|use-release|use-review" \
  apps/web/core apps/web/app packages/types/src packages/constants/src packages/i18n/src apps/api-rs/crates \
  --glob '!**/node_modules/**'; echo "exit=$?"
```

Expected: `exit=1` (tidak ada match). Sebutan di `docs/**` dan file ber-nama `release.sh` tidak di-scope.

Tidak ada commit di task ini (tidak ada perubahan file).

---

### Task 7: Deploy backend + verifikasi DB

**Files:** none (ops).

- [ ] **Step 1: Rebuild + up container backend (detached, sesuai AGENTS.md)**

Run (root):

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Catatan: build Rust LTO bisa 10+ menit **tanpa output** — bukan hang, jangan abort.

- [ ] **Step 2: Poll build sampai selesai**

Run:

```bash
tail -5 /tmp/plane-api-build.log
docker ps --filter name=plane-for-itsm-api-1 --format '{{.Status}}'
```

Ulangi sampai log tidak berkembang dan container `Up`. Expected: `plane-for-itsm-api-1` `Up ...` (healthcheck passing).

- [ ] **Step 3: Verifikasi tabel DB hilang + notifikasi bersih**

Run:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c \
  "SELECT to_regclass('public.releases') AS releases, to_regclass('public.release_changes') AS release_changes, to_regclass('public.review_requests') AS review_requests, to_regclass('public.review_sessions') AS review_sessions, to_regclass('public.review_session_participants') AS participants, to_regclass('public.review_session_items') AS items;"
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c \
  "SELECT count(*) FROM notifications WHERE entity_name IN ('review_request','review_session');"
```

Expected: keenam kolom `NULL`; count `0`.

- [ ] **Step 4: Health check + endpoint lama 404**

Run:

```bash
curl -s -o /dev/null -w "health=%{http_code}\n" http://localhost:8000/health
curl -s -o /dev/null -w "releases=%{http_code}\n" http://localhost:8000/api/workspaces/x/releases/
curl -s -o /dev/null -w "review-sessions=%{http_code}\n" http://localhost:8000/api/workspaces/x/review-sessions/
```

Expected: `health=200`, `releases=404`, `review-sessions=404`.

- [ ] **Step 5: Restart live (Redis connection ikut ter-recreate saat api rebuild)**

Run:

```bash
systemctl --user restart plane-live.service
curl -s -o /dev/null -w "live=%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `live=200` (cold start ~8 detik, ulangi curl bila perlu).

Tidak ada commit di task ini.

---

### Task 8: Deploy web prod + verifikasi manual

**Files:** none (ops).

- [ ] **Step 1: Build web**

Run:

```bash
pnpm --filter=web build
```

Expected: build sukses.

- [ ] **Step 2: Restart prod**

Run:

```bash
systemctl --user restart plane-web-prod.service
systemctl --user is-active plane-web-prod.service
```

Expected: `active`. Jangan menyalakan `plane-web.service` (dev) bersamaan (AGENTS.md).

- [ ] **Step 3: Manual checks (semua harus true)**

1. Sidebar workspace: tidak ada `Release packages` / `CAB review`.
2. Project sidebar + tabbed nav: tidak ada `Technical review` / `Testing control`.
3. URL langsung `/:ws/releases`, `/:ws/releases/:id`, `/:ws/release-control`, `/:ws/release-control/sessions/:id`, `/:ws/projects/:p/testing-control`, `/:ws/projects/:p/testing-control/sessions/:id` → 404.
4. Issue detail quick actions: tidak ada Submit-to-TCB.
5. Inbox/notifikasi: tidak error; notifikasi issue/war room/ai schedule tetap tampil normal.

- [ ] **Step 4: Rollback note (hanya bila verifikasi gagal)**

```bash
git revert <commit>   # revert commit task yang gagal
pnpm --filter=web build && systemctl --user restart plane-web-prod.service
# backend: git revert + setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker
```

DB: `0018` tidak punya down-migration; skema lama bisa direstore dengan menjalankan ulang SQL `0012` + `0013` manual (keduanya `CREATE TABLE IF NOT EXISTS`). Data lama (6 tabel kosong + 2 notifikasi review) tidak bisa dikembalikan — inconsequential.

---

## Self-Review

**1. Spec coverage:** §3 DB → Task 1 + Task 7 Step 3; §4 backend → Task 2; §5 web → Task 3; §6 types/constants/i18n → Task 4 + Task 5; §8 testing → Task 2 Step 8-9, Task 3 Step 6, Task 4 Step 6, Task 5 Step 6, Task 6, Task 7 Step 3-4; §9 rollout/rollback → Task 7 + Task 8. Tidak ada gap.

**2. Placeholder scan:** Tidak ada TBD/TODO/"implement later"; semua edit menampilkan kode before/after eksak, semua perintah punya expected output.

**3. Type consistency:** `TNotificationReviewRequest`/`TNotificationReviewSession`/`TReviewBoardType`/`TReviewOutcome` dihapus konsisten dari types + workspace-notifications; `IReleaseStore`/`IReviewStore`/`ReleaseStore`/`ReviewStore` dihapus konsisten dari import, interface, dan 2 constructor; namespace `release`/`review` dihapus dari `NAMESPACES` bersamaan dengan file locale-nya sehingga `generate:types` + `check:sync` konsisten; whitelist `notification.rs` dan fixture `notification_test.rs` sama-sama tinggal `issue`/`ai_schedule_run`/`war_room`/`mystery`.
