# Service Graph Layout Reset — Design

**Date:** 2026-10-25
**Status:** Approved design; pending written-spec review
**Scope:** `apps/api-rs`, `apps/web`, docs.
**Related:** [`2026-09-13-services-feature-design.md`](./2026-09-13-services-feature-design.md), [`2026-10-16-galileo-service-dependencies-design.md`](./2026-10-16-galileo-service-dependencies-design.md), [`2026-10-21-workspace-parity-design.md`](./2026-10-21-workspace-parity-design.md).

## Context

Spec awal Services menjanjikan **auto layered layout (dagre) + node draggable dengan posisi tersimpan + tombol Re-layout**. Implementasinya tidak pernah benar-benar berfungsi:

1. `service-graph-canvas.tsx` menyinkronkan `translatedNodes` ke state React Flow dengan comparator yang hanya membandingkan label/health, **bukan `position`**. Saat dependency dibuat, dagre menghitung posisi baru tetapi state node menolak update; edge baru digambar di atas posisi lama sehingga graph berantakan.
2. Bug comparator yang sama membuat tombol **Re-layout** terasa tidak bekerja: posisi tersimpan dikosongkan, dagre menghitung layout baru, tetapi state node tidak pernah memakai hasilnya.
3. `updateNodePosition` dan Re-layout di mode workspace memakai endpoint project (`/projects/:project_id/services/:pk/`) padahal graph workspace bisa berisi service lintas project. Untuk service yang tidak ter-link ke project filter, request gagal; untuk user non-workspace-admin, `gate_writer` bisa menolak.
4. Tidak ada mekanisme yang mereset posisi saat topologi berubah dari sumber selain canvas (tab Dependencies, API), sehingga layout lama tetap dipakai setelah reload.

Tujuan: jadikan layout graph deterministik dan rapi setiap topologi berubah, perbaiki Re-layout, dan tutup gap workspace scope.

## Keputusan yang dikunci

1. **Auto re-layout saat topologi berubah** — setiap service dibuat/dihapus atau dependency dibuat/dihapus, layout dihitung ulang. Posisi manual yang ada di-reset.
2. **Semua sumber perubahan** — reset dipicu dari backend, bukan hanya canvas. Perubahan via graph, tab Dependencies, atau API menghasilkan perilaku sama.
3. **Penyimpanan** — posisi dikosongkan (`position = NULL`), bukan menghitung/menyimpan posisi dagre per node. Client menjalankan dagre saat graph dibuka; dagre deterministik sehingga semua client menghasilkan layout sama.
4. **Cakupan reset topologi** — seluruh service di workspace (satu UPDATE), termasuk project lain.
5. **Pemicu** — service create/delete dan dependency create/delete. Perubahan tipe dependency **tidak** memicu reset karena topologi tidak berubah.
6. **Tombol Re-layout** — tetap manual; mereset posisi service yang tampil di graph saat ini lalu menjalankan dagre. Ini berbeda dari reset topologi yang workspace-wide.
7. **Viewport** — `fitView` otomatis setelah auto re-layout (topologi berubah) dan setelah tombol Re-layout. Drag manual, update health, ganti bahasa, dan filter tidak menggeser viewport.
8. **Drag manual tetap persisten** — posisi node yang di-drag disimpan (project atau workspace endpoint) dan bertahan setelah refresh sampai topologi berubah lagi.
9. **Tanpa schema/migration baru** — `position = NULL` tetap berarti "pakai auto-layout".

## Non-goals

- Realtime sync layout antar client/user (client lain melihat hasilnya saat refetch/reload).
- Mempertahankan susunan manual saat topologi berubah.
- Layout versioning atau revision column.
- Mengganti algoritma dagre, mengganti React Flow, atau optimasi graph ratusan node.
- Reset layout saat dependency type diubah, project link ditambah/dihapus, atau service di-transfer.
- Menyimpan posisi hasil auto-layout ke backend.
- Undo/redo untuk layout.

## Arsitektur

```
Mutasi topologi (service/dependency create/delete)
        │
        ▼
api-rs handler ── transaksi ──► UPDATE services SET position = NULL
        │                        WHERE workspace_id = ... AND deleted_at IS NULL
        ▼
response sukses
        │
        ▼
Web store ── clearLayoutPositions() ──► semua position di serviceMap = null
        │                               layoutEpoch++
        ▼
Canvas ── dagre (semua position null) ──► setNodes ──► fitView
```

Posisi manual:

```
Drag stop ──► PATCH position (workspace/project scope) ──► serviceMap
           └─ tidak mengubah layoutEpoch, tidak fitView
```

Re-layout manual:

```
Button ──► reset position untuk service yang tampil ──► layoutEpoch++ ──► dagre ──► fitView
```

## Backend

File utama: `apps/api-rs/crates/api/src/routes/service.rs`.

### Helper reset

```rust
async fn reset_workspace_layout(
    tx: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE services SET position = NULL \
         WHERE workspace_id = $1 AND deleted_at IS NULL AND position IS NOT NULL",
    )
    .bind(workspace_id)
    .execute(tx)
    .await?;
    Ok(())
}
```

Aturan:

- `updated_at` dan `updated_by_id` **tidak** diubah; posisi adalah metadata layout, bukan perubahan konten.
- Hanya service hidup (`deleted_at IS NULL`) yang di-reset.
- Satu UPDATE per mutasi; tidak ada request per node.

### Titik pemanggilan

| Handler/core                   | Perubahan                                                                                                                                   |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `create_service_core`          | Setelah insert service + owner link, panggil reset di tx yang sudah ada.                                                                    |
| `destroy_service_core`         | Setelah soft-delete service + dependency + work-item links, panggil reset di tx yang sudah ada. `workspace_id` diambil sebelum soft-delete. |
| `create_dependency_core`       | Bungkus INSERT dependency + reset dalam satu transaksi baru; commit sebelum membangun response.                                             |
| `dependency_destroy`           | Bungkus soft-delete dependency + reset dalam transaksi.                                                                                     |
| `dependency_destroy_workspace` | Sama seperti di atas.                                                                                                                       |

Tidak ada perubahan pada gate permission. Mutasi yang ditolak (`bad_request` duplikat/cycle, `deny`, `missing`) keluar sebelum reset, sehingga posisi tidak berubah.

Endpoint project dan workspace sama-sama mereset seluruh workspace; ini disengaja agar workspace graph selalu konsisten.

## Frontend

### 1. Sinkronisasi node canvas

File baru: `apps/web/core/components/services/graph/node-sync.ts`.

Fungsi murni `syncGraphNodes(prev: Node[], next: Node[]): Node[]`:

- Jika semua node sama (id, posisi, field tampilan, referensi `service`) → kembalikan array `prev` (referensi tidak berubah).
- Jika ada perubahan → hasilkan node baru dari `next`.
- Pertahankan `selected` dari node sebelumnya.
- Jika node sebelumnya sedang `dragging`, kembalikan node sebelumnya apa adanya (jangan menggeser node di tengah drag).
- Bandingkan juga referensi `data.service` agar perubahan nama/atribut service langsung terlihat.
- Field yang dibandingkan minimal: `position.x`, `position.y`, `statusLabel`, `criticalityLabel`, `typeLabel`, `healthLabel`, `ownerProjectIdentifier`, `health`, `incidents.length`, `lastDeployedAt`, `noDeploymentLabel`, `dimmed`, dan `data.service`.

`service-graph-canvas.tsx` memakai helper ini di effect sinkronisasi, menggantikan comparator inline. Sinkronisasi edge tidak berubah.

### 2. Store

File: `apps/web/core/store/service.store.ts`.

- Tambah observable `layoutEpoch: number = 0` (interface + `makeObservable`).
- Tambah action `markLayoutChanged()` yang hanya menaikkan `layoutEpoch`; dipakai setelah Re-layout manual selesai.
- Tambah action `clearLayoutPositions()`:
  - set `position: null` untuk setiap service di `serviceMap` yang posisinya tidak null;
  - memanggil `markLayoutChanged()`.
- Panggil `clearLayoutPositions()` setelah sukses:
  - `addDependency`
  - `removeDependency`
  - `createService` dan `createWorkspaceService`
  - `deleteService` dan `deleteWorkspaceService` (setelah service dihapus dari map)
- `updateNodePosition` berubah menjadi:

```ts
updateNodePosition(
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  serviceId: string,
  position: { x: number; y: number } | null
): Promise<void>
```

`projectId === null` memakai `serviceService.updateWorkspaceService`; selain itu memakai endpoint project. `updateNodePosition` melakukan optimistic update ke `serviceMap` sebelum request dan mengembalikan service asli jika request gagal (pola sama dengan `updateService`). Ini membuat Re-layout langsung terlihat sebelum semua PATCH selesai.

File: `apps/web/core/services/service.service.ts`.

- `updateNodePosition` menerima `projectId: string | null` dan `position: { x: number; y: number } | null`.
- Jika `projectId === null`, delegasikan ke `updateWorkspaceService`; selain itu ke `updateService` (endpoint project).

### 3. Graph component

File: `apps/web/core/components/services/graph/service-graph.tsx`.

- `handleNodeDragStop`: gunakan `scope === "workspace" ? null : pid` sebagai `projectId`; jangan return lebih awal hanya karena `pid` kosong di workspace scope.
- `handleReLayout`:
  - target = service penuh (`!isLiteService`) di `graphData.services` yang punya `position !== null`;
  - tidak perlu `pid` untuk workspace scope;
  - panggil `updateNodePosition(..., null)` via `Promise.allSettled`;
  - tampilkan toast error jika ada yang gagal;
  - setelah selesai, panggil `markLayoutChanged()` untuk memicu `fitView` dan reset loading state.
- Baca `layoutEpoch` dari store dan teruskan ke `ServiceGraphCanvas`.

Catatan: reset topologi sudah dilakukan backend; `handleReLayout` hanya untuk reset manual posisi yang tampil. Karena itu ia tidak memakai endpoint bulk.

### 4. Fit viewport

`ServiceGraphCanvas` menerima prop `layoutEpoch: number`.

- Simpan `lastEpoch` di ref, inisialisasi dengan nilai mount.
- Effect: jika `layoutEpoch` berubah dan `instance` tersedia, jalankan `fitView` setelah dua `requestAnimationFrame` (agar `setNodes` dari effect sinkronisasi sudah diterapkan), dengan padding wajar.
- Initial mount tetap memakai mekanisme fit yang ada (`fitDone` + `ResizeObserver`).
- Cleanup membatalkan RAF yang belum jalan.
- Tidak ada `fitView` untuk perubahan health/label/filter/drag.

## Matriks perilaku

| Kejadian                                  | Backend                                      | Store lokal                             | Canvas                            |
| ----------------------------------------- | -------------------------------------------- | --------------------------------------- | --------------------------------- |
| Dependency dibuat/dihapus (UI/API)        | Reset semua posisi workspace dalam transaksi | `clearLayoutPositions`, `layoutEpoch++` | Dagre layout baru + fit           |
| Service dibuat/dihapus                    | Reset semua posisi workspace dalam transaksi | `clearLayoutPositions`, `layoutEpoch++` | Dagre layout baru + fit           |
| Mutasi ditolak (duplikat/cycle/forbidden) | Tidak ada reset                              | Tidak ada perubahan                     | Tidak berubah                     |
| Tipe dependency diubah                    | Tidak ada reset                              | Update edge label                       | Tidak berubah                     |
| Node di-drag                              | Simpan satu posisi                           | Update satu service                     | Node tetap, tanpa fit             |
| Tombol Re-layout                          | PATCH posisi `null` per service yang tampil  | Posisi jadi null, `markLayoutChanged()` | Dagre + fit                       |
| Health poll / ganti bahasa / filter       | —                                            | —                                       | Update data saja, tanpa reset/fit |

## Error handling

- Reset berjalan dalam transaksi yang sama dengan mutasi; jika UPDATE gagal, mutasi ikut rollback dan client menerima error. Tidak ada dependency/service "setengah jadi".
- Re-layout manual memakai `Promise.allSettled`; kegagalan sebagian memunculkan toast dan tidak membatalkan node yang sudah berhasil.
- Jika `fitView` gagal karena canvas belum terukur, error diabaikan (perilaku yang sama dengan fit awal).
- Client lain yang membuka graph saat mutasi terjadi tidak menerima push; setelah fetch/reload berikutnya posisi sudah null dan layout baru dihitung.

## Testing

### Rust

Tambahkan integration test (file baru atau perluasan `service_workspace_test.rs`):

1. Set posisi beberapa service, buat dependency via endpoint project → semua service workspace `position = NULL`.
2. Buat dependency via endpoint workspace → sama.
3. Hapus dependency (project + workspace) → semua posisi `NULL`.
4. Buat service → semua posisi workspace `NULL`.
5. Hapus service → posisi service tersisa `NULL`.
6. Percobaan dependency duplikat/cycle → posisi tidak berubah.
7. `updated_at` service tidak berubah akibat reset layout.

Test DB-backed dijalankan dengan `--test-threads=1` bila memakai helper purge yang sama.

### Web

1. `node-sync.test.ts`:
   - layout/posisi berubah → node baru dipakai;
   - label/service berubah → data baru dipakai;
   - tidak ada perubahan → referensi array `prev` dipertahankan;
   - `selected` dipertahankan;
   - node `dragging` tidak dipindahkan.
2. `service.store.test.ts`:
   - `addDependency`/`removeDependency` mengosongkan posisi dan menaikkan `layoutEpoch`;
   - `createService`/`createWorkspaceService`/`deleteService`/`deleteWorkspaceService` melakukan hal sama;
   - `updateNodePosition` memakai `updateWorkspaceService` saat `projectId === null` dan endpoint project saat tidak null;
   - rollback posisi jika request gagal.
3. Manual smoke:
   - tambah edge dari canvas dan dari tab Dependencies → graph rapi + fit;
   - hapus edge → graph rapi + fit;
   - drag node → refresh → posisi bertahan;
   - Re-layout di project graph dan workspace graph;
   - health poll tidak menggeser node;
   - non-admin/project writer bisa drag dan Re-layout pada service yang boleh ditulis.

## Rollout

1. Implementasi backend + test, rebuild `api/worker/beat-worker` (Docker), verifikasi `/health`.
2. Implementasi web + test, `pnpm --filter=web build`, restart `plane-web-prod.service`.
3. Smoke lewat tunnel/LAN pada project graph dan workspace graph.
4. Tidak ada migration; rollback cukup dengan revert kode. Data `position` yang sudah null akan diisi ulang oleh dagre saat graph dibuka.

## Risiko & trade-off

- **Susunan manual workspace hilang saat topologi berubah** — konsekuensi langsung dari keputusan "selalu auto re-layout".
- **Workspace-wide reset** juga menghapus susunan project lain saat satu dependency berubah. Diterima agar workspace graph tidak mencampur posisi lama/baru.
- **Client lain tidak realtime** — tanpa mekanisme sinkronisasi, layout baru terlihat setelah refetch/reload. Ini konsisten dengan service graph saat ini yang juga tidak realtime.
- **N PATCH untuk tombol Re-layout** — hanya untuk aksi manual dan service yang tampil; bukan jalur kritis. Jika jumlah node menjadi masalah, bulk endpoint bisa ditambahkan di iterasi berikutnya.
