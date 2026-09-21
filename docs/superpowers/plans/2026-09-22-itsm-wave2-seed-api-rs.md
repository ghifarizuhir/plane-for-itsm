# ITSM Wave 2A — Seed Demo di api-rs + Konten ITSM — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port `workspace_seed_task.py` ke backend Rust (api-rs) dan tulis ulang seluruh konten seed demo ke skenario IT service management.

**Architecture:** Modul `seed.rs` di crate `api` memuat 8 JSON seed lewat `include_str!`, lalu menulis bot user + project + states/labels/cycles/modules/issues/views/pages dalam satu transaksi. Dipanggil sinkron dari handler `POST /api/workspaces/` setelah transaksi create commit; kegagalan seed hanya di-log. Tidak ada route baru, tidak ada migrasi.

**Tech Stack:** Rust (axum, sqlx/Postgres, serde_json, chrono), cargo test, Postgres lokal `postgres://plane:plane@localhost:5432/plane`, docker compose (`docker-compose-local.yml`).

**Referensi parity:** `apps/api/plane/bgtasks/workspace_seed_task.py` (sumber perilaku), `apps/api-rs/crates/api/src/routes/{project,draft,label,view,cycle,module,page}.rs` (pola SQL yang sudah terbukti), `apps/api-rs/crates/api/src/routes/issue_activity_write.rs` (helper aktivitas).

---

## File Structure

| File                                                  | Aksi            | Tanggung jawab                             |
| ----------------------------------------------------- | --------------- | ------------------------------------------ |
| `apps/api-rs/crates/api/assets/seeds/data/*.json`     | Pindah (8 file) | Data seed kanonik (satu-satunya sumber)    |
| `apps/api/plane/settings/common.py`                   | Modify          | `SEED_DIR` menunjuk lokasi baru            |
| `apps/api-rs/crates/api/src/seed.rs`                  | Create          | Loader, tipe seed, helper, seluruh insert  |
| `apps/api-rs/crates/api/src/lib.rs`                   | Modify          | `pub mod seed;` (agar test bisa memanggil) |
| `apps/api-rs/crates/api/src/routes/workspace.rs`      | Modify          | Panggil `seed_workspace` setelah commit    |
| `apps/api-rs/crates/api/tests/workspace_seed_test.rs` | Create          | Integration test handler + baris seed      |
| `apps/api-rs/crates/api/parity-inventory.json`        | Modify          | Catatan endpoint `POST /api/workspaces/`   |
| `apps/api/plane/seeds/data/`                          | Delete (pindah) | —                                          |

---

### Task 1: Pindah aset seed + repoint Django `SEED_DIR` + hapus field mati

**Files:**

- Move: `apps/api/plane/seeds/data/{projects,cycles,modules,pages,issues,labels,states,views}.json` → `apps/api-rs/crates/api/assets/seeds/data/`
- Modify: `apps/api/plane/settings/common.py:559`

- [ ] **Step 1: Pindahkan file dengan git mv**

```bash
mkdir -p apps/api-rs/crates/api/assets/seeds/data
git mv apps/api/plane/seeds/data/projects.json apps/api-rs/crates/api/assets/seeds/data/projects.json
git mv apps/api/plane/seeds/data/cycles.json apps/api-rs/crates/api/assets/seeds/data/cycles.json
git mv apps/api/plane/seeds/data/modules.json apps/api-rs/crates/api/assets/seeds/data/modules.json
git mv apps/api/plane/seeds/data/pages.json apps/api-rs/crates/api/assets/seeds/data/pages.json
git mv apps/api/plane/seeds/data/issues.json apps/api-rs/crates/api/assets/seeds/data/issues.json
git mv apps/api/plane/seeds/data/labels.json apps/api-rs/crates/api/assets/seeds/data/labels.json
git mv apps/api/plane/seeds/data/states.json apps/api-rs/crates/api/assets/seeds/data/states.json
git mv apps/api/plane/seeds/data/views.json apps/api-rs/crates/api/assets/seeds/data/views.json
```

- [ ] **Step 2: Repoint `SEED_DIR`**

Di `apps/api/plane/settings/common.py`, ganti baris:

```python
SEED_DIR = os.path.join(BASE_DIR, "seeds")
```

menjadi:

```python
# Seed data kanonik pindah ke api-rs (`apps/api-rs/crates/api/assets/seeds`);
# lihat docs/superpowers/specs/2026-09-22-itsm-copy-kit-wave2-design.md.
SEED_DIR = os.path.join(BASE_DIR, "..", "..", "api-rs", "crates", "api", "assets", "seeds")
```

- [ ] **Step 3: Hapus field mati dari `projects.json` dan `pages.json`**

`name`/`identifier` di `projects.json` selalu ditimpa task Django; `description` (stringified doc JSON) di `pages.json` tidak dibaca task mana pun (task hanya membaca `description_html`, `description_binary`, `description_stripped`, `description_json`).

```bash
node -e "
const fs=require('fs');
const d='apps/api-rs/crates/api/assets/seeds/data/';
const p=JSON.parse(fs.readFileSync(d+'projects.json','utf8'));
for(const item of p){ delete item.name; delete item.identifier; }
fs.writeFileSync(d+'projects.json', JSON.stringify(p,null,4)+'\n');
const g=JSON.parse(fs.readFileSync(d+'pages.json','utf8'));
for(const item of g){ delete item.description; }
fs.writeFileSync(d+'pages.json', JSON.stringify(g,null,4)+'\n');
console.log('projects keys:', Object.keys(p[0]).join(','));
console.log('pages keys:', Object.keys(g[0]).join(','));
"
```

Expected: `projects keys: id,description,network,cover_image,logo_props` dan `pages keys: id,name,project_id,description_html,description_stripped,type,access,logo_props`

- [ ] **Step 4: Verifikasi semua JSON valid dan lokasi Django resolve**

```bash
node -e "
const fs=require('fs');
for (const f of ['projects','cycles','modules','pages','issues','labels','states','views']) {
  JSON.parse(fs.readFileSync('apps/api-rs/crates/api/assets/seeds/data/'+f+'.json','utf8'));
}
console.log('all 8 seed JSON valid');
"
python3 -c "
import os
base='apps/api/plane'
p=os.path.normpath(os.path.join(base,'..','..','api-rs','crates','api','assets','seeds'))
print('SEED_DIR resolves to:', p)
print('exists:', os.path.isdir(p))
"
```

Expected: `all 8 seed JSON valid` lalu `exists: True`

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/assets/seeds apps/api/plane/settings/common.py apps/api/plane/seeds
git commit -m "chore(seed): pindahkan data seed ke api-rs assets + repoint Django SEED_DIR"
```

---

### Task 2: Modul seed — loader, tipe, helper + unit test

**Files:**

- Create: `apps/api-rs/crates/api/src/seed.rs`
- Modify: `apps/api-rs/crates/api/src/lib.rs:2`

- [ ] **Step 1: Tulis test yang gagal (helper murni)**

Buat `apps/api-rs/crates/api/src/seed.rs`:

```rust
//! Workspace demo seed — port dari `plane/bgtasks/workspace_seed_task.py`.

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn project_identifier_takes_five_alnum_chars() {
        assert_eq!(project_identifier("Acme IT"), "AcmeI");
        assert_eq!(project_identifier("My Workspace 42"), "MyWor");
    }

    #[test]
    fn project_identifier_can_be_empty() {
        assert_eq!(project_identifier("---"), "");
    }

    #[test]
    fn project_identifier_is_unicode_aware() {
        assert_eq!(project_identifier("Ünïcode 123"), "Ünïco");
    }

    #[test]
    fn current_cycle_runs_two_weeks_from_now() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let (start, end) = cycle_dates("CURRENT", None, now);
        assert_eq!(start, now);
        assert_eq!(end, now + Duration::days(14));
    }

    #[test]
    fn upcoming_cycle_starts_after_last_end() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let last_end = now + Duration::days(14);
        let (start, end) = cycle_dates("UPCOMING", Some(last_end), now);
        assert_eq!(start, last_end + Duration::days(1));
        assert_eq!(end, start + Duration::days(14));
    }

    #[test]
    fn module_dates_offset_by_index() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let (start, target) = module_dates(2, now);
        assert_eq!(start, now + Duration::days(4));
        assert_eq!(target, start + Duration::days(14));
    }

    #[test]
    fn bot_email_uses_web_url_host() {
        assert_eq!(url_host("http://192.168.1.11:8000"), Some("192.168.1.11".into()));
        assert_eq!(url_host("https://terraline.space/path"), Some("terraline.space".into()));
        assert_eq!(url_host(""), None);
    }
}
```

Tambahkan di `apps/api-rs/crates/api/src/lib.rs` setelah `pub mod routes;`:

```rust
pub mod seed;
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api seed::tests 2>&1 | tail -20`
Expected: FAIL — `cannot find function project_identifier` (compile error).

- [ ] **Step 3: Implementasi loader, tipe, helper**

Tambahkan di atas modul tests pada `apps/api-rs/crates/api/src/seed.rs`:

```rust
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const PROJECTS_JSON: &str = include_str!("../assets/seeds/data/projects.json");
const STATES_JSON: &str = include_str!("../assets/seeds/data/states.json");
const LABELS_JSON: &str = include_str!("../assets/seeds/data/labels.json");
const CYCLES_JSON: &str = include_str!("../assets/seeds/data/cycles.json");
const MODULES_JSON: &str = include_str!("../assets/seeds/data/modules.json");
const ISSUES_JSON: &str = include_str!("../assets/seeds/data/issues.json");
const VIEWS_JSON: &str = include_str!("../assets/seeds/data/views.json");
const PAGES_JSON: &str = include_str!("../assets/seeds/data/pages.json");

fn parse_seed<T: for<'de> Deserialize<'de>>(raw: &str) -> Vec<T> {
    serde_json::from_str(raw).expect("seed JSON embedded at compile time must be valid")
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectSeed {
    description: String,
    network: i16,
    cover_image: Option<String>,
    #[serde(default)]
    logo_props: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct StateSeed {
    id: i64,
    name: String,
    color: String,
    sequence: f64,
    group: String,
    #[serde(rename = "default")]
    is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct LabelSeed {
    id: i64,
    name: String,
    color: String,
    sort_order: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct CycleSeed {
    id: i64,
    name: String,
    sort_order: f64,
    timezone: String,
    #[serde(rename = "type")]
    cycle_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ModuleSeed {
    id: i64,
    name: String,
    sort_order: f64,
    status: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IssueSeed {
    id: i64,
    name: String,
    sequence_id: i32,
    description_html: String,
    #[serde(default)]
    description_stripped: Option<String>,
    sort_order: f64,
    state_id: i64,
    #[serde(default)]
    labels: Vec<i64>,
    priority: Option<String>,
    cycle_id: Option<i64>,
    #[serde(default)]
    module_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewSeed {
    name: String,
    description: String,
    access: i16,
    filters: Value,
    display_filters: Value,
    display_properties: Value,
    rich_filters: Value,
    sort_order: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct PageSeed {
    name: String,
    #[serde(default)]
    description_html: String,
    #[serde(default)]
    description_stripped: Option<String>,
    access: i16,
    #[serde(rename = "type")]
    page_type: String,
    #[serde(default)]
    logo_props: Value,
    project_id: Option<i64>,
}

/// `"".join(ch for ch in name if ch.isalnum())[:5]` (`workspace_seed_task.py:85`).
pub fn project_identifier(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).take(5).collect()
}

fn cycle_dates(
    cycle_type: &str,
    last_end: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    if cycle_type == "UPCOMING" {
        let start = match last_end {
            Some(end) => end + Duration::days(1),
            None => now + Duration::days(14),
        };
        (start, start + Duration::days(14))
    } else {
        (now, now + Duration::days(14))
    }
}

fn module_dates(index: i64, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = now + Duration::days(index * 2);
    (start, start + Duration::days(14))
}

fn url_host(raw: &str) -> Option<String> {
    let after_scheme = raw.split("://").nth(1).unwrap_or(raw);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or("");
    if host.is_empty() { None } else { Some(host.to_string()) }
}

fn bot_email(workspace_id: Uuid) -> String {
    let host = std::env::var("WEB_URL")
        .ok()
        .and_then(|u| url_host(&u))
        .unwrap_or_else(|| "terraline.space".into());
    format!("bot_user_{workspace_id}@{host}")
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api seed::tests 2>&1 | tail -15`
Expected: `test result: ok. 7 passed`

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/src/seed.rs apps/api-rs/crates/api/src/lib.rs
git commit -m "feat(api-rs): seed module skeleton + helper test"
```

---

### Task 3: Seed — bot user, workspace member, project, project members, user properties

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs`

- [ ] **Step 1: Implementasi insert bot user + member**

Tambahkan (sebelum modul tests):

```rust
async fn insert_bot_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let email = bot_email(workspace_id);
    let username = format!("bot_user_{workspace_id}");
    let password = common::auth::make_django_password(&Uuid::new_v4().simple().to_string());
    let token = Uuid::new_v4().simple().to_string();
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, \
         avatar, date_joined, token, user_timezone, last_location, created_location, last_login_ip, \
         last_logout_ip, last_login_medium, last_login_uagent, last_active, last_login_time, \
         token_updated_at, is_active, is_staff, is_superuser, is_managed, is_password_expired, \
         is_email_verified, is_password_autoset, is_bot, bot_type, is_email_valid, \
         is_password_reset_required, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'Terraline', '', 'Terraline', '', now(), $4, \
         'UTC', '', '', '', '', '', '', now(), now(), now(), true, false, false, false, false, \
         false, true, true, 'WORKSPACE_SEED', true, false, now(), now()) RETURNING id",
    )
    .bind(&email)
    .bind(&username)
    .bind(&password)
    .bind(&token)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

async fn insert_workspace_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO workspace_members (id, workspace_id, member_id, role, company_role, \
         view_props, default_props, issue_props, is_active, getting_started_checklist, tips, \
         explored_features, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 20, '', '{}', '{}', '{}', true, '{}', '{}', '{}', \
         now(), now())",
    )
    .bind(workspace_id)
    .bind(bot_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 2: Implementasi project + members + user properties**

```rust
const SEED_DISPLAY_FILTERS: &str = r#"{"layout":"list","calendar":{"layout":"month","show_weekends":false},"group_by":"state","order_by":"sort_order","sub_issue":true,"sub_group_by":null,"show_empty_groups":true}"#;
const SEED_DISPLAY_PROPERTIES: &str = r#"{"key":true,"link":true,"cycle":false,"state":true,"labels":false,"modules":false,"assignee":true,"due_date":false,"estimate":true,"priority":true,"created_on":true,"issue_type":true,"start_date":false,"updated_on":true,"customer_count":true,"sub_issue_count":false,"attachment_count":false,"customer_request_count":true}"#;
const SEED_FILTERS: &str = r#"{"priority":null,"state":null,"state_group":null,"assignees":null,"created_by":null,"labels":null,"start_date":null,"target_date":null,"subscriber":null}"#;
const SEED_PREFERENCES: &str = r#"{"pages":{"block_display":true},"navigation":{"default_tab":"work_items","hide_in_more_menu":[]}}"#;

async fn insert_project(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    workspace_name: &str,
    bot_id: Uuid,
) -> Result<Uuid, anyhow::Error> {
    let identifier = project_identifier(workspace_name);
    if identifier.is_empty() {
        anyhow::bail!("seed: workspace name has no alphanumeric characters for an identifier");
    }
    let seed = parse_seed::<ProjectSeed>(PROJECTS_JSON)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("seed: projects.json is empty"))?;
    let (project_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO projects (id, name, description, identifier, workspace_id, network, \
         module_view, cycle_view, issue_views_view, page_view, intake_view, \
         is_time_tracking_enabled, is_issue_type_enabled, guest_view_all_features, archive_in, \
         close_in, cover_image, logo_props, timezone, created_by_id, updated_by_id, created_at, \
         updated_at) \
         SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, true, true, true, true, false, false, \
         false, false, 0, 0, $5, $6, w.timezone, $7, $7, now(), now() \
         FROM workspaces w WHERE w.id = $8 RETURNING id",
    )
    .bind(workspace_name)
    .bind(&seed.description)
    .bind(&identifier)
    .bind(seed.network)
    .bind(seed.cover_image.as_deref())
    .bind(&seed.logo_props)
    .bind(bot_id)
    .bind(workspace_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(project_id)
}

async fn insert_project_members_and_props(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
         view_props, default_props, sort_order, preferences, created_at, updated_at) \
         SELECT gen_random_uuid(), wm.member_id, wm.role, $1, $2, true, '{}', '{}', 65535, '{}', \
         now(), now() \
         FROM workspace_members wm WHERE wm.workspace_id = $2 AND wm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(workspace_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO project_user_properties (id, user_id, project_id, workspace_id, filters, \
         display_filters, display_properties, rich_filters, preferences, sort_order, created_by_id, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), wm.member_id, $1, $2, $3::jsonb, $4::jsonb, $5::jsonb, \
         '{}'::jsonb, $6::jsonb, 65535, $7, now(), now() \
         FROM workspace_members wm WHERE wm.workspace_id = $2 AND wm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(SEED_FILTERS)
    .bind(SEED_DISPLAY_FILTERS)
    .bind(SEED_DISPLAY_PROPERTIES)
    .bind(SEED_PREFERENCES)
    .bind(bot_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 3: Verifikasi kompilasi**

Run: `cargo check -p api 2>&1 | tail -5`
Expected: `Finished` tanpa error.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/src/seed.rs
git commit -m "feat(api-rs): seed bot user + project + members"
```

---

### Task 4: Seed — states, labels, cycles, modules

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs`

- [ ] **Step 1: Implementasi states + labels**

```rust
fn slugify(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

async fn insert_states(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let mut map = std::collections::HashMap::new();
    for s in parse_seed::<StateSeed>(STATES_JSON) {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO states (id, name, description, color, slug, created_by_id, project_id, \
             workspace_id, sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', $2, $3, $4, $5, $6, $7, $8, $9, false, now(), now()) \
             RETURNING id",
        )
        .bind(&s.name)
        .bind(&s.color)
        .bind(slugify(&s.name))
        .bind(bot_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(s.sequence)
        .bind(&s.group)
        .bind(s.is_default)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(s.id, id);
    }
    Ok(map)
}

async fn insert_labels(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let mut map = std::collections::HashMap::new();
    for l in parse_seed::<LabelSeed>(LABELS_JSON) {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO labels (id, name, color, description, project_id, workspace_id, parent_id, \
             sort_order, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, '', $3, $4, NULL, $5, $6, $6, now(), now()) \
             RETURNING id",
        )
        .bind(&l.name)
        .bind(&l.color)
        .bind(project_id)
        .bind(workspace_id)
        .bind(l.sort_order)
        .bind(bot_id)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(l.id, id);
    }
    Ok(map)
}
```

- [ ] **Step 2: Implementasi cycles + modules**

```rust
async fn insert_cycles(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let now = Utc::now();
    let mut map = std::collections::HashMap::new();
    let mut last_end: Option<DateTime<Utc>> = None;
    for c in parse_seed::<CycleSeed>(CYCLES_JSON) {
        let (start, end) = cycle_dates(&c.cycle_type, last_end, now);
        last_end = Some(end);
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO cycles (id, name, description, project_id, workspace_id, owned_by_id, \
             created_by_id, timezone, version, view_props, logo_props, progress_snapshot, \
             sort_order, start_date, end_date, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', $2, $3, $4, $4, $5, 1, '{}', '{}', '{}', $6, $7, \
             $8, now(), now()) RETURNING id",
        )
        .bind(&c.name)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .bind(&c.timezone)
        .bind(c.sort_order)
        .bind(start)
        .bind(end)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(c.id, id);
    }
    Ok(map)
}

async fn insert_modules(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let now = Utc::now();
    let mut map = std::collections::HashMap::new();
    for (index, m) in parse_seed::<ModuleSeed>(MODULES_JSON).into_iter().enumerate() {
        let (start, target) = module_dates(index as i64, now);
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO modules (id, name, description, status, lead_id, project_id, workspace_id, \
             created_by_id, updated_by_id, view_props, logo_props, sort_order, start_date, \
             target_date, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, NULL, $4, $5, $6, $6, '{}', '{}', $7, $8, $9, \
             now(), now()) RETURNING id",
        )
        .bind(&m.name)
        .bind(&m.description)
        .bind(&m.status)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .bind(m.sort_order)
        .bind(start)
        .bind(target)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(m.id, id);
    }
    Ok(map)
}
```

- [ ] **Step 3: Verifikasi kompilasi**

Run: `cargo check -p api 2>&1 | tail -5`
Expected: `Finished` tanpa error.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/src/seed.rs
git commit -m "feat(api-rs): seed states, labels, cycles, modules"
```

---

### Task 5: Seed — issues + sequence + activity + relasi

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs`

- [ ] **Step 1: Implementasi insert issues**

```rust
async fn insert_issues(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
    states: &std::collections::HashMap<i64, Uuid>,
    labels: &std::collections::HashMap<i64, Uuid>,
    cycles: &std::collections::HashMap<i64, Uuid>,
    modules: &std::collections::HashMap<i64, Uuid>,
) -> Result<(), anyhow::Error> {
    for issue in parse_seed::<IssueSeed>(ISSUES_JSON) {
        let state_id = states
            .get(&issue.state_id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("seed: unknown state_id {}", issue.state_id))?;
        let (issue_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO issues (id, name, description_html, description_json, \
             description_stripped, priority, start_date, target_date, sequence_id, sort_order, \
             completed_at, is_draft, estimate_point_id, parent_id, type_id, state_id, project_id, \
             workspace_id, created_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, '{}', $3, COALESCE($4, 'none'), NULL, NULL, $5, \
             $6, NULL, false, NULL, NULL, NULL, $7, $8, $9, $10, now(), now()) RETURNING id",
        )
        .bind(&issue.name)
        .bind(&issue.description_html)
        .bind(issue.description_stripped.as_deref())
        .bind(issue.priority.as_deref())
        .bind(issue.sequence_id)
        .bind(issue.sort_order)
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, \
             created_by_id, deleted, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, now(), now())",
        )
        .bind(issue.sequence_id)
        .bind(issue_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .execute(&mut **tx)
        .await?;
        crate::routes::issue_activity_write::insert_created_activity(
            tx,
            issue_id,
            project_id,
            workspace_id,
            bot_id,
            Utc::now().timestamp_millis() as f64 / 1000.0,
        )
        .await?;
        for seed_label in &issue.labels {
            let label_id = labels
                .get(seed_label)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("seed: unknown label id {seed_label}"))?;
            sqlx::query(
                "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
            )
            .bind(issue_id)
            .bind(label_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
        if let Some(seed_cycle) = issue.cycle_id {
            let cycle_id = cycles
                .get(&seed_cycle)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("seed: unknown cycle id {seed_cycle}"))?;
            sqlx::query(
                "INSERT INTO cycle_issues (id, cycle_id, issue_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
            )
            .bind(cycle_id)
            .bind(issue_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
        for seed_module in &issue.module_ids {
            let module_id = modules
                .get(seed_module)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("seed: unknown module id {seed_module}"))?;
            sqlx::query(
                "INSERT INTO module_issues (id, module_id, issue_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
            )
            .bind(module_id)
            .bind(issue_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
```

Catatan: `insert_created_activity` saat ini `pub(crate)` di `routes/issue_activity_write.rs` — sudah dapat diakses dari `seed.rs` (satu crate). Pastikan signature-nya `(tx, issue_id, project_id, workspace_id, actor, epoch)`.

- [ ] **Step 2: Verifikasi kompilasi**

Run: `cargo check -p api 2>&1 | tail -5`
Expected: `Finished` tanpa error.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/src/seed.rs
git commit -m "feat(api-rs): seed issues + sequence + activity + relasi"
```

---

### Task 6: Seed — views, pages, orchestrator

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs`

- [ ] **Step 1: Implementasi views + pages**

```rust
async fn insert_views(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    for v in parse_seed::<ViewSeed>(VIEWS_JSON) {
        sqlx::query(
            "INSERT INTO issue_views (id, name, description, query, filters, display_filters, \
             display_properties, rich_filters, logo_props, access, sort_order, is_locked, \
             project_id, workspace_id, owned_by_id, created_by_id, updated_by_id, created_at, \
             updated_at) \
             VALUES (gen_random_uuid(), $1, $2, '{}', $3, $4, $5, $6, '{}', $7, $8, false, $9, \
             $10, $11, $11, $11, now(), now())",
        )
        .bind(&v.name)
        .bind(&v.description)
        .bind(&v.filters)
        .bind(&v.display_filters)
        .bind(&v.display_properties)
        .bind(&v.rich_filters)
        .bind(v.access)
        .bind(v.sort_order)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_pages(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    for p in parse_seed::<PageSeed>(PAGES_JSON) {
        let (page_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
             description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
             access, parent_id, archived_at, is_locked, view_props, logo_props, is_global, \
             sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '{}', NULL, $2, $3, $4, $4, $4, $5, '', $6, NULL, \
             NULL, false, '{}', '{}', false, 65535, now(), now()) RETURNING id",
        )
        .bind(&p.name)
        .bind(&p.description_html)
        .bind(p.description_stripped.as_deref())
        .bind(bot_id)
        .bind(workspace_id)
        .bind(p.access)
        .fetch_one(&mut **tx)
        .await?;
        if p.project_id.is_some() && p.page_type == "PROJECT" {
            sqlx::query(
                "INSERT INTO project_pages (id, workspace_id, project_id, page_id, created_by_id, \
                 updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now())",
            )
            .bind(workspace_id)
            .bind(project_id)
            .bind(page_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Implementasi orchestrator**

```rust
/// Seed demo untuk satu workspace baru. Semua insert dalam satu transaksi;
/// error menggagalkan seluruh seed (caller hanya mencatat log).
pub async fn seed_workspace(pool: &PgPool, workspace_id: Uuid) -> Result<(), anyhow::Error> {
    let mut tx = pool.begin().await?;
    let workspace_name: Option<(String,)> =
        sqlx::query_as("SELECT name FROM workspaces WHERE id = $1 AND deleted_at IS NULL")
            .bind(workspace_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((workspace_name,)) = workspace_name else {
        anyhow::bail!("seed: workspace {workspace_id} not found");
    };
    let bot_id = insert_bot_user(&mut tx, workspace_id).await?;
    insert_workspace_member(&mut tx, workspace_id, bot_id).await?;
    let project_id = insert_project(&mut tx, workspace_id, &workspace_name, bot_id).await?;
    insert_project_members_and_props(&mut tx, workspace_id, project_id, bot_id).await?;
    let states = insert_states(&mut tx, workspace_id, project_id, bot_id).await?;
    let labels = insert_labels(&mut tx, workspace_id, project_id, bot_id).await?;
    let cycles = insert_cycles(&mut tx, workspace_id, project_id, bot_id).await?;
    let modules = insert_modules(&mut tx, workspace_id, project_id, bot_id).await?;
    insert_issues(
        &mut tx, workspace_id, project_id, bot_id, &states, &labels, &cycles, &modules,
    )
    .await?;
    insert_views(&mut tx, workspace_id, project_id, bot_id).await?;
    insert_pages(&mut tx, workspace_id, project_id, bot_id).await?;
    tx.commit().await?;
    Ok(())
}
```

- [ ] **Step 3: Verifikasi kompilasi**

Run: `cargo check -p api 2>&1 | tail -5`
Expected: `Finished` tanpa error.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/src/seed.rs
git commit -m "feat(api-rs): seed views + pages + orchestrator"
```

---

### Task 7: Wire ke handler `POST /api/workspaces/`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workspace.rs:326-333`

- [ ] **Step 1: Panggil seed setelah commit**

Di `routes/workspace.rs`, ganti blok:

```rust
    if tx.commit().await.is_err() {
        tracing::warn!("ws-create: commit failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    match fetch_ws_full(&st.pool, slug, owner).await? {
```

menjadi:

```rust
    if tx.commit().await.is_err() {
        tracing::warn!("ws-create: commit failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    // Demo seed (parity `workspace_seed_task.py`), sinkron setelah commit:
    // kegagalan seed tidak menggagalkan create — hanya di-log.
    if let Err(e) = crate::seed::seed_workspace(&st.pool, ws_id).await {
        tracing::error!(error = %e, "ws-create: workspace seed failed");
    }
    match fetch_ws_full(&st.pool, slug, owner).await? {
```

Perbarui juga komentar doc di atas `pub async fn create` (`workspace.rs:222-232`): ganti `workspace_seed` celery skipped.` menjadi `` `workspace_seed` dijalankan sinkron setelah commit (crate::seed). ``

- [ ] **Step 2: Verifikasi kompilasi**

Run: `cargo check -p api 2>&1 | tail -5`
Expected: `Finished` tanpa error.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/src/routes/workspace.rs
git commit -m "feat(api-rs): jalankan seed demo saat workspace dibuat"
```

---

### Task 8: Integration test — handler men-seed baris yang benar

**Files:**

- Create: `apps/api-rs/crates/api/tests/workspace_seed_test.rs`

- [ ] **Step 1: Tulis test**

```rust
//! Integration test seed workspace: panggil handler `POST /api/workspaces/`
//! (men-seed sinkron) lalu assert baris demo yang dihasilkan.
//! Butuh Postgres: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.

use api::middleware::auth::AuthUser;
use api::routes::workspace::create;
use api::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
}

async fn state() -> AppState {
    AppState {
        pool: pool().await,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) {
    sqlx::query(
        "INSERT INTO users (id, password, username, email, first_name, last_name, avatar, \
         date_joined, created_at, updated_at, last_location, created_location, is_superuser, \
         is_managed, is_password_expired, is_active, is_staff, is_email_verified, \
         is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip, \
         last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid, \
         is_password_reset_required) \
         VALUES ($1, '', $2, $3, '', '', '', now(), now(), now(), '', '', false, false, \
         false, true, false, false, true, $4, 'UTC', '', '', '', '', false, $2, true, false)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
}

async fn purge(pool: &PgPool, slug: &str) {
    let ws: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .expect("purge lookup");
    let Some((ws_id,)) = ws else { return };
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT member_id FROM workspace_members WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_all(pool)
            .await
            .expect("purge members");
    for stmt in [
        "DELETE FROM module_issues WHERE workspace_id = $1",
        "DELETE FROM cycle_issues WHERE workspace_id = $1",
        "DELETE FROM issue_labels WHERE workspace_id = $1",
        "DELETE FROM issue_activities WHERE workspace_id = $1",
        "DELETE FROM issue_sequences WHERE workspace_id = $1",
        "DELETE FROM issues WHERE workspace_id = $1",
        "DELETE FROM issue_views WHERE workspace_id = $1",
        "DELETE FROM project_pages WHERE workspace_id = $1",
        "DELETE FROM pages WHERE workspace_id = $1",
        "DELETE FROM modules WHERE workspace_id = $1",
        "DELETE FROM cycles WHERE workspace_id = $1",
        "DELETE FROM labels WHERE workspace_id = $1",
        "DELETE FROM states WHERE workspace_id = $1",
        "DELETE FROM project_user_properties WHERE workspace_id = $1",
        "DELETE FROM project_members WHERE workspace_id = $1",
        "DELETE FROM projects WHERE workspace_id = $1",
        "DELETE FROM workspace_members WHERE workspace_id = $1",
        "DELETE FROM workspaces WHERE id = $1",
    ] {
        sqlx::query(stmt).bind(ws_id).execute(pool).await.expect("purge");
    }
    for (member_id,) in members {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(member_id)
            .execute(pool)
            .await
            .expect("purge user");
    }
}

#[tokio::test]
async fn workspace_create_seeds_itsm_demo() {
    let st = state().await;
    let pool = st.pool.clone();
    let slug = format!("wsseed-{}", Uuid::new_v4().simple());
    let owner = Uuid::new_v4();
    insert_user(&pool, owner, &slug).await;

    let (status, _) = create(
        State(st.clone()),
        AuthUser(owner),
        Json(json!({"name": "Acme IT", "slug": slug})),
    )
    .await
    .expect("create must not error");
    assert_eq!(status, StatusCode::CREATED);

    let (ws_id,): (Uuid,) = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&pool)
        .await
        .expect("workspace row");

    let count = |table: &'static str| {
        let pool = pool.clone();
        async move {
            let (c,): (i64,) =
                sqlx::query_as(&format!("SELECT COUNT(*) FROM {table} WHERE workspace_id = $1"))
                    .bind(ws_id)
                    .fetch_one(&pool)
                    .await
                    .expect("count");
            c
        }
    };

    let (project_name, identifier): (String, String) =
        sqlx::query_as("SELECT name, identifier FROM projects WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_one(&pool)
            .await
            .expect("project row");
    assert_eq!(project_name, "Acme IT");
    assert_eq!(identifier, "AcmeI");

    assert_eq!(count("states").await, 5);
    assert_eq!(count("labels").await, 2);
    assert_eq!(count("cycles").await, 2);
    assert_eq!(count("modules").await, 3);
    assert_eq!(count("issues").await, 7);
    assert_eq!(count("issue_sequences").await, 7);
    assert_eq!(count("issue_activities").await, 7);
    assert_eq!(count("issue_labels").await, 3);
    assert_eq!(count("cycle_issues").await, 7);
    assert_eq!(count("module_issues").await, 11);
    assert_eq!(count("issue_views").await, 1);
    assert_eq!(count("pages").await, 2);
    assert_eq!(count("project_pages").await, 2);
    assert_eq!(count("project_members").await, 2);
    assert_eq!(count("project_user_properties").await, 2);

    let (bot_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE is_bot = true AND bot_type = 'WORKSPACE_SEED' AND email LIKE 'bot_user_%'")
            .fetch_one(&pool)
            .await
            .expect("bot count");
    assert_eq!(bot_count, 1);

    purge(&pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workspace_seed_test 2>&1 | tail -20`
Expected: `test workspace_create_seeds_itsm_demo ... ok` dan `test result: ok. 1 passed`

Jika `module_issues` bukan 11, periksa `module_ids` di `issues.json` (jumlah pasangan issue-module) dan sesuaikan ekspektasi — jangan mengubah data untuk memenuhi test.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all
git add apps/api-rs/crates/api/tests/workspace_seed_test.rs
git commit -m "test(api-rs): integration test seed workspace"
```

---

### Task 9: Perbarui parity inventory + jalankan gate

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (entri `POST /api/workspaces/`, sekitar baris 1388-1404)

- [ ] **Step 1: Perbarui `notes` entri `POST /api/workspaces/`**

Di akhir string `notes` entri tersebut, tambahkan kalimat:

```
 Seed demo: `workspace_seed` dijalankan sinkron setelah commit lewat `crate::seed::seed_workspace` (port `workspace_seed_task.py`: bot user WORKSPACE_SEED, project nama=workspace + identifier alnum[:5], project members + user properties, 5 states, labels, cycles/modules dengan tanggal relatif, 7 issues + sequence/activity/labels/cycle/module links, 1 view, 2 pages + project_pages); kegagalan seed hanya di-log, create tetap 201.
```

- [ ] **Step 2: Jalankan gate test**

Run: `cargo test -p api --test route_inventory_test --test fe_tripwire_test --test parity_gate_test 2>&1 | tail -15`
Expected: semua `ok`, tidak ada route/evidence yang hilang.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "docs(api-rs): parity inventory — workspace seed tidak lagi skipped"
```

Catatan: bila `parity-inventory.json` sudah punya perubahan pra-existing yang tidak terkait, stage hanya hunk `notes` milikmu (`git add -p`) dan pastikan `git diff --cached` bersih dari perubahan orang lain sebelum commit.

---

### Task 10: Konten — projects, cycles, modules, labels, views

**Files:**

- Modify: `apps/api-rs/crates/api/assets/seeds/data/{projects,cycles,modules,labels,views}.json`

- [ ] **Step 1: Tulis ulang nilai kecil**

`projects.json` — ganti `description`:

```
Welcome to Terraline. This demo project shows how a team runs services — the services you support, the requests and incidents that come in, and the knowledge your team works from. Every card here is a work item: read them in order or jump to what you need. When you're ready, create your own project and make it yours.
```

`cycles.json` — ganti `name`:

```
Week 1: Set up your first service
Week 2: Triage and resolve
```

`modules.json` — ganti `name` + `description`:

```
Service Catalog (System)        | The services your team supports and the work that keeps them healthy.
Request Fulfilment (Process)    | Intake, triage, and assignment for incoming service requests.
Knowledge Base (Area)           | Runbooks, SOPs, and postmortems your team works from.
```

`labels.json` — ganti `name` + `color`:

```
incident (#EF4444)
service-request (#0693e3)
```

`views.json` — ganti `name` + `description`:

```
Urgent requests | Urgent priority work across this service.
```

- [ ] **Step 2: Verifikasi JSON valid**

Run:

```bash
node -e "
const fs=require('fs');
const d='apps/api-rs/crates/api/assets/seeds/data/';
console.log(JSON.parse(fs.readFileSync(d+'cycles.json'))[0].name);
console.log(JSON.parse(fs.readFileSync(d+'modules.json')).map(m=>m.name).join(' | '));
console.log(JSON.parse(fs.readFileSync(d+'labels.json')).map(l=>l.name).join(' | '));
console.log(JSON.parse(fs.readFileSync(d+'views.json'))[0].name);
console.log(JSON.parse(fs.readFileSync(d+'projects.json'))[0].description.slice(0,40));
"
```

Expected: nama-nama ITSM baru, tanpa error parse.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/assets/seeds/data
git commit -m "content(seed): cycle/module/label/view ITSM"
```

---

### Task 11: Konten — 7 work item

**Files:**

- Modify: `apps/api-rs/crates/api/assets/seeds/data/issues.json`

- [ ] **Step 1: Tulis ulang `name` + `description_html` per item**

Pertahankan `id`, `sequence_id`, `sort_order`, `state_id`, `labels`, `priority`, `cycle_id`, `module_ids` apa adanya. Struktur HTML mengikuti konvensi existing:

- Paragraf: `<p class="editor-paragraph-block">…</p>`
- List: `<ol class="list-decimal pl-7 space-y-(--list-spacing-y) tight" data-tight="true"><li class="not-prose space-y-2"><p class="editor-paragraph-block">…</p></li></ol>` (atau `<ul class="list-disc pl-7 space-y-(--list-spacing-y)" data-tight="false">` untuk bullet)
- Bold: `<strong>…</strong>`

Judul dan isi:

1. **Welcome to Terraline 👋**
   - P: "Hey there! This demo project is your playground to see how a team runs services in Terraline. Click around, change things, and don't worry about breaking anything."
   - P: "Each work item walks you through one part of running a service — from setting it up to triaging what comes in. Follow along card by card, or jump straight to what you need."
   - P: "First thing to try"
   - OL: "Find the <strong>Properties</strong> section below where it says <strong>State: Todo</strong>." / "Click it and change it to <strong>Done</strong>, or drag this card to the Done column."

2. **1. Create a project for your service 🎯**
   - P: "A project is where a service lives in Terraline — its work items, requests, runbooks, and the people who run it."
   - P: "Note: this demo is already set up as a project, and the cards you're reading are work items inside it. Here's how you'd create your own when you're ready."
   - OL: "Open the <strong>Projects</strong> list from the sidebar and choose <strong>Create project</strong>." / "Give it a name and an identifier — something short your team will recognise in work item IDs." / "Add a description so anyone landing here knows which service this is."
   - P: "Tip: keep one project per service. If two teams run the same service, they can share the project and split work with Modules."

3. **2. Invite your team 🤜🤛**
   - P: "Running a service is a team effort. Invite the people who triage, resolve, and own it."
   - OL: "Open workspace <strong>Settings → Members</strong> and choose <strong>Add member</strong>." / "Invite by email and pick a role: <strong>Admin</strong> for the people who configure the workspace, <strong>Member</strong> for day-to-day work, <strong>Guest</strong> for stakeholders who only need to follow along." / "Use project settings when someone should only work in one service."
   - P: "Roles decide what people can see and change, so start with the least access someone needs."

4. **3. Log requests and incidents ✏️**
   - P: "Every piece of work is a work item. A request, an incident, a task, a change — same building block, different properties."
   - OL: "Press <strong>C</strong> anywhere, or click <strong>+ New Work Item</strong>." / "Give it a name, then set <strong>Priority</strong>, <strong>Assignee</strong>, and <strong>State</strong> so it lands in the right queue." / "Add labels to group work across services, and attach a runbook page if the fix is already written down."
   - P: "Work items can be split into sub-work items when a fix has several steps. Use Modules when the work spans more than one cycle."

5. **4. Visualize your work 🔮**
   - P: "The same work items can be viewed in different ways depending on what you need to see."
   - UL: "List and Kanban — day-to-day triage and handovers." / "Calendar — what's due when, and what's coming up." / "Spreadsheet — bulk edits across many work items." / "Gantt — dependencies and timelines for larger rollouts."
   - P: "Save any combination of filters as a View so the whole team opens the same queue. Start with the <strong>Urgent requests</strong> view already saved in this project."

6. **5. Timebox work with cycles 🗓️**
   - P: "A Cycle is a timebox: a week, a fortnight, or a maintenance window. Use it to agree what the team takes on and keep progress visible."
   - OL: "Open <strong>Cycles</strong> and create one for the period you're planning." / "Add work items to it — only what the team can realistically finish." / "At the end, move unfinished work to the next cycle and start again."
   - P: "For efforts longer than a cycle — a platform migration, a service onboarding — group work items into a Module instead."

7. **6. Customize your settings ⚙️**
   - P: "Terraline works the way your team works. A few settings make it fit your service management practice."
   - UL: "States — rename them to match your process: Triage, Waiting on customer, Resolved." / "Labels — add incident, service request, change, or your own categories." / "Estimates — switch to a system your team already uses." / "Features — turn Cycles, Modules, Pages, or Intake on and off per project."
   - P: "You can change all of this later — nothing here is permanent."

- [ ] **Step 2: Verifikasi JSON + jumlah**

Run:

```bash
node -e "
const fs=require('fs');
const j=JSON.parse(fs.readFileSync('apps/api-rs/crates/api/assets/seeds/data/issues.json','utf8'));
console.log('issues:', j.length);
for (const i of j) console.log(i.sequence_id, i.name, '| html', i.description_html.length);
"
```

Expected: 7 issues, judul baru, `description_html` non-kosong.

- [ ] **Step 3: Jalankan ulang integration test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workspace_seed_test 2>&1 | tail -8`
Expected: `test result: ok. 1 passed` (struktur tidak berubah).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/assets/seeds/data/issues.json
git commit -m "content(seed): 7 work item tutorial ITSM"
```

---

### Task 12: Konten — 2 page

**Files:**

- Modify: `apps/api-rs/crates/api/assets/seeds/data/pages.json`

- [ ] **Step 1: Tulis ulang Page 1 — Service Runbook**

Ganti `name` menjadi `Service Runbook` dan `description_stripped` menjadi "Service runbook: summary, health checks, common tasks, and the escalation path for this service."; pertahankan `id`, `project_id`, `type: PROJECT`, `access: 0`. Isi `description_html` (pertahankan atribut existing: `xmlns="http://www.w3.org/1999/xhtml"` pada blok, `class="editor-paragraph-block"`, `class="editor-heading-block"`, `horizontalRule`, dan atribut tabel `colwidth/background/hidecontent`):

- P: "Welcome to your <strong>Service Runbook</strong> — the page your team opens at 3 a.m. Keep it short, current, and honest. Replace the examples below with your service's details."
- HR
- H2: "🧭 Service summary" + tabel Field/Details dengan baris: Service name ("✏ Add your service name"), Owner ("Add the team or person accountable"), Status ("🟢 Healthy / 🟡 Degraded / 🔴 Down"), On-call ("Who is paged, and in which rotation"), Dependencies ("What this service needs to run"), Related modules ("Link the modules that carry this service's work"), Cycles ("The cycle this service is planned in")
- HR
- H2: "🚦 Health checks" + UL: "Dashboards and alerts this service depends on" / "Last known failure mode and how to spot it" / "Where the logs live" / "Who to escalate to, and when"
- HR
- H2: "🛠️ Common tasks" + tabel Task/Steps: "Restart the service" → "Drain traffic, restart the process, watch the health check for 10 minutes."; "Rotate credentials" → "Update the secret in the vault, redeploy, verify integrations, then revoke the old key."; "Scale up for peak load" → "Raise replica count, confirm latency, and note the change in this page."
- HR
- H2: "📣 Escalation path" + OL: "L1 triage — confirm impact and check the runbook." / "L2 owner — page the service owner if the fix isn't documented." / "Incident manager — if customer impact is confirmed, open an incident work item and link it here."
- P: "Update this page after every incident — the next person on call will thank you."

- [ ] **Step 2: Tulis ulang Page 2 — Incident Postmortem (Template)**

Ganti `name` menjadi `Incident Postmortem (Template)` dan `description_stripped` menjadi "Blameless postmortem template: summary, impact, timeline, root cause, and action items."; pertahankan `access: 1`. Isi `description_html`:

- P: "A blameless postmortem. Focus on what happened and what changes, not who to blame. Copy this page for each incident."
- HR
- H2: "📝 Summary" + P: "What happened, in two or three sentences. Include the customer impact."
- H2: "📉 Impact" + UL: "Services affected" / "Duration (start → end)" / "Customers affected" / "Linked work items"
- H2: "🕒 Timeline" + tabel Time/Event: "Detection" → "When monitoring or a customer first flagged it"; "First response" → "Who picked it up and what they tried"; "Mitigation" → "What stopped the impact"; "Resolution" → "What fixed it for good"
- H2: "🔍 Root cause" + P: "What actually failed — include contributing factors, not just the trigger."
- H2: "✅ What went well" + UL: "What worked in detection, response, or communication?"
- H2: "🧯 Action items" + tabel Action/Owner/Due/Work item dengan baris contoh: "Add an alert for the failing dependency" / "Service owner" / "Within 2 weeks" / "Link the work item"; "Document the recovery step in the runbook" / "On-call engineer" / "Within 1 week" / "Link the work item"
- P: "File action items as work items and link them here so follow-up doesn't get lost."

- [ ] **Step 3: Verifikasi JSON**

Run:

```bash
node -e "
const fs=require('fs');
const j=JSON.parse(fs.readFileSync('apps/api-rs/crates/api/assets/seeds/data/pages.json','utf8'));
console.log(j.map(p=>p.name+' | '+p.type+' | access '+p.access+' | html '+p.description_html.length+' | stripped '+p.description_stripped.length).join('\n'));
"
```

Expected: 2 page dengan nama baru, `type: PROJECT`, HTML dan stripped non-kosong.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/assets/seeds/data/pages.json
git commit -m "content(seed): service runbook + postmortem template"
```

---

### Task 13: Verifikasi E2E + deploy

**Files:** — (tidak ada perubahan kode)

- [ ] **Step 1: Seluruh test Rust + lint**

Run:

```bash
cargo fmt --all --check
cargo test -p api 2>&1 | tail -20
```

Expected: `test result: ok` (unit + gate; test DB-dependent perlu `DATABASE_URL` untuk `workspace_seed_test` dan `issue_create_test`).

- [ ] **Step 2: Rebuild image + deploy**

```bash
docker compose -f docker-compose-local.yml up -d --build api
systemctl --user reload plane-backend.service
```

Expected: container `api` sehat (`docker ps`), log tidak memuat `workspace seed failed`.

- [ ] **Step 3: Buat workspace baru dan inspeksi**

Lewat UI (tunnel): buat workspace baru → project demo langsung muncul dengan nama workspace, berisi 7 work item ITSM, 2 page (Service Runbook, Incident Postmortem), 1 view "Urgent requests", 2 cycle, 3 module, label incident/service-request.

Cek DB:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tAc "
SELECT p.name, p.identifier,
 (SELECT COUNT(*) FROM issues i WHERE i.project_id=p.id) AS issues,
 (SELECT COUNT(*) FROM pages pg JOIN project_pages pp ON pp.page_id=pg.id WHERE pp.project_id=p.id) AS pages,
 (SELECT COUNT(*) FROM cycles c WHERE c.project_id=p.id) AS cycles,
 (SELECT COUNT(*) FROM modules m WHERE m.project_id=p.id) AS modules
FROM projects p WHERE p.name = '<nama workspace>' AND p.deleted_at IS NULL;"
```

Expected: 1 baris dengan angka 7/2/2/3.

- [ ] **Step 4: Commit sisa (bila ada)**

```bash
git status --short
```

Expected: bersih (selain file yang memang tidak terkait).

---

## Catatan eksekusi

- Test DB-dependent butuh Postgres lokal di `localhost:5432` (port ter-mapping di `docker-compose-local.yml`).
- `include_str!` berarti setiap perubahan JSON butuh rebuild image.
- Jangan mengubah `id`, mapping relasi, atau `sequence_id` di JSON — integration test dan parity bergantung padanya.
- Workspace lama tidak mendapat demo (non-retroaktif) — sesuai spec.
