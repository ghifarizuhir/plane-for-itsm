# Intake ITSM: Type Gate + Service Binding + Saran Service Jev — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Intake jadi front-door ITSM dengan klasifikasi: type wajib saat accept, service wajib untuk tipe ber-flag `requires_service`, dan Jev menyarankan category + service (suggest-only).

**Architecture:** Tambah kolom `requires_service` di `issue_types` + kolom saran service di `intake_triage_suggestions` (sqlx 0014 + Django 0128). Gate di handler accept Rust; apply category/service menulis langsung tanpa state resolution; question `service` baru di builder Jev. Web menambah picker type/service, gate tombol accept, dan aksi apply di panel saran.

**Tech Stack:** Rust (axum/sqlx), Django migrations, React + MobX + TypeScript, vitest, i18n.

Spec: `docs/superpowers/specs/2026-10-04-intake-itsm-type-service-gate-design.md`.

---

## File Structure

**Create:**

- `apps/api-rs/migrations/0014_intake_service_gate.sql` — kolom flag + kolom saran service.
- `apps/api/plane/db/migrations/0128_issue_type_requires_service.py` — AddField + backfill.
- `apps/web/core/components/inbox/accept-gate.ts` — kalkulasi gate murni.
- `apps/web/core/components/inbox/accept-gate.test.ts` — unit test gate.

**Modify (backend):**

- `apps/api/plane/db/models/issue_type.py` — field `requires_service`.
- `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs` — expose + guard flag.
- `apps/api-rs/crates/api/src/routes/v1/../tests` — `tests/v1_work_item_type_test.rs`.
- `apps/api-rs/crates/api/src/seed.rs` — seed Incident requires_service.
- `apps/api-rs/crates/api/src/routes/intake.rs` — accept gate, patch type, apply category/service, response service.
- `apps/api-rs/crates/ai/src/triage.rs` — question + mapping service.
- `apps/api-rs/crates/ai/src/triage_job.rs` — load services + simpan.
- `apps/api-rs/crates/api/tests/intake_triage_test.rs` — fixture service + test baru.

**Modify (web):**

- `packages/types/src/inbox.ts` — field service + triage field `service`.
- `packages/types/src/work-item-type.ts` — `requires_service`.
- `apps/web/core/store/inbox/inbox-issue.store.ts` — interface slug/project + apply category update.
- `apps/web/core/components/services/select/service-select.tsx` — override issue opsional.
- `apps/web/core/components/inbox/content/issue-properties.tsx` — baris Type + Service.
- `apps/web/core/components/inbox/content/inbox-issue-header.tsx` — gate tombol Accept.
- `apps/web/core/components/inbox/content/triage-suggestion.tsx` — apply category/service.
- `apps/web/core/components/work-item-types/type-form-modal.tsx` — toggle requires_service.
- `packages/i18n/src/locales/en/inbox.json` + `workspace-settings.json` — key baru.
- `docs/features/intake.md` — snapshot + changelog.

---

### Task 1: Migrasi sqlx 0014

**Files:**

- Create: `apps/api-rs/migrations/0014_intake_service_gate.sql`

- [ ] **Step 1: Tulis migrasi**

Isi file:

```sql
-- Intake ITSM: per-type service requirement + Jev service suggestion columns.
-- Applied at boot by `common::db::migrate` (sqlx migrate); IF NOT EXISTS so
-- Django 0128 (`AddField`) may land first without conflict.

ALTER TABLE public.issue_types
    ADD COLUMN IF NOT EXISTS requires_service boolean NOT NULL DEFAULT false;

ALTER TABLE public.intake_triage_suggestions
    ADD COLUMN IF NOT EXISTS service_id uuid,
    ADD COLUMN IF NOT EXISTS service_label character varying(255),
    ADD COLUMN IF NOT EXISTS service_confidence double precision;
```

- [ ] **Step 2: Terapkan ke DB dev dan verifikasi**

Run:

```bash
psql "postgres://plane:plane@localhost:5432/plane" -f apps/api-rs/migrations/0014_intake_service_gate.sql
psql "postgres://plane:plane@localhost:5432/plane" -c "SELECT column_name FROM information_schema.columns WHERE table_name IN ('issue_types','intake_triage_suggestions') AND column_name IN ('requires_service','service_id','service_label','service_confidence') ORDER BY column_name;"
```

Expected: `ALTER TABLE` dua kali, lalu 4 baris kolom (`requires_service`, `service_confidence`, `service_id`, `service_label`).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/0014_intake_service_gate.sql
git commit -m "feat(api-rs): add intake service gate migration"
```

---

### Task 2: Django model + migrasi 0128

**Files:**

- Modify: `apps/api/plane/db/models/issue_type.py`
- Create: `apps/api/plane/db/migrations/0128_issue_type_requires_service.py`

- [ ] **Step 1: Tambah field model**

Di `apps/api/plane/db/models/issue_type.py`, tambah setelah `is_epic`:

```python
    is_epic = models.BooleanField(default=False)
    requires_service = models.BooleanField(default=False)
    is_default = models.BooleanField(default=False)
```

- [ ] **Step 2: Tulis migrasi**

Isi `apps/api/plane/db/migrations/0128_issue_type_requires_service.py`:

```python
# Generated by Django 5.2.15 on 2026-10-04

from django.db import migrations, models
from django.db.models import Q


def backfill_requires_service(apps, schema_editor):
    IssueType = apps.get_model("db", "IssueType")
    IssueType.objects.filter(
        Q(name__iexact="Incident") | Q(name__iexact="Request") | Q(name__iexact="Service Request"),
        deleted_at__isnull=True,
        is_epic=False,
    ).update(requires_service=True)


class Migration(migrations.Migration):
    dependencies = [
        ("db", "0127_remove_workflow_schema"),
    ]

    operations = [
        migrations.AddField(
            model_name="issuetype",
            name="requires_service",
            field=models.BooleanField(default=False),
        ),
        migrations.RunPython(backfill_requires_service, migrations.RunPython.noop),
    ]
```

- [ ] **Step 3: Verifikasi sintaks**

Run:

```bash
python3 -m py_compile apps/api/plane/db/models/issue_type.py apps/api/plane/db/migrations/0128_issue_type_requires_service.py
```

Expected: tidak ada output (exit 0). Migrasi diterapkan oleh migrator saat rebuild backend (Task 15).

- [ ] **Step 4: Commit**

```bash
git add apps/api/plane/db/models/issue_type.py apps/api/plane/db/migrations/0128_issue_type_requires_service.py
git commit -m "feat(api): add requires_service flag to issue types"
```

---

### Task 3: Expose flag di API type (Rust)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`
- Test: `apps/api-rs/crates/api/tests/v1_work_item_type_test.rs`

- [ ] **Step 1: Update test dulu (failing)**

Di `tests/v1_work_item_type_test.rs`:

1. Tambah `requires_service: false,` setelah `is_epic: false,` pada literal `V1WorkItemTypeRow`.
2. Tambah `"requires_service",` ke daftar key di `work_item_type_json_has_sdk_required_and_optional_keys`.
3. Tambah test baru:

```rust
#[test]
fn create_body_accepts_requires_service() {
    let body: V1CreateWorkItemType =
        serde_json::from_value(serde_json::json!({"name": "Incident", "requires_service": true})).unwrap();
    assert_eq!(body.requires_service, Some(true));
}
```

Run: `cargo test -p api --test v1_work_item_type_test` (dari `apps/api-rs`)
Expected: FAIL kompilasi (`requires_service` tidak ada).

- [ ] **Step 2: Implementasi**

Di `routes/v1/work_item_type.rs`:

1. `TYPE_COLS`: sisipkan `t.requires_service,` setelah `t.is_epic,`.
2. `V1WorkItemTypeRow`: tambah `pub requires_service: bool,` setelah `is_epic`.
3. `v1_work_item_type_json`: tambah `"requires_service": row.requires_service,` setelah `is_epic`.
4. `V1CreateWorkItemType` dan `V1UpdateWorkItemType`: tambah `#[serde(default)] pub requires_service: Option<bool>,` setelah `is_epic`.
5. `create_type`: setelah `let is_epic = ...`:

```rust
    let requires_service = body.requires_service.unwrap_or(false);
    if is_epic && requires_service {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "epic types cannot require a service"})),
        ));
    }
```

Ubah INSERT menjadi:

```rust
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, requires_service, \
         is_default, is_active, level, external_id, external_source, workspace_id, created_by_id, \
         updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, $6, $7, $8, $9, $10, $11, $11, now(), now()) \
         RETURNING id, name, description, logo_props, is_epic, requires_service, is_default, is_active, \
         level::int AS level, external_id, external_source, \
         created_by_id AS created_by, updated_by_id AS updated_by, workspace_id AS workspace, \
         created_at, updated_at, deleted_at, ARRAY[]::uuid[] AS project_ids",
```

Bind berurutan: `name`, `description`, `logo_props`, `is_epic`, `requires_service`, `is_active`, `level`, `external_id`, `external_source`, `ws`, `user` (sebelumnya `is_active` di $5; sekarang bergeser).

6. `update_type`: setelah `in_scope` check, tambah guard epic:

```rust
    let current: Option<(bool, bool)> = sqlx::query_as(
        "SELECT is_epic, requires_service FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some((current_epic, current_requires)) = current else {
        return Ok(missing());
    };
    if body.is_epic.unwrap_or(current_epic) && body.requires_service.unwrap_or(current_requires) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "epic types cannot require a service"})),
        ));
    }
```

7. UPDATE `update_type`: ganti baris `updated_by_id = $10, updated_at = now() \` dengan:

```sql
         requires_service = COALESCE($10, requires_service), \
         updated_by_id = $11, updated_at = now() \
```

dan bind `body.requires_service` tepat sebelum `.bind(user)` (urutan bind: …, `external_source` ($9), `requires_service` ($10), `user` ($11)).

- [ ] **Step 3: Jalankan test**

Run: `cargo test -p api --test v1_work_item_type_test` (dari `apps/api-rs`)
Expected: PASS (3 test).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/tests/v1_work_item_type_test.rs
git commit -m "feat(api-rs): expose requires_service on work item types"
```

---

### Task 4: Seed Incident dengan `requires_service`

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs:296-370`

- [ ] **Step 1: Ubah signature + INSERT**

Ubah `seed_issue_type_id` menerima `requires_service: bool`; INSERT tambah kolom:

```rust
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, requires_service, workspace_id, external_source, external_id, created_by_id, updated_by_id, \
         created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '', '{}', false, false, true, 0, $2, $3, $4, $5, $6, $6, now(), now()) \
         RETURNING id",
    )
    .bind(type_name)
    .bind(requires_service)
    .bind(workspace_id)
    .bind(SEED_EXTERNAL_SOURCE)
    .bind(&external_id)
    .bind(bot_id)
    .fetch_one(&mut **tx)
    .await?;
```

- [ ] **Step 2: Pass flag di loop**

```rust
    for type_name in TYPE_SEEDS {
        seed_issue_type_id(tx, workspace_id, bot_id, type_name, *type_name == "Incident").await?;
    }
```

- [ ] **Step 3: Verifikasi kompilasi**

Run: `cargo check -p api` (dari `apps/api-rs`)
Expected: sukses tanpa warning baru soal signature.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/seed.rs
git commit -m "feat(api-rs): seed Incident with requires_service"
```

---

### Task 5: Accept gate (type wajib, service kondisional)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (helper baru + jalur accept ~1588)
- Test: `apps/api-rs/crates/api/tests/intake_triage_test.rs`

- [ ] **Step 1: Tambah fixture service/default state di Scratch**

Di `intake_triage_test.rs`, tambah method di `impl Scratch`:

```rust
    async fn add_default_state(&self, pool: &PgPool) {
        sqlx::query(
            "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \
             \"default\", project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Backlog', '', 'backlog', 'backlog', '#4E5355', 1000, false, \
             true, $1, $2, now(), now())",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch default state");
    }

    async fn add_type_requiring_service(&self, pool: &PgPool, name: &str) -> Uuid {
        let type_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
             is_active, level, requires_service, workspace_id, created_at, updated_at) \
             VALUES ($1, $2, 'Something is broken', '{}'::jsonb, false, false, true, 0, true, $3, \
             now(), now())",
        )
        .bind(type_id)
        .bind(name)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch type");
        sqlx::query(
            "INSERT INTO project_issue_types (id, created_at, updated_at, project_id, \
             workspace_id, issue_type_id, level, is_default) \
             VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, 0, false)",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .bind(type_id)
        .execute(pool)
        .await
        .expect("scratch project type");
        type_id
    }

    async fn add_service(&self, pool: &PgPool, name: &str) -> Uuid {
        let service_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO services (id, workspace_id, project_id, name, description, description_html, \
             status, criticality, \"type\", created_at, updated_at) \
             VALUES ($1, $2, $3, $4, '', '', 'active', 'high', 'internal', now(), now())",
        )
        .bind(service_id)
        .bind(self.workspace_id)
        .bind(self.project_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("scratch service");
        service_id
    }

    async fn link_service(&self, pool: &PgPool, service_id: Uuid, issue_id: Uuid) {
        sqlx::query(
            "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
             created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
        )
        .bind(self.workspace_id)
        .bind(self.project_id)
        .bind(service_id)
        .bind(issue_id)
        .execute(pool)
        .await
        .expect("scratch service link");
    }
```

Tambahkan juga di `purge` (sebelum DELETE states):

```rust
            "DELETE FROM service_issues WHERE project_id = $1",
            "DELETE FROM services WHERE project_id = $1",
```

- [ ] **Step 2: Tulis test gate (failing)**

Tambah import di `intake_triage_test.rs`:

```rust
use api::routes::intake::{patch_issue, InboxIssueFields, InboxIssuePatch};
```

Tambah test:

```rust
#[tokio::test]
async fn accept_gate_requires_type_then_service() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    scratch.add_default_state(&pool).await;
    let incident_type = scratch.add_type_requiring_service(&pool, "Incident").await;
    let service_id = scratch.add_service(&pool, "Payment Gateway").await;
    let st = state(&pool).await;

    let (_, issue_id) = scratch.create_item(&st, "Checkout down").await;

    let accept = || {
        patch_issue(
            State(st.clone()),
            AuthUser(scratch.user_id),
            Path((scratch.slug.clone(), scratch.project_id, issue_id)),
            Json(InboxIssuePatch {
                status: Some(1),
                ..Default::default()
            }),
        )
    };

    // 1. Type kosong, project punya tipe → 400.
    let (status, Json(body)) = accept().await.expect("accept");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("work item type"));

    // 2. Type di-set lewat PATCH intake → state tetap triage.
    let (status, _) = patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(InboxIssuePatch {
            issue: Some(InboxIssueFields {
                type_id: Some(incident_type),
                ..Default::default()
            }),
            ..Default::default()
        }),
    )
    .await
    .expect("patch type");
    assert_eq!(status, StatusCode::OK);
    let state_group: String = sqlx::query_scalar(
        "SELECT s.\"group\" FROM issues i JOIN states s ON s.id = i.state_id WHERE i.id = $1",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_group, "triage");

    // 3. Type requires_service tanpa link → 400.
    let (status, Json(body)) = accept().await.expect("accept");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("service"));

    // 4. Link service → accept lolos dan keluar dari triage.
    scratch.link_service(&pool, service_id, issue_id).await;
    let (status, _) = accept().await.expect("accept");
    assert_eq!(status, StatusCode::OK);
    let state_group: String = sqlx::query_scalar(
        "SELECT s.\"group\" FROM issues i JOIN states s ON s.id = i.state_id WHERE i.id = $1",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(state_group, "triage");

    // 5. Tipe tanpa flag → accept tanpa service.
    let (_, issue_id_2) = scratch.create_item(&st, "Printer queue").await;
    let problem_type = scratch.add_type(&pool, "Problem").await;
    patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id_2)),
        Json(InboxIssuePatch {
            issue: Some(InboxIssueFields {
                type_id: Some(problem_type),
                ..Default::default()
            }),
            ..Default::default()
        }),
    )
    .await
    .expect("patch type");
    let problem_accept = patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id_2)),
        Json(InboxIssuePatch {
            status: Some(1),
            ..Default::default()
        }),
    )
    .await
    .expect("accept problem");
    assert_eq!(problem_accept.0, StatusCode::OK);

    scratch.purge(&pool).await;
}
```

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test accept_gate_requires_type_then_service -- --test-threads=1` (dari `apps/api-rs`)
Expected: FAIL — step 1 mengembalikan 200 (atau error "No default state" bila fixture belum lolos), bukan 400 type.

- [ ] **Step 3: Implementasi helper + wiring**

Di `routes/intake.rs`, tambah helper sebelum `pub async fn patch_issue`:

```rust
/// Accept gate ITSM: type wajib bila project punya tipe live non-epic;
/// service wajib bila tipe terpilih ber-flag `requires_service`.
async fn accept_gate(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
) -> Result<Option<&'static str>, sqlx::Error> {
    let issue: Option<(Option<uuid::Uuid>, Option<bool>)> = sqlx::query_as(
        "SELECT i.type_id, t.requires_service FROM issues i \
         LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
         WHERE i.id = $1 AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await?;
    let Some((type_id, requires_service)) = issue else {
        return Ok(None);
    };
    if type_id.is_none() {
        let (has_types,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM project_issue_types pit \
             JOIN issue_types t ON t.id = pit.issue_type_id \
             WHERE pit.project_id = $1 AND pit.deleted_at IS NULL \
             AND t.deleted_at IS NULL AND t.is_epic = false)",
        )
        .bind(project_id)
        .fetch_one(pool)
        .await?;
        if has_types {
            return Ok(Some("Select a work item type before accepting"));
        }
        return Ok(None);
    }
    if requires_service == Some(true) {
        let (has_service,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM service_issues \
             WHERE issue_id = $1 AND deleted_at IS NULL)",
        )
        .bind(issue_id)
        .fetch_one(pool)
        .await?;
        if !has_service {
            return Ok(Some("Select a service before accepting this work item"));
        }
    }
    Ok(None)
}
```

Di `patch_issue`, di dalam `if may_write_intake {`, tepat setelah `let n_status = body.status;` tambah:

```rust
        if n_status == Some(1) {
            if let Some(message) = accept_gate(&st.pool, project_id, issue_id).await? {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
            }
        }
```

- [ ] **Step 4: Jalankan test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test accept_gate_requires_type_then_service -- --test-threads=1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/tests/intake_triage_test.rs
git commit -m "feat(api-rs): gate intake accept on type and required service"
```

---

### Task 6: Set type manual lewat PATCH intake

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (`InboxIssueFields` ~1219, validasi ~1390, write ~1470)
- Test: `apps/api-rs/crates/api/tests/intake_triage_test.rs`

- [ ] **Step 1: Tambah test (failing)**

Test di Task 5 sudah memakai `InboxIssueFields.type_id`; tanpa implementasi, PATCH mengabaikan type (test gagal di assertion `type_id`/state). Tambah assertion eksplisit di test Task 5 setelah step 2:

```rust
    let stored_type: Option<Uuid> = sqlx::query_scalar("SELECT type_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_type, Some(incident_type));
```

- [ ] **Step 2: Implementasi**

1. `InboxIssueFields` tambah:

```rust
    #[serde(default)]
    pub type_id: Option<uuid::Uuid>,
```

2. Tambah `let new_type` di level yang sama dengan `new_priority` (bukan di dalam blok `if !narrowed`), tepat setelah `let new_priority = ...`:

```rust
    let new_type = issue.and_then(|i| i.type_id);
```

Lalu validasi di dalam blok `if !narrowed {` setelah validasi priority (pakai `new_type` dari scope luar):

```rust
        if let Some(type_id) = new_type {
            let (type_ok,): (bool,) = sqlx::query_as(
                "SELECT EXISTS(SELECT 1 FROM issue_types t \
                 JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.deleted_at IS NULL \
                 WHERE t.id = $1 AND pit.project_id = $2 AND t.deleted_at IS NULL AND t.is_epic = false)",
            )
            .bind(type_id)
            .bind(project_id)
            .fetch_one(&st.pool)
            .await?;
            if !type_ok {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Invalid work item type"})),
                ));
            }
        }
```

3. Non-narrowed UPDATE: tambah `type_id = COALESCE($5, type_id),` dan geser bind berikutnya:

```rust
        if new_name.is_some()
            || desc_html.is_some()
            || desc_json.is_some()
            || new_priority.is_some()
            || new_type.is_some()
        {
            sqlx::query(
                "UPDATE issues SET name = COALESCE($1, name), \
                  description_html = COALESCE($2, description_html), \
                  description_json = COALESCE($3::jsonb, description_json), \
                  priority = COALESCE($4, priority), \
                  type_id = COALESCE($5, type_id), \
                  updated_at = now(), updated_by_id = $7, \
                  description_stripped = CASE WHEN $8::boolean THEN $9::text ELSE description_stripped END \
                  WHERE id = $6 AND deleted_at IS NULL",
            )
            .bind(&new_name)
            .bind(&desc_html)
            .bind(&desc_json)
            .bind(&new_priority)
            .bind(new_type)
            .bind(issue_id)
            .bind(user_id)
            .bind(desc_stripped_flag)
            .bind(&desc_stripped)
            .execute(&mut *tx)
            .await?;
        }
```

- [ ] **Step 3: Jalankan test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test accept_gate_requires_type_then_service -- --test-threads=1`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/tests/intake_triage_test.rs
git commit -m "feat(api-rs): allow setting work item type from intake"
```

---

### Task 7: Apply category/service + response `service`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (konstanta ~1789, `TriageRow` ~1799, `triage_json` ~1853, `apply_triage_suggestion` ~1967)

- [ ] **Step 1: Update konstanta + row + json**

```rust
const TRIAGE_FIELDS: [&str; 4] = ["category", "service", "severity", "needs_human"];
const TRIAGE_APPLY_FIELDS: [&str; 3] = ["category", "service", "severity"];
```

`TriageRow` tambah:

```rust
    service_id: Option<uuid::Uuid>,
    service_label: Option<String>,
    service_confidence: Option<f64>,
```

`fetch_triage_row` SELECT tambah `service_id, service_label, service_confidence,` setelah `severity_confidence,`.

`triage_json` tambah objek `service` setelah `category`:

```rust
    let service = if ready {
        row.service_label.as_ref().map(|label| {
            json!({
                "id": row.service_id,
                "label": label,
                "confidence": row.service_confidence,
                "probabilities": answers
                    .get("service")
                    .map(|value| probability_json(value, None))
                    .unwrap_or_else(|| json!({})),
            })
        })
    } else {
        None
    };
```

dan masukkan `"service": service,` ke json output antara `category` dan `severity`.

- [ ] **Step 2: Ganti blok apply**

Ganti isi `apply_triage_suggestion` setelah validasi dismissed dengan:

```rust
    if body
        .fields
        .iter()
        .any(|field| !TRIAGE_APPLY_FIELDS.contains(&field.as_str()))
    {
        return Ok(triage_bad_request(
            "Only category, service and severity can be applied",
        ));
    }
    let apply_category = body.fields.iter().any(|f| f == "category")
        && !row.applied_fields.iter().any(|f| f == "category");
    let apply_service = body.fields.iter().any(|f| f == "service")
        && !row.applied_fields.iter().any(|f| f == "service");
    let apply_severity = body.fields.iter().any(|f| f == "severity")
        && !row.applied_fields.iter().any(|f| f == "severity");

    if apply_category {
        let type_ok: bool = match row.category_type_id {
            Some(type_id) => sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM issue_types t \
                 JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.deleted_at IS NULL \
                 WHERE t.id = $1 AND pit.project_id = $2 AND t.deleted_at IS NULL AND t.is_epic = false)",
            )
            .bind(type_id)
            .bind(project_id)
            .fetch_one(&st.pool)
            .await?,
            None => false,
        };
        if !type_ok {
            return Ok(triage_bad_request("Category suggestion is no longer available"));
        }
    }
    if apply_service {
        let service_ok: bool = match row.service_id {
            Some(service_id) => sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM services WHERE id = $1 AND project_id = $2 \
                 AND deleted_at IS NULL AND status <> 'retired')",
            )
            .bind(service_id)
            .bind(project_id)
            .fetch_one(&st.pool)
            .await?,
            None => false,
        };
        if !service_ok {
            return Ok(triage_bad_request("No service to apply"));
        }
    }

    if apply_category || apply_service || apply_severity {
        let mut tx = st.pool.begin().await?;
        let mut applied = row.applied_fields.clone();
        if apply_category {
            if let Some(type_id) = row.category_type_id {
                sqlx::query(
                    "UPDATE issues SET type_id = $1, updated_at = now(), updated_by_id = $2 \
                     WHERE id = $3 AND deleted_at IS NULL",
                )
                .bind(type_id)
                .bind(auth.0)
                .bind(scope.issue_id)
                .execute(&mut *tx)
                .await?;
                applied.push("category".to_string());
            }
        }
        if apply_service {
            if let Some(service_id) = row.service_id {
                sqlx::query(
                    "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
                     created_by_id, updated_by_id, created_at, updated_at) \
                     SELECT gen_random_uuid(), ii.workspace_id, ii.project_id, $1, ii.issue_id, $2, $2, \
                     now(), now() FROM intake_issues ii WHERE ii.id = $3 \
                     AND NOT EXISTS(SELECT 1 FROM service_issues si \
                       WHERE si.service_id = $1 AND si.issue_id = ii.issue_id AND si.deleted_at IS NULL)",
                )
                .bind(service_id)
                .bind(auth.0)
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
                applied.push("service".to_string());
            }
        }
        if apply_severity {
            if let Some(priority) = &row.severity_priority {
                sqlx::query(
                    "UPDATE issues SET priority = $1, updated_at = now(), updated_by_id = $2 \
                     WHERE id = $3 AND deleted_at IS NULL",
                )
                .bind(priority)
                .bind(auth.0)
                .bind(scope.issue_id)
                .execute(&mut *tx)
                .await?;
                applied.push("severity".to_string());
            }
        }
        sqlx::query(
            "UPDATE intake_triage_suggestions SET applied_fields = $2, updated_at = now() WHERE id = $1",
        )
        .bind(row.id)
        .bind(&applied)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    let refreshed = fetch_triage_row(&st.pool, scope.row_id).await?.unwrap_or(row);
    Ok((StatusCode::OK, Json(json!({"data": triage_json(&refreshed)}))))
```

Hapus blok severity lama yang digantikan (dari `if let Some(priority) = &row.severity_priority` sampai `tx.commit().await?;}`).

- [ ] **Step 3: Test unit (mode `cargo test` tidak butuh DB) + integrasi**

Run: `cargo test -p api --test intake_triage_routes_test` (tidak berubah; memverifikasi URL masih cocok)
Expected: PASS.

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1`
Expected: PASS (apply severity existing masih lolos).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs
git commit -m "feat(api-rs): apply category and service from triage suggestion"
```

---

### Task 8: Jev question `service` + mapping

**Files:**

- Modify: `apps/api-rs/crates/ai/src/triage.rs`
- Modify: `apps/api-rs/crates/ai/src/triage_job.rs`
- Test: unit test di `triage.rs`; integrasi ditambah di Task 8 Step 5.

- [ ] **Step 1: Update unit test (failing)**

Di `apps/api-rs/crates/ai/src/triage.rs` `mod tests`, update semua pemanggilan: `build_questions(&[])` → `build_questions(&[], &[])`, `build_questions(&types)` → `build_questions(&types, &[])`, `triage_outcome(outcome, &[incident.clone()])` → `triage_outcome(outcome, &[incident.clone()], &[])` (3 call site `build_questions` + 1 `triage_outcome`). Tambah test:

```rust
    fn service_option(name: &str) -> ServiceOption {
        ServiceOption {
            service_id: Uuid::new_v4(),
            name: name.to_string(),
            criteria: format!("{name} — internal, criticality high, status active"),
        }
    }

    #[test]
    fn questions_include_service_between_category_and_severity() {
        let types = vec![type_option("Incident")];
        let services = vec![service_option("Payment Gateway")];
        let questions = build_questions(&types, &services);
        let ids: Vec<&str> = questions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["category", "service", "severity", "needs_human"]);
        match &questions[1].1 {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria[0].0, "Payment Gateway");
                assert_eq!(criteria[1].0, NO_SERVICE_CHOICE);
            }
            other => panic!("expected choice, got {other:?}"),
        }
        assert_eq!(build_questions(&types, &[]).len(), 3);
    }

    #[test]
    fn outcome_maps_service_choice_and_abstain() {
        let service = service_option("Payment Gateway");
        let mut answers = BTreeMap::new();
        answers.insert(
            "service".to_string(),
            Answer::Choice {
                choice: "Payment Gateway".to_string(),
                confidence: 0.74,
                probabilities: BTreeMap::new(),
            },
        );
        let outcome = DecisionOutcome {
            model: "jev".to_string(),
            answers,
            input_tokens: 1,
            output_tokens: 1,
        };
        let mapped = triage_outcome(outcome, &[], &[service.clone()]);
        assert_eq!(mapped.service_id, Some(service.service_id));
        assert_eq!(mapped.service_label.as_deref(), Some("Payment Gateway"));
        assert_eq!(mapped.service_confidence, Some(0.74));

        let mut abstain = BTreeMap::new();
        abstain.insert(
            "service".to_string(),
            Answer::Choice {
                choice: NO_SERVICE_CHOICE.to_string(),
                confidence: 0.6,
                probabilities: BTreeMap::new(),
            },
        );
        let outcome = DecisionOutcome {
            model: "jev".to_string(),
            answers: abstain,
            input_tokens: 1,
            output_tokens: 1,
        };
        let mapped = triage_outcome(outcome, &[], &[service]);
        assert_eq!(mapped.service_id, None);
        assert_eq!(mapped.service_label.as_deref(), Some(NO_SERVICE_SENTINEL));
    }
```

Run: `cargo test -p ai triage` (dari `apps/api-rs`)
Expected: FAIL kompilasi (signature + tipe baru belum ada).

- [ ] **Step 2: Implementasi `triage.rs`**

Tambah konstanta + tipe:

```rust
pub const MAX_SERVICES: usize = 20;
pub const NO_SERVICE_CHOICE: &str = "No service / unsure";
pub const NO_SERVICE_SENTINEL: &str = "__none__";

#[derive(Clone)]
pub struct ServiceOption {
    pub service_id: Uuid,
    pub name: String,
    pub criteria: String,
}
```

`build_questions(types: &[TypeOption], services: &[ServiceOption])`: sisipkan blok berikut setelah blok `category` dan sebelum `severity`:

```rust
    if !services.is_empty() {
        let mut criteria: Vec<(String, String)> = services
            .iter()
            .take(MAX_SERVICES)
            .map(|s| (s.name.clone(), s.criteria.clone()))
            .collect();
        criteria.push((
            NO_SERVICE_CHOICE.to_string(),
            "This request does not concern a specific service or the service is unknown".to_string(),
        ));
        questions.push((
            "service".to_string(),
            Question::Choice {
                instructions: "Which service does this intake item concern?".to_string(),
                criteria,
            },
        ));
    }
```

`TriageOutcome` tambah:

```rust
    pub service_id: Option<Uuid>,
    pub service_label: Option<String>,
    pub service_confidence: Option<f64>,
```

`triage_outcome(outcome, types, services)`: tambah var `let mut service: Option<(Option<Uuid>, String, f64)> = None;`, arm:

```rust
            ("service", Answer::Choice { choice, confidence, .. }) => {
                if choice == NO_SERVICE_CHOICE {
                    service = Some((None, NO_SERVICE_SENTINEL.to_string(), *confidence));
                } else {
                    let service_id = services.iter().find(|s| &s.name == choice).map(|s| s.service_id);
                    service = Some((service_id, choice.clone(), *confidence));
                }
            }
```

dan isi field baru di struct return:

```rust
        service_id: service.as_ref().and_then(|(id, _, _)| *id),
        service_label: service.as_ref().map(|(_, label, _)| label.clone()),
        service_confidence: service.as_ref().map(|(_, _, confidence)| *confidence),
```

- [ ] **Step 3: Implementasi `triage_job.rs`**

Tambah loader:

```rust
async fn load_services(pool: &PgPool, project_id: Uuid) -> Result<Vec<triage::ServiceOption>, sqlx::Error> {
    let rows: Vec<(Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, \"type\", criticality, status FROM services \
         WHERE project_id = $1 AND deleted_at IS NULL AND status <> 'retired' \
         ORDER BY name LIMIT $2",
    )
    .bind(project_id)
    .bind(triage::MAX_SERVICES as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(service_id, name, service_type, criticality, status)| triage::ServiceOption {
            criteria: format!("{name} — {service_type}, criticality {criticality}, status {status}"),
            service_id,
            name,
        })
        .collect())
}
```

Di `classify`: setelah `let types = load_types(...)`, tambah `let services = load_services(pool, item.project_id).await?;`; ubah `build_questions(&types)` → `build_questions(&types, &services)`; `triage_outcome(outcome, &types)` → `triage_outcome(outcome, &types, &services)`.

`save_ready` UPDATE tambah:

```sql
            service_id = $13, service_label = $14, service_confidence = $15, \
```

dan tambah bind setelah `.bind(mapped.output_tokens as i32)` (urutan posisi: `needs_human` $10, `input_tokens` $11, `output_tokens` $12, lalu service $13/$14/$15):

```rust
    .bind(mapped.service_id)
    .bind(&mapped.service_label)
    .bind(mapped.service_confidence)
```

- [ ] **Step 4: Jalankan unit test**

Run: `cargo test -p ai triage` (dari `apps/api-rs`)
Expected: PASS (semua test `triage` + `triage_job` compile).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/triage.rs apps/api-rs/crates/ai/src/triage_job.rs
git commit -m "feat(ai): ask Jev for a service suggestion"
```

---

### Task 9: Integration test saran service + apply category/service

**Files:**

- Modify: `apps/api-rs/crates/api/tests/intake_triage_test.rs`

- [ ] **Step 1: Tambah test end-to-end (failing sampai Task 7+8 selesai)**

```rust
#[tokio::test]
async fn suggestion_includes_service_and_apply_writes_type_and_link() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let incident_type = scratch.add_type_requiring_service(&pool, "Incident").await;
    let service_id = scratch.add_service(&pool, "Payment Gateway").await;
    let st = state(&pool).await;

    let (base_url, bodies) = support::spawn_systemone_upstream(json!({
        "model": "jev-1.13.0",
        "answers": {
            "category": {"type": "choice", "choice": "Incident", "confidence": 0.9,
                         "probabilities": {"Incident": 0.9}},
            "service": {"type": "choice", "choice": "Payment Gateway", "confidence": 0.74,
                        "probabilities": {"Payment Gateway": 0.74}},
            "severity": {"type": "score", "score": 3.0, "confidence": 0.8,
                         "probabilities": {"3": 0.8}},
            "needs_human": {"type": "noul", "noul": 0.4}
        },
        "usage": {"input_tokens": 100, "output_tokens": 10}
    }))
    .await;
    set_decision_env(&base_url);

    let (row_id, issue_id) = scratch.create_item(&st, "Payments failing").await;
    ai::triage_job::classify(&pool, row_id).await.expect("classify");

    let sent = bodies.lock().unwrap().clone();
    assert!(sent[0]["questions"]["service"]["criteria"]
        .get("Payment Gateway")
        .is_some());

    let (status, Json(body)) = get_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .expect("get suggestion");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["service"]["label"], "Payment Gateway");
    assert_eq!(body["data"]["service"]["confidence"], 0.74);

    let (status, Json(applied)) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["category".to_string(), "service".to_string()],
        }),
    )
    .await
    .expect("apply");
    assert_eq!(status, StatusCode::OK);
    for field in ["category", "service"] {
        assert!(applied["data"]["applied_fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == field));
    }
    let stored_type: Option<Uuid> = sqlx::query_scalar("SELECT type_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_type, Some(incident_type));
    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_issues WHERE issue_id = $1 AND service_id = $2 \
         AND deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(service_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked);

    clear_decision_env();
    scratch.purge(&pool).await;
}
```

Dan test sentinel:

```rust
#[tokio::test]
async fn service_abstain_stores_sentinel_and_rejects_apply() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    scratch.add_service(&pool, "Payment Gateway").await;
    let st = state(&pool).await;

    let (base_url, _) = support::spawn_systemone_upstream(json!({
        "model": "jev-1.13.0",
        "answers": {
            "service": {"type": "choice", "choice": "No service / unsure", "confidence": 0.6,
                        "probabilities": {}},
            "severity": {"type": "score", "score": 1.0, "confidence": 0.5,
                         "probabilities": {"1": 0.5}},
            "needs_human": {"type": "noul", "noul": 0.2}
        },
        "usage": {"input_tokens": 10, "output_tokens": 2}
    }))
    .await;
    set_decision_env(&base_url);

    let (row_id, issue_id) = scratch.create_item(&st, "General question").await;
    ai::triage_job::classify(&pool, row_id).await.expect("classify");

    let (status, Json(body)) = get_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .expect("get suggestion");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["service"]["label"], "__none__");
    assert!(body["data"]["service"]["id"].is_null());

    let (status, _) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["service".to_string()],
        }),
    )
    .await
    .expect("apply sentinel");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    clear_decision_env();
    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1`
Expected: semua PASS (4 test).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/intake_triage_test.rs
git commit -m "test(api-rs): cover service suggestion end to end"
```

---

### Task 10: Tipe FE + payload

**Files:**

- Modify: `packages/types/src/inbox.ts:101-125`
- Modify: `packages/types/src/work-item-type.ts`

- [ ] **Step 1: Update types**

`packages/types/src/inbox.ts`:

```ts
export type TInboxIssueTriageField = "category" | "service" | "severity" | "needs_human";
```

dan tambah ke `TInboxIssueTriageSuggestion` setelah `category`:

```ts
  service: {
    id: string | null;
    label: string;
    confidence: number;
    probabilities: Record<string, number>;
  } | null;
```

`packages/types/src/work-item-type.ts`:

```ts
export type TWorkItemType = {
  // ... field existing
  is_epic: boolean;
  requires_service: boolean;
  is_default: boolean;
  // ...
};

export type TWorkItemTypePayload = {
  name?: string;
  description?: string;
  is_active?: boolean;
  requires_service?: boolean;
  project_ids?: string[];
};
```

- [ ] **Step 2: Verifikasi tipe**

Run: `pnpm --filter=web check:types`
Expected: PASS (atau error hanya di file yang belum diupdate di task berikutnya; kalau error muncul di tempat lain, perbaiki di task terkait).

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/inbox.ts packages/types/src/work-item-type.ts
git commit -m "feat(types): add service suggestion and requires_service types"
```

---

### Task 11: Helper gate + unit test

**Files:**

- Create: `apps/web/core/components/inbox/accept-gate.ts`
- Create: `apps/web/core/components/inbox/accept-gate.test.ts`

- [ ] **Step 1: Tulis test dulu (failing)**

```ts
import { describe, expect, it } from "vitest";
import { getIntakeAcceptGate } from "./accept-gate";

describe("getIntakeAcceptGate", () => {
  it("requires a type when the project has types", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: null,
      typeRequiresService: false,
      hasLinkedService: false,
    });
    expect(gate).toEqual({ missingType: true, missingService: false, ready: false });
  });

  it("requires a service when the type needs one", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: "type-1",
      typeRequiresService: true,
      hasLinkedService: false,
    });
    expect(gate).toEqual({ missingType: false, missingService: true, ready: false });
  });

  it("is ready when the required service is linked", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: "type-1",
      typeRequiresService: true,
      hasLinkedService: true,
    });
    expect(gate.ready).toBe(true);
  });

  it("does not require a service for a non-flagged type", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: true,
      issueTypeId: "type-1",
      typeRequiresService: false,
      hasLinkedService: false,
    });
    expect(gate.ready).toBe(true);
  });

  it("turns the type gate off when the project has no types", () => {
    const gate = getIntakeAcceptGate({
      projectHasTypes: false,
      issueTypeId: null,
      typeRequiresService: false,
      hasLinkedService: false,
    });
    expect(gate.ready).toBe(true);
  });
});
```

Run: `pnpm --filter=web test -- core/components/inbox/accept-gate.test.ts`
Expected: FAIL (module tidak ditemukan).

- [ ] **Step 2: Implementasi helper**

```ts
export type TIntakeAcceptGate = {
  missingType: boolean;
  missingService: boolean;
  ready: boolean;
};

type TIntakeAcceptGateInput = {
  /** Project punya minimal satu tipe non-epic aktif. */
  projectHasTypes: boolean;
  /** Tipe work item item intake (nullable). */
  issueTypeId: string | null | undefined;
  /** `requires_service` tipe terpilih. */
  typeRequiresService: boolean;
  /** Item sudah punya minimal satu service link live. */
  hasLinkedService: boolean;
};

export const getIntakeAcceptGate = (input: TIntakeAcceptGateInput): TIntakeAcceptGate => {
  const missingType = input.projectHasTypes && !input.issueTypeId;
  const missingService =
    !missingType && Boolean(input.issueTypeId) && input.typeRequiresService && !input.hasLinkedService;
  return { missingType, missingService, ready: !missingType && !missingService };
};
```

- [ ] **Step 3: Jalankan test**

Run: `pnpm --filter=web test -- core/components/inbox/accept-gate.test.ts`
Expected: PASS (5 test).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/inbox/accept-gate.ts apps/web/core/components/inbox/accept-gate.test.ts
git commit -m "feat(web): add intake accept gate helper"
```

---

### Task 12: Gate tombol Accept di header

**Files:**

- Modify: `apps/web/core/components/inbox/content/inbox-issue-header.tsx`
- Modify: `packages/i18n/src/locales/en/inbox.json`

- [ ] **Step 1: Tambah i18n**

Di `inbox.json`, di dalam `inbox_issue` tambah objek `gate`:

```json
    "gate": {
      "type_required": "Select a work item type before accepting",
      "service_required": "Select a service before accepting this work item"
    },
```

- [ ] **Step 2: Implementasi gate**

Import yang bertambah:

```tsx
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
import { useService } from "@/hooks/store/use-service";
import { getIntakeAcceptGate } from "@/components/inbox/accept-gate";
```

Di dalam komponen setelah hook store existing:

```tsx
const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();
const { workItemLinkMap } = useService();

useEffect(() => {
  if (!workItemTypes) void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
}, [workItemTypes, workspaceSlug, fetchWorkItemTypes]);

const projectTypes = (workItemTypes ?? []).filter(
  (type) => !type.is_epic && type.is_active && type.project_ids.includes(projectId)
);
const selectedType = projectTypes.find((type) => type.id === issue?.type_id);
const hasLinkedService = Object.values(workItemLinkMap).some(
  (link) => link.issue_id === issue?.id && link.project_id === projectId
);
const acceptGate = getIntakeAcceptGate({
  projectHasTypes: projectTypes.length > 0,
  issueTypeId: issue?.type_id,
  typeRequiresService: selectedType?.requires_service ?? false,
  hasLinkedService,
});
const acceptGateHint = acceptGate.missingType
  ? t("inbox_issue.gate.type_required")
  : acceptGate.missingService
    ? t("inbox_issue.gate.service_required")
    : undefined;
```

Ganti tombol Accept agar disabled + hint lewat atribut `title` (tanpa komponen Tooltip baru):

```tsx
{
  canMarkAsAccepted && (
    <Button
      variant="secondary"
      size="lg"
      disabled={!acceptGate.ready}
      title={acceptGateHint}
      onClick={() =>
        handleActionWithPermission(
          isProjectAdmin,
          () => setAcceptIssueModal(true),
          t("inbox_issue.errors.accept_permission")
        )
      }
    >
      <TickCircleFilled className="size-4 shrink-0 text-success-secondary" />
      {t("inbox_issue.actions.accept")}
    </Button>
  );
}
```

- [ ] **Step 3: Verifikasi tipe**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/inbox/content/inbox-issue-header.tsx packages/i18n/src/locales/en/inbox.json
git commit -m "feat(web): gate intake accept button"
```

---

### Task 13: Baris Type + Service di detail intake

**Files:**

- Modify: `apps/web/core/components/services/select/service-select.tsx`
- Modify: `apps/web/core/components/inbox/content/issue-properties.tsx`

- [ ] **Step 1: Override issue di ServiceSelect**

Tambah prop opsional di `TServiceSelect`:

```ts
type TServiceSelect = {
  className?: string;
  workspaceSlug: string;
  projectId: string;
  issueId: string;
  disabled?: boolean;
  issueData?: Partial<TIssue>;
};
```

Destructure `issueData` dan ubah derived issue:

```ts
const issue = issueData ?? getIssueById(issueId);
```

- [ ] **Step 2: Tambah baris Type + Service**

Di `issue-properties.tsx` import:

```tsx
import { IntakeOutline } from "@makeplane/propel/icons";
import { WorkItemTypeDropdown } from "@/components/dropdowns/work-item-type/dropdown";
import { ServiceSelect } from "@/components/services/select/service-select";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
```

Di komponen: `const { workItemTypes } = useWorkItemType();` dan hitung tipe terpilih:

```tsx
const projectTypes = (workItemTypes ?? []).filter(
  (type) => !type.is_epic && type.is_active && type.project_ids.includes(projectId)
);
const selectedType = projectTypes.find((type) => type.id === issue?.type_id);
```

Sisipkan setelah baris State dan sebelum Assignees:

```tsx
{
  /* Type */
}
<div className="flex h-8 items-center gap-2">
  <div className="flex w-2/5 flex-shrink-0 items-center gap-1 text-13 text-tertiary">
    <IntakeOutline className="h-4 w-4 flex-shrink-0" />
    <span>Type</span>
  </div>
  <WorkItemTypeDropdown
    value={issue?.type_id}
    onChange={(typeId) => issue?.id && issueOperations.update(workspaceSlug, projectId, issue?.id, { type_id: typeId })}
    disabled={!isEditable}
    projectId={projectId?.toString()}
    buttonVariant="border-with-text"
    className="w-3/5 flex-grow rounded-sm px-2 hover:bg-layer-1"
    buttonContainerClassName="w-full text-left"
    buttonClassName="text-13"
  />
</div>;
{
  /* Service */
}
<div className="flex min-h-8 items-center gap-2">
  <div className="flex w-2/5 flex-shrink-0 items-center gap-1 text-13 text-tertiary">
    <div className="h-4 w-4 flex-shrink-0" />
    <span>Service</span>
    {selectedType?.requires_service && (
      <span className="rounded-sm bg-layer-2 px-1 text-10 text-tertiary">Required</span>
    )}
  </div>
  <div className="h-full min-h-8 w-3/5 flex-grow pt-1">
    {issue?.id && (
      <ServiceSelect
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        issueId={issue.id}
        issueData={issue}
        disabled={!isEditable}
      />
    )}
  </div>
</div>;
```

- [ ] **Step 3: Verifikasi tipe**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/select/service-select.tsx apps/web/core/components/inbox/content/issue-properties.tsx
git commit -m "feat(web): add type and service pickers to intake detail"
```

---

### Task 14: Apply category/service di panel saran

**Files:**

- Modify: `apps/web/core/components/inbox/content/triage-suggestion.tsx`
- Modify: `apps/web/core/store/inbox/inbox-issue.store.ts`
- Modify: `packages/i18n/src/locales/en/inbox.json`

- [ ] **Step 1: Store — expose slug/project + update issue setelah apply category**

Di `IInboxIssueStore` tambah:

```ts
workspaceSlug: string;
projectId: string;
```

Di `applyTriageSuggestion`, setelah `set(this, "triageSuggestion", suggestion);` tambah:

```ts
if (suggestion?.category && suggestion.category.type_id && suggestion.applied_fields.includes("category")) {
  set(this.issue, "type_id", suggestion.category.type_id);
}
```

- [ ] **Step 2: i18n**

Di `inbox.json` bagian `inbox_issue.triage` tambah:

```json
      "service": "Service",
      "no_service": "No service suggested",
```

- [ ] **Step 3: Panel**

1. Category row: ganti blok aksi agar punya Apply (pola sama dengan severity):

```tsx
{
  applied.includes("category") ? (
    <span className="text-11 text-tertiary">{t("inbox_issue.triage.applied")}</span>
  ) : dismissed.includes("category") ? (
    <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
  ) : (
    <div className="flex items-center gap-3">
      <button
        type="button"
        className="text-11 text-accent-primary hover:underline"
        onClick={() => void runAction("apply", ["category"])}
      >
        {t("inbox_issue.triage.apply")}
      </button>
      <button
        type="button"
        className="text-11 text-tertiary hover:text-primary"
        onClick={() => void runAction("dismiss", ["category"])}
      >
        {t("inbox_issue.triage.dismiss")}
      </button>
    </div>
  );
}
```

2. Service row, sisipkan setelah category:

```tsx
{
  suggestion.service && (
    <div className="flex items-center justify-between gap-2 py-2">
      <div className="flex flex-col">
        <span className="text-13 text-tertiary">{t("inbox_issue.triage.service")}</span>
        <span className="text-13 text-primary">
          {suggestion.service.label === "__none__" ? t("inbox_issue.triage.no_service") : suggestion.service.label}{" "}
          <span className="text-tertiary">({percent(suggestion.service.confidence)})</span>
        </span>
      </div>
      {applied.includes("service") ? (
        <span className="text-11 text-tertiary">{t("inbox_issue.triage.applied")}</span>
      ) : dismissed.includes("service") ? (
        <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
      ) : suggestion.service.label === "__none__" ? (
        <button
          type="button"
          className="text-11 text-tertiary hover:text-primary"
          onClick={() => void runAction("dismiss", ["service"])}
        >
          {t("inbox_issue.triage.dismiss")}
        </button>
      ) : (
        <div className="flex items-center gap-3">
          <button
            type="button"
            className="text-11 text-accent-primary hover:underline"
            onClick={() => void runAction("apply", ["service"])}
          >
            {t("inbox_issue.triage.apply")}
          </button>
          <button
            type="button"
            className="text-11 text-tertiary hover:text-primary"
            onClick={() => void runAction("dismiss", ["service"])}
          >
            {t("inbox_issue.triage.dismiss")}
          </button>
        </div>
      )}
    </div>
  );
}
```

3. Refresh service store setelah apply service. Tambah hook + ubah `runAction`:

```tsx
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// ...
const { fetchServices } = useService();
const { currentWorkspace, getWorkspaceBySlug } = useWorkspace();
// ...
const runAction = async (action: "apply" | "dismiss", fields: TInboxIssueTriageField[]) => {
  try {
    if (action === "apply") {
      await inboxIssue.applyTriageSuggestion(fields);
      if (fields.includes("service")) {
        const workspaceId = getWorkspaceBySlug(inboxIssue.workspaceSlug)?.id ?? currentWorkspace?.id;
        if (workspaceId) void fetchServices(inboxIssue.workspaceSlug, workspaceId, inboxIssue.projectId);
      }
    } else await inboxIssue.dismissTriageSuggestion(fields);
  } catch {
    // ... toast existing
  }
};
```

- [ ] **Step 4: Verifikasi**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/inbox/content/triage-suggestion.tsx apps/web/core/store/inbox/inbox-issue.store.ts packages/i18n/src/locales/en/inbox.json
git commit -m "feat(web): apply type and service from triage suggestion panel"
```

---

### Task 15: Toggle `requires_service` di settings type

**Files:**

- Modify: `apps/web/core/components/work-item-types/type-form-modal.tsx`
- Modify: `packages/i18n/src/locales/en/workspace-settings.json`

- [ ] **Step 1: i18n**

Di `packages/i18n/src/locales/en/workspace-settings.json`, pada `workspace_settings.settings.work_item_types.form` tambah:

```json
          "requires_service": "Requires a service",
```

- [ ] **Step 2: Modal**

1. `defaultValues` tambah `requires_service: false,`.
2. `reset` untuk edit tambah `requires_service: type.requires_service,`.
3. Tambah switch setelah baris Active:

```tsx
<div className="flex items-center gap-2">
  <span className="text-13 font-medium">{t("workspace_settings.settings.work_item_types.form.requires_service")}</span>
  <Controller
    control={control}
    name="requires_service"
    render={({ field: { value, onChange } }) => (
      <Switch
        size="sm"
        checked={value ?? false}
        onCheckedChange={onChange}
        aria-label={t("workspace_settings.settings.work_item_types.form.requires_service")}
      />
    )}
  />
</div>
```

- [ ] **Step 3: Verifikasi**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/work-item-types/type-form-modal.tsx packages/i18n/src/locales/en/workspace-settings.json
git commit -m "feat(web): toggle requires_service in work item type settings"
```

Catatan: sinkronisasi locale lain memakai skill translate (repo workflow) — jalankan bila diminta user.

---

### Task 16: Docs + verifikasi penuh

**Files:**

- Modify: `docs/features/intake.md`

- [ ] **Step 1: Update snapshot docs**

Di `docs/features/intake.md`:

1. Tambah di §Current State: item daftar bahwa accept digate type (+ service untuk tipe `requires_service`) dan Jev menyaran category + service.
2. Tambah subsection `### Gate accept & service binding (2026-10-04)` yang merangkum: flag per tipe, sentinel `__none__`, apply category/service, state triage dipertahankan.
3. Tambah baris changelog `2026-10-04`.

- [ ] **Step 2: Jalankan seluruh check**

Run (dari root):

```bash
pnpm check:types
pnpm check:lint
```

Expected: PASS.

Run (dari `apps/api-rs`):

```bash
cargo test -p ai triage
cargo test -p api --test v1_work_item_type_test
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1
pnpm --filter=web test -- core/components/inbox/accept-gate.test.ts
```

Expected: semua PASS.

- [ ] **Step 3: Manual live (sesuai AGENTS.md)**

```bash
docker compose -f docker-compose-local.yml up -d --build api worker beat-worker
curl http://localhost:8000/health
systemctl --user restart plane-live.service
```

Lalu rebuild web prod (perubahan FE):

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Verifikasi manual: buka intake, buat item, cek panel saran memuat service, Apply category+service, tombol Accept enabled, accept sukses dan issue punya type+service link.

- [ ] **Step 4: Commit**

```bash
git add docs/features/intake.md
git commit -m "docs(intake): document type gate and service binding"
```

---

## Catatan Eksekusi

- Suite scratch Rust (`intake_triage_test`) memakai prefix unik per test dan `purge` per workspace; selalu jalankan `-- --test-threads=1` (AGENTS.md).
- Migrasi sqlx 0014 idempotent (`IF NOT EXISTS`), aman bila Django 0128 sudah jalan lebih dulu.
- Jangan mengubah endpoint intake existing selain gate accept + field baru; parity FE (`intake_triage_routes_test`) memverifikasi URL triage tetap 3.
