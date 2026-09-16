# Services Rich-Text Description (ala Work Items) — Design

Date: 2026-09-16
Status: Approved by user (option A + 3 poin)
Scope: Frontend `apps/web` (+ kecil di `packages/types` bila perlu). Backend tidak berubah (kolom `services.description` + `services.description_html` di Rust sudah ada).

## Goal

Samakan `description` Services ke `RichTextEditor` ala work items: formatting WYSIWYG + markdown shortcut (`**bold**`, `# heading`, `- list`, `code`, `> quote`, `[link](url)`), di dua permukaan: modal create/update + tab Overview detail (autosave).

## Konteks temuan

- Work items tidak pakai `react-markdown` untuk editing. Editing = `@plane/editor` (`packages/editor`, Tiptap/ProseMirror + `tiptap-markdown`, `Markdown.configure({ html: true, transformPastedText: true, breaks: true })` di `packages/editor/src/extensions/extensions.ts:108-113`). Simpan `description_html` (+ `description_json` untuk issue).
- Display read-only work items = editor `editable={false}`, bukan `MarkdownRenderer`. `MarkdownRenderer` (`apps/web/core/components/ui/markdown-to-component.tsx:70`, `react-markdown`) saat ini tidak punya caller di work items.
- Services saat ini: `service-form.tsx:191-205` masih `TextArea` plain + bungkus manual `<p>...</p>` (`:135`); `detail/overview.tsx:48-52` display pakai disabled `TextArea`.
- Tipe: `IService { description: string; description_html: string }` (`packages/types/src/service/core.ts:23-24`). Backend Rust: `apps/api-rs/crates/api/src/routes/service.rs:80-81,166-167,285-286,387-391` sudah terima/simpan keduanya.

## Keputusan (dari brainstorming)

1. **Pendekatan A**: reuse `DescriptionInput` + `RichTextEditor` yang sama dengan work items.
2. **Dua permukaan**: (a) modal `ServiceForm` pakai `RichTextEditor` controlled; (b) `ServiceOverview` pakai `DescriptionInput` autosave (debounce 1.5s) + indikator saving/saved, mengikuti `IssueMainContent.tsx:110-129`.
3. **Upload nonaktif dulu**: `disabledExtensions: ["image", "ai"]`; pass dummy `uploadFile`/`duplicateFile` karena tipe `RichTextEditor` mewajibkannya saat `editable=true`.
4. **Ringkas**: bold/italic/heading/list/quote/code/link/table/mention + markdown shortcut (paritas work items). Mati: `image` (upload) dan `ai` — hanya itu yang didukung `TExtensions` (`packages/editor/src/types/extensions.ts`) dan benar-benar di-gate di `CoreEditorExtensions`. Tabel/mention tidak bisa dimatikan tanpa menyentuh shared package, jadi dibiarkan aktif.

## Perubahan komponen

### 1. `ServiceForm` (`apps/web/core/components/services/service-form.tsx`)

- Ganti `Controller name="description"` (`TextArea`) → `RichTextEditor` controlled mengikuti `IssueDescriptionEditor` (`apps/web/core/components/issues/issue-modal/components/description-editor.tsx:180-245`).
- Props wajib saat `editable`: `workspaceSlug`, `workspaceId` (dari `useWorkspace`), `projectId`, `searchMentionCallback` (tetap pass `workspaceService.searchEntity` karena mention aktif ala work items), `uploadFile`/`duplicateFile` dummy yang throw `Asset upload disabled for services`.
- `disabledExtensions={["image", "ai"]}` (shared constant `SERVICE_DESCRIPTION_DISABLED_EXTENSIONS`).
- Submit: `handleCreateUpdateService` kirim `description_html` dari editor + `description = strip_tags(description_html)` (helper baru, lihat di bawah) agar kolom plain Rust tetap terisi untuk search/list. Hapus bungkus manual `<p>${description}</p>`.
- Validasi: tidak required (boleh kosong → `"<p></p>"`).

### 2. `ServiceOverview` (`apps/web/core/components/services/detail/overview.tsx`)

- Ganti blok `service.description` disabled `TextArea` → `DescriptionInput` mengikuti `IssueMainContent`:
  - `entityId={service.id}`, `initialValue={service.description_html}`, `key={service.id}`, `fileAssetType` — tidak ada `SERVICE_*`, jadi reuse tipe yang paling netral dan tidak dipakai validasi ketat, atau tambah union bila `TEditorAssetType` menolak. Opsi jatuh-bayang: `EFileAssetType.PROJECT_DESCRIPTION` (upload tetap di-disable di UI sehingga tidak terpanggil).
  - `disabledExtensions` sama dengan form.
  - `onSubmit`: `updateService(workspaceSlug, workspaceId, projectId, serviceId, { description_html, description })`.
  - `setIsSubmitting` → status autosave lokal (reuse `NameDescriptionUpdateStatus` bila cocok, atau teks kecil saving/saved).
- Data lama (plain `<p>...</p>`) render apa adanya; tanpa migrasi.

### 3. Helper `strip_tags` FE (baru, kecil)

- Tambah `stripHtmlToText(html: string): string` di `apps/web/core/services/service.helpers.ts` (atau `@plane/utils` bila lebih pas): `DOMParser`/`textarea` trick, trim, collapse whitespace. Dipakai form + overview sebelum kirim.

## Data flow

1. Ketik di editor → Tiptap update `description_json` internal + `description_html`.
2. Modal: `onChange` → `react-hook-form` (`description_html`); submit → `description = stripHtmlToText(html)` → `createService`/`updateService` → MobX `service.store.ts` (optimistic + rollback sudah ada `:189-203`) → Rust API.
3. Overview: `DescriptionInput.onChange` → debounce 1.5s → `handleSubmit` → `updateService` sama seperti di atas.
4. Tidak ada perubahan API/Rust/Django. Tidak ada migrasi data.

## Error handling & edge cases

- Save gagal: toast error dari modal/store yang sudah ada; overview kembalikan nilai optimistik (ikuti pola `service.store.ts` rollback) + status kembali ke `saved` dengan pesan gagal.
- Editor kosong: normalisasi `"<p></p>"` (ikuti `DescriptionInput`).
- HTML asing/tempel: biarkan `tiptap-markdown` (`transformPastedText: true`) + sanitasi backend yang sudah ada yang bekerja.
- Upload dipanggil walau di-disable (mis. drag-drop): dummy handler throw → toast error yang sudah ada di editor.

## Testing

- Unit: `stripHtmlToText` (kosong, `<p>`, bold/list/link, entity `&amp;`).
- Komponen/store (mengikuti pola repo yang ada): submit form mengirim `description_html` + `description` plain; overview autosave memanggil `updateService` sekali setelah debounce; gagal save rollback ke `original`.
- Manual: buat service dengan heading/list/code/quote/link → tersimpan → reload → tampil sama di overview; edit di overview → autosave tanpa reload; paste markdown mentah → terkonversi.

## Non-goals (eksplisit tidak dikerjakan)

- Upload gambar/file services (`EFileAssetType.SERVICE_DESCRIPTION` + validasi backend Django/Rust).
- `@mention`, AI assist, tabel, embed work-item di description services.
- Version history description services (`DescriptionVersionsRoot`).
- Migrasi/backfill data lama ke format editor baru.
