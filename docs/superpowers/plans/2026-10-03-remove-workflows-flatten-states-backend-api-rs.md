# Hapus Workflow & Flatten State — Plan 2: Backend api-rs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menghapus seluruh route/handler workflow + engine transisi dari api-rs, menyederhanakan state/type/default-state ke model flat (type + state project), dan menyelaraskan migrasi SQL delta, inventory, test, serta script.

**Architecture:** api-rs memakai sqlx runtime query; baseline `migrations/0001_initial.sql` (squash s/d Django 0122) tidak memuat tabel workflow, jadi delta `0002_remove_workflows.sql` hanya berisi DDL `IF EXISTS` (no-op bila Django `0126` sudah jalan). Route workflow dihapus; `resolve_issue_state` menjadi project-only; seed workspace menyisakan work item type.

**Tech Stack:** Rust (axum + sqlx, runtime query), PostgreSQL, Docker compose local/test.

**Spec:** `docs/superpowers/specs/2026-10-03-remove-workflows-flatten-states-design.md`
**Prasyarat:** Plan 1 (`docs/superpowers/plans/2026-10-03-remove-workflows-flatten-states-schema.md`) selesai — model/migrasi Django `0126` sudah ada. Sebelum menjalankan test DB-backed, terapkan migrasi ke DB lokal:

```bash
docker compose -f docker-compose-local.yml run --rm migrator
```

**Backup wajib sebelum migrasi DB lokal** (`0126` destruktif):

```bash
docker exec plane-db pg_dump -U plane -d plane -Fc -f /tmp/plane-pre-flatten.dump
```

---

## File Structure

| File                                                                           | Aksi   | Tanggung jawab                                                       |
| ------------------------------------------------------------------------------ | ------ | -------------------------------------------------------------------- |
| `apps/api-rs/migrations/0002_remove_workflows.sql`                             | Create | DDL delta drop workflow + index constraint state baru (guarded)      |
| `apps/api-rs/crates/api/src/routes/validation.rs`                              | Create | `validate_name` pindahan dari `workflow.rs`                          |
| `apps/api-rs/crates/api/src/routes/workflow.rs`                                | Delete | Seluruh workflow API + engine transisi                               |
| `apps/api-rs/crates/api/src/routes/mod.rs`                                     | Modify | Tukar `pub mod workflow;` → `pub mod validation;`                    |
| `apps/api-rs/crates/api/src/main.rs`                                           | Modify | Hapus route workflow/workflow-map; unlink pindah handler             |
| `apps/api-rs/crates/api/src/routes/state.rs`                                   | Modify | State flat: buang `type_id`/`workflow_state_id` + guard typed mirror |
| `apps/api-rs/crates/api/src/routes/workspace.rs`                               | Modify | SELECT state mengikuti `StateFullRow` baru                           |
| `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`                       | Modify | Buang `workflow` payload/kolom/efek mirror; terima `unlink_type`     |
| `apps/api-rs/crates/api/src/routes/issue_common.rs`                            | Modify | `resolve_issue_state` project-only                                   |
| `apps/api-rs/crates/api/src/routes/issue_write.rs`                             | Modify | Call site resolver                                                   |
| `apps/api-rs/crates/api/src/routes/issue_update.rs`                            | Modify | Call site + hapus enforcement transisi + ownership check project     |
| `apps/api-rs/crates/api/src/routes/draft.rs`                                   | Modify | Wrapper default state + hapus enforcement/`state_matches_type`       |
| `apps/api-rs/crates/api/src/routes/intake.rs`                                  | Modify | Accept memakai resolver project-only                                 |
| `apps/api-rs/crates/api/src/routes/v1/work_item.rs`                            | Modify | Call site + hapus enforcement                                        |
| `apps/api-rs/crates/api/src/routes/release.rs`, `review.rs`                    | Modify | Import `validate_name` baru                                          |
| `apps/api-rs/crates/api/src/seed.rs`                                           | Modify | Seed type saja (tanpa workflow/state/transisi)                       |
| `apps/api-rs/crates/api/parity-inventory.json`                                 | Modify | Hapus entri workflow; perbarui handler unlink                        |
| `apps/api-rs/crates/api/tests/workflow_test.rs`, `workflow_transition_test.rs` | Delete | Route/model workflow hilang                                          |
| `apps/api-rs/crates/api/tests/*.rs`                                            | Modify | Fixture state flat + route inventory                                 |
| `scripts/smoke.sh`, `scripts/shadow.sh`, `scripts/v1-smoke.py`                 | Modify | Hapus endpoint workflow                                              |

**Urutan milestone:** B1 (migrasi SQL) → B2 (hapus route/workflow.rs) → B3 (state flat) → B4 (type flat) → B5 (resolver) → B6 (transisi) → B7 (seed) → B8 (test/inventory/script + verifikasi). Kompilasi (`cargo check`) dijalankan tiap task.

---

## Task B1: Migrasi SQL delta `0002_remove_workflows.sql`

**Files:**

- Create: `apps/api-rs/migrations/0002_remove_workflows.sql`

- [ ] **Step 1: Tulis delta SQL**

```sql
-- Hapus skema workflow (Django 0123-0125) dan flatten constraint state.
-- Guarded: no-op bila Django 0126 sudah menjalankannya, aman di fresh DB
-- (baseline 0001 tidak memuat tabel workflow).
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_legacy_name_project_when_deleted_at_null;
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_name_project_type_when_deleted_at_null;
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_project_workflow_state_when_deleted_at_null;
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_default_project_type_when_deleted_at_null;
ALTER TABLE states DROP COLUMN IF EXISTS type_id;
ALTER TABLE states DROP COLUMN IF EXISTS workflow_state_id;
ALTER TABLE issue_types DROP COLUMN IF EXISTS workflow_id;
DROP TABLE IF EXISTS workflow_transitions;
DROP TABLE IF EXISTS workflow_states;
DROP TABLE IF EXISTS workflows;
CREATE UNIQUE INDEX IF NOT EXISTS state_unique_name_project_when_deleted_at_null
  ON states (project_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS state_unique_default_project_when_deleted_at_null
  ON states (project_id) WHERE deleted_at IS NULL AND "default" = true;
```

- [ ] **Step 2: Verifikasi migrasi ter-embed dan kompilasi**

Run:

```bash
cd apps/api-rs && cargo check -p api
```

Expected: sukses (sqlx `migrate!` meng-embed file baru saat kompilasi).

- [ ] **Step 3: Verifikasi idempoten di DB lokal**

Run (setelah Plan 1 & backup):

```bash
docker compose -f docker-compose-local.yml run --rm migrator
docker exec plane-db psql -U plane -d plane -c "SELECT to_regclass('workflows'), to_regclass('workflow_states'), to_regclass('workflow_transitions');"
docker exec plane-db psql -U plane -d plane -c "SELECT indexname FROM pg_indexes WHERE tablename='states' AND indexname LIKE 'state_unique%';"
```

Expected: tiga `to_regclass` NULL; index `state_unique_name_project_when_deleted_at_null` dan `state_unique_default_project_when_deleted_at_null` ada.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0002_remove_workflows.sql
git commit -m "feat(api-rs): add sql delta removing workflow schema"
```

---

## Task B2: Hapus route & modul workflow, pindahkan `validate_name`

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/validation.rs`
- Delete: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/src/routes/release.rs:22`, `review.rs:24`

- [ ] **Step 1: Buat modul validasi**

```rust
// apps/api-rs/crates/api/src/routes/validation.rs
//! Shared DRF-style name validation (dipindah dari `routes/workflow.rs`).

/// Validasi nama ala DRF (required, <=255). Mengembalikan nama ter-trim.
pub fn validate_name(name: &str, field: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.chars().count() > 255 {
        return Err(format!("Ensure {field} has no more than 255 characters."));
    }
    Ok(trimmed.to_string())
}
```

- [ ] **Step 2: Tukar modul dan hapus `workflow.rs`**

Di `routes/mod.rs`, ganti `pub mod workflow;` dengan `pub mod validation;`. Lalu:

```bash
git rm apps/api-rs/crates/api/src/routes/workflow.rs
```

- [ ] **Step 3: Perbarui import `validate_name`**

Di `release.rs` dan `review.rs`, ganti `use super::workflow::validate_name;` menjadi `use super::validation::validate_name;`.

- [ ] **Step 4: Hapus registrasi route workflow di `main.rs`**

Hapus blok `.route(...)` berikut (cari dengan `rg -n "routes::workflow" main.rs`):

- `/api/workspaces/:slug/workflows/`
- `/api/workspaces/:slug/workflows/:workflow_id/`
- `/api/workspaces/:slug/workflows/:workflow_id/states/`
- `/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/`
- `/api/workspaces/:slug/workflows/:workflow_id/transitions/`
- `/api/workspaces/:slug/workflows/:workflow_id/transitions/:transition_id/`
- `/api/workspaces/:slug/projects/:project_id/workflow-map/`

Untuk route `/api/workspaces/:slug/projects/:project_id/work-item-types/:type_id/`, arahkan handler ke `routes::v1::work_item_type::unlink_type` (dibuat di Task B4). Hapus komentar blok workflow yang tidak relevan.

- [ ] **Step 5: Kompilasi (masih gagal karena handler unlink belum ada)**

Run:

```bash
cd apps/api-rs && cargo check -p api 2>&1 | head -40
```

Expected: error `routes::v1::work_item_type::unlink_type` belum ada. Error ini diselesaikan di Task B4; lanjutkan.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/validation.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/src/routes/release.rs apps/api-rs/crates/api/src/routes/review.rs
git rm apps/api-rs/crates/api/src/routes/workflow.rs
git commit -m "refactor(api-rs): remove workflow routes and module"
```

---

## Task B3: `state.rs` flat — buang kolom & guard typed mirror

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/state.rs`
- Modify: `apps/api-rs/crates/api/src/routes/workspace.rs` (SELECT state)

- [ ] **Step 1: Temukan semua proyeksi `StateFullRow` dan referensi kolom**

Run:

```bash
rg -n "STATE_FULL_SELECT_SQL|type_id, s.workflow_state_id|workflow_state_id|guard_typed_state_mutation|TYPED_MIRROR_WRITE_MSG" apps/api-rs/crates/api/src/routes
```

Expected: semua hit ada di `state.rs`; `workspace.rs` memakai `STATE_FULL_SELECT_SQL`.

- [ ] **Step 2: Ganti SELECT dan struct**

Ganti `STATE_FULL_SELECT_SQL` menjadi:

```rust
const STATE_FULL_SELECT_SQL: &str = "SELECT s.id, s.project_id, s.workspace_id, s.name, s.color, s.\"group\", s.\"default\" AS is_default, s.description, s.sequence FROM states s";
```

Di `StateFullRow` (sekitar baris 536-560), hapus field `pub type_id` dan `pub workflow_state_id`. Di `state_serializer_json`, hapus key `"type_id"` dan `"workflow_state_id"`.

- [ ] **Step 3: Bersihkan guard & filter typed di semua handler**

- Hapus konstanta `TYPED_MIRROR_WRITE_MSG` dan fungsi `guard_typed_state_mutation`.
- `create`: pada cek duplikat, hapus `AND type_id IS NULL`; pada INSERT-RETURNING, ganti SELECT akhir menjadi kolom `STATE_FULL_SELECT_SQL` (tanpa `s.type_id, s.workflow_state_id`).
- `patch`: hapus blok lookup `SELECT type_id, workflow_state_id ...` + pemanggilan guard; pada cek duplikat hapus `AND type_id IS NULL`; pada UPDATE hapus `AND type_id IS NULL`.
- `destroy`: hapus lookup + guard; UPDATE hapus `AND type_id IS NULL`.
- `mark_default`: hapus lookup + guard; kedua UPDATE hapus `AND type_id IS NULL`.
- `intake_state`: ganti SELECT yang memuat `s.type_id, s.workflow_state_id` menjadi kolom `STATE_FULL_SELECT_SQL`.
- `ws_states` (`workspace.rs`): pastikan SELECT-nya memakai `STATE_FULL_SELECT_SQL` baru.

- [ ] **Step 4: Hapus/ubah test modul di `state.rs`**

- Hapus modul test `typed_mirror_guard_tests`.
- Pada test serializer, hapus assertion `single["type_id"]` dan `single["workflow_state_id"]`; sesuaikan ekspektasi jumlah key bila ada.

- [ ] **Step 5: Verifikasi tidak ada referensi tersisa + kompilasi**

Run:

```bash
rg -n "type_id|workflow_state_id" apps/api-rs/crates/api/src/routes/state.rs apps/api-rs/crates/api/src/routes/workspace.rs
cd apps/api-rs && cargo check -p api 2>&1 | head -40
```

Expected: `rg` kosong (kecuali komentar bila ada — hapus juga); `cargo check` hanya menyisakan error `unlink_type` dari Task B2.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/state.rs apps/api-rs/crates/api/src/routes/workspace.rs
git commit -m "refactor(api-rs): flatten project state handlers"
```

---

## Task B4: `work_item_type.rs` flat + handler `unlink_type`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`

- [ ] **Step 1: Buang `workflow` dari kolom, struct, JSON, dan payload**

- `TYPE_COLS`: hapus `t.workflow_id AS workflow, `.
- `V1WorkItemTypeRow`: hapus `pub workflow: Option<uuid::Uuid>,`.
- `v1_work_item_type_json`: hapus `"workflow": row.workflow,`.
- `V1CreateWorkItemType`: hapus field `workflow`.
- `V1UpdateWorkItemType`: hapus field `workflow` dan fungsi `deserialize_optional_nullable` (pastikan tidak dipakai field lain; `rg "deserialize_optional_nullable"`).

- [ ] **Step 2: Sederhanakan `create_type`**

- Hapus dua cek `body.workflow` (epic + "Workflows are managed...").
- Hapus perhitungan `workflow_id` dan panggilan `ensure_workflow_for_type`.
- INSERT `issue_types`: hapus kolom `workflow_id` dan bind-nya; RETURNING hapus `workflow_id AS workflow`.
- Hapus loop `materialize_type_states` setelah `link_projects`.

- [ ] **Step 3: Sederhanakan `update_type`**

- Hapus cek `body.workflow`.
- Ganti query `current` menjadi:

```rust
    let current: Option<(bool, bool, String)> = sqlx::query_as(
        "SELECT is_epic, is_active, name FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some((current_is_epic, current_is_active, current_name)) = current else {
        return Ok(missing());
    };
```

- Hapus blok `sync_type_workflow` dan loop `materialize_type_states` setelah commit. Hapus variabel `effective_is_epic`, `effective_is_active`, `effective_name` bila tidak lagi terpakai (compiler akan menandai).
- Pertahankan logika link project (`link_projects`) tanpa materialization.

- [ ] **Step 4: Sederhanakan delete & import**

- `delete_workspace`: hapus lookup `workflow_id` dan panggilan `soft_delete_workflow_cascade`.
- `import_to_project`: hapus blok materialize (query `linked` + loop) — cukup sampai loop INSERT `project_issue_types`.
- `delete_project`: ganti `crate::routes::workflow::detach_type_from_project(...)` menjadi soft-delete link lokal:

```rust
    sqlx::query(
        "UPDATE project_issue_types SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
```

- [ ] **Step 5: Tambahkan handler `unlink_type` di file ini**

```rust
/// DELETE `/api/workspaces/:slug/projects/:project_id/work-item-types/:type_id/`
/// (un-enable type dari project). Soft-delete link saja.
pub async fn unlink_type(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, type_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    let (project_ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL AND w.deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !project_ok {
        return Ok(missing());
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Type is in use by work items"));
    }
    sqlx::query(
        "UPDATE project_issue_types SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

- [ ] **Step 6: Verifikasi tidak ada referensi workflow + kompilasi penuh**

Run:

```bash
rg -n "workflow" apps/api-rs/crates/api/src/routes/v1/work_item_type.rs
cd apps/api-rs && cargo check -p api
```

Expected: `rg` kosong; `cargo check` sukses (error `unlink_type` dari Task B2 hilang).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs
git commit -m "refactor(api-rs): flatten work item type handlers"
```

---

## Task B5: `resolve_issue_state` project-only + semua call site

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_common.rs`
- Modify: `apps/api-rs/crates/api/src/routes/issue_write.rs:297`
- Modify: `apps/api-rs/crates/api/src/routes/issue_update.rs:755-797`
- Modify: `apps/api-rs/crates/api/src/routes/draft.rs:480-493, 609-626, 1329-1341`
- Modify: `apps/api-rs/crates/api/src/routes/intake.rs:1592-1603`
- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs:834, 987-1017`

- [ ] **Step 1: Ganti `resolve_issue_state`**

Ganti seluruh fungsi (dan doc comment-nya) di `issue_common.rs` menjadi:

```rust
/// Pick the effective state of a new issue: explicit, else project default,
/// else first non-triage state by sequence.
pub async fn resolve_issue_state(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    explicit: Option<Uuid>,
) -> Result<Option<Uuid>, sqlx::Error> {
    if explicit.is_some() {
        return Ok(explicit);
    }
    let default_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    if default_id.is_some() {
        return Ok(default_id);
    }
    let first_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(resolve_effective_state(explicit, default_id, first_id))
}
```

`resolve_effective_state` tetap (dipakai test crate `issue_test.rs`).

- [ ] **Step 2: Perbarui call site**

- `issue_write.rs:297` → `resolve_issue_state(&st.pool, project_id, body.state_id)`.
- `issue_update.rs` (baris 769, 773, 795) → buang argumen `effective_type_id`.
- `v1/work_item.rs` (baris 834, 990) → buang argumen `body.type_id`/`effective_type_id`.
- `intake.rs`: ganti lookup scope menjadi hanya project:

```rust
            let issue_scope: Option<(uuid::Uuid,)> =
                sqlx::query_as("SELECT project_id FROM issues WHERE id = $1")
                    .bind(issue_id)
                    .fetch_optional(&st.pool)
                    .await?;
            match issue_scope {
                Some((issue_project_id,)) => {
                    resolve_issue_state(&st.pool, issue_project_id, None).await?
                }
                None => None,
            }
```

- `draft.rs`: ganti `resolve_default_state` dan buang `state_matches_type`:

```rust
async fn resolve_default_state(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    explicit: Option<uuid::Uuid>,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    match project_id {
        Some(project_id) => {
            crate::routes::issue_common::resolve_issue_state(pool, project_id, explicit).await
        }
        None => Ok(explicit),
    }
}
```

Blok resolusi state draft (baris ~1329-1341) menjadi:

```rust
    let state_id = if let Some(sid) = b.state_id {
        Some(sid)
    } else if let Some(sid) = d.state_id {
        if state_belongs_to_project(&st.pool, project_id, sid).await? {
            Some(sid)
        } else {
            resolve_default_state(&st.pool, Some(project_id), None).await?
        }
    } else {
        resolve_default_state(&st.pool, Some(project_id), None).await?
    };
```

Ganti fungsi `state_matches_type` dengan:

```rust
/// Apakah `state_id` sah dipakai di project ini: state hidup dan non-triage.
async fn state_belongs_to_project(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
    state_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let (ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
         AND deleted_at IS NULL AND is_triage = false AND \"group\" != 'triage')",
    )
    .bind(state_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}
```

Perbarui pemanggil `resolve_default_state` di baris ~850 (buang `b.type_id`) dan `state_belongs_to_project` di baris ~1300, serta semua referensi `effective_type` yang menjadi tidak terpakai (compiler menandai; `effective_type` masih dipakai untuk update `type_id` issue, pertahankan bila masih perlu).

- [ ] **Step 3: Sederhanakan validasi state saat ganti type**

- `issue_update.rs` (baris ~777-797): ganti query validasi type dengan validasi project:

```rust
    if type_changed {
        new_state_id = match body.state_id {
            Some(Some(explicit)) => {
                let (ok,): (bool,) = sqlx::query_as(
                    "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
                     AND deleted_at IS NULL AND is_triage = false AND \"group\" != 'triage')",
                )
                .bind(explicit)
                .bind(project_id)
                .fetch_one(&st.pool)
                .await?;
                if !ok {
                    return Ok(bad("State is not valid for this project"));
                }
                Some(explicit)
            }
            _ => resolve_issue_state(&st.pool, project_id, None).await?,
        };
    }
```

- `v1/work_item.rs`: pada cabang ganti type, validasi state eksplisit dengan pola project yang sama (cari blok yang memvalidasi state terhadap `effective_type_id` dan ganti pesan ke "State is not valid for this project").

- [ ] **Step 4: Kompilasi + unit test non-DB**

Run:

```bash
cd apps/api-rs && cargo check -p api && cargo test -p api --lib
```

Expected: kompilasi sukses; unit test lib lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_common.rs apps/api-rs/crates/api/src/routes/issue_write.rs apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/src/routes/draft.rs apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "refactor(api-rs): resolve state from project only"
```

---

## Task B6: Hapus engine transisi & pemanggilnya

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_update.rs:29, 798-826`
- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs:18-20, 993-1015`
- Modify: `apps/api-rs/crates/api/src/routes/draft.rs:12, 1342-1370`

- [ ] **Step 1: Hapus blok enforcement**

- `issue_update.rs`: hapus blok `if state_changed && !type_changed { ... }` (baris ~799-826). `state_changed` tetap dihitung untuk `completed_at`.
- `v1/work_item.rs`: pada cabang `else` (type tidak berubah), hapus blok `if let Some(target) = body.state { ... validate ... }`; sisakan `body.state.map(Some)`.
- `draft.rs`: hapus blok `if state_changed && !type_changed { ... }` (baris ~1348-1370). `state_changed` tetap bila dipakai; jika tidak, hapus.

- [ ] **Step 2: Hapus import workflow**

- `issue_update.rs`: hapus `use super::workflow::{transition_denied, validate_initial_transition, validate_state_transition};`.
- `v1/work_item.rs`: hapus `use crate::routes::workflow::{transition_denied, validate_initial_transition, validate_state_transition};`.
- `draft.rs`: hapus `use super::workflow::{transition_denied, validate_initial_transition, validate_state_transition};`.

- [ ] **Step 3: Verifikasi tidak ada referensi transisi**

Run:

```bash
rg -n "validate_state_transition|validate_initial_transition|transition_denied|workflow::" apps/api-rs/crates/api/src
cd apps/api-rs && cargo check -p api && cargo test -p api --lib
```

Expected: `rg` kosong; kompilasi + unit test lulus.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/src/routes/v1/work_item.rs apps/api-rs/crates/api/src/routes/draft.rs
git commit -m "refactor(api-rs): remove workflow transition enforcement"
```

---

## Task B7: Seed workspace — type saja

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs:293-572, 920, 980-1000`

- [ ] **Step 1: Ganti data seed**

- Ganti `WORKFLOW_SEEDS` dengan:

```rust
/// Nama work item type seed (parity migrasi Django 0124 untuk type saja).
const TYPE_SEEDS: &[&str] = &["Incident", "Problem", "Change", "Improvement"];
```

- Hapus fungsi `seed_workflow_id`, `seed_workflow_state_id`, dan `group_color`.

- [ ] **Step 2: Sederhanakan `seed_issue_type_id`**

Hapus parameter `workflow_id: Uuid` dan kolom `workflow_id` + bind-nya:

```rust
async fn seed_issue_type_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    bot_id: Uuid,
    type_name: &str,
) -> Result<Uuid, sqlx::Error> {
    let external_id = format!("issue-type:{}", slugify(type_name));
    if let Some((id,)) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM issue_types WHERE workspace_id = $1 AND external_source = $2 \
         AND external_id = $3 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .bind(SEED_EXTERNAL_SOURCE)
    .bind(&external_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }
    let (conflict,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issue_types WHERE workspace_id = $1 AND name = $2 \
         AND deleted_at IS NULL)",
    )
    .bind(workspace_id)
    .bind(type_name)
    .fetch_one(&mut **tx)
    .await?;
    if conflict {
        return Err(sqlx::Error::Protocol(format!(
            "seed conflict: workspace {workspace_id} already has an active issue type named '{type_name}'"
        )));
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, external_source, external_id, created_by_id, updated_by_id, \
         created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '', '{}', false, false, true, 0, $2, $3, $4, $5, $5, now(), now()) \
         RETURNING id",
    )
    .bind(type_name)
    .bind(workspace_id)
    .bind(SEED_EXTERNAL_SOURCE)
    .bind(&external_id)
    .bind(bot_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}
```

- [ ] **Step 3: Ganti `insert_workflows`**

```rust
/// Seed work item type default workspace (parity migrasi Django 0124 untuk
/// type; workflow/states/transitions tidak lagi ada). Tidak mengaktifkan type
/// di project mana pun. Idempotent via marker `plane-default-itsm`.
pub async fn insert_work_item_types(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    for type_name in TYPE_SEEDS {
        seed_issue_type_id(tx, workspace_id, bot_id, type_name).await?;
    }
    Ok(())
}
```

- [ ] **Step 4: Perbarui call site & test seed**

- `seed.rs:920`: ganti `insert_workflows(...)` → `insert_work_item_types(...)`.
- Test seed (~980-1000): ganti loop `WORKFLOW_SEEDS` dengan `TYPE_SEEDS`; assert 4 `issue_types` dengan marker; hapus assertion workflow/state/transisi.

- [ ] **Step 5: Verifikasi**

Run:

```bash
rg -n "workflow|WORKFLOW_SEEDS|seed_workflow|group_color" apps/api-rs/crates/api/src/seed.rs
cd apps/api-rs && cargo check -p api && cargo test -p api --lib
```

Expected: `rg` kosong; kompilasi + unit test lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/seed.rs
git commit -m "refactor(api-rs): seed work item types only"
```

---

## Task B8: Test, inventory, script, verifikasi penuh

**Files:**

- Delete: `apps/api-rs/crates/api/tests/workflow_test.rs`, `workflow_transition_test.rs`
- Modify: `apps/api-rs/crates/api/parity-inventory.json`
- Modify: `apps/api-rs/crates/api/tests/route_inventory_test.rs`, `parity_gate_test.rs`
- Modify: test lain yang gagal (daftar dari hasil `rg`)
- Modify: `scripts/smoke.sh`, `scripts/shadow.sh`, `scripts/v1-smoke.py`

- [ ] **Step 1: Hapus test workflow**

```bash
git rm apps/api-rs/crates/api/tests/workflow_test.rs apps/api-rs/crates/api/tests/workflow_transition_test.rs
```

- [ ] **Step 2: Perbarui inventory & route test**

- `parity-inventory.json`: hapus grup `"workflow"` (semua entri `/workflows/*` dan `workflow-map`); perbarui entri unlink agar `rust_handler` = `routes::v1::work_item_type::unlink_type`.
- `route_inventory_test.rs` / `parity_gate_test.rs`: sesuaikan daftar route yang diharapkan (hapus workflow, pertahankan work-item-types/import/unlink).

Run:

```bash
cd apps/api-rs && cargo test -p api --test route_inventory_test --test parity_gate_test
```

Expected: lulus.

- [ ] **Step 3: Bersihkan referensi workflow di seluruh test**

Run:

```bash
rg -l "workflow|WORKFLOW|workflow_state" apps/api-rs/crates/api/tests
```

Untuk tiap file yang muncul (mis. `workspace_seed_test.rs`, `v1_work_item_type_test.rs`, `module_state_test.rs`, `intake_triage_test.rs`, `issue_create_test.rs`, `issue_patch_test.rs`, `issue_test.rs`, `v1_work_item_test.rs`, `release_review_test.rs`, `war_room_test.rs`, `helper_test.rs`, `cutover_test.rs`, `v1_routes_test.rs`, `v1_project_test.rs`, `notification_test.rs`, `misc_test.rs`, `search_test.rs`, `analytic_test.rs`, `detail_*.rs`):

- Buang fixture/INSERT kolom `type_id`/`workflow_state_id` pada `states`, `workflow_id` pada `issue_types`, dan seluruh query ke `workflows`/`workflow_states`/`workflow_transitions`.
- Ganti assertion state type-aware menjadi state project default; hapus assertion route workflow.
- Untuk `workspace_seed_test.rs`: seed kini 4 type tanpa state/transisi; perbarui hitungan.

- [ ] **Step 4: Kompilasi seluruh test**

Run:

```bash
cd apps/api-rs && cargo test -p api --no-run
```

Expected: semua target test terkompilasi tanpa error. Perbaiki setiap error yang tersisa dengan pola langkah 3.

- [ ] **Step 5: Jalankan suite DB-backed**

Prasyarat: migrasi Django `0126` sudah diterapkan ke DB lokal (lihat header plan) dan backup sudah dibuat.

```bash
cd apps/api-rs && cargo test -p api --test route_inventory_test --test parity_gate_test --test v1_work_item_type_test --test workspace_seed_test --test module_state_test --test intake_triage_test
# Suite scratch-workspace WAJIB serial:
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test --test issue_patch_test --test issue_test -- --test-threads=1
```

Expected: semua lulus. Jika ada kegagalan karena fixture typed state, perbaiki ke state flat project dan ulangi.

- [ ] **Step 6: Update script**

- `scripts/smoke.sh`: hapus panggilan endpoint `/states/` workflow (baris ~84, 116, 211-220) — pertahankan smoke project states bila ada.
- `scripts/shadow.sh`: hapus entry workflow (baris ~17-18).
- `scripts/v1-smoke.py`: hapus catatan/step archive state-group workflow (baris ~129-138) yang merujuk workflow.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json apps/api-rs/crates/api/tests scripts/smoke.sh scripts/shadow.sh scripts/v1-smoke.py
git commit -m "test(api-rs): update suites for flat state model"
```

---

## Task B9: Rollout backend (setelah Plan 3 selesai)

**Files:** tidak ada perubahan kode; operasional.

- [ ] **Step 1: Backup DB live**

```bash
docker exec plane-db pg_dump -U plane -d plane -Fc -f /tmp/plane-pre-flatten.dump
docker cp plane-db:/tmp/plane-pre-flatten.dump /tmp/plane-pre-flatten.dump
```

- [ ] **Step 2: Rebuild backend detached**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Tunggu; link LTO bisa 10+ menit tanpa output — poll log, jangan abort.

- [ ] **Step 3: Verifikasi health + live**

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
systemctl --user restart plane-live.service
sleep 8 && curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
```

Expected: `200` keduanya.

- [ ] **Step 4: Smoke API**

```bash
curl -s http://localhost:8000/api/workspaces/<slug>/projects/<project_id>/states/ -H 'Cookie: ...' | head
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/api/workspaces/<slug>/projects/<project_id>/workflow-map/ -H 'Cookie: ...'
```

Expected: states 200 dengan 5 state non-triage; workflow-map 404.

---

## Self-Review

- **Spec coverage:** B1 = migrasi SQL delta; B2-B4 = route/handler workflow & state/type flat; B5 = resolusi default state; B6 = engine transisi; B7 = seed; B8 = test/script; B9 = rollout. Semua poin backend di spec tercakup.
- **Placeholder scan:** tidak ada TBD; langkah destruktif memakai perintah eksplisit. Beberapa langkah memakai pola grep karena rentang baris bergeser setelah edit — perintah verifikasinya eksplisit.
- **Type consistency:** `resolve_issue_state(pool, project_id, explicit)` konsisten di semua call site; `unlink_type` pindah ke `routes::v1::work_item_type` dan direferensikan `main.rs`; `STATE_FULL_SELECT_SQL`/`StateFullRow` konsisten dengan `workspace.rs`.
