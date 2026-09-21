# Legacy Issue Create — Full Parity (bridges, default assignee, activities, response) — Design (2026-09-21)

## Tujuan

Legacy create `POST /api/workspaces/:slug/projects/:id/issues/`
(`routes::issue_write::create`, `issue_write.rs:204-286`) sudah punya gate authz
dan alokasi sequence yang benar, tetapi belum menulis data yang dikirim web:

1. `assignee_ids` / `label_ids` hanya divalidasi lalu dibuang — tidak ada baris
   `issue_assignees` / `issue_labels` (temuan #1 audit).
2. Fallback default assignee project tidak diterapkan (Django `role >= 15`).
3. Tidak ada baris `issue_activities` (created + assignee) maupun
   `issue_subscribers` untuk assignee.
4. Response hanya `{"id","name"}` (`IssueOut`, `issue_common.rs:8-12`),
   sedangkan web menyuntikkan response create apa adanya ke store: identifier
   `${projectIdentifier}-${sequence_id}` (`issue.store.ts:61-75`) dan grouping
   list butuh 26 key (Django `.values()`, `views/issue/base.py:440-467`).
5. Field lain yang dikirim web dalam POST yang sama — `description_html`,
   `priority`, `start_date`, `target_date`, `parent_id`, `type_id`,
   `estimate_point` — di-drop karena `CreateIssue` hanya punya 4 field
   (`issue_write.rs:61-70`). Django menanganinya lewat `fields = "__all__"`
   (`serializers/issue.py:82-114`). Tidak ada follow-up PATCH untuk field-field
   ini di web (`base.tsx:160-247`).

Slice ini menutup kelimanya sekaligus untuk endpoint legacy (dipakai web).
Referensi Django: `IssueCreateSerializer.create` (`serializers/issue.py:199-274`)
dan view create (`views/issue/base.py:404-490`).

## Keputusan brainstorming

1. **Scope "full"** — sekaligus semua field yang dikirim web (description,
   priority, dates, parent, type, estimate), bukan hanya bridges + response.
2. **Validasi ketat (bukan silent filter)** — assignee/label/state/type/parent/
   estimate invalid dan error validasi lain → **400**
   `{"error": "..."}`. Ini deviasi sadar dari Django web yang membuang ID
   assignee/label invalid diam-diam (`serializers/issue.py:149-166`); pilihan
   ini mempertahankan semangat validasi #9526 dan konsisten dengan v1
   (`v1/work_item.rs:575-627`).
3. **Activities parity Django** — tulis baris `created`, activities assignee
   yang **diminta** (`field="assignees"`), subscriber rows; **tanpa** activity
   label; default assignee tidak dicatat. Notifikasi tidak dibangun (infra
   belum ada).
4. **Response 26-key** — shape sama dengan list `IssueListRow`
   (`issue_common.rs:20-48`), bukan issue detail v1 yang punya alias
   `assignees`/`labels`.
5. **Pendekatan 1** — perluas kode yang ada + helper bersama di
   `issue_common.rs`; activities/subscribers di modul baru
   `issue_activity_write.rs`.

## Kontrak API

**Request** (`CreateIssue`, `issue_write.rs:61-70`, diperluas):

| Field              | Tipe                      | Default bila absen | Catatan                                                                           |
| ------------------ | ------------------------- | ------------------ | --------------------------------------------------------------------------------- |
| `name`             | `String`                  | wajib              |                                                                                   |
| `assignee_ids`     | `Option<Vec<Uuid>>` (lax) | `None`             | `None` dan `[]` sama-sama memicu fallback                                         |
| `label_ids`        | `Option<Vec<Uuid>>` (lax) | `None`             |                                                                                   |
| `state_id`         | `Option<Uuid>` (lax)      | resolve state      | `resolve_issue_state` sudah ada                                                   |
| `description_html` | `Option<String>`          | `'<p></p>'`        | `description_json` tetap `'{}'`, `description_stripped` NULL (web tidak mengirim) |
| `priority`         | `Option<String>`          | `"none"`           |                                                                                   |
| `start_date`       | `Option<String>`          | NULL               | parse `%Y-%m-%d`                                                                  |
| `target_date`      | `Option<String>`          | NULL               | parse `%Y-%m-%d`                                                                  |
| `parent_id`        | `Option<Uuid>`            | NULL               |                                                                                   |
| `type_id`          | `Option<Uuid>`            | NULL               |                                                                                   |
| `estimate_point`   | `Option<Uuid>` (lax)      | NULL               | kolom `estimate_point_id`                                                         |

`cycle_id` / `module_ids` diabaikan — bukan field model, Django juga
mengabaikannya, dan web menanganinya lewat call lanjutan (`base.tsx:180-215`).
`is_draft` / `project_id` juga diabaikan karena web tidak mengirimnya di jalur
ini (draft lewat endpoint `/draft-issues/` terpisah); pemanggil API yang
mengirim `is_draft` tidak didukung — deviasi kecil dicatat.

**Sukses 201:** objek flat 26 key, urutan sama dengan `IssueListRow`:
`id`, `name`, `state_id`, `sort_order`, `completed_at`, `estimate_point`,
`priority`, `start_date`, `target_date`, `sequence_id`, `project_id`,
`parent_id`, `cycle_id`, `module_ids`, `label_ids`, `assignee_ids`,
`sub_issues_count`, `created_at`, `updated_at`, `created_by`, `updated_by`,
`attachment_count`, `link_count`, `is_draft`, `archived_at`, `deleted_at`.
`label_ids`/`assignee_ids` adalah array UUID (bukan null). `updated_by` = null
saat create (parity Django `db/models/base.py:36-39`).

**Error:**

| Kondisi                               | Status | Body                                                    |
| ------------------------------------- | ------ | ------------------------------------------------------- |
| Gate gagal (sudah ada, tidak berubah) | 403    | `{"error": "You don't have the required permissions."}` |
| Validasi field apa pun                | 400    | `{"error": "<pesan>"}`                                  |
| Error DB internal                     | 500    | `{"error": "..."}`                                      |

## Alur handler

```
1. Gate require_project_write                      (existing)
2. Parse body (CreateIssue diperluas)
3. Normalisasi: dedupe assignee_ids/label_ids      (HashSet, urutan dipertahankan)
4. Validasi semua field → 400                      (baru, eksplisit)
5. resolve_issue_state                             (existing)
6. BEGIN TX
     a. resolve workspace_id
     b. insert_issue(field baru, updated_by NULL)  → issue id
     c. apply_create_bridges                        → assignee/label + default assignee
     d. insert_created_activity
        insert_assignee_activities (bila key assignee_ids ada)
        insert_subscribers
   COMMIT
7. fetch_issue_row(pool, issue_id) → Some(IssueListRow) (di luar tx)
8. Some → 201 + JSON; None → 404 (lihat bagian Response)
```

Aturan validasi (semua → 400):

| Field                      | Aturan                                                             |
| -------------------------- | ------------------------------------------------------------------ |
| `name`                     | non-empty setelah trim, ≤255 char                                  |
| `priority`                 | salah satu `low\|medium\|high\|urgent\|none`                       |
| `state_id`                 | ada & milik project (pertahankan cek lama, tanpa filter triage)    |
| `assignee_ids`             | tiap id = project member aktif `role >= 15` (setelah dedupe)       |
| `label_ids`                | tiap id milik project (setelah dedupe)                             |
| `type_id`                  | ada & belum dihapus (pola v1 `v1/work_item.rs:591-596`)            |
| `parent_id`                | ada, belum dihapus, project sama (pola v1 `:619-623`)              |
| `estimate_point`           | milik estimate project (pola v1 `:614-618`)                        |
| `start_date`/`target_date` | parse `%Y-%m-%d`, error `"Invalid date: {s}"` (pola v1 `:624-625`) |

Gaya error: closure `bad(msg)` yang mengembalikan
`Ok((StatusCode::BAD_REQUEST, Json(json!({"error": msg}))))` seperti v1
(`v1/work_item.rs:581`), bukan `AppError` (selalu 500,
`common/src/errors.rs:17-21`).

## Bridges & default assignee

`replace_bridges` dipindah dari `v1/work_item.rs:630-661` →
`issue_common.rs` sebagai `pub(crate)`; v1 mengimpor dari sana (perilaku v1
tidak berubah: `None` = jangan sentuh, `Some([])` = soft-delete semua).

Handler me-resolve `workspace_id` sekali di dalam tx
(`SELECT workspace_id FROM projects WHERE id = $1`) dan meneruskannya ke semua
helper.

Helper baru `apply_create_bridges(tx, issue_id, project_id, workspace_id,
creator, requested_assignees, requested_labels)`:

- Assignee: bila requested non-kosong → insert tiap id; bila `None`/kosong →
  baca `projects.default_assignee_id` di dalam tx, cek eligible (project member
  aktif `role >= 15`, `deleted_at IS NULL`) lalu insert satu baris. Pola sama
  dengan `draft.rs:1263-1280`.
- Label: insert tiap id.
- Insert memakai `ON CONFLICT DO NOTHING` (index unik parsial
  `issue_assignees` / `issue_subscribers`); `created_by_id` = creator,
  `updated_by_id` = NULL (parity Django: baris bridge memakai
  `issue.updated_by_id` yang NULL saat create, `serializers/issue.py:211-212`).
- Return: `()` — verifikasi lewat DB di tes.

## Activities & subscribers

Modul baru `crates/api/src/routes/issue_activity_write.rs`:

- `insert_created_activity(tx, issue_id, project_id, workspace_id, actor, epoch)` —
  satu baris `issue_activities`: `verb='created'`, `field=NULL`,
  `old_value=NULL`, `new_value=NULL`, `comment='created the issue'`,
  `attachments='{}'`, `actor_id=actor` (= pembuat issue), `created_by_id=actor`,
  `updated_by_id=NULL`, `created_at` = `issues.created_at`, `epoch` = unix
  timestamp. Mirror `issue_activities_task.py:567-579` + `BaseModel.save`
  create semantics (`db/models/base.py:36-39`).
- `insert_assignee_activities(tx, issue_id, project_id, workspace_id, actor, added)` —
  hanya dipanggil bila key `assignee_ids` ada di body (termasuk `[]` → nol
  baris). Per assignee yang diminta (bukan default assignee):
  `verb='updated'`, `field='assignees'`, `old_value=''`,
  `new_value=users.display_name`, `new_identifier=user_id`,
  `comment='added assignee '`, `attachments='{}'`, `actor_id=actor`,
  `created_by_id=NULL`, `updated_by_id=NULL` (parity `bulk_create` Django yang
  melewati `save()`, `issue_activities_task.py:385-423`).
- `insert_subscribers(tx, issue_id, project_id, workspace_id, ids)` —
  baris `issue_subscribers` dengan `created_by_id` / `updated_by_id` =
  **assignee itu sendiri** (parity `issue_activities_task.py:394-407`),
  `ON CONFLICT DO NOTHING` (padanan `ignore_conflicts=True`).
- **Tidak ada activity label** (Django tidak punya `track_labels` di jalur
  create).
- **Tidak ada notifikasi** — API Rust belum pernah menulis `notifications` dan
  worker masih stub (`worker/handlers/mod.rs:44-51`); gap ini dicatat, bukan
  dibangun di slice ini.

## Response helper

`fetch_issue_row(pool, issue_id) -> Option<IssueListRow>` di `issue_query.rs`:
26 kolom identik `LIST_SELECT_SQL` (`issue_query.rs:68-70`) dengan
`WHERE i.id = $1` dan guard yang sama dengan `list_by_ids` — `deleted_at IS
NULL`, `archived_at IS NULL`, `is_draft = false`, state non-triage, project
tidak diarsipkan. Guard ini mirror `Issue.issue_objects`
(`db/models/issue.py:92-101`) yang dipakai re-query Django create
(`views/issue/base.py:432-441`). Dipanggil setelah commit (pola v1
`v1/work_item.rs:749-752`).

Edge case: bila row tidak ditemukan oleh query berguard (mis. pemanggil API
menyisipkan `state_id` triage eksplisit), Django jatuh ke 500 karena
`user_timezone_converter(None, ...)` (`utils/timezone_converter.py:20-24`);
Rust mengembalikan 404 `missing()` — deviasi sadar, mengikuti konvensi handler
v1 create.

## Deviasi dari Django (dicatat sadar)

1. **Validasi ketat 400** vs Django web yang membuang ID assignee/label invalid
   diam-diam. Dipilih di brainstorming butir 2.
2. **Dedupe + `ON CONFLICT DO NOTHING`** vs `bulk_create` Django yang kehilangan
   seluruh batch saat ada konflik/duplikat. Data valid tetap tersimpan.
3. **Satu transaksi** untuk issue + bridges + activities + subscribers vs
   autocommit Django (bridges ditulis setelah `Issue.save`). Lebih aman.
4. **Timestamp UTC** vs konversi timezone per-user Django
   (`user_timezone_converter`); konsisten dengan list Rust
   (`issue_query.rs:345-346`).
5. **Tanpa notifikasi** (in-app/email) — infra belum ada.
6. **Perbaikan parity:** `updated_by` issue = null saat create (sekarang Rust
   mengisi creator); baris bridge `updated_by_id` juga null.
7. **Response fetch tidak menemukan row → 404** (Django 500 karena bug
   `user_timezone_converter(None)`); hanya mungkin bila pemanggil API memberi
   `state_id` triage eksplisit.

## Testing

Perluas `crates/api/tests/issue_create_test.rs` (fixture `Scratch` + `purge`,
wajib `--test-threads=1`). Fixture ditambah: helper set default assignee,
helper insert label, `display_name` user, dan cleanup untuk
`issue_assignees` / `issue_labels` / `issue_activities` / `issue_subscribers`
(cleanup sekarang belum menghapus tabel-tabel ini).

- **Bridges:** assignee+label tertulis dengan `created_by_id`/`workspace_id`
  benar; default assignee saat absen dan saat `[]`; tidak dipakai bila assignee
  dikirim; tidak dipakai bila default tidak eligible (bukan member/role <15);
  duplikat → satu baris.
- **Field:** description/priority/dates/parent/type/estimate tersimpan;
  default `'<p></p>'`/`"none"` saat absen.
- **Response:** tepat 26 key; `assignee_ids`/`label_ids` array terisi;
  `sequence_id` benar; `updated_by` null; status 201.
- **Activities/subscribers:** created row (verb/comment/field/actor/epoch);
  activity + subscriber per assignee yang diminta; default assignee tanpa
  activity/subscriber; label tanpa activity.
- **Validasi:** tiap field invalid → 400; pesan body.
- 11 tes lama (authz + sequence) tetap hijau; `intake` tetap kompatibel.
- **E2E HTTP** di task akhir: rebuild `plane-api-rs:local`, create via curl
  (assignee+label+description), verifikasi DB + shape 26-key, cleanup.

## Out of scope

- Notifikasi in-app/email dan subscriber untuk notifikasi.
- `cycle_id`/`module_ids` (web menangani via call lanjutan), `is_draft`
  (endpoint draft terpisah).
- Bridges untuk jalur draft-promote (`draft.rs`) dan intake (`intake.rs`) —
  keduanya sudah punya perilakunya sendiri; tidak diubah.
- `issue_description_version`, webhooks (`model_activity.delay`), konversi
  timezone per-user.
- Perbaikan 500→400 untuk handler lain di luar create.
- Hygiene terpisah: `insert_issue` `pub` → `pub(crate)` (finding #5).

## File yang disentuh

| File                                            | Perubahan                                                       |
| ----------------------------------------------- | --------------------------------------------------------------- |
| `crates/api/src/routes/issue_write.rs`          | `CreateIssue`, validasi 400, `NewIssue`/`insert_issue`, handler |
| `crates/api/src/routes/issue_common.rs`         | `replace_bridges` (pindah), `apply_create_bridges`              |
| `crates/api/src/routes/issue_activity_write.rs` | **baru** — activities + subscribers                             |
| `crates/api/src/routes/issue_query.rs`          | `fetch_issue_row` 26-key                                        |
| `crates/api/src/routes/v1/work_item.rs`         | impor `replace_bridges` dari `issue_common`                     |
| `crates/api/src/routes/mod.rs`                  | registrasi `pub mod issue_activity_write;`                      |
| `crates/api/tests/issue_create_test.rs`         | tes baru + fixture                                              |
| `crates/api/parity-inventory.json`              | update note route POST create                                   |
