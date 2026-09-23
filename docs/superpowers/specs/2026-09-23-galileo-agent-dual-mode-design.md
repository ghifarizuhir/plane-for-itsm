# Galileo Dual-Mode (Klasik ↔ Agent) — Design

Tanggal: 2026-09-23
Status: disetujui user (4/4 bagian), menunggu review spec tertulis.

## Latar

Sidebar chat Galileo (`AiAssistantSidebar`) hari ini POST `{task, prompt}` ke
`POST /api/workspaces/:slug/ai-assistant/` (single-shot; history 8 pesan ditempel
di prompt via `buildAiPrompt`) lalu merender `res.response_html`. Jawaban hanya
berbasis teks konteks issue.

Prototipe agent Rig (`POST /api/workspaces/:slug/ai-agent/`, spec
`2026-09-23-rust-rig-tool-agent-design.md`) menjawab berbasis data DB via tools
(`list_projects`, `count_work_items`, `search_work_items`). Tujuan: sidebar
Galileo bisa beralih antara mode Klasik dan mode Agent lewat tombol di header,
dengan jalur klasik sebagai fallback manual.

## Keputusan yang dikunci saat brainstorming

1. Ganti mode = chat baru (toggle otomatis `clearConversation`, tanpa dialog konfirmasi).
2. Tombol toggle di header sidebar, di samping judul "Galileo".
3. `tool_calls` dari backend agent disembunyikan di UI (trace cukup di log server).
4. Fallback manual saja: bila `/ai-agent/` error, tampilkan error + Retry di mode
   aktif; user pindah mode sendiri via tombol header.
5. Pendekatan A: backend-lean swap (ubahan backend aditif-opsional + cabang kecil di FE).

## Desain

### 1. Backend — aditif saja (`apps/api-rs`)

- Request `POST /api/workspaces/:slug/ai-agent/` tambah field opsional
  `task: Option<String>`. Bila ada, prompt efektif yang dikirim ke model adalah
  `task + "\n" + prompt` — persis pola Django `GptAssistantView._get_response`.
  Preamble agent tetap didepan prompt efektif; tools dan `max_turns` tidak berubah.
- Response tambah field `response_html`, diturunkan dari `response` dengan
  pemetaan `\n`→`<br/>` yang persis sama dengan `/ai-assistant/`
  (`routes/ai.rs`), diekstrak jadi helper bersama — reuse, bukan duplikasi
  logika, dan tanpa escaping baru (perilaku render harus identik dengan mode
  klasik).
- Tidak ada perubahan pada: gate ADMIN/MEMBER, rate-limit 429 per host+workspace,
  pemetaan error 400/429/500, maupun kontrak `/ai-assistant/`.
- Kompatibel mundur: kedua field baru opsional/derivatif; klien lama yang hanya
  kirim `{prompt}` tetap berfungsi.
- Test: perluas `crates/api/tests/ai_agent_test.rs` — (a) `task` terlipat ke
  prompt yang diterima upstream fake; (b) `response_html` terisi dengan pemetaan
  newline yang benar. Tanpa entri `parity-inventory.json` baru (route sudah ada;
  perluasan aditif tidak mengubah status parity).

### 2. FE service + store

- `apps/web/core/services/ai.service.ts`: method baru
  `createAgentTask(workspaceSlug, {task, prompt})` → POST
  `/api/workspaces/${slug}/ai-agent/`, mengembalikan `response?.data`
  (bentuk `{response, response_html?, tool_calls}`).
- `apps/web/core/store/ai-assistant.store.ts`:
  - State baru `mode: "classic" | "agent"` (observable), persist per workspace di
    `localStorage` dengan kunci `ai_assistant_mode_<slug>`; default `"classic"`.
  - `setWorkspace` me-restore mode bersama messages; `setMode(mode)` action yang
    memanggil `clearConversation` (keputusan #1).
  - `request()` branch per mode: klasik → `createGptTask` (tidak berubah); agent →
    `createAgentTask`, konten balon = `res.response_html ?? res.response ?? ""`
    (HTML diutamakan agar jalur render sama dengan mode klasik).
  - Kedua mode mengirim payload `{task: AI_ASSISTANT_TASK, prompt: buildAiPrompt(...)}`
    yang identik — prompt sama, perbedaan perilaku (tools + preamble) murni di server.
  - `TAiService` Pick + interface `IAIAssistantStore` + hook `use-ai-assistant`
    diekspos untuk `mode`/`setMode`.
- History: tetap 8 pesan terakhir yang ditempel FE di prompt; tanpa memory
  server-side (tetap single-shot per request; multi-turn agent Rig hanya untuk
  loop tool internal, bukan antar pesan chat).

### 3. UI toggle

- `apps/web/core/components/ai/assistant-sidebar/root.tsx`: segmented control
  berlabel `Classic | Agent` (Inggris, mengikuti copy UI yang ada: "New
  conversation", "Ask AI anything…") di header, di samping judul "Galileo"
  sebelum tombol New conversation. Disabled saat `isGenerating`. Tooltip
  menjelaskan perbedaan: Agent menjawab berbasis data workspace (tools),
  Klasik seperti sekarang.
- Selected state segmented control adalah indikator mode; tidak ada label mode
  tambahan. Copy empty-state, suggestions, dan strip konteks tidak berubah.
- Pesan agent dirender seperti pesan klasik (`dangerouslySetInnerHTML` dari
  `response_html ?? response`); `tool_calls` diabaikan.

### 4. Error handling, data flow, verifikasi

- Data flow mode agent: composer → `sendMessage` → `buildAiPrompt` (konteks +
  history + pertanyaan) → `createAgentTask {task, prompt}` → `/ai-agent/` →
  preamble + `task\nprompt` + tool loop (maks 6 turn) → `{response,
response_html, tool_calls}` → balon chat (tool_calls dibuang di klien).
- Error mapping store tidak berubah: 429 → pesan rate-limit dari server;
  400 → "AI is not configured"; selain itu → internal error. Retry mengulang di
  mode aktif. Fallback = toggle manual ke Klasik (chat baru) — didokumentasikan
  di sini, tanpa UI tambahan.
- Verifikasi: `cargo test -p api` serial (1141+ test yang ada + test baru);
  `pnpm check` untuk paket web yang tersentuh; rebuild `pnpm --filter=web build`
  - restart `plane-web-prod.service` sesuai AGENTS.md; smoke UI kedua mode
    (kirim pesan, toggle, retry) termasuk kasus negatif.
- Rollout aman: default mode Klasik, endpoint agent tidak menerima trafik sampai
  user menekan toggle.

## Out of scope (eksplisit bukan bagian desain ini)

- Jalur GPT popover / editor (`rephrase-grammar`, generate description) tetap ke
  `/ai-assistant/`; tidak diubah.
- Tanpa history/pace server-side antar pesan chat; tanpa streaming/SSE.
- Tanpa UI untuk `tool_calls`; tanpa analitik pemakaian per mode.
- Tanpa perubahan parity Django (endpoint agent tetap Rust-only sesuai spec
  prototipe; parity-inventory tidak disentuh).

## Risiko dan mitigasi

- Preamble agent + teks `AI_ASSISTANT_TASK` ganda memberi instruksi redundan →
  diterima (teks task untuk mode chat kompatibel: fokus work item, tanpa aturan
  "hanya HTML" seperti task editor); bila kualitas jawaban menurun, opsi lanjutan
  adalah preamble khusus mode chat (di luar scope ini).
- Toggle tak sengaja menghapus riwayat → diterima sesuai keputusan #1 (tanpa
  konfirmasi, YAGNI); persist per workspace tetap; tombol New conversation sudah
  punya perilaku hapus yang sama.
