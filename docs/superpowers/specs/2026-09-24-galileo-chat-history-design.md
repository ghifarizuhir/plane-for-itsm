# Galileo Chat History (Multi-Sesi) — Design

Tanggal: 2026-09-24
Status: disetujui user (3/3 bagian), menunggu review spec tertulis.

## Latar

Sidebar chat Galileo (`AIAssistantStore` + `apps/web/core/components/ai/assistant-sidebar/`)
saat ini menyimpan pesan **hanya di `localStorage`** dengan key
`ai_assistant_messages_<workspace_slug>`. Tidak ada satu pun tabel atau endpoint
pesan di server:

- `POST /api/workspaces/:slug/ai-assistant/` dan `POST /api/workspaces/:slug/ai-agent/`
  stateless — body `{task?, prompt}` berisi prompt gabungan yang dibangun FE
  (`buildAiPrompt`, 8 pesan terakhir), dan handler tidak menulis DB.
- Tidak ada konsep percakapan/sesi; id pesan adalah UUID buatan FE yang tidak
  pernah dikirim ke server.
- `localStorage` hilang saat ganti browser/perangkat, dan dihapus saat sign-out
  (`root.store.ts` memanggil `clearPersistedAiConversations`).
- Ganti mode Klasik ↔ Agent mengosongkan chat (`setMode` → `clearConversation`).

Preseden penyimpanan agen sudah ada di fitur Scheduler: `ai_schedule_runs`
menyimpan `prompt`, `response`, `response_html`, `tool_calls jsonb`, status, dan
timestamp — pola yang akan diikuti.

Tujuan fitur: percakapan Galileo tersimpan di server, bisa dibuka lagi lintas
browser/perangkat, dengan daftar sesi seperti ChatGPT.

## Keputusan yang dikunci saat brainstorming

1. Bentuk = **multi-sesi di server**: daftar percakapan lama bisa dibuka lagi,
   lintas browser/perangkat.
2. **Percakapan terikat satu mode** (`classic` atau `agent`). Ganti mode =
   pindah ke percakapan terakhir mode itu, atau kondisi "chat baru" bila belum
   ada. Tidak ada percakapan campuran mode.
3. **Mulai fresh dari server**: history `localStorage` lama tidak dimigrasikan
   dan key lamanya dibersihkan.
4. Daftar history = **panel di dalam sidebar assistant** (bukan dropdown, bukan
   halaman terpisah).
5. Retensi: **maks 50 percakapan** per user per workspace dan **maks 200 pesan**
   per percakapan; konteks agen tetap **8 pesan terakhir**.
6. Judul **otomatis dari pesan pertama user** (dipotong 60 char) dan bisa
   **rename**.
7. **Kartu proposal jadwal disimpan di server** (proposal + `proposal_key` +
   keputusan). Membuka percakapan lama yang masih pending tetap bisa
   dikonfirmasi; `proposal_key` menjamin idempotensi.
8. Arsitektur = **conversation-scoped REST** (pendekatan A): server menyimpan
   kedua sisi pesan dalam satu request dan membangun konteks prompt dari DB.
   `conversation_id` **wajib** di endpoint chat — tidak ada jalur legacy.

## Desain

### 1. Model data — migrasi `apps/api-rs/migrations/0008_ai_conversations.sql`

Mengikuti konvensi 0006: id uuid dibuat aplikasi (tanpa DEFAULT), constraint
dan index diberi nama, `character varying` untuk kolom terbatas, tanpa soft
delete (tidak ada referensi eksternal yang perlu dijaga; hapus = hard delete
cascade).

```sql
CREATE TABLE IF NOT EXISTS public.ai_conversations (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    created_by_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    mode character varying(10) NOT NULL,
    title character varying(120) NOT NULL DEFAULT '',
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT ai_conversations_mode_check CHECK (mode IN ('classic','agent'))
);

CREATE INDEX IF NOT EXISTS ai_conversations_owner_idx
    ON public.ai_conversations (workspace_id, created_by_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS public.ai_messages (
    id uuid NOT NULL,
    conversation_id uuid NOT NULL REFERENCES public.ai_conversations(id) ON DELETE CASCADE,
    role character varying(10) NOT NULL,
    content text NOT NULL,
    content_html text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT ai_messages_role_check CHECK (role IN ('user','assistant'))
);

CREATE INDEX IF NOT EXISTS ai_messages_conversation_idx
    ON public.ai_messages (conversation_id, created_at, id);
```

`metadata` pesan assistant (contoh):

```json
{
  "schedule_proposal": { "name": "...", "frequency": "weekly", "...": "..." },
  "schedule_proposal_key": "<uuid>",
  "schedule_decision": "pending",
  "created_schedule_id": "<uuid?>",
  "is_error": false
}
```

`proposal_key` **digenerate server** saat menulis metadata (bukan FE lagi), lalu
dipakai FE saat Confirm ke `POST /ai-schedules/` (unique parsial
`(workspace_id, proposal_key) WHERE deleted_at IS NULL` sudah ada).

Migrasi diterapkan otomatis saat boot image baru (sqlx migrate ter-embed),
seperti 0006/0007. Tanpa migrasi data (keputusan #3).

### 2. Endpoint API (di bawah `/api/workspaces/:slug/`)

Semua butuh member workspace (guard yang sama dengan endpoint chat); hanya
`created_by_id` yang boleh mengakses percakapannya. Percakapan milik user/workspace
lain → **404**.

| Endpoint                                      | Method | Body / hasil                                                                                    |
| --------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------- |
| `/ai-conversations/`                          | GET    | `{conversations: [{id, title, mode, updated_at}]}` — 50 terbaru, urut `updated_at DESC`         |
| `/ai-conversations/`                          | POST   | `{mode, title?}` → conversation; memangkas percakapan lama (`updated_at ASC`) sampai tersisa 50 |
| `/ai-conversations/:id/`                      | GET    | conversation                                                                                    |
| `/ai-conversations/:id/`                      | PATCH  | `{title}` (trim, 1–120 char) → conversation; `updated_at` diperbarui                            |
| `/ai-conversations/:id/`                      | DELETE | 204; cascade pesan                                                                              |
| `/ai-conversations/:id/messages/`             | GET    | `{messages: [...]}` urut `created_at, id` naik                                                  |
| `/ai-conversations/:id/messages/:message_id/` | PATCH  | `{metadata}` merge allowlist → message                                                          |

PATCH metadata hanya menerima key `schedule_decision` (`created`/`cancelled`)
dan `created_schedule_id` (uuid); key lain → 400. Dipakai FE untuk menandai
keputusan kartu proposal.

### 3. Perubahan endpoint chat + konteks server

`POST /ai-agent/` dan `POST /ai-assistant/` berubah:

- Body: `{task?, prompt, conversation_id}` — `conversation_id` **wajib**.
- Percakapan tidak ada / bukan milik user / workspace beda → 404.
- `conversation.mode` tidak cocok dengan endpoint (`agent` vs `classic`) → 400.
- Alur satu request:
  1. Load percakapan + validasi.
  2. `INSERT` pesan user (`content = prompt`).
  3. Bila percakapan belum punya pesan sebelumnya → isi `title` otomatis:
     satu baris, trim, potong 60 char; `updated_at = now()`.
  4. Load **8 pesan terakhir sebelum pesan user baru** (urut `created_at, id`),
     format `User:` / `Assistant:` memakai `content` (bukan html) — menggantikan
     `buildAiPrompt` di FE, limit tetap 8.
  5. Jalankan agen seperti sekarang (`run_agent` / `chat_completion`).
  6. `INSERT` pesan assistant: `content` = teks jawaban, `content_html` =
     `response_html`, `metadata` = proposal (bila `pending_action.kind ==
"create_schedule"`) + `schedule_proposal_key` (uuid baru) +
     `schedule_decision = "pending"`, atau `is_error = true` bila agen gagal.
  7. Prune pesan >200 per percakapan (hapus `created_at, id` terkecil).
  8. `updated_at` percakapan = `now()`.
- Respons: `{response, response_html, tool_calls, pending_action, conversation,
user_message, assistant_message}` — FE memakai baris server untuk mengganti
  pesan optimistik dan memperbarui judul di daftar.

### 4. FE — service, store, dan panel history

**Service** (`apps/web/core/services/`):

- Baru `ai-conversations.service.ts`: `list`, `create`, `retrieve`, `update`
  (rename), `remove`, `listMessages`, `updateMessageMetadata`.
- `ai.service.ts`: payload chat menambah `conversation_id`; tipe respons
  menambah `conversation`, `user_message`, `assistant_message`.

**Store** (`ai-assistant.store.ts`):

- State baru: `conversations`, `activeConversationId`, `conversationsLoading`,
  `historyOpen`.
- Id aktif per workspace+mode disimpan di localStorage key kecil
  `ai_assistant_active_conversation_<slug>_<mode>`.
- `setWorkspace`: bersihkan key lama `ai_assistant_messages_<slug>`, muat daftar
  percakapan, buka percakapan terakhir mode aktif (atau kondisi chat baru).
- `setMode`: pindah ke percakapan terakhir mode tujuan / chat baru — tidak
  mengosongkan apa pun.
- `newChat`: reset `activeConversationId` + `messages` (percakapan dibuat saat
  pesan pertama dikirim: POST create → POST chat).
- `openConversation`, `renameConversation`, `deleteConversation`,
  `sendMessage` (optimistic user bubble → ganti dengan `user_message` +
  `assistant_message` dari server), `confirmScheduleProposal` /
  `resolveScheduleProposal` menambah PATCH metadata keputusan.
- `clearPersistedAiConversations` (sign-out) tetap menghapus key lokal tapi
  **tidak** menghapus history server.

**UI** (`assistant-sidebar/`):

- Header: ikon history + tombol New chat, di samping toggle mode yang ada.
- Komponen baru `conversation-history-panel.tsx`: daftar item (judul, badge
  mode, waktu relatif, item aktif ditandai), aksi rename (inline/dialog) dan
  hapus (dialog konfirmasi), empty state, loading skeleton.
- `schedule-proposal-card.tsx` tidak berubah tampilannya; sumber data kini
  metadata pesan server. Pending tetap bisa dikonfirmasi dari history.
- Mapping server message → `TAiMessage`: `metadata.is_error` → `isError`,
  `schedule_proposal` → `scheduleProposal`, `schedule_decision` →
  `scheduleDecision`, `created_schedule_id` → `createdScheduleId`.

### 5. Error handling & guardrail

- Otorisasi: member workspace + owner-only; 404 lintas user/workspace.
- Mode mismatch chat ↔ percakapan → 400.
- Rename: trim, 1–120 char. Auto-title: satu baris, potong 60 char.
- Retensi dalam transaksi yang sama: create → prune percakapan >50; insert pesan
  → prune pesan >200.
- Agen gagal → baris assistant `is_error` tetap tersimpan supaya history jujur;
  percakapan kosong (chat gagal total) tetap ada dengan judul "New conversation",
  bisa rename/hapus, ikut terhitung cap.
- Konfirmasi proposal idempoten via unique parsial `proposal_key`; replay
  mengembalikan jadwal yang sama.
- Dua tab pada percakapan sama: pesan interleave urut insert, rename
  last-write-wins; tanpa locking. Percakapan dihapus di tab lain → 404 → FE
  reset ke chat baru + toast.
- `content_html` disimpan apa adanya (konsisten `ai_schedule_runs`) dan tetap
  disanitasi saat render oleh `sanitizeAssistantHtml`; metadata hanya ditulis
  server + PATCH allowlist.

## Testing dan verifikasi

**Rust integration** (`crates/api/tests/ai_conversations_test.rs`, pola
`ai_schedule_test`: DB nyata + fake upstream axum):

- CRUD + ownership: create/list/rename/delete; user lain & workspace lain → 404.
- List urut `updated_at DESC`; create ke-51 memangkas yang tertua.
- Prune 200 pesan menyisakan 200 terbaru.
- Chat sukses (fake upstream): satu request menulis user + assistant, auto-title
  dari pesan pertama, respons berisi `conversation`/`user_message`/`assistant_message`.
- Konteks: assert prompt yang diterima fake upstream = 8 pesan terakhir.
- Jalur gagal: baris assistant `is_error` tersimpan.
- PATCH metadata: key di luar allowlist ditolak; `schedule_decision` tersimpan.
- Round-trip proposal: chat → metadata proposal + key → Confirm via
  `POST /ai-schedules/` → replay tidak menggandakan.
- Mode mismatch → 400; `conversation_id` hilang → 400.

**FE** (`apps/web/core/store/ai-assistant.store.test.ts` dkk):

- Load daftar + buka percakapan terakhir per mode; ganti mode tidak menghapus
  pesan; new chat; optimistic replace; pembersihan key lama saat load;
  reset saat 404.
- Komponen panel: render item + badge, dialog rename/hapus memanggil service.

**Smoke manual (tunnel):** buat percakapan → kirim pesan → reload (masih ada) →
rename → hapus; konfirmasi proposal dari percakapan lama; history mode Classic;
sign-out tidak menghapus history server; dua tab.

Verifikasi standar repo: `cargo test -p api -- --test-threads=1`, `pnpm --filter=web test`,
`check:types`, `check:format`, `check:lint`, build web, lalu rebuild image API +
`up -d api worker beat-worker` (migrasi 0008 tercatat) dan rebuild web prod.

## Out of scope (eksplisit bukan bagian desain ini)

- Streaming jawaban & realtime (tanpa websocket/polling pesan).
- Berbagi percakapan ke member lain; akses admin ke history user.
- Pencarian pesan, filter lanjutan, pin/arsip.
- Judul digenerate LLM.
- Impor history `localStorage` lama.
- Menampilkan `tool_calls` di UI.
- Edit/hapus pesan individual atau regenerate jawaban.

## Risiko dan mitigasi

- **Latensi pesan pertama** (create percakapan + chat = 2 request) — kecil dan
  hanya sekali per sesi; percakapan dibuat sebelum chat agar id tersedia untuk
  panel.
- **Prune pesan bisa memotong pasangan user/assistant** — konteks tetap valid
  karena server memakai 8 pesan terakhir apa adanya; dampak kosmetik di history
  lama, diterima.
- **Token prompt membengkak** karena pesan panjang — limit 8 pesan sama dengan
  perilaku sekarang; belum ada pemotongan per pesan (sama seperti `buildAiPrompt`).
- **Dua tab menulis bersamaan** — interleave urut insert; tidak ada korupsi
  data; rename last-write-wins.
- **Percakapan kosong menumpuk** bila chat gagal setelah create — ikut terhitung
  cap 50 dan bisa dihapus user; alternatif auto-delete ditolak karena
  menyembunyikan kegagalan.
- **Hapus percakapan tidak menghapus jadwal** yang pernah dibuat darinya —
  disengaja (jadwal berdiri sendiri; `proposal_key` menjaga idempotensi).
- **Kontrak chat berubah** (`conversation_id` wajib) — `smoke.sh` dan contoh
  dokumen diupdate; tidak ada klien lain yang memakai endpoint ini.
- **`updated_at` percakapan** diperbarui tiap giliran; list 50 memakai
  `updated_at DESC` sehingga percakapan aktif selalu di atas.
