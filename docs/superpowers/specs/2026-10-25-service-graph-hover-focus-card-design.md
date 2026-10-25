# Service Graph Hover Focus Card — Design

**Date:** 2026-10-25
**Status:** Approved design; pending written-spec review
**Scope:** `apps/web`, `packages/i18n` (reuse existing keys), `apps/web/package.json`, lockfile.
**Related:** [`2026-09-13-services-feature-design.md`](./2026-09-13-services-feature-design.md), [`2026-10-25-service-component-kind-graph-grouping-design.md`](./2026-10-25-service-component-kind-graph-grouping-design.md).

## Konteks

Service graph saat ini membuka inspector (`ServiceFocusCard`) hanya setelah user mengklik node. Untuk eksplorasi topologi, klik terasa berat: user harus bolak-balik klik node, klik kanvas untuk menutup, lalu klik node lain. Graph juga sudah menyorot relasi node terpilih, sehingga hover sudah cukup untuk memberi konteks.

Repo belum memakai Motion di `apps/web`. `framer-motion` sudah ada di catalog dan dipakai `packages/propel` untuk tab navigation, jadi menambahkannya ke web konsisten dan tidak menambah library baru ke workspace.

## Keputusan yang dikunci

1. **Hover membuka card, klik mem-pin.** Hover dengan intent delay membuka inspector; klik/tap mem-pin card agar tetap terbuka.
2. **Pin menang atas hover.** `effectiveServiceId = pinnedServiceId ?? hoveredServiceId`. Saat card A di-pin, hover node B tidak mengganti card; klik B memindahkan pin.
3. **Close eksplisit.** Tombol close, `Escape`, dan klik kanvas membersihkan pin + hover. Pin tidak disimpan antar navigasi/sesi.
4. **Hover intent + grace.** Delay buka **180ms**, grace tutup **140ms**. Grace menutup celah 10px antara node dan card sehingga cursor bisa menyeberang tanpa flicker.
5. **Touch memakai tap-pin.** Hover hanya aktif pada pointer yang mendukung hover (`(hover: hover) and (pointer: fine)`). Di touch, tap node langsung mem-pin; tidak ada hover state.
6. **Tidak ada trigger saat interaksi kanvas.** Drag node, connect handle, dan mulai pan/zoom menonaktifkan hover sampai interaksi selesai; hover tidak boleh terbuka di tengah drag.
7. **Motion penuh untuk card dan node.** `framer-motion` dipakai untuk entry/exit card, stagger konten, lift node, dan rotasi chevron. Posisi screen-space card saat pan/zoom tidak dianimasikan.
8. **Tetap aksesibel.** Jalur klik/tap tidak dihapus; card tetap punya close button, `Escape`, focus ring, dan `prefers-reduced-motion` mematikan transform/delay.
9. **Tanpa perubahan backend.** Semua data sudah tersedia di `ServiceGraphCanvas`.

## Non-goals

- Menyimpan pin ke store/server atau mempertahankannya setelah pindah halaman.
- Hover card untuk list view service.
- Long-press, context menu, atau gesture baru di mobile.
- Mengubah double-click (tetap membuka peek/detail penuh).
- Animasi posisi card saat panning/zooming.
- Pin keyboard pada node React Flow (jalur keyboard card belum menjadi target spec ini).
- Perubahan pada war room selain ikut memakai canvas yang sama.

## Arsitektur

```
ServiceGraphCanvas
├─ useHoverPointer()              // media query (hover: hover) + (pointer: fine)
├─ useServiceFocusInteraction()   // controller timer + state
│  ├─ hoveredServiceId
│  ├─ pinnedServiceId
│  └─ suppressed
├─ ReactFlow events
│  ├─ onNodeMouseEnter  → hover(id)
│  ├─ onNodeMouseLeave  → leaveNode()
│  ├─ onNodeClick       → pin(id)
│  ├─ onNodeDragStart   → suppress()
│  ├─ onNodeDragStop    → resume()
│  ├─ onConnectStart    → suppress()
│  ├─ onConnectEnd      → resume()
│  ├─ onMoveStart       → suppress()
│  ├─ onMoveEnd         → resume()
│  └─ onPaneClick       → clear()
└─ AnimatePresence
   └─ ServiceFocusCard
      ├─ onPointerEnter → enterCard()
      ├─ onPointerLeave → leaveCard()
      └─ pinned
```

`effectiveServiceId` menggantikan `selectedServiceId` untuk:

- memilih `selectedService` + `selectedNode`;
- menghitung `relationships` (highlight edge dan node tetangga);
- menandai node `focused`;
- menampilkan `ServiceFocusCard`.

### State machine

State efektif:

```ts
type TFocusState = {
  hoveredServiceId: string | null;
  pinnedServiceId: string | null;
  suppressed: boolean;
};

const effectiveServiceId = pinnedServiceId ?? hoveredServiceId;
```

Transisi:

| Event              | Syarat                                                | Aksi                                                                    |
| ------------------ | ----------------------------------------------------- | ----------------------------------------------------------------------- |
| `hover(id)`        | tidak pinned, tidak suppressed, pointer hover-capable | batalkan close timer; jadwalkan open 180ms                              |
| open timer selesai | masih tidak pinned/suppressed                         | `hoveredServiceId = id`                                                 |
| `leaveNode()`      | tidak pinned                                          | batalkan open timer; jadwalkan close 140ms                              |
| `enterCard()`      | —                                                     | batalkan close timer                                                    |
| `leaveCard()`      | tidak pinned                                          | jadwalkan close 140ms                                                   |
| `pin(id)`          | —                                                     | batalkan semua timer; `pinnedServiceId = id`; `hoveredServiceId = null` |
| `clear()`          | —                                                     | batalkan semua timer; kedua id `null`                                   |
| `suppress()`       | —                                                     | batalkan semua timer; `hoveredServiceId = null`; `suppressed = true`    |
| `resume()`         | —                                                     | `suppressed = false`                                                    |
| unmount            | —                                                     | batalkan timer                                                          |

Aturan tambahan:

- Hover node lain saat `pinnedServiceId` terisi tidak melakukan apa pun.
- Klik node yang sama saat sudah pinned tidak mengubah state.
- Klik node lain memindahkan pin (card lama exit, card baru enter).
- `onNodeDoubleClick` tetap memanggil `onNodeOpen`; event klik pertama boleh mem-pin sesaat sebelum navigasi.
- `onNodeMouseLeave` yang terjadi karena cursor masuk card dibatalkan oleh `enterCard()`.

### Timer controller

Logika timer diekstrak ke `focus-intent.ts` sebagai controller framework-free agar bisa diuji dengan fake timer:

```ts
createFocusIntentController({
  openDelay: 180,
  closeDelay: 140,
  onChange: (state) => void,
});
```

API: `hover(id)`, `leaveNode()`, `enterCard()`, `leaveCard()`, `pin(id)`, `clear()`, `suppress()`, `resume()`, `dispose()`.

Hook `useServiceFocusInteraction` membuat controller di `useRef`, meneruskan state ke React via `useState`, dan memanggil `dispose()` saat unmount.

## Motion design

Dependency: `framer-motion: catalog:` di `apps/web/package.json` (versi catalog `^12.23.0`). Import `motion`, `AnimatePresence`, `useReducedMotion` dari `framer-motion`, konsisten dengan Propel.

### Card entry/exit

- `AnimatePresence mode="wait"` membungkus card di canvas.
- Wrapper luar tetap `absolute` dengan posisi screen-space yang dihitung dari viewport; posisi `top`/`bottom`/`left` **tidak** memakai Motion.
- Card dalam menggunakan `motion.div`:
  - `initial`: `opacity: 0`, `scale: 0.96`, `y: placeAbove ? 8 : -8`.
  - `animate`: `opacity: 1`, `scale: 1`, `y: 0`.
  - spring: `stiffness: 380`, `damping: 30`, `mass: 0.7`.
  - `exit`: `opacity: 0`, `scale: 0.98`, `y: placeAbove ? 6 : -6`, duration `0.12`, ease-out.
  - `transformOrigin`: `${pointerLeft}px ${placeAbove ? "bottom" : "top"}` agar card tumbuh dari arah node.
- Saat ganti service (unpinned), key berubah sehingga exit card lama selesai sebelum enter card baru.

### Stagger konten

Section di dalam card memakai `motion.div` dengan `initial={{ opacity: 0, y: 4 }}`, `animate={{ opacity: 1, y: 0 }}` dan delay berurutan:

- header `0ms`;
- meta `30ms`;
- description `50ms`;
- footer `70ms`.

Total stagger tetap di bawah 100ms agar card terasa instan.

### Node feedback

- `ServiceNode` membungkus card visual dengan `motion.div`.
- `whileHover` (hanya hover-capable): `y: -2`, spring `stiffness: 420`, `damping: 28`.
- `whileTap`: `scale: 0.995`.
- Ring `is-focused` dan highlight edge tetap CSS karena reaktif terhadap `effectiveServiceId`.
- Shadow hover tetap CSS (`hover:shadow-raised-200`) agar tidak mengorbankan performa graph.

### Detail interaksi

- Chevron expand/collapse: `motion.span` dengan `animate={{ rotate: expanded ? 180 : 0 }}`; `ChevronDownOutline` tetap satu ikon.
- Indikator pinned: dot aksen kecil + label mono uppercase `t("common.pin")` muncul dengan fade/scale singkat di header card. Tidak ada i18n key baru.
- `useReducedMotion()`:
  - card hanya fade singkat tanpa scale/y/spring dan stagger `0`;
  - node tidak lift;
  - chevron berubah tanpa spring.
- Hapus `animation` dan `@keyframes service-focus-card-in` dari `globals.css`; box-shadow tetap.

## Perubahan file

| File                                                                  | Perubahan                                               |
| --------------------------------------------------------------------- | ------------------------------------------------------- |
| `apps/web/package.json` + lockfile                                    | tambah `framer-motion: catalog:`                        |
| `apps/web/core/hooks/use-mobile-viewport.ts`                          | tambah `HOVER_POINTER_QUERY` + `useHoverPointer()`      |
| `apps/web/core/components/services/graph/focus-intent.ts` (baru)      | timer/state controller                                  |
| `apps/web/core/components/services/graph/focus-intent.test.ts` (baru) | unit test fake timer                                    |
| `apps/web/core/components/services/graph/use-service-focus.ts` (baru) | React adapter hook                                      |
| `apps/web/core/components/services/graph/service-graph-canvas.tsx`    | event React Flow, effective id, `AnimatePresence`       |
| `apps/web/core/components/services/graph/service-focus-card.tsx`      | `motion.div`, stagger, pinned indicator, reduced motion |
| `apps/web/core/components/services/graph/service-node.tsx`            | `motion.div` hover/tap                                  |
| `apps/web/styles/globals.css`                                         | hapus keyframe card lama                                |

## Edge cases

- **Cursor menyeberang node → card:** leave node menjadwalkan close 140ms; `pointerenter` card membatalkan. Gap 10px aman.
- **Cursor keluar cepat:** open timer dibatalkan; card tidak pernah muncul.
- **Card di-pin lalu cursor keluar:** card tetap; edge highlight tetap.
- **Hover B saat A pinned:** tidak ada perubahan card; node B tetap mendapat lift CSS.
- **Drag node:** hover ditutup dan diblokir; card pinned tetap mengikuti node yang di-drag.
- **Pan/zoom:** hover ditutup/diblokir saat mulai move. Card pinned tetap mengikuti node.
- **Node keluar viewport:** card mengikuti aturan hide yang sudah ada; pin tetap tersimpan dan card muncul lagi saat node kembali.
- **Touch:** hover handler tidak dijalankan; tap mem-pin.
- **Double-click:** detail penuh tetap terbuka seperti sekarang.
- **Card berubah ukuran saat expand:** posisi screen-space tetap; tidak ada layout animation pada card luar.
- **Reduced motion:** semua transisi menjadi fade/instan.

## Testing

Unit (`focus-intent.test.ts`, Vitest fake timers):

1. Hover tidak membuka card sebelum 180ms; membuka tepat setelahnya.
2. Leave sebelum 180ms membatalkan open.
3. Leave setelah open menutup setelah 140ms; enter card membatalkan close.
4. Pin mencegah hover node lain mengganti card.
5. `pin(B)` saat A pinned memindahkan pin.
6. `clear()` menutup card dan membatalkan semua timer.
7. `suppress()` membatalkan hover dan mencegah hover baru sampai `resume()`.
8. `dispose()` membersihkan timer.

Existing graph tests tetap dijalankan:

- `graph-clusters`, `graph-groups`, `graph-relationships`, `node-sync` (23 tests).

Verifikasi:

- `pnpm --filter=web check:types`
- `pnpm exec oxlint <file yang berubah>`
- `pnpm exec oxfmt --check <file yang berubah>`
- `pnpm turbo run build --filter=web`
- restart `plane-web-prod.service`

Manual:

- hover < 180ms tidak membuka; hover 200ms membuka;
- menyeberang ke card tidak flicker;
- pin, hover node lain, klik node lain, Esc, klik kanvas;
- drag, connect, pan/zoom;
- touch emulation tap-pin;
- `prefers-reduced-motion`.

## Rollback

Semua perubahan terisolasi di web graph. Rollback = revert commit fitur; tidak ada migrasi atau perubahan API.
