# Dropdown Positioning Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Perbaiki dropdown yang muncul di ujung kiri atas / tidak sinkron dengan tombol, secara global di semua page, dengan memperbaiki 1 hook reuse + dropdown inline yang salah pola positioning.

**Architecture:** Dua lapis perbaikan di package reuse (tanpa menyentuh logic form/API): (1) `usePopper` menyembunyikan popper sampai posisi pertama terkomputasi — menghilangkan flash `(0,0)` untuk seluruh 32 konsumen sekaligus; (2) dropdown yang render inline dengan kombinasi rusak (`fixed` class + `absolute` strategy + ancestor ber-`transform`) dipindah ke portal `document.body` + `strategy: "fixed"`, mengikuti preseden `MemberOptions`/`DateDropdown` yang sudah bekerja.

**Tech Stack:** React 19, HeadlessUI Combobox/Menu/Popover, `@floating-ui/dom` (`computePosition` + `autoUpdate`), `createPortal`, Tailwind, tsdown/tsc/oxlint.

**Decisions (user, 2026-09-18):** scope sekaligus (Task 1-3+5); `date-range.tsx` default portal = follow-up terpisah; eksekusi subagent-driven.

---

## Bukti inventarisasi (hasil deep dive read-only)

| Pola                                                 | File                                                                                                                                                                                                                                                                                                                                                                                                                | Status                                                                     |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| Hook tunggal                                         | `packages/hooks/src/use-popper.ts`                                                                                                                                                                                                                                                                                                                                                                                  | initial `top:0 left:0` tanpa transform (`:111-124`), tanpa flag positioned |
| Inline + outer `fixed` + strategy `absolute` (RUSAK) | `apps/web/core/components/dropdowns/state/base.tsx:217`, `project/base.tsx:239`, `intake-state/base.tsx:215`, `estimate.tsx:233`, `cycle/cycle-options.tsx:131`, `priority.tsx:450`, `module/module-options.tsx:116`, `issues/select/base.tsx:194`, `issue-layouts/properties/label-dropdown.tsx:256`, `issue-layouts/filters/header/helpers/dropdown.tsx:101`, `packages/ui/src/dropdowns/custom-menu.tsx:195-226` | 11 lokasi                                                                  |
| Portal ke body + absolute (OK, sisa flash)           | `member/member-options.tsx:135`, `date.tsx:184`, `custom-select.tsx:122`, `custom-search-select.tsx:145`                                                                                                                                                                                                                                                                                                            | 4 lokasi                                                                   |
| Portal opsional, default inline                      | `date-range.tsx:105,283` (`renderInPortal=false`)                                                                                                                                                                                                                                                                                                                                                                   | follow-up, JANGAN ubah                                                     |
| Sudah `strategy:"fixed"` (preseden benar)            | `custom-menu.tsx:334` (SubMenu), `context-menu/item.tsx:42`                                                                                                                                                                                                                                                                                                                                                         | referensi, jangan ubah                                                     |
| Ancestor ber-`transform`                             | `packages/ui/src/modals/modal-core.tsx:49-63` (`Dialog.Panel`: `transform ... scale/translate`)                                                                                                                                                                                                                                                                                                                     | penyebab fixed pecah di modal                                              |
| Infra test                                           | `packages/hooks` & `packages/ui` hanya punya `check:lint` (oxlint), `check:types` (tsc), `check:format` (oxfmt), storybook — tanpa unit test runner                                                                                                                                                                                                                                                                 | verifikasi = tsc + lint + smoke manual                                     |

---

### Task 1: Tambah flag `isPositioned` di `usePopper` + sembunyikan popper sampai posisi pertama komit

**Files:**

- Modify: `packages/hooks/src/use-popper.ts:108-124,140-171,173-179`

**Alasan:** satu perubahan di sini memperbaiki flash `(0,0)` untuk seluruh 32 call-site tanpa menyentuh mereka (mereka hanya destructure `{styles, attributes}` — penambahan field backward-compatible).

- [ ] **Step 1: Tambah state `isPositioned` dan selipkan `visibility` ke initial styles**

```ts
const [isPositioned, setIsPositioned] = useState(false);
const [data, setData] = useState<{
  styles: { popper: React.CSSProperties };
  attributes: { popper: Record<string, string> };
}>(() => ({
  styles: {
    popper: {
      position: strategy,
      top: "0",
      left: "0",
      visibility: "hidden",
    } as React.CSSProperties,
  },
  attributes: {
    popper: {
      "data-popper-placement": placement,
    },
  },
}));
```

- [ ] **Step 2: Set `isPositioned=true` + hapus `visibility:hidden` saat `computePosition` pertama resolve**

```ts
.then(({ x, y, placement: computedPlacement, strategy: computedStrategy }) => {
  setIsPositioned(true);
  setData({
    styles: {
      popper: {
        position: computedStrategy,
        top: "0",
        left: "0",
        transform: `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`,
        willChange: "transform",
      } as React.CSSProperties,
    },
    attributes: {
      popper: {
        "data-popper-placement": computedPlacement,
      },
    },
  });
})
```

- [ ] **Step 3: Kembalikan `isPositioned` di return (tambah tanpa menghapus field lama)**

```ts
return {
  styles: data.styles,
  attributes: data.attributes,
  isPositioned,
  state: null,
  update: () => Promise.resolve(null),
  forceUpdate: () => null,
};
```

- [ ] **Step 4: Verifikasi types + lint package hooks**

```bash
pnpm --filter=@plane/hooks check:types && pnpm --filter=@plane/hooks check:lint
```

Expected: kedua command PASS, tidak ada error TS (konsumen lama yang destructure `{styles, attributes}` tetap valid).

- [ ] **Step 5: Commit HANYA file task ini**

```bash
git add packages/hooks/src/use-popper.ts
git commit -m "fix(hooks): hide popper until first position commits"
```

JANGAN stage/commit file lain (repo punya modifikasi tak terkait di `apps/api-rs/` — biarkan).

---

### Task 2: Portal + `strategy:"fixed"` untuk 7 dropdown inline di `apps/web/core/components/dropdowns/`

**Files (satu pola yang sama, contoh penuh di bawah memakai `state/base.tsx`):**

- Modify: `apps/web/core/components/dropdowns/state/base.tsx:79-100,216-258`
- Modify: `apps/web/core/components/dropdowns/project/base.tsx:94,239` (pola identik)
- Modify: `apps/web/core/components/dropdowns/intake-state/base.tsx:90,215`
- Modify: `apps/web/core/components/dropdowns/estimate.tsx:81,233`
- Modify: `apps/web/core/components/dropdowns/cycle/cycle-options.tsx:69,131`
- Modify: `apps/web/core/components/dropdowns/priority.tsx:340,450`
- Modify: `apps/web/core/components/dropdowns/module/module-options.tsx:62,116`

**Pola per file (tiga edit kecil):**

- [ ] **Step 1: Tambah `strategy:"fixed"` di `usePopper` (contoh `state/base.tsx:90-100`)**

```tsx
const { styles, attributes } = usePopper(referenceElement, popperElement, {
  placement: placement ?? "bottom-start",
  strategy: "fixed",
  modifiers: [
    {
      name: "preventOverflow",
      options: {
        padding: 12,
      },
    },
  ],
});
```

(Pertahankan modifiers masing-masing file apa adanya — hanya tambah `strategy: "fixed"`.)

- [ ] **Step 2: Bungkus `<Combobox.Options>` dengan `createPortal(..., document.body)`, hapus class `fixed` dari outer (posisinya digantikan inline style), tambah `import { createPortal } from "react-dom"` jika belum ada**

```tsx
// state/base.tsx:216-217 berubah dari:
{isOpen && (
  <Combobox.Options as="ul" className="fixed z-10" static>
    <div ... style={styles.popper} ...>
// menjadi:
{isOpen &&
  createPortal(
    <Combobox.Options as="ul" className="z-10" static>
      <div ... style={styles.popper} ...>
      </div>
    </Combobox.Options>,
    document.body
  )}
```

Catatan: guard `{isOpen && ...}` sudah ada sehingga `document.body` tidak tersentuh saat SSR/first render — sama seperti preseden `date.tsx:183-213`. Pertahankan semua class lain (`z-10`, `static`, `data-prevent-outside-click` bila ada) dan semua children apa adanya.

- [ ] **Step 3: Ulangi Step 1–2 untuk 6 file sisanya** (daftar di atas; masing-masing hanya 1 call `usePopper` + 1 block `Combobox.Options`; baca tiap file dulu sebelum edit)
- [ ] **Step 4: Verifikasi types + lint web**

```bash
pnpm --filter=web check:types
```

Expected: PASS. Jika ada error import `createPortal` duplikat, hapus yang lama. (Lint web menyusul di Task 5 global.)

- [ ] **Step 5: Commit HANYA file task ini**

```bash
git add apps/web/core/components/dropdowns/
git commit -m "fix(web): portal dropdown options to body with fixed strategy"
```

---

### Task 3: Portal + `strategy:"fixed"` untuk `IssueLabelSelect`, `label-dropdown`, `FiltersDropdown`, `CustomMenu`

**Files:**

- Modify: `apps/web/core/components/issues/select/base.tsx:70,194`
- Modify: `apps/web/core/components/issues/issue-layouts/properties/label-dropdown.tsx:256` (+ `usePopper` di file yang sama)
- Modify: `apps/web/core/components/issues/issue-layouts/filters/header/helpers/dropdown.tsx:43-45,101-113`
- Modify: `packages/ui/src/dropdowns/custom-menu.tsx:87-96,195-226`

**Khusus `CustomMenu`:** hanya ubah jalur **tanpa** `portalElement` (jalur `portalElement` di `:224-226` sudah benar, jangan disentuh). SubMenu (`:332-355`, sudah `strategy:"fixed"`) jangan disentuh.

- [ ] **Step 1: Tambah `strategy:"fixed"` di `usePopper` utama (`custom-menu.tsx:94-96`)**

```tsx
const { styles, attributes } = usePopper(referenceElement, popperElement, {
  placement: placement ?? "auto",
  strategy: "fixed",
});
```

- [ ] **Step 2: Selalu portal-kan `menuItems` ke `document.body` bila `portalElement` tidak diberikan**

```tsx
if (portalElement) {
  menuItems = ReactDOM.createPortal(menuItems, portalElement);
} else if (isOpen) {
  menuItems = ReactDOM.createPortal(menuItems, document.body);
}
```

dan hapus `fixed` dari `Menu.Items` (`:196-202`) karena posisi kini dari inline style:

```tsx
className={cn("z-30 translate-y-0", menuItemsClassName)}
```

(Pertahankan komentar hack `translate-y-0` untuk Safari.)

- [ ] **Step 3: Terapkan pola portal yang sama ke `select/base.tsx`, `label-dropdown.tsx`, `filters/header/helpers/dropdown.tsx`** (masing-masing: baca file dulu, lalu `strategy:"fixed"` + `createPortal(..., document.body)` di-guard state open + hapus `fixed` dari outer; pertahankan semua class/children lain)
- [ ] **Step 4: Verifikasi types + lint kedua package**

```bash
pnpm --filter=@plane/ui check:types && pnpm --filter=@plane/ui check:lint && pnpm --filter=web check:types
```

Expected: PASS (batas warning ui: max 66).

- [ ] **Step 5: Commit HANYA file task ini**

```bash
git add packages/ui/src/dropdowns/custom-menu.tsx apps/web/core/components/issues/select/base.tsx apps/web/core/components/issues/issue-layouts/properties/label-dropdown.tsx apps/web/core/components/issues/issue-layouts/filters/header/helpers/dropdown.tsx
git commit -m "fix(ui,web): portal floating menus to body with fixed strategy"
```

---

### Task 4: Verifikasi end-to-end (build, checks, smoke-test checklist untuk pelaksana manual)

- [ ] **Step 1: Build hooks + ui**

```bash
pnpm --filter=@plane/hooks build && pnpm --filter=@plane/ui build
```

Expected: build sukses, `dist` ter-regenerasi.

- [ ] **Step 2: Checks global**

```bash
pnpm check:lint && pnpm check:types
```

Expected: PASS.

- [ ] **Step 3: Serahkan checklist QA manual ke user** (tidak bisa otomatis — butuh browser):
  - Modal create → State, Priority, Assignee, Date, Cycle, Module, Estimate, Label, Parent: dropdown nempel tombol, tanpa flash kiri atas
  - List/board → tombol `...` (CustomMenu), dropdown State di row
  - Filter header + calendar dropdowns; outside-click menutup dropdown portal; keyboard Enter/Escape/panah; scroll saat terbuka
- [ ] **Step 4: Rebuild + restart prod (per AGENTS.md, setelah QA lolos)**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

CATATAN: Step 3–4 melibatkan service sistem + browser — subagent hanya menyiapkan sampai Step 2 dan melaporkan; Step 3–4 dieksekusi/dikonfirmasi bersama user.

---

## Follow-up terpisah (YAGNI — di luar plan ini)

- `date-range.tsx`: ubah default `renderInPortal` jadi `true`
- `popovers/popover.tsx`: potensi double-offset (`absolute top-full` + transform popper)
- Audit: `dropdown/single-select.tsx`, `form-fields/input-color-picker.tsx`, calendar `months/options-dropdown`
