# AI Agent: Buat Work Item dari Chat (Proposal + Confirm) — Design

Tanggal: 2026-09-28
Status: disetujui user saat brainstorming (Approach 1), menunggu review spec tertulis.

## Latar

Agent chat (Galileo, mode `agent`) saat ini punya tiga tool read-only
(`list_projects`, `count_work_items`, `search_work_items`) plus
`create_schedule` yang hanya mempropose resep jadwal. Satu-satunya tulisan
nyata terjadi setelah user menekan Confirm di card, lewat endpoint schedule
yang sudah ada (`POST /api/workspaces/:slug/ai-schedules/`).

Belum ada cara membuat work item dari chat. Tujuan slice ini: user bisa
meminta agent membuat work item (bahasa natural atau prefix `/task`), agent
menyusun proposal terstruktur yang bisa diedit di chat, dan setelah user
Confirm work item benar-benar dibuat lewat endpoint create issue yang ada.

## Keputusan yang dikunci saat brainstorming

1. Semantik: **propose → confirm → langsung created**. Bukan draft
   (`is_draft`), bukan buka modal create prefilled. Mengikuti pola
   `create_schedule`.
2. **Satu card = satu work item.** Agent boleh mempropose beberapa card dalam
   satu turn (mis. breakdown epic jadi 3 task); tiap card confirm terpisah.
3. Field yang bisa diisi agent — semua opsional kecuali `name`: `name`,
   `description`, `priority`, `state`, `assignee(s)`, `labels`,
   `start_date`/`target_date`. Setara create API yang ada
   (`issue_write.rs::CreateIssue`).
4. Project **wajib disebut**; kalau ambigu agent bertanya, tidak menebak.
   Hanya project yang user punya akses write yang bisa dipakai (ditegakkan
   endpoint create yang ada). Card tetap menyediakan dropdown project sebagai
   jalan keluar bila nilai yang diusulkan tidak ketemu.
5. Scheduled run **tetap read-only** (3 tool). `create_work_item` tidak
   pernah masuk `read_tools`; tidak ada auto-create dari background.
6. Trigger: bahasa natural yang jelas **plus** prefix `/task` untuk eksplisit.
7. Resolusi nama manusiawi (project/state/assignee/label) → UUID dilakukan
   **di FE** memakai store yang sudah ada; create memakai
   `POST /api/workspaces/:slug/projects/:projectId/issues/` yang sudah ada.
   Tidak ada endpoint AI khusus, tidak ada duplikasi logika validasi.
8. Idempotency: satu `key` per proposal + guard `decision != pending` +
   tombol Confirm disabled saat in-flight. Tidak ada mekanisme idempotency
   level DB (dinilai cukup; user bisa menghapus duplikat kalau terjadi).
9. Tanpa migration: proposal & decision disimpan di metadata pesan
   percakapan (jsonb). Tanpa perubahan worker/Django/endpoint schedule.

## Desain

### 1. Alur end-to-end

1. User chat: "buatkan task ganti filter pompa, priority urgent, project LTS"
   atau `/task ...`.
2. Agent boleh memanggil `list_projects` dulu untuk verifikasi project, lalu
   memanggil `create_work_item` — **proposal-only, tidak menyentuh DB**.
3. Response turn sukses membawa:
   - metadata assistant message `work_item_proposals: [{key, proposal}]`
     (key UUID di-generate server, satu per proposal; urut sesuai trace);
   - field body baru `pending_actions`: array semua proposal turn itu
     (`create_schedule` maupun `create_work_item`, urut trace). Field lama
     `pending_action` (proposal `create_schedule` terakhir) dipertahankan apa
     adanya untuk kompatibilitas.
4. FE merender satu `WorkItemProposalCard` per proposal di pesan assistant.
   Semua field bisa diedit; nilai hasil resolusi ditampilkan.
5. Confirm → FE resolve nama → UUID, memanggil service create issue yang ada.
   Sukses → decision `created` + id work item + project id disimpan ke
   metadata `work_item_decisions: {"<key>": {...}}` (PATCH pesan). Card
   berubah jadi link ke work item.
6. Cancel → decision `cancelled` disimpan dengan cara yang sama.
7. Reload halaman: proposal dibaca dari `work_item_proposals`, decision dari
   `work_item_decisions`; card dirender sesuai kondisi terakhir.

### 2. Tool `create_work_item` (crate `ai`)

Args (schemars `JsonSchema`, deskripsi per field untuk model):

| Field         | Tipe                  | Aturan                                                                 |
| ------------- | --------------------- | ---------------------------------------------------------------------- |
| `project`     | `String`              | wajib; identifier (mis. `LTS`) atau nama project; trim, ≤100           |
| `name`        | `String`              | wajib; trim, 1–255                                                     |
| `description` | `Option<String>`      | trim, ≤5000; teks/markdown, bukan HTML                                 |
| `priority`    | `Option<String>`      | allowlist `PRIORITIES` (urgent/high/medium/low/none), case-insensitive |
| `state`       | `Option<String>`      | nama state (mis. `In Progress`); trim, ≤100                            |
| `assignees`   | `Option<Vec<String>>` | ≤10; tiap entri trim non-empty ≤100; dedupe case-insensitive           |
| `labels`      | `Option<Vec<String>>` | ≤10; aturan sama                                                       |
| `start_date`  | `Option<String>`      | `YYYY-MM-DD` (`chrono::NaiveDate` parse)                               |
| `target_date` | `Option<String>`      | `YYYY-MM-DD`; bila keduanya ada, `start_date <= target_date`           |

- Validasi bentuk murni di Rust (tanpa DB). Args invalid →
  `ToolExecutionError::invalid_args`, **tidak** dicatat ke trace (sama seperti
  `create_schedule`).
- Args valid → dinormalisasi (trim, lowercase priority) lalu dicatat ke trace
  sebagai satu `ToolCallTrace` bernama `create_work_item`; output tool =
  proposal JSON (supaya model tahu card siap).
- `workspace_tools` += `CreateWorkItem`; `read_tools` **tidak berubah**
  (hanya 3 read tool) sehingga scheduled run tidak bisa membuat work item.
- `agent.rs`:
  - `PREAMBLE` diperbarui: jelaskan kapan mempropose (permintaan jelas atau
    `/task`), satu item per call, project wajib disebut (kalau ambigu tanya),
    nilai state/assignee/label boleh nama manusiawi, dan jangan pernah bilang
    work item sudah dibuat sebelum user Confirm.
  - Helper baru `pending_actions(trace) -> Vec<Value>`: semua panggilan
    `create_schedule` + `create_work_item` berurutan, bentuk
    `{"kind": "...", "proposal": {...}}`. `pending_action` lama tetap
    `Option<Value>` berisi `create_schedule` terakhir.
  - `MAX_TURNS` tidak berubah (6); pembatasan jumlah proposal per turn
    mengikuti budget turn yang ada (tanpa cap khusus di luar validasi per
    proposal).

### 3. API handler (crate `api`, `routes/ai_agent/mod.rs`)

- Setelah agent sukses: susun metadata assistant message seperti sekarang,
  tambah:
  - `work_item_proposals`: array `{"key": "<uuid>", "proposal": {...}}` untuk
    tiap trace `create_work_item` (key di-generate di sini);
  - `pending_actions` di body sukses (semua proposal turn, termasuk
    `create_schedule`; entri work item menyertakan `key`).
- `success_body`/`chat_success_body` diperluas dengan parameter
  `pending_actions`; test shape diperbarui.
- Metadata schedule yang ada (`schedule_proposal`, `schedule_proposal_key`,
  `schedule_decision`) tidak berubah.

### 4. PATCH metadata (`routes/ai_conversations.rs`)

- Allowlist key baru: `work_item_decisions`.
  - Harus object; tiap key harus UUID; tiap value object dengan:
    - `decision` wajib, salah satu `created` / `cancelled`;
    - `created_work_item_id` wajib UUID bila `decision=created` (dilarang
      bila `cancelled`);
    - `created_project_id` wajib UUID bila `decision=created` (dibutuhkan
      untuk membangun link setelah reload, termasuk saat user mengganti
      project di card).
  - Key/value di luar bentuk itu → 400 (pesan konsisten dengan validasi
    `schedule_decision` yang ada).
- Merge tetap `metadata || $patch` (replace per top-level key). FE mengirim
  **seluruh map** `work_item_decisions` agar tidak menimpa decision card lain
  di pesan yang sama.

### 5. Frontend

File baru `apps/web/core/lib/ai-work-items.ts`:

- `TAiWorkItemProposal` (project, name, description?, priority?, state?,
  assignees?, labels?, start_date?, target_date?) dan
  `TAiWorkItemProposalEntry` (`{key, proposal}`).
- `TAiWorkItemDecision` (`{decision, created_work_item_id?,
created_project_id?}`).
- `WORK_ITEM_PRIORITIES`, `validateWorkItemProposal` (nama wajib, panjang,
  tanggal, start ≤ target), `workItemHref(workspaceSlug, projectId, issueId)`
  → `/{slug}/projects/{projectId}/issues/{issueId}`.

`ai-context.ts` / `ai-conversations.ts`:

- `TAiMessage` += `workItemProposals?: TAiWorkItemProposalEntry[]`,
  `workItemDecisions?: Record<string, TAiWorkItemDecision>`.
- `TAiMessageMetadata` += `work_item_proposals?`, `work_item_decisions?`;
  `toAiMessage` memetakannya; `TAiMessageMetadataPatch` +=
  `work_item_decisions`.

`store/ai-assistant.store.ts`:

- `confirmWorkItemProposal(messageId, key, payload)`:
  - guard `slug` + decision masih `pending`;
  - panggil `issueService.createIssue(slug, projectId, payload)` (service
    yang ada, tanpa perubahan);
  - sukses → set decision `created` + `created_work_item_id` +
    `created_project_id`, lalu persist seluruh map `work_item_decisions`;
  - gagal → lempar error (card menampilkan pesan, decision tetap `pending`).
- `resolveWorkItemProposal(messageId, key, "cancelled")` → set + persist map.

Komponen baru
`apps/web/core/components/ai/assistant-sidebar/work-item-proposal-card.tsx`:

- Mirror `ScheduleProposalCard` (state `draft`, `submitting`, `error`,
  decision `created`/`cancelled`/`pending`).
- Field editable: project (select dari project workspace), name (input),
  description (textarea), priority (select), state (select dari state
  project), assignees (multi-select member project), labels (multi-select
  label project), start/target date (date input).
- Resolusi nilai usulan agent (case-insensitive, dijalankan saat card
  dirender / project berubah):
  - project: `useProject()` — identifier exact, lalu nama exact, lalu nama
    substring unik; kalau gagal → project kosong + pesan, Confirm disabled
    sampai user memilih.
  - state: `useProjectState().fetchProjectStates(slug, projectId)` (bila
    belum termuat) + match nama; gagal/kosong → dibiarkan kosong (API memakai
    default state project via `resolve_issue_state`).
  - assignees: `useMember().fetchProjectMembers` + match `display_name` atau
    `email`; label: `useLabel().fetchProjectLabels` + match nama. Yang tidak
    ketemu di-drop dari pilihan + pesan kecil "not found — pick manually".
- Confirm membangun payload `CreateIssue` dan memanggil `onConfirm`; Cancel
  memanggil `onCancel`. Konversi deskripsi teks → HTML (helper murni di
  `ai-work-items.ts`, teruji): escape `& < > "`, pecah per baris `\n`, gabung
  dengan `<br/>`, bungkus `<p>…</p>`; deskripsi kosong → `null`.
- Setelah `created`: tampil "Work item created." + link
  `workItemHref(slug, created_project_id, created_work_item_id)`.
  `created_project_id` selalu ada karena diwajibkan saat decision `created`
  (tidak ada data lama: fitur ini baru).
- Setelah `cancelled`: teks status "Work item cancelled."

`root.tsx`:

- Render `message.workItemProposals?.map((entry) => <WorkItemProposalCard
key={entry.key} ... />)` di bawah card schedule.
- Hint slash diperluas: `/schedule` **dan** `/task` (dua baris hint; klik
  mengisi composer).

### 6. Error handling & edge cases

- **Project tidak resolve** → card menampilkan error, Confirm disabled,
  user pilih project manual (dropdown).
- **State/assignee/label tidak resolve** → field dikosongkan/di-drop dengan
  catatan; user memilih manual; tidak memblokir Confirm.
- **API create gagal** (permission project, validasi, network) → pesan error
  di card; decision tetap `pending` sehingga bisa retry; tidak ada decision
  yang ter-persist.
- **Double-confirm** → tombol disabled saat in-flight + guard decision.
- **Beberapa card per pesan** → decision map per key; PATCH mengirim seluruh
  map.
- **Pesan lama** (tanpa `work_item_proposals`) → tidak ada perubahan
  rendering.
- **Scheduled run** → tidak berubah, tetap 3 read tool.

### 7. Testing

- Rust unit (`crates/ai/src/tools.rs`): validasi args (name wajib/trim,
  priority allowlist, tanggal & urutan, limit assignee/label, dedupe),
  normalisasi, trace dicatat hanya saat valid, `read_tools` tidak memuat
  `create_work_item`, `workspace_tools` memuatnya.
- Rust unit (`crates/ai/src/agent.rs`): `pending_actions` urutan & bentuk,
  `pending_action` lama tetap.
- Rust (`crates/api`): shape `success_body`/`chat_success_body` dengan
  `pending_actions`; metadata `work_item_proposals` berisi key; validasi
  PATCH `work_item_decisions` (valid, UUID salah, decision salah, id wajib
  saat created, key tidak dikenal → 400).
- Integrasi `crates/api/tests/ai_agent_test.rs` (fake upstream): turn dengan
  tool call `create_work_item` → response memuat proposal + metadata; PATCH
  decision roundtrip.
- FE vitest: `validateWorkItemProposal`, `toAiMessage` mapping, store
  `confirmWorkItemProposal`/`resolveWorkItemProposal` dengan service mock
  (sukses, gagal, guard), `workItemHref`.
- Smoke manual: chat `/task`, edit field, confirm, buka link; cancel; reload
  halaman; cek scheduled run tidak bisa create.

### 8. Out of scope

- Update/delete/assign work item lewat agent setelah dibuat.
- Satu card berisi banyak item (bulk).
- Auto-create work item dari scheduled run.
- Tool baca member/label/state untuk agent (FE yang resolve).
- Idempotency level DB untuk create issue.
- Perubahan pada alur/card schedule, worker, atau Django.
