---
status: current
last_verified: 2026-10-27
source_hash: sha256:aa93072d7a3a42a96f4b13936bff1231406e8abc63ce1c32649db240b6e68e49
source_paths:
  - apps/api-rs/crates/api/src/routes/page.rs
  - apps/live
  - apps/web/core/store/pages
  - apps/web/core/components/docs
  - apps/web/app/routes/core.ts
---

# Knowledge (Pages and In-App Docs)

Status: **Current.**

## Intent

Knowledge base menyimpan artikel, runbook, dan catatan solusi dalam page tree
workspace. Page dapat dikolaborasikan realtime dan dipublikasikan lewat Space.
In-app docs adalah panduan produk yang terpisah dari knowledge base.

## Routes

| Surface               | Route                                                     |
| --------------------- | --------------------------------------------------------- |
| Workspace knowledge   | `/:workspaceSlug/pages`                                   |
| Application knowledge | `/:workspaceSlug/projects/:projectId/pages`               |
| Page detail           | `.../pages/:pageId`                                       |
| In-app docs           | `/:workspaceSlug/docs`, `/:workspaceSlug/docs/:topicSlug` |
| Legacy KB slug        | `/:workspaceSlug/kb` → `/pages`                           |

## Page tree

- Page dapat memiliki sub-page.
- Page tree bersifat workspace-level; application page tetap terikat pada
  application.
- Page mendukung urutan, judul, dan metadata.
- Page yang dihapus masuk soft delete dan dapat dipulihkan jika didukung.

## Editor and collaboration

- Editor: TipTap 2.
- Kolaborasi realtime: Yjs + Hocuspocus melalui `apps/live`.
- Presence dan update dokumen dikirim lewat WebSocket.
- Konflik diselesaikan oleh CRDT Yjs, bukan last-write-wins manual.
- Editor harus tetap bisa membuka dokumen read-only jika koneksi realtime
  terputus.

## Versions and comments

- Setiap perubahan deskripsi dapat menghasilkan version.
- Version dapat dilihat dan dibandingkan.
- Komentar dan activity mengikuti shared comments/activity.
- Attachment dan image mengikuti pipeline asset.

## Space publishing

- Page terpilih dapat dipublikasikan ke `apps/space`.
- Publikasi tidak otomatis; harus diaktifkan per page.
- Page publik tidak boleh membocorkan data workspace yang tidak dipublikasikan.

## In-app docs

In-app docs berada di `apps/web/core/components/docs` dan registry topik di
`apps/web/core/components/docs/registry.ts`. Ini adalah panduan user-facing,
bukan pengganti dokumentasi engineering di `docs/`.

## API touchpoints

- `page.rs` — CRUD, page tree, version.
- `apps/live` — collaboration server.
- Store: `apps/web/core/store/pages`.
- Tabel: `pages`, `descriptions`, `description_versions`.

## Permissions

| Role              | Create       | Read             | Update              | Publish        |
| ----------------- | ------------ | ---------------- | ------------------- | -------------- |
| Member            | Sesuai scope | Ya               | Page yang diizinkan | Tidak          |
| Application admin | Ya           | Ya               | Ya                  | Jika diizinkan |
| Workspace admin   | Ya           | Ya               | Ya                  | Ya             |
| Guest             | Tidak        | Page publik saja | Tidak               | Tidak          |

## Limitations

- Collaboration membutuhkan koneksi ke `apps/live`.
- Page publik adalah snapshot konten, bukan mirror live dari page private.
- In-app docs tidak otomatis tersinkron dengan `docs/`.

## In-app guide

`/:workspaceSlug/docs/knowledge-base`.

## Changelog

| Tanggal    | Perubahan                                                                 |
| ---------- | ------------------------------------------------------------------------- |
| 2026-10-27 | new — menggantikan pages.md, page tree, collaboration, Space, in-app docs |
