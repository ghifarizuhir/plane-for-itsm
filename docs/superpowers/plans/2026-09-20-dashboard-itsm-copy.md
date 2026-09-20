# Dashboard ITSM Copy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ubah copywriting Home dashboard dari voice project management ke voice service management (ITSM), dengan minimal rewiring CTA quickstart card 1.

**Architecture:** Copy tinggal di `packages/i18n/src/locales/en/home.json` (locale `en` saja per spec). Rewiring terbatas di quickstart card "create-project" pada `apps/web/core/components/home/widgets/empty-states/no-projects.tsx` — bila user sudah punya joined project, CTA mengarah ke halaman Services project pertama; jika belum, tetap create project modal. Tidak ada perubahan schema/backend/widget.

**Tech Stack:** i18n JSON (@plane/i18n), React Router 8 + MobX (`useProject` store), OxLint/oxfmt. `apps/web` tidak punya test framework — verifikasi = lint + types + build + manual visual.

**Spec:** `docs/superpowers/specs/2026-09-20-dashboard-itsm-copy-design.md`

---

### Task 1: Copy changes di `en/home.json`

**Files:**

- Modify: `packages/i18n/src/locales/en/home.json`

- [ ] **Step 1: Edit copy keys**

Ganti 4 key dalam object `"home"`:

```json
"create_project": {
  "title": "Create your first project",
  "description": "A project is home to your services, incidents, and work items in Terraline.",
  "cta": "Get started",
  "cta_view_services": "View services"
},
"invite_team": {
  "title": "Invite your team",
  "description": "Keep services healthy and resolve incidents together.",
  "cta": "Get them in"
},
```

Dan di `"recents": { "empty": { ... } }`:

```json
"project": "Your recent projects will show up here after you visit one.",
```

Hasil akhir bagian `home.empty` tidak boleh menambah/mengurangi key lain — hanya konten string keempat di atas plus key baru `cta_view_services`.

- [ ] **Step 2: Verifikasi cepat**

Run: `pnpm --filter=@plane/i18n check:types 2>/dev/null || pnpm exec oxfmt --check packages/i18n/src/locales/en/home.json`
Expected: no formatting errors.

- [ ] **Step 3: Commit**

```bash
git add packages/i18n/src/locales/en/home.json
git commit -m "i18n(dashboard): ITSM voice for home empty-state and recents copy"
```

---

### Task 2: Rewiring CTA quickstart card 1

**Files:**

- Modify: `apps/web/core/components/home/widgets/empty-states/no-projects.tsx`

- [ ] **Step 1: Tambah derived value**

Setelah baris `const { joinedProjectIds } = useProject();` (line 32):

```tsx
const firstJoinedProjectId = joinedProjectIds?.[0];
```

- [ ] **Step 2: Kondisikan CTA card 1**

Di array `EMPTY_STATE_DATA`, ganti entry card pertama (`id: "create-project"`) — bagian `cta` inilah yang berubah, sisa entry tetap:

```tsx
{
  id: "create-project",
  title: "home.empty.create_project.title",
  description: "home.empty.create_project.description",
  icon: <ProjectsOutline className="size-4" />,
  flag: "projects",
  cta: firstJoinedProjectId
    ? {
        text: "home.empty.create_project.cta_view_services",
        link: `/${workspaceSlug}/projects/${firstJoinedProjectId}/services`,
      }
    : {
        text: "home.empty.create_project.cta",
        onClick: (e: React.MouseEvent<HTMLButtonElement, MouseEvent>) => {
          if (!canCreateProject) return;
          e.preventDefault();
          e.stopPropagation();
          toggleCreateProjectModal(true);
        },
        disabled: !canCreateProject,
      },
},
```

Catatan: render CTA `link` existing sudah ada di komponen (pattern kartu `invite-team`, baris 171-188) dan mengabaikan `disabled` — card ini tampil hanya saat member, jadi tidak butuh guard permission untuk path services. `flag: "projects"` dan `isComplete("projects")` tetap seperti semula.

- [ ] **Step 3: Verifikasi lint + types**

Run: `pnpm --filter=web check:lint && pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/home/widgets/empty-states/no-projects.tsx
git commit -m "feat(dashboard): point quickstart CTA to services page when a project exists"
```

---

### Task 3: Build + verifikasi visual

- [ ] **Step 1: Build web**

Run: `pnpm --filter=web build`
Expected: build sukses.

- [ ] **Step 2: Restart prod service (tunnel demo)**

```bash
systemctl --user restart plane-web-prod.service
```

- [ ] **Step 3: Verifikasi visual manual**

Buka dashboard via tunnel/profile browser:

1. Workspace kosong (0 project): quickstart guide tampil, card 1 "Create your first project" + CTA "Get started" membuka create project modal.
2. Workspace ≥1 project & <2 member (atau guide belum tersembunyi): card 1 CTA "View services" mengarah ke `/[ws]/projects/<id>/services`.
3. Widget Recents: empty project copy sesuai string baru.
4. Locale non-`en` tidak berubah (diff hanya `en/home.json`).

---

## Self-review notes

- Spec coverage: 4 perubahan copy (Task 1), rewiring 2 jalur CTA (Task 2, sesuai logic `firstJoinedProjectId`), verifikasi build/restart/visual (Task 3) — semua bagian spec terpetakan.
- Placeholder scan: tidak ada TBD/TODO; semua langkah memuat konten konkret.
- Konsistensi: key baru `cta_view_services` dipakai konsisten antara Task 1 (definisi) dan Task 2 (konsumsi).
