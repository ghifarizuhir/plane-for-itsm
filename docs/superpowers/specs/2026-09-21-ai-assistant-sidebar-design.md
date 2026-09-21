# AI Assistant Sidebar (kanan) — Design (2026-09-21)

## Tujuan

Setelah AI backend hidup di Rust (`POST /api/workspaces/:slug/ai-assistant/`,
spec `2026-09-21-rust-ai-assistant-design.md`), kemampuan AI di FE masih
terbatas pada dua tombol di description editor issue modal. Slice ini
menambahkan **panel AI assistant di sisi kanan aplikasi** yang bisa dibuka
dari halaman mana saja (workspace-wide), berfungsi sebagai asisten kontekstual
issue.

## Keputusan brainstorming

1. **Peran:** asisten kontekstual issue — menjawab Q&A dan menghasilkan teks
   siap pakai (deskripsi, komentar, draft) berdasarkan konteks issue aktif.
   Tidak ada aksi otomatis (buat issue, ubah field) di iterasi ini.
2. **Penempatan (opsi B):** panel kanan berdiri sendiri dengan toggle global.
   Bukan tab di peek view (opsi A) dan bukan hibrida (opsi C).
3. **Konteks:** otomatis dari issue aktif (peek terakhir / detail page), tanpa
   override manual.
4. **Ketersediaan:** workspace-wide — semua halaman authenticated.
5. **Pendekatan A:** reuse endpoint `ai-assistant/` yang ada (tanpa pekerjaan
   backend), dengan seam bersih di FE agar swap ke endpoint chat
   (`ai-chat/`, pendekatan B) di iterasi berikutnya hanya mengubah satu file.
6. Backend tidak disentuh sama sekali di slice ini.

## Arsitektur & State

- **ThemeStore** (`apps/web/core/store/theme.store.ts`): tambah
  `aiSidebarCollapsed` + `toggleAiSidebar`, persist ke localStorage — pola yang
  sama dengan `profileSidebarCollapsed` dan saudaranya.
- **AIAssistantStore** (baru, `apps/web/core/store/ai-assistant.store.ts`):
  observable `messages[]` (`role: "user" | "assistant"`), `isGenerating`,
  `activeIssueContext`, `sendMessage()`, `clearConversation()`, restore/save
  localStorage per workspace.
- **Hook akses:** `useAiAssistant()` mengikuti konvensi
  `apps/web/core/hooks/store/*`.
- **Panel** (baru, `apps/web/core/components/ai/assistant-sidebar/`):
  di-mount di `apps/web/core/components/workspace/content-wrapper.tsx`
  sehingga berlaku workspace-wide (termasuk settings).
  - Fixed overlay kanan: `right-0`, dari bawah top nav sampai bawah, lebar
    `~24rem`, `z-[30]`.
  - Entry point: tombol sparkle di top nav kanan (`TopNavigationRoot`),
    hanya tampil jika `has_llm_configured`.
- **Seam service:** `AIService` tidak diubah. Komposisi prompt di
  `apps/web/core/lib/ai-context.ts` — fungsi murni `buildAiPrompt(context,
  history, question)`, dipanggil store; swap endpoint berikutnya mengubah
  satu tempat.

## Sumber konteks (prioritas)

1. Issue yang sedang di-peek → `useIssueDetail().peekIssue` (judul, deskripsi,
   state, prioritas, assignee).
2. Issue detail page terbuka (route param `projectId` + `issueId`) → issue dari
   store/service yang sudah ada.
3. Tidak keduanya → empty state panel: "No active issue — your question will
   be answered without issue context"; chat tetap berfungsi.

Konteks dibaca saat `sendMessage` dipanggil dan hanya di-inject ke prompt —
tidak ditampilkan sebagai pesan.

## Komposisi prompt

- `task` (tetap): instruksi asisten ITSM — "You are an ITSM work-item
  assistant. Answer using the issue context below. Be concise, use bullet
  points. If generating text (description/comment), output the text only."
- `prompt`: blok konteks issue (judul + deskripsi strip-HTML + state +
  prioritas) + riwayat chat (dipotong, ~8 pesan terakhir) + pertanyaan
  terakhir.

## UX panel

- Bubble user rata kanan (plain text), AI rata kiri (render `response_html`,
  paritas dengan `gpt-assistant-popover.tsx:152`).
- Loading "generating…" selama request; tanpa streaming (endpoint single-shot).
- Header: judul "AI Assistant" + indikator konteks (nama issue aktif) + tombol
  clear conversation + close.
- Input: textarea 1–3 baris, Enter kirim, Shift+Enter newline.

## Error handling

Semua error tampil sebagai bubble/chat-state, bukan toast:

| Kondisi            | Tampilan                                            |
| ------------------ | --------------------------------------------------- |
| 400 (unconfigured) | bubble "AI belum dikonfigurasi"                     |
| 429                | bubble pesan server (`Rate limit exceeded for …`)   |
| 500 / network      | bubble "An internal error has occurred." + retry    |
| Request gagal      | pesan user tetap tampil, ditandai gagal             |

Gating: tombol sparkle + panel hanya dirender jika `config?.has_llm_configured`
(paritas dengan `description-editor.tsx`).

## Testing

- Unit `buildAiPrompt`: komposisi, pemotongan riwayat, strip HTML, kasus tanpa
  konteks.
- Unit `AIAssistantStore`: `sendMessage` (service dimock), path error
  400/429/500 → bubble benar, persist/restore localStorage, clear.
- Component test panel: render bubble, empty state, gating.

## Konvensi yang diikuti

- UI string hardcoded Inggris (paritas dengan `gpt-assistant-popover.tsx`,
  tanpa i18n).
- MobX `makeObservable`; styling via `@plane/ui`, kelas panel mengikuti
  `extended-sidebar-wrapper.tsx`.

## Trade-off yang disepakati

1. **Overlay, bukan layout-push** — panel menutupi tepi kanan peek view
   (`z-25`) saat keduanya terbuka; aman karena tidak menyentuh layout lain.
   Kalau terasa mengganggu, iterasi berikutnya bisa jadikan sibling flex.
2. **Tanpa streaming** — batasan endpoint single-shot.
3. **Riwayat client-side only** — localStorage per workspace, tidak tersimpan
   server; upgrade ke endpoint chat (pendekatan B) memindahkan penyimpanan ke
   server nanti.
4. **Tanpa rate-limit user-level** — paritas dengan Django OSS.
