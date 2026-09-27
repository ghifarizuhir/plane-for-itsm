# Work Item Types/Workflows Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menuntaskan follow-up kecil pasca rilis Work Item Types & Workflows: race state store, label settings, i18n register, dead prop, retry fetch map, error map-refresh, denominator persentase, chips typed, copy empty-state, dan hapus halaman toggle Features yang duplikat/inert.

**Architecture:** Perbaikan terisolasi per file tanpa perubahan skema. State/Workflow store memakai pola epoch/flag yang sudah ada di `workflow.store.ts`; i18n mengikuti skill translate; halaman toggle Features dihapus karena flag `is_issue_type_enabled` tidak dibaca backend dan labelnya duplikat dengan halaman Work structure.

**Tech Stack:** React 18 + MobX, TypeScript, vitest, i18n JSON (20 locale), React Router v7, oxlint/oxfmt.

**Prasyarat & aturan repo:**

- Kerjakan di branch `preview`. Working tree punya ~20 file branding termodifikasi pre-existing — jangan `git add -A`; stage hanya file per task.
- Gate standar: `pnpm --filter=web check:types`, `pnpm --filter=web check:lint`, `pnpm --filter=web test`.
- Setelah perubahan web yang harus tampil di tunnel: `pnpm --filter=web build && systemctl --user restart plane-web-prod.service`.
- Baseline i18n (jangan bertambah): `pnpm --filter=@plane/i18n sync:check` — drift saat ini 18 missing / 32 stale per locale.
- Task B4 WAJIB memuat skill `translate` (`skill` tool) sebelum menyentuh file locale.

---

## File Structure

- `apps/web/core/store/state.store.ts` (+ `.test.ts`) — epoch race + persentase typed.
- `apps/web/core/store/workflow.store.ts` (+ `.test.ts`) — flag kegagalan refresh map.
- `apps/web/core/components/issues/issue-modal/components/default-properties.tsx` — retry lazy fetch.
- `apps/web/core/components/project-work-item-types/root.tsx` — toast kegagalan refresh map.
- `apps/web/core/components/project-states/root.tsx` — chips typed per type.
- `apps/web/core/components/workflows/state-list.tsx`, `.../transition-matrix.tsx` — copy empty-state.
- `apps/web/core/components/dropdowns/state/base.tsx`, `.../intake-state/base.tsx`, `.../workflow/state-option.tsx` — hapus prop mati.
- `packages/constants/src/settings/workspace.ts`, `project.ts` — highlight path + label i18n + hapus entri toggle.
- `apps/web/app/routes/core.ts` + direktori `.../features/work-item-types/` — hapus route/halaman toggle.
- `packages/i18n/src/locales/*` — register sweep.
- `docs/superpowers/plans/2026-09-25-work-item-types-workflows-web.md` — perbaiki defect dokumentasi plan lama.

---

### Task B1: Epoch race di `StateStore.fetchProjectStates`

**Files:**

- Modify: `apps/web/core/store/state.store.ts:63-96` (field + makeObservable), `:219-232` (fetch), `:272-373` (mutations)
- Test: `apps/web/core/store/state.store.test.ts`

Masalah: GET yang dimulai sebelum `createState`/`updateState` bisa men-prune/menimpa state baru saat responsnya tiba (`:222-228`).

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `describe("StateStore.fetchProjectStates")`:

```ts
it("tidak men-prune state yang dibuat saat GET masih in-flight", async () => {
  const store = makeStore();
  store.stateMap = {};
  let resolveGet!: (states: IState[]) => void;
  store.stateService = {
    getStates: vi.fn(() => new Promise<IState[]>((resolve) => (resolveGet = resolve))),
    createState: vi.fn(async () => makeState("s-new", "p-1")),
  } as never;

  const fetchPromise = store.fetchProjectStates("acme", "p-1");
  await store.createState("acme", "p-1", { name: "New" } as Partial<IState>);
  resolveGet([makeState("s-old", "p-1")]);
  await fetchPromise;

  expect(store.stateMap["s-new"]).toBeDefined();
  expect(store.stateMap["s-old"]).toBeDefined();
});
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test state.store`
Expected: FAIL — `s-new` terhapus oleh prune.

- [ ] **Step 3: Implementasi**

Di `apps/web/core/store/state.store.ts`, dekat field privat `stateService`:

```ts
  // epoch per-project: mutasi menaikkan nilai ini supaya GET basi tidak
  // men-prune/menimpa state yang baru dibuat/diubah saat request in-flight.
  private stateRequestEpoch: Record<string, number> = {};

  private bumpStateEpoch = (projectId: string) => {
    this.stateRequestEpoch[projectId] = (this.stateRequestEpoch[projectId] ?? 0) + 1;
  };
```

`fetchProjectStates` (`:219-232`):

```ts
fetchProjectStates = async (workspaceSlug: string, projectId: string) => {
  const epoch = this.stateRequestEpoch[projectId] ?? 0;
  const statesResponse = await this.stateService.getStates(workspaceSlug, projectId);
  runInAction(() => {
    // mutasi lokal terjadi setelah GET ini dimulai → jangan prune/timpa
    if ((this.stateRequestEpoch[projectId] ?? 0) !== epoch) return;
    const fetchedStateIds = new Set(statesResponse.map((state) => state.id));
    Object.values(this.stateMap).forEach((state) => {
      if (state.project_id === projectId && !fetchedStateIds.has(state.id)) delete this.stateMap[state.id];
    });
    statesResponse.forEach((state) => {
      set(this.stateMap, [state.id], state);
    });
    set(this.fetchedMap, projectId, true);
  });
  return statesResponse;
};
```

Panggil `this.bumpStateEpoch(projectId)` di awal setiap mutasi: `createState` (`:272-278`), `updateState` (`:288`), `deleteState` (`:313`), `markStateAsDefault` (`:329`), `moveStatePosition` (`:357`).

- [ ] **Step 4: Jalankan, pastikan lulus**

Run: `pnpm --filter=web test state.store`
Expected: PASS (3 test).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/state.store.ts apps/web/core/store/state.store.test.ts
git commit -m "fix(web): guard state store against stale in-flight fetches"
```

---

### Task B2: Bersihkan highlight path settings + label i18n + defect plan lama

**Files:**

- Modify: `packages/constants/src/settings/workspace.ts:61-74`
- Modify: `packages/constants/src/settings/project.ts:96-102`
- Modify: `docs/superpowers/plans/2026-09-25-work-item-types-workflows-web.md:475,482,1084`

- [ ] **Step 1: Perbaiki highlight workspace settings**

Di `packages/constants/src/settings/workspace.ts` (entri `work_item_types` + `workflows`) ubah:

```ts
    highlight: (pathname: string, baseUrl: string) => pathname === `${baseUrl}/settings/work-item-types/`,
```

```ts
    highlight: (pathname: string, baseUrl: string) => pathname.startsWith(`${baseUrl}/settings/workflows/`),
```

- [ ] **Step 2: Perbaiki `i18n_label` entri project**

Di `packages/constants/src/settings/project.ts` entri `work_item_types` (`:96-102`), ganti `i18n_label: "workspace_settings.settings.work_item_types.title"` agar memakai namespace project-settings:

```ts
    i18n_label: "project_settings.work_item_types.heading",
```

Pakai key **yang sudah ada** `project_settings.work_item_types.heading` (tersedia di 20 locale — diverifikasi), JANGAN menambah key `title` baru karena akan menaikkan drift i18n. Jalankan `pnpm --filter=@plane/i18n sync:check` untuk memastikan drift tidak bertambah.

- [ ] **Step 3: Perbaiki defect plan lama**

Di `docs/superpowers/plans/2026-09-25-work-item-types-workflows-web.md`:

- baris 475 → `${baseUrl}/settings/work-item-types/`
- baris 482 → `startsWith(\`${baseUrl}/settings/workflows/\`)`
- baris 1084 → hapus `packages/i18n/src/types/keys.generated.ts` dari `git add` (file gitignored, `.gitignore:117`); sisakan `git add packages/i18n/src/locales`.

- [ ] **Step 4: Verifikasi**

Run: `pnpm --filter=web check:types && pnpm --filter=@plane/i18n sync:check`
Expected: types exit 0; drift tidak bertambah dari baseline (18 missing / 32 stale).

- [ ] **Step 5: Commit**

```bash
git add packages/constants/src/settings/workspace.ts packages/constants/src/settings/project.ts packages/i18n docs/superpowers/plans/2026-09-25-work-item-types-workflows-web.md
git commit -m "fix(web): correct settings highlight paths and plan defects"
```

---

### Task B3: Hapus halaman toggle Features "Work item types" (duplikat & inert)

**Files:**

- Modify: `packages/constants/src/settings/project.ts:82-88` (entri `features_work_item_types`)
- Modify: `apps/web/core/components/settings/project/sidebar/item-icon.tsx` (mapping ikon entri tsb)
- Modify: `apps/web/app/routes/core.ts` (route halaman)
- Delete: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/features/work-item-types/` (page.tsx + header.tsx)

Konteks: flag `is_issue_type_enabled` tidak dibaca backend mana pun (hanya halaman toggle itu sendiri), dan labelnya sama dengan halaman Work structure → Work item types. Kontrol enable type per project tetap satu-satunya di halaman Work structure. Key i18n `project_settings.features.work_item_types.*` dibiarkan (dibersihkan terpisah bila perlu).

- [ ] **Step 1: Discovery referensi**

```bash
rg -n "features/work-item-types|features_work_item_types" apps/web packages --glob '!**/node_modules/**'
```

Catat semua hit (settings entry, ikon, route, page, header, link lain).

- [ ] **Step 2: Hapus entri + ikon + route + halaman**

- Hapus objek entri `features_work_item_types` di `packages/constants/src/settings/project.ts` (dan dari agregat `PROJECT_SETTINGS` bila didaftarkan eksplisit).
- Hapus key ikon terkait di `apps/web/core/components/settings/project/sidebar/item-icon.tsx`.
- Hapus entri route di `apps/web/app/routes/core.ts` yang menunjuk `features/work-item-types/page.tsx`.
- Hapus direktori `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/features/work-item-types/`.

- [ ] **Step 3: Verifikasi tidak ada referensi tersisa + gate**

```bash
rg -n "features/work-item-types|features_work_item_types" apps/web packages --glob '!**/node_modules/**'
pnpm --filter=web check:types && pnpm --filter=web check:lint
```

Expected: `rg` kosong (selain i18n keys); types/lint exit 0.

- [ ] **Step 4: Commit**

```bash
git add packages/constants/src/settings/project.ts packages/types/src/settings.ts apps/web/core/components/settings/project/sidebar/item-icon.tsx apps/web/app/routes/core.ts
git rm -r "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/features/work-item-types"
git commit -m "chore(web): remove inert work item types feature toggle page"
```

---

### Task B4: Sweep register i18n (ro + es)

**Files:**

- Modify: `packages/i18n/src/locales/ro/workspace-settings.json` (baris 491, 495, 504, 510, 522, 544)
- Modify: `packages/i18n/src/locales/es/workspace-settings.json` (baris 518)
- Referensi: `.claude/skills/translate/SKILL.md:349-367`

- [ ] **Step 1: Muat skill translate**

Panggil `skill` tool dengan nama `translate`, lalu baca `.claude/skills/translate/SKILL.md` (terutama tabel per-locale register: ro = `dumneavoastră`/impersonal, es = usted/ustedes).

- [ ] **Step 2: Perbaiki inkonsistensi nyata**

- `ro/workspace-settings.json` — 6 key fitur ini memakai "tu" padahal blok fitur yang sama di `ro/project-settings.json:413-419` memakai formal plural. Ubah key berikut ke formal plural/impersonal (ikuti skill):
  - `settings.work_item_types.description` (491)
  - `settings.work_item_types.empty_state.description` (495)
  - `settings.work_item_types.attach_workflow_first` (504)
  - `settings.workflows.description` (510)
  - `settings.workflows.empty_state.description` (522)
  - `settings.workflows.transitions.description` (544)
- `es/workspace-settings.json:518` `workflows.not_found.description` memakai 3rd-person formal di tengah blok "tú" — samakan register dengan string tetangga di blok itu.

Catat di deskripsi PR: konversi file-wide register es/it/pl (yang mengikuti konvensi informal file masing-masing) **ditunda** sebagai `chore(i18n)` terpisah agar satu file konsisten; jangan hanya mengubah string fitur ini.

- [ ] **Step 3: Regenerasi tipe + cek drift**

```bash
pnpm --filter=@plane/i18n generate:types
pnpm --filter=@plane/i18n sync:check
```

Expected: drift tidak bertambah dari baseline (18 missing / 32 stale per locale).

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/ro/workspace-settings.json packages/i18n/src/locales/es/workspace-settings.json
git commit -m "fix(i18n): align ro and es work item type strings with locale register"
```

---

### Task B5: Hapus prop mati `alwaysAllowStateChange`

**Files:**

- Modify: `apps/web/core/components/dropdowns/state/base.tsx:33`
- Modify: `apps/web/core/components/dropdowns/intake-state/base.tsx:30`
- Modify: `apps/web/core/components/workflow/state-option.tsx:23`

- [ ] **Step 1: Discovery (pastikan nol pemakai)**

```bash
rg -n "alwaysAllowStateChange" apps/web packages --glob '!**/node_modules/**'
```

Expected: hanya 3 deklarasi tipe (tidak ada yang mengoper/membaca).

- [ ] **Step 2: Hapus tiga deklarasi**

Hapus baris field pada ketiga file di atas. Forward `{...props}` yang tersisa tidak masalah.

- [ ] **Step 3: Verifikasi**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && rg -n "alwaysAllowStateChange" apps/web packages --glob '!**/node_modules/**'`
Expected: types/lint exit 0; `rg` kosong.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/dropdowns/state/base.tsx apps/web/core/components/dropdowns/intake-state/base.tsx apps/web/core/components/workflow/state-option.tsx
git commit -m "chore(web): remove unused alwaysAllowStateChange prop"
```

---

### Task B6: Retry lazy fetch workflow map di create modal

**Files:**

- Modify: `apps/web/core/components/issues/issue-modal/components/default-properties.tsx:71-95`

Masalah: saat `fetchWorkflowMap` gagal, ref dihapus (`:94`) tapi tidak ada yang memicu effect ulang (dependency stabil) → dropdown Type hilang sampai remount.

- [ ] **Step 1: Implementasi retry terbatas**

Tambahkan state + retry:

```tsx
const [workflowMapRetry, setWorkflowMapRetry] = useState(0);
```

Di dalam effect (`:90-95`):

```tsx
useEffect(() => {
  if (!projectId || !workspaceSlug || getWorkflowMap(projectId) !== undefined) return;
  if (requestedWorkflowMaps.current.has(projectId)) return;
  requestedWorkflowMaps.current.add(projectId);
  void fetchWorkflowMap(workspaceSlug, projectId).catch(() => {
    requestedWorkflowMaps.current.delete(projectId);
    // retry terbatas (maks 2) supaya kegagalan sesaat tidak menghilangkan
    // dropdown Type sampai modal di-remount
    if (workflowMapRetry < 2) setTimeout(() => setWorkflowMapRetry((value) => value + 1), 1500);
  });
}, [fetchWorkflowMap, getWorkflowMap, projectId, workspaceSlug, workflowMapRetry]);
```

Pastikan `useState` sudah diimpor di file itu.

- [ ] **Step 2: Verifikasi**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: exit 0. Smoke: matikan api-rs sesaat → buka create modal → nyalakan lagi → dropdown Type muncul dalam ≤3 detik tanpa remount.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/issues/issue-modal/components/default-properties.tsx
git commit -m "fix(web): retry workflow map fetch in create form"
```

---

### Task B7: Surface kegagalan refresh workflow-map setelah mutasi type

**Files:**

- Modify: `apps/web/core/store/workflow.store.ts:75-90` (observable), `:285-292` (refresh)
- Modify: `apps/web/core/components/project-work-item-types/root.tsx:79-90` (toast)
- Test: `apps/web/core/store/workflow.store.test.ts`

Konteks: `refreshWorkflowMaps` menelan error (`.catch(() => undefined)`, `:290`), jadi enable/disable type bisa "sukses" tapi daftar type/map basi tanpa peringatan.

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `workflow.store.test.ts`:

```ts
describe("WorkflowStore.map refresh failures", () => {
  it("mencatat kegagalan refresh map tanpa menggagalkan mutasi", async () => {
    const { store, service } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    service.getWorkflowMap.mockRejectedValueOnce(new Error("boom"));

    await expect(store.importWorkItemTypes("acme", "p-1", ["type-1"])).resolves.toBeUndefined();
    expect(store.mapRefreshError["p-1"]).toBeTruthy();
  });

  it("membersihkan flag setelah refresh sukses", async () => {
    const { store } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    store.mapRefreshError["p-1"] = "failed";

    await store.importWorkItemTypes("acme", "p-1", ["type-1"]);
    expect(store.mapRefreshError["p-1"]).toBeNull();
  });
});
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test workflow.store`
Expected: FAIL — `mapRefreshError` belum ada.

- [ ] **Step 3: Implementasi store**

Di `workflow.store.ts` tambahkan observable + daftarkan di `makeObservable`:

```ts
// pesan kegagalan refresh map per project (dibaca UI untuk toast peringatan)
mapRefreshError: Record<string, string | null> = {};
```

dan di `makeObservable` blok observables:

```ts
      mapRefreshError: observable,
```

Ganti `refreshWorkflowMaps` (`:285-292`):

```ts
  private refreshWorkflowMaps = async (workspaceSlug: string, projectIds?: string[]) => {
    const targets = projectIds
      ? projectIds.filter((projectId) => this.workflowMap[projectId] !== undefined)
      : Object.keys(this.workflowMap);
    await Promise.all(
      targets.map(async (projectId) => {
        try {
          await this.fetchWorkflowMap(workspaceSlug, projectId);
          runInAction(() => {
            this.mapRefreshError[projectId] = null;
          });
        } catch {
          runInAction(() => {
            this.mapRefreshError[projectId] = "refresh_failed";
          });
        }
      })
    );
  };
```

- [ ] **Step 4: Toast di UI enable/disable**

Di `apps/web/core/components/project-work-item-types/root.tsx` (blok aksi toggle, `:79-90`), setelah `await importWorkItemTypes(...)` / `await unlinkWorkItemType(...)` sukses, periksa flag dan tampilkan toast peringatan non-fatal:

```tsx
const refreshFailed = useWorkflow().mapRefreshError[projectId];
if (refreshFailed) {
  setToast({
    type: TOAST_TYPE.WARNING,
    title: t("common.warning"),
    message: t("workspace_settings.settings.work_item_types.map_refresh_failed"),
  });
}
```

Tambahkan key `map_refresh_failed` **hanya di `en`** (`packages/i18n/src/locales/en/workspace-settings.json`):

```json
"map_refresh_failed": "The type was saved, but the project workflow list could not be refreshed. Reload the page to see the latest state."
```

Lokalisasi key ini ke 19 locale lain digabung dalam satu pass terjemahan di Task B10 (jangan menambah ke locale lain di task ini). Konsekuensinya drift sementara naik 1 missing per locale non-en; B10 mengembalikannya ke baseline.

Gunakan hook `useWorkflow()` di scope komponen (bukan memanggilnya di dalam callback berulang); baca flag via variabel yang sudah di-destructure.

- [ ] **Step 5: Jalankan, pastikan lulus + gate**

Run: `pnpm --filter=web test workflow.store && pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS; test lama "tidak gagal saat refresh state gagal" tetap lulus (perilaku store tidak berubah, hanya mencatat flag).

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/store/workflow.store.ts apps/web/core/store/workflow.store.test.ts apps/web/core/components/project-work-item-types/root.tsx packages/i18n/src/locales/en/workspace-settings.json
git commit -m "fix(web): surface workflow map refresh failures"
```

---

### Task B8: Denominator persentase state mengabaikan typed mirror

**Files:**

- Modify: `apps/web/core/store/state.store.ts:380-396`
- Test: `apps/web/core/store/state.store.test.ts`

Masalah: `getStatePercentageInGroup` menghitung `this.groupedProjectStates[group]` yang masih memuat state mirror typed → pie icon state legacy terdilusi.

- [ ] **Step 1: Tulis test yang gagal**

```ts
describe("StateStore.getStatePercentageInGroup", () => {
  it("mengabaikan typed mirror state di denominator", () => {
    const store = makeStore();
    // `groupedProjectStates` digate oleh fetchedMap; tanpa ini daftar kosong
    // dan test gagal karena alasan yang salah.
    store.fetchedMap["p-1"] = true;
    store.stateMap = {
      "s-1": { ...makeState("s-1", "p-1"), sequence: 1, order: 1 },
      "s-2": { ...makeState("s-2", "p-1"), sequence: 2, order: 2 },
      "s-mirror": { ...makeState("s-mirror", "p-1"), sequence: 3, order: 3, type_id: "type-1" },
    };

    expect(store.getStatePercentageInGroup("s-1")).toBe(50);
    expect(store.getStatePercentageInGroup("s-2")).toBe(100);
  });
});
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test state.store`
Expected: FAIL — hasil 33.33/66.66 karena mirror ikut dihitung.

- [ ] **Step 3: Implementasi**

Di `state.store.ts`, impor helper dan filter:

```ts
import { isTypedState } from "./workflow.helpers";
```

lalu di `getStatePercentageInGroup` (`:389`):

```ts
// typed mirror states dikelola di halaman terpisah; jangan dilusikan ke
// persentase state legacy (list legacy juga memfilternya di UI).
const statesInGroup = this.groupedProjectStates[group].filter((state) => !isTypedState(state));
if (statesInGroup.length === 0) return -1;
```

- [ ] **Step 4: Jalankan, pastikan lulus**

Run: `pnpm --filter=web test state.store`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/state.store.ts apps/web/core/store/state.store.test.ts
git commit -m "fix(web): exclude typed states from legacy state percentage"
```

---

### Task B9: Chips state typed dikelompokkan per type

**Files:**

- Modify: `apps/web/core/components/project-states/root.tsx:91-105`

Masalah: `typedStates` dirender sebagai daftar datar, sehingga state bernama sama dari type berbeda tampak duplikat tanpa konteks.

Sekaligus konsistensi namespace: heading di `:93` masih memakai `workspace_settings.settings.work_item_types.title` dan deskripsi di `:94-95` memakai `.description` namespace yang sama; ganti keduanya ke namespace project — `project_settings.work_item_types.heading` dan `project_settings.work_item_types.description` (kedua key sudah ada di 20 locale).

- [ ] **Step 1: Kelompokkan per type**

Di `project-states/root.tsx` tambahkan hook workflow map + grouping:

```tsx
const { getWorkflowMap } = useWorkflow();
const workflowMap = getWorkflowMap(projectId);
const typedStateGroups = useMemo(() => {
  const groups = new Map<string, IState[]>();
  typedStates.forEach((state) => {
    const key = state.type_id ?? "untyped";
    groups.set(key, [...(groups.get(key) ?? []), state]);
  });
  return [...groups.entries()];
}, [typedStates]);
```

Ganti render chip (`:97-103`) dengan per-group:

```tsx
<div className="flex flex-col gap-3">
  {typedStateGroups.map(([typeId, states]) => (
    <div key={typeId} className="flex flex-col gap-1">
      <span className="text-caption-md-medium text-tertiary">
        {workflowMap?.types.find((type) => type.type_id === typeId)?.type_name ?? ""}
      </span>
      <div className="flex flex-wrap gap-2">
        {states.map((state) => (
          <span key={state.id} className="rounded border border-subtle px-2 py-1 text-caption-md-medium">
            {state.name}
          </span>
        ))}
      </div>
    </div>
  ))}
</div>
```

Tambahkan impor `useWorkflow` (`@/hooks/store/use-workflow`).

- [ ] **Step 2: Verifikasi**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: exit 0. Smoke: project dengan 2 type aktif → chip terkelompok dengan nama type masing-masing.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/project-states/root.tsx
git commit -m "fix(web): group typed states by work item type"
```

---

### Task B10: Copy empty-state khusus di editor workflow

**Files:**

- Modify: `apps/web/core/components/workflows/state-list.tsx:186-189`
- Modify: `apps/web/core/components/workflows/transition-matrix.tsx:91-94`
- Modify: `packages/i18n/src/locales/*/workspace-settings.json` (key baru)

Masalah: keduanya memakai `common.no_items_in_this_group` ("No items in this group") padahal konteksnya daftar state workflow.

- [ ] **Step 1: Tambah key i18n (via skill translate)**

Muat skill `translate`, lalu tambahkan di `workspace_settings.settings.workflows` untuk `en`:

```json
"no_states": "No states yet",
"no_transitions": "Add states first to configure transitions",
```

Sekaligus lokalisasikan key `workspace_settings.settings.work_item_types.map_refresh_failed` (yang di task B7 baru ditambahkan di `en`) ke 19 locale lain dalam pass yang sama.

Lalu terjemahkan ke seluruh locale mengikuti alur skill (generate types + sync check; drift tidak bertambah).

- [ ] **Step 2: Pakai key di komponen**

`state-list.tsx:186-189`:

```tsx
<div className="flex items-center justify-center py-8 text-13 text-tertiary">
  {t("workspace_settings.settings.workflows.no_states")}
</div>
```

`transition-matrix.tsx:91-94`:

```tsx
<div className="flex items-center justify-center py-8 text-13 text-tertiary">
  {t("workspace_settings.settings.workflows.no_transitions")}
</div>
```

- [ ] **Step 3: Verifikasi**

Run: `pnpm --filter=web check:types && pnpm --filter=@plane/i18n sync:check`
Expected: types exit 0; drift tidak bertambah.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/workflows/state-list.tsx apps/web/core/components/workflows/transition-matrix.tsx packages/i18n/src/locales
git commit -m "fix(web): dedicated empty state copy for workflow editor"
```

---

### Task B11: Gate akhir + kesiapan PR

**Files:** tidak ada perubahan kode (kecuali temuan).

- [ ] **Step 1: Gate lengkap**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web test
pnpm --filter=@plane/i18n sync:check
```

Expected: types/lint exit 0; semua test lulus; drift i18n ≤ baseline.

- [ ] **Step 2: Build + restart prod web**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Tunggu ~10 detik; `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000/` → 200.

- [ ] **Step 3: Checklist PR (saat membuat PR nanti)**

- Deskripsi PR menandai string hasil mesin: `machine_translated, needs native review` untuk locale low-resource (id, vi-VN, ua, ro, ka-ge) — ikuti skill translate.
- Catat follow-up terpisah: konversi register file-wide es/it/pl; penambahan filter type di workspace-level views (my issues/profile) di luar scope.

- [ ] **Step 4: Laporan status**

Rangkum: task selesai, bukti gate, dan item yang sengaja ditunda. Jangan commit file branding pre-existing.

---

## Self-Review

- **Spec coverage:** race store (B1), highlight/label/plan defects (B2), label duplikat → hapus toggle (B3), i18n register (B4), dead prop (B5), retry lazy fetch (B6), silent refresh failure (B7), denominator typed (B8), chips typed (B9), copy empty-state (B10), PR readiness (B11). Item "seed Request Workflow"/docs sengaja di luar plan (dikerjakan user sebagai smoke test).
- **Placeholder scan:** tidak ada TBD; semua step berisi kode/komando konkret. Task B4/B10 mendelegasikan isi terjemahan ke skill translate (bukan placeholder — aturan register/CLDR ada di skill).
- **Type consistency:** `stateRequestEpoch`/`bumpStateEpoch`, `mapRefreshError`, `getStatePercentageInGroup`, `typedStateGroups` konsisten antar step; key i18n baru dipakai persis di komponen.

## Risiko

- B3 menghapus route/halaman: pastikan tidak ada link sidebar lain (langkah discovery) dan user sudah setuju flag inert.
- B7 mengubah observable store: test lama yang mem-pin perilaku "swallow" tetap dipertahankan karena mutasi tetap resolve; hanya flag baru yang diuji.
