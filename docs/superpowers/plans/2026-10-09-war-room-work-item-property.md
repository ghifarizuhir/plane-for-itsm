# War Room sebagai Property Work Item — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** War room (Incident bridge) jadi property on/off di work item tipe Incident, tanpa halaman list terpisah — daftar dilihat lewat filter di Work Items.

**Architecture:** Toggle & filter **diturunkan dari tabel `war_rooms`** (tanpa kolom baru di `issues`): api-rs menambah 3 kolom subquery (`war_room_id/status/severity`) ke payload issue list/detail + filter virtual `war_room` (`on`/`off`), plus 1 index migration. Web menambah display-property & rich-filter `war_room`, komponen toggle yang memanggil endpoint war-rooms existing (create/patch resolve/reopen), nav "Incident bridge" → `/issues?war_room=on`, dan menghapus halaman list + create modal.

**Tech Stack:** Rust (axum/sqlx) `apps/api-rs` · React 19 + MobX + vitest `apps/web` · react-router v7 routes · Tailwind tokens via `@plane/propel` · i18n `@plane/i18n`.

**Spec:** `docs/superpowers/specs/2026-10-09-war-room-work-item-property-design.md`

---

## File Map

**Backend (`apps/api-rs`):**

- Create: `apps/api-rs/migrations/0017_war_rooms_primary_issue_idx.sql`
- Modify: `crates/api/src/routes/issue_common.rs` (2 struct), `crates/api/src/routes/issue_query.rs` (3 SELECT + allowlist + 1 helper + tests), `crates/api/tests/war_room_test.rs` (1 test)

**Web types/constants/utils:**

- Modify: `packages/types/src/issues/issue.ts`, `packages/types/src/view-props.ts`
- Create: `packages/utils/src/work-item-filters/configs/filters/war-room.ts`
- Modify: `packages/utils/src/work-item-filters/configs/filters/index.ts`
- Modify: `packages/constants/src/issue/common.ts`, `packages/constants/src/issue/filter.ts`, `packages/constants/src/war-room.ts`
- Modify: `packages/utils/src/work-item/base.ts` (`getComputedDisplayProperties`)

**Web app:**

- Create: `apps/web/core/components/war-rooms/war-room-property.tsx`
- Create: `apps/web/core/components/issues/issue-layouts/spreadsheet/columns/war-room-column.tsx`
- Create: `apps/web/app/routes/redirects/core/war-rooms.tsx`
- Modify: `apps/web/core/store/war-room.store.ts` (+ `.test.ts`), `apps/web/core/services/war-room.service.ts`
- Modify: `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx`
- Modify: `apps/web/core/components/issues/issue-detail/sidebar.tsx`, `apps/web/core/components/issues/peek-overview/properties.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/properties/all-properties.tsx`, `apps/web/core/components/issues/issue-layouts/utils.tsx`, `.../spreadsheet/columns/index.ts`
- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx`, `apps/web/core/components/navigation/use-navigation-items.ts`, `apps/web/core/components/navigation/tab-navigation-utils.ts`
- Modify: `apps/web/app/routes/core.ts`, `packages/i18n/src/locales/en/war-room.json`
- Delete (list UI): `apps/web/core/components/war-rooms/list/**`, `.../create/create-war-room-modal.tsx`, `apps/web/core/components/war-rooms/issue-war-room-button.tsx`, `apps/web/core/store/war-room_filter.store.ts`, `apps/web/core/hooks/store/use-war-room-filter.ts`, `apps/web/app/(all)/.../war-rooms/(list)/{page,layout}.tsx`

**Docs:** `docs/features/war-rooms.md`, `docs/superpowers/plans/2026-09-30-war-room-phase-3-web-list-create.md`

**Prasyarat test DB (sekali):** `DATABASE_URL=postgres://plane:plane@localhost:5432/plane` (container `plane-db`).

---

## Phase 1 — Backend (api-rs)

### Task 1: Index migration 0017

**Files:**

- Create: `apps/api-rs/migrations/0017_war_rooms_primary_issue_idx.sql`

- [ ] **Step 1: Tulis migration**

`apps/api-rs/migrations/0017_war_rooms_primary_issue_idx.sql`:

```sql
-- War room property (Incident bridge di Work Items): lookup kolom turunan
-- `war_rooms` per issue memakai prefix `primary_issue_id`. Index existing
-- ber-prefix `project_id` (war_rooms_project_status_idx) dan partial unique
-- hanya untuk status active/monitoring (war_rooms_one_active_per_issue_idx),
-- sehingga room resolved/archived terakhir tidak ter-cover. Delta diterapkan
-- saat boot oleh `common::db::migrate`.

CREATE INDEX IF NOT EXISTS war_rooms_primary_issue_latest_idx
    ON public.war_rooms (primary_issue_id, created_at DESC)
    WHERE deleted_at IS NULL;
```

- [ ] **Step 2: Guard versi migration**

Run: `cargo test -p common --test migration_versions_test` (dari `apps/api-rs`)
Expected: `test result: ok` (versi 0017 unik, tidak ada duplikat).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/0017_war_rooms_primary_issue_idx.sql
git commit -m "feat(api): add war_rooms primary_issue lookup index"
```

---

### Task 2: Kolom turunan `war_room_id/status/severity` di payload issue

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_common.rs`
- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs`
- Test: `apps/api-rs/crates/api/src/routes/issue_query.rs` (modul `issue_list_tests`)

- [ ] **Step 1: Tulis test yang gagal**

Di `issue_query.rs`, dalam `mod issue_list_tests` (dekat `list_and_detail_selects_expose_issue_type_id`), tambahkan:

```rust
    #[test]
    fn list_and_detail_selects_expose_war_room_property() {
        // Fork (2026-10-09): property "War room" membaca kolom turunan dari
        // war_rooms (primary_issue_id), tanpa kolom baru di `issues`.
        for sql in [LIST_SELECT_SQL, DETAIL_SELECT_SQL] {
            assert!(sql.contains("AS war_room_id"), "{sql}");
            assert!(sql.contains("AS war_room_status"), "{sql}");
            assert!(sql.contains("AS war_room_severity"), "{sql}");
        }
    }
```

- [ ] **Step 2: Jalankan test — harus GAGAL**

Run: `cargo test -p api issue_list_tests::list_and_detail_selects_expose_war_room_property` (dari `apps/api-rs`)
Expected: FAIL (`AS war_room_id` belum ada).

- [ ] **Step 3: Tambah field struct**

`issue_common.rs` — di `IssueListRow` (setelah `pub type_id: Option<uuid::Uuid>,`) dan di `IssueDetailRow` (setelah `pub type_id: Option<uuid::Uuid>,`), tambahkan blok identik:

```rust
    /// Fork (2026-10-09): room terbaru non-deleted untuk issue ini (turunan,
    /// bukan kolom). Ordering: active/monitoring menang, lalu `created_at`
    /// terbaru — dipakai property "War room" & filter di web.
    pub war_room_id: Option<uuid::Uuid>,
    pub war_room_status: Option<String>,
    pub war_room_severity: Option<String>,
```

Catatan: field selalu di-append setelah `type_id` agar urutan key JSON tetap terjaga (struct serialization memakai urutan deklarasi).

- [ ] **Step 4: Tambah subquery ke `LIST_SELECT_SQL`**

Di `issue_query.rs:70`, ganti ekor SELECT:

```
..., i.is_draft, i.archived_at, i.deleted_at, i.type_id FROM issues i LEFT JOIN states s ON s.id = i.state_id";
```

menjadi:

```
..., i.is_draft, i.archived_at, i.deleted_at, i.type_id, (SELECT wr.id FROM war_rooms wr WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_id, (SELECT wr.status FROM war_rooms wr WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_status, (SELECT wr.severity FROM war_rooms wr WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_severity FROM issues i LEFT JOIN states s ON s.id = i.state_id";
```

- [ ] **Step 5: Tambah subquery ke `DETAIL_SELECT_SQL`**

Di `issue_query.rs` (`DETAIL_SELECT_SQL`), ganti `     i.type_id \` pada baris terakhir sebelum `FROM` menjadi:

```rust
     i.type_id, \
     (SELECT wr.id FROM war_rooms wr \
       WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL \
       ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_id, \
     (SELECT wr.status FROM war_rooms wr \
       WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL \
       ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_status, \
     (SELECT wr.severity FROM war_rooms wr \
       WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL \
       ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_severity \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";
```

(Hapus `,` setelah `i.type_id` yang lama; item SELECT terakhir tidak boleh ada koma sebelum `FROM`.)

- [ ] **Step 6: Tambah subquery ke `list_by_ids`**

Di `issue_query.rs` fungsi `list_by_ids`, pada literal `format!(...)`, ganti baris:

```
        i.is_draft, i.archived_at, i.deleted_at, i.type_id \
        FROM issues i \
```

menjadi:

```
        i.is_draft, i.archived_at, i.deleted_at, i.type_id, \
        (SELECT wr.id FROM war_rooms wr \
          WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL \
          ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_id, \
        (SELECT wr.status FROM war_rooms wr \
          WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL \
          ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_status, \
        (SELECT wr.severity FROM war_rooms wr \
          WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL \
          ORDER BY (wr.status IN ('active','monitoring')) DESC, wr.created_at DESC LIMIT 1) AS war_room_severity \
        FROM issues i \
```

- [ ] **Step 7: Audit semua query yang mengisi kedua struct**

Run: `rg -n "query_as::<_, IssueListRow>|query_as::<_, IssueDetailRow>" apps/api-rs/crates/api/src`
Expected (semua harus memakai SELECT yang sudah diupdate — `LIST_SELECT_SQL`, `DETAIL_SELECT_SQL`, atau `format!` di `list_by_ids`):
`issue_query.rs` (list page, `list_by_ids`, `fetch_issue_row`), `v1/work_item.rs:323`, `work_item.rs:1689`, `draft.rs:1491`. Tidak boleh ada SELECT lain yang memberi makan struct ini.

- [ ] **Step 8: Jalankan test — harus LULUS**

Run: `cargo test -p api issue_list_tests` (dari `apps/api-rs`)
Expected: PASS semua, termasuk `list_and_detail_selects_expose_war_room_property`.

- [ ] **Step 9: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_common.rs apps/api-rs/crates/api/src/routes/issue_query.rs
git commit -m "feat(api): expose war room fields on issue list/detail"
```

---

### Task 3: Filter virtual `war_room` (`on`/`off`)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs`
- Test: `apps/api-rs/crates/api/src/routes/issue_query.rs` (modul `issue_list_tests`)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan ke `mod issue_list_tests`:

```rust
    #[test]
    fn complex_war_room_filter_is_allowlisted() {
        assert!(COMPLEX_FILTER_ALLOWLIST.contains(&"war_room"));
        assert!(COMPLEX_FILTER_ALLOWLIST.contains(&"war_room__exact"));
        assert!(COMPLEX_FILTER_ALLOWLIST.contains(&"war_room__in"));
    }

    #[test]
    fn complex_war_room_on_off_render_exists_checks() {
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1 FROM issues i");
        apply_complex_filter(&mut qb, &serde_json::json!({"and":[{"war_room__in":"on"}]})).unwrap();
        assert!(qb.sql().contains("EXISTS(SELECT 1 FROM war_rooms wr"), "{}", qb.sql());

        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1 FROM issues i");
        apply_complex_filter(&mut qb, &serde_json::json!({"and":[{"war_room__in":"off"}]})).unwrap();
        assert!(qb.sql().contains("NOT EXISTS(SELECT 1 FROM war_rooms wr"), "{}", qb.sql());
    }

    #[test]
    fn complex_war_room_both_values_match_all_and_unknown_400s() {
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1 FROM issues i");
        apply_complex_filter(&mut qb, &serde_json::json!({"and":[{"war_room__in":["on","off"]}]})).unwrap();
        assert!(qb.sql().contains("TRUE"), "{}", qb.sql());

        let err = apply_complex_leaf(
            &mut QueryBuilder::<Postgres>::new("SELECT 1"),
            "war_room",
            &serde_json::json!("maybe"),
        );
        assert!(err.is_err());
    }
```

- [ ] **Step 2: Jalankan test — harus GAGAL**

Run: `cargo test -p api issue_list_tests::complex_war_room` (dari `apps/api-rs`)
Expected: FAIL (field belum allowlisted / leaf belum ada).

- [ ] **Step 3: Tambah ke allowlist**

Di `COMPLEX_FILTER_ALLOWLIST` (`issue_query.rs`), tepat setelah `"priority__in",` tambahkan:

```rust
    "war_room",
    "war_room__exact",
    "war_room__in",
```

- [ ] **Step 4: Tambah helper leaf**

Di `issue_query.rs`, setelah fungsi `apply_complex_text_leaf`, tambahkan:

```rust
/// Leaf virtual khusus fork: `war_room` mencocokkan issue berdasarkan state
/// room turunan (bukan kolom). Nilai: `on` (ada room active/monitoring) dan
/// `off` (tidak ada). `in`/exact memperlakukan CSV pieces sebagai set: `on`
/// saja → EXISTS, `off` saja → NOT EXISTS, keduanya/kosong → TRUE (semua).
/// Suffix `range` dan nilai tak dikenal → 400 `invalid_filterset`.
fn apply_complex_war_room_leaf(
    qb: &mut QueryBuilder<Postgres>,
    pieces: &[String],
    suffix: &str,
) -> Result<(), ComplexFilterError> {
    if suffix == "range" {
        return Err(ComplexFilterError::invalid_filterset());
    }
    let mut wants_on = false;
    let mut wants_off = false;
    for piece in pieces {
        match piece.as_str() {
            "on" => wants_on = true,
            "off" => wants_off = true,
            "" => {}
            _ => return Err(ComplexFilterError::invalid_filterset()),
        }
    }
    if wants_on && !wants_off {
        qb.push("EXISTS(SELECT 1 FROM war_rooms wr WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL AND wr.status IN ('active','monitoring'))");
    } else if wants_off && !wants_on {
        qb.push("NOT EXISTS(SELECT 1 FROM war_rooms wr WHERE wr.primary_issue_id = i.id AND wr.deleted_at IS NULL AND wr.status IN ('active','monitoring'))");
    } else {
        qb.push("TRUE");
    }
    Ok(())
}
```

- [ ] **Step 5: Panggil helper di `apply_complex_leaf`**

Di match `apply_complex_leaf`, sebelum `_ => Err(ComplexFilterError::invalid_filterset()),` tambahkan:

```rust
        "war_room" => apply_complex_war_room_leaf(qb, &pieces, suffix),
```

- [ ] **Step 6: Jalankan test — harus LULUS**

Run: `cargo test -p api issue_list_tests` (dari `apps/api-rs`)
Expected: PASS semua.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_query.rs
git commit -m "feat(api): war_room complex filter (on/off)"
```

---

### Task 4: Test integrasi DB — payload + filter list issue

**Files:**

- Modify: `apps/api-rs/crates/api/tests/war_room_test.rs`

- [ ] **Step 1: Tulis test**

Tambahkan import di atas file (bareng import `api::routes::war_room::{...}`):

```rust
use api::routes::issue_query::{list as list_issues, ProjectIssuesQuery};
```

Lalu tambahkan test (di mana saja di file, pola `Scratch` yang ada):

```rust
#[tokio::test]
async fn issue_list_exposes_and_filters_war_room_property() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;

    let query = |filters: &str| ProjectIssuesQuery {
        filters: Some(filters.to_string()),
        ..Default::default()
    };

    // Tanpa room: kolom turunan null dan filter `off` match.
    let (status, Json(body)) = list_issues(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(query(r#"{"and":[{"war_room__in":"off"}]}"#)),
    )
    .await
    .expect("list off");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_count"], 1);
    let row = &body["results"][0];
    assert!(row["war_room_id"].is_null());
    assert!(row["war_room_status"].is_null());
    assert!(row["war_room_severity"].is_null());

    create_room(&st, &scratch, issue_id).await;

    // Ada room active: filter `on` match dan row membawa id/status/severity.
    let (_, Json(body)) = list_issues(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(query(r#"{"and":[{"war_room__in":"on"}]}"#)),
    )
    .await
    .expect("list on");
    assert_eq!(body["total_count"], 1);
    let row = &body["results"][0];
    assert_eq!(row["war_room_status"], "active");
    assert!(row["war_room_id"].is_string());
    assert!(row["war_room_severity"].is_string());

    let (_, Json(body)) = list_issues(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(query(r#"{"and":[{"war_room__in":"off"}]}"#)),
    )
    .await
    .expect("list off again");
    assert_eq!(body["total_count"], 0);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Jalankan test**

Run (dari `apps/api-rs`):
`DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test issue_list_exposes_and_filters_war_room_property -- --test-threads=1`
Expected: `test result: ok`.

- [ ] **Step 3: Regression suite war room**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1`
Expected: semua PASS (suite serial karena helper purge by prefix).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/war_room_test.rs
git commit -m "test(api): issue list war room property + filter"
```

---

## Phase 2 — Web data & filter

### Task 5: Field issue + key filter property

**Files:**

- Modify: `packages/types/src/issues/issue.ts`
- Modify: `packages/types/src/view-props.ts`

- [ ] **Step 1: Tambah field di `TBaseIssue`**

Di `packages/types/src/issues/issue.ts`, tambahkan import di atas file:

```ts
import type { TWarRoomSeverity, TWarRoomStatus } from "../war-room/core";
```

Di dalam `TBaseIssue` (setelah `type_id: string | null;`) tambahkan:

```ts
  // Fork: kolom turunan dari war room terbaru non-deleted (getWarRoomLink).
  war_room_id?: string | null;
  war_room_status?: TWarRoomStatus | null;
  war_room_severity?: TWarRoomSeverity | null;
```

- [ ] **Step 2: Tambah key filter property**

Di `packages/types/src/view-props.ts`, tambahkan `"war_room",` di `WORK_ITEM_FILTER_PROPERTY_KEYS` (setelah `"updated_at",`):

```ts
export const WORK_ITEM_FILTER_PROPERTY_KEYS = [
  "state_group",
  "priority",
  "start_date",
  "target_date",
  "assignee_id",
  "mention_id",
  "created_by_id",
  "subscriber_id",
  "label_id",
  "state_id",
  "type_id",
  "cycle_id",
  "module_id",
  "project_id",
  "created_at",
  "updated_at",
  "war_room",
] as const;
```

- [ ] **Step 3: Typecheck**

Run: `pnpm --filter=@plane/types check:types 2>/dev/null || pnpm check:types`
Expected: PASS (tidak ada error baru). Bila ada consumer yang pecah karena union key baru, perbaiki di task ini.

- [ ] **Step 4: Commit**

```bash
git add packages/types/src/issues/issue.ts packages/types/src/view-props.ts
git commit -m "feat(types): issue war room fields + war_room filter key"
```

---

### Task 6: Filter config "War room" + registrasi

**Files:**

- Create: `packages/utils/src/work-item-filters/configs/filters/war-room.ts`
- Modify: `packages/utils/src/work-item-filters/configs/filters/index.ts`
- Modify: `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`
- Modify: `packages/constants/src/issue/filter.ts`

- [ ] **Step 1: Buat config factory**

`packages/utils/src/work-item-filters/configs/filters/war-room.ts` (full file):

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
import type { TCreateFilterConfigParams, IFilterIconConfig, TCreateFilterConfig } from "../../../rich-filters";
import { createFilterConfig, getMultiSelectConfig, createOperatorConfigEntry } from "../../../rich-filters";

// ------------ War room filter (fork) ------------

export type TWarRoomFilterValue = "on" | "off";

const WAR_ROOM_FILTER_OPTIONS: { key: TWarRoomFilterValue; title: string }[] = [
  { key: "on", title: "On" },
  { key: "off", title: "Off" },
];

export type TCreateWarRoomFilterParams = TCreateFilterConfigParams & IFilterIconConfig<TWarRoomFilterValue>;

export const getWarRoomMultiSelectConfig = (
  params: TCreateWarRoomFilterParams,
  singleValueOperator: TSupportedOperators
) =>
  getMultiSelectConfig<{ key: TWarRoomFilterValue; title: string }, TWarRoomFilterValue, TWarRoomFilterValue>(
    {
      items: WAR_ROOM_FILTER_OPTIONS,
      getId: (option) => option.key,
      getLabel: (option) => option.title,
      getValue: (option) => option.key,
      getIconData: (option) => option.key,
    },
    {
      singleValueOperator,
      ...params,
    },
    {
      ...params,
    }
  );

export const getWarRoomFilterConfig =
  <P extends TFilterProperty>(key: P): TCreateFilterConfig<P, TCreateWarRoomFilterParams> =>
  (params: TCreateWarRoomFilterParams) =>
    createFilterConfig<P>({
      id: key,
      label: "War room",
      ...params,
      icon: params.filterIcon,
      supportedOperatorConfigsMap: new Map([
        createOperatorConfigEntry(COLLECTION_OPERATOR.IN, params, (updatedParams) =>
          getWarRoomMultiSelectConfig(updatedParams, EQUALITY_OPERATOR.EXACT)
        ),
      ]),
    });
```

- [ ] **Step 2: Export dari index**

Di `packages/utils/src/work-item-filters/configs/filters/index.ts`, tambahkan di urutan alfabetis:

```ts
export * from "./war-room";
```

- [ ] **Step 3: Registrasi di hook config**

Di `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`:

a. Tambah `AlertOctagonOutline` ke import `@makeplane/propel/icons` (blok import pertama).

b. Tambah `getWarRoomFilterConfig,` ke import dari `@plane/utils` (blok alfabetis, setelah `getUpdatedAtFilterConfig`).

c. Sebelum `return {` (dekat `updatedAtFilterConfig`), tambahkan:

```tsx
// war room / incident bridge filter config (fork)
const warRoomFilterConfig = useMemo(
  () =>
    getWarRoomFilterConfig<TWorkItemFilterProperty>("war_room")({
      isEnabled: isFilterEnabled("war_room"),
      filterIcon: AlertOctagonOutline,
      ...operatorConfigs,
    }),
  [isFilterEnabled, operatorConfigs]
);
```

d. Tambah `warRoomFilterConfig` ke array `configs` dan `war_room: warRoomFilterConfig` ke `configMap`.

- [ ] **Step 4: Tampilkan di Add-filter Work Items**

Di `packages/constants/src/issue/filter.ts`, di `ISSUE_DISPLAY_FILTERS_BY_PAGE.issues.filters`, tambahkan `"war_room",` (setelah `"target_date",`).

- [ ] **Step 5: Typecheck + lint**

Run: `pnpm check:types && pnpm check:lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/utils/src/work-item-filters/configs/filters/war-room.ts packages/utils/src/work-item-filters/configs/filters/index.ts apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx packages/constants/src/issue/filter.ts
git commit -m "feat(web): war room rich filter config"
```

---

### Task 7: Preset `/issues?war_room=on|off` di ProjectLayoutRoot

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx`

- [ ] **Step 1: Tambah import**

```tsx
import { useEffect, useRef } from "react";
import { useParams, useSearchParams } from "next/navigation";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkItemFilters } from "@/hooks/store/work-item-filters/use-work-item-filters";
```

- [ ] **Step 2: Tambah effect preset**

Di body `ProjectLayoutRoot`, setelah blok `useSWR(...)` dan sebelum `if (!workspaceSlug || !projectId || !workItemFilters) return <></>;`, tambahkan:

```tsx
// Preset nav "Incident bridge": `/issues?war_room=on|off` meng-apply rich
// filter sekali, lalu param di-strip supaya URL bersih dan bisa dibagikan.
const searchParams = useSearchParams();
const router = useAppRouter();
const { updateFilterExpressionFromConditions } = useWorkItemFilters();
const warRoomPreset = searchParams.get("war_room");
const presetApplied = useRef(false);

useEffect(() => {
  if (presetApplied.current || !workspaceSlug || !projectId || !workItemFilters) return;
  if (warRoomPreset !== "on" && warRoomPreset !== "off") return;
  presetApplied.current = true;
  void updateFilterExpressionFromConditions(
    EIssuesStoreType.PROJECT,
    projectId,
    [{ property: "war_room", operator: "in", value: [warRoomPreset] }],
    issuesFilter?.updateFilterExpression.bind(issuesFilter, workspaceSlug, projectId)
  ).finally(() => {
    router.replace(`/${workspaceSlug}/projects/${projectId}/issues`);
  });
}, [
  warRoomPreset,
  workItemFilters,
  workspaceSlug,
  projectId,
  issuesFilter,
  updateFilterExpressionFromConditions,
  router,
]);
```

- [ ] **Step 3: Typecheck**

Run: `pnpm check:types`
Expected: PASS. Bila `value: [warRoomPreset]` dianggap bukan tipe yang benar, cocokkan dengan `TWorkItemFilterCondition` (array string diperbolehkan).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx
git commit -m "feat(web): apply war_room preset filter from query param"
```

---

## Phase 3 — UI property toggle

### Task 8: i18n keys (en)

**Files:**

- Modify: `packages/i18n/src/locales/en/war-room.json`

- [ ] **Step 1: Tambah keys**

Di objek `war_room` pada `packages/i18n/src/locales/en/war-room.json`, tambahkan setelah `"open": "Open bridge",`:

```json
    "property_label": "War room",
    "toggle": {
      "turn_on": "Start incident bridge",
      "turn_off": "Resolve bridge"
    },
```

Catatan: locale lain mengikuti fallback en; lokalisasi menyusul di luar scope plan ini (gunakan skill `translate` bila diminta).

- [ ] **Step 2: Validasi JSON + i18n test**

Run: `node -e "JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/war-room.json','utf8'))" && pnpm --filter=@plane/i18n test 2>/dev/null || true`
Expected: tidak ada error parse.

- [ ] **Step 3: Commit**

```bash
git add packages/i18n/src/locales/en/war-room.json
git commit -m "feat(i18n): war room property toggle strings"
```

---

### Task 9: Store `toggleWarRoom` + unit test

**Files:**

- Modify: `apps/web/core/store/war-room.store.ts`
- Modify: `apps/web/core/store/war-room.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `apps/web/core/store/war-room.store.test.ts`:

a. Tambah `TIssue` ke import type dari `@plane/types`.

b. Tambah helper setelah `makeRoom`:

```ts
const makeIssue = (overrides: Partial<TIssue> = {}): TIssue =>
  ({
    id: "issue-1",
    name: "Checkout latency spike",
    priority: "urgent",
    type_id: "type-1",
    project_id: "project-1",
    sequence_id: 102,
    sort_order: 0,
    state_id: null,
    label_ids: [],
    assignee_ids: [],
    estimate_point: null,
    sub_issues_count: 0,
    attachment_count: 0,
    link_count: 0,
    parent_id: null,
    cycle_id: null,
    module_ids: [],
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    start_date: null,
    target_date: null,
    completed_at: null,
    archived_at: null,
    created_by: "user-1",
    updated_by: "user-1",
    is_draft: false,
    ...overrides,
  }) as TIssue;
```

c. Update `makeStore` supaya punya `rootStore` stub (dibutuhkan refetch issue setelah toggle):

```ts
const makeStore = () => {
  const store = new WarRoomStore({} as never);
  const refetchIssues = vi.fn();
  const warRoomService = {
    getWarRooms: vi.fn(async () => [makeRoom()]),
    getWarRoomSummary: vi.fn(async (): Promise<IWarRoomSummary> => ({ active: 1, sev1_2: 0, resolved_7d: 0 })),
    getWarRoom: vi.fn(async () => makeDetail()),
    createWarRoom: vi.fn(async () => makeDetail({ id: "room-2", sequence_id: 2 })),
    updateWarRoom: vi.fn(async () => makeDetail({ name: "Updated room" })),
    deleteWarRoom: vi.fn(async () => undefined),
    addServices: vi.fn(async () => ({ linked: 1 })),
    removeService: vi.fn(async () => undefined),
    addIssues: vi.fn(async () => ({ linked: 1 })),
    removeIssue: vi.fn(async () => undefined),
    createParticipant: vi.fn(async () => makeParticipant({ id: "participant-2", member_id: "user-2" })),
    updateParticipant: vi.fn(async () => makeParticipant({ id: "participant-1", role: "comms" })),
    deleteParticipant: vi.fn(async () => undefined),
    createRunbookItem: vi.fn(async () => makeRunbookItem({ id: "item-2" })),
    updateRunbookItem: vi.fn(async () => makeRunbookItem({ id: "item-1", is_done: true })),
    deleteRunbookItem: vi.fn(async () => undefined),
    getMessages: vi.fn(async () => [makeMessage()]),
    createMessage: vi.fn(async () => makeMessage()),
    updateMessage: vi.fn(async () => makeMessage({ body: "edited" })),
    deleteMessage: vi.fn(async () => undefined),
    getEvents: vi.fn(async () => [makeEvent()]),
  };
  (store as unknown as { warRoomService: typeof warRoomService }).warRoomService = warRoomService;
  (store as unknown as { rootStore: unknown }).rootStore = {
    issue: { projectIssues: { fetchIssuesWithExistingPagination: refetchIssues } },
  };
  return { store, warRoomService, refetchIssues };
};
```

d. Tambah describe baru di akhir file:

```ts
describe("WarRoomStore.toggleWarRoom", () => {
  it("resolves the active room when toggled off", async () => {
    const { store, warRoomService, refetchIssues } = makeStore();

    await store.toggleWarRoom(
      "acme",
      "project-1",
      makeIssue({ war_room_id: "room-1", war_room_status: "active", war_room_severity: "sev1" })
    );

    expect(warRoomService.updateWarRoom).toHaveBeenCalledWith("acme", "project-1", "room-1", { status: "resolved" });
    expect(warRoomService.createWarRoom).not.toHaveBeenCalled();
    expect(refetchIssues).toHaveBeenCalledWith("acme", "project-1", "mutation");
  });

  it("reopens the resolved room when toggled on", async () => {
    const { store, warRoomService } = makeStore();

    await store.toggleWarRoom(
      "acme",
      "project-1",
      makeIssue({ war_room_id: "room-1", war_room_status: "resolved", war_room_severity: "sev2" })
    );

    expect(warRoomService.updateWarRoom).toHaveBeenCalledWith("acme", "project-1", "room-1", { status: "active" });
  });

  it("creates a room when the issue has none", async () => {
    const { store, warRoomService } = makeStore();

    await store.toggleWarRoom("acme", "project-1", makeIssue({ war_room_id: null }), { serviceIds: ["service-1"] });

    expect(warRoomService.createWarRoom).toHaveBeenCalledWith("acme", "project-1", {
      primary_issue_id: "issue-1",
      name: "Checkout latency spike",
      severity: "sev1",
      service_ids: ["service-1"],
    });
  });

  it("adopts the existing room on a 409 duplicate", async () => {
    const { store, warRoomService } = makeStore();
    const duplicate = Object.assign(new Error("active_war_room_exists"), {
      error: "active_war_room_exists",
      war_room_id: "room-9",
    });
    warRoomService.createWarRoom.mockRejectedValueOnce(duplicate);

    await store.toggleWarRoom("acme", "project-1", makeIssue({ war_room_id: null }));

    expect(warRoomService.getWarRoom).toHaveBeenCalledWith("acme", "project-1", "room-9");
  });
});
```

- [ ] **Step 2: Jalankan test — harus GAGAL**

Run: `pnpm --filter=web test -- war-room.store`
Expected: FAIL (`toggleWarRoom` belum ada).

- [ ] **Step 3: Implementasi store**

Di `apps/web/core/store/war-room.store.ts`:

a. Import `severityFromPriority` bareng helper:
`import { isActiveWarRoomStatus, severityFromPriority } from "@/services/war-room.helpers";`
dan tambah `TIssue` ke import type dari `@plane/types`.

b. Di `IWarRoomStore` tambahkan setelah `updateWarRoom`:

```ts
toggleWarRoom: (workspaceSlug: string, projectId: string, issue: TIssue, options?: { serviceIds?: string[] }) =>
  Promise<IWarRoom>;
```

c. Di `makeObservable` tambahkan `toggleWarRoom: action,` (setelah `updateWarRoom: action,`).

d. Implementasi setelah `updateWarRoom`:

```ts
toggleWarRoom = async (
  workspaceSlug: string,
  projectId: string,
  issue: TIssue,
  options?: { serviceIds?: string[] }
) => {
  const roomId = issue.war_room_id ?? null;
  const status = issue.war_room_status ?? null;
  let room: IWarRoom;
  if (roomId && status && isActiveWarRoomStatus(status)) {
    room = await this.updateWarRoom(workspaceSlug, projectId, roomId, { status: "resolved" });
  } else if (roomId && status === "resolved") {
    room = await this.updateWarRoom(workspaceSlug, projectId, roomId, { status: "active" });
  } else {
    try {
      room = await this.createWarRoom(workspaceSlug, projectId, {
        primary_issue_id: issue.id,
        name: issue.name,
        severity: severityFromPriority(issue.priority) ?? undefined,
        service_ids: options?.serviceIds ?? [],
      });
    } catch (error) {
      const duplicate = error as { error?: string; war_room_id?: string };
      if (duplicate?.error === "active_war_room_exists" && duplicate.war_room_id) {
        const existing = await this.fetchWarRoomDetail(workspaceSlug, projectId, duplicate.war_room_id);
        if (!existing) throw error;
        room = existing;
      } else {
        throw error;
      }
    }
  }
  // Kolom turunan war_room_* datang dari server; refetch list issue aktif.
  this.rootStore.issue.projectIssues.fetchIssuesWithExistingPagination(workspaceSlug, projectId, "mutation");
  return room;
};
```

- [ ] **Step 4: Jalankan test — harus LULUS**

Run: `pnpm --filter=web test -- war-room.store`
Expected: PASS (test lama + 4 test baru).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/war-room.store.ts apps/web/core/store/war-room.store.test.ts
git commit -m "feat(web): war room toggle store action"
```

---

### Task 10: Komponen `WarRoomProperty` + integrasi detail/peek

**Files:**

- Create: `apps/web/core/components/war-rooms/war-room-property.tsx`
- Modify: `apps/web/core/components/war-rooms/index.ts`
- Modify: `apps/web/core/components/issues/issue-detail/sidebar.tsx`
- Modify: `apps/web/core/components/issues/peek-overview/properties.tsx`

- [ ] **Step 1: Buat komponen**

`apps/web/core/components/war-rooms/war-room-property.tsx` (full file):

```tsx
import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { WAR_ROOM_STATUS_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TIssue } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { isActiveWarRoomStatus } from "@/services/war-room.helpers";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  workspaceSlug: string;
  projectId: string;
  issue: TIssue;
  disabled?: boolean;
  readOnly?: boolean;
};

export const WarRoomProperty = observer(function WarRoomProperty(props: Props) {
  const { workspaceSlug, projectId, issue, disabled = false, readOnly = false } = props;
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { toggleWarRoom } = useWarRoom();
  const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();
  const { fetchedMap, workItemLinkMap } = useService();
  // states
  const [isToggling, setIsToggling] = useState(false);

  useEffect(() => {
    if (workItemTypes) return;
    void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
  }, [workItemTypes, workspaceSlug, fetchWorkItemTypes]);

  const issueType = workItemTypes?.find((type) => type.id === issue.type_id);
  const isIncident = issueType?.name.toLowerCase() === "incident";
  if (!isIncident || issue.archived_at) return null;

  const roomStatus = issue.war_room_status ?? null;
  const isOn = roomStatus ? isActiveWarRoomStatus(roomStatus) : false;
  const isEditable = !disabled && !readOnly && !issue.archived_at;
  const statusConfig = roomStatus ? WAR_ROOM_STATUS_CONFIG[roomStatus] : null;

  const serviceIds = fetchedMap[projectId]
    ? Object.values(workItemLinkMap)
        .filter((link) => link.issue_id === issue.id && link.project_id === projectId)
        .map((link) => link.service_id)
    : [];

  const handleToggle = async () => {
    if (!isEditable || isToggling) return;
    setIsToggling(true);
    try {
      await toggleWarRoom(workspaceSlug, projectId, issue, { serviceIds });
    } catch (error) {
      const apiError = error as { error?: string; war_room_id?: string };
      if (apiError?.error !== "active_war_room_exists") {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: t("toast.error"),
          message: t("war_room.errors.generic"),
        });
      }
    } finally {
      setIsToggling(false);
    }
  };

  const handleOpenRoom = () => {
    if (!issue.war_room_id) return;
    router.push(getWarRoomLink(workspaceSlug, projectId, issue.war_room_id));
  };

  return (
    <div className="flex items-center gap-1.5">
      <Tooltip label={isOn ? t("war_room.toggle.turn_off") : t("war_room.toggle.turn_on")} disabled={!isEditable}>
        <button
          type="button"
          role="switch"
          aria-checked={isOn}
          aria-label={t("war_room.property_label")}
          disabled={!isEditable || isToggling}
          onClick={handleToggle}
          className={cn(
            "relative inline-flex h-4 w-7 flex-shrink-0 items-center rounded-full transition-colors",
            isOn ? "bg-danger-primary" : "bg-layer-3",
            !isEditable && "cursor-not-allowed opacity-60"
          )}
        >
          <span
            className={cn(
              "inline-block size-3 rounded-full bg-white transition-transform",
              isOn ? "translate-x-3.5" : "translate-x-0.5"
            )}
          />
        </button>
      </Tooltip>
      {isOn && roomStatus && statusConfig && issue.war_room_severity && (
        <span className={cn("rounded-sm px-1.5 py-0.5 text-11 font-medium", statusConfig.pill)}>
          {t(`war_room.severity_values.${issue.war_room_severity}`)} · {t(statusConfig.label_key)}
        </span>
      )}
      {isOn && issue.war_room_id && !readOnly && (
        <button
          type="button"
          onClick={handleOpenRoom}
          className="text-11 font-medium text-accent-primary hover:underline"
        >
          {t("war_room.open")}
        </button>
      )}
      {readOnly && !isOn && <span className="text-11 text-tertiary">—</span>}
    </div>
  );
});
```

- [ ] **Step 2: Export komponen**

Di `apps/web/core/components/war-rooms/index.ts` tambahkan:

```ts
export * from "./war-room-property";
```

- [ ] **Step 3: Integrasi sidebar detail**

Di `apps/web/core/components/issues/issue-detail/sidebar.tsx`:

a. Tambah `AlertOctagonOutline` ke import icons yang sudah ada.

b. Tambah import komponen: `import { WarRoomProperty } from "@/components/war-rooms/war-room-property";`

c. Setelah blok `ServerOutline`/`ServiceSelect` (property Service), tambahkan (dibungkus guard karena `issue` bisa undefined):

```tsx
{
  issue && (
    <SidebarPropertyListItem icon={AlertOctagonOutline} label={t("war_room.title")}>
      <WarRoomProperty workspaceSlug={workspaceSlug} projectId={projectId} issue={issue} disabled={!isEditable} />
    </SidebarPropertyListItem>
  );
}
```

- [ ] **Step 4: Integrasi peek properties**

Di `apps/web/core/components/issues/peek-overview/properties.tsx`, lakukan hal sama (icon import, komponen import, blok row setelah Service):

```tsx
<SidebarPropertyListItem icon={AlertOctagonOutline} label={t("war_room.title")}>
  <WarRoomProperty workspaceSlug={workspaceSlug} projectId={projectId} issue={issue} disabled={disabled} />
</SidebarPropertyListItem>
```

- [ ] **Step 5: Typecheck**

Run: `pnpm check:types`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/war-rooms/war-room-property.tsx apps/web/core/components/war-rooms/index.ts apps/web/core/components/issues/issue-detail/sidebar.tsx apps/web/core/components/issues/peek-overview/properties.tsx
git commit -m "feat(web): war room toggle property in issue detail + peek"
```

---

### Task 11: Display property wiring + list/kanban + spreadsheet

**Files:**

- Modify: `packages/types/src/view-props.ts`, `packages/constants/src/issue/common.ts`, `packages/utils/src/work-item/base.ts`
- Create: `apps/web/core/components/issues/issue-layouts/spreadsheet/columns/war-room-column.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/utils.tsx`, `.../spreadsheet/columns/index.ts`, `.../properties/all-properties.tsx`

- [ ] **Step 1: Tambah key display property**

a. `packages/types/src/view-props.ts` — di `IIssueDisplayProperties`, setelah `issue_type?: boolean;`:

```ts
  war_room?: boolean;
```

b. `packages/constants/src/issue/common.ts` — di `ISSUE_DISPLAY_PROPERTIES_KEYS` tambah `"war_room",` (setelah `"issue_type",`) dan di `ISSUE_DISPLAY_PROPERTIES` tambah:

```ts
  {
    key: "war_room",
    titleTranslationKey: "war_room.property_label",
  },
```

c. `packages/constants/src/issue/common.ts` — di `SPREADSHEET_PROPERTY_LIST` tambah `"war_room",` (setelah `"priority",`) dan di `SPREADSHEET_PROPERTY_DETAILS` tambah (sort fallback ke tanggal karena war_room tidak server-sortable):

```ts
  war_room: {
    i18n_title: "war_room.property_label",
    ascendingOrderKey: "-created_at",
    ascendingOrderTitle: "New",
    descendingOrderKey: "created_at",
    descendingOrderTitle: "Old",
    icon: "ContrastIcon",
  },
```

d. `packages/utils/src/work-item/base.ts` — di `getComputedDisplayProperties`, tambah (default **off**, beda dari property lain):

```ts
  war_room: displayProperties?.war_room ?? false,
```

- [ ] **Step 2: Buat kolom spreadsheet**

`apps/web/core/components/issues/issue-layouts/spreadsheet/columns/war-room-column.tsx` (full file):

```tsx
import { useParams } from "next/navigation";
import { observer } from "mobx-react";
// plane imports
import type { TIssue } from "@plane/types";
// components
import { WarRoomProperty } from "@/components/war-rooms/war-room-property";

type Props = {
  issue: TIssue;
  onClose: () => void;
  onChange: (issue: TIssue, data: Partial<TIssue>, updates: any) => void;
  disabled: boolean;
};

export const SpreadsheetWarRoomColumn = observer(function SpreadsheetWarRoomColumn(props: Props) {
  const { issue, disabled } = props;
  const { workspaceSlug } = useParams();
  const workspaceSlugString = workspaceSlug?.toString() ?? "";

  return (
    <div className="flex h-11 items-center border-b-[0.5px] border-subtle px-page-x">
      <WarRoomProperty
        workspaceSlug={workspaceSlugString}
        projectId={issue.project_id ?? ""}
        issue={issue}
        disabled={disabled}
      />
    </div>
  );
});
```

- [ ] **Step 3: Registrasi kolom spreadsheet**

a. `apps/web/core/components/issues/issue-layouts/spreadsheet/columns/index.ts` — tambah `export * from "./war-room-column";` (alfabetis).

b. `apps/web/core/components/issues/issue-layouts/utils.tsx` — import `SpreadsheetWarRoomColumn` dan tambah ke `SPREADSHEET_COLUMNS`:

```tsx
  war_room: SpreadsheetWarRoomColumn,
```

- [ ] **Step 4: Render di list + kanban (`IssueProperties`)**

Di `apps/web/core/components/issues/issue-layouts/properties/all-properties.tsx`:

a. Import komponen: `import { WarRoomProperty } from "@/components/war-rooms/war-room-property";`

b. Setelah blok `{/* priority */}` (`WithDisplayPropertiesHOC` priority), tambahkan:

```tsx
{
  /* war room (Incident bridge) */
}
<WithDisplayPropertiesHOC displayProperties={displayProperties} displayPropertyKey="war_room">
  {/* oxlint-disable-next-line jsx-a11y/click-events-have-key-events oxlint-disable-next-line jsx-a11y/no-static-element-interactions */}
  <div className="h-5" onFocus={handleEventPropagation} onClick={handleEventPropagation}>
    <WarRoomProperty
      workspaceSlug={workspaceSlug?.toString() ?? ""}
      projectId={issue.project_id ?? ""}
      issue={issue}
      disabled={isReadOnly}
      readOnly={activeLayout === "Kanban"}
    />
  </div>
</WithDisplayPropertiesHOC>;
```

(`workspaceSlug` sudah tersedia dari `useParams()` di file ini; `activeLayout` adalah prop — pastikan ikut didestruktur di body komponen: tambah `activeLayout` ke destructuring props yang ada.)

- [ ] **Step 5: Typecheck + test web**

Run: `pnpm check:types && pnpm --filter=web test`
Expected: PASS. Bila mapped type `SPREADSHEET_COLUMNS`/`SPREADSHEET_PROPERTY_DETAILS` mewajibkan key lain, error TS akan menunjuk persis entry yang kurang — tambahkan mengikuti pola di atas.

- [ ] **Step 6: Commit**

```bash
git add packages/types/src/view-props.ts packages/constants/src/issue/common.ts packages/utils/src/work-item/base.ts apps/web/core/components/issues/issue-layouts/spreadsheet/columns/war-room-column.tsx apps/web/core/components/issues/issue-layouts/spreadsheet/columns/index.ts apps/web/core/components/issues/issue-layouts/utils.tsx apps/web/core/components/issues/issue-layouts/properties/all-properties.tsx
git commit -m "feat(web): war room display property (list, kanban, spreadsheet)"
```

---

## Phase 4 — Nav, redirect & pembersihan

### Task 12: Nav href + redirect route + link fallback

**Files:**

- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx`
- Modify: `apps/web/core/components/navigation/use-navigation-items.ts`
- Modify: `apps/web/core/components/navigation/tab-navigation-utils.ts`
- Modify: `packages/constants/src/war-room.ts`
- Create: `apps/web/app/routes/redirects/core/war-rooms.tsx`
- Modify: `apps/web/app/routes/core.ts`

- [ ] **Step 1: Ubah href nav**

a. `project-navigation.tsx` — item `war-rooms`, ganti `href`:

```tsx
        href: `/${workspaceSlug}/projects/${projectId}/issues?war_room=on`,
```

b. `use-navigation-items.ts` — item `war-rooms`, ganti `href` dengan nilai yang sama. Ganti juga `name: "War rooms"` → `name: "Incident bridge"` (konsisten dengan label i18n).

c. `tab-navigation-utils.ts` — `getTabUrl`: `war_rooms: `${baseUrl}/issues?war_room=on`,`.

- [ ] **Step 2: Update fallback link**

`packages/constants/src/war-room.ts` — `getWarRoomLink` tanpa id kini menunjuk ke daftar terfilter:

```ts
export const getWarRoomLink = (workspaceSlug: string, projectId: string, warRoomId?: string): string =>
  warRoomId
    ? `/${workspaceSlug}/projects/${projectId}/war-rooms/${warRoomId}`
    : `/${workspaceSlug}/projects/${projectId}/issues?war_room=on`;
```

- [ ] **Step 3: Buat redirect route**

`apps/web/app/routes/redirects/core/war-rooms.tsx` (full file, pola `inbox.tsx`):

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/war-rooms";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const { workspaceSlug, projectId } = params;
  throw redirect(`/${workspaceSlug}/projects/${projectId}/issues?war_room=on`);
};

export default function WarRoomsRedirect() {
  return null;
}
```

- [ ] **Step 4: Registrasi route + hapus route list lama**

Di `apps/web/app/routes/core.ts`:

a. Hapus blok:

```ts
          // War Rooms List
          layout("./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/layout.tsx", [
            route(
              ":workspaceSlug/projects/:projectId/war-rooms",
              "./(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/page.tsx"
            ),
          ]),
```

b. Di blok "Legacy URL redirects" (bareng `inbox`), tambahkan:

```ts
  // War rooms list redirect: /:workspaceSlug/projects/:projectId/war-rooms → filtered issues
  route(":workspaceSlug/projects/:projectId/war-rooms", "routes/redirects/core/war-rooms.tsx"),
```

- [ ] **Step 5: Hapus file route list**

```bash
git rm -r "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)"
```

- [ ] **Step 6: Typecheck**

Run: `pnpm check:types`
Expected: PASS (route types `+types/war-rooms` tergenerasi saat typecheck/build).

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/workspace/sidebar/project-navigation.tsx apps/web/core/components/navigation/use-navigation-items.ts apps/web/core/components/navigation/tab-navigation-utils.ts packages/constants/src/war-room.ts apps/web/app/routes/redirects/core/war-rooms.tsx apps/web/app/routes/core.ts
git commit -m "feat(web): bridge nav preset + war-rooms list redirect"
```

---

### Task 13: Hapus UI list lama (komponen + modul)

**Files (delete):**

- `apps/web/core/components/war-rooms/list/**` (board, board-row, list-view, summary-chips, search-input, load-error, view-header)
- `apps/web/core/components/war-rooms/create/create-war-room-modal.tsx`
- `apps/web/core/components/war-rooms/issue-war-room-button.tsx`
- Modify: `apps/web/core/components/war-rooms/index.ts`
- Modify: `apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`
- Modify: `apps/web/core/components/war-rooms/room/header/root.tsx` dan `.../room/root.tsx` (fallback link — sudah otomatis lewat `getWarRoomLink` tanpa id, verifikasi tidak ada path `/war-rooms` literal tersisa)

- [ ] **Step 1: Hapus file**

```bash
git rm -r apps/web/core/components/war-rooms/list apps/web/core/components/war-rooms/create
git rm apps/web/core/components/war-rooms/issue-war-room-button.tsx
```

- [ ] **Step 2: Bersihkan export**

`apps/web/core/components/war-rooms/index.ts` — sisakan yang masih hidup:

```ts
export * from "./room/root";
export * from "./war-room-property";
```

- [ ] **Step 3: Lepas tombol lama di quick actions**

`issue-detail-quick-actions.tsx` — hapus import `IssueWarRoomButton` dan baris penggunaannya `<IssueWarRoomButton ... />`.

- [ ] **Step 4: Audit referensi tersisa**

Run: `rg -n "war-rooms\b|WarRoomsListView|CreateWarRoomModal|WarRoomSearchInput|WarRoomsBoard|issue-war-room-button" apps/web --glob '!app/routes/core.ts' --glob '!app/routes/redirects/**'`
Expected: hanya `getWarRoomLink` (detail), route detail, dan komentar — tidak ada import file yang sudah dihapus.

- [ ] **Step 5: Typecheck + lint**

Run: `pnpm check:types && pnpm check:lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/war-rooms/index.ts apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx
git commit -m "refactor(web): remove war rooms list UI"
```

---

### Task 14: Prune store/service/filter-store list-only

**Files:**

- Modify: `apps/web/core/store/war-room.store.ts`, `apps/web/core/store/war-room.store.test.ts`
- Modify: `apps/web/core/services/war-room.service.ts`
- Delete: `apps/web/core/store/war-room_filter.store.ts`, `apps/web/core/hooks/store/use-war-room-filter.ts`
- Modify: `apps/web/core/store/root.store.ts`
- Modify (bila ada referensi): `packages/constants/src/war-room.ts` (`WAR_ROOM_STATUS_TABS`), `packages/types/src/war-room/filters.ts` (`TWarRoomStatusTab`)

- [ ] **Step 1: Hapus service list-only**

Di `war-room.service.ts` hapus `getWarRooms` dan `getWarRoomSummary` (create/update/detail/links/participants/runbook/messages/events tetap).

- [ ] **Step 2: Hapus store list-only**

Di `war-room.store.ts` hapus: `fetchWarRooms`, `fetchWarRoomSummary`, `getProjectWarRoomIds`, `getProjectSummary`, `getActiveWarRoomByIssue`, field `warRoomIdsMap`, `summaryMap`, `fetchedMap`, `errorMap`, `loader` (verifikasi dengan grep bahwa tidak ada sisa pemakai), beserta entri `makeObservable`-nya. Pertahankan `warRoomMap` + `getWarRoomById` (dipakai `updateWarRoom`) dan seluruh logic detail/chat/participants/runbook/events.

- [ ] **Step 3: Hapus filter store**

```bash
git rm apps/web/core/store/war-room_filter.store.ts apps/web/core/hooks/store/use-war-room-filter.ts
```

Di `root.store.ts` hapus import, field `warRoomFilter`, dan instansiasi `this.warRoomFilter = new WarRoomFilterStore(this);`.

- [ ] **Step 4: Hapus test list-only**

Di `war-room.store.test.ts` hapus describe `fetchWarRooms`/`fetchWarRoomSummary` dan mock yang tak lagi dipakai (`getWarRooms`, `getWarRoomSummary`). Pertahankan test detail & toggle.

- [ ] **Step 5: Bersihkan konstanta/tab**

Hapus `WAR_ROOM_STATUS_TABS` di `packages/constants/src/war-room.ts` dan `TWarRoomStatusTab` (beserta file `packages/types/src/war-room/filters.ts` bila tidak ada pemakai lain). Cek: `rg -n "WAR_ROOM_STATUS_TABS|TWarRoomStatusTab|warRoomFilter|useWarRoomFilter" apps packages`
Expected: 0 referensi.

- [ ] **Step 6: Jalankan test + typecheck**

Run: `pnpm --filter=web test && pnpm check:types && pnpm check:lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A apps/web/core/store apps/web/core/services/war-room.service.ts apps/web/core/hooks/store packages/constants/src/war-room.ts packages/types/src/war-room
git commit -m "refactor(web): drop war rooms list store/service surface"
```

---

## Phase 5 — Docs & verifikasi

### Task 15: Update dokumentasi fitur

**Files:**

- Modify: `docs/features/war-rooms.md`
- Modify: `docs/superpowers/plans/2026-09-30-war-room-phase-3-web-list-create.md`

- [ ] **Step 1: Update `docs/features/war-rooms.md`**

Ubah minimal:

- Route: list lama dihapus → `:workspaceSlug/projects/:projectId/issues?war_room=on` (preset filter) + redirect dari `/war-rooms`; detail tetap.
- Current State: hapus list/board/create modal; tambah property toggle "War room" (auto-create/resolve/reopen), kolom list & spreadsheet, chip kanban, filter rich `war_room`.
- Actions: ganti "Create via toolbar" → "Toggle ON (auto-create, nama=judul, severity=priority)"; "Toggle OFF = resolve"; "Reopen = toggle ON saat resolved".
- Filters/Search/Sort: ganti tabs/summary dengan filter `War room: On/Off` + nav shortcut.
- Changelog: tambah baris `2026-10-09 — War room jadi property Work Items; list terpisah dihapus.`

- [ ] **Step 2: Tandai plan lama superseded**

Di baris atas `docs/superpowers/plans/2026-09-30-war-room-phase-3-web-list-create.md` tambahkan:

```md
> **Superseded (2026-10-09):** halaman list + create modal dihapus; bridge kini property + filter di Work Items (`docs/superpowers/plans/2026-10-09-war-room-work-item-property.md`).
```

- [ ] **Step 3: Commit**

```bash
git add docs/features/war-rooms.md docs/superpowers/plans/2026-09-30-war-room-phase-3-web-list-create.md
git commit -m "docs: war room as work item property"
```

---

### Task 16: Verifikasi penuh + smoke

- [ ] **Step 1: Checks monorepo**

Run: `pnpm check && pnpm --filter=web test`
Expected: format/lint/types PASS, semua test web PASS.

- [ ] **Step 2: Rust**

Run (dari `apps/api-rs`):

```bash
cargo check -p api
cargo test -p api issue_list_tests
cargo test -p common --test migration_versions_test
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1
```

Expected: semua PASS.

- [ ] **Step 3: Rebuild backend + live (per `AGENTS.md`)**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
# poll sampai link selesai (LTO bisa 10+ menit, tanpa output)
curl http://localhost:8000/health
systemctl --user restart plane-live.service
curl http://localhost:3100/live/health/
```

Expected: health 200 (cold start live ~8 detik).

- [ ] **Step 4: Build web + restart prod**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

- [ ] **Step 5: Smoke manual (tunnel demo)**

- Buka work item Incident → toggle "War room" ON → room terbuat otomatis (nama = judul, severity = priority), chip + link "Open bridge" muncul.
- Di room: Resolve → kembali ke work item → toggle OFF & chip hilang.
- Toggle ON lagi → room resolved yang sama di-reopen (tidak membuat room baru).
- List Work Items: aktifkan kolom "War room" di Display → toggle bisa diklik; baris non-Incident tidak menampilkan apa-apa.
- Filter "War room: On" / "Off" (termasuk halaman 2+ pagination) benar.
- Nav "Incident bridge" (sidebar + tab) → `/issues` dengan chip filter ter-apply, URL param `war_room` ter-strip.
- URL lama `/{ws}/projects/{id}/war-rooms` → redirect ke `.../issues?war_room=on`.
- Notifikasi mention war room masih membuka `/war-rooms/:id` (detail route hidup).
- Room archived + toggle ON → membuat room baru (bukan reopen).

- [ ] **Step 6: Commit sisa (bila ada)**

```bash
git status --short
git add <file-yang-tersisa>
git commit -m "chore(web): war room property smoke fixes"
```

---

## Catatan eksekusi

- Jalankan Task 1–4 dulu dan pastikan hijau sebelum web, karena Task 5+ mengandalkan payload `war_room_*`.
- `git rm` Task 12–14 meninggalkan working tree bersih hanya bila tidak ada perubahan lain yang belum di-commit di area yang sama; cek `git status` sebelum tiap commit.
- Jangan menyentuh `apps/live`; chat realtime tidak berubah.
- Bila `pnpm check:types` mengeluh soal `SPREADSHEET_PROPERTY_DETAILS`/`SPREADSHEET_COLUMNS`, tambahkan entry `war_room` sesuai pesan TS (sudah disertakan di Task 11).
- Test DB memakai prefix slug `wr-*` dan `purge` by prefix — selalu `--test-threads=1` untuk suite war room.
