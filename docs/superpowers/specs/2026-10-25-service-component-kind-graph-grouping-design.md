# Service Component Kind & Graph Grouping — Design

**Date:** 2026-10-25
**Status:** Approved design; pending written-spec review
**Scope:** `apps/api-rs`, `apps/web`, `packages/types`, `packages/i18n`, docs.
**Related:** [`2026-09-13-services-feature-design.md`](./2026-09-13-services-feature-design.md), [`2026-10-21-workspace-services-grouping-design.md`](./2026-10-21-workspace-services-grouping-design.md), [`2026-10-25-service-graph-layout-reset-design.md`](./2026-10-25-service-graph-layout-reset-design.md).

## Konteks

Service saat ini punya satu klasifikasi `type` dengan nilai `internal`, `external`, `infrastructure`, dan `third_party`. Nilai itu menjawab pertanyaan **batas/ownership** (internal vs pihak ketiga), bukan **jenis komponen** yang dikelola. Untuk katalog service/CMDB, user perlu klasifikasi yang lebih konkret seperti database, server, cache, atau queue. Menaruh kedua konsep dalam satu enum akan menghilangkan informasi boundary yang sudah dipakai filter, ikon node, dan AI tools.

Graph service juga belum mengelompokkan node. Semua node di-layout dagre sebagai satu graf datar, sehingga database, server, dan application service bercampur. User ingin graph menampilkan klaster per aplikasi/project dan per jenis komponen.

## Keputusan yang dikunci

1. **Dua dimensi terpisah.** `type` lama tetap. Field baru `kind` menyatakan jenis komponen: `service`, `database`, `server`, `cache`, `queue`, `storage`, `network`, `external_api`, `other`.
2. **Daftar kind tetap (bukan configurable per workspace).** Cukup lengkap untuk ITSM tanpa menambah tabel lookup dan UI admin.
3. **Backfill aman.** Semua service lama menjadi `kind = 'service'`. Tidak ada tebakan otomatis dari `type`.
4. **Grouping graph Project × Kind.** Klaster dibentuk dari pasangan owner project dan kind. Service tanpa owner masuk klaster **Unassigned × Kind**; service lite masuk satu klaster **Restricted** karena kind/project-nya tidak diekspos.
5. **Frame mengikuti node.** Posisi manual tetap dipakai. Bounding box klaster dihitung dari posisi node anggota, sehingga drag tidak menghilangkan posisi dan frame ikut bergerak.
6. **Layout per klaster.** Tiap klaster di-layout dagre sendiri, lalu klaster dikemas grid. Ini menghasilkan susunan awal yang rapi tanpa mengubah persistensi posisi absolut.
7. **Grouping selalu aktif** di graph project, workspace, dan war room. Tidak ada toggle.
8. **Perubahan kind mengosongkan posisi service itu saja**, bukan seluruh workspace. Node lalu masuk klaster barunya. Perubahan `type`/status/nama tidak menyentuh layout.
9. **Tanpa breaking change.** Field aditif di API; endpoint, response lama, dan filter lama tetap bekerja.

## Non-goals

- Kind yang bisa ditambah/diubah per workspace.
- Grouping di list view, bulk edit kind, atau impor/ekspor kind.
- Metrik, SLO, atau dashboard yang di-group by kind.
- Rename label `Type` lama menjadi `Boundary`/`Ownership`.
- Warna/ikon kind yang bisa dikustomisasi user.
- Parent/container node React Flow atau posisi relatif.
- Realtime sync layout/kind antar client.

## Arsitektur

```
Form / sidebar / AI / API
        │  PATCH { kind }
        ▼
api-rs ── validasi enum ──► UPDATE services SET kind = $n [, position = NULL]
        │
        ▼
Web store ── update serviceMap ──► markLayoutChanged() (hanya jika kind berubah)
        │
        ▼
Canvas ── group Project × Kind ──► dagre per group ──► pack groups ──► frame dari node
```

Graph load:

```
services + dependencies
        │
        ▼
getLayoutedElements
  ├─ group by (owner_project_id, kind)
  ├─ dagre per group
  ├─ pack group bounding boxes
  ├─ override dengan stored position
  └─ nodes + edges
        │
        ▼
getServiceClusters(nodes) ──► frame Project × Kind (ViewportPortal)
```

## Backend

### Migrasi

File baru: `apps/api-rs/migrations/0025_service_kind.sql`.

```sql
ALTER TABLE public.services
    ADD COLUMN IF NOT EXISTS kind varchar(20) NOT NULL DEFAULT 'service';

ALTER TABLE public.services
    DROP CONSTRAINT IF EXISTS services_kind_check;

ALTER TABLE public.services
    ADD CONSTRAINT services_kind_check
    CHECK (kind IN ('service', 'database', 'server', 'cache', 'queue',
                    'storage', 'network', 'external_api', 'other'));
```

- Data lama otomatis `service`; `updated_at` tidak berubah.
- Tidak ada index baru: filter service berjalan di client, dan graph mengambil seluruh service workspace.
- Migrasi idempoten mengikuti pola `0023_service_dependency_type.sql`.

### Konstanta bersama

File baru: `apps/api-rs/crates/common/src/models/service.rs`, diekspor dari `models/mod.rs`.

```rust
pub const SERVICE_KINDS: [&str; 9] = [
    "service", "database", "server", "cache", "queue",
    "storage", "network", "external_api", "other",
];
```

`api` dan `ai` sama-sama sudah bergantung pada crate `common`, sehingga daftar tidak diduplikasi. `SERVICE_TYPES` lama tetap di tempatnya (tidak ada refactor di luar scope).

### Row, select, serializer

File: `apps/api-rs/crates/api/src/routes/service.rs`.

- `ServiceRow` tambah `pub kind: String`.
- `SERVICE_SELECT` tambah `s.kind`.
- `service_json` tambah `"kind": row.kind`.
- Lite projection (`ServiceLiteRow`/JSON lite) **tidak** menambah `kind`, konsisten dengan `type` yang juga tidak diekspos.

### Create

`CreateService` tambah `pub kind: Option<String>`.

- Default `"service"` bila tidak dikirim.
- Validasi `validate_enum("kind", &kind, &SERVICE_KINDS)`.
- INSERT menambah kolom `kind`.
- Response memuat kind.

### Update

`PatchService` tambah `pub kind: Option<String>`.

- Nilai baru = `body.kind.unwrap_or(current.kind)`.
- Validasi enum.
- `kind_changed = new_kind != current.kind`.
- Jika `kind_changed`, `position` di-set `NULL` dalam UPDATE yang sama. Ini mengalahkan `body.position` bila keduanya dikirim, karena memindahkan node ke klaster baru tanpa mengosongkan posisi akan menyesatkan.
- Jika kind tidak berubah, `position` mengikuti perilaku sekarang (`body.position` atau `current.position`).
- `updated_at`/`updated_by_id` tetap diperbarui seperti update service biasa.

### AI tools

File: `apps/api-rs/crates/ai/src/tools/services.rs` dan `mutations.rs`.

- Import `common::models::service::SERVICE_KINDS`.
- `list_services`: tambah filter opsional `kind` (allowlist), diterapkan ke query seperti `service_type`.
- `create_service`: tambah argumen `kind` opsional, default `service`.
- `update_service`: tambah perubahan `kind` opsional.
- JSON service yang dikembalikan ke model memuat `kind`.
- Tool description menyebut nilai yang valid.

## Frontend data & API

### Types

File: `packages/types/src/service/core.ts`.

```ts
export type TServiceKind =
  | "service"
  | "database"
  | "server"
  | "cache"
  | "queue"
  | "storage"
  | "network"
  | "external_api"
  | "other";

export interface IService {
  // ...
  kind: TServiceKind;
}
```

File: `packages/types/src/service/filters.ts`: `kind?: TServiceKind[]`.

`ILiteService` tetap tanpa `kind` dan `type`.

### Filter logic

File: `apps/web/core/services/service.helpers.ts`.

```ts
if (filters.kind && filters.kind.length > 0 && !filters.kind.includes(service.kind)) return false;
```

### AI proposal web

File: `apps/web/core/lib/ai-proposals.ts`.

- `SERVICE_KIND_VALUES` (sembilan nilai).
- `TAiCreateServiceProposal.kind?: string | null`.
- `TAiUpdateServiceChanges.kind?: string | null`.
- Validator menerima kind hanya dari daftar.

File: `apps/web/core/lib/ai-proposal-forms.ts`: tambah field select `kind` pada form create dan update service.

### Mock repository

File: `apps/web/core/services/service-mock.repository.ts`: service mock memakai `kind` yang masuk akal (minimal satu database/server agar grouping terlihat saat memakai mock).

## Frontend graph

### Konfigurasi kind

File baru: `apps/web/core/components/services/kind-config.ts`.

- `SERVICE_KIND_ORDER`: urutan dropdown dan layout.
- `SERVICE_KIND_ICON`: ikon propel per kind (mis. `CubeOutline`, `LibraryOutline`, `ServerOutline`, `CacheOutline`, `ListOutline`, `ArchiveOutline`, `ConnectionsOutline`, `GlobeOutline`, `BoxesOutline`).
- `SERVICE_KIND_CLUSTER_STYLE`: kelas Tailwind statis untuk frame/label, dengan varian dark. Warna: service sky, database violet, server orange, cache pink, queue amber, storage emerald, network cyan, external_api indigo, other neutral.
- `RESTRICTED_CLUSTER_STYLE`: netral/dashed.

Kelas Tailwind harus literal (bukan string yang dirakit) agar terdeteksi build.

### Grouping & layout

File baru: `apps/web/core/components/services/graph/graph-groups.ts`.

```ts
export type TServiceGroup = {
  key: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  serviceIds: string[];
};

export const getServiceGroups = (services: (IService | ILiteService)[]): TServiceGroup[];
```

Aturan group:

- Lite → `key = "__restricted__"`, `kind = null`, `restricted = true`.
- Full tanpa `owner_project_id` → `key = "__unassigned__::<kind>"`, `projectId = null`.
- Full dengan owner → `key = "<project_id>::<kind>"`.
- Urutan deterministik: group bernama (project id) lalu Unassigned per kind, Restricted terakhir; di dalam group, service diurutkan berdasarkan nama.

`getLayoutedElements` di `use-graph-layout.ts` diubah:

1. Buat group.
2. Untuk tiap group, buat dagre graph berisi service anggota dan edge yang kedua endpoint-nya ada di group. Konfigurasi tetap `rankdir: "LR"`, `nodesep: 48`, `ranksep: 112`.
3. Hitung bounding box hasil dagre per group.
4. Kemas group secara deterministik: kiri-ke-kanan, wrap pada lebar maksimum 1800px, gap 64px horizontal/vertikal. Padding klaster 24px, header 32px.
5. Posisi default node = slot group + offset dagre + padding/header.
6. Override dengan `service.position` bila ada.
7. Untuk group yang sebagian node-nya punya posisi tersimpan: hitung centroid posisi tersimpan dan centroid posisi default, lalu geser node yang belum punya posisi sebesar selisih kedua centroid. Node yang sudah punya posisi tidak diubah.
8. Bangun edges seperti sekarang; edge lintas group tetap dirender, hanya tidak dipakai saat dagre per group.

Fungsi mengembalikan `{ nodes, edges }` dengan bentuk yang sama seperti sekarang, sehingga `syncGraphNodes` dan `fitView` tidak perlu berubah. Node service diberi `zIndex: 10` agar frame klaster berada di belakangnya.

### Frame klaster

File baru: `apps/web/core/components/services/graph/graph-clusters.ts`.

```ts
export type TServiceCluster = {
  id: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
};

export const getServiceClusters = (
  nodes: Node[],
  services: (IService | ILiteService)[],
  projectLookup: Record<string, { identifier: string; name: string }> | undefined,
  t: (key: string) => string
): TServiceCluster[];
```

- Bounding box dihitung dari `node.position` dan ukuran node (`measured` bila ada, fallback `SERVICE_NODE_WIDTH`/`SERVICE_NODE_HEIGHT`).
- Frame: `x = minX - 24`, `y = minY - 32 - 24`, `width = maxX - minX + 48`, `height = maxY - minY + 32 + 48`.
- Label:
  - Restricted → `t("service.graph.restricted")`.
  - Unassigned → `t("service.graph.unassigned")` + label kind.
  - Named project → identifier project (dari `projectLookup`) + label kind.
  - Prefix project/Unassigned hanya ditampilkan bila graph punya lebih dari satu project-key (termasuk Unassigned), supaya graph satu project tidak penuh label berulang.
  - Pemisah label: `" · "`.
- Frame dihitung ulang setiap `nodes` berubah, sehingga ikut saat drag.

### Rendering

File: `apps/web/core/components/services/graph/service-graph-canvas.tsx`.

- Import `ViewportPortal` dari `@xyflow/react`.
- `clusters = useMemo(() => getServiceClusters(nodes, graphData.services, projectLookup, t), [...])`.
- Render di dalam `<ReactFlow>`:

```tsx
<ViewportPortal>
  {clusters.map((cluster) => (
    <div
      key={cluster.id}
      className={cn("pointer-events-none absolute rounded-xl border", clusterStyle(cluster))}
      style={{
        transform: `translate(${cluster.x}px, ${cluster.y}px)`,
        width: cluster.width,
        height: cluster.height,
        zIndex: 0,
      }}
    >
      <span className="...">{cluster.label}</span>
    </div>
  ))}
</ViewportPortal>
```

- Frame tidak selectable/clickable, tidak masuk minimap, dan tidak memengaruhi event node karena `pointer-events-none`.
- `fitView` memakai padding sedikit lebih besar (0.25) agar frame tidak terpotong.
- Node service memakai ikon kind dan `title` label kind; label `type` lama tetap tampil seperti sekarang.
- War room ikut mendapat grouping karena memakai canvas yang sama.

## UI lain

### Form create/update

File: `apps/web/core/components/services/service-form.tsx`.

- `SERVICE_KIND_OPTIONS` dari `SERVICE_KIND_ORDER`.
- Dropdown **Component kind** di samping Type.
- `defaultValues` dan `useForm` default `kind: "service"`; nilai edit memakai `data.kind`.

### Sidebar detail

File: `apps/web/core/components/services/detail/sidebar.tsx`.

- Properti **Component kind** dengan `ServicePropertySelect`, opsi dari `SERVICE_KIND_ORDER`, prefix i18n `service.kind_values`.
- Perubahan kind memakai `updateService` yang sudah ada; store akan menaikkan `layoutEpoch` bila kind berubah.

### Filter

File baru: `apps/web/core/components/services/filters/kind.tsx`, mengikuti pola `filters/type.tsx`.

- `FilterServiceKind` multi-select.
- Dimasukkan ke `filters/root.tsx`.
- `applied-filters/root.tsx` menambah mapping `kind: (value) => \`service.kind_values.${value}\``.
- Filter memakai localStorage yang sudah ada; tidak ada perubahan store filter.

## i18n

Tambahkan key berikut ke `service.json` untuk **semua 20 locale** (mengikuti skill translate):

```json
{
  "fields": {
    "kind": "Component kind"
  },
  "kind_values": {
    "service": "Service",
    "database": "Database",
    "server": "Server",
    "cache": "Cache",
    "queue": "Queue",
    "storage": "Storage",
    "network": "Network",
    "external_api": "External API",
    "other": "Other"
  },
  "graph": {
    "unassigned": "Unassigned",
    "restricted": "Restricted"
  }
}
```

Terjemahan mengikuti istilah teknis yang sudah dipakai tiap locale; `external_api` diterjemahkan sebagai istilah API, bukan dipaksakan.

## Matriks perilaku

| Kejadian                             | Backend                                                                                   | Store                                   | Canvas                               |
| ------------------------------------ | ----------------------------------------------------------------------------------------- | --------------------------------------- | ------------------------------------ |
| Service dibuat (dengan/s tanpa kind) | Kind default `service`; reset seluruh posisi workspace (perilaku topologi yang sudah ada) | `clearLayoutPositions`, `layoutEpoch++` | Layout per klaster + fit             |
| Kind diubah                          | Posisi service itu jadi `NULL`                                                            | Service di-update, `layoutEpoch++`      | Node pindah klaster + fit            |
| Type/status/nama diubah              | Tidak menyentuh posisi                                                                    | Tidak ada layout change                 | Tidak berubah                        |
| Dependency dibuat/dihapus            | Reset seluruh posisi workspace                                                            | `clearLayoutPositions`, `layoutEpoch++` | Layout per klaster + fit             |
| Node di-drag                         | Simpan satu posisi                                                                        | Update satu service                     | Frame ikut bergerak, tanpa fit       |
| Re-layout                            | PATCH `position: null` untuk service yang tampil                                          | Posisi null, `markLayoutChanged()`      | Layout per klaster + fit             |
| Filter kind berubah                  | —                                                                                         | Filter state                            | Node/klaster yang tersisa, tanpa fit |
| Health poll / ganti bahasa           | —                                                                                         | Data saja                               | Frame menyesuaikan data, tanpa fit   |
| Service lite                         | Kind tidak diekspos                                                                       | —                                       | Klaster Restricted                   |
| Service tanpa owner                  | —                                                                                         | —                                       | Klaster Unassigned × Kind            |

## Error handling

- Kind invalid ditolak `400` dengan pesan `validate_enum`, sama seperti `type`/`status`.
- Create/update gagal tidak mengubah posisi atau kind.
- Perubahan kind yang berhasil tetapi `markLayoutChanged` tidak terpanggil hanya membuat viewport tidak fit; layout tetap benar pada render berikutnya. Implementasi tetap memanggilnya setelah response sukses.
- Jika `projectLookup` tidak memuat project (mis. data belum termuat), label klaster jatuh ke kind saja; frame tetap tampil.
- Frame tidak boleh menangkap pointer event; seluruh frame memakai `pointer-events-none`.
- Graph kosong atau satu service tetap valid: group/klaster minimal satu.

## Testing

### Rust

Perluas `apps/api-rs/crates/api/tests/service_workspace_test.rs` (atau file baru):

1. Create tanpa kind → response `kind = "service"`, DB terisi.
2. Create dengan kind `database` → response dan DB sesuai.
3. Create kind invalid → `400`, service tidak dibuat.
4. Patch kind `database` → response berubah; `position` service itu `NULL`; service lain tetap.
5. Patch kind invalid → `400`, kind dan posisi tidak berubah.
6. Patch type/status → posisi tidak berubah.
7. List project/workspace memuat `kind`; lite projection tidak memuat `kind`.
8. AI allowlist: nilai valid diterima, invalid ditolak (unit test di crate `ai`).

Jalankan dengan `--test-threads=1` karena helper purge berbagi prefix.

### Web

1. `graph-groups.test.ts`:
   - group key Project × Kind;
   - lite → Restricted; tanpa owner → Unassigned × Kind;
   - urutan group deterministik;
   - group dikemas tanpa overlap saat tidak ada stored position;
   - stored position menang;
   - node tanpa posisi di group campuran digeser ke centroid posisi tersimpan;
   - edge lintas group tetap ada.
2. `graph-clusters.test.ts`:
   - bounds memperhitungkan padding/header;
   - label kind saja untuk satu project;
   - label memakai identifier project saat lebih dari satu project;
   - label Unassigned/Restricted;
   - perubahan posisi node menggeser frame.
3. `service.helpers` / `workspace-services.test.ts`: filter kind di project dan workspace.
4. `service.store.test.ts`: update kind menaikkan `layoutEpoch`; update field lain tidak.
5. Semua fixture `IService` di test (`service.store.test.ts`, `workspace-services.test.ts`) dan `service-mock.repository.ts` ditambah `kind` wajib agar typecheck tetap hijau.
6. Manual smoke:
   - create/edit kind dari form dan sidebar;
   - filter kind di project dan workspace;
   - graph project, workspace, dan war room menampilkan klaster yang benar;
   - drag node lalu refresh: posisi bertahan dan frame mengikuti;
   - Re-layout dan auto re-layout topologi;
   - ganti kind: node pindah klaster, node lain tidak bergeser;
   - health poll dan ganti bahasa tidak menggeser node;
   - light/dark mode;
   - service lite dan tanpa owner masuk klaster fallback.

### Verifikasi wajib

- `pnpm --filter=web check:types`
- `pnpm --filter=web check:lint`
- `pnpm --filter=web test`
- `cargo check -p api`
- Suite Rust service dengan DB.
- `pnpm --filter=web build`

## Rollout

1. Implementasi backend + migrasi + test.
2. Rebuild `api/worker/beat-worker`:
   `docker compose -f docker-compose-local.yml up -d --build api worker beat-worker`
3. Verifikasi `curl http://localhost:8000/health` → `200`.
4. Restart live: `systemctl --user restart plane-live.service`, cek `http://localhost:3100/live/health/`.
5. Build web + restart prod:
   `pnpm --filter=web build` lalu `systemctl --user restart plane-web-prod.service`, cek `http://localhost:3000`.
6. Smoke manual di project graph, workspace graph, dan war room.

Migrasi berjalan otomatis saat api start. Rollback kode tidak menghapus kolom `kind`; data tetap aman dan field diabaikan oleh versi lama.

## Risiko & trade-off

- **Frame bisa overlap setelah drag manual.** Karena frame mengikuti node, dua klaster yang node-nya ditarik berdekatan bisa saling menimpa. Diterima sesuai keputusan “frame mengikuti node”; Re-layout tersedia untuk merapikan.
- **Edge lintas klaster tidak memengaruhi dagre per group.** Edge bisa tampak panjang. Ini konsekuensi grouping; keterbacaan klaster lebih diprioritaskan.
- **Perubahan kind tidak merapikan seluruh graph.** Hanya node tersebut yang pindah; node lain tetap. Jika posisi barunya bertabrakan, user bisa menekan Re-layout.
- **Biaya komputasi frame saat drag.** `getServiceClusters` berjalan O(n) setiap perubahan node. Untuk jumlah service saat ini (puluhan–ratusan) aman; jika perlu, perhitungan bisa di-throttle dengan `requestAnimationFrame` tanpa mengubah desain.
- **Frame tidak dihitung `fitView`.** React Flow hanya mem-fit node. Padding 0.25 memberi ruang agar frame tidak terpotong; jika masih terpotong di viewport sempit, padding bisa dinaikkan.
- **Backfill semua ke `service`.** Graph lama tidak langsung terklasifikasi sebagai database/server; user perlu mengkurasi. Ini dipilih agar tidak ada salah klasifikasi otomatis.
- **Label `Type` dan `Component kind` berdampingan.** Dua dropdown bisa terasa redundan bagi user baru; label “Component kind” dan tooltip kind dipakai untuk membedakan.
