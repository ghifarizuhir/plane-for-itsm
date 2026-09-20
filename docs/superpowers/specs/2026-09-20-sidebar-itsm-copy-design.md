# Sidebar Recopywriting (ITSM Voice) — Design

Tanggal: 2026-09-20

## Context

Lanjutan dari dashboard copy redesign (`2026-09-20-dashboard-itsm-copy-design.md`). Kali ini scope sidebar kiri workspace (`apps/web`). Temuan: label navigasi struktural sudah data-accurate ("Projects", "Your work", "Home", dsb.), tetapi ada (a) copy hardcoded English yang belum malalui i18n, dan (b) onboarding `USE_CASES` yang masih PM voice.

## Scope

- `packages/constants/src/workspace.ts` — rewrite `USE_CASES` ke ITSM voice.
- `apps/web/{app,core}/**` — konversi 6 hardcoded string ke i18n (`t()`).
- `packages/i18n/src/locales/en/navigation.json` — tambah `sidebar.hide`, `sidebar.more`, `sidebar.pin`, `sidebar.unpin`.
- Locale lain tidak disentuh (en saja, sesuai keputusan initiatif ini).
- Tidak diubah: label struktural ("Projects", "Your work", "New work item", "Create project", "Add project", "No favorites yet").

## Perubahan

### 1. USE_CASES (packages/constants/src/workspace.ts:297)

```ts
export const USE_CASES = [
  "Reduce incident response time",
  "Manage services and changes",
  "Track service requests end to end",
  "Coordinate ops across teams",
  "Replace our current tool",
  "Just exploring",
];
```

Duplikasi 2 item terakhir dipertahankan. Konsumen satu-satunya: `apps/web/core/components/onboarding/steps/usecase/root.tsx`.

### 2. Hardcoded → i18n

Key baru di `navigation.json` → object `sidebar`: `hide: "Hide"`, `more: "More"`, `pin: "Pin"`, `unpin: "Unpin"` (sesuai state UI, tooltip berbahasa English).

| Lokasi                                                                  | Sebelum                 | Sesudah                                                                |
| ----------------------------------------------------------------------- | ----------------------- | ---------------------------------------------------------------------- |
| `app/(all)/[workspaceSlug]/(projects)/sidebar.tsx:35`                   | `title="Projects"`      | `title={t("sidebar.projects")}`                                        |
| `core/components/sidebar/sidebar-wrapper.tsx:60`                        | `title === "Projects"`  | prop baru `showCustomizeGear` (default false), sidebar.tsx pass `true` |
| `core/components/workspace/sidebar/sidebar-menu-items.tsx:173`          | `"Hide" : "More"`       | `t("sidebar.hide") : t("sidebar.more")`                                |
| `core/components/workspace/sidebar/projects-list.tsx:264-265`           | `"Hide" : "More"`       | `t("sidebar.hide") : t("sidebar.more")`                                |
| `app/(all)/[workspaceSlug]/(projects)/extended-project-sidebar.tsx:115` | `<span>Projects</span>` | `<span>{t("sidebar.projects")}</span>`                                 |
| `core/components/workspace/sidebar/extended-sidebar-item.tsx:205,212`   | `"Unpin"` / `"Pin"`     | `t("sidebar.unpin")` / `t("sidebar.pin")`                              |

Nama prop final: `showCustomizeGear?: boolean` — wrapper default `false`; pemanggil lain tidak diubah. `@plane/constants` dipakai lintas app — konsumen pakai `cn`? pnpm check:types lintas apps yang relevan dijalankan.

## Error handling

- Tidak ada logic error-handling baru; perubahan murni copy/render.
- Guard `workspaceSlug` existing di `sidebar.tsx` tetap.

## Testing / verification

- `pnpm --filter=web check:lint` + `pnpm --filter=web check:types`
- `pnpm --filter=web build`; restart `plane-web-prod.service`; confirm HTTP 200 & string baru di bundle.
- Visual: sidebar utama, extended sidebar (pin/unpin toolbar), collapse toggle "Hide/More", dan gear customize tampil seperti sebelumnya.
