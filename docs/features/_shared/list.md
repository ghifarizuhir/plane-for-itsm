---
status: current
last_verified: 2026-10-27
source_hash: sha256:cb827bbb6705b6ab90b11e7c371117e632b961dabd32f87aadc908390232b4da
source_paths:
  - apps/web/core/components/issues
  - apps/web/core/components/views
  - apps/web/core/components/cycles
  - apps/web/core/components/modules
---

# List — Filter, Sort, and Pagination

Status: **Current.**

## Purpose

Perilaku list dipakai bersama oleh ticket, sprint, track, view, dan queue.

## Behavior

- Filter disimpan di URL agar bisa dibagikan dan di-restore.
- Sort default per surface; user dapat mengganti urutan.
- Pagination mengikuti endpoint: page-based atau cursor-based.
- Application scope selector membatasi data di halaman agregat workspace.
- Empty, loading, dan error state harus konsisten.

## Layouts

- Table/list untuk data padat.
- Board/kanban untuk state.
- Calendar/gantt untuk tanggal dan timeline.
- Spreadsheet untuk editing cepat.

Layout bukan sumber kebenaran data; semuanya membaca filter yang sama.

## Permission

List hanya menampilkan entity yang boleh dibaca user. Jangan melakukan
client-side filtering sebagai pengganti permission API.

## Edge cases

- Filter yang menunjuk state/label terhapus harus menampilkan pesan atau
  diabaikan dengan aman.
- Pagination harus reset saat filter berubah.
- Empty state berbeda antara "tidak ada data" dan "tidak ada hasil filter".

## Related

- [`detail-page.md`](./detail-page.md)
- [`../views.md`](../views.md)
- [`../work-items.md`](../work-items.md)
