# Commit Preview Backlog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Commit 533 uncommitted files on branch `preview` as scoped commits and push to `origin/preview`, with `release/` fully gitignored.

**Architecture:** Exclude build artifacts first via `.gitignore`, then commit per scope (docs, api-rs, web, admin+space, propel/constants, i18n) in dependency order, verifying before each commit and pushing once at the end.

**Tech Stack:** git, GitHub (`origin` = ghifarizuhir/plane-for-itsm, `upstream` = makeplane/plane), pnpm, cargo.

---

## Baseline (verified 2026-10-20)

- Branch: `preview`, tracking `origin/preview`, `ahead 3, behind 0` (fast-forward push, no force needed).
- 3 ahead commits already on `origin/preview`: `966a86c89`, `861714f62`, `14eecb5a4`.
- Uncommit: `509 M + 24 ?? = 533` entries, `+3498/-2514`.
- Breakdown: `packages/i18n 292`, `apps/web 85+3 untracked`, `apps/api-rs 62+1 untracked`, `apps/admin 33+2 untracked`, `apps/space 24+2 untracked`, `packages/propel 9`, `packages/constants 2`, `packages/tailwind-config 1`, `docs/superpowers 1M+~17 untracked`, `release/v1.0.0 6 untracked small files`.
- `release/v1.0.0/tars/` (~1015M, 10x `.tar`) is ALREADY ignored by `.gitignore:45` (`release/*/tars/`), verified with `git check-ignore -v`. The 6 remaining untracked release files are: `.env.prod.example`, `README.md`, `docker-compose.prod.yml`, `install.sh`, `load.sh`, `sha256sums.txt`.
- No `.env`/secret in `git status` (`.gitignore` already covers `.env*`, `*.pem`, `.secrets/`).

---

### Task 0: Gitignore seluruh `release/`

**Files:**

- Modify: `.gitignore:44-46`
- Verify: `git status --short`, `git check-ignore -v`

- [ ] **Step 1: Tambah aturan `/release/` ke `.gitignore`**

Ubah blok ini di `.gitignore`:

```gitignore
# Offline release bundles (built tars, never commit)
release/*/tars/
release/*/.env.prod
```

Menjadi:

```gitignore
# Offline release bundles (built tars, never commit)
release/*/tars/
release/*/.env.prod
/release/
```

Alasan: user memutuskan `release/` tidak di-commit sama sekali. Aturan `/release/` mengabaikan seluruh direktori termasuk `docker-compose.prod.yml`, `install.sh`, `README.md`, dll. Aturan lama tetap dipertahankan agar maksudnya jelas.

- [ ] **Step 2: Verifikasi ignore bekerja**

Run:

```bash
git check-ignore -v release/v1.0.0/docker-compose.prod.yml release/v1.0.0/tars/plane-web-v1.0.0.tar
```

Expected: kedua path diprint dengan prefix `.gitignore` (ignored).

Run:

```bash
git status --short --untracked-files=all | grep "^??" | grep -c release || echo "0 release entries"
```

Expected: `0 release entries` (tidak ada lagi `?? release/...`).

- [ ] **Step 3: Commit `.gitignore`**

Run:

```bash
git add .gitignore
git commit -m "chore: gitignore release bundles"
```

Expected: `1 file changed, 1 insertion(+)`.

---

### Task 1: Commit docs (plans + specs)

**Files:**

- Modify: `docs/superpowers/specs/2026-10-04-intake-itsm-type-service-gate-design.md` (1 file, 2-line change)
- Create (untracked, 16 file): `docs/superpowers/plans/2026-10-02-rcb-tcb-backend-releases.md`, `docs/superpowers/plans/2026-10-02-rcb-tcb-backend-review-engine.md`, `docs/superpowers/plans/2026-10-02-rcb-tcb-web-boards-notifications.md`, `docs/superpowers/plans/2026-10-02-rcb-tcb-web-foundations-releases.md`, `docs/superpowers/plans/2026-10-04-galileo-tools-phase1-search-lookups.md`, `docs/superpowers/plans/2026-10-04-galileo-tools-phase1-services-intake.md`, `docs/superpowers/plans/2026-10-04-galileo-tools-phase1-sprints-tracks-kb.md`, `docs/superpowers/plans/2026-10-04-galileo-tools-phase1-work-item-read.md`, `docs/superpowers/plans/2026-10-05-galileo-tools-phase2a-proposal-foundation.md`, `docs/superpowers/plans/2026-10-05-galileo-tools-phase2b-link-mutations.md`, `docs/superpowers/plans/2026-10-05-galileo-tools-phase3a-container-create-update.md`, `docs/superpowers/plans/2026-10-06-galileo-tools-phase3b-articles.md`, `docs/superpowers/plans/2026-10-06-galileo-tools-phase3c-triage.md`, `docs/superpowers/plans/2026-10-09-ai-article-edit-consistency.md`, `docs/superpowers/plans/2026-10-09-migrate-lock-hardening.md`, `docs/superpowers/plans/2026-10-15-page-sync-and-image-upload-fixes.md`, `docs/superpowers/plans/2026-10-20-service-typed-relationships.md`, `docs/superpowers/specs/2026-10-20-service-typed-relationships-design.md`

- [ ] **Step 1: Stage hanya docs**

Run:

```bash
git add docs/superpowers/plans/2026-10-02-rcb-tcb-backend-releases.md docs/superpowers/plans/2026-10-02-rcb-tcb-backend-review-engine.md docs/superpowers/plans/2026-10-02-rcb-tcb-web-boards-notifications.md docs/superpowers/plans/2026-10-02-rcb-tcb-web-foundations-releases.md docs/superpowers/plans/2026-10-04-galileo-tools-phase1-search-lookups.md docs/superpowers/plans/2026-10-04-galileo-tools-phase1-services-intake.md docs/superpowers/plans/2026-10-04-galileo-tools-phase1-sprints-tracks-kb.md docs/superpowers/plans/2026-10-04-galileo-tools-phase1-work-item-read.md docs/superpowers/plans/2026-10-05-galileo-tools-phase2a-proposal-foundation.md docs/superpowers/plans/2026-10-05-galileo-tools-phase2b-link-mutations.md docs/superpowers/plans/2026-10-05-galileo-tools-phase3a-container-create-update.md docs/superpowers/plans/2026-10-06-galileo-tools-phase3b-articles.md docs/superpowers/plans/2026-10-06-galileo-tools-phase3c-triage.md docs/superpowers/plans/2026-10-09-ai-article-edit-consistency.md docs/superpowers/plans/2026-10-09-migrate-lock-hardening.md docs/superpowers/plans/2026-10-15-page-sync-and-image-upload-fixes.md docs/superpowers/plans/2026-10-20-service-typed-relationships.md docs/superpowers/specs/2026-10-20-service-typed-relationships-design.md docs/superpowers/specs/2026-10-04-intake-itsm-type-service-gate-design.md
```

- [ ] **Step 2: Verifikasi stage**

Run:

```bash
git diff --cached --stat | tail -5
git status --short | grep "^??" | grep -v -E "logo-spinner|page_version_write|release" || echo "no unexpected untracked docs left"
```

Expected: staged ~19 files docs saja, tidak ada `apps/` atau `packages/` di `--cached`.

- [ ] **Step 3: Commit**

```bash
git commit -m "docs: add Galileo/RCB plans and service-relationship spec"
```

Expected: `19 files changed`.

---

### Task 2: Commit backend `apps/api-rs`

**Files:**

- Modify (62): `apps/api-rs/Cargo.lock`, `apps/api-rs/Cargo.toml`, `apps/api-rs/crates/ai/Cargo.toml`, `apps/api-rs/crates/ai/src/decision.rs`, `apps/api-rs/crates/ai/src/llm.rs`, `apps/api-rs/crates/ai/src/tools/lookups.rs`, `apps/api-rs/crates/ai/src/tools/mutations.rs`, `apps/api-rs/crates/ai/src/tools/work_items.rs`, `apps/api-rs/crates/ai/src/triage.rs`, `apps/api-rs/crates/ai/src/triage_job.rs`, `apps/api-rs/crates/api/src/middleware/origin.rs`, `apps/api-rs/crates/api/src/routes/ai.rs`, `apps/api-rs/crates/api/src/routes/asset.rs`, `apps/api-rs/crates/api/src/routes/draft.rs`, `apps/api-rs/crates/api/src/routes/inbound.rs`, `apps/api-rs/crates/api/src/routes/issue_query.rs`, `apps/api-rs/crates/api/src/routes/issue_update.rs`, `apps/api-rs/crates/api/src/routes/issue_write.rs`, `apps/api-rs/crates/api/src/routes/label.rs`, `apps/api-rs/crates/api/src/routes/page.rs`, `apps/api-rs/crates/api/src/routes/project.rs`, `apps/api-rs/crates/api/src/routes/service_access.rs`, `apps/api-rs/crates/api/src/routes/v1/work_item.rs`, `apps/api-rs/crates/api/src/seed.rs`, 27 test files di `apps/api-rs/crates/api/tests/` (`ai_conversations_test.rs`, `ai_test.rs`, `asset_test.rs`, `common/mod.rs`, `cors_test.rs`, `cutover_test.rs`, `detail_label_test.rs`, `estimate_test.rs`, `fe_tripwire_test.rs`, `helper_test.rs`, `inbound_intake_test.rs`, `intake_test.rs`, `intake_triage_routes_test.rs`, `member_test.rs`, `misc_test.rs`, `origin_test.rs`, `page_desc_patch_test.rs`, `parity_gate_test.rs`, `rate_limit_test.rs`, `search_test.rs`, `v1_project_test.rs`, `v1_work_item_type_test.rs`, `view_test.rs`, `work_item_test.rs`, `workspace_seed_test.rs`), `apps/api-rs/crates/common/src/auth.rs`, `apps/api-rs/crates/common/src/config.rs`, `apps/api-rs/crates/common/src/db.rs`, `apps/api-rs/crates/common/src/errors.rs`, `apps/api-rs/crates/common/src/models/mod.rs`, `apps/api-rs/crates/common/tests/django_hash_test.rs`, `apps/api-rs/crates/common/tests/migrate_lock_test.rs`, `apps/api-rs/crates/common/tests/migrate_test.rs`, `apps/api-rs/crates/worker/src/consumer.rs`, `apps/api-rs/crates/worker/src/handlers/cleanup.rs`, `apps/api-rs/crates/worker/src/handlers/file_asset.rs`, `apps/api-rs/crates/worker/src/handlers/mod.rs`, `apps/api-rs/migrations/0022_taxonomy_workspace.sql`
- Create (1): `apps/api-rs/crates/api/src/routes/page_version_write.rs`

- [ ] **Step 1: Stage hanya `apps/api-rs`**

Run:

```bash
git add apps/api-rs/Cargo.lock apps/api-rs/Cargo.toml apps/api-rs/crates apps/api-rs/migrations
```

Expected: `git diff --cached --stat | tail -3` menunjukkan hanya path `apps/api-rs/...`.

- [ ] **Step 2: Verifikasi kompilasi cepat (tanpa DB)**

Run:

```bash
cargo check -p api --manifest-path apps/api-rs/Cargo.toml 2>&1 | tail -5
```

Expected: `Finished` tanpa error. Jika gagal, STOP — perbaiki dulu sebelum commit, jangan lanjut ke Task 3.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(api-rs): Galileo tools, intake triage, and service dependencies"
```

Expected: `63 files changed`.

---

### Task 3: Commit frontend `apps/web`

**Files:**

- Modify (85): routes `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/intake/page.tsx`, 4 auth layouts (`forgot-password`, `reset-password`, `set-password`, `sign-up`), assets (`app/assets/auth/*.webp`, `app/assets/favicon/*`, `app/assets/icons/*`, `app/assets/og-image.png`, `app/assets/plane-logos/*`), `apps/web/app/provider.tsx`, `apps/web/app/root.tsx`, `apps/web/core/components/*` (auth-screens, common, cycles, gantt, inbox, instance, integration/single-integration-card.tsx, issues/_, modules, onboarding, profile, project-states, services, settings, web-hooks, workspace), `apps/web/core/hooks/use-service-peek-overview-redirection.ts`, `apps/web/core/layouts/auth-layout/workspace-wrapper.tsx`, `apps/web/core/store/issue/issue-details/_`(comment, comment_reaction, reaction, sub_issues),`apps/web/core/store/issue/workspace-draft/filter.store.ts`, `apps/web/core/store/label.store.ts`, `apps/web/manifest.json`, `apps/web/public/favicon/_`, `apps/web/public/icons/_`, `apps/web/public/plane-logos/\*`, `apps/web/public/site.webmanifest.json`, `packages/constants/src/endpoints.ts` TIDAK ikut (masuk Task 5)
- Create (2): `apps/web/app/assets/images/logo-spinner-dark.gif`, `apps/web/app/assets/images/logo-spinner-light.gif`

- [ ] **Step 1: Stage hanya `apps/web`**

Run:

```bash
git add apps/web
```

- [ ] **Step 2: Verifikasi stage tidak bocor**

Run:

```bash
git diff --cached --name-only | cut -d/ -f1-2 | sort | uniq -c
```

Expected: hanya `apps/web` (85 modified + 2 untracked = 87 entries).

- [ ] **Step 3: Typecheck web (boleh longgar, catat hasilnya)**

Run:

```bash
pnpm --filter=web exec tsc --noEmit 2>&1 | tail -20
```

Expected: catat error jika ada. Jika error pre-existing di luar file yang diubah, tetap lanjut tapi sebutkan di pesan commit review. Jika error di file yang diubah Task ini, perbaiki dulu.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(web): service typed relationships UI and branding assets"
```

Expected: `~87 files changed`.

---

### Task 4: Commit `apps/admin` + `apps/space` (branding)

**Files:**

- Modify apps/admin (33): `apps/admin/app/(all)/(dashboard)/ai/*`, `authentication/*`, `email/*`, `general/form.tsx`, `sidebar-help-section.tsx`, `(home)/*`, `app/assets/favicon/*`, `app/assets/logos/takeoff-icon-*.svg`, `app/root.tsx`, `components/common/*`, `components/instance/setup-form.tsx`, `hooks/*`, `public/favicon/*`, `public/site.webmanifest.json`
- Create apps/admin (2): `apps/admin/app/assets/images/logo-spinner-dark.gif`, `apps/admin/app/assets/images/logo-spinner-light.gif`
- Modify apps/space (24): `apps/space/app/assets/favicon/*`, `app/assets/plane-logo.svg`, `app/assets/plane-logos/*`, `app/error.tsx`, `app/issues/[anchor]/layout.tsx`, `app/root.tsx`, `components/account/*`, `components/common/*`, `components/instance/*`, `lib/instance-provider.tsx`, `public/favicon/*`, `public/site.webmanifest.json`
- Create apps/space (2): `apps/space/app/assets/images/logo-spinner-dark.gif`, `apps/space/app/assets/images/logo-spinner-light.gif`

- [ ] **Step 1: Stage**

Run:

```bash
git add apps/admin apps/space
```

- [ ] **Step 2: Verifikasi**

Run:

```bash
git diff --cached --stat | tail -3
git diff --cached --name-only | cut -d/ -f1-2 | sort | uniq -c
```

Expected: hanya `apps/admin` + `apps/space`.

- [ ] **Step 3: Commit**

```bash
git commit -m "chore(admin,space): rebrand logos, favicons, and auth headers"
```

Expected: `~61 files changed`.

---

### Task 5: Commit shared packages (`propel`, `constants`, `tailwind-config`)

**Files:**

- Modify (9): `packages/propel/public/plane-lockup-light.svg`, `packages/propel/src/accordion/accordion.stories.tsx`, `packages/propel/src/icons/brand/plane-lockup.tsx`, `packages/propel/src/icons/brand/plane-logo-boxed.tsx`, `packages/propel/src/icons/brand/plane-logo.tsx`, `packages/propel/src/icons/brand/plane-wordmark.tsx`, `packages/propel/src/icons/sub-brand/plane-icon.tsx`, `packages/propel/src/tab-navigation/tab-navigation.stories.tsx`, `packages/propel/src/toast/toast.stories.tsx`
- Modify (2): `packages/constants/src/endpoints.ts`, `packages/constants/src/metadata.ts`
- Modify (1): `packages/tailwind-config/index.css`

- [ ] **Step 1: Stage**

Run:

```bash
git add packages/propel packages/constants packages/tailwind-config
```

- [ ] **Step 2: Verifikasi + lint**

Run:

```bash
git diff --cached --stat | tail -3
pnpm check:lint 2>&1 | tail -10
```

Expected: staged 12 files. Lint boleh ada warning pre-existing, tapi tidak ada error baru di file yang di-stage.

- [ ] **Step 3: Commit**

```bash
git commit -m "chore(packages): rebrand propel icons, metadata, and tailwind base"
```

Expected: `12 files changed`.

---

### Task 6: Commit `packages/i18n` (292 locale JSON)

**Files:**

- Modify (292): seluruh `packages/i18n/src/locales/{cs,de,es,fr,id,it,ja,ka-ge,ko,pl,pt-BR,ro,ru,sk,tr-TR,ua,vi-VN,zh-CN,zh-TW}/*.json` (`auth`, `automation`, `common`, `empty-state`, `home`, `integration`, `power-k`, `project-settings`, `project`, `settings`, `template`, `tour`, `wiki`, `work-item-type`, `workspace-settings`, `workspace`)

- [ ] **Step 1: Stage**

Run:

```bash
git add packages/i18n
```

- [ ] **Step 2: Verifikasi hanya JSON locale**

Run:

```bash
git diff --cached --name-only | awk -F. '{print $NF}' | sort | uniq -c
git diff --cached --stat | tail -2
```

Expected: semuanya `.json`, `292 files changed`. Jika ada `.ts` ikut (mis. `keys.generated.ts`), unstage file itu karena sudah di-ignore:

```bash
git restore --staged packages/i18n/src/types/keys.generated.ts 2>/dev/null || true
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(i18n): add service relationship labels across locales"
```

Expected: `292 files changed`.

---

### Task 7: Final verification + push

- [ ] **Step 1: Pastikan working tree bersih (kecuali yang sengaja diabaikan)**

Run:

```bash
git status --short | head -20
git status --short | wc -l
```

Expected: `0` (atau hanya file sengaja-sisa yang sudah disepakati). `release/` tidak boleh muncul karena sudah di-ignore di Task 0.

- [ ] **Step 2: Review riwayat lokal vs remote**

Run:

```bash
git log --oneline -12
git rev-list --left-right --count HEAD...@{u}
```

Expected: `ahead 10, behind 0` (3 lama + 7 baru: gitignore + 6 scope; jumlah bisa ±1 jika ada task digabung). Tidak ada `behind > 0`. Jika `behind > 0`, STOP — jalankan `git fetch origin` dan rebase/merge dulu, jangan force-push.

- [ ] **Step 3: Push**

Run:

```bash
git push origin preview
```

Expected: `To https://github.com/ghifarizuhir/plane-for-itsm.git ... preview -> preview`, tanpa error. Ukuran push kecil (tidak ada `.tar` 1GB karena sudah di-ignore).

- [ ] **Step 4: Konfirmasi remote**

Run:

```bash
git status -sb | head -3
```

Expected: `## preview...origin/preview` tanpa `[ahead X]` (in sync).

---

## Self-Review

1. **Spec coverage:** Keputusan user ("release di gitignore aja") → Task 0. Backlog 533 file → Task 1–6 per scope sesuai `git status` (`i18n 292`, `web 85+2`, `api-rs 62+1`, `admin 33+2`, `space 24+2`, `propel 9`, `constants 2`, `tailwind 1`, `docs 1M+18 untracked`). Push aman (`ahead 3, behind 0`) → Task 7.
2. **Placeholder scan:** Tidak ada TBD/TODO; semua `git add` menyebut path eksplisit, semua command punya Expected output.
3. **Type consistency:** Urutan commit dari risiko terendah (docs, gitignore) ke tertinggi (api-rs, web), i18n terakhir karena massal tapi aman (JSON saja). Tidak ada dependensi tipe antar task yang bertentangan.

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-10-20-commit-preview-backlog.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
