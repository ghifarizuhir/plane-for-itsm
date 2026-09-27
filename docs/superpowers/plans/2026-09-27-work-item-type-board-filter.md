# Work Item Type Board Filter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membuat filter **Work item type** berfungsi end-to-end di board/list project: rich `filters` dihormati api-rs, `type_id` masuk allowlist, UI filter type baru, dan board berfilter satu type menampilkan kolom state milik type itu.

**Architecture:** Backend api-rs `GET .../issues/` saat ini mengabaikan `filters` (flat + grouped). Plan ini mengalirkan `parse_complex_filter` + `apply_complex_filter` ke `push_list_where` (count, page, scan, rows) lalu menambah `type_id` ke allowlist/`apply_complex_leaf`. Di web, filter baru didaftarkan di rich-filters config (`packages/utils`), opsi diambil dari `workflow-map` project, dan `getSingleWorkItemTypeId` membaca `richFilters` sehingga `getStateColumns` (yang sudah menerima `typeId`) otomatis merender kolom state type tersebut. Quick-add/header create menyuntik `type_id` agar lolos `validate_create_refs`.

**Tech Stack:** Rust (axum + sqlx QueryBuilder), React 18 + MobX + React Router v7, TypeScript, vitest, pnpm/turbo.

**Prasyarat & aturan repo:**

- Branch saat ini `preview`, HEAD `dbc178c95`. Working tree punya ~20 file branding termodifikasi yang **bukan** milik plan ini — jangan pernah `git add -A`; stage hanya file per task.
- api-rs berjalan via `docker compose -f docker-compose-local.yml` (project `plane-for-itsm`). Rebuild Rust LTO bisa 10+ menit tanpa output: jalankan detached dengan `setsid ... > /tmp/plane-api-build.log 2>&1 < /dev/null &`, poll log, jangan abort.
- Web prod (port 3000) dilayani `serve -s apps/web/build/client`; setelah perubahan web: `pnpm --filter=web build && systemctl --user restart plane-web-prod.service`.
- Test yang dipakai: Rust `cd apps/api-rs && cargo test -p api --lib issue_list_tests` dan integrasi `cargo test -p api --test workflow_test -- --test-threads=1`; web `pnpm --filter=web test`, `pnpm --filter=web check:types`, `pnpm --filter=web check:lint`.

---

## File Structure

**Backend (api-rs):**

- `apps/api-rs/crates/api/src/routes/issue_query.rs` — `push_list_where` menerima filter tree; `list` + `grouped_list_response` mem-parse dan meneruskan; allowlist + arm `type_id`; skip `customproperty_*`; unit test di `mod issue_list_tests`.
- `apps/api-rs/crates/api/src/routes/workflow.rs` — `workflow_map` menyaring `is_active`/`is_epic`.
- `apps/api-rs/crates/api/tests/workflow_test.rs` — test integrasi map menyaring type nonaktif.

**Backend (Django, parity):**

- `apps/api/plane/utils/filters/filterset.py` — `type_id` di `IssueFilterSet`.
- `apps/api/plane/utils/filters/converters.py` — mapping legacy `issue_type` → `type_id`.

**Web — tipe & config:**

- `packages/types/src/view-props.ts` — `"type_id"` di `WORK_ITEM_FILTER_PROPERTY_KEYS`.
- `packages/constants/src/issue/filter.ts` — `"type_id"` di `ISSUE_DISPLAY_FILTERS_BY_PAGE.issues.filters`.
- `packages/utils/src/work-item-filters/configs/filters/work-item-type.ts` (baru) + `.../filters/index.ts` — factory config.
- `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx` — registrasi config + opsi dari workflow map.
- `packages/shared-state/src/store/work-item-filters/adapter.ts` — buang `customproperty_*`.

**Web — board:**

- `apps/web/core/store/workflow.helpers.ts` (+ `.test.ts`) — `getSingleWorkItemTypeId` membaca `richFilters`.
- `apps/web/core/components/issues/issue-layouts/kanban/default.tsx`, `kanban/swimlanes.tsx`, `kanban/kanban-group.tsx`, `list/default.tsx`, `list/list-group.tsx`, `issue-layouts/utils.tsx` — typeId per-axis, payload `type_id`, quick-add.

---

### Task A1: api-rs `list` menghormati rich filters (flat + grouped)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs:82-94` (`push_list_where`), `:181-310` (`list`), `:96-179` (`grouped_list_response`)
- Test: `apps/api-rs/crates/api/src/routes/issue_query.rs` (`mod issue_list_tests`, mulai `:2050`)

- [ ] **Step 1: Tulis test SQL-shape yang gagal**

Tambahkan di `mod issue_list_tests` (dekat `complex_filter_empty_object_is_noop_like_django`):

```rust
/// `list` (board/list project) harus ikut menerapkan complex `filters`,
/// bukan hanya `list_detail`. Bentuk SQL: WHERE dasar + ` AND (...i.priority...)`.
#[test]
fn list_where_includes_complex_filter_like_django() {
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT 1 FROM issues i LEFT JOIN states s ON s.id = i.state_id");
    let tree = parse_complex_filter(Some(r#"{"and":[{"priority__in":"urgent,high"}]}"#)).unwrap();
    push_list_where(&mut qb, uuid::Uuid::nil(), false, uuid::Uuid::nil(), tree.as_ref()).unwrap();
    let sql = qb.sql();
    assert!(sql.contains("i.priority"), "sql: {sql}");
}

/// Field tidak dikenal tetap 400 (bukan diabaikan) — parity dengan list_detail.
#[test]
fn list_where_rejects_unknown_field_like_django() {
    let err = parse_complex_filter(Some(r#"{"and":[{"nope__in":"x"}]}"#)).unwrap_err();
    assert_eq!(err.code, "invalid_filter_field");
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --lib list_where_includes_complex_filter_like_django`
Expected: FAIL — `push_list_where` hanya menerima 4 argumen (E0061).

- [ ] **Step 3: Implementasi**

Ganti `push_list_where` (`issue_query.rs:82-94`) menjadi:

```rust
/// Shared WHERE scope for the `list` COUNT + page + scan + rows queries: the
/// flat visibility (`base.py:266-294`) plus, when present, the complex
/// `filters` tree (same parse/apply as `list_detail`). Apply errors map to
/// the DRF-style 400 body via `complex_filter_error_response`.
fn push_list_where(
    qb: &mut QueryBuilder<Postgres>,
    project_id: uuid::Uuid,
    guest_scoped: bool,
    user_id: uuid::Uuid,
    filters: Option<&serde_json::Value>,
) -> Result<(), (StatusCode, Json<Value>)> {
    qb.push(" WHERE i.project_id = ").push_bind(project_id).push(
        " AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false AND s.\"group\" <> 'triage'",
    );
    if guest_scoped {
        qb.push(" AND i.created_by_id = ").push_bind(user_id);
    }
    if let Some(tree) = filters {
        apply_complex_filter(qb, tree).map_err(complex_filter_error_response)?;
    }
    Ok(())
}

/// `ComplexFilterError` → 400 body `{"message","code"}` (sama seperti `list_detail`).
fn complex_filter_error_response(e: ComplexFilterError) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"message": e.message, "code": e.code})))
}
```

Di `list` (`:181`), tepat setelah blok allowlist `group_by` (`:234-238`), tambahkan parse sekali:

```rust
    // Rich `filters` kini juga berlaku untuk board/list project (parity
    // `list_detail`): parse + validasi sekali, lalu diteruskan ke semua query.
    let filter_tree = match parse_complex_filter(q.filters.as_deref()) {
        Ok(t) => t,
        Err(e) => return Ok(complex_filter_error_response(e)),
    };
```

Lalu di cabang grouped (`:261-274`) teruskan tree-nya:

```rust
        return grouped_list_response(
            &st.pool,
            &slug,
            project_id,
            auth.0,
            guest_scoped,
            q.group_by.as_deref().unwrap_or(""),
            q.sub_group_by.as_deref().filter(|s| !s.is_empty()),
            filter_tree.as_ref(),
            limit,
            cursor.page,
        )
        .await;
```

Ganti baris `let _ = (&q.order_by, &q.filters);` (`:278`) menjadi:

```rust
    // `order_by` masih accepted-and-ignored pada slice ini; `filters` kini dipakai.
    let _ = &q.order_by;
```

Pada dua query flat (`:284-287` count dan `:294-302` page) tambahkan argumen tree + propagasi error (`list` mengembalikan `Ok(400 JSON)`, bukan `AppError`, jadi pakai `if let`):

```rust
    if let Err(resp) = push_list_where(&mut count_qb, project_id, guest_scoped, auth.0, filter_tree.as_ref()) {
        return Ok(resp);
    }
```

dan

```rust
            if let Err(resp) = push_list_where(&mut page_qb, project_id, guest_scoped, auth.0, filter_tree.as_ref()) {
                return Ok(resp);
            }
```

Ubah `grouped_list_response` (`:101-111`) supaya menerima + memakai tree:

```rust
async fn grouped_list_response(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    user_id: uuid::Uuid,
    guest_scoped: bool,
    group: &str,
    sub: Option<&str>,
    filters: Option<&serde_json::Value>,
    limit: i64,
    page: i128,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
```

lalu pada `scan_qb` (`:117`) dan `rows_qb` (`:151`):

```rust
    if let Err(resp) = push_list_where(&mut scan_qb, project_id, guest_scoped, user_id, filters) {
        return Ok(resp);
    }
```

```rust
        if let Err(resp) = push_list_where(&mut rows_qb, project_id, guest_scoped, user_id, filters) {
            return Ok(resp);
        }
```

Catatan: `ComplexFilterError` bukan `AppError`; semua error di permukaan `list`/`grouped` dikembalikan sebagai `Ok(400 JSON)` agar tidak perlu `From` impl baru.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p api --lib issue_list_tests`
Expected: PASS, termasuk 2 test baru; test lama (`complex_filter_*`, `legacy_*`) tetap lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_query.rs
git commit -m "feat(api-rs): honor rich filters on project issue list"
```

---

### Task A2: api-rs allowlist + kolom `type_id`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs:1065-1115` (allowlist), `:1462-1470` (direct UUID arm)
- Test: `mod issue_list_tests`

- [ ] **Step 1: Tulis test yang gagal**

```rust
#[test]
fn complex_type_id_filter_maps_to_issue_column() {
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT 1 FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE true");
    let tree = parse_complex_filter(Some(
        r#"{"and":[{"type_id__in":"11111111-1111-1111-1111-111111111111"}]}"#,
    ))
    .unwrap();
    apply_complex_filter(&mut qb, tree.as_ref().unwrap()).unwrap();
    assert!(qb.sql().contains("i.type_id"), "sql: {}", qb.sql());
}

#[test]
fn complex_type_id_exact_is_allowlisted() {
    assert!(COMPLEX_FILTER_ALLOWLIST.contains(&"type_id"));
    assert!(COMPLEX_FILTER_ALLOWLIST.contains(&"type_id__exact"));
    assert!(COMPLEX_FILTER_ALLOWLIST.contains(&"type_id__in"));
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --lib complex_type_id`
Expected: FAIL — `invalid_filter_field` / assert allowlist false.

- [ ] **Step 3: Implementasi**

Di `COMPLEX_FILTER_ALLOWLIST` (`:1065-1115`) sisipkan setelah `"state_id__in",`:

```rust
    "type_id",
    "type_id__exact",
    "type_id__in",
```

Di blok direct UUID columns (`:1463-1467`) tambahkan satu arm:

```rust
    if let Some(col) = match base {
        "created_by_id" => Some("i.created_by_id"),
        "state_id" => Some("i.state_id"),
        "type_id" => Some("i.type_id"),
        "project_id" => Some("i.project_id"),
        _ => None,
    } {
```

- [ ] **Step 4: Jalankan, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p api --lib complex_type_id`
Expected: PASS ×2.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_query.rs
git commit -m "feat(api-rs): allow work item type filter in issue queries"
```

---

### Task A3: api-rs abaikan `customproperty_*` (fork tanpa tabel custom property)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs:1157-1164` (`parse_complex_filter` field loop)
- Test: `mod issue_list_tests`

Konteks: DB fork ini tidak punya tabel `issue_properties`/`issue_property_values` dan UI web tidak pernah membuat kondisi `customproperty_*`; tapi adapter FE mengizinkan prefix itu saat memuat data lama. Tanpa skip, begitu Task A1 aktif, board dengan filter lama seperti itu akan 400.

- [ ] **Step 1: Tulis test yang gagal**

```rust
/// Fork tidak punya custom property; kondisi legacy `customproperty_*`
/// di-skip (no-op) supaya board lama tidak 400 begitu `filters` dihormati.
#[test]
fn complex_customproperty_leaf_is_noop_not_400() {
    let tree = parse_complex_filter(Some(r#"{"and":[{"customproperty_abc__in":"x"}]}"#)).unwrap();
    assert!(tree.is_some());
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT 1 FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE true");
    apply_complex_filter(&mut qb, tree.as_ref().unwrap()).unwrap();
    assert!(qb.sql().contains("TRUE"), "sql: {}", qb.sql());
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --lib complex_customproperty_leaf_is_noop_not_400`
Expected: FAIL — `invalid_filter_field` (unwrap panik).

- [ ] **Step 3: Implementasi**

Di loop field `parse_complex_filter`:

```rust
    for field in extract_filter_fields(&value) {
        // Fork ini tidak punya tabel custom property; kondisi `customproperty_*`
        // (mungkin tersisa dari data lama) di-skip sebagai no-op alih-alih 400.
        if field.starts_with("customproperty_") {
            continue;
        }
        if !COMPLEX_FILTER_ALLOWLIST.contains(&field.as_str()) {
            return Err(ComplexFilterError::new(
                &format!("Filtering on field '{field}' is not allowed"),
                "invalid_filter_field",
            ));
        }
    }
```

`eval_filter_node` (`:1399-1408`) sudah melewati key non-allowlist dan menyisipkan `TRUE` untuk leaf all-skipped, jadi tidak ada perubahan lain.

- [ ] **Step 4: Jalankan, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p api --lib complex_customproperty`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_query.rs
git commit -m "fix(api-rs): ignore legacy custom property filter leaves"
```

---

### Task A4: `workflow_map` menyaring type nonaktif / epic

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs:1265-1275`
- Test: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tulis test integrasi yang gagal**

Di `workflow_test.rs`, dekat `workflow_switch_guard_and_map_details` (`:1749`), tambahkan test baru mengikuti harness `purge`/`make_workspace` (`:125-231`): buat workspace+project, buat type + workflow + state + transisi (lewat helper/V1 endpoint yang sudah dipakai test tetangga), import type ke project, lalu set type `is_active = false` via PATCH work-item-types, kemudian panggil handler `workflow_map` dan assert `types == []`. Setelah itu set kembali `is_active = true` dan assert type muncul lagi.

```rust
#[tokio::test]
async fn workflow_map_hides_inactive_types() {
    // ...harness: pool, app_state, purge(slug), make_workspace, project, type+workflow...
    // 1) aktif → map berisi 1 type
    // 2) PATCH is_active=false
    // 3) map kosong
    // 4) PATCH is_active=true → map berisi 1 type lagi
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --test workflow_test workflow_map_hides_inactive_types -- --test-threads=1`
Expected: FAIL — type nonaktif masih muncul di map.

- [ ] **Step 3: Implementasi**

Di `workflow_map` (`workflow.rs:1265-1275`) ubah SQL jadi:

```rust
        "SELECT t.id, t.workflow_id, t.name FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         JOIN projects p ON p.id = pit.project_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND t.workflow_id IS NOT NULL AND p.deleted_at IS NULL \
           AND t.is_active = true AND t.is_epic = false \
         ORDER BY t.name",
```

Biarkan `ensure_project_workflows` (`:202-220`) apa adanya: materialisasi mirror untuk type yang sudah ter-link tidak dibatalkan hanya karena type dinonaktifkan (menghindari churn), cukup disembunyikan dari map/selector.

- [ ] **Step 4: Jalankan, pastikan lulus**

Run: `cd apps/api-rs && cargo test -p api --test workflow_test -- --test-threads=1`
Expected: PASS (semua test workflow_test).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "fix(api-rs): hide inactive and epic types from workflow map"
```

---

### Task A5: Django parity (`type_id` di filterset + converter legacy)

**Files:**

- Modify: `apps/api/plane/utils/filters/filterset.py:135-200`
- Modify: `apps/api/plane/utils/filters/converters.py:15-40`
- Test: `apps/api/plane/tests/unit/utils/` (jalankan suite unit filter yang ada)

- [ ] **Step 1: Tambah field di `IssueFilterSet`**

Di `apps/api/plane/utils/filters/filterset.py` (class mulai `:135`), tambahkan sejajar field uuid lain (`created_by_id` dsb.):

```python
    type_id = filters.UUIDFilter(field_name="type_id")
    type_id__in = UUIDInFilter(field_name="type_id", lookup_expr="in")
```

Sesuaikan nama helper `UUIDInFilter` persis seperti yang dipakai field `*_id__in` tetangga di file itu (ikuti impor yang sudah ada).

- [ ] **Step 2: Tambah mapping legacy di converters**

Di `apps/api/plane/utils/filters/converters.py`, tambahkan mapping legacy `"issue_type": "type_id"` pada tabel konversi (dekat mapping field uuid lain) dan pastikan `"type_id"` masuk `DEFAULT_UUID_FIELDS` (`:31-40`).

- [ ] **Step 3: Jalankan unit test filter Django**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest -m unit -k "filterset or datetime"`
Expected: PASS (tidak ada regresi impor/validasi). Tidak ada test baru khusus: perilaku live dipegang api-rs, Django hanya parity.

- [ ] **Step 4: Commit**

```bash
git add apps/api/plane/utils/filters/filterset.py apps/api/plane/utils/filters/converters.py
git commit -m "feat(api): add work item type to IssueFilterSet (parity)"
```

---

### Task A6: Tipe filter + daftar filter halaman project

**Files:**

- Modify: `packages/types/src/view-props.ts:96-112`
- Modify: `packages/constants/src/issue/filter.ts:206-219`

- [ ] **Step 1: Tambah properti filter**

`WORK_ITEM_FILTER_PROPERTY_KEYS` — sisipkan setelah `"state_id"`:

```ts
  "state_id",
  "type_id",
```

`ISSUE_DISPLAY_FILTERS_BY_PAGE.issues.filters` — sisipkan setelah `"state_id"`:

```ts
      "state_id",
      "type_id",
```

- [ ] **Step 2: Verifikasi tipe**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/view-props.ts packages/constants/src/issue/filter.ts
git commit -m "feat(types): register work item type as rich filter property"
```

---

### Task A7: Factory config filter work item type

**Files:**

- Create: `packages/utils/src/work-item-filters/configs/filters/work-item-type.ts`
- Modify: `packages/utils/src/work-item-filters/configs/filters/index.ts:7-14`

- [ ] **Step 1: Tulis factory**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import type { TFilterProperty, TSupportedOperators } from "@plane/types";
import { COLLECTION_OPERATOR, EQUALITY_OPERATOR } from "@plane/types";
// local imports
import type { IFilterIconConfig, TCreateFilterConfig, TCreateFilterConfigParams } from "../../../rich-filters";
import { createFilterConfig, getMultiSelectConfig, createOperatorConfigEntry } from "../../../rich-filters";

/** Opsi type berasal dari workflow-map project (`type_id` + `type_name`). */
export type TWorkItemTypeOption = { type_id: string; type_name: string };

export type TCreateWorkItemTypeFilterParams = TCreateFilterConfigParams &
  IFilterIconConfig<TWorkItemTypeOption> & {
    types: TWorkItemTypeOption[];
  };

export const getWorkItemTypeMultiSelectConfig = (
  params: TCreateWorkItemTypeFilterParams,
  singleValueOperator: TSupportedOperators
) =>
  getMultiSelectConfig<TWorkItemTypeOption, string, TWorkItemTypeOption>(
    {
      items: params.types,
      getId: (type) => type.type_id,
      getLabel: (type) => type.type_name,
      getValue: (type) => type.type_id,
      getIconData: (type) => type,
    },
    {
      singleValueOperator,
      ...params,
    },
    {
      ...params,
    }
  );

export const getWorkItemTypeFilterConfig =
  <P extends TFilterProperty>(key: P): TCreateFilterConfig<P, TCreateWorkItemTypeFilterParams> =>
  (params: TCreateWorkItemTypeFilterParams) =>
    createFilterConfig<P>({
      id: key,
      label: "Work item type",
      ...params,
      icon: params.filterIcon,
      supportedOperatorConfigsMap: new Map([
        createOperatorConfigEntry(COLLECTION_OPERATOR.IN, params, (updatedParams) =>
          getWorkItemTypeMultiSelectConfig(updatedParams, EQUALITY_OPERATOR.EXACT)
        ),
      ]),
    });
```

- [ ] **Step 2: Ekspor dari barrel**

Di `packages/utils/src/work-item-filters/configs/filters/index.ts` tambahkan (ikut urutan alfabetis file itu):

```ts
export * from "./work-item-type";
```

- [ ] **Step 3: Verifikasi tipe**

Run: `pnpm --filter=web check:types`
Expected: exit 0. (Tidak ada test infra di `packages/utils`; verifikasi perilaku lewat typecheck + smoke UI di Task A13.)

- [ ] **Step 4: Commit**

```bash
git add packages/utils/src/work-item-filters/configs/filters/work-item-type.ts packages/utils/src/work-item-filters/configs/filters/index.ts
git commit -m "feat(utils): add work item type rich filter config"
```

---

### Task A8: Registrasi filter type di hook filter project

**Files:**

- Modify: `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`

Discovery dulu (wajib, 1 menit):

```bash
rg -n "getWorkflowMap|useWorkflow" apps/web/core/hooks/store/use-workflow.ts apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx
rg -n "WorkItemsOutline|WorkItem" apps/web/core/components/dropdowns/work-item-type/dropdown.tsx | head -5
rg -n "registerAll|configs" apps/web/core/components/work-item-filters/filters-hoc/base.tsx | head -20
```

Catat: nama hook store workflow, ikon yang dipakai dropdown type, dan apakah `base.tsx` me-registrasi ulang config saat array `configs` berubah (kalau tidak, panel filter butuh remount — catat di komentar task ini).

- [ ] **Step 1: Tambah opsi type dari workflow map**

Di `use-work-item-filters-config.tsx`:

1. Impor hook store workflow (nama persis hasil discovery, mis. `import { useWorkflow } from "@/hooks/store/use-workflow";`).
2. Impor `getWorkItemTypeFilterConfig` dari `@plane/utils`.
3. Impor ikon dari `@plane/propel/icons` (ikon sama dengan `dropdowns/work-item-type/dropdown.tsx`).
4. Di body hook setelah `const project = ...` (`:104`):

```ts
const { getWorkflowMap } = useWorkflow();
const workItemTypes = useMemo(
  () =>
    projectId
      ? (getWorkflowMap(projectId)?.types ?? []).map((type) => ({
          type_id: type.type_id,
          type_name: type.type_name,
        }))
      : [],
  [projectId, getWorkflowMap]
);
```

- [ ] **Step 2: Tambah memo config**

Setelah `priorityFilterConfig` (`:297-306`), tambahkan:

```ts
  // work item type filter config (opsi dari workflow-map project)
  const workItemTypeFilterConfig = useMemo(
    () =>
      getWorkItemTypeFilterConfig<TWorkItemFilterProperty>("type_id")({
        isEnabled: isFilterEnabled("type_id") && workItemTypes.length > 0,
        filterIcon: WorkItemsOutline,
        getOptionIcon: () => <WorkItemsOutline className="h-3 w-3 flex-shrink-0" />,
        types: workItemTypes,
        ...operatorConfigs,
      }),
    [isFilterEnabled, workItemTypes, operatorConfigs]
  );
```

- [ ] **Step 3: Daftarkan di `configs` + `configMap`**

Di array `configs` (`:367-383`) sisipkan `workItemTypeFilterConfig` setelah `stateFilterConfig`; di `configMap` (`:384-400`) tambahkan `type_id: workItemTypeFilterConfig,`.

- [ ] **Step 4: Verifikasi tipe + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: exit 0 (warning lama boleh tetap).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx
git commit -m "feat(web): register work item type filter with workflow map options"
```

---

### Task A9: `getSingleWorkItemTypeId` membaca `richFilters` (TDD)

**Files:**

- Modify: `apps/web/core/store/workflow.helpers.ts:14-32`
- Test: `apps/web/core/store/workflow.helpers.test.ts:293-317`

- [ ] **Step 1: Tulis test yang gagal**

Ganti blok test `getSingleWorkItemTypeId` (`:293-317`) menjadi:

```ts
describe("getSingleWorkItemTypeId", () => {
  it("membaca satu type dari richFilters bentuk type_id__in", () => {
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__in: "type-1" }] } })).toBe("type-1");
  });

  it("membaca type_id__exact dan nilai comma tunggal", () => {
    expect(getSingleWorkItemTypeId({ richFilters: { type_id__exact: "type-2" } })).toBe("type-2");
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__in: "type-3," }] } })).toBe("type-3");
  });

  it("null saat tidak ada / lebih dari satu type", () => {
    expect(getSingleWorkItemTypeId({ richFilters: {} })).toBeNull();
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__in: "t-1,t-2" }] } })).toBeNull();
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ priority__in: "urgent" }] } })).toBeNull();
  });

  it("tetap membaca bentuk legacy filters.issue_type", () => {
    expect(getSingleWorkItemTypeId({ filters: { issue_type: ["type-1"] } })).toBe("type-1");
    expect(getSingleWorkItemTypeId({ filters: { issue_type: ["t-1", "t-2"] } })).toBeNull();
  });

  it("null untuk input kosong", () => {
    expect(getSingleWorkItemTypeId(undefined)).toBeNull();
    expect(getSingleWorkItemTypeId(null)).toBeNull();
  });
});
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test workflow.helpers`
Expected: FAIL pada 3 test richFilters.

- [ ] **Step 3: Implementasi**

Ganti `getSingleWorkItemTypeId` (`workflow.helpers.ts:14-32`) dengan:

```ts
/**
 * Type tunggal efektif dari filter board: utamanya `richFilters`
 * (`type_id__in` / `type_id__exact`, boleh di dalam grup `and`), fallback ke
 * bentuk legacy `filters.issue_type`. Dipakai `getStateColumns` untuk memilih
 * kolom state milik type tersebut.
 */
const collectTypeIds = (node: TWorkItemFilterExpressionData | undefined, out: Set<string>): void => {
  if (!node) return;
  const record = node as Record<string, unknown>;
  const andChildren = record.and;
  if (Array.isArray(andChildren)) {
    andChildren.forEach((child) => collectTypeIds(child as TWorkItemFilterExpressionData, out));
    return;
  }
  for (const key of ["type_id", "type_id__exact", "type_id__in"] as const) {
    const raw = record[key];
    if (raw === undefined || raw === null) continue;
    const value = Array.isArray(raw) ? raw.join(",") : String(raw);
    value
      .split(",")
      .map((part) => part.trim())
      .filter((part) => part.length > 0)
      .forEach((part) => out.add(part));
  }
};

export const getSingleWorkItemTypeId = (
  issueFilters: TLegacyIssueFilterBag | IIssueFilters | null | undefined
): string | null => {
  if (!issueFilters) return null;
  if ("filters" in issueFilters) {
    const legacyTypeIds = issueFilters.filters?.issue_type;
    return legacyTypeIds?.length === 1 ? (legacyTypeIds[0] ?? null) : null;
  }
  const ids = new Set<string>();
  collectTypeIds(issueFilters.richFilters, ids);
  return ids.size === 1 ? ([...ids][0] ?? null) : null;
};
```

Perbarui impor tipe di `workflow.helpers.ts:1`:

```ts
import type {
  IIssueFilterOptions,
  IIssueFilters,
  IState,
  TWorkItemFilterExpressionData,
  TWorkflowMap,
  TWorkflowMapType,
} from "@plane/types";
```

- [ ] **Step 4: Jalankan, pastikan lulus**

Run: `pnpm --filter=web test workflow.helpers`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/workflow.helpers.ts apps/web/core/store/workflow.helpers.test.ts
git commit -m "feat(web): resolve single work item type from rich filters"
```

---

### Task A10: Kolom kanban typed + payload `type_id` (per-axis)

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/utils.tsx:245-264`
- Modify: `apps/web/core/components/issues/issue-layouts/kanban/swimlanes.tsx:295-316`
- Modify: `apps/web/core/components/issues/issue-layouts/list/default.tsx:88-93`

- [ ] **Step 1: Payload `type_id` saat typed**

Di `utils.tsx` `getStateColumns` (`:254-263`) ubah payload:

```ts
  return states.map((state) => ({
    id: state.id,
    name: state.name,
    icon: (
      <div className="size-4 rounded-full">
        <StateGroupIcon stateGroup={state.group} color={state.color} size={EIconSize.LG} percentage={state.order} />
      </div>
    ),
    // type_id ikut payload supaya header "+" create form membuka type yang benar
    payload: { state_id: state.id, ...(mapType ? { type_id: mapType.type_id } : {}) },
  }));
```

- [ ] **Step 2: Per-axis gating di swimlanes**

Di `swimlanes.tsx` (`:295-316`), `typeId` hanya berlaku untuk axis yang di-group by `"state"` (kalau tidak, `resolveStateColumns` salah menerapkan daftar kolom typed ke axis lain):

```ts
const groupTypeId = group_by === "state" ? workItemTypeId : null;
const subGroupTypeId = sub_group_by === "state" ? workItemTypeId : null;
```

lalu pakai `typeId: groupTypeId` pada pemanggilan `getGroupByColumns` untuk group (`:301-308`) dan `typeId: subGroupTypeId` untuk sub-group (`:309-316`). Pastikan `group_by`/`sub_group_by` dari `displayFilters` sudah ada di scope (tambahkan dari `displayFilters` yang sama seperti `kanban/default.tsx:108-113` bila belum).

- [ ] **Step 3: List view ikut payload typed**

Di `list/default.tsx` (`:88-93`), tambahkan `projectId` + `typeId`:

```ts
  const { projectId } = useParams();
  const { issuesFilter } = useIssues(storeType);
  const workItemTypeId = getSingleWorkItemTypeId(issuesFilter?.issueFilters);
  ...
  getGroupByColumns({
    groupBy,
    includeNone,
    isWorkspaceLevel,
    isEpic,
    projectId,
    typeId: groupBy === "state" ? workItemTypeId : null,
  })
```

sesuaikan impor (`useParams`, `getSingleWorkItemTypeId`) mengikuti `kanban/default.tsx`.

- [ ] **Step 4: Verifikasi tipe + lint + test**

Run: `pnpm --filter=web check:types && pnpm --filter=web test workflow.helpers && pnpm --filter=web check:lint`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/utils.tsx apps/web/core/components/issues/issue-layouts/kanban/swimlanes.tsx apps/web/core/components/issues/issue-layouts/list/default.tsx
git commit -m "feat(web): typed kanban columns follow the single type filter"
```

---

### Task A11: Quick-add & header create menyuntik `type_id`

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/kanban/default.tsx` (prop ke `KanbanGroup`)
- Modify: `apps/web/core/components/issues/issue-layouts/kanban/kanban-group.tsx:199-249`
- Modify: `apps/web/core/components/issues/issue-layouts/list/list-group.tsx:150-177`
- Modify: `apps/web/core/components/issues/issue-layouts/list/default.tsx` (prop ke `ListGroup`)

Konteks: `validate_create_refs` (`apps/api-rs/crates/api/src/routes/issue_write.rs:103-134`) menolak `state_id` typed tanpa `type_id` yang cocok ("State is not valid for this work item type"). Quick-add tidak punya pemilih type, jadi type tunggal dari filter harus disuntik.

- [ ] **Step 1: Kanban group**

Di `kanban-group.tsx` tambahkan prop `workItemTypeId?: string | null` pada tipe props komponen, lalu di `prePopulateQuickAddData` (`:209-210`) dan cabang sub-group state (`:229-230`):

```ts
if (groupByKey === "state") {
  preloadedData = {
    ...preloadedData,
    state_id: groupValue,
    ...(workItemTypeId ? { type_id: workItemTypeId } : {}),
  };
}
```

```ts
if (subGroupByKey === "state") {
  preloadedData = {
    ...preloadedData,
    state_id: subGroupValue,
    ...(workItemTypeId ? { type_id: workItemTypeId } : {}),
  };
}
```

Di `kanban/default.tsx` pada render `<KanbanGroup ... />` tambahkan `workItemTypeId={workItemTypeId}` (`workItemTypeId` sudah dihitung di `:113`).

- [ ] **Step 2: List group**

Di `list-group.tsx` tambahkan prop `workItemTypeId?: string | null`, ubah cabang `groupByKey === "state"` (`:157-158`):

```ts
if (groupByKey === "state") {
  preloadedData = {
    ...preloadedData,
    state_id: value,
    ...(workItemTypeId ? { type_id: workItemTypeId } : {}),
  };
}
```

Di `list/default.tsx` hitung `workItemTypeId` (Task A10 Step 3) dan teruskan ke render `<ListGroup ... />`.

- [ ] **Step 3: Verifikasi tipe + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/kanban apps/web/core/components/issues/issue-layouts/list
git commit -m "feat(web): inject filtered work item type into quick add"
```

---

### Task A12: Adapter buang kondisi `customproperty_*` saat parse

**Files:**

- Modify: `packages/shared-state/src/store/work-item-filters/adapter.ts:170`

- [ ] **Step 1: Implementasi**

Ubah validasi properti di `adapter.ts:170` agar hanya properti yang dikenal yang lolos (prefix legacy dibuang):

```ts
if (!WORK_ITEM_FILTER_PROPERTY_KEYS.includes(property as any)) {
  return null;
}
```

- [ ] **Step 2: Verifikasi tipe**

Run: `pnpm --filter=web check:types`
Expected: exit 0. (Tidak ada infra test di `packages/shared-state`; perilaku diverifikasi di smoke Task A13 dengan memuat view lama.)

- [ ] **Step 3: Commit**

```bash
git add packages/shared-state/src/store/work-item-filters/adapter.ts
git commit -m "fix(shared-state): drop unsupported custom property filter conditions"
```

---

### Task A13: Verifikasi menyeluruh + deploy

**Files:** tidak ada perubahan kode.

- [ ] **Step 1: Gate web**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web test
```

Expected: types exit 0; lint 0 error; test 160+ lulus (termasuk test baru A9).

- [ ] **Step 2: Rebuild api-rs (detached, sabar LTO)**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll `/tmp/plane-api-build.log` sampai selesai (10+ menit tanpa output = normal), lalu:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
systemctl --user restart plane-live.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
```

Expected: 200 dan 200 (live cold start ~8 detik).

- [ ] **Step 3: Smoke API filter (curl dengan sesi browser user atau lewat UI)**

Di board project (mis. `PREPAID`): set filter **Work item type = Request** lalu muat ulang. Cek di Network tab bahwa request `/issues/` mengembalikan hanya item type itu (bukan semua). Cek juga filter lama (priority/state) sekarang benar-benar menyaring board — sebelumnya no-op.

- [ ] **Step 4: Build + restart web prod**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Tunggu ~10 detik (cold start `serve`), verifikasi `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000/` → 200, dan deep route `/{workspaceSlug}/projects/{projectId}/` → 200. Hard-refresh browser sekali.

- [ ] **Step 5: Checklist smoke manual (dengan user)**

1. Filter **Work item type** muncul di panel filter project (hanya kalau project punya type aktif) dan opsinya hanya type aktif.
2. Pilih satu type → kolom kanban berubah ke state workflow type itu (urut sequence), drag antar kolom masih jalan.
3. Quick-add di kolom typed membuat item bertipe benar (tidak error "State is not valid for this work item type").
4. Header "+" di kolom typed membuka create modal dengan type terisi.
5. Hapus filter type → kembali ke 5 kolom group; drag & quick-add tetap normal.
6. Type yang di-nonaktifkan hilang dari opsi filter dan dari dropdown Type create form.
7. Filter priority/label lama kini benar-benar menyaring board & list.
8. Buka view lama (kalau ada) yang menyimpan `customproperty_*` → tidak 400 (kondisi itu hilang dari chip filter).

- [ ] **Step 6: Commit sisa (bila ada) + laporan**

```bash
git status --short
```

Pastikan hanya file plan ini/plan B yang mungkin tersisa; jangan commit file branding pre-existing.

---

## Self-Review

- **Spec coverage:** Filter `type_id` satu type → kolom state type (spec §Board/views 265-266) ✔ (A6-A11); mixed tanpa filter tetap 5 group ✔ (A10 tidak mengubah `group_by`); transition dropdown sudah ada dari feature sebelumnya (tidak diubah); quick-add type (spec create form “pilih type” untuk jalur inline) ✔ A11; badge type list/spreadsheet di luar scope follow-up ini.
- **Placeholder scan:** tidak ada TBD; semua step berisi kode/komando konkret. Task A4 test integrasi memakai harness yang sudah ada — detail body test mengikuti test tetangga di file yang sama (helper tersedia di file).
- **Type consistency:** `push_list_where` 5 argumen dipakai konsisten di 4 call site; `complex_filter_error_response` satu definisi; `getWorkItemTypeFilterConfig`/`getSingleWorkItemTypeId`/`collectTypeIds` konsisten antar task; payload `type_id` konsisten dengan `TWorkflowMapType.type_id`.

## Risiko yang diterima

- Mengaktifkan `filters` di `/issues/` membuat semua filter board/list akhirnya bekerja; bug lama yang sebelumnya tersembunyi (mis. filter tak didukung) bisa muncul. `customproperty_*` ditangani (A3/A12); bila ada key lain muncul, tambahkan ke allowlist atau skip dengan alasan jelas.
- `order_by` pada `/issues/` tetap no-op (deviation lama, di luar scope).
