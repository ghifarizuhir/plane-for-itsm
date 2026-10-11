# Document Export (DOCX) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Export work items and KB articles (Pages/Wiki) as formal DOCX documents with a workspace-configured header/footer, properties, and a milestone history appendix.

**Architecture:** Client-side generation in `apps/web` using the `docx` npm library; backend work is a workspace-scoped settings table `workspace_document_settings` with GET/PATCH routes in Rust api-rs plus `DOC_HEADER_LOGO` file-asset binding for custom logo uploads. Builders share a shell (header/footer/placeholders) and an HTML→DOCX converter.

**Tech Stack:** Rust (axum + sqlx), Django (parity model/migration only), React/React Router + MobX + SWR + TipTap (`apps/web`), `docx` v9, Vitest + happy-dom, `@plane/i18n`.

**Spec:** `docs/superpowers/specs/2026-10-12-document-export-docx-design.md`

---

## File Structure

Backend:

- Create `apps/api-rs/migrations/0019_workspace_document_settings.sql`
- Create `apps/api/plane/db/models/document_export.py`; modify `apps/api/plane/db/models/__init__.py`
- Create `apps/api/plane/db/migrations/0129_workspace_document_settings.py`
- Modify `apps/api-rs/crates/api/src/routes/asset.rs` (DOC_HEADER_LOGO)
- Create `apps/api-rs/crates/api/src/routes/document_export.rs`; modify `routes/mod.rs`, `main.rs`
- Create `apps/api-rs/crates/api/tests/document_export_settings_test.rs`

Web:

- Modify `pnpm-workspace.yaml` (catalog `docx`, `happy-dom`), `apps/web/package.json`
- Create `packages/types/src/document-export/index.ts`; modify `packages/types/src/{index,enums,settings}.ts`
- Create `apps/web/core/services/document-export/document-export-settings.service.ts`; modify `packages/constants/src/fetch-keys.ts`
- Modify `packages/constants/src/settings/workspace.ts`, `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`
- Create `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/document-export/{page,header}.tsx`
- Create `apps/web/core/components/settings/document-export/form.tsx`
- Create `apps/web/core/components/document-export/`: `download.ts`, `resolve-placeholders.ts(+test)`, `milestones.ts(+test)`, `html-to-docx.ts(+test)`, `shell.ts(+test)`, `logo.ts`, `build-work-item-doc.ts(+test)`, `build-page-doc.ts(+test)`, `work-item-export-modal.tsx`
- Modify `apps/web/core/components/issues/issue-layouts/quick-action-dropdowns/{helper.tsx,issue-detail.tsx}`
- Modify `apps/web/core/components/pages/modals/export-page-modal.tsx`
- i18n: create `packages/i18n/src/locales/en/document-export.json`; modify `en/workspace-settings.json` + all locales

---

### Task 1: Database migration + Django parity model

**Files:**

- Create: `apps/api-rs/migrations/0019_workspace_document_settings.sql`
- Create: `apps/api/plane/db/models/document_export.py`
- Modify: `apps/api/plane/db/models/__init__.py`
- Create: `apps/api/plane/db/migrations/0129_workspace_document_settings.py`

- [ ] **Step 1: Write the sqlx migration**

Create `apps/api-rs/migrations/0019_workspace_document_settings.sql`:

```sql
-- Workspace-scoped DOCX export settings: letterhead header/footer config.
CREATE TABLE IF NOT EXISTS public.workspace_document_settings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL,
    header_logo_mode varchar(16) NOT NULL DEFAULT 'none',
    header_logo_asset_id uuid,
    header_text_lines jsonb NOT NULL DEFAULT '[]'::jsonb,
    footer_text_lines jsonb NOT NULL DEFAULT '[]'::jsonb,
    show_page_numbers boolean NOT NULL DEFAULT true,
    show_generated_by boolean NOT NULL DEFAULT true,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS workspace_document_settings_ws_uniq
    ON public.workspace_document_settings (workspace_id) WHERE deleted_at IS NULL;
```

- [ ] **Step 2: Write the Django parity model**

Create `apps/api/plane/db/models/document_export.py`:

```python
# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

# Django imports
from django.db import models

# Module imports
from .base import BaseModel


class WorkspaceDocumentSettings(BaseModel):
    workspace = models.ForeignKey("db.Workspace", related_name="document_settings", on_delete=models.CASCADE)
    header_logo_mode = models.CharField(max_length=16, default="none")
    header_logo_asset = models.ForeignKey(
        "db.FileAsset", related_name="+", on_delete=models.SET_NULL, null=True, blank=True
    )
    header_text_lines = models.JSONField(default=list)
    footer_text_lines = models.JSONField(default=list)
    show_page_numbers = models.BooleanField(default=True)
    show_generated_by = models.BooleanField(default=True)

    class Meta:
        verbose_name = "Workspace Document Settings"
        verbose_name_plural = "Workspace Document Settings"
        db_table = "workspace_document_settings"

    def __str__(self):
        return f"{self.workspace} - document settings"
```

- [ ] **Step 3: Register the model**

In `apps/api/plane/db/models/__init__.py` add `from .document_export import WorkspaceDocumentSettings` following the existing import style.

- [ ] **Step 4: Write the Django migration**

Create `apps/api/plane/db/migrations/0129_workspace_document_settings.py`, modeled on `0128_issue_type_requires_service.py` and a `CreateModel` from any existing migration (copy the `BaseModel` field definitions verbatim — `created_at`, `updated_at`, `deleted_at`, `created_by`, `updated_by` — from that file so field options match):

```python
# Generated by Django 5.2.15 on 2026-10-12

import uuid
from django.conf import settings
from django.db import migrations, models
import django.db.models.deletion


class Migration(migrations.Migration):
    dependencies = [
        ("db", "0128_issue_type_requires_service"),
    ]

    operations = [
        migrations.CreateModel(
            name="WorkspaceDocumentSettings",
            fields=[
                ("created_at", models.DateTimeField(auto_now_add=True, verbose_name="Created At")),
                ("updated_at", models.DateTimeField(auto_now=True, verbose_name="Last Modified At")),
                ("deleted_at", models.DateTimeField(blank=True, null=True, verbose_name="Deleted At")),
                (
                    "id",
                    models.UUIDField(
                        db_index=True, default=uuid.uuid4, editable=False, primary_key=True, serialize=False, unique=True
                    ),
                ),
                ("header_logo_mode", models.CharField(default="none", max_length=16)),
                ("header_text_lines", models.JSONField(default=list)),
                ("footer_text_lines", models.JSONField(default=list)),
                ("show_page_numbers", models.BooleanField(default=True)),
                ("show_generated_by", models.BooleanField(default=True)),
                (
                    "created_by",
                    models.ForeignKey(
                        blank=True,
                        null=True,
                        on_delete=django.db.models.deletion.SET_NULL,
                        related_name="workspacedocumentsettings_created_by",
                        to=settings.AUTH_USER_MODEL,
                        verbose_name="Created By",
                    ),
                ),
                (
                    "header_logo_asset",
                    models.ForeignKey(
                        blank=True,
                        null=True,
                        on_delete=django.db.models.deletion.SET_NULL,
                        related_name="+",
                        to="db.fileasset",
                    ),
                ),
                (
                    "updated_by",
                    models.ForeignKey(
                        blank=True,
                        null=True,
                        on_delete=django.db.models.deletion.SET_NULL,
                        related_name="workspacedocumentsettings_updated_by",
                        to=settings.AUTH_USER_MODEL,
                        verbose_name="Updated By",
                    ),
                ),
                (
                    "workspace",
                    models.ForeignKey(
                        on_delete=django.db.models.deletion.CASCADE,
                        related_name="document_settings",
                        to="db.workspace",
                    ),
                ),
            ],
            options={
                "verbose_name": "Workspace Document Settings",
                "verbose_name_plural": "Workspace Document Settings",
                "db_table": "workspace_document_settings",
            },
        ),
    ]
```

If `FileAsset`'s app label differs, adjust `to="db.fileasset"` accordingly (mirror how `workspace.py` references `logo_asset`).

- [ ] **Step 5: Run the migration version guard**

Run: `cd apps/api-rs && cargo test -p common --test migration_versions_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/migrations/0019_workspace_document_settings.sql apps/api/plane/db/models/document_export.py apps/api/plane/db/models/__init__.py apps/api/plane/db/migrations/0129_workspace_document_settings.py
git commit -m "feat(api): workspace document settings schema (sqlx + django parity)"
```

---

### Task 2: Rust settings endpoints (GET/PATCH) + tests

**Files:**

- Create: `apps/api-rs/crates/api/tests/document_export_settings_test.rs`
- Create: `apps/api-rs/crates/api/src/routes/document_export.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (near the work-item-types routes, ~line 681)

- [ ] **Step 1: Write the failing test file**

Create `apps/api-rs/crates/api/tests/document_export_settings_test.rs`:

```rust
//! DB-backed workspace document-export settings tests. Run with `--test-threads=1`.

use api::middleware::auth::AuthUser;
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
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

async fn state(pool: &PgPool) -> AppState {
    AppState {
        pool: pool.clone(),
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

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("docx-{}", Uuid::new_v4().simple());
        let admin_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, admin_id, &format!("{slug}-admin")).await;
        insert_user(pool, member_id, &format!("{slug}-member")).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Docx', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        for (user_id, role) in [(admin_id, 20), (member_id, 15)] {
            sqlx::query(
                "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
                 workspace_id, view_props, default_props, issue_props, explored_features, \
                 getting_started_checklist, tips, is_active) \
                 VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', \
                 '{}', '{}', true)",
            )
            .bind(role)
            .bind(user_id)
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("scratch workspace member");
        }
        Self { slug, workspace_id, admin_id, member_id }
    }

    async fn purge(&self, pool: &PgPool) {
        for statement in [
            "DELETE FROM workspace_document_settings WHERE workspace_id = $1",
            "DELETE FROM file_assets WHERE workspace_id = $1",
            "DELETE FROM workspace_members WHERE workspace_id = $1",
        ] {
            sqlx::query(statement).bind(self.workspace_id).execute(pool).await.ok();
        }
        sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(self.workspace_id).execute(pool).await.ok();
        sqlx::query("DELETE FROM users WHERE username LIKE $1")
            .bind(format!("{}%", self.slug))
            .execute(pool)
            .await
            .ok();
    }
}

#[tokio::test]
async fn get_returns_defaults_when_no_row() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(body)) = api::routes::document_export::get_settings(
        State(st.clone()),
        AuthUser(scratch.member_id),
        Path(scratch.slug.clone()),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], serde_json::Value::Null);
    assert_eq!(body["header_logo_mode"], "none");
    assert_eq!(body["show_page_numbers"], true);
    assert_eq!(body["show_generated_by"], true);
    assert_eq!(body["header_text_lines"], serde_json::json!([]));
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_is_admin_only() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.member_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            header_logo_mode: Some("workspace".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::FORBIDDEN);
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_upserts_and_validates() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(body)) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            header_logo_mode: Some("workspace".into()),
            header_text_lines: Some(serde_json::json!([
                {"text": "PT Example — IT Services", "align": "center"}
            ])),
            show_generated_by: Some(false),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["header_logo_mode"], "workspace");
    assert_eq!(body["show_generated_by"], false);
    assert_eq!(body["header_text_lines"][0]["align"], "center");

    let (status, Json(body)) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            footer_text_lines: Some(serde_json::json!([{"text": "Confidential"}])),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["footer_text_lines"][0]["text"], "Confidential");
    assert_eq!(body["header_logo_mode"], "workspace");

    let (status, _) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            footer_text_lines: Some(serde_json::json!([{"text": "x", "align": "diagonal"}])),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_document_settings WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test document_export_settings_test -- --test-threads=1`
Expected: FAIL to compile — `api::routes::document_export` does not exist.

- [ ] **Step 3: Write the handlers**

Create `apps/api-rs/crates/api/src/routes/document_export.rs`:

```rust
//! Workspace document-export settings (`workspace_document_settings`).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::routes::{
    project::{deny, missing, ws_role},
    v1::work_item_type::ws_id,
};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

const LOGO_MODES: [&str; 3] = ["none", "workspace", "custom"];
const ALIGNS: [&str; 3] = ["left", "center", "right"];
const MAX_TEXT_LINES: usize = 5;
const MAX_LINE_LEN: usize = 200;

const SETTINGS_COLS: &str = "id, header_logo_mode, header_logo_asset_id, header_text_lines, \
    footer_text_lines, show_page_numbers, show_generated_by, created_at, updated_at, \
    created_by_id, updated_by_id";

#[derive(Debug, Clone, FromRow)]
pub struct DocumentExportSettingsRow {
    pub id: Uuid,
    pub header_logo_mode: String,
    pub header_logo_asset_id: Option<Uuid>,
    pub header_text_lines: Value,
    pub footer_text_lines: Value,
    pub show_page_numbers: bool,
    pub show_generated_by: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateDocumentExportSettings {
    #[serde(default)]
    pub header_logo_mode: Option<String>,
    #[serde(default)]
    pub header_logo_asset_id: Option<Uuid>,
    #[serde(default)]
    pub header_text_lines: Option<Value>,
    #[serde(default)]
    pub footer_text_lines: Option<Value>,
    #[serde(default)]
    pub show_page_numbers: Option<bool>,
    #[serde(default)]
    pub show_generated_by: Option<bool>,
}

fn settings_json(row: Option<&DocumentExportSettingsRow>) -> Value {
    match row {
        Some(r) => json!({
            "id": r.id,
            "header_logo_mode": r.header_logo_mode,
            "header_logo_asset_id": r.header_logo_asset_id,
            "header_logo_url": r
                .header_logo_asset_id
                .map(|id| format!("/api/assets/v2/static/{id}/")),
            "header_text_lines": r.header_text_lines,
            "footer_text_lines": r.footer_text_lines,
            "show_page_numbers": r.show_page_numbers,
            "show_generated_by": r.show_generated_by,
            "created_at": r.created_at,
            "updated_at": r.updated_at,
        }),
        None => json!({
            "id": null,
            "header_logo_mode": "none",
            "header_logo_asset_id": null,
            "header_logo_url": null,
            "header_text_lines": [],
            "footer_text_lines": [],
            "show_page_numbers": true,
            "show_generated_by": true,
            "created_at": null,
            "updated_at": null,
        }),
    }
}

fn validate_text_lines(value: &Value, field: &str) -> Result<(), String> {
    let Some(lines) = value.as_array() else {
        return Err(format!("{field} must be an array"));
    };
    if lines.len() > MAX_TEXT_LINES {
        return Err(format!("{field} supports at most {MAX_TEXT_LINES} lines"));
    }
    for line in lines {
        let Some(text) = line.get("text").and_then(Value::as_str) else {
            return Err(format!("{field} line text is required"));
        };
        if text.chars().count() > MAX_LINE_LEN {
            return Err(format!("{field} line text must be at most {MAX_LINE_LEN} characters"));
        }
        if let Some(align) = line.get("align") {
            match align.as_str() {
                Some(a) if ALIGNS.contains(&a) => {}
                _ => return Err(format!("{field} line align must be left, center, or right")),
            }
        }
    }
    Ok(())
}

async fn fetch_settings(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
) -> Result<Option<DocumentExportSettingsRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SETTINGS_COLS} FROM workspace_document_settings \
         WHERE workspace_id = $1 AND deleted_at IS NULL"
    ))
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_settings(State(st): State<AppState>, auth: AuthUser, Path(slug): Path<String>) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny());
    }
    let Some(ws) = ws_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let row = fetch_settings(&st.pool, ws).await?;
    Ok((StatusCode::OK, Json(settings_json(row.as_ref()))))
}

pub async fn update_settings(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<UpdateDocumentExportSettings>,
) -> R {
    let Some(role) = ws_role(&st.pool, auth.0, &slug).await? else {
        return Ok(deny());
    };
    if role < 20 {
        return Ok(deny());
    }
    let Some(ws) = ws_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };

    if let Some(mode) = &body.header_logo_mode {
        if !LOGO_MODES.contains(&mode.as_str()) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "header_logo_mode must be none, workspace, or custom"})),
            ));
        }
    }
    if let Some(lines) = &body.header_text_lines {
        if let Err(message) = validate_text_lines(lines, "header_text_lines") {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    if let Some(lines) = &body.footer_text_lines {
        if let Err(message) = validate_text_lines(lines, "footer_text_lines") {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    if let Some(asset_id) = body.header_logo_asset_id {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM file_assets WHERE id = $1 AND workspace_id = $2 \
             AND entity_type = 'DOC_HEADER_LOGO' AND deleted_at IS NULL)",
        )
        .bind(asset_id)
        .bind(ws)
        .fetch_one(&st.pool)
        .await?;
        if !owned {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid header logo asset"})),
            ));
        }
    }

    let current = fetch_settings(&st.pool, ws).await?;
    match current {
        Some(row) => {
            sqlx::query(
                "UPDATE workspace_document_settings SET \
                 header_logo_mode = COALESCE($1, header_logo_mode), \
                 header_logo_asset_id = COALESCE($2, header_logo_asset_id), \
                 header_text_lines = COALESCE($3, header_text_lines), \
                 footer_text_lines = COALESCE($4, footer_text_lines), \
                 show_page_numbers = COALESCE($5, show_page_numbers), \
                 show_generated_by = COALESCE($6, show_generated_by), \
                 updated_by_id = $7, updated_at = now() WHERE id = $8",
            )
            .bind(&body.header_logo_mode)
            .bind(body.header_logo_asset_id)
            .bind(&body.header_text_lines)
            .bind(&body.footer_text_lines)
            .bind(body.show_page_numbers)
            .bind(body.show_generated_by)
            .bind(auth.0)
            .bind(row.id)
            .execute(&st.pool)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO workspace_document_settings (id, workspace_id, header_logo_mode, \
                 header_logo_asset_id, header_text_lines, footer_text_lines, show_page_numbers, \
                 show_generated_by, created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, COALESCE($2, 'none'), $3, COALESCE($4, '[]'::jsonb), \
                 COALESCE($5, '[]'::jsonb), COALESCE($6, true), COALESCE($7, true), $8, $8, now(), now())",
            )
            .bind(ws)
            .bind(&body.header_logo_mode)
            .bind(body.header_logo_asset_id)
            .bind(&body.header_text_lines)
            .bind(&body.footer_text_lines)
            .bind(body.show_page_numbers)
            .bind(body.show_generated_by)
            .bind(auth.0)
            .execute(&st.pool)
            .await?;
        }
    }

    let row = fetch_settings(&st.pool, ws).await?;
    Ok((StatusCode::OK, Json(settings_json(row.as_ref()))))
}
```

- [ ] **Step 4: Register module + route**

`routes/mod.rs`: add `pub mod document_export;` (alphabetical, after `pub mod draft;`).

`main.rs`, right after the work-item-types routes block (~line 681):

```rust
        // Workspace document export (DOCX) settings.
        .route(
            "/api/workspaces/:slug/document-export-settings/",
            get(routes::document_export::get_settings).patch(routes::document_export::update_settings),
        )
```

- [ ] **Step 5: Run tests**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test document_export_settings_test -- --test-threads=1`
Expected: 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/document_export.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/document_export_settings_test.rs
git commit -m "feat(api-rs): workspace document export settings endpoints"
```

---

### Task 3: Rust DOC_HEADER_LOGO asset binding + test

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/asset.rs` (ENTITY_TYPES ~L35, asset_url_for ~L700, ws_presign gate ~L876, mark_uploaded ~L1325)
- Test: `apps/api-rs/crates/api/tests/document_export_settings_test.rs`

- [ ] **Step 1: Write the failing test**

Append to `document_export_settings_test.rs`:

```rust
#[tokio::test]
async fn mark_uploaded_binds_header_logo_to_settings() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let old_asset: Uuid = sqlx::query_scalar(
        "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, \
         entity_type, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
         VALUES (gen_random_uuid(), '{}', 'key-old', 10, $1, $2, 'DOC_HEADER_LOGO', true, false, false, \
         '{}', now(), now()) RETURNING id",
    )
    .bind(scratch.workspace_id)
    .bind(scratch.admin_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_document_settings (id, workspace_id, header_logo_mode, \
         header_logo_asset_id, header_text_lines, footer_text_lines, show_page_numbers, \
         show_generated_by, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, 'custom', $2, '[]'::jsonb, '[]'::jsonb, true, true, now(), now())",
    )
    .bind(scratch.workspace_id)
    .bind(old_asset)
    .execute(&pool)
    .await
    .unwrap();

    let new_asset: Uuid = sqlx::query_scalar(
        "INSERT INTO file_assets (id, attributes, asset, size, workspace_id, created_by_id, \
         entity_type, is_uploaded, is_deleted, is_archived, storage_metadata, created_at, updated_at) \
         VALUES (gen_random_uuid(), '{}', 'key-new', 10, $1, $2, 'DOC_HEADER_LOGO', false, false, false, \
         '{}', now(), now()) RETURNING id",
    )
    .bind(scratch.workspace_id)
    .bind(scratch.admin_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, _) = api::routes::asset::mark_uploaded(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path((scratch.slug.clone(), new_asset)),
        Json(api::routes::asset::PatchAsset { attributes: None }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (asset_id, mode): (Option<Uuid>, String) = sqlx::query_as(
        "SELECT header_logo_asset_id, header_logo_mode FROM workspace_document_settings \
         WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(asset_id, Some(new_asset));
    assert_eq!(mode, "custom");

    let old_deleted: bool = sqlx::query_scalar("SELECT is_deleted FROM file_assets WHERE id = $1")
        .bind(old_asset)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(old_deleted);
    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test document_export_settings_test mark_uploaded_binds_header_logo_to_settings -- --test-threads=1`
Expected: FAIL — `mark_uploaded` does not update the settings row.

- [ ] **Step 3: Implement asset support in `asset.rs`**

1. `ENTITY_TYPES` becomes `[&str; 11]` with `"DOC_HEADER_LOGO"` appended.
2. `asset_url_for`: add `"DOC_HEADER_LOGO"` to the static-URL match arm:
   `Some("WORKSPACE_LOGO" | "USER_AVATAR" | "USER_COVER" | "PROJECT_COVER" | "DOC_HEADER_LOGO")`.
3. `ws_presign` admin gate (~L876) becomes:
   `if matches!(entity_type, "WORKSPACE_LOGO" | "DOC_HEADER_LOGO") && !gate_ws_roles(&st.pool, auth.0, &slug, &[20]).await? { ... }`
4. `mark_uploaded`: after the `WORKSPACE_LOGO` arm add:

```rust
            Some("DOC_HEADER_LOGO") => {
                if let Some(wsid) = r.workspace_id {
                    let existing: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
                        "SELECT id, header_logo_asset_id FROM workspace_document_settings \
                         WHERE workspace_id = $1 AND deleted_at IS NULL",
                    )
                    .bind(wsid)
                    .fetch_optional(&st.pool)
                    .await?;
                    match existing {
                        Some((settings_id, prev)) => {
                            if let Some(p) = prev.filter(|p| *p != asset_id) {
                                sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
                                    .bind(p).execute(&st.pool).await?;
                            }
                            sqlx::query(
                                "UPDATE workspace_document_settings SET header_logo_mode = 'custom', \
                                 header_logo_asset_id = $1, updated_by_id = $2, updated_at = now() \
                                 WHERE id = $3",
                            )
                            .bind(asset_id)
                            .bind(auth.0)
                            .bind(settings_id)
                            .execute(&st.pool)
                            .await?;
                        }
                        None => {
                            sqlx::query(
                                "INSERT INTO workspace_document_settings (id, workspace_id, \
                                 header_logo_mode, header_logo_asset_id, header_text_lines, \
                                 footer_text_lines, show_page_numbers, show_generated_by, \
                                 created_by_id, updated_by_id, created_at, updated_at) \
                                 VALUES (gen_random_uuid(), $1, 'custom', $2, '[]'::jsonb, '[]'::jsonb, \
                                 true, true, $3, $3, now(), now())",
                            )
                            .bind(wsid)
                            .bind(asset_id)
                            .bind(auth.0)
                            .execute(&st.pool)
                            .await?;
                        }
                    }
                }
            }
```

No change to `entity_fk_fragment` needed — unmatched types use the no-FK branch and `workspace_id` is always set.

- [ ] **Step 4: Run tests**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test document_export_settings_test -- --test-threads=1`
Expected: 4 tests PASS.

Regression: `cargo test -p api --test asset_test -- --test-threads=1` → PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/asset.rs apps/api-rs/crates/api/tests/document_export_settings_test.rs
git commit -m "feat(api-rs): DOC_HEADER_LOGO asset binding for document export"
```

### Task 4: Web deps, shared types, service, SWR key

**Files:**

- Modify: `pnpm-workspace.yaml`, `apps/web/package.json`
- Create: `packages/types/src/document-export/index.ts`
- Modify: `packages/types/src/{index,enums,settings}.ts`
- Create: `apps/web/core/services/document-export/document-export-settings.service.ts`
- Modify: `packages/constants/src/fetch-keys.ts`

- [ ] **Step 1: Add dependencies**

In `pnpm-workspace.yaml` `catalog:` block (alphabetical) add:

```yaml
docx: "9.7.1"
happy-dom: "20.11.6"
```

In `apps/web/package.json`: `dependencies` add `"docx": "catalog:"`; `devDependencies` add `"happy-dom": "catalog:"`.

Run: `pnpm install` → lockfile updates, no peer errors.

- [ ] **Step 2: Create shared types**

Create `packages/types/src/document-export/index.ts`:

```ts
export type TDocumentExportTextAlign = "left" | "center" | "right";

export type TDocumentExportTextLine = {
  text: string;
  align: TDocumentExportTextAlign;
};

export type TDocumentExportLogoMode = "none" | "workspace" | "custom";

export type TDocumentExportSettings = {
  id: string | null;
  header_logo_mode: TDocumentExportLogoMode;
  header_logo_asset_id: string | null;
  header_logo_url: string | null;
  header_text_lines: TDocumentExportTextLine[];
  footer_text_lines: TDocumentExportTextLine[];
  show_page_numbers: boolean;
  show_generated_by: boolean;
  created_at: string | null;
  updated_at: string | null;
};

export type TDocumentExportSettingsPayload = Partial<
  Pick<
    TDocumentExportSettings,
    | "header_logo_mode"
    | "header_logo_asset_id"
    | "header_text_lines"
    | "footer_text_lines"
    | "show_page_numbers"
    | "show_generated_by"
  >
>;
```

Add the SPDX header comment block used by sibling files. In `packages/types/src/index.ts` add `export * from "./document-export";` between `./description_version` and `./editor`.

- [ ] **Step 3: Extend enums and settings tabs**

`packages/types/src/enums.ts` → `EFileAssetType` add `DOC_HEADER_LOGO = "DOC_HEADER_LOGO",`.

`packages/types/src/settings.ts` line 13 → `export type TWorkspaceSettingsTabs = "general" | "members" | "export" | "webhooks" | "work_item_types" | "document_export";`

- [ ] **Step 4: Create the service**

Create `apps/web/core/services/document-export/document-export-settings.service.ts`:

```ts
import { API_BASE_URL } from "@plane/constants";
import type { TDocumentExportSettings, TDocumentExportSettingsPayload } from "@plane/types";
import { APIService } from "@/services/api.service";

export class DocumentExportSettingsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async fetchSettings(workspaceSlug: string): Promise<TDocumentExportSettings> {
    return this.get(`/api/workspaces/${workspaceSlug}/document-export-settings/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateSettings(workspaceSlug: string, data: TDocumentExportSettingsPayload): Promise<TDocumentExportSettings> {
    return this.patch(`/api/workspaces/${workspaceSlug}/document-export-settings/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
```

Add the same SPDX header block as other services.

- [ ] **Step 5: Add the SWR key**

In `packages/constants/src/fetch-keys.ts` after `EXPORT_SERVICES_LIST` (~line 129):

```ts
// document export
export const DOCUMENT_EXPORT_SETTINGS = (workspaceSlug: string) =>
  `DOCUMENT_EXPORT_SETTINGS_${workspaceSlug.toUpperCase()}`;
```

- [ ] **Step 6: Typecheck**

Run: `pnpm --filter=web check:types` → PASS.

- [ ] **Step 7: Commit**

```bash
git add pnpm-workspace.yaml pnpm-lock.yaml apps/web/package.json packages/types/src/document-export/index.ts packages/types/src/index.ts packages/types/src/enums.ts packages/types/src/settings.ts apps/web/core/services/document-export/document-export-settings.service.ts packages/constants/src/fetch-keys.ts
git commit -m "feat(web,types): document export types, service, and docx dependency"
```

---

### Task 5: i18n keys (en + all locales)

**Files:**

- Create: `packages/i18n/src/locales/en/document-export.json`
- Modify: `packages/i18n/src/locales/en/workspace-settings.json`
- Modify: all other locale dirs under `packages/i18n/src/locales/*/`

**IMPORTANT:** Load and follow the `translate` skill before touching `packages/i18n/src/locales/**`. `packages/i18n/scripts/sync-check.ts` fails when any locale misses an `en` key — every locale must be updated.

- [ ] **Step 1: Create the en namespace file**

Create `packages/i18n/src/locales/en/document-export.json`:

```json
{
  "document_export": {
    "export_as_document": "Export as document",
    "modal": {
      "title": "Export as document",
      "description": "The work item will be downloaded as a DOCX file using the workspace document settings.",
      "include_history": "Include history appendix",
      "export": "Export",
      "exporting": "Exporting",
      "success": "Document exported successfully.",
      "error": "Document could not be exported. Please try again."
    },
    "doc": {
      "properties": "Properties",
      "description": "Description",
      "linked_work_items": "Linked work items",
      "history": "History (milestones)",
      "sub_pages": "Sub-pages",
      "versions": "Versions",
      "generated_by": "Generated by {user} on {date}",
      "no_value": "—",
      "relation_blocking": "Blocks",
      "relation_blocked_by": "Blocked by",
      "relation_relates_to": "Relates to",
      "relation_duplicate": "Duplicate of",
      "event_created": "Created",
      "event_state": "State",
      "event_priority": "Priority",
      "event_assignees": "Assignee",
      "event_target_date": "Target date",
      "event_start_date": "Start date",
      "event_archived_at": "Archived",
      "column_when": "When",
      "column_actor": "Actor",
      "column_event": "Event",
      "column_change": "Change"
    }
  }
}
```

- [ ] **Step 2: Add settings-page keys to en**

In `packages/i18n/src/locales/en/workspace-settings.json`, inside `workspace_settings.settings`, add:

```json
      "document_export": {
        "title": "Document export",
        "heading": "Document export",
        "description": "Configure the letterhead used when work items and wiki articles are exported as DOCX documents.",
        "logo": "Header logo",
        "logo_mode_none": "No logo",
        "logo_mode_workspace": "Use workspace logo",
        "logo_mode_custom": "Custom logo",
        "upload_logo": "Upload logo",
        "header_lines": "Header text lines",
        "footer_lines": "Footer text lines",
        "line_placeholder": "Text or placeholders",
        "add_line": "Add line",
        "remove_line": "Remove",
        "align_left": "Left",
        "align_center": "Center",
        "align_right": "Right",
        "show_page_numbers": "Show page numbers",
        "show_generated_by": "Show generated-by line",
        "placeholders": "Available placeholders: {workspace_name}, {document_title}, {document_id}, {export_date}, {exported_by}",
        "limit_hint": "Up to 5 lines, 200 characters each.",
        "save": "Save changes",
        "saved": "Document export settings saved.",
        "error": "Document export settings could not be saved. Please try again."
      }
```

- [ ] **Step 3: Translate to all locales**

With the `translate` skill, add both blocks to every locale dir (`cs`, `de`, `es`, `fr`, `id`, `it`, `ja`, `ka-ge`, `ko`, `pl`, `pt-BR`, `ro`, `ru`, `sk`, `tr-TR`, `ua`, `vi-VN`, `zh-CN`, `zh-TW`).

- [ ] **Step 4: Verify sync**

Run: `pnpm --filter=@plane/i18n check:sync` → exits 0, no missing-key lines.

- [ ] **Step 5: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "feat(i18n): document export keys for all locales"
```

---

### Task 6: Workspace settings page for document export

**Files:**

- Modify: `packages/constants/src/settings/workspace.ts`
- Modify: `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/document-export/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/document-export/header.tsx`
- Create: `apps/web/core/components/settings/document-export/form.tsx`

- [ ] **Step 1: Register the settings item**

In `packages/constants/src/settings/workspace.ts`, add to `WORKSPACE_SETTINGS`:

```ts
  document_export: {
    key: "document_export",
    i18n_label: "workspace_settings.settings.document_export.title",
    href: `/settings/document-export`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
    highlight: (pathname: string, baseUrl: string) => pathname === `${baseUrl}/settings/document-export/`,
  },
```

Add `WORKSPACE_SETTINGS["document_export"]` to `GROUPED_WORKSPACE_SETTINGS[WORKSPACE_SETTINGS_CATEGORY.ADMINISTRATION]` after the existing `export` item.

In `item-icon.tsx`, add `document_export: <Icon>,` to `WORKSPACE_SETTINGS_ICONS` (use `PageOutline` or `FileText` — verify it exists in `@makeplane/propel/icons`/`lucide-react`; `ExportOutline` is already imported there and acceptable).

- [ ] **Step 2: Create the header**

Create `.../document-export/header.tsx` — copy `work-item-types/header.tsx` verbatim, rename the component to `DocumentExportWorkspaceSettingsHeader`, use `WORKSPACE_SETTINGS.document_export` and the same icon.

- [ ] **Step 3: Create the page**

Create `.../document-export/page.tsx`:

```tsx
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { DocumentExportSettingsForm } from "@/components/settings/document-export/form";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import type { Route } from "./+types/page";
import { DocumentExportWorkspaceSettingsHeader } from "./header";

function DocumentExportSettingsPage({ params }: Route.ComponentProps) {
  const { workspaceSlug } = params;
  const { t } = useTranslation();
  const { currentWorkspace } = useWorkspace();
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.document_export.title")}`
    : undefined;

  return (
    <SettingsContentWrapper header={<DocumentExportWorkspaceSettingsHeader />}>
      <PageHead title={pageTitle} />
      {workspaceSlug && <DocumentExportSettingsForm workspaceSlug={workspaceSlug.toString()} />}
    </SettingsContentWrapper>
  );
}

export default observer(DocumentExportSettingsPage);
```

Add the SPDX header block used by sibling files. If `./+types/page` does not resolve (React Router codegen), run `pnpm --filter=web dev` once or copy the exact typing pattern from `work-item-types/page.tsx`.

- [ ] **Step 4: Create the form component**

Create `apps/web/core/components/settings/document-export/form.tsx`:

```tsx
import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import useSWR from "swr";
// plane imports
import { DOCUMENT_EXPORT_SETTINGS, EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TDocumentExportSettings, TDocumentExportTextAlign, TDocumentExportTextLine } from "@plane/types";
import { EFileAssetType } from "@plane/types";
import { cn } from "@plane/utils";
// services
import { DocumentExportSettingsService } from "@/services/document-export/document-export-settings.service";
import { FileService } from "@/services/file.service";
// hooks
import { useUserPermissions } from "@/hooks/store/user";

const MAX_LINES = 5;
const MAX_LINE_LEN = 200;

type Props = { workspaceSlug: string };

export const DocumentExportSettingsForm = observer(function DocumentExportSettingsForm({ workspaceSlug }: Props) {
  const { t } = useTranslation();
  const { allowPermissions } = useUserPermissions();
  const settingsService = useMemo(() => new DocumentExportSettingsService(), []);
  const fileService = useMemo(() => new FileService(), []);
  const { data, isLoading, mutate } = useSWR(DOCUMENT_EXPORT_SETTINGS(workspaceSlug), () =>
    settingsService.fetchSettings(workspaceSlug)
  );
  const [draft, setDraft] = useState<TDocumentExportSettings | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const isAdmin = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);

  useEffect(() => {
    if (data) setDraft(data);
  }, [data]);

  if (isLoading || !draft) return <div className="text-13 text-tertiary">Loading…</div>;

  const updateLines = (
    key: "header_text_lines" | "footer_text_lines",
    updater: (lines: TDocumentExportTextLine[]) => TDocumentExportTextLine[]
  ) => setDraft((current) => (current ? { ...current, [key]: updater(current[key]) } : current));

  const showError = () =>
    setToast({ type: TOAST_TYPE.ERROR, title: t("workspace_settings.settings.document_export.error") });

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await settingsService.updateSettings(workspaceSlug, {
        header_logo_mode: draft.header_logo_mode,
        header_text_lines: draft.header_text_lines,
        footer_text_lines: draft.footer_text_lines,
        show_page_numbers: draft.show_page_numbers,
        show_generated_by: draft.show_generated_by,
      });
      await mutate();
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("workspace_settings.settings.document_export.saved") });
    } catch {
      showError();
    } finally {
      setIsSaving(false);
    }
  };

  const handleLogoUpload = async (file: File) => {
    try {
      await fileService.uploadWorkspaceAsset(
        workspaceSlug,
        { entity_identifier: draft.id ?? workspaceSlug, entity_type: EFileAssetType.DOC_HEADER_LOGO },
        file
      );
      await mutate();
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("workspace_settings.settings.document_export.saved") });
    } catch {
      showError();
    }
  };

  const renderLines = (key: "header_text_lines" | "footer_text_lines", label: string) => (
    <div className="space-y-2">
      <h6 className="text-13 font-medium text-secondary">{label}</h6>
      <p className="text-11 text-tertiary">{t("workspace_settings.settings.document_export.limit_hint")}</p>
      {draft[key].map((line, index) => (
        <div key={`${key}-${index}`} className="flex items-center gap-2">
          <input
            className="w-full rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-13"
            disabled={!isAdmin}
            maxLength={MAX_LINE_LEN}
            placeholder={t("workspace_settings.settings.document_export.line_placeholder")}
            value={line.text}
            onChange={(event) =>
              updateLines(key, (lines) =>
                lines.map((item, itemIndex) => (itemIndex === index ? { ...item, text: event.target.value } : item))
              )
            }
          />
          <select
            className="rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-13"
            disabled={!isAdmin}
            value={line.align}
            onChange={(event) =>
              updateLines(key, (lines) =>
                lines.map((item, itemIndex) =>
                  itemIndex === index ? { ...item, align: event.target.value as TDocumentExportTextAlign } : item
                )
              )
            }
          >
            <option value="left">{t("workspace_settings.settings.document_export.align_left")}</option>
            <option value="center">{t("workspace_settings.settings.document_export.align_center")}</option>
            <option value="right">{t("workspace_settings.settings.document_export.align_right")}</option>
          </select>
          <button
            type="button"
            className="text-13 text-danger-primary disabled:opacity-50"
            disabled={!isAdmin}
            onClick={() => updateLines(key, (lines) => lines.filter((_, itemIndex) => itemIndex !== index))}
          >
            {t("workspace_settings.settings.document_export.remove_line")}
          </button>
        </div>
      ))}
      <Button
        variant="secondary"
        size="sm"
        disabled={!isAdmin || draft[key].length >= MAX_LINES}
        onClick={() => updateLines(key, (lines) => [...lines, { text: "", align: "left" }])}
      >
        {t("workspace_settings.settings.document_export.add_line")}
      </Button>
      <p className="text-11 text-tertiary">{t("workspace_settings.settings.document_export.placeholders")}</p>
    </div>
  );

  return (
    <div className="flex flex-col gap-8">
      <div className="space-y-2">
        <h4 className="text-16 font-medium text-primary">{t("workspace_settings.settings.document_export.heading")}</h4>
        <p className="text-13 text-tertiary">{t("workspace_settings.settings.document_export.description")}</p>
      </div>

      <div className="space-y-2">
        <h6 className="text-13 font-medium text-secondary">{t("workspace_settings.settings.document_export.logo")}</h6>
        <div className="flex flex-wrap items-center gap-3">
          {(["none", "workspace", "custom"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              disabled={!isAdmin}
              onClick={() => setDraft({ ...draft, header_logo_mode: mode })}
              className={cn("rounded-md border px-3 py-1.5 text-13", {
                "border-accent-strong text-accent-primary": draft.header_logo_mode === mode,
                "border-subtle text-secondary": draft.header_logo_mode !== mode,
              })}
            >
              {t(`workspace_settings.settings.document_export.logo_mode_${mode}`)}
            </button>
          ))}
          {draft.header_logo_mode === "custom" && (
            <label className="cursor-pointer text-13 text-accent-primary">
              {t("workspace_settings.settings.document_export.upload_logo")}
              <input
                accept="image/png,image/jpeg,image/webp,image/gif"
                className="hidden"
                disabled={!isAdmin}
                type="file"
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file) void handleLogoUpload(file);
                }}
              />
            </label>
          )}
        </div>
      </div>

      {renderLines("header_text_lines", t("workspace_settings.settings.document_export.header_lines"))}
      {renderLines("footer_text_lines", t("workspace_settings.settings.document_export.footer_lines"))}

      <div className="space-y-2">
        <label className="flex items-center gap-2 text-13 text-secondary">
          <input
            checked={draft.show_page_numbers}
            disabled={!isAdmin}
            type="checkbox"
            onChange={(event) => setDraft({ ...draft, show_page_numbers: event.target.checked })}
          />
          {t("workspace_settings.settings.document_export.show_page_numbers")}
        </label>
        <label className="flex items-center gap-2 text-13 text-secondary">
          <input
            checked={draft.show_generated_by}
            disabled={!isAdmin}
            type="checkbox"
            onChange={(event) => setDraft({ ...draft, show_generated_by: event.target.checked })}
          />
          {t("workspace_settings.settings.document_export.show_generated_by")}
        </label>
      </div>

      {isAdmin && (
        <div>
          <Button variant="primary" size="lg" loading={isSaving} onClick={handleSave}>
            {t("workspace_settings.settings.document_export.save")}
          </Button>
        </div>
      )}
    </div>
  );
});
```

Adapt to real props: verify `Button` accepts `size="sm"` (drop it if not) and uses `loading` (see `export-page-modal.tsx` — it uses `loading`). Add the SPDX header block.

- [ ] **Step 5: Typecheck and lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint` → PASS.

- [ ] **Step 6: Manual smoke (dev stack)**

Open `/{workspaceSlug}/settings/document-export/`: toggle/save/reload persists; custom logo upload returns 200 on PATCH and sets `header_logo_mode=custom`.

- [ ] **Step 7: Commit**

```bash
git add packages/constants/src/settings/workspace.ts apps/web/core/components/settings/workspace/sidebar/item-icon.tsx "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/document-export" apps/web/core/components/settings/document-export
git commit -m "feat(web): workspace document export settings page"
```

### Task 7: Pure helpers — placeholders + milestones (TDD)

**Files:**

- Create: `apps/web/core/components/document-export/resolve-placeholders.ts` + `.test.ts`
- Create: `apps/web/core/components/document-export/milestones.ts` + `.test.ts`

- [ ] **Step 1: Write the failing placeholder test**

Create `resolve-placeholders.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { resolvePlaceholders } from "./resolve-placeholders";

const values = {
  workspace_name: "Acme",
  document_title: "Server down",
  document_id: "WORK-1",
  export_date: "2026-10-12",
  exported_by: "Budi",
};

describe("resolvePlaceholders", () => {
  it("replaces known placeholders", () => {
    expect(resolvePlaceholders("PT {workspace_name} — {document_id}", values)).toBe("PT Acme — WORK-1");
  });

  it("leaves unknown placeholders literal", () => {
    expect(resolvePlaceholders("{org_name} report", values)).toBe("{org_name} report");
  });

  it("handles text without placeholders", () => {
    expect(resolvePlaceholders("Confidential", values)).toBe("Confidential");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm --filter=web test -- core/components/document-export/resolve-placeholders.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement placeholders**

Create `resolve-placeholders.ts`:

```ts
export const DOCUMENT_PLACEHOLDER_KEYS = [
  "workspace_name",
  "document_title",
  "document_id",
  "export_date",
  "exported_by",
] as const;

export type TDocumentPlaceholderKey = (typeof DOCUMENT_PLACEHOLDER_KEYS)[number];
export type TDocumentPlaceholderValues = Record<TDocumentPlaceholderKey, string>;

export function resolvePlaceholders(text: string, values: TDocumentPlaceholderValues): string {
  return text.replace(/\{([a-z_]+)\}/g, (match, key: string) =>
    (DOCUMENT_PLACEHOLDER_KEYS as readonly string[]).includes(key) ? values[key as TDocumentPlaceholderKey] : match
  );
}
```

Add the SPDX header block (same as other web files).

- [ ] **Step 4: Write the failing milestones test**

Create `milestones.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { TIssueActivity } from "@plane/types";
import { milestoneRows, type TMilestoneLabels } from "./milestones";

const labels: TMilestoneLabels = {
  created: "Created",
  state: "State",
  priority: "Priority",
  assignees: "Assignee",
  target_date: "Target date",
  start_date: "Start date",
  archived_at: "Archived",
  noValue: "—",
};

const activity = (overrides: Partial<TIssueActivity>): TIssueActivity =>
  ({
    id: "a1",
    verb: "updated",
    field: "state",
    old_value: "state-1",
    new_value: "state-2",
    created_at: "2026-10-12T10:00:00Z",
    actor_detail: { display_name: "Budi" },
    ...overrides,
  }) as TIssueActivity;

const lookups = {
  stateName: (id: string) => ({ "state-1": "Todo", "state-2": "In Progress" })[id],
  memberName: (id: string) => ({ "user-1": "Budi", "user-2": "Siti" })[id],
};

describe("milestoneRows", () => {
  it("includes created events", () => {
    const rows = milestoneRows([activity({ verb: "created", field: undefined })], lookups, labels);
    expect(rows).toHaveLength(1);
    expect(rows[0].event).toBe("Created");
  });

  it("resolves state changes to names", () => {
    const rows = milestoneRows([activity({})], lookups, labels);
    expect(rows[0]).toMatchObject({ event: "State", from: "Todo", to: "In Progress", actor: "Budi" });
  });

  it("computes added and removed assignees", () => {
    const rows = milestoneRows(
      [activity({ field: "assignees", old_value: "user-1", new_value: "user-1,user-2" })],
      lookups,
      labels
    );
    expect(rows[0]).toMatchObject({ event: "Assignee", from: "—", to: "Siti" });
  });

  it("excludes non-milestone fields", () => {
    const rows = milestoneRows(
      [activity({ field: "labels" }), activity({ field: "description" }), activity({ field: "comment" })],
      lookups,
      labels
    );
    expect(rows).toHaveLength(0);
  });

  it("sorts ascending by time", () => {
    const rows = milestoneRows(
      [
        activity({ id: "b", created_at: "2026-10-12T12:00:00Z" }),
        activity({ id: "a", created_at: "2026-10-12T08:00:00Z" }),
      ],
      lookups,
      labels
    );
    expect(rows[0].createdAt).toBe("2026-10-12T08:00:00Z");
  });
});
```

- [ ] **Step 5: Run to verify it fails**

Run: `pnpm --filter=web test -- core/components/document-export/milestones.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 6: Implement milestones**

Create `milestones.ts`:

```ts
import type { TIssueActivity } from "@plane/types";

export const MILESTONE_FIELDS = ["state", "priority", "assignees", "target_date", "start_date", "archived_at"] as const;

export type TMilestoneField = (typeof MILESTONE_FIELDS)[number];

export type TMilestoneLabels = {
  created: string;
  state: string;
  priority: string;
  assignees: string;
  target_date: string;
  start_date: string;
  archived_at: string;
  noValue: string;
};

export type TMilestoneLookups = {
  stateName: (id: string) => string | undefined;
  memberName: (id: string) => string | undefined;
};

export type TMilestoneRow = {
  createdAt: string;
  actor: string;
  event: string;
  from: string;
  to: string;
};

const splitIds = (value?: string): string[] =>
  (value ?? "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const formatDate = (value?: string): string => {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(0, 10);
};

export function milestoneRows(
  activities: TIssueActivity[],
  lookups: TMilestoneLookups,
  labels: TMilestoneLabels
): TMilestoneRow[] {
  const rows: TMilestoneRow[] = [];
  const sorted = [...activities].sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());

  for (const activity of sorted) {
    const actor = activity.actor_detail?.display_name ?? "";
    const base = { createdAt: activity.created_at, actor };

    if (activity.verb === "created") {
      rows.push({ ...base, event: labels.created, from: labels.noValue, to: labels.noValue });
      continue;
    }

    const field = activity.field as TMilestoneField | undefined;
    if (!field || !(MILESTONE_FIELDS as readonly string[]).includes(field)) continue;

    switch (field) {
      case "state":
        rows.push({
          ...base,
          event: labels.state,
          from: lookups.stateName(activity.old_value ?? "") ?? activity.old_value ?? labels.noValue,
          to: lookups.stateName(activity.new_value ?? "") ?? activity.new_value ?? labels.noValue,
        });
        break;
      case "priority":
        rows.push({
          ...base,
          event: labels.priority,
          from: activity.old_value ?? labels.noValue,
          to: activity.new_value ?? labels.noValue,
        });
        break;
      case "assignees": {
        const oldIds = splitIds(activity.old_value);
        const newIds = splitIds(activity.new_value);
        const added = newIds.filter((id) => !oldIds.includes(id)).map((id) => lookups.memberName(id) ?? id);
        const removed = oldIds.filter((id) => !newIds.includes(id)).map((id) => lookups.memberName(id) ?? id);
        if (!added.length && !removed.length) break;
        rows.push({
          ...base,
          event: labels.assignees,
          from: removed.join(", ") || labels.noValue,
          to: added.join(", ") || labels.noValue,
        });
        break;
      }
      case "target_date":
      case "start_date":
      case "archived_at":
        rows.push({
          ...base,
          event: labels[field],
          from: formatDate(activity.old_value) || labels.noValue,
          to: formatDate(activity.new_value) || labels.noValue,
        });
        break;
    }
  }

  return rows;
}
```

- [ ] **Step 7: Run both tests**

Run: `pnpm --filter=web test -- core/components/document-export` → all PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/document-export/resolve-placeholders.ts apps/web/core/components/document-export/resolve-placeholders.test.ts apps/web/core/components/document-export/milestones.ts apps/web/core/components/document-export/milestones.test.ts
git commit -m "feat(web): document export placeholder and milestone helpers"
```

---

### Task 8: HTML → DOCX converter (TDD)

**Files:**

- Create: `apps/web/core/components/document-export/html-to-docx.ts` + `.test.ts`

- [ ] **Step 1: Write the failing test**

Create `html-to-docx.test.ts` (needs DOM; happy-dom via docblock):

```ts
// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { Paragraph, Table } from "docx";
import { htmlToDocxBlocks, type TImageFetcher } from "./html-to-docx";

const noopImageFetcher: TImageFetcher = async () => null;

describe("htmlToDocxBlocks", () => {
  it("maps headings and paragraphs", async () => {
    const blocks = await htmlToDocxBlocks("<h2>Hello</h2><p>Plain <strong>bold</strong></p>", {
      imageFetcher: noopImageFetcher,
    });
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toBeInstanceOf(Paragraph);
  });

  it("maps tables with header cells", async () => {
    const blocks = await htmlToDocxBlocks("<table><tr><th>ID</th></tr><tr><td>1</td></tr></table>", {
      imageFetcher: noopImageFetcher,
    });
    expect(blocks[0]).toBeInstanceOf(Table);
  });

  it("unwraps unknown tags and drops script/style", async () => {
    const blocks = await htmlToDocxBlocks("<custom-tag>kept</custom-tag><script>alert(1)</script><style>.x{}</style>", {
      imageFetcher: noopImageFetcher,
    });
    const json = JSON.stringify(blocks);
    expect(json).toContain("kept");
    expect(json).not.toContain("alert");
    expect(json).not.toContain(".x");
  });

  it("renders mention spans as plain @label text", async () => {
    const blocks = await htmlToDocxBlocks('<p><span data-type="mention" data-label="Budi">@</span> hello</p>', {
      imageFetcher: noopImageFetcher,
    });
    expect(JSON.stringify(blocks)).toContain("@Budi");
  });

  it("falls back to a placeholder when image fetch fails", async () => {
    const blocks = await htmlToDocxBlocks('<img src="https://x/y.png" alt="diagram" />', {
      imageFetcher: async () => null,
    });
    expect(JSON.stringify(blocks)).toContain("diagram");
  });

  it("embeds images when the fetcher succeeds", async () => {
    const blocks = await htmlToDocxBlocks('<img src="https://x/y.png" />', {
      imageFetcher: async () => ({ data: new Uint8Array([1, 2, 3]), width: 800, height: 400, type: "png" }),
    });
    expect(JSON.stringify(blocks)).toContain("image");
  });

  it("produces list paragraphs", async () => {
    const blocks = await htmlToDocxBlocks("<ul><li>one</li><li>two</li></ul>", { imageFetcher: noopImageFetcher });
    expect(blocks.length).toBeGreaterThanOrEqual(2);
    expect(blocks.every((block) => block instanceof Paragraph)).toBe(true);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm --filter=web test -- core/components/document-export/html-to-docx.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the converter**

Create `html-to-docx.ts`:

```ts
import {
  AlignmentType,
  ExternalHyperlink,
  HeadingLevel,
  ImageRun,
  LevelFormat,
  Paragraph,
  ShadingType,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
} from "docx";
import { getBase64Image } from "@plane/utils";

export const MAX_IMAGE_WIDTH = 600;
export const DOCX_BULLET_REFERENCE = "doc-export-bullet";
export const DOCX_NUMBERED_REFERENCE = "doc-export-numbered";

export type TDocumentBlock = Paragraph | Table;

export type TDocxImage = {
  data: Uint8Array;
  width: number;
  height: number;
  type: "png" | "jpg" | "gif";
};

export type TImageFetcher = (src: string) => Promise<TDocxImage | null>;

export type THtmlToDocxOptions = {
  imageFetcher?: TImageFetcher;
  maxImageWidth?: number;
};

const HEADING_BY_TAG: Record<string, (typeof HeadingLevel)[keyof typeof HeadingLevel]> = {
  h1: HeadingLevel.HEADING_1,
  h2: HeadingLevel.HEADING_2,
  h3: HeadingLevel.HEADING_3,
  h4: HeadingLevel.HEADING_4,
  h5: HeadingLevel.HEADING_5,
  h6: HeadingLevel.HEADING_6,
};

const BORDER = { style: "single" as const, size: 4, color: "D9D9D9" };

type TRunStyle = {
  bold?: boolean;
  italics?: boolean;
  underline?: boolean;
  strike?: boolean;
  code?: boolean;
  link?: boolean;
};

const isElement = (node: Node): node is HTMLElement => node.nodeType === 1;

const dataUrlToUint8Array = (dataUrl: string): Uint8Array => {
  const binary = atob(dataUrl.split(",")[1] ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
};

const defaultImageFetcher: TImageFetcher = async (src) => {
  try {
    const dataUrl = await getBase64Image(src);
    const mime = dataUrl.slice(5, dataUrl.indexOf(";"));
    if (mime === "image/svg+xml") return null;
    const type = mime === "image/jpeg" ? "jpg" : mime === "image/gif" ? "gif" : "png";
    const data = dataUrlToUint8Array(dataUrl);
    const bitmap = await createImageBitmap(await (await fetch(dataUrl)).blob());
    return { data, width: bitmap.width, height: bitmap.height, type };
  } catch {
    return null;
  }
};

const scaledSize = (image: TDocxImage, maxWidth: number) => {
  if (image.width <= maxWidth) return { width: image.width, height: image.height };
  const ratio = maxWidth / image.width;
  return { width: maxWidth, height: Math.round(image.height * ratio) };
};

const isMention = (element: HTMLElement) =>
  element.getAttribute("data-type") === "mention" || element.classList.contains("mention");

const collectRuns = (node: Node, style: TRunStyle): (TextRun | ExternalHyperlink)[] => {
  if (node.nodeType === 3) {
    const text = node.textContent ?? "";
    if (!text) return [];
    return [
      new TextRun({
        text,
        bold: style.bold,
        italics: style.italics,
        underline: style.underline || style.link ? {} : undefined,
        strike: style.strike,
        font: style.code ? "Courier New" : undefined,
        color: style.link ? "0563C1" : undefined,
      }),
    ];
  }
  if (!isElement(node)) return [];
  const element = node;
  const tag = element.tagName.toLowerCase();
  if (tag === "br") return [new TextRun({ break: 1 })];
  if (tag === "script" || tag === "style" || tag === "img") return [];
  if (isMention(element)) {
    const label = element.getAttribute("data-label") ?? element.textContent ?? "";
    return [new TextRun({ text: `@${label.trim()}` })];
  }
  const next: TRunStyle = { ...style };
  if (tag === "strong" || tag === "b") next.bold = true;
  if (tag === "em" || tag === "i") next.italics = true;
  if (tag === "u") next.underline = true;
  if (tag === "s" || tag === "strike" || tag === "del") next.strike = true;
  if (tag === "code") next.code = true;
  if (tag === "a") {
    next.link = true;
    const href = element.getAttribute("href");
    const children = Array.from(element.childNodes).flatMap((child) => collectRuns(child, next));
    const runs = children.filter((child): child is TextRun => child instanceof TextRun);
    if (href && runs.length) return [new ExternalHyperlink({ link: href, children: runs })];
    return children;
  }
  return Array.from(element.childNodes).flatMap((child) => collectRuns(child, next));
};

type TWalkContext = { imageFetcher: TImageFetcher; maxImageWidth: number };

const imageParagraph = async (src: string, alt: string, ctx: TWalkContext): Promise<Paragraph> => {
  const image = await ctx.imageFetcher(src);
  if (!image) {
    return new Paragraph({
      children: [new TextRun({ text: `[${alt || src}]`, italics: true, color: "808080" })],
      spacing: { after: 120 },
    });
  }
  return new Paragraph({
    children: [
      new ImageRun({ type: image.type, data: image.data, transformation: scaledSize(image, ctx.maxImageWidth) }),
    ],
    spacing: { before: 120, after: 120 },
  });
};

const listBlocks = async (list: HTMLElement, level: number, ctx: TWalkContext): Promise<Paragraph[]> => {
  const ordered = list.tagName.toLowerCase() === "ol";
  const blocks: Paragraph[] = [];
  const items = Array.from(list.children).filter((child) => child.tagName.toLowerCase() === "li");
  for (const item of items) {
    const nested: HTMLElement[] = [];
    const runs: (TextRun | ExternalHyperlink)[] = [];
    for (const child of Array.from(item.childNodes)) {
      if (isElement(child) && ["ul", "ol"].includes(child.tagName.toLowerCase())) {
        nested.push(child);
        continue;
      }
      runs.push(...collectRuns(child, {}));
    }
    blocks.push(
      new Paragraph({
        numbering: {
          reference: ordered ? DOCX_NUMBERED_REFERENCE : DOCX_BULLET_REFERENCE,
          level: Math.min(level, 2),
        },
        children: runs,
        spacing: { after: 40 },
      })
    );
    for (const child of nested) blocks.push(...(await listBlocks(child, level + 1, ctx)));
  }
  return blocks;
};

const tableBlock = (table: HTMLElement): Table => {
  const rows = Array.from(table.querySelectorAll("tr")).map((row) => {
    const cells = Array.from(row.querySelectorAll("th,td"));
    return new TableRow({
      children: cells.map((cell) => {
        const isHeader = cell.tagName.toLowerCase() === "th";
        return new TableCell({
          shading: isHeader ? { type: ShadingType.CLEAR, fill: "F2F2F2", color: "auto" } : undefined,
          children: [new Paragraph({ children: collectRuns(cell, { bold: isHeader }) })],
        });
      }),
    });
  });
  return new Table({
    rows,
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: BORDER,
      bottom: BORDER,
      left: BORDER,
      right: BORDER,
      insideHorizontal: BORDER,
      insideVertical: BORDER,
    },
  });
};

async function walkBlocks(nodes: Node[], ctx: TWalkContext): Promise<TDocumentBlock[]> {
  const blocks: TDocumentBlock[] = [];
  for (const node of nodes) {
    if (node.nodeType === 3) {
      const text = node.textContent?.trim();
      if (text) blocks.push(new Paragraph({ children: [new TextRun({ text })], spacing: { after: 120 } }));
      continue;
    }
    if (!isElement(node)) continue;
    const element = node;
    const tag = element.tagName.toLowerCase();

    if (HEADING_BY_TAG[tag]) {
      blocks.push(new Paragraph({ heading: HEADING_BY_TAG[tag], children: collectRuns(element, {}) }));
      continue;
    }
    if (tag === "p") {
      const images = Array.from(element.querySelectorAll("img"));
      for (const image of images) {
        blocks.push(await imageParagraph(image.getAttribute("src") ?? "", image.getAttribute("alt") ?? "", ctx));
      }
      const runs = images.length
        ? Array.from(element.childNodes).flatMap((child) =>
            isElement(child) && child.tagName.toLowerCase() === "img" ? [] : collectRuns(child, {})
          )
        : collectRuns(element, {});
      if (runs.length) blocks.push(new Paragraph({ children: runs, spacing: { after: 120 } }));
      continue;
    }
    if (tag === "img") {
      blocks.push(await imageParagraph(element.getAttribute("src") ?? "", element.getAttribute("alt") ?? "", ctx));
      continue;
    }
    if (tag === "ul" || tag === "ol") {
      blocks.push(...(await listBlocks(element, 0, ctx)));
      continue;
    }
    if (tag === "blockquote") {
      blocks.push(
        new Paragraph({
          children: collectRuns(element, { italics: true }),
          indent: { left: 720 },
          spacing: { after: 120 },
        })
      );
      continue;
    }
    if (tag === "pre") {
      blocks.push(
        new Paragraph({
          children: [new TextRun({ text: element.textContent ?? "", font: "Courier New", size: 20 })],
          shading: { type: ShadingType.CLEAR, fill: "F5F5F5", color: "auto" },
          spacing: { before: 120, after: 120 },
        })
      );
      continue;
    }
    if (tag === "table") {
      blocks.push(tableBlock(element));
      continue;
    }
    if (tag === "hr") {
      blocks.push(
        new Paragraph({
          border: { bottom: { style: "single", size: 6, color: "BFBFBF" } },
          children: [],
          spacing: { after: 120 },
        })
      );
      continue;
    }
    if (tag === "script" || tag === "style") continue;
    blocks.push(...(await walkBlocks(Array.from(element.childNodes), ctx)));
  }
  return blocks;
}

export async function htmlToDocxBlocks(html: string, options: THtmlToDocxOptions = {}): Promise<TDocumentBlock[]> {
  const imageFetcher = options.imageFetcher ?? defaultImageFetcher;
  const maxImageWidth = options.maxImageWidth ?? MAX_IMAGE_WIDTH;
  if (typeof DOMParser === "undefined") return [];
  const document = new DOMParser().parseFromString(html || "", "text/html");
  return walkBlocks(Array.from(document.body.childNodes), { imageFetcher, maxImageWidth });
}

export const DOCX_NUMBERING_CONFIG = [
  {
    reference: DOCX_BULLET_REFERENCE,
    levels: [0, 1, 2].map((level) => ({
      level,
      format: LevelFormat.BULLET,
      text: "•",
      alignment: AlignmentType.LEFT,
      style: { paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } } },
    })),
  },
  {
    reference: DOCX_NUMBERED_REFERENCE,
    levels: [0, 1, 2].map((level) => ({
      level,
      format: LevelFormat.DECIMAL,
      text: `%${level + 1}.`,
      alignment: AlignmentType.LEFT,
      style: { paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } } },
    })),
  },
];
```

- [ ] **Step 4: Run tests**

Run: `pnpm --filter=web test -- core/components/document-export/html-to-docx.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/html-to-docx.ts apps/web/core/components/document-export/html-to-docx.test.ts
git commit -m "feat(web): html to docx converter"
```

---

### Task 9: Document shell + download and logo helpers (TDD)

**Files:**

- Create: `apps/web/core/components/document-export/download.ts`
- Create: `apps/web/core/components/document-export/logo.ts`
- Create: `apps/web/core/components/document-export/shell.ts` + `.test.ts`

- [ ] **Step 1: Create download + logo helpers**

Create `download.ts`:

```ts
export function initiateDownload(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 1000);
}

export function documentFileName(prefix: string, title: string): string {
  const slug =
    title
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 60) || "document";
  return `${prefix}-${slug}.docx`;
}
```

Create `logo.ts`:

```ts
import type { IWorkspace, TDocumentExportSettings } from "@plane/types";
import { getBase64Image } from "@plane/utils";
import type { TDocxImage } from "./html-to-docx";

const dataUrlToUint8Array = (dataUrl: string): Uint8Array => {
  const binary = atob(dataUrl.split(",")[1] ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
};

const loadImage = async (src: string | null | undefined): Promise<TDocxImage | null> => {
  if (!src) return null;
  try {
    const dataUrl = await getBase64Image(src);
    const mime = dataUrl.slice(5, dataUrl.indexOf(";"));
    if (mime === "image/svg+xml") return null;
    const type = mime === "image/jpeg" ? "jpg" : mime === "image/gif" ? "gif" : "png";
    const data = dataUrlToUint8Array(dataUrl);
    const bitmap = await createImageBitmap(await (await fetch(dataUrl)).blob());
    return { data, width: bitmap.width, height: bitmap.height, type };
  } catch {
    return null;
  }
};

export async function resolveDocumentLogo(
  settings: Pick<TDocumentExportSettings, "header_logo_mode" | "header_logo_url">,
  workspace: IWorkspace | undefined
): Promise<TDocxImage | null> {
  if (settings.header_logo_mode === "workspace") return loadImage(workspace?.logo_url);
  if (settings.header_logo_mode === "custom") return loadImage(settings.header_logo_url);
  return null;
}
```

Add SPDX headers.

- [ ] **Step 2: Write the failing shell test**

Create `shell.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { Paragraph, TextRun } from "docx";
import { packDocument } from "./shell";

describe("packDocument", () => {
  it("packs a document into a non-empty blob", async () => {
    const blob = await packDocument({
      title: "WORK-1 — Server down",
      children: [new Paragraph({ children: [new TextRun({ text: "Body" })] })],
      headerLines: [{ text: "PT Example", align: "center" }],
      footerLines: [],
      showPageNumbers: true,
      generatedByLine: "Generated by Budi on 2026-10-12",
      creator: "Budi",
      logo: null,
      numbering: [],
    });
    expect(blob.size).toBeGreaterThan(0);
  });

  it("works without header, footer, or logo", async () => {
    const blob = await packDocument({
      title: "Minimal",
      children: [new Paragraph({ children: [new TextRun({ text: "x" })] })],
      headerLines: [],
      footerLines: [],
      showPageNumbers: false,
      generatedByLine: "",
      creator: "Budi",
      logo: null,
      numbering: [],
    });
    expect(blob.size).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 3: Run to verify it fails**

Run: `pnpm --filter=web test -- core/components/document-export/shell.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 4: Implement the shell**

Create `shell.ts`:

```ts
import { AlignmentType, Document, Footer, Header, ImageRun, Packer, PageNumber, Paragraph, TextRun } from "docx";
import type { TDocumentExportTextAlign, TDocumentExportTextLine } from "@plane/types";
import type { TDocxImage, TDocumentBlock } from "./html-to-docx";

const ALIGNMENT_MAP: Record<TDocumentExportTextAlign, (typeof AlignmentType)[keyof typeof AlignmentType]> = {
  left: AlignmentType.LEFT,
  center: AlignmentType.CENTER,
  right: AlignmentType.RIGHT,
};

const LOGO_MAX_WIDTH = 160;
const LOGO_MAX_HEIGHT = 48;

const fitLogo = (image: TDocxImage) => {
  const ratio = Math.min(LOGO_MAX_WIDTH / image.width, LOGO_MAX_HEIGHT / image.height, 1);
  return { width: Math.round(image.width * ratio), height: Math.round(image.height * ratio) };
};

export type TPackDocumentInput = {
  title: string;
  creator: string;
  children: TDocumentBlock[];
  headerLines: TDocumentExportTextLine[];
  footerLines: TDocumentExportTextLine[];
  showPageNumbers: boolean;
  generatedByLine: string;
  logo: TDocxImage | null;
  numbering: unknown[];
};

export async function packDocument(input: TPackDocumentInput): Promise<Blob> {
  const headerChildren: Paragraph[] = [];
  if (input.logo) {
    headerChildren.push(
      new Paragraph({
        alignment: AlignmentType.CENTER,
        children: [new ImageRun({ type: input.logo.type, data: input.logo.data, transformation: fitLogo(input.logo) })],
        spacing: { after: 80 },
      })
    );
  }
  for (const line of input.headerLines) {
    if (!line.text.trim()) continue;
    headerChildren.push(
      new Paragraph({
        alignment: ALIGNMENT_MAP[line.align] ?? AlignmentType.LEFT,
        children: [new TextRun({ text: line.text, size: 18, color: "404040" })],
        spacing: { after: 40 },
      })
    );
  }

  const footerChildren: Paragraph[] = [];
  for (const line of input.footerLines) {
    if (!line.text.trim()) continue;
    footerChildren.push(
      new Paragraph({
        alignment: ALIGNMENT_MAP[line.align] ?? AlignmentType.LEFT,
        children: [new TextRun({ text: line.text, size: 16, color: "595959" })],
        spacing: { after: 20 },
      })
    );
  }
  if (input.showPageNumbers) {
    footerChildren.push(
      new Paragraph({
        alignment: AlignmentType.CENTER,
        children: [
          new TextRun({
            size: 16,
            color: "595959",
            children: ["Page ", PageNumber.CURRENT, " of ", PageNumber.TOTAL_PAGES],
          }),
        ],
      })
    );
  }
  if (input.generatedByLine) {
    footerChildren.push(
      new Paragraph({
        alignment: AlignmentType.CENTER,
        children: [new TextRun({ text: input.generatedByLine, size: 16, color: "808080" })],
      })
    );
  }

  const document = new Document({
    creator: input.creator,
    title: input.title,
    numbering: { config: input.numbering as never },
    sections: [
      {
        properties: { page: { margin: { top: 1440, bottom: 1440, left: 1080, right: 1080 } } },
        headers: headerChildren.length ? { default: new Header({ children: headerChildren }) } : undefined,
        footers: footerChildren.length ? { default: new Footer({ children: footerChildren }) } : undefined,
        children: input.children,
      },
    ],
  });

  return Packer.toBlob(document);
}
```

If `Packer.toBlob` misbehaves under the node test env, use `Packer.toBuffer` and wrap with `new Blob([buffer])`; retest.

- [ ] **Step 5: Run tests**

Run: `pnpm --filter=web test -- core/components/document-export/shell.test.ts` → PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/document-export/shell.ts apps/web/core/components/document-export/shell.test.ts apps/web/core/components/document-export/download.ts apps/web/core/components/document-export/logo.ts
git commit -m "feat(web): docx shell, download, and logo helpers"
```

---

### Task 10: Work item document builder (TDD)

**Files:**

- Create: `apps/web/core/components/document-export/build-work-item-doc.ts` + `.test.ts`

- [ ] **Step 1: Write the failing test**

Create `build-work-item-doc.test.ts`:

```ts
// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { buildWorkItemDocument, type TWorkItemDocInput } from "./build-work-item-doc";

const labels = {
  properties: "Properties",
  description: "Description",
  linked: "Linked work items",
  history: "History (milestones)",
  generatedBy: "Generated by {user} on {date}",
  noValue: "—",
  columnWhen: "When",
  columnActor: "Actor",
  columnEvent: "Event",
  columnChange: "Change",
};

const baseInput = (): TWorkItemDocInput => ({
  title: "WORK-1 — Server down",
  projectName: "Support",
  stateName: "In Progress",
  typeName: "Incident",
  priority: "urgent",
  assigneeNames: ["Budi"],
  labelNames: ["infra"],
  startDate: null,
  targetDate: "2026-10-20",
  createdBy: "Budi",
  createdAt: "2026-10-11T08:00:00Z",
  updatedBy: "Siti",
  updatedAt: "2026-10-12T08:00:00Z",
  descriptionHtml: "<p>Disk full</p>",
  linkedItems: [{ identifier: "WORK-2", title: "Add disk", relationLabel: "Blocked by", stateName: "Todo" }],
  milestones: [{ createdAt: "2026-10-11T08:00:00Z", actor: "Budi", event: "Created", from: "—", to: "—" }],
  includeHistory: true,
  headerLines: [{ text: "PT Example", align: "center" }],
  footerLines: [],
  showPageNumbers: true,
  generatedByLine: "Generated by Budi on 2026-10-12",
  creator: "Budi",
  logo: null,
  labels,
  workspaceName: "Acme",
});

describe("buildWorkItemDocument", () => {
  it("builds a non-empty docx blob", async () => {
    const blob = await buildWorkItemDocument(baseInput());
    expect(blob.size).toBeGreaterThan(0);
  });

  it("omits the history appendix when disabled", async () => {
    const withHistory = await buildWorkItemDocument(baseInput());
    const withoutHistory = await buildWorkItemDocument({ ...baseInput(), includeHistory: false });
    expect(withoutHistory.size).toBeGreaterThan(0);
    expect(withHistory.size).not.toBe(withoutHistory.size);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm --filter=web test -- core/components/document-export/build-work-item-doc.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the builder**

Create `build-work-item-doc.ts`:

```ts
import { HeadingLevel, Paragraph, ShadingType, Table, TableCell, TableRow, TextRun, WidthType } from "docx";
import type { TDocumentExportTextLine } from "@plane/types";
import {
  DOCX_NUMBERING_CONFIG,
  htmlToDocxBlocks,
  type TDocxImage,
  type TDocumentBlock,
  type TImageFetcher,
} from "./html-to-docx";
import { packDocument } from "./shell";
import type { TMilestoneRow } from "./milestones";

export type TWorkItemLinkedItem = {
  identifier: string;
  title: string;
  relationLabel: string;
  stateName?: string;
};

export type TWorkItemDocLabels = {
  properties: string;
  description: string;
  linked: string;
  history: string;
  generatedBy: string;
  noValue: string;
  columnWhen: string;
  columnActor: string;
  columnEvent: string;
  columnChange: string;
};

export type TWorkItemDocInput = {
  title: string;
  projectName: string;
  stateName?: string;
  typeName?: string;
  priority?: string;
  assigneeNames: string[];
  labelNames: string[];
  startDate?: string | null;
  targetDate?: string | null;
  createdBy?: string;
  createdAt?: string;
  updatedBy?: string;
  updatedAt?: string;
  descriptionHtml: string;
  linkedItems: TWorkItemLinkedItem[];
  milestones: TMilestoneRow[];
  includeHistory: boolean;
  headerLines: TDocumentExportTextLine[];
  footerLines: TDocumentExportTextLine[];
  showPageNumbers: boolean;
  generatedByLine: string;
  creator: string;
  logo: TDocxImage | null;
  labels: TWorkItemDocLabels;
  workspaceName: string;
  imageFetcher?: TImageFetcher;
};

const BORDER = { style: "single" as const, size: 4, color: "D9D9D9" };

const sectionHeading = (text: string) =>
  new Paragraph({
    heading: HeadingLevel.HEADING_2,
    children: [new TextRun({ text })],
    spacing: { before: 240, after: 120 },
  });

const formatDate = (value?: string | null) => {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(0, 10);
};

const gridTable = (rows: [string, string][]): Table =>
  new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: BORDER,
      bottom: BORDER,
      left: BORDER,
      right: BORDER,
      insideHorizontal: BORDER,
      insideVertical: BORDER,
    },
    rows: rows.map(
      ([key, value]) =>
        new TableRow({
          children: [
            new TableCell({
              width: { size: 28, type: WidthType.PERCENTAGE },
              shading: { type: ShadingType.CLEAR, fill: "F7F7F7", color: "auto" },
              children: [new Paragraph({ children: [new TextRun({ text: key, bold: true, size: 20 })] })],
            }),
            new TableCell({
              children: [new Paragraph({ children: [new TextRun({ text: value, size: 20 })] })],
            }),
          ],
        })
    ),
  });

const dataTable = (headerCells: string[], rows: string[][]): Table =>
  new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: BORDER,
      bottom: BORDER,
      left: BORDER,
      right: BORDER,
      insideHorizontal: BORDER,
      insideVertical: BORDER,
    },
    rows: [
      new TableRow({
        children: headerCells.map(
          (cell) =>
            new TableCell({
              shading: { type: ShadingType.CLEAR, fill: "F2F2F2", color: "auto" },
              children: [new Paragraph({ children: [new TextRun({ text: cell, bold: true, size: 20 })] })],
            })
        ),
      }),
      ...rows.map(
        (row) =>
          new TableRow({
            children: row.map(
              (cell) =>
                new TableCell({ children: [new Paragraph({ children: [new TextRun({ text: cell, size: 20 })] })] })
            ),
          })
      ),
    ],
  });

export async function buildWorkItemDocument(input: TWorkItemDocInput): Promise<Blob> {
  const children: TDocumentBlock[] = [
    new Paragraph({ heading: HeadingLevel.HEADING_1, children: [new TextRun({ text: input.title })] }),
  ];
  if (input.typeName) {
    children.push(new Paragraph({ children: [new TextRun({ text: input.typeName, size: 20, color: "595959" })] }));
  }

  children.push(sectionHeading(input.labels.properties));
  const propertyRows: [string, string][] = [];
  const push = (key: string, value?: string | null) => {
    if (value && value.trim()) propertyRows.push([key, value]);
  };
  push("State", input.stateName);
  push("Priority", input.priority);
  push("Assignees", input.assigneeNames.join(", "));
  push("Labels", input.labelNames.join(", "));
  push("Start date", formatDate(input.startDate));
  push("Target date", formatDate(input.targetDate));
  push("Project", input.projectName);
  push("Created", [input.createdBy, formatDate(input.createdAt)].filter(Boolean).join(" · "));
  push("Last updated", [input.updatedBy, formatDate(input.updatedAt)].filter(Boolean).join(" · "));
  children.push(gridTable(propertyRows));

  if (input.descriptionHtml?.trim()) {
    children.push(sectionHeading(input.labels.description));
    children.push(...(await htmlToDocxBlocks(input.descriptionHtml, { imageFetcher: input.imageFetcher })));
  }

  if (input.linkedItems.length) {
    children.push(sectionHeading(input.labels.linked));
    children.push(
      dataTable(
        ["ID", "Title", "Relation", "State"],
        input.linkedItems.map((item) => [
          item.identifier,
          item.title,
          item.relationLabel,
          item.stateName ?? input.labels.noValue,
        ])
      )
    );
  }

  if (input.includeHistory) {
    children.push(sectionHeading(input.labels.history));
    if (input.milestones.length) {
      children.push(
        dataTable(
          [input.labels.columnWhen, input.labels.columnActor, input.labels.columnEvent, input.labels.columnChange],
          input.milestones.map((row) => [row.createdAt, row.actor, row.event, `${row.from} → ${row.to}`])
        )
      );
    } else {
      children.push(new Paragraph({ children: [new TextRun({ text: input.labels.noValue, size: 20 })] }));
    }
  }

  return packDocument({
    title: input.title,
    creator: input.creator,
    children,
    headerLines: input.headerLines,
    footerLines: input.footerLines,
    showPageNumbers: input.showPageNumbers,
    generatedByLine: input.generatedByLine,
    logo: input.logo,
    numbering: DOCX_NUMBERING_CONFIG,
  });
}
```

- [ ] **Step 4: Run tests**

Run: `pnpm --filter=web test -- core/components/document-export/build-work-item-doc.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/build-work-item-doc.ts apps/web/core/components/document-export/build-work-item-doc.test.ts
git commit -m "feat(web): work item docx builder"
```

### Task 11: Work item export modal + entry points

**Files:**

- Create: `apps/web/core/components/document-export/work-item-export-modal.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/quick-action-dropdowns/helper.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/quick-action-dropdowns/issue-detail.tsx`

- [ ] **Step 1: Create the modal**

Create `work-item-export-modal.tsx`:

```tsx
import { useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TIssue, TIssueActivity, TIssueRelationTypes } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { buildWorkItemDocument } from "@/components/document-export/build-work-item-doc";
import { documentFileName, initiateDownload } from "@/components/document-export/download";
import { resolveDocumentLogo } from "@/components/document-export/logo";
import { milestoneRows } from "@/components/document-export/milestones";
import { resolvePlaceholders } from "@/components/document-export/resolve-placeholders";
// hooks
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useProjectState } from "@/hooks/store/use-project-state";
import { useUser } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
// services
import { DocumentExportSettingsService } from "@/services/document-export/document-export-settings.service";
import { IssueActivityService } from "@/services/issue/issue_activity.service";
import { IssueRelationService } from "@/services/issue/issue_relation.service";

type Props = {
  issue: TIssue;
  isOpen: boolean;
  onClose: () => void;
};

const RELATION_LABEL_KEYS: Record<TIssueRelationTypes, string> = {
  blocking: "document_export.doc.relation_blocking",
  blocked_by: "document_export.doc.relation_blocked_by",
  relates_to: "document_export.doc.relation_relates_to",
  duplicate: "document_export.doc.relation_duplicate",
};

const DEFAULT_SETTINGS = {
  header_logo_mode: "none" as const,
  header_logo_url: null,
  header_text_lines: [] as { text: string; align: "left" | "center" | "right" }[],
  footer_text_lines: [] as { text: string; align: "left" | "center" | "right" }[],
  show_page_numbers: true,
  show_generated_by: true,
};

export const WorkItemExportModal = observer(function WorkItemExportModal({ issue, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const { workspaceSlug } = useParams();
  const { currentWorkspace } = useWorkspace();
  const { getStateById } = useProjectState();
  const { getProjectIdentifierById, getProjectById } = useProject();
  const { memberMap } = useMember();
  const { getLabelById } = useLabel();
  const { workItemTypes } = useWorkItemType();
  const { data: currentUser } = useUser();
  const [includeHistory, setIncludeHistory] = useState(true);
  const [isExporting, setIsExporting] = useState(false);
  const settingsService = useMemo(() => new DocumentExportSettingsService(), []);
  const activityService = useMemo(() => new IssueActivityService(), []);
  const relationService = useMemo(() => new IssueRelationService(), []);

  const handleExport = async () => {
    if (!workspaceSlug) return;
    setIsExporting(true);
    try {
      const slug = workspaceSlug.toString();
      const [settings, activities, relations] = await Promise.all([
        settingsService.fetchSettings(slug).catch(() => null),
        activityService.getIssueActivities(slug, issue.project_id ?? "", issue.id).catch(() => []),
        issue.project_id
          ? relationService.listIssueRelations(slug, issue.project_id, issue.id).catch(() => null)
          : Promise.resolve(null),
      ]);

      const resolvedSettings = settings ?? { ...DEFAULT_SETTINGS };
      const identifier = `${getProjectIdentifierById(issue.project_id)}-${issue.sequence_id}`;
      const exportedByName = currentUser?.display_name ?? "";
      const placeholderValues = {
        workspace_name: currentWorkspace?.name ?? "",
        document_title: issue.name,
        document_id: identifier,
        export_date: new Date().toISOString().slice(0, 10),
        exported_by: exportedByName,
      };
      const headerLines = resolvedSettings.header_text_lines.map((line) => ({
        ...line,
        text: resolvePlaceholders(line.text, placeholderValues),
      }));
      const footerLines = resolvedSettings.footer_text_lines.map((line) => ({
        ...line,
        text: resolvePlaceholders(line.text, placeholderValues),
      }));
      const milestones = milestoneRows(
        activities as TIssueActivity[],
        {
          stateName: (id) => getStateById(id)?.name,
          memberName: (id) => memberMap?.[id]?.display_name,
        },
        {
          created: t("document_export.doc.event_created"),
          state: t("document_export.doc.event_state"),
          priority: t("document_export.doc.event_priority"),
          assignees: t("document_export.doc.event_assignees"),
          target_date: t("document_export.doc.event_target_date"),
          start_date: t("document_export.doc.event_start_date"),
          archived_at: t("document_export.doc.event_archived_at"),
          noValue: t("document_export.doc.no_value"),
        }
      );
      const linkedItems = Object.entries(relations ?? {}).flatMap(([relationType, related]) =>
        (related as TIssue[]).map((item) => ({
          identifier: `${getProjectIdentifierById(item.project_id)}-${item.sequence_id}`,
          title: item.name,
          relationLabel: t(RELATION_LABEL_KEYS[relationType as TIssueRelationTypes]),
          stateName: getStateById(item.state_id)?.name,
        }))
      );
      const logo = await resolveDocumentLogo(resolvedSettings, currentWorkspace);

      const blob = await buildWorkItemDocument({
        title: `${identifier} — ${issue.name}`,
        projectName: getProjectById(issue.project_id)?.name ?? "",
        stateName: getStateById(issue.state_id)?.name,
        typeName: workItemTypes?.find((type) => type.id === issue.type_id)?.name,
        priority: issue.priority,
        assigneeNames: (issue.assignee_ids ?? []).map((id) => memberMap?.[id]?.display_name ?? id),
        labelNames: (issue.label_ids ?? []).map((id) => getLabelById(id)?.name ?? id),
        startDate: issue.start_date,
        targetDate: issue.target_date,
        createdBy: memberMap?.[issue.created_by ?? ""]?.display_name,
        createdAt: issue.created_at,
        updatedBy: memberMap?.[issue.updated_by ?? ""]?.display_name,
        updatedAt: issue.updated_at,
        descriptionHtml: issue.description_html ?? "",
        linkedItems,
        milestones,
        includeHistory,
        headerLines,
        footerLines,
        showPageNumbers: resolvedSettings.show_page_numbers,
        generatedByLine: resolvedSettings.show_generated_by
          ? t("document_export.doc.generated_by", {
              user: exportedByName,
              date: new Date().toISOString().slice(0, 10),
            })
          : "",
        creator: exportedByName,
        logo,
        labels: {
          properties: t("document_export.doc.properties"),
          description: t("document_export.doc.description"),
          linked: t("document_export.doc.linked_work_items"),
          history: t("document_export.doc.history"),
          generatedBy: t("document_export.doc.generated_by"),
          noValue: t("document_export.doc.no_value"),
          columnWhen: t("document_export.doc.column_when"),
          columnActor: t("document_export.doc.column_actor"),
          columnEvent: t("document_export.doc.column_event"),
          columnChange: t("document_export.doc.column_change"),
        },
        workspaceName: currentWorkspace?.name ?? "",
      });

      initiateDownload(blob, documentFileName(identifier, issue.name));
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("document_export.modal.success") });
      onClose();
    } catch (error) {
      console.error("Error exporting work item document:", error);
      setToast({ type: TOAST_TYPE.ERROR, title: t("document_export.modal.error") });
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.CENTER} width={EModalWidth.SM}>
      <div className="space-y-5 p-5">
        <h3 className="text-18 font-medium text-secondary">{t("document_export.modal.title")}</h3>
        <p className="text-13 text-tertiary">{t("document_export.modal.description")}</p>
        <label className="flex items-center gap-2 text-13 text-secondary">
          <input
            type="checkbox"
            checked={includeHistory}
            onChange={(event) => setIncludeHistory(event.target.checked)}
          />
          {t("document_export.modal.include_history")}
        </label>
      </div>
      <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
        <Button variant="secondary" size="lg" onClick={onClose}>
          {t("cancel")}
        </Button>
        <Button variant="primary" size="lg" loading={isExporting} onClick={handleExport}>
          {isExporting ? t("document_export.modal.exporting") : t("document_export.modal.export")}
        </Button>
      </div>
    </ModalCore>
  );
});
```

Verify hook shapes at implementation time and adjust with fallbacks: `useUser().data?.display_name`, `useProject().getProjectById`, `useLabel().getLabelById`, `useMember().memberMap`, `useWorkItemType().workItemTypes` (all confirmed to exist or have close equivalents). If `t("cancel")` key is missing, use `"Cancel"`.

- [ ] **Step 2: Wire the menu item in `helper.tsx`**

- Add `onExportDocument?: () => void;` to `MenuItemFactoryProps`.
- Add `ExportOutline` to the `@makeplane/propel/icons` import.
- Add a factory method and expose it:

```ts
const createExportDocumentMenuItem = (): TContextMenuItem => ({
  key: "export-document",
  title: t("document_export.export_as_document"),
  icon: ExportOutline,
  action: () => handleOptionalAction(props.onExportDocument, "Export document"),
});
```

Include `createExportDocumentMenuItem` in the returned object of `useMenuItemFactory`, and add `factory.createExportDocumentMenuItem(),` to `useWorkItemDetailMenuItems` after `createOpenInNewTabMenuItem()`.

- [ ] **Step 3: Wire icon + modal in `issue-detail.tsx`**

- Add `ExportOutline` to the icons import and import `WorkItemExportModal`.
- Add state `const [isExportModalOpen, setIsExportModalOpen] = useState(false);`
- Add `onExportDocument: () => setIsExportModalOpen(true)` to `menuItemProps`.
- Render before `<ContextMenu ... />`:

```tsx
<IconButton size="lg" variant="secondary" icon={ExportOutline} onClick={() => setIsExportModalOpen(true)} />
```

- Render the modal next to the other modals:

```tsx
<WorkItemExportModal issue={issue} isOpen={isExportModalOpen} onClose={() => setIsExportModalOpen(false)} />
```

- [ ] **Step 4: Typecheck, lint, test**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web test -- core/components/document-export` → PASS.

- [ ] **Step 5: Manual smoke**

Dev stack → open a work item detail → kebab shows "Export as document" + header export icon → dialog → Export downloads `.docx` opening correctly in LibreOffice (title, properties, description, linked items, history table, header/footer).

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/document-export apps/web/core/components/issues/issue-layouts/quick-action-dropdowns/helper.tsx apps/web/core/components/issues/issue-layouts/quick-action-dropdowns/issue-detail.tsx
git commit -m "feat(web): export work item as docx from detail actions"
```

---

### Task 12: KB article document builder (TDD)

**Files:**

- Create: `apps/web/core/components/document-export/build-page-doc.ts` + `.test.ts`

- [ ] **Step 1: Write the failing test**

Create `build-page-doc.test.ts`:

```ts
// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { buildPageDocument, type TPageDocInput } from "./build-page-doc";

const labels = {
  properties: "Properties",
  subPages: "Sub-pages",
  generatedBy: "Generated by {user} on {date}",
  noValue: "—",
};

const input = (): TPageDocInput => ({
  title: "Runbook",
  bodyHtml: "<p>Steps</p>",
  ownerName: "Budi",
  createdBy: "Budi",
  createdAt: "2026-10-01T00:00:00Z",
  updatedBy: "Siti",
  updatedAt: "2026-10-10T00:00:00Z",
  labelNames: ["ops"],
  projectName: "Support",
  versionCount: 3,
  subPages: [{ name: "Child A", bodyHtml: "<p>child</p>", level: 1 }],
  headerLines: [],
  footerLines: [],
  showPageNumbers: true,
  generatedByLine: "",
  creator: "Budi",
  logo: null,
  labels,
  imageFetcher: async () => null,
});

describe("buildPageDocument", () => {
  it("builds a non-empty docx blob", async () => {
    const blob = await buildPageDocument(input());
    expect(blob.size).toBeGreaterThan(0);
  });

  it("handles no sub-pages", async () => {
    const blob = await buildPageDocument({ ...input(), subPages: [] });
    expect(blob.size).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm --filter=web test -- core/components/document-export/build-page-doc.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the builder**

Create `build-page-doc.ts`:

```ts
import { HeadingLevel, Paragraph, TextRun } from "docx";
import type { TDocumentExportTextLine } from "@plane/types";
import {
  DOCX_NUMBERING_CONFIG,
  htmlToDocxBlocks,
  type TDocxImage,
  type TDocumentBlock,
  type TImageFetcher,
} from "./html-to-docx";
import { packDocument } from "./shell";

export type TPageDocSubPage = {
  name: string;
  bodyHtml: string;
  level: number;
};

export type TPageDocLabels = {
  properties: string;
  subPages: string;
  generatedBy: string;
  noValue: string;
};

export type TPageDocInput = {
  title: string;
  bodyHtml: string;
  ownerName?: string;
  createdBy?: string;
  createdAt?: string;
  updatedBy?: string;
  updatedAt?: string;
  labelNames: string[];
  projectName?: string;
  versionCount?: number;
  subPages: TPageDocSubPage[];
  headerLines: TDocumentExportTextLine[];
  footerLines: TDocumentExportTextLine[];
  showPageNumbers: boolean;
  generatedByLine: string;
  creator: string;
  logo: TDocxImage | null;
  labels: TPageDocLabels;
  imageFetcher?: TImageFetcher;
};

const formatDate = (value?: string) => {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(0, 10);
};

const propertyLine = (key: string, value?: string) => {
  if (!value?.trim()) return undefined;
  return new Paragraph({
    children: [new TextRun({ text: `${key}: `, bold: true, size: 20 }), new TextRun({ text: value, size: 20 })],
    spacing: { after: 40 },
  });
};

export async function buildPageDocument(input: TPageDocInput): Promise<Blob> {
  const children: TDocumentBlock[] = [
    new Paragraph({ heading: HeadingLevel.HEADING_1, children: [new TextRun({ text: input.title })] }),
    new Paragraph({
      heading: HeadingLevel.HEADING_2,
      children: [new TextRun({ text: input.labels.properties })],
      spacing: { before: 240, after: 120 },
    }),
  ];
  for (const line of [
    propertyLine("Owner", input.ownerName),
    propertyLine("Created", [input.createdBy, formatDate(input.createdAt)].filter(Boolean).join(" · ")),
    propertyLine("Last updated", [input.updatedBy, formatDate(input.updatedAt)].filter(Boolean).join(" · ")),
    propertyLine("Labels", input.labelNames.join(", ")),
    propertyLine("Project", input.projectName),
    propertyLine("Versions", input.versionCount !== undefined ? String(input.versionCount) : undefined),
  ]) {
    if (line) children.push(line);
  }

  if (input.bodyHtml?.trim()) {
    children.push(...(await htmlToDocxBlocks(input.bodyHtml, { imageFetcher: input.imageFetcher })));
  }

  if (input.subPages.length) {
    children.push(
      new Paragraph({
        heading: HeadingLevel.HEADING_2,
        children: [new TextRun({ text: input.labels.subPages })],
        spacing: { before: 240, after: 120 },
      })
    );
    const counters: (number | undefined)[] = [];
    for (const subPage of input.subPages) {
      counters.length = subPage.level;
      counters[subPage.level - 1] = (counters[subPage.level - 1] ?? 0) + 1;
      const number = counters.map((value) => value ?? 1).join(".");
      const headingLevel = subPage.level === 1 ? HeadingLevel.HEADING_3 : HeadingLevel.HEADING_4;
      children.push(
        new Paragraph({
          heading: headingLevel,
          children: [new TextRun({ text: `${number} ${subPage.name}` })],
          spacing: { before: 200, after: 80 },
        })
      );
      if (subPage.bodyHtml?.trim()) {
        children.push(...(await htmlToDocxBlocks(subPage.bodyHtml, { imageFetcher: input.imageFetcher })));
      }
    }
  }

  return packDocument({
    title: input.title,
    creator: input.creator,
    children,
    headerLines: input.headerLines,
    footerLines: input.footerLines,
    showPageNumbers: input.showPageNumbers,
    generatedByLine: input.generatedByLine,
    logo: input.logo,
    numbering: DOCX_NUMBERING_CONFIG,
  });
}
```

- [ ] **Step 4: Run tests**

Run: `pnpm --filter=web test -- core/components/document-export/build-page-doc.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/document-export/build-page-doc.ts apps/web/core/components/document-export/build-page-doc.test.ts
git commit -m "feat(web): kb article docx builder"
```

---

### Task 13: Wire DOCX into the page export modal

**Files:**

- Modify: `apps/web/core/components/pages/modals/export-page-modal.tsx`

- [ ] **Step 1: Extend the modal**

Changes to `ExportPageModal`:

1. `TExportFormats` becomes `"pdf" | "markdown" | "docx"`; add `{ key: "docx", label: "DOCX (Formal document)" }` to `EXPORT_FORMATS`.
2. In `handleExport`, add:

```ts
if (selectedExportFormat === "docx") {
  await handleExportAsDocx();
}
```

3. Implement `handleExportAsDocx`:

```ts
const handleExportAsDocx = async () => {
  if (!workspaceSlug || !projectId) return;
  const slug = workspaceSlug.toString();
  const settingsService = new DocumentExportSettingsService();
  const pageService = new ProjectPageService();
  const versionService = new ProjectPageVersionService();
  const settings = await settingsService.fetchSettings(slug).catch(() => null);
  const resolvedSettings = settings ?? {
    header_logo_mode: "none" as const,
    header_logo_url: null,
    header_text_lines: [],
    footer_text_lines: [],
    show_page_numbers: true,
    show_generated_by: true,
  };
  const exportedByName = currentUser?.display_name ?? "";
  const documentId = pageId?.toString() ?? "";
  const identifier = documentId ? documentId.slice(0, 8) : "";
  const placeholderValues = {
    workspace_name: currentWorkspace?.name ?? "",
    document_title: pageTitle,
    document_id: identifier,
    export_date: new Date().toISOString().slice(0, 10),
    exported_by: exportedByName,
  };
  const headerLines = resolvedSettings.header_text_lines.map((line) => ({
    ...line,
    text: resolvePlaceholders(line.text, placeholderValues),
  }));
  const footerLines = resolvedSettings.footer_text_lines.map((line) => ({
    ...line,
    text: resolvePlaceholders(line.text, placeholderValues),
  }));

  const detail = pageId ? await pageService.fetchById(slug, projectId.toString(), pageId.toString(), false) : null;
  const allPages = await pageService.fetchAll(slug, projectId.toString()).catch(() => []);
  const subPages: { name: string; bodyHtml: string; level: number }[] = [];
  const collectChildren = async (parentId: string, level: number) => {
    if (level > 10 || subPages.length >= 50) return;
    const children = allPages.filter((page) => page.parent === parentId && !page.archived_at);
    for (const child of children) {
      if (subPages.length >= 50) break;
      const childDetail = await pageService.fetchById(slug, projectId.toString(), child.id, false).catch(() => null);
      subPages.push({ name: child.name, bodyHtml: childDetail?.description_html ?? "", level });
      await collectChildren(child.id, level + 1);
    }
  };
  if (pageId) await collectChildren(pageId.toString(), 1);

  const versions = pageId
    ? await versionService.fetchAllVersions(slug, projectId.toString(), pageId.toString()).catch(() => [])
    : [];
  const logo = await resolveDocumentLogo(resolvedSettings, currentWorkspace);

  const blob = await buildPageDocument({
    title: pageTitle,
    bodyHtml: `<h1 class="page-title">${pageTitle}</h1>${editorRef?.getDocument().html ?? "<p></p>"}`,
    ownerName: memberMap?.[detail?.owned_by ?? ""]?.display_name,
    createdBy: memberMap?.[detail?.created_by ?? ""]?.display_name,
    createdAt: detail?.created_at,
    updatedBy: memberMap?.[detail?.updated_by ?? ""]?.display_name,
    updatedAt: detail?.updated_at,
    labelNames: (detail?.label_ids ?? []).map((id) => getLabelById(id)?.name ?? id),
    projectName: getProjectById(projectId.toString())?.name,
    versionCount: versions.length || undefined,
    subPages,
    headerLines,
    footerLines,
    showPageNumbers: resolvedSettings.show_page_numbers,
    generatedByLine: resolvedSettings.show_generated_by
      ? t("document_export.doc.generated_by", { user: exportedByName, date: new Date().toISOString().slice(0, 10) })
      : "",
    creator: exportedByName,
    logo,
    labels: {
      properties: t("document_export.doc.properties"),
      subPages: t("document_export.doc.sub_pages"),
      generatedBy: t("document_export.doc.generated_by"),
      noValue: t("document_export.doc.no_value"),
    },
  });
  initiateDownload(blob, documentFileName("article", pageTitle));
};
```

4. Add imports at the top of the file: `import { useTranslation } from "@plane/i18n";` + `const { t } = useTranslation();`, `import { buildPageDocument } from "@/components/document-export/build-page-doc";`, `import { documentFileName, initiateDownload } from "@/components/document-export/download";`, `import { resolveDocumentLogo } from "@/components/document-export/logo";`, `import { resolvePlaceholders } from "@/components/document-export/resolve-placeholders";`, `DocumentExportSettingsService`, `ProjectPageService`, `ProjectPageVersionService`, plus hooks `useWorkspace`, `useProject`, `useMember`, `useLabel`, `useUser`.
5. Replace the local `initiateDownload` function with the imported one (delete the local copy).
6. `pageId` comes from `useParams()` (the route is page-scoped): destructure `const { workspaceSlug, projectId, pageId } = useParams();`.

Do NOT change PDF/Markdown behavior. The `editorRef` HTML is the freshest content (unsaved edits included) — keep that source rather than `description_html` for the main body.

- [ ] **Step 2: Typecheck, lint, test**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web test -- core/components/document-export` → PASS.

- [ ] **Step 3: Manual smoke**

Open a Wiki page → toolbar export → choose "DOCX (Formal document)" → Export downloads `.docx` with properties (owner, dates, labels, version count), body, numbered sub-pages, header/footer.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/pages/modals/export-page-modal.tsx
git commit -m "feat(web): export kb article as docx from page export modal"
```

---

### Task 14: Full verification

**Files:** none (verification only).

- [ ] **Step 1: Web checks**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web test`
Expected: all PASS.

- [ ] **Step 2: Rust checks**

Run: `cd apps/api-rs && cargo fmt --check && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test document_export_settings_test -- --test-threads=1`
Expected: PASS. If the repo's standard check command differs, use `pnpm check` from root as the final gate.

- [ ] **Step 3: i18n sync**

Run: `pnpm --filter=@plane/i18n check:sync` → exits 0.

- [ ] **Step 4: End-to-end manual verification (spec §11)**

With the dev stack running:

1. Configure header/footer in workspace settings (logo custom upload, 2 header lines, footer, page numbers on, generated-by on).
2. Export a work item with description images, linked items, and at least 5 history events → open in Word and LibreOffice: header/footer on every page, Page X of Y, images embedded, milestone table ascending, excluded fields absent.
3. Export a KB article with 2 levels of sub-pages and check version count in properties.
4. Export with a workspace that has no settings row → defaults (page numbers + generated-by, no header lines).
5. Temporarily set a broken image URL in a description → export still succeeds with alt-text placeholder.

- [ ] **Step 5: Prod build + restart (tunnel deployment, per AGENTS.md)**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

For api-rs (if backend changes should be live): follow the detached LTO build procedure in `AGENTS.md` (`docker compose -f docker-compose-local.yml up -d --build api worker beat-worker` via `setsid` + log file), then verify `curl http://localhost:8000/health` and restart `plane-live.service`.

- [ ] **Step 6: Commit (if any fixes were needed)**

```bash
git add -p
git commit -m "fix(web): document export verification fixes"
```

---

## Notes for the executor

- Repo conventions: SPDX header comments in every TS file; `catalog:` for external deps; oxfmt formatting (`pnpm fix:format`); OxLint.
- Do not touch `apps/live`; DOCX generation is client-side by design.
- If a docx API shape differs from the snippets (v9 differences in `ImageRun`, `numbering`, `IBorderOptions`, `Packer`), adapt locally and keep the tests' behavioral assertions.
- `route_inventory_test.rs` needs no update (this is a net-new endpoint, not a Django-parity claim).
