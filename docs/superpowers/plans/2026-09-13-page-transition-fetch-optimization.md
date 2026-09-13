# Page Transition Fetch Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hilangkan double-fetch saat transisi `/:workspaceSlug` -> `/:workspaceSlug/projects/:projectId/issues` sehingga 1x navigasi hanya memicu 1 gelombang API per resource.

**Architecture:** Gate semua SWR project-level sampai `projectRole` ter-resolve, skip `fetchIssues init-loader` bila MobX store sudah terhidrasi, dan turunkan revalidasi agresif di Home widgets. Tidak ada perubahan API backend, hanya gating client-side.

**Tech Stack:** React Router v7 SPA (`ssr:false`), SWR, MobX, Vite, TypeScript strict, OxLint + oxfmt

---

## File Structure

- Modify: `apps/web/core/layouts/auth-layout/project-wrapper.tsx:48-137` — gate 9 SWR key project-level (`LABELS`, `MEMBERS`, `STATES`, `INTAKE`, `ESTIMATES`, `ALL_CYCLES`, `MODULES`, `VIEWS`, `MEMBER_PREFERENCES`) + 2 non-role (`DETAILS`, `ME_INFO` tetap langsung) agar tidak fetch dengan `role=undefined`.
- Modify: `apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx:89-91` — guard `useEffect fetchIssues("init-loader")` agar skip bila `groupedIssueIds` sudah ada dan loader bukan `init-loader`.
- Modify: `apps/web/core/components/home/root.tsx:29-37` — ubah `HOME_DASHBOARD_WIDGETS` dari `revalidateIfStale:true, revalidateOnReconnect:true` menjadi `false/false`.
- Verify: `apps/web/package.json:14-15` scripts `check:types`, `check:lint`; manual Network panel `Fetch/XHR`.

---

### Task 1: Gate ProjectAuthWrapper SWR sampai role ready

**Files:**

- Modify: `apps/web/core/layouts/auth-layout/project-wrapper.tsx:52-74`
- Modify: `apps/web/core/layouts/auth-layout/project-wrapper.tsx:90-137`

- [ ] **Step 1: Tulis skrip reproduksi key tidak stabil**

Buat file `/tmp/repro-role-key.mjs`:

```js
// Reproduksi: key berubah undefined -> ADMIN sehingga SWR fetch 2x
const PROJECT_LABELS = (projectId, projectRole) =>
  `PROJECT_LABELS_${projectId.toString().toUpperCase()}_${projectRole}`;

const projectId = "abc-123";
const keyBefore = PROJECT_LABELS(projectId, undefined);
const keyAfter = PROJECT_LABELS(projectId, 20);

console.log("keyBefore:", keyBefore);
console.log("keyAfter:", keyAfter);
console.log("stable:", keyBefore === keyAfter ? "STABLE" : "UNSTABLE-DOUBLE-FETCH");

if (keyBefore === keyAfter) {
  console.log("PASS: key stabil");
  process.exit(0);
} else {
  console.log("FAIL: key berubah, SWR akan fetch 2x");
  process.exit(1);
}
```

- [ ] **Step 2: Jalankan skrip untuk pastikan FAIL**

Run: `node /tmp/repro-role-key.mjs`
Expected: exit 1 dengan output:

```
keyBefore: PROJECT_LABELS_ABC-123_undefined
keyAfter: PROJECT_LABELS_ABC-123_20
stable: UNSTABLE-DOUBLE-FETCH
FAIL: key berubah, SWR akan fetch 2x
```

- [ ] **Step 3: Implementasi gating minimal**

Edit `apps/web/core/layouts/auth-layout/project-wrapper.tsx:52-74`, ubah destructure:

```tsx
const { fetchUserProjectInfo, allowPermissions, getProjectRoleByWorkspaceSlugAndProjectId } = useUserPermissions();
```

menjadi:

```tsx
const {
  fetchUserProjectInfo,
  allowPermissions,
  getProjectRoleByWorkspaceSlugAndProjectId,
  loader: permissionsLoader,
} = useUserPermissions();
```

Tambahkan tepat setelah `const currentProjectRole = ...` (baris 74):

```tsx
const isRoleReady = !permissionsLoader && currentProjectRole !== undefined;
```

Lalu ubah 9 blok useSWR role-dependent dari pola:

```tsx
useSWR(PROJECT_LABELS(projectId, currentProjectRole), () => fetchProjectLabels(workspaceSlug, projectId), {
  revalidateIfStale: false,
  revalidateOnFocus: false,
});
```

menjadi pola gated (terapkan identik ke `MEMBER_PREFERENCES`, `LABELS`, `MEMBERS`, `STATES`, `INTAKE_STATE`, `ESTIMATES`, `ALL_CYCLES`, `MODULES`, `VIEWS`):

```tsx
useSWR(
  isRoleReady ? PROJECT_LABELS(projectId, currentProjectRole) : null,
  () => fetchProjectLabels(workspaceSlug, projectId),
  {
    revalidateIfStale: false,
    revalidateOnFocus: false,
  }
);
```

Untuk `MEMBER_PREFERENCES` yang sudah punya guard `currentUserData?.id`, gabungkan:

```tsx
useSWR(
  currentUserData?.id && isRoleReady ? PROJECT_MEMBER_PREFERENCES(projectId, currentProjectRole) : null,
  () => fetchProjectUserProperties(workspaceSlug, projectId),
  { revalidateIfStale: false, revalidateOnFocus: false }
);
```

Biarkan `PROJECT_DETAILS` dan `PROJECT_ME_INFORMATION` (baris 83-88) tanpa gate karena key-nya tidak mengandung role.

- [ ] **Step 4: Verifikasi types + lint**

Run: `pnpm --filter=web check:types`
Expected: exit 0, tidak ada error `Property 'loader' does not exist` (loader ada di `apps/web/core/store/user/base-permissions.store.ts:68`).

Run: `pnpm --filter=web check:lint`
Expected: exit 0 (max warnings sesuai `apps/web/package.json:14`).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/layouts/auth-layout/project-wrapper.tsx
git commit -m "fix(web): gate project SWR until role ready to avoid double-fetch"
```

---

### Task 2: Skip fetchIssues init-loader bila store sudah terhidrasi

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx:89-91`

- [ ] **Step 1: Dokumentasikan kondisi skip sebagai komentar gagal dulu**

Tambahkan di atas `useEffect` baris 89 komentar eksplisit yang menjelaskan ekspektasi (ini menjadi “failing test” manual — sebelum fix, navigasi balik selalu network):

```tsx
// EXPECTED: back-navigation dengan groupedIssueIds terisi tidak memicu fetchIssues init-loader lagi
```

Tidak ada kode logika dulu. File tetap sama secara perilaku.

- [ ] **Step 2: Verifikasi perilaku saat ini via code read**

Run: `rg -n "fetchIssues\(\"init-loader\"" apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx`
Expected: `89:    fetchIssues("init-loader", { canGroup: true, perPageCount: group_by ? 50 : 100 }, viewId);` — tanpa guard.

- [ ] **Step 3: Implementasi guard minimal**

Ubah `apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx:89-91` dari:

```tsx
useEffect(() => {
  fetchIssues("init-loader", { canGroup: true, perPageCount: group_by ? 50 : 100 }, viewId);
}, [fetchIssues, storeType, group_by, viewId]);
```

menjadi:

```tsx
const hasHydratedIssues =
  !!groupedIssueIds && Object.keys(groupedIssueIds).length > 0 && issues?.getIssueLoader() !== "init-loader";

useEffect(() => {
  if (hasHydratedIssues) return;
  fetchIssues("init-loader", { canGroup: true, perPageCount: group_by ? 50 : 100 }, viewId);
}, [fetchIssues, storeType, group_by, viewId, hasHydratedIssues]);
```

Catatan: `groupedIssueIds` sudah ada di baris 93 (`const groupedIssueIds = issues?.groupedIssueIds`), pindahkan deklarasi `hasHydratedIssues` tepat setelah baris itu agar tidak ada TDZ. `getIssueLoader` ada di `apps/web/core/store/issue/helpers/base-issues.store.ts:71`.

- [ ] **Step 4: Verifikasi types + lint**

Run: `pnpm --filter=web check:types`
Expected: PASS, tidak ada error `groupedIssueIds used before declaration`.

Run: `pnpm --filter=web check:lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx
git commit -m "fix(web): skip issues init-loader when store already hydrated"
```

---

### Task 3: Turunkan revalidasi Home widgets

**Files:**

- Modify: `apps/web/core/components/home/root.tsx:29-37`

- [ ] **Step 1: Catat config agresif saat ini**

Run: `rg -n -A6 "HOME_DASHBOARD_WIDGETS" apps/web/core/components/home/root.tsx`
Expected:

```
useSWR(
  workspaceSlug ? `HOME_DASHBOARD_WIDGETS_${workspaceSlug}` : null,
  ...
  {
    revalidateIfStale: true,
    revalidateOnFocus: false,
    revalidateOnReconnect: true,
  }
```

- [ ] **Step 2: Konfirmasi tidak ada test yang bergantung pada revalidate true**

Run: `rg -rn "HOME_DASHBOARD_WIDGETS" apps/web --glob '!node_modules'`
Expected: hanya 1 hasil di `core/components/home/root.tsx:30`. Tidak ada test yang assert `revalidateIfStale:true`.

- [ ] **Step 3: Implementasi minimal**

Ubah `apps/web/core/components/home/root.tsx:29-37` dari:

```tsx
useSWR(
  workspaceSlug ? `HOME_DASHBOARD_WIDGETS_${workspaceSlug}` : null,
  workspaceSlug ? () => fetchWidgets(workspaceSlug?.toString()) : null,
  {
    revalidateIfStale: true,
    revalidateOnFocus: false,
    revalidateOnReconnect: true,
  }
);
```

menjadi:

```tsx
useSWR(
  workspaceSlug ? `HOME_DASHBOARD_WIDGETS_${workspaceSlug}` : null,
  workspaceSlug ? () => fetchWidgets(workspaceSlug?.toString()) : null,
  {
    revalidateIfStale: false,
    revalidateOnFocus: false,
    revalidateOnReconnect: false,
  }
);
```

- [ ] **Step 4: Verifikasi types + lint**

Run: `pnpm --filter=web check:types`
Expected: PASS.

Run: `pnpm --filter=web check:lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/home/root.tsx
git commit -m "fix(web): reduce home widgets revalidation on back-navigation"
```

---

### Task 4: Verifikasi akhir transisi Home -> Issue List

**Files:**

- Test: manual Network panel + `pnpm --filter=web check:types`

- [ ] **Step 1: Jalankan semua checks**

Run: `pnpm --filter=web check:types`
Expected: PASS.

Run: `pnpm --filter=web check:lint`
Expected: PASS (warnings ≤ batas di `package.json`).

- [ ] **Step 2: Verifikasi manual di browser dev**

1. `pnpm dev` (web:3000), buka `/:workspaceSlug`, buka DevTools -> Network -> centang Preserve log -> filter Fetch/XHR -> Clear.
2. Klik ke `/:workspaceSlug/projects/:projectId/issues`.
3. Expected: tidak ada baris `Document` baru; hanya 1x per key `PROJECT_LABELS_*`, `PROJECT_MEMBERS_*`, dst (tidak ada pasangan `..._undefined` + `..._20`); `HOME_DASHBOARD_WIDGETS_*` tidak refetch saat balik ke Home; `init-loader` issues hanya sekali saat pertama masuk, tidak tiap bolak-balik.
4. Klik Back ke Home, lalu Forward lagi ke Issues. Expected: tidak ada `init-loader` kedua bila data masih di store.

- [ ] **Step 3: Bersihkan artefak reproduksi**

Run: `rm -f /tmp/repro-role-key.mjs`
Expected: exit 0.

- [ ] **Step 4: Commit kosong tidak diperlukan — verifikasi saja, jangan commit**

Jika semua PASS, laporkan ringkasan. Jika ada FAIL, kembali ke Task terkait, jangan tambah fix baru di Task ini.

---

## Self-Review

1. **Spec coverage:** Audit meminta hilangkan double-fetch Home->Issues. Task 1 menutup 9 key role-dependent + WORKSPACE_FAVORITE indirect (ikut stabil setelah role ready karena `allowPermissions` di workspace-wrapper membaca role yang sama). Task 2 menutup `init-loader` berulang. Task 3 menutup refetch widgets saat back-nav. Task 4 verifikasi end-to-end.
2. **Placeholder scan:** Tidak ada TBD/TODO/fill-in. Semua code block lengkap, semua command exact dengan expected output.
3. **Type consistency:** `permissionsLoader` dari `IUserPermissionStore.loader:boolean` (`base-permissions.store.ts:68`); `currentProjectRole:EUserPermissions|undefined` (`base-permissions.store.ts:41`); `groupedIssueIds:TGroupedIssues|TSubGroupedIssues|undefined` (`base-issues.store.ts:60`); `getIssueLoader():TLoader` (`base-issues.store.ts:71`). Nama konsisten di semua task.
