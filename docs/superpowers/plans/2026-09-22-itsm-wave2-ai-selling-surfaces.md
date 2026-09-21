# ITSM Wave 2B — AI Galileo + Penghapusan Surface Jualan — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Seragamkan branding AI ke "Galileo", perbaiki copy AI yang tidak akurat, dan hapus seluruh surface jualan (billing, license/upgrade, badge "Pro", upsell active-cycles, banner bulk-ops) beserta key i18n yang menjadi mati.

**Architecture:** Semua perubahan frontend (kecuali satu string error di `apps/api-rs/crates/api/src/routes/ai.rs`). Tidak ada fitur yang dibuka/dikunci ulang — hanya UI upsell dan dead code yang dihapus. Key i18n dihapus dari locale `en` saja; locale lain menjadi stale sampai wave translate.

**Tech Stack:** React 19 + React Router (apps/web), TypeScript strict, OxLint/oxfmt, pnpm workspace, i18n JSON (`packages/i18n`), Rust string di `apps/api-rs`.

**Prasyarat:** Baseline pra-existing yang BUKAN gate: `pnpm --filter=web check:types` gagal di `core/components/workspace/settings/members-list.tsx:66` (`toSorted`); `pnpm --filter=@plane/i18n check:sync` gagal karena key _missing_ pra-existing di locale lain. Jangan perbaiki keduanya di plan ini.

---

## File Structure

| File                                                                            | Aksi   | Tanggung jawab                                     |
| ------------------------------------------------------------------------------- | ------ | -------------------------------------------------- |
| `packages/constants/src/ai.ts`                                                  | Modify | Loading text Galileo                               |
| `apps/web/core/components/pages/editor/ai/menu.tsx`                             | Modify | Loading text editor Galileo                        |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`                        | Modify | Copy panel Galileo                                 |
| `apps/web/core/components/core/modals/gpt-assistant-popover.tsx`                | Modify | Copy + fallback error akurat                       |
| `apps/api-rs/crates/api/src/routes/ai.rs`                                       | Modify | Error config AI user-facing                        |
| `packages/i18n/src/locales/en/navigation.json`                                  | Modify | `sidebar.pi_chat` → Galileo; hapus key Pro/upgrade |
| `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/billing/**` | Delete | Route billing                                      |
| `apps/web/core/components/workspace/billing/**`                                 | Delete | Komponen billing                                   |
| `apps/web/core/components/license/**`                                           | Delete | Modal license/upgrade                              |
| `packages/constants/src/{payment,subscription}.ts`                              | Delete | Konstanta plan                                     |
| `packages/types/src/payment.ts`                                                 | Delete | Tipe plan                                          |
| `packages/constants/src/settings/workspace.ts`                                  | Modify | Nav billing                                        |
| `packages/types/src/settings.ts`                                                | Modify | Union tab settings                                 |
| `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`             | Modify | Icon billing                                       |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/active-cycles/**`                | Delete | Route upsell active-cycles                         |
| `apps/web/core/components/active-cycles/**`                                     | Delete | Komponen upsell                                    |
| `apps/web/app/assets/workspace-active-cycles/**`                                | Delete | Aset upsell                                        |
| `apps/web/core/components/common/pro-icon.tsx`                                  | Delete | Ikon hanya dipakai upsell                          |
| `apps/web/core/components/workspace/sidebar/workspace-menu{,-item,-header}.tsx` | Delete | Sidebar yatim berisi badge Pro                     |
| `apps/web/core/components/issues/bulk-operations/**`                            | Delete | Banner upsell bulk-ops                             |
| `apps/web/core/components/workspace/upgrade-badge.tsx`                          | Delete | Badge Pro                                          |
| `apps/web/core/services/cycle.service.ts`                                       | Modify | Method active-cycles tak terpakai                  |
| `packages/constants/src/endpoints.ts`                                           | Modify | Link marketing                                     |
| `apps/web/core/components/workspace/sidebar/helper.tsx`                         | Modify | Case icon active_cycles                            |
| `apps/web/core/components/workspace/sidebar/extended-sidebar-item.tsx`          | Modify | Badge Pro                                          |
| `apps/web/core/components/project/settings/features-list.tsx`                   | Modify | Field `isPro` mati                                 |
| `apps/web/core/components/estimates/create/stage-one.tsx`                       | Modify | Branch badge unreachable                           |
| `packages/i18n/src/locales/en/{common,empty-state,workspace-settings}.json`     | Modify | Key mati                                           |

---

### Task 1: Branding Galileo — rename "Pi" + `sidebar.pi_chat`

**Files:**

- Modify: `packages/constants/src/ai.ts:12`
- Modify: `apps/web/core/components/pages/editor/ai/menu.tsx:272`
- Modify: `packages/i18n/src/locales/en/navigation.json` (nilai `sidebar.pi_chat`)

- [ ] **Step 1: Rename loading text**

`packages/constants/src/ai.ts`:

```ts
export const LOADING_TEXTS = {
  [AI_EDITOR_TASKS.ASK_ANYTHING]: "Galileo is generating response",
} satisfies { [key in AI_EDITOR_TASKS]: string };
```

`apps/web/core/components/pages/editor/ai/menu.tsx:272`:

```tsx
{activeTask ? LOADING_TEXTS[activeTask] : "Galileo is writing"}...
```

- [ ] **Step 2: Seragamkan label chat di user menu**

Di `packages/i18n/src/locales/en/navigation.json`, ubah nilai `sidebar.pi_chat`:

```json
"pi_chat": "Galileo",
```

- [ ] **Step 3: Verifikasi tidak ada sisa "Pi"**

Run:

```bash
rg -n --no-heading '"Pi is|>Pi<|\bPi\b' apps/web/core packages/constants/src packages/i18n/src/locales/en --glob "*.ts" --glob "*.tsx" --glob "*.json" | rg -v "Pinned|pin|Pill|Pipe|picker" | head
```

Expected: tidak ada output.

- [ ] **Step 4: Commit**

```bash
git add packages/constants/src/ai.ts apps/web/core/components/pages/editor/ai/menu.tsx packages/i18n/src/locales/en/navigation.json
git commit -m "content(ai): seragamkan nama asisten ke Galileo"
```

---

### Task 2: Copy panel assistant sidebar

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx` (baris ~25-27, ~131, ~166, ~178)

- [ ] **Step 1: Ubah header + chip + empty state**

Ganti header (baris ~131):

```tsx
<span className="text-sm font-semibold text-primary">Galileo</span>
```

Ganti chip saran (baris ~25-27):

```tsx
const SUGGESTIONS = [
  "Summarize this work item in 3 bullets",
  "Draft a status comment for this work item",
  "Suggest resolution steps for this work item",
];
```

Ganti empty state (baris ~166):

```tsx
<span className="truncate text-xs text-tertiary">No work item in view — general answers</span>
```

Ganti subtitle (baris ~178):

```tsx
Summaries, descriptions, comment drafts — grounded in the work item on screen.
```

(Pertahankan nama konstanta yang ada; hanya isi string yang berubah.)

- [ ] **Step 2: Verifikasi**

Run:

```bash
rg -n --no-heading "No issue in view|acceptance criteria|grounded in the issue" apps/web/core/components/ai
```

Expected: tidak ada output.

- [ ] **Step 3: Lint file yang berubah**

Run: `pnpm exec oxlint apps/web/core/components/ai/assistant-sidebar/root.tsx`
Expected: `0 errors`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "content(ai): panel Galileo + copy work item"
```

---

### Task 3: Copy popover AI + fallback error akurat + error api-rs

**Files:**

- Modify: `apps/web/core/components/core/modals/gpt-assistant-popover.tsx` (baris ~94-97, ~128, ~274)
- Modify: `apps/api-rs/crates/api/src/routes/ai.rs` (baris ~220)

- [ ] **Step 1: Ganti fallback error + hapus pesan kuota 50/bulan**

Di `handleServiceError` (baris ~94-97), ganti:

```tsx
const error = err?.data?.error;
const errorMessage =
  err?.status === 429
    ? error || "You have reached the maximum number of requests of 50 requests per month per user."
    : error || "Some error occurred. Please try again.";
```

menjadi:

```tsx
const error = err?.data?.error;
const errorMessage = error || "Something went wrong. Please try again.";
```

- [ ] **Step 2: Rapikan copy input**

Baris ~128:

```tsx
      message: "Enter a prompt to get AI assistance.",
```

Baris ~274:

```tsx
                  placeholder={`${
                    prompt && prompt !== "" ? "Tell Galileo what to do with this content..." : "Ask Galileo anything..."
                  }`}
```

- [ ] **Step 3: Perbaiki error backend yang tampil ke user**

Di `apps/api-rs/crates/api/src/routes/ai.rs` (baris ~220), ganti:

```rust
            Json(json!({"error": "LLM provider API key and model are required"})),
```

menjadi:

```rust
            Json(json!({"error": "AI is not configured for this workspace."})),
```

- [ ] **Step 4: Verifikasi**

Run:

```bash
rg -n --no-heading "50 requests per month|Some error occurred|Please enter some task" apps/web/core
cargo check -p api 2>&1 | tail -3
```

Expected: rg tanpa output; cargo `Finished`.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/core/modals/gpt-assistant-popover.tsx
git commit -m "content(ai): fallback error jujur + copy Galileo"
git add apps/api-rs/crates/api/src/routes/ai.rs
git commit -m "fix(api-rs): pesan error AI user-facing"
```

---

### Task 4: Hapus Billing & Plans (settings)

**Files:**

- Delete: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/billing/`
- Delete: `apps/web/core/components/workspace/billing/`
- Modify: `packages/constants/src/settings/workspace.ts:44-49,75`
- Modify: `packages/types/src/settings.ts:13`
- Modify: `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`

- [ ] **Step 1: Hapus route + komponen**

```bash
git rm -r "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/billing"
git rm -r apps/web/core/components/workspace/billing
```

- [ ] **Step 2: Hapus entri nav + tipe**

Di `packages/constants/src/settings/workspace.ts`, hapus blok:

```ts
  "billing-and-plans": {
    key: "billing-and-plans",
    i18n_label: "workspace_settings.settings.billing_and_plans.title",
    href: `/settings/billing`,
    access: [EUserWorkspaceRoles.ADMIN],
    highlight: (pathname: string, baseUrl: string) => pathname === `${baseUrl}/settings/billing/`,
  },
```

dan hapus baris `WORKSPACE_SETTINGS["billing-and-plans"],` dari `GROUPED_WORKSPACE_SETTINGS`.

Di `packages/types/src/settings.ts:13`, ganti:

```ts
export type TWorkspaceSettingsTabs = "general" | "members" | "export" | "webhooks";
```

- [ ] **Step 3: Hapus icon mapping**

Di `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`, hapus `BillingsOutline,` dari daftar import dan baris:

```tsx
  "billing-and-plans": BillingsOutline,
```

- [ ] **Step 4: Verifikasi**

Run:

```bash
rg -n --no-heading "billing-and-plans|components/workspace/billing|settings/billing" apps/web packages --glob "*.ts" --glob "*.tsx" | head
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: rg tanpa output; typecheck hanya menyisakan error pra-existing `members-list.tsx` (`toSorted`).

- [ ] **Step 5: Commit**

```bash
git add -A apps/web/app apps/web/core packages/constants packages/types
git commit -m "chore(web): hapus Billing & Plans"
```

---

### Task 5: Hapus license/upgrade modal + konstanta plan

**Files:**

- Delete: `apps/web/core/components/license/`
- Delete: `packages/constants/src/payment.ts`, `packages/constants/src/subscription.ts`, `packages/types/src/payment.ts`
- Modify: `packages/constants/src/index.ts:32,42`, `packages/types/src/index.ts:40`

- [ ] **Step 1: Hapus file**

```bash
git rm -r apps/web/core/components/license
git rm packages/constants/src/payment.ts packages/constants/src/subscription.ts packages/types/src/payment.ts
```

- [ ] **Step 2: Hapus ekspor**

Di `packages/constants/src/index.ts`, hapus baris:

```ts
export * from "./payment";
```

dan:

```ts
export * from "./subscription";
```

Di `packages/types/src/index.ts`, hapus baris:

```ts
export * from "./payment";
```

- [ ] **Step 3: Verifikasi tidak ada konsumen tersisa**

Run:

```bash
rg -n --no-heading "components/license|PLANE_COMMUNITY_PRODUCTS|SUBSCRIPTION_REDIRECTION_URLS|EProductSubscriptionEnum|ENTERPRISE_PLAN_FEATURES|PRO_PLAN_FEATURES" apps packages --glob "*.ts" --glob "*.tsx" | rg -v "keys.generated" | head
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: rg tanpa output; typecheck hanya error pra-existing.

- [ ] **Step 4: Commit**

```bash
git add -A apps/web/core packages/constants packages/types
git commit -m "chore(web): hapus modal license + konstanta plan"
```

---

### Task 6: Hapus upsell active-cycles + sidebar yatim + aset

**Files:**

- Delete: `apps/web/app/(all)/[workspaceSlug]/(projects)/active-cycles/`
- Delete: `apps/web/core/components/active-cycles/`
- Delete: `apps/web/app/assets/workspace-active-cycles/`
- Delete: `apps/web/core/components/common/pro-icon.tsx`
- Delete: `apps/web/core/components/workspace/sidebar/workspace-menu.tsx`, `workspace-menu-item.tsx`, `workspace-menu-header.tsx`
- Modify: `apps/web/core/services/cycle.service.ts:13,64-73`
- Modify: `packages/constants/src/endpoints.ts:31`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx:7-18,31-32`

- [ ] **Step 1: Hapus route, komponen, aset, file yatim**

```bash
git rm -r "apps/web/app/(all)/[workspaceSlug]/(projects)/active-cycles"
git rm -r apps/web/core/components/active-cycles
git rm -r apps/web/app/assets/workspace-active-cycles
git rm apps/web/core/components/common/pro-icon.tsx
git rm apps/web/core/components/workspace/sidebar/workspace-menu.tsx \
       apps/web/core/components/workspace/sidebar/workspace-menu-item.tsx \
       apps/web/core/components/workspace/sidebar/workspace-menu-header.tsx
```

- [ ] **Step 2: Hapus method service yang tak terpakai**

Di `apps/web/core/services/cycle.service.ts`, hapus `IWorkspaceActiveCyclesResponse,` dari blok `import type { ... } from "@plane/types"` dan hapus method:

```ts
  async workspaceActiveCycles(
    workspaceSlug: string,
    cursor: string,
    per_page: number
  ): Promise<IWorkspaceActiveCyclesResponse> {
    return this.get(`/api/workspaces/${workspaceSlug}/active-cycles/`, {
      params: {
        per_page,
        cursor,
      },
    })
      .then((res) => res?.data)
      .catch((err) => {
        throw err?.response?.data;
      });
  }
```

(Pertahankan `IWorkspaceActiveCyclesResponse` di `packages/types` — masih dipakai `packages/services/src/cycle/cycle.service.ts`.)

- [ ] **Step 3: Hapus link marketing + case icon**

Di `packages/constants/src/endpoints.ts`, hapus baris:

```ts
export const MARKETING_PRICING_PAGE_LINK = "https://terraline.space/pricing";
```

Di `apps/web/core/components/workspace/sidebar/helper.tsx`, hapus blok:

```tsx
    case "active_cycles":
      return <CyclesOutline className={cn("size-4 flex-shrink-0", className)} />;
```

dan hapus `CyclesOutline,` dari daftar import.

- [ ] **Step 4: Verifikasi**

Run:

```bash
rg -n --no-heading "active-cycles|active_cycles|MARKETING_PRICING_PAGE_LINK|workspace-active-cycles" apps/web packages/constants --glob "*.ts" --glob "*.tsx" | head
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: rg hanya menyisakan key i18n/`packages/services` (bukan UI); typecheck hanya error pra-existing.

- [ ] **Step 5: Commit**

```bash
git add -A apps/web packages/constants
git commit -m "chore(web): hapus upsell active-cycles + sidebar yatim"
```

---

### Task 7: Hapus banner upsell bulk-ops

**Files:**

- Delete: `apps/web/core/components/issues/bulk-operations/`
- Modify: `apps/web/core/components/issues/issue-layouts/list/default.tsx:30,175`
- Modify: `apps/web/core/components/issues/issue-layouts/spreadsheet/spreadsheet-view.tsx:16,122`
- Modify: `packages/constants/src/endpoints.ts` (`MARKETING_PLANE_ONE_PAGE_LINK`)

- [ ] **Step 1: Hapus komponen**

```bash
git rm -r apps/web/core/components/issues/bulk-operations
```

- [ ] **Step 2: Hapus pemakaian di layout**

Di `list/default.tsx`: hapus import `import { IssueBulkOperationsRoot } from "@/components/issues/bulk-operations";` dan baris `<IssueBulkOperationsRoot selectionHelpers={helpers} />`.

Di `spreadsheet/spreadsheet-view.tsx`: hapus import `import { IssueBulkOperationsRoot } from "@/components/issues/bulk-operations";` dan baris `<IssueBulkOperationsRoot selectionHelpers={helpers} />`.

- [ ] **Step 3: Hapus konstanta marketing**

Di `packages/constants/src/endpoints.ts`, hapus baris:

```ts
export const MARKETING_PLANE_ONE_PAGE_LINK = "https://terraline.space/one";
```

- [ ] **Step 4: Verifikasi**

Run:

```bash
rg -n --no-heading "bulk-operations|MARKETING_PLANE_ONE_PAGE_LINK" apps/web packages/constants --glob "*.ts" --glob "*.tsx" | head
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: rg tanpa output; typecheck hanya error pra-existing.

- [ ] **Step 5: Commit**

```bash
git add -A apps/web packages/constants
git commit -m "chore(web): hapus banner upsell bulk-ops"
```

---

### Task 8: Hapus UpgradeBadge + pemakaian tersisa

**Files:**

- Delete: `apps/web/core/components/workspace/upgrade-badge.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/extended-sidebar-item.tsx:199-203`
- Modify: `apps/web/core/components/project/settings/features-list.tsx:11,21,37,46,55,64,73,121-127`
- Modify: `apps/web/core/components/estimates/create/stage-one.tsx:17,53-56`

- [ ] **Step 1: Hapus komponen**

```bash
git rm apps/web/core/components/workspace/upgrade-badge.tsx
```

- [ ] **Step 2: Hapus blok badge di extended sidebar**

Di `apps/web/core/components/workspace/sidebar/extended-sidebar-item.tsx`, hapus:

```tsx
{
  item.key === "active_cycles" && (
    <div className="flex-shrink-0">
      <UpgradeBadge />
    </div>
  );
}
```

dan hapus import `import { UpgradeBadge } from "@/components/workspace/upgrade-badge";`.

- [ ] **Step 3: Bersihkan features-list**

Di `apps/web/core/components/project/settings/features-list.tsx`:

- Hapus `isPro: false,` pada 5 entri (baris ~37, 46, 55, 64, 73).
- Hapus blok badge:

```tsx
{
  featureItem.isPro && (
    <Tooltip label="Pro feature">
      <UpgradeBadge className="rounded-sm" />
    </Tooltip>
  );
}
```

- Hapus import `import { Tooltip } from "@makeplane/propel/components/tooltip";` dan `import { UpgradeBadge } from "@/components/workspace/upgrade-badge";` (keduanya hanya dipakai blok di atas).

- [ ] **Step 4: Sederhanakan estimates stage-one**

Di `apps/web/core/components/estimates/create/stage-one.tsx`, hapus import `import { UpgradeBadge } from "@/components/workspace/upgrade-badge";` dan ganti blok label:

```tsx
                label: !ESTIMATE_SYSTEMS[currentSystem]?.is_available ? (
                  <div className="relative flex cursor-no-drop items-center gap-2 text-tertiary">
                    {t(ESTIMATE_SYSTEMS[currentSystem]?.i18n_name)}
                    <Tooltip label={t("common.coming_soon")}>
                      <InfoOutline width={12} height={12} />
                    </Tooltip>
                  </div>
                ) : !isEnabled ? (
                  <div className="relative flex cursor-no-drop items-center gap-2 text-tertiary">
                    {t(ESTIMATE_SYSTEMS[currentSystem]?.i18n_name)}
                    <UpgradeBadge />
                  </div>
                ) : (
                  <div>{t(ESTIMATE_SYSTEMS[currentSystem]?.i18n_name)}</div>
                ),
```

menjadi:

```tsx
                label: !ESTIMATE_SYSTEMS[currentSystem]?.is_available ? (
                  <div className="relative flex cursor-no-drop items-center gap-2 text-tertiary">
                    {t(ESTIMATE_SYSTEMS[currentSystem]?.i18n_name)}
                    <Tooltip label={t("common.coming_soon")}>
                      <InfoOutline width={12} height={12} />
                    </Tooltip>
                  </div>
                ) : (
                  <div>{t(ESTIMATE_SYSTEMS[currentSystem]?.i18n_name)}</div>
                ),
```

- [ ] **Step 5: Verifikasi**

Run:

```bash
rg -n --no-heading "UpgradeBadge|upgrade-badge|isPro|Pro feature" apps/web/core --glob "*.ts" --glob "*.tsx" | head
pnpm exec oxlint apps/web/core/components/workspace/sidebar/extended-sidebar-item.tsx apps/web/core/components/project/settings/features-list.tsx apps/web/core/components/estimates/create/stage-one.tsx
```

Expected: rg tanpa output; oxlint `0 errors`.

- [ ] **Step 6: Commit**

```bash
git add -A apps/web/core
git commit -m "chore(web): hapus UpgradeBadge + pemakaiannya"
```

---

### Task 9: Bersihkan key i18n yang mati (locale `en`)

**Files:**

- Modify: `packages/i18n/src/locales/en/navigation.json`
- Modify: `packages/i18n/src/locales/en/common.json`
- Modify: `packages/i18n/src/locales/en/empty-state.json`
- Modify: `packages/i18n/src/locales/en/workspace-settings.json`

- [ ] **Step 1: Pastikan setiap key benar-benar tidak terpakai**

Run untuk tiap key (contoh; ulangi untuk semua di daftar):

```bash
for k in 'sidebar.pro' 'sidebar.upgrade' 'sidebar.upgrade_plan' 'sidebar.plane_pro' \
  'common.upgrade' 'common.upgrade_request' 'common.active_cycles' 'common.active_cycles_description' \
  'common.on_demand_snapshots_of_all_your_cycles' 'common.10000_feet_view' \
  'common.10000_feet_view_description' 'common.get_snapshot_of_each_active_cycle' \
  'common.get_snapshot_of_each_active_cycle_description' 'common.compare_burndowns' \
  'common.compare_burndowns_description' 'common.quickly_see_make_or_break_issues' \
  'common.quickly_see_make_or_break_issues_description' 'common.zoom_into_cycles_that_need_attention' \
  'common.zoom_into_cycles_that_need_attention_description' 'common.stay_ahead_of_blockers' \
  'common.stay_ahead_of_blockers_description' 'common.upgrade_cta.talk_to_sales' \
  'common.upgrade_cta.higher_subscription' 'workspace_empty_state.active_cycles'; do
  echo "-- $k"; rg -n --no-heading -F "t(\"$k\")" apps/web packages/constants --glob "*.ts" --glob "*.tsx" | head -2
done
rg -n --no-heading "billing_and_plans" apps/web packages/constants --glob "*.ts" --glob "*.tsx" | head -2
```

Expected: tidak ada output untuk semua key. Bila ada yang masih dipakai, JANGAN hapus key itu (hapus dari daftar).

- [ ] **Step 2: Hapus key dari JSON `en`**

- `navigation.json` → hapus dari objek `sidebar`: `pro`, `upgrade`, `upgrade_plan`, `plane_pro`.
- `common.json` → hapus key **top-level**: `active_cycles`, `active_cycles_description`, `on_demand_snapshots_of_all_your_cycles`, `upgrade`, `upgrade_request`, `10000_feet_view`, `10000_feet_view_description`, `get_snapshot_of_each_active_cycle`, `get_snapshot_of_each_active_cycle_description`, `compare_burndowns`, `compare_burndowns_description`, `quickly_see_make_or_break_issues`, `quickly_see_make_or_break_issues_description`, `zoom_into_cycles_that_need_attention`, `zoom_into_cycles_that_need_attention_description`, `stay_ahead_of_blockers`, `stay_ahead_of_blockers_description`; lalu hapus objek `upgrade_cta` di dalam objek bersarang `common` (berisi `higher_subscription`, `talk_to_sales`).
- `empty-state.json` → hapus `workspace_empty_state.active_cycles`.
- `workspace-settings.json` → hapus objek `workspace_settings.settings.billing_and_plans` (beserta subkey `heading`, `description`, `title`, `current_plan`, `free_plan`, `view_plans`).

- [ ] **Step 3: Verifikasi**

Run:

```bash
pnpm --filter=@plane/i18n check:types 2>&1 | tail -5
pnpm --filter=@plane/i18n check:sync 2>&1 | tail -12
node -e "
const fs=require('fs');
for (const f of ['navigation','common','empty-state','workspace-settings']) {
  JSON.parse(fs.readFileSync('packages/i18n/src/locales/en/'+f+'.json','utf8'));
}
console.log('en JSON valid');
"
```

Expected: `check:types` PASS (key generated ter-regenerasi); `check:sync` tetap gagal karena _missing_ pra-existing (bukan _stale_ baru); `en JSON valid`.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n
git commit -m "chore(i18n): hapus key surface jualan (en)"
```

---

### Task 10: Verifikasi akhir + build + deploy + visual

**Files:** — (tidak ada perubahan kode)

- [ ] **Step 1: Lint + types + i18n**

Run:

```bash
pnpm check:lint 2>&1 | tail -8
pnpm --filter=web check:types 2>&1 | tail -6
pnpm --filter=@plane/i18n check:types 2>&1 | tail -3
```

Expected: lint `0 errors`; web types hanya error pra-existing `members-list.tsx` (`toSorted`); i18n types PASS.

- [ ] **Step 2: Audit teks jualan**

Run:

```bash
rg -n --no-heading "Upgrade|upgrade|Talk to Sales|talk-to-sales|pricing|billing|Billing|subscription|Subscription|Pro feature" apps/web/core/components apps/web/app --glob "*.tsx" --glob "*.ts" | rg -v "toUpperCase|proceed|property|provider|prompt|process|project|proof|prop\b|appropriate|approve|improve|protect|profile|progress|proxy|prose|proto|provid" | head -20
```

Expected: tidak ada hit yang terkait jualan. (Kata seperti "upgrade" yang sah untuk versi aplikasi hanya ada di app admin, bukan web.)

- [ ] **Step 3: Rebuild api-rs (untuk string `ai.rs`) + deploy**

```bash
docker compose -f docker-compose-local.yml up -d --build api
systemctl --user reload plane-backend.service
```

Expected: container `api` sehat.

- [ ] **Step 4: Build web + restart prod**

```bash
pnpm --filter=web build 2>&1 | tail -5
systemctl --user restart plane-web-prod.service
sleep 8
curl -s -o /dev/null -w "HTTP %{http_code}\n" http://127.0.0.1:3000/
```

Expected: build sukses; `HTTP 200`.

- [ ] **Step 5: Inspeksi visual di tunnel**

- Workspace Settings: tidak ada item "Billing & Plans"; `/settings/billing` → 404.
- Sidebar workspace: tidak ada badge "Pro"; tidak ada link "Active cycles".
- Panel AI: header "Galileo"; chip "Suggest resolution steps for this work item"; empty state menyebut "work item".
- Editor page AI: loading text "Galileo is writing".
- User menu: item chat berlabel "Galileo".
- Cek `/active-cycles` → 404.

- [ ] **Step 6: Commit sisa (bila ada)**

```bash
git status --short
```

Expected: bersih.

---

## Catatan eksekusi

- Jangan menyentuh `apps/web/core/components/issues/issue-layouts/**` selain dua baris `IssueBulkOperationsRoot` (Task 7).
- Stage hanya file yang disentuh task; sebelum tiap commit jalankan `git diff --cached --stat` dan pastikan tidak ada perubahan orang lain yang ikut terbawa (workspace ini dipakai sesi lain).
- Infrastruktur multiple-select/bulk-ops (`use-bulk-operation-status.ts`, `MultipleSelectGroup`) sengaja dibiarkan.
- Key i18n dihapus dari `en` saja — locale lain stale (diterima, wave translate).
- Bila `pnpm check:lint` menemukan warning baru di file yang disentuh, perbaiki dengan perubahan identik-behavior (pola wave 1) atau `// oxlint-disable-next-line` bila memang preseden repo.
