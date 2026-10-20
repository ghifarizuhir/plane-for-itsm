# Galileo Tools Fase 1C — Services + Intake Read — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah lima read tool Batch 1.2 — `list_services`, `get_service`, `list_intake_items`, `get_intake_item`, `count_intake_items` — plus helper feature gate project (`intake_view`, dipakai juga oleh Plan 1D).

**Architecture:** Tool services di `tools/services.rs` (resolver service: UUID atau nama dengan deteksi ambigu, dependency graph, work item terbuka, count per state group). Tool intake di `tools/intake.rs` (status intake `-2..2` dipetakan dari label, suggestion triage dibaca dari `intake_triage_suggestions`). Semua tool memakai `resolve_project` + guard `project_members` dari Plan 1A, dan `ensure_feature` baru untuk gate `intake_view`. Scheduled run eligible (read-only).

**Tech Stack:** Rust (axum, sqlx, Rig 0.42), serde/schemars; perintah Rust dari `apps/api-rs`.

**Prasyarat:** Plan 1A dan 1B sudah dieksekusi.

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4.3, §4.5, §4.6, §5.2).

---

## File Structure

| File                                     | Tanggung jawab                                                                                       |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crates/ai/src/tools/mod.rs`             | `ProjectFeature` + `feature_column` + `ensure_feature`; module decl + re-export `services`, `intake` |
| `crates/ai/src/tools/services.rs` (baru) | `ListServices`, `GetService`, `resolve_service`, `pick_service`, SQL + JSON helper                   |
| `crates/ai/src/tools/intake.rs` (baru)   | `ListIntakeItems`, `GetIntakeItem`, `CountIntakeItems`, `intake_status_arg`, SQL + JSON helper       |

---

### Task 1: Helper feature gate project

**Files:**

- Modify: `crates/ai/src/tools/mod.rs`
- Test: `crates/ai/src/tools/mod.rs` (`mod tests`)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
#[test]
fn feature_column_maps_each_project_gate() {
    assert_eq!(
        feature_column(ProjectFeature::Cycles),
        ("cycle_view", "sprints")
    );
    assert_eq!(
        feature_column(ProjectFeature::Modules),
        ("module_view", "tracks")
    );
    assert_eq!(feature_column(ProjectFeature::Pages), ("page_view", "knowledge base"));
    assert_eq!(feature_column(ProjectFeature::Intake), ("intake_view", "intake"));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai feature_column`
Expected: gagal kompilasi — `ProjectFeature`/`feature_column` belum ada.

- [ ] **Step 3: Implementasi di `tools/mod.rs`**

```rust
/// Project feature gates exposed to tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectFeature {
    Cycles,
    Modules,
    Pages,
    Intake,
}

/// `(column, human label)` for one feature gate.
pub fn feature_column(feature: ProjectFeature) -> (&'static str, &'static str) {
    match feature {
        ProjectFeature::Cycles => ("cycle_view", "sprints"),
        ProjectFeature::Modules => ("module_view", "tracks"),
        ProjectFeature::Pages => ("page_view", "knowledge base"),
        ProjectFeature::Intake => ("intake_view", "intake"),
    }
}

/// Reject a tool call when the project feature is disabled, with a
/// model-visible message instead of an empty result.
pub async fn ensure_feature(
    pool: &PgPool,
    project_id: Uuid,
    feature: ProjectFeature,
) -> Result<(), ToolExecutionError> {
    let (column, label) = feature_column(feature);
    let sql = format!("SELECT {column} FROM projects WHERE id = $1 AND deleted_at IS NULL");
    let enabled: Option<bool> = sqlx::query_scalar(&sql)
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?;
    match enabled {
        Some(true) => Ok(()),
        Some(false) => Err(ToolExecutionError::invalid_args(format!(
            "the {label} feature is disabled in this project"
        ))),
        None => Err(ToolExecutionError::invalid_args("project was not found")),
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai feature_column`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add project feature gate helper"
```

---

### Task 2: Tool `list_services`

**Files:**

- Create: `crates/ai/src/tools/services.rs`
- Modify: `crates/ai/src/tools/mod.rs` (module decl + re-export)

- [ ] **Step 1: Tulis test yang gagal**

Buat `tools/services.rs` dengan header + tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn service_allowlist_rejects_unknown_values() {
        assert!(service_allowlist(Some("Active"), SERVICE_STATUSES, "status").unwrap().is_some());
        assert!(service_allowlist(Some("burning"), SERVICE_STATUSES, "status").is_err());
        assert!(service_allowlist(Some("CRITICAL"), SERVICE_CRITICALITIES, "criticality").unwrap().is_some());
        assert!(service_allowlist(Some("nope"), SERVICE_TYPES, "type").is_err());
        assert_eq!(service_allowlist(None, SERVICE_TYPES, "type").unwrap(), None);
    }

    #[test]
    fn services_json_shape() {
        let rows = vec![ServiceRow {
            id: Uuid::nil(),
            name: "API".to_string(),
            description: String::new(),
            status: "active".to_string(),
            criticality: "critical".to_string(),
            service_type: "internal".to_string(),
            owner: "Budi".to_string(),
            project: "LTS".to_string(),
            repository_url: None,
            documentation_url: None,
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&services_json(&rows)).unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["name"], json!("API"));
        assert_eq!(parsed["items"][0]["type"], json!("internal"));
        assert_eq!(parsed["items"][0]["owner"], json!("Budi"));
    }

    #[test]
    fn services_sql_is_scoped_and_guarded() {
        assert!(SERVICES_SQL.contains("workspace_id = $1"));
        assert!(SERVICES_SQL.contains("project_members"));
        assert!(SERVICES_SQL.contains("sv.deleted_at IS NULL"));
    }
}
```

Header file:

```rust
//! Service catalog read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{clamp_limit, db_error, optional_text, resolve_project, schema_of};
use crate::agent::{record, ToolTrace};
```

Tambahkan di `tools/mod.rs`:

```rust
pub mod services;
```

dan `pub use services::*;`.

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai services_json_shape`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/services.rs`**

```rust
pub const SERVICE_STATUSES: [&str; 5] = ["active", "planned", "maintenance", "deprecated", "retired"];
pub const SERVICE_CRITICALITIES: [&str; 4] = ["critical", "high", "medium", "low"];
pub const SERVICE_TYPES: [&str; 4] = ["internal", "external", "infrastructure", "third_party"];

pub fn service_allowlist(
    value: Option<&str>,
    allowed: &[&str],
    field: &str,
) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if allowed.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "{field} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub criticality: String,
    pub service_type: String,
    pub owner: String,
    pub project: String,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
}

pub const SERVICES_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.workspace_id = $1 AND sv.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($3::text IS NULL OR sv.name ILIKE '%' || $3 || '%') \
     AND ($4::text IS NULL OR sv.status = $4) \
     AND ($5::text IS NULL OR sv.criticality = $5) \
     AND ($6::text IS NULL OR sv.\"type\" = $6) \
     AND ($7::text IS NULL OR p.identifier ILIKE $7 OR p.name ILIKE '%' || $7 || '%') \
     ORDER BY sv.name LIMIT $8";

pub fn services_json(rows: &[ServiceRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "project": row.project,
                "status": row.status,
                "criticality": row.criticality,
                "type": row.service_type,
                "owner": row.owner,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListServicesArgs {
    /// Case-insensitive substring to match against service names.
    pub query: Option<String>,
    /// One of: active, planned, maintenance, deprecated, retired.
    pub status: Option<String>,
    /// One of: critical, high, medium, low.
    pub criticality: Option<String>,
    /// One of: internal, external, infrastructure, third_party.
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListServices {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListServices {
    const NAME: &'static str = "list_services";
    type Args = ListServicesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List services in the service catalog the caller can access, optionally \
         filtered by name substring, lifecycle status, criticality, type, and \
         project. Returns name, project, status, criticality, type, and owner."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListServicesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let status = service_allowlist(args.status.as_deref(), &SERVICE_STATUSES, "status")?;
        let criticality =
            service_allowlist(args.criticality.as_deref(), &SERVICE_CRITICALITIES, "criticality")?;
        let service_type =
            service_allowlist(args.service_type.as_deref(), &SERVICE_TYPES, "type")?;
        let project = optional_text(args.project.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<ServiceRow> = sqlx::query_as(SERVICES_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(&query)
            .bind(&status)
            .bind(&criticality)
            .bind(&service_type)
            .bind(&project)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(services_json(&rows))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai services`
Expected: 3 test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/services.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add list_services read tool"
```

---

### Task 3: Tool `get_service` + resolver service

**Files:**

- Modify: `crates/ai/src/tools/services.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/services.rs`:

```rust
    fn service(name: &str, project: &str) -> ServiceRow {
        ServiceRow {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            status: "active".to_string(),
            criticality: "high".to_string(),
            service_type: "internal".to_string(),
            owner: String::new(),
            project: project.to_string(),
            repository_url: None,
            documentation_url: None,
        }
    }

    #[test]
    fn pick_service_prefers_exact_then_unique_substring() {
        let rows = vec![service("API", "LTS"), service("API Gateway", "OPS")];
        assert_eq!(pick_service(rows.clone(), "api").unwrap().name, "API");
        assert_eq!(pick_service(rows.clone(), "gateway").unwrap().name, "API Gateway");
        assert!(pick_service(rows, "a").is_err());
        assert!(pick_service(vec![], "api").is_err());
    }

    #[test]
    fn pick_service_is_ambiguous_when_exact_matches_multiple_projects() {
        let rows = vec![service("API", "LTS"), service("API", "OPS")];
        let error = pick_service(rows, "API").unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
        assert!(error.to_string().contains("LTS/API"));
    }

    #[test]
    fn service_detail_json_shape() {
        let row = service("API", "LTS");
        let dependencies = vec![DependencyRow {
            direction: "depends_on".to_string(),
            name: "Database".to_string(),
            status: "active".to_string(),
        }];
        let items = vec![ServiceItemRow {
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "urgent".to_string(),
        }];
        let counts = vec![StateCountRow {
            state_group: "started".to_string(),
            count: 3,
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&service_detail_json(&row, &dependencies, &items, &counts)).unwrap();
        assert_eq!(parsed["name"], json!("API"));
        assert_eq!(parsed["dependencies"][0]["direction"], json!("depends_on"));
        assert_eq!(parsed["open_work_items"][0]["identifier"], json!("LTS-42"));
        assert_eq!(parsed["state_counts"][0]["count"], json!(3));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pick_service`
Expected: gagal kompilasi — `pick_service`/`ServiceItemRow` belum ada.

- [ ] **Step 3: Implementasi di `tools/services.rs`**

Tambahkan `ProjectRow` ke baris `use super::{...}` di header file (`resolve_service` memakainya), lalu tambahkan:

```rust
pub const SERVICE_BY_ID_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.id = $1 AND sv.workspace_id = $2 AND sv.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

pub const SERVICES_BY_NAME_PROJECT_SQL: &str = "SELECT sv.id, sv.name, sv.description, \
     sv.status, sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.project_id = $1 AND sv.workspace_id = $2 AND sv.deleted_at IS NULL \
     AND (lower(sv.name) = lower($3) OR sv.name ILIKE '%' || $3 || '%') \
     ORDER BY sv.name LIMIT 10";

pub const SERVICES_BY_NAME_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN projects p ON p.id = sv.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.workspace_id = $1 AND sv.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(sv.name) = lower($3) OR sv.name ILIKE '%' || $3 || '%') \
     ORDER BY sv.name LIMIT 10";

/// Pick one service: a single exact name match, else a unique substring.
pub fn pick_service(
    rows: Vec<ServiceRow>,
    reference: &str,
) -> Result<ServiceRow, ToolExecutionError> {
    let needle = reference.trim();
    if rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "service '{needle}' was not found or is not accessible"
        )));
    }
    let exact: Vec<&ServiceRow> = rows
        .iter()
        .filter(|row| row.name.eq_ignore_ascii_case(needle))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }
    if exact.len() > 1 {
        let names = exact
            .iter()
            .map(|row| format!("{}/{}", row.project, row.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolExecutionError::invalid_args(format!(
            "service '{needle}' is ambiguous; use one of: {names}"
        )));
    }
    if rows.len() == 1 {
        return Ok(rows.into_iter().next().expect("one row"));
    }
    let names = rows
        .iter()
        .map(|row| format!("{}/{}", row.project, row.name))
        .collect::<Vec<_>>()
        .join(", ");
    Err(ToolExecutionError::invalid_args(format!(
        "service '{needle}' is ambiguous; use one of: {names}"
    )))
}

pub async fn resolve_service(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
    project: Option<&ProjectRow>,
) -> Result<ServiceRow, ToolExecutionError> {
    if let Ok(id) = Uuid::parse_str(reference.trim()) {
        let row: Option<ServiceRow> = sqlx::query_as(SERVICE_BY_ID_SQL)
            .bind(id)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
        return row.ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "service '{reference}' was not found or is not accessible"
            ))
        });
    }
    let rows: Vec<ServiceRow> = match project {
        Some(project) => {
            sqlx::query_as(SERVICES_BY_NAME_PROJECT_SQL)
                .bind(project.id)
                .bind(workspace_id)
                .bind(reference.trim())
                .fetch_all(pool)
                .await
                .map_err(db_error)?
        }
        None => sqlx::query_as(SERVICES_BY_NAME_SQL)
            .bind(workspace_id)
            .bind(user_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
    };
    pick_service(rows, reference)
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DependencyRow {
    pub direction: String,
    pub name: String,
    pub status: String,
}

pub const SERVICE_DEPENDENCIES_SQL: &str = "SELECT \
     CASE WHEN d.from_service_id = $1 THEN 'depends_on' ELSE 'needed_by' END AS direction, \
     sv.name, sv.status \
     FROM service_dependencies d \
     JOIN services sv ON sv.id = CASE WHEN d.from_service_id = $1 \
       THEN d.to_service_id ELSE d.from_service_id END \
     WHERE (d.from_service_id = $1 OR d.to_service_id = $1) AND d.deleted_at IS NULL \
     AND sv.deleted_at IS NULL \
     ORDER BY direction, sv.name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct ServiceItemRow {
    pub identifier: String,
    pub name: String,
    pub state: String,
    pub state_group: String,
    pub priority: String,
}

pub const SERVICE_OPEN_ITEMS_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, COALESCE(s.\"group\", '') AS state_group, i.priority \
     FROM service_issues si \
     JOIN issues i ON i.id = si.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE si.service_id = $1 AND si.deleted_at IS NULL \
     AND s.\"group\" NOT IN ('completed', 'cancelled') \
     ORDER BY i.updated_at DESC LIMIT 10";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct StateCountRow {
    pub state_group: String,
    pub count: i64,
}

pub const SERVICE_STATE_COUNTS_SQL: &str = "SELECT s.\"group\" AS state_group, \
     count(*)::int8 AS count \
     FROM service_issues si \
     JOIN issues i ON i.id = si.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE si.service_id = $1 AND si.deleted_at IS NULL \
     GROUP BY s.\"group\" ORDER BY s.\"group\"";

pub fn service_detail_json(
    row: &ServiceRow,
    dependencies: &[DependencyRow],
    open_items: &[ServiceItemRow],
    counts: &[StateCountRow],
) -> String {
    json!({
        "name": row.name,
        "project": row.project,
        "description": crate::tools::work_items::truncate_chars(&row.description, 2000),
        "status": row.status,
        "criticality": row.criticality,
        "type": row.service_type,
        "owner": row.owner,
        "repository_url": row.repository_url,
        "documentation_url": row.documentation_url,
        "dependencies": dependencies,
        "open_work_items": open_items,
        "state_counts": counts,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetServiceArgs {
    /// Service name, or a service id when known.
    pub service: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
}

pub struct GetService {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetService {
    const NAME: &'static str = "get_service";
    type Args = GetServiceArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one service from the catalog by name or id: status, criticality, \
         type, owner, description, links, its dependencies (depends_on / \
         needed_by), the open work items linked to it, and the count of linked \
         work items per state group."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetServiceArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        let row = resolve_service(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.service,
            project.as_ref(),
        )
        .await?;
        let dependencies: Vec<DependencyRow> = sqlx::query_as(SERVICE_DEPENDENCIES_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let open_items: Vec<ServiceItemRow> = sqlx::query_as(SERVICE_OPEN_ITEMS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let counts: Vec<StateCountRow> = sqlx::query_as(SERVICE_STATE_COUNTS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(service_detail_json(&row, &dependencies, &open_items, &counts))
    }
}
```

Catatan: `truncate_chars` di `tools/work_items.rs` sudah `pub` dari Plan 1A, jadi bisa dipakai lintas modul.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai pick_service`
Expected: seluruh test services lulus (jalankan juga `cargo test -p ai service_detail_json` bila perlu).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/services.rs
git commit -m "feat(ai): add get_service read tool"
```

---

### Task 4: Tool `list_intake_items` + `count_intake_items`

**Files:**

- Create: `crates/ai/src/tools/intake.rs`
- Modify: `crates/ai/src/tools/mod.rs` (module decl + re-export)

- [ ] **Step 1: Tulis test yang gagal**

Buat `tools/intake.rs` dengan header + tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn intake_status_arg_maps_labels_and_defaults_to_pending() {
        assert_eq!(intake_status_arg(None).unwrap(), -2);
        assert_eq!(intake_status_arg(Some(" PENDING ")).unwrap(), -2);
        assert_eq!(intake_status_arg(Some("accepted")).unwrap(), 1);
        assert_eq!(intake_status_arg(Some("duplicate")).unwrap(), 2);
        assert!(intake_status_arg(Some("maybe")).is_err());
    }

    #[test]
    fn status_label_roundtrips_every_code() {
        for (label, code) in INTAKE_STATUSES {
            assert_eq!(status_label(code), label);
        }
    }

    #[test]
    fn intake_items_json_shapes_suggestion_or_null() {
        let rows = vec![
            IntakeItemRow {
                id: Uuid::nil(),
                identifier: "LTS-7".to_string(),
                name: "Alert: disk full".to_string(),
                priority: "none".to_string(),
                source: "webhook".to_string(),
                created_at: "2026-10-01 00:00:00+00".to_string(),
                project: "LTS".to_string(),
                suggestion_status: Some("ready".to_string()),
                category_label: Some("Incident".to_string()),
                severity_priority: Some("high".to_string()),
                service_label: Some("API".to_string()),
                needs_human: Some(0.2),
            },
            IntakeItemRow {
                id: Uuid::new_v4(),
                identifier: "LTS-8".to_string(),
                name: "Manual request".to_string(),
                priority: "low".to_string(),
                source: String::new(),
                created_at: "2026-10-02 00:00:00+00".to_string(),
                project: "LTS".to_string(),
                suggestion_status: None,
                category_label: None,
                severity_priority: None,
                service_label: None,
                needs_human: None,
            },
        ];
        let parsed: serde_json::Value =
            serde_json::from_str(&intake_items_json(&rows, "pending")).unwrap();
        assert_eq!(parsed["status"], json!("pending"));
        assert_eq!(parsed["items"][0]["suggestion"]["category"], json!("Incident"));
        assert_eq!(parsed["items"][0]["suggestion"]["service"], json!("API"));
        assert_eq!(parsed["items"][1]["suggestion"], json!(null));
    }

    #[test]
    fn intake_sql_is_scoped_guarded_and_filtered_by_status() {
        for sql in [INTAKE_ITEMS_SQL, INTAKE_COUNT_SQL] {
            assert!(sql.contains("workspace_id = $1"));
            assert!(sql.contains("project_members"));
            assert!(sql.contains("ii.status = $3"));
        }
    }
}
```

Header file:

```rust
//! Intake (triage inbox) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, optional_text, priority_arg, resolve_project, schema_of, ProjectFeature,
};
use crate::agent::{record, ToolTrace};
```

Tambahkan di `tools/mod.rs`: `pub mod intake;` dan `pub use intake::*;`.

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai intake_status_arg`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/intake.rs`**

```rust
/// Intake status codes, mirroring Django `intake.py` status choices.
pub const INTAKE_STATUSES: [(&str, i32); 5] = [
    ("pending", -2),
    ("rejected", -1),
    ("snoozed", 0),
    ("accepted", 1),
    ("duplicate", 2),
];

pub fn intake_status_arg(value: Option<&str>) -> Result<i32, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(-2);
    };
    INTAKE_STATUSES
        .iter()
        .find(|(label, _)| *label == normalized)
        .map(|(_, code)| *code)
        .ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "status must be one of: {}",
                INTAKE_STATUSES
                    .iter()
                    .map(|(label, _)| *label)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

pub fn status_label(code: i32) -> &'static str {
    INTAKE_STATUSES
        .iter()
        .find(|(_, value)| *value == code)
        .map(|(label, _)| *label)
        .unwrap_or("unknown")
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct IntakeItemRow {
    pub id: Uuid,
    pub identifier: String,
    pub name: String,
    pub priority: String,
    pub source: String,
    pub created_at: String,
    pub project: String,
    pub suggestion_status: Option<String>,
    pub category_label: Option<String>,
    pub severity_priority: Option<String>,
    pub service_label: Option<String>,
    pub needs_human: Option<f64>,
}

pub const INTAKE_ITEMS_SQL: &str = "SELECT i.id, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, i.priority, \
     COALESCE(ii.source, '') AS source, ii.created_at::text AS created_at, \
     p.identifier AS project, s.status AS suggestion_status, s.category_label, \
     s.severity_priority, s.service_label, s.needs_human \
     FROM intake_issues ii \
     JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = ii.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     LEFT JOIN intake_triage_suggestions s ON s.intake_issue_id = ii.id \
     WHERE ii.workspace_id = $1 AND ii.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ii.status = $3 \
     AND ($4::uuid IS NULL OR p.id = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     AND ($6::text IS NULL OR i.name ILIKE '%' || $6 || '%') \
     ORDER BY ii.created_at DESC LIMIT $7";

pub const INTAKE_COUNT_SQL: &str = "SELECT count(*)::int8 \
     FROM intake_issues ii \
     JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = ii.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     WHERE ii.workspace_id = $1 AND ii.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ii.status = $3 \
     AND ($4::uuid IS NULL OR p.id = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     AND ($6::text IS NULL OR i.name ILIKE '%' || $6 || '%')";

pub fn intake_items_json(rows: &[IntakeItemRow], status: &str) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let suggestion = row.suggestion_status.as_ref().map(|suggestion_status| {
                json!({
                    "status": suggestion_status,
                    "category": row.category_label,
                    "severity": row.severity_priority,
                    "service": row.service_label,
                    "needs_human": row.needs_human,
                })
            });
            json!({
                "id": row.id,
                "identifier": row.identifier,
                "name": row.name,
                "priority": row.priority,
                "source": row.source,
                "created_at": row.created_at,
                "project": row.project,
                "suggestion": suggestion,
            })
        })
        .collect();
    json!({"status": status, "returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListIntakeItemsArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: pending, rejected, snoozed, accepted, duplicate. Defaults to pending.
    pub status: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Case-insensitive substring to match against item names.
    pub query: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListIntakeItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListIntakeItems {
    const NAME: &'static str = "list_intake_items";
    type Args = ListIntakeItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List intake (triage inbox) items the caller can access, newest first, \
         with their triage suggestion summary when one exists \
         (category, severity, service, needs_human). Defaults to pending items. \
         Use the returned id with get_intake_item."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListIntakeItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                super::ensure_feature(&self.pool, project.id, ProjectFeature::Intake).await?;
                Some(project)
            }
            None => None,
        };
        let status = intake_status_arg(args.status.as_deref())?;
        let priority = priority_arg(args.priority.as_deref())?;
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<IntakeItemRow> = sqlx::query_as(INTAKE_ITEMS_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(status)
            .bind(project.as_ref().map(|row| row.id))
            .bind(&priority)
            .bind(&query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(intake_items_json(&rows, status_label(status)))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CountIntakeItemsArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: pending, rejected, snoozed, accepted, duplicate. Defaults to pending.
    pub status: Option<String>,
}

pub struct CountIntakeItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for CountIntakeItems {
    const NAME: &'static str = "count_intake_items";
    type Args = CountIntakeItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Count intake (triage inbox) items the caller can access for one \
         status (default pending), optionally scoped to a project."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CountIntakeItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                super::ensure_feature(&self.pool, project.id, ProjectFeature::Intake).await?;
                Some(project)
            }
            None => None,
        };
        let status = intake_status_arg(args.status.as_deref())?;
        let count: i64 = sqlx::query_scalar(INTAKE_COUNT_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(status)
            .bind(project.as_ref().map(|row| row.id))
            .bind(None::<String>)
            .bind(None::<String>)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(json!({
            "count": count,
            "status": status_label(status),
            "project": project.as_ref().map(|row| row.identifier.clone()),
        })
        .to_string())
    }
}
```

Catatan: `INTAKE_COUNT_SQL` tidak memakai filter priority/query, jadi dua bind terakhir dikirim `None::<String>`; count hanya mendukung project + status (sesuai spec §5.2).

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai intake`
Expected: seluruh test intake lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/intake.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add intake list and count read tools"
```

---

### Task 5: Tool `get_intake_item`

**Files:**

- Modify: `crates/ai/src/tools/intake.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/intake.rs`:

```rust
    #[tokio::test]
    async fn get_intake_item_rejects_a_non_uuid_before_querying() {
        let tool = GetIntakeItem {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                GetIntakeItemArgs {
                    intake_item: "LTS-7".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("uuid"));
    }

    #[test]
    fn intake_detail_json_shape() {
        let row = IntakeDetailRow {
            intake_issue_id: Uuid::nil(),
            identifier: "LTS-7".to_string(),
            name: "Alert: disk full".to_string(),
            priority: "high".to_string(),
            source: "webhook".to_string(),
            intake_status: -2,
            snoozed_till: None,
            created_at: "2026-10-01 00:00:00+00".to_string(),
            project: "LTS".to_string(),
            intake_view: true,
        };
        let suggestion = SuggestionRow {
            status: "ready".to_string(),
            model: Some("test-model".to_string()),
            category_label: Some("Incident".to_string()),
            category_confidence: Some(0.9),
            severity_priority: Some("high".to_string()),
            severity_score: Some(0.8),
            severity_confidence: Some(0.7),
            service_label: Some("API".to_string()),
            service_confidence: Some(0.6),
            needs_human: Some(0.1),
            applied_fields: vec!["category".to_string()],
            dismissed_fields: vec![],
        };
        let parsed: serde_json::Value =
            serde_json::from_str(&intake_detail_json(&row, Some(&suggestion))).unwrap();
        assert_eq!(parsed["identifier"], json!("LTS-7"));
        assert_eq!(parsed["status"], json!("pending"));
        assert_eq!(parsed["suggestion"]["service_confidence"], json!(0.6));
        assert_eq!(parsed["suggestion"]["applied_fields"][0], json!("category"));

        let without: serde_json::Value =
            serde_json::from_str(&intake_detail_json(&row, None)).unwrap();
        assert_eq!(without["suggestion"], json!(null));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai intake_detail_json`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/intake.rs`**

```rust
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct IntakeDetailRow {
    pub intake_issue_id: Uuid,
    pub identifier: String,
    pub name: String,
    pub priority: String,
    pub source: String,
    pub intake_status: i32,
    pub snoozed_till: Option<String>,
    pub created_at: String,
    pub project: String,
    pub intake_view: bool,
}

pub const INTAKE_DETAIL_SQL: &str = "SELECT ii.id AS intake_issue_id, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, i.priority, \
     COALESCE(ii.source, '') AS source, ii.status AS intake_status, \
     ii.snoozed_till::text AS snoozed_till, ii.created_at::text AS created_at, \
     p.identifier AS project, p.intake_view AS intake_view \
     FROM intake_issues ii \
     JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = ii.project_id AND p.deleted_at IS NULL \
     WHERE ii.issue_id = $1 AND ii.workspace_id = $2 AND ii.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SuggestionRow {
    pub status: String,
    pub model: Option<String>,
    pub category_label: Option<String>,
    pub category_confidence: Option<f64>,
    pub severity_priority: Option<String>,
    pub severity_score: Option<f64>,
    pub severity_confidence: Option<f64>,
    pub service_label: Option<String>,
    pub service_confidence: Option<f64>,
    pub needs_human: Option<f64>,
    pub applied_fields: Vec<String>,
    pub dismissed_fields: Vec<String>,
}

pub const INTAKE_SUGGESTION_SQL: &str = "SELECT status, model, category_label, \
     category_confidence, severity_priority, severity_score, severity_confidence, \
     service_label, service_confidence, needs_human, applied_fields, dismissed_fields \
     FROM intake_triage_suggestions WHERE intake_issue_id = $1";

pub fn intake_detail_json(row: &IntakeDetailRow, suggestion: Option<&SuggestionRow>) -> String {
    json!({
        "id": row.intake_issue_id,
        "identifier": row.identifier,
        "name": row.name,
        "priority": row.priority,
        "source": row.source,
        "status": status_label(row.intake_status),
        "snoozed_till": row.snoozed_till,
        "created_at": row.created_at,
        "project": row.project,
        "suggestion": suggestion,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetIntakeItemArgs {
    /// Intake item id (uuid) as returned by list_intake_items.
    pub intake_item: String,
}

pub struct GetIntakeItem {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetIntakeItem {
    const NAME: &'static str = "get_intake_item";
    type Args = GetIntakeItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one intake (triage inbox) item by id with its full AI triage \
         suggestion: category, severity, and service with confidence scores, \
         which fields were already applied or dismissed, and needs_human."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetIntakeItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let intake_item = Uuid::parse_str(args.intake_item.trim()).map_err(|_| {
            ToolExecutionError::invalid_args("intake_item must be a uuid from list_intake_items")
        })?;
        let row: Option<IntakeDetailRow> = sqlx::query_as(INTAKE_DETAIL_SQL)
            .bind(intake_item)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Err(ToolExecutionError::invalid_args(format!(
                "intake item '{}' was not found or is not accessible",
                args.intake_item
            )));
        };
        if !row.intake_view {
            return Err(ToolExecutionError::invalid_args(
                "the intake feature is disabled in this project",
            ));
        }
        let suggestion: Option<SuggestionRow> = sqlx::query_as(INTAKE_SUGGESTION_SQL)
            .bind(row.intake_issue_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(intake_detail_json(&row, suggestion.as_ref()))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai intake`
Expected: seluruh test intake lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/intake.rs
git commit -m "feat(ai): add get_intake_item read tool"
```

---

### Task 6: Registrasi + verifikasi akhir

**Files:**

- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Daftarkan 5 tool baru di kedua registry**

Tambahkan ke rantai `.tool(...)` `workspace_tools` (setelah `ListWorkItemTypes`):

```rust
        .tool(ListServices {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetService {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListIntakeItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetIntakeItem {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(CountIntakeItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
```

Tambahkan blok kondisional yang sama di `read_tools` sebelum `server.run()`
(pola `if allowed.contains(&TYPE::NAME) { server = server.tool(TYPE { pool: pool.clone(), workspace_id, user_id, trace: trace.clone() }); }` untuk tiap tipe; tool terakhir memakai `pool` tanpa clone).

- [ ] **Step 2: Tambah test metadata**

```rust
#[tokio::test]
async fn service_and_intake_tools_expose_metadata() {
    let pool = lazy_pool();
    let trace = crate::agent::new_trace();
    let services = ListServices {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListServices::NAME, "list_services");
    assert!(services.parameters()["properties"]["type"].is_object());
    let get_service = GetService {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(GetService::NAME, "get_service");
    let intake = ListIntakeItems {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListIntakeItems::NAME, "list_intake_items");
    assert!(intake.parameters()["properties"]["status"].is_object());
    let detail = GetIntakeItem {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(GetIntakeItem::NAME, "get_intake_item");
    let count = CountIntakeItems {
        pool,
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace,
    };
    assert_eq!(CountIntakeItems::NAME, "count_intake_items");
    assert!(!count.description().is_empty());
}
```

- [ ] **Step 3: Sinkronkan allowlist schedule (backend + FE + PREAMBLE)**

Ganti `SPEC_TOOLS` di `crates/ai/src/schedule.rs` menjadi:

```rust
pub const SPEC_TOOLS: [&str; 15] = [
    "list_projects",
    "count_work_items",
    "search_work_items",
    "get_work_item",
    "list_work_item_comments",
    "list_work_item_relations",
    "list_members",
    "list_states",
    "list_labels",
    "list_work_item_types",
    "list_services",
    "get_service",
    "list_intake_items",
    "get_intake_item",
    "count_intake_items",
];
```

Perbarui test `spec_accepts_every_spec_tool` di `schedule.rs` (`assert_eq!(SPEC_TOOLS.len(), 15);`)
dan test worker `allowed_tools_legacy_is_all_read_tools` di
`crates/worker/src/handlers/ai_schedule.rs` dengan menambahkan lima nama baru
di akhir vec. Tambahkan juga lima nama itu ke `AI_SCHEDULE_TOOLS` di
`apps/web/core/lib/ai-schedule.ts` dan ke ekspektasi test
`apps/web/core/lib/ai-schedule.test.ts`.

Terakhir, di `PREAMBLE` (`crates/ai/src/agent.rs`) ganti frasa:

```
projects, work items, people, and their metadata
```

menjadi:

```
projects, work items, services, the intake triage queue, people, and their metadata
```

- [ ] **Step 4: Verifikasi**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

Run: `cargo test -p worker allowed_tools`
Expected: lulus.

Run: `cargo build -p api -p worker`
Expected: sukses tanpa warning.

Run (dari `apps/web`): `pnpm test core/lib/ai-schedule.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/schedule.rs apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/api-rs/crates/ai/src/agent.rs apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): register service and intake read tools"
```

---

### Task 7: Smoke manual (opsional)

- [ ] Rebuild backend (perintah `setsid docker compose ...` dari `AGENTS.md`), lalu uji chat:
  1. `service apa saja yang active` — `list_services` + filter status.
  2. `detail service API` — dependency + work item terbuka.
  3. `berapa tiket di antrian triage project LTS` — `count_intake_items`.
  4. `tampilkan item intake pending` lalu `detail intake item <id>` — suggestion tampil dengan confidence.
  5. Project dengan `intake_view=false` → pesan "feature is disabled", bukan hasil kosong.
