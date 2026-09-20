# Sidebar ITSM Copy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Voice ITSM untuk sidebar — rewrite USE_CASES + konversi hardcoded string sidebar ke i18n.

**Architecture:** Copy-only + satu perubahan kecil API (`SidebarWrapper` prop `showCustomizeGear`). Tanpa backend.

**Tech Stack:** i18n JSON, React (t() via @plane/i18n), @plane/constants, OxLint.

**Spec:** `docs/superpowers/specs/2026-09-20-sidebar-itsm-copy-design.md`

---

### Task 1: i18n keys + USE_CASES rewrite

**Files:** Modify `packages/i18n/src/locales/en/navigation.json`, `packages/constants/src/workspace.ts`

- [ ] **Step 1:** Tambah keys di object `"sidebar"` (navigation.json): `"hide": "Hide"`, `"more": "More"`, `"pin": "Pin"`, `"unpin": "Unpin"`.
- [ ] **Step 2:** Ganti isi `USE_CASES` sesuai spec (6 item, 2 terakhir tetap).
- [ ] **Step 3:** Validasi JSON + format (`pnpm exec oxfmt --check`), commit `i18n(sidebar): ITSM voice for onboarding use-cases and sidebar helpers`.

### Task 2: Konversi hardcoded string → t()

**Files:** Modify `apps/web/app/(all)/[workspaceSlug]/(projects)/sidebar.tsx`, `apps/web/core/components/sidebar/sidebar-wrapper.tsx`, `apps/web/core/components/workspace/sidebar/sidebar-menu-items.tsx`, `apps/web/core/components/workspace/sidebar/projects-list.tsx`, `apps/web/app/(all)/[workspaceSlug]/(projects)/extended-project-sidebar.tsx`, `apps/web/core/components/workspace/sidebar/extended-sidebar-item.tsx`

- [ ] **Step 1:** `sidebar.tsx` — import `useTranslation`, `title={t("sidebar.projects")}`, pass `showCustomizeGear`.
- [ ] **Step 2:** `sidebar-wrapper.tsx` — props `showCustomizeGear?: boolean` (default false), gear gate = prop, bukan `title === "Projects"`; jaga itemsCollection coupling yang lain tetap.
- [ ] **Step 3:** Ganti `"Hide" : "More"` (2 file), `Pin`/`Unpin` tooltips, `<span>Projects</span>` sesuai mapping spec.
- [ ] **Step 4:** `pnpm --filter=web check:lint && pnpm --filter=web check:types`, commit `feat(sidebar): localize hardcoded copy and pass customize-gear flag`.

### Task 3: Build + verifikasi

- [ ] Build web, restart `plane-web-prod.service`, HTTP 200, string baru ada di bundle, visual sidebar.

---

## Self-review

- Spec coverage: USE_CASES (Task 1), 6 konversi + API wrapper (Task 2), verifikasi (Task 3) — lengkap.
- Placeholder: tidak ada.
- Konsistensi: key `sidebar.hide/more/pin/unpin` dipakai konsisten Task 1 (definisi) & Task 2 (konsumsi).
