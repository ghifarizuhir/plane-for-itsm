# Galileo Chat History (Multi-Sesi) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menyimpan percakapan Galileo (Klasik + Agent) di server sebagai multi-sesi: daftar percakapan, buka/rename/hapus, konteks dibangun server dari 8 pesan terakhir, kartu proposal jadwal tetap hidup di history.

**Architecture:** Dua tabel baru (`ai_conversations`, `ai_messages`) + modul `routes/ai_conversations.rs` berisi CRUD percakapan dan endpoint pesan. Endpoint chat yang ada (`/ai-agent/`, `/ai-assistant/`) menjadi stateful: `conversation_id` wajib, server menulis pesan user sebelum panggilan LLM dan menulis pesan assistant + prune + `updated_at` dalam satu transaksi setelahnya. FE mengganti sumber data dari `localStorage` ke service percakapan, dengan panel history di dalam sidebar assistant.

**Tech Stack:** Rust (axum, sqlx/Postgres, serde_json), Rig agent, Redis (tidak berubah), React 19 + MobX + vitest, Tailwind/propel.

**Spec:** `docs/superpowers/specs/2026-09-24-galileo-chat-history-design.md`

---

## Penyimpangan kecil dari spec (disengaja, ikut diverifikasi di Task 13)

1. **Transaksi:** spec menulis "satu transaksi" untuk kedua pesan. Implementasi: pesan user di-commit **sebelum** panggilan LLM (agar transaksi tidak menggantung sampai 180 detik), lalu `INSERT` pesan assistant + prune pesan + `UPDATE updated_at` percakapan dalam **satu transaksi**. Sisi assistant tetap atomik.
2. **Test komponen FE:** repo tidak punya infrastruktur test komponen (hanya lib/store/hook). Cakupan test FE = lib + store; panel diuji lewat smoke manual + typecheck/build.
3. **Kontrak prompt:** `prompt` di body chat kini = **teks mentah user** (disimpan apa adanya sebagai pesan). Blok konteks work item + timezone dikirim di field baru `context`; history dibangun server dari DB (8 pesan). `buildAiPrompt` FE dihapus.
4. **Konfirmasi hapus** di panel history memakai konfirmasi inline dua langkah (bukan modal), konsisten dengan ukuran panel sidebar.

## Peta file

**Backend (`apps/api-rs/`)**

- Create: `migrations/0008_ai_conversations.sql`
- Modify: `crates/ai/src/agent.rs` (+ `history_prompt`, `HistoryMessage`, `HISTORY_MESSAGE_LIMIT`; re-export lewat `crates/ai/src/lib.rs` bila perlu)
- Create: `crates/api/src/routes/ai_conversations.rs`
- Modify: `crates/api/src/routes/mod.rs` (tambah `pub mod ai_conversations;`)
- Modify: `crates/api/src/main.rs` (4 route baru)
- Modify: `crates/api/src/routes/ai_agent/mod.rs` (handler stateful)
- Modify: `crates/api/src/routes/ai.rs` (handler stateful)
- Create: `crates/api/tests/ai_conversations_test.rs`
- Modify: `crates/api/tests/ai_agent_test.rs` (unit `history_prompt` bila belum ada di crate `ai`; handler tests yang lama disesuaikan)

**Frontend (`apps/web/`)**

- Create: `core/lib/ai-conversations.ts` (+ `core/lib/ai-conversations.test.ts`)
- Modify: `core/lib/ai-context.ts` (buang `buildAiPrompt`/limit; tambah `buildAiContext`; `TAiMessage.createdAt`)
- Modify: `core/lib/ai-context.test.ts`
- Create: `core/services/ai-conversations.service.ts`
- Modify: `core/services/ai.service.ts` (payload `context`/`conversation_id`, tipe respons)
- Modify: `core/store/ai-assistant.store.ts` (+ `core/store/ai-assistant.store.test.ts`)
- Create: `core/components/ai/assistant-sidebar/conversation-history-panel.tsx`
- Modify: `core/components/ai/assistant-sidebar/root.tsx`

**Docs**

- Modify: `docs/superpowers/specs/2026-09-24-galileo-chat-history-design.md` (sinkron dengan penyimpangan 1–4)
- Modify: `apps/api-rs/scripts/smoke.sh` (komentar saja; body tanpa `conversation_id` tetap 400 yang diterima smoke)

## Aturan kerja (berlaku untuk semua task)

- Repo `/home/ghifari/plane-for-itsm`, branch `preview`. Jangan menyentuh 7 file modifikasi lama yang tidak terkait (`apps/admin/vite.config.ts`, `apps/api-rs/crates/api/src/routes/auth.rs`, `apps/api/plane/tests/unit/utils/test_host.py`, `apps/space/vite.config.ts`, `apps/web/app/root.tsx`, `apps/web/vite.config.ts`, `packages/constants/src/metadata.ts`).
- Rust: format dengan `rustfmt --edition 2021 <file>` (JANGAN `cargo fmt --all`). Verifikasi suite: `cargo test -p api -- --test-threads=1` (timeout ≥ 900s, jalankan sekuensial — mesin 8 core).
- DB test: `postgres://plane:plane@localhost:5432/plane` (container `plane-db`, user/db `plane`).
- FE: `pnpm --filter=web test`, `pnpm --filter=web check:types`. Jangan menjalankan `cargo test` dan `pnpm build` bersamaan.
- Commit kecil per task; pre-commit hook oxfmt/oxlint aktif; jangan `--no-verify`.

---

### Task 1: Migrasi `0008_ai_conversations.sql`

**Files:**

- Create: `apps/api-rs/migrations/0008_ai_conversations.sql`

- [ ] **Step 1: Tulis migrasi**

```sql
-- Galileo chat history: multi-session conversations for the AI assistant.
-- Applied at boot by `common::db::migrate` (sqlx migrate).

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

- [ ] **Step 2: Terapkan ke DB dev lokal (agar test integrasi bisa jalan sebelum image dibangun)**

Run:

```bash
docker exec -i plane-db psql -U plane -d plane -v ON_ERROR_STOP=1 < apps/api-rs/migrations/0008_ai_conversations.sql
```

Expected: `CREATE TABLE` dua kali + `CREATE INDEX` dua kali, tanpa error.

- [ ] **Step 3: Verifikasi struktur**

Run:

```bash
docker exec plane-db psql -U plane -d plane -c "\d ai_conversations" && docker exec plane-db psql -U plane -d plane -c "\d ai_messages"
```

Expected: kolom + constraint + index sesuai (mode CHECK, role CHECK, index `ai_conversations_owner_idx`, `ai_messages_conversation_idx`).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0008_ai_conversations.sql
git commit -m "feat(api-rs): add ai_conversations and ai_messages tables"
```

---

### Task 2: Helper konteks history di crate `ai`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs`
- Test: unit test di `apps/api-rs/crates/ai/src/agent.rs` (mod `tests` bila belum ada, atau `crates/ai/tests/agent_history_test.rs`)

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/api-rs/crates/ai/tests/agent_history_test.rs`:

```rust
//! Pure tests for `history_prompt` (no DB, no network).

use ai::agent::{history_prompt, HistoryMessage, HISTORY_MESSAGE_LIMIT};

fn message(role: &str, content: &str) -> HistoryMessage {
    HistoryMessage {
        role: role.to_string(),
        content: content.to_string(),
    }
}

#[test]
fn keeps_only_the_last_eight_messages_oldest_first() {
    let mut history = Vec::new();
    for index in 0..10 {
        history.push(message("user", &format!("q{index}")));
        history.push(message("assistant", &format!("a{index}")));
    }
    let prompt = history_prompt("ctx", &history, "new question");
    assert_eq!(HISTORY_MESSAGE_LIMIT, 8);
    // 20 messages -> only the newest 8 survive
    assert!(!prompt.contains("q0"));
    assert!(!prompt.contains("q5"));
    assert!(prompt.contains("q6"));
    assert!(prompt.contains("a9"));
    // oldest-first inside the window
    assert!(prompt.find("q6").unwrap() < prompt.find("a9").unwrap());
    assert!(prompt.contains("User's new question: new question"));
}

#[test]
fn labels_roles_and_marks_empty_history() {
    let prompt = history_prompt("ctx", &[], "hi");
    assert!(prompt.contains("Conversation so far:\n(empty)"));
    assert!(prompt.contains("User's new question: hi"));

    let prompt = history_prompt(
        "ctx",
        &[message("user", "hello"), message("assistant", "world")],
        "again",
    );
    assert!(prompt.contains("User: hello"));
    assert!(prompt.contains("Assistant: world"));
}

#[test]
fn blank_context_is_omitted() {
    let prompt = history_prompt("   ", &[], "hi");
    assert!(prompt.starts_with("Conversation so far:"));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (di `apps/api-rs`): `cargo test -p ai --test agent_history_test`
Expected: FAIL — `cannot find function history_prompt` / `HistoryMessage`.

- [ ] **Step 3: Implementasi minimal di `crates/ai/src/agent.rs`**

Tambahkan setelah `effective_prompt`:

```rust
/// How many stored messages are folded back into the model prompt.
pub const HISTORY_MESSAGE_LIMIT: usize = 8;

/// One stored conversation message used to rebuild model context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

/// Compose the model prompt from the FE-supplied context block, the stored
/// conversation (newest `HISTORY_MESSAGE_LIMIT`, oldest first) and the new
/// question. Mirrors the old FE `buildAiPrompt` output.
pub fn history_prompt(context: &str, history: &[HistoryMessage], question: &str) -> String {
    let history_block = history
        .iter()
        .rev()
        .take(HISTORY_MESSAGE_LIMIT)
        .rev()
        .map(|message| {
            let role = if message.role == "user" { "User" } else { "Assistant" };
            format!("{role}: {}", message.content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let history_block = if history_block.is_empty() {
        "(empty)".to_string()
    } else {
        history_block
    };
    let context = context.trim();
    let prefix = if context.is_empty() {
        String::new()
    } else {
        format!("{context}\n\n")
    };
    format!("{prefix}Conversation so far:\n{history_block}\n\nUser's new question: {question}")
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai --test agent_history_test`
Expected: `3 passed`.

- [ ] **Step 5: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/ai/src/agent.rs apps/api-rs/crates/ai/tests/agent_history_test.rs
git add apps/api-rs/crates/ai/src/agent.rs apps/api-rs/crates/ai/tests/agent_history_test.rs
git commit -m "feat(ai): build model prompt from stored conversation history"
```

---

### Task 3: Modul `ai_conversations` — list + create (+ prune cap 50)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` (tambah `pub mod ai_conversations;`)
- Modify: `apps/api-rs/crates/api/src/main.rs` (route list/create)
- Create: `apps/api-rs/crates/api/tests/ai_conversations_test.rs`

- [ ] **Step 1: Tulis test integrasi yang gagal**

Buat `apps/api-rs/crates/api/tests/ai_conversations_test.rs` (harness meniru `ai_schedule_test.rs`; salin helper `database_url`, `pool`, `state`, `insert_user`, dan `Scratch` dari file itu, ganti prefix slug menjadi `aich-` dan tambah `purge` untuk tabel baru):

```rust
//! DB-backed tests for the Galileo conversation endpoints.

use api::middleware::auth::AuthUser;
use api::routes::ai_conversations;
use api::state::AppState;
use axum::extract::{Path, State};
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

async fn state(pool: &PgPool) -> AppState {
    AppState {
        pool: pool.clone(),
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
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

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("aich-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'AI Chat', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(user_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch workspace member");
        Self {
            slug,
            workspace_id,
            user_id,
        }
    }

    async fn add_actor(&self, pool: &PgPool, workspace_role: i16) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(workspace_role)
        .bind(user_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch actor");
        user_id
    }

    async fn purge(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM ai_conversations WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE username LIKE $1")
            .bind(format!("{}%", self.slug))
            .execute(pool)
            .await
            .ok();
    }
}

#[tokio::test]
async fn create_and_list_are_owner_scoped() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let other = scratch.add_actor(&pool, 20).await;

    let (status, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "agent"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);
    assert!(Uuid::parse_str(created["id"].as_str().unwrap()).is_ok());
    assert_eq!(created["mode"], json!("agent"));
    assert_eq!(created["title"], json!(""));

    let (status, Json(list)) = ai_conversations::list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("list");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["conversations"].as_array().unwrap().len(), 1);

    // Another workspace member cannot see it.
    let (_, Json(other_list)) = ai_conversations::list(
        State(st.clone()),
        AuthUser(other),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("other list");
    assert_eq!(other_list["conversations"].as_array().unwrap().len(), 0);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn creating_more_than_fifty_conversations_prunes_the_oldest() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    for index in 0..51 {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at) \
             VALUES ($1, $2, $3, 'classic', $4, now() - make_interval(secs => $5), now() - make_interval(secs => $5))",
        )
        .bind(id)
        .bind(scratch.workspace_id)
        .bind(scratch.user_id)
        .bind(format!("old-{index}"))
        .bind(5000 - index)
        .execute(&pool)
        .await
        .unwrap();
    }

    let (status, _) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);

    let (_, Json(list)) = ai_conversations::list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("list");
    let conversations = list["conversations"].as_array().unwrap();
    assert_eq!(conversations.len(), 50);
    // Newest (empty title) stays.
    assert_eq!(conversations[0]["title"], json!(""));

    // Assert the DB itself, not just the LIMIT-50 list: the prune must have
    // deleted the two oldest rows.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE workspace_id = $1 AND created_by_id = $2",
    )
    .bind(scratch.workspace_id)
    .bind(scratch.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, 50);
    let oldest: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE workspace_id = $1 AND title = 'old-0'",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(oldest, 0);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn pruning_is_scoped_to_the_owner() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let other = scratch.add_actor(&pool, 20).await;

    // 51 conversations for the owner + 1 for the other member.
    for index in 0..51 {
        sqlx::query(
            "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at)              VALUES ($1, $2, $3, 'classic', $4, now() - make_interval(secs => $5), now() - make_interval(secs => $5))",
        )
        .bind(Uuid::new_v4())
        .bind(scratch.workspace_id)
        .bind(scratch.user_id)
        .bind(format!("mine-{index}"))
        .bind(5000 - index)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at)          VALUES ($1, $2, $3, 'classic', 'theirs', now() - interval '10 seconds', now() - interval '10 seconds')",
    )
    .bind(Uuid::new_v4())
    .bind(scratch.workspace_id)
    .bind(other)
    .execute(&pool)
    .await
    .unwrap();

    let (status, _) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);

    let mine: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE created_by_id = $1",
    )
    .bind(scratch.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let theirs: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE created_by_id = $1",
    )
    .bind(other)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mine, 50);
    assert_eq!(theirs, 1, "another member's conversations are untouched");

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_trims_and_truncates_the_optional_title() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let long_title = format!("  {}  ", "x".repeat(150));
    let (status, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic", "title": long_title})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"].as_str().unwrap().chars().count(), 120);
    assert!(!created["title"].as_str().unwrap().starts_with(' '));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn invalid_mode_is_rejected() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "turbo"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run (di `apps/api-rs`): `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: FAIL — module `ai_conversations` belum ada.

- [ ] **Step 3: Implementasi modul (bagian list + create + helper bersama)**

Buat `apps/api-rs/crates/api/src/routes/ai_conversations.rs`:

```rust
//! Rust-only Galileo chat history endpoints (no Django counterpart).
//!
//! `GET/POST /api/workspaces/:slug/ai-conversations/` — list + create;
//! `GET/PATCH/DELETE /:conversation_id/` — detail, rename, delete;
//! `GET /:conversation_id/messages/` — stored messages;
//! `PATCH /:conversation_id/messages/:message_id/` — schedule-decision
//! metadata. Reads and mutations are owner-only (404 otherwise).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::routes::ai_schedule::workspace_id_for_slug;
use crate::routes::module::guard_am;
use crate::routes::project::{deny, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

pub const MAX_CONVERSATIONS_PER_USER: i64 = 50;
pub const MAX_MESSAGES_PER_CONVERSATION: i64 = 200;
pub const TITLE_MAX_CHARS: usize = 60;
pub const RENAME_MAX_CHARS: usize = 120;

#[derive(serde::Deserialize)]
pub struct CreateConversationBody {
    pub mode: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ConversationRow {
    pub id: Uuid,
    pub mode: String,
    pub title: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct MessageRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub content: String,
    pub content_html: Option<String>,
    pub metadata: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub(crate) fn conversation_json(row: &ConversationRow) -> Value {
    json!({
        "id": row.id,
        "title": row.title,
        "mode": row.mode,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

pub(crate) fn message_json(row: &MessageRow) -> Value {
    json!({
        "id": row.id,
        "role": row.role,
        "content": row.content,
        "content_html": row.content_html,
        "metadata": row.metadata,
        "created_at": row.created_at,
    })
}

/// First line of the first user message, whitespace-collapsed, 60 chars max.
pub(crate) fn title_from(prompt: &str) -> String {
    let single_line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let collapsed = single_line.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(TITLE_MAX_CHARS).collect()
}

/// Load a conversation by workspace slug + id, scoped to its owner
/// (unknown slug, foreign user or missing row → `None`).
pub(crate) async fn load_owned_conversation(
    pool: &PgPool,
    slug: &str,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ConversationRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.id, c.mode, c.title, c.created_at, c.updated_at \
         FROM ai_conversations c \
         JOIN workspaces w ON w.id = c.workspace_id AND w.slug = $1 AND w.deleted_at IS NULL \
         WHERE c.id = $2 AND c.created_by_id = $3",
    )
    .bind(slug)
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// The newest `limit` messages, returned oldest-first for prompt building.
pub(crate) async fn recent_messages(
    pool: &PgPool,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    let mut rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT id, conversation_id, role, content, content_html, metadata, created_at \
         FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

/// Insert one message and return the stored row. Takes a connection so the
/// caller can run it inside a transaction (`&mut tx`) or on a pooled
/// connection (`&mut conn`).
pub(crate) async fn insert_message(
    conn: &mut sqlx::PgConnection,
    conversation_id: Uuid,
    role: &str,
    content: &str,
    content_html: Option<&str>,
    metadata: &Value,
) -> Result<MessageRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO ai_messages (id, conversation_id, role, content, content_html, metadata, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         RETURNING id, conversation_id, role, content, content_html, metadata, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(content_html)
    .bind(metadata)
    .fetch_one(conn)
    .await
}

/// Close one chat turn: fill the auto-title when empty, bump `updated_at`,
/// and prune messages beyond the per-conversation cap. Runs on the caller's
/// connection/transaction — the success path wraps this together with the
/// assistant `insert_message` in one transaction.
pub(crate) async fn finish_turn(
    conn: &mut sqlx::PgConnection,
    conversation_id: Uuid,
    title: &str,
) -> Result<ConversationRow, sqlx::Error> {
    let row: ConversationRow = sqlx::query_as(
        "UPDATE ai_conversations SET \
         title = CASE WHEN title = '' THEN $2 ELSE title END, updated_at = now() \
         WHERE id = $1 \
         RETURNING id, mode, title, created_at, updated_at",
    )
    .bind(conversation_id)
    .bind(title)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM ai_messages WHERE id IN ( \
         SELECT id FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at DESC, id DESC OFFSET $2)",
    )
    .bind(conversation_id)
    .bind(MAX_MESSAGES_PER_CONVERSATION)
    .execute(&mut *conn)
    .await?;
    Ok(row)
}

/// True when a write failed because the conversation disappeared mid-turn
/// (row deleted between load and write, or an insert hit the FK): callers map
/// this to a 404 instead of a 500.
pub(crate) fn conversation_gone(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::RowNotFound => true,
        sqlx::Error::Database(db) => db.code().as_deref() == Some("23503"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_collapses_whitespace_and_truncates() {
        assert_eq!(title_from("  buat laporan overdue  "), "buat laporan overdue");
        assert_eq!(title_from("first line\nsecond line"), "first line");
        assert_eq!(title_from("   \n\n  "), "");
        let long = "a".repeat(80);
        assert_eq!(title_from(&long).chars().count(), TITLE_MAX_CHARS);
    }
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok((StatusCode::OK, Json(json!({"conversations": []}))));
    };
    let rows: Vec<ConversationRow> = sqlx::query_as(
        "SELECT id, mode, title, created_at, updated_at \
         FROM ai_conversations WHERE workspace_id = $1 AND created_by_id = $2 \
         ORDER BY updated_at DESC, id DESC LIMIT $3",
    )
    .bind(workspace_id)
    .bind(auth.0)
    .bind(MAX_CONVERSATIONS_PER_USER)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "conversations": rows.iter().map(conversation_json).collect::<Vec<_>>(),
        })),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let body: CreateConversationBody = match serde_json::from_value(payload) {
        Ok(body) => body,
        Err(err) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid conversation payload: {err}")})),
            ));
        }
    };
    if body.mode != "classic" && body.mode != "agent" {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "mode must be 'classic' or 'agent'"})),
        ));
    }
    let title = body
        .title
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .chars()
        .take(RENAME_MAX_CHARS)
        .collect::<String>();
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let mut tx = st.pool.begin().await?;
    let row: ConversationRow = sqlx::query_as(
        "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now(), now()) \
         RETURNING id, mode, title, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id)
    .bind(auth.0)
    .bind(&body.mode)
    .bind(&title)
    .fetch_one(&mut tx)
    .await?;
    sqlx::query(
        "DELETE FROM ai_conversations WHERE id IN ( \
         SELECT id FROM ai_conversations WHERE workspace_id = $1 AND created_by_id = $2 \
         ORDER BY updated_at DESC, id DESC OFFSET $3)",
    )
    .bind(workspace_id)
    .bind(auth.0)
    .bind(MAX_CONVERSATIONS_PER_USER)
    .execute(&mut tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(conversation_json(&row))))
}
```

- [ ] **Step 4: Daftarkan modul + route**

`apps/api-rs/crates/api/src/routes/mod.rs` — tambah setelah `pub mod ai_agent;`:

```rust
pub mod ai_conversations;
```

`apps/api-rs/crates/api/src/main.rs` — tambah setelah route `ai-agent` (blok komentar + `.route(...)`):

```rust
        // Rust-only (no Django counterpart; consumed by the web app's Galileo
        // sidebar): Galileo chat history. List/create conversations; owner-only
        // reads and mutations (404 for foreign rows). Gate WORKSPACE
        // ADMIN/MEMBER.
        .route(
            "/api/workspaces/:slug/ai-conversations/",
            get(routes::ai_conversations::list).post(routes::ai_conversations::create),
        )
```

Catatan: `detail`, `patch`, `destroy`, `messages`, `patch_message` ditambahkan di Task 4–5; untuk sekarang cukup list+create.

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run (di `apps/api-rs`): `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: `5 passed` (integration tests; +1 unit test in `-p api --lib ai_conversations`).

- [ ] **Step 6: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git commit -m "feat(api): add conversation list and create endpoints"
```

---

### Task 4: Detail, rename, delete percakapan

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_conversations_test.rs`

- [ ] **Step 1: Perluas test (tambahkan di akhir file test)**

```rust
#[tokio::test]
async fn detail_rename_delete_are_owner_scoped_and_validated() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let other = scratch.add_actor(&pool, 20).await;

    let (_, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // Foreign user sees nothing: detail, rename and delete are all 404.
    let (status, _) = ai_conversations::detail(
        State(st.clone()),
        AuthUser(other),
        Path((scratch.slug.clone(), id)),
    )
    .await
    .expect("foreign detail");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = ai_conversations::patch(
        State(st.clone()),
        AuthUser(other),
        Path((scratch.slug.clone(), id)),
        Json(json!({"title": "hijack"})),
    )
    .await
    .expect("foreign rename");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = ai_conversations::destroy(
        State(st.clone()),
        AuthUser(other),
        Path((scratch.slug.clone(), id)),
    )
    .await
    .expect("foreign delete");
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Owner can rename (trimmed) and read it back.
    let (status, Json(renamed)) = ai_conversations::patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
        Json(json!({"title": "  My chat  "})),
    )
    .await
    .expect("rename");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["title"], json!("My chat"));
    let (_, Json(detail)) = ai_conversations::detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
    )
    .await
    .expect("detail");
    assert_eq!(detail["title"], json!("My chat"));

    // Blank and over-long titles are rejected.
    let (status, _) = ai_conversations::patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
        Json(json!({"title": "   "})),
    )
    .await
    .expect("blank title");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let long_title = "x".repeat(121);
    let (status, _) = ai_conversations::patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
        Json(json!({"title": long_title})),
    )
    .await
    .expect("long title");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Unknown id → 404 for every verb (detail, rename, delete).
    let unknown = Uuid::new_v4();
    let (status, _) = ai_conversations::detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), unknown)),
    )
    .await
    .expect("missing detail");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = ai_conversations::patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), unknown)),
        Json(json!({"title": "ghost"})),
    )
    .await
    .expect("missing rename");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = ai_conversations::destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), unknown)),
    )
    .await
    .expect("missing delete");
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 120 chars is allowed (boundary) and the rename bumps `updated_at`.
    let boundary = "x".repeat(120);
    let (status, Json(boundary_row)) = ai_conversations::patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
        Json(json!({"title": boundary})),
    )
    .await
    .expect("boundary rename");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(boundary_row["title"].as_str().unwrap().chars().count(), 120);
    assert!(boundary_row["updated_at"].as_str().unwrap() >= created["updated_at"].as_str().unwrap());

    // Owner delete removes the row.
    let (status, _) = ai_conversations::destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_conversations WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn deleting_a_conversation_cascades_its_messages() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (_, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "agent"})),
    )
    .await
    .expect("create");
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    // Seed one message with raw SQL: `insert_message` is `pub(crate)` and not
    // visible from this integration-test crate.
    sqlx::query(
        "INSERT INTO ai_messages (id, conversation_id, role, content, metadata, created_at) \
         VALUES ($1, $2, 'user', 'hi', '{}'::jsonb, now())",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .execute(&pool)
    .await
    .expect("seed message");

    let (status, _) = ai_conversations::destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), id)),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_messages WHERE conversation_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: FAIL — `ai_conversations::detail` / `patch` / `destroy` belum ada.

- [ ] **Step 3: Implementasi handler**

Tambahkan di `ai_conversations.rs`:

```rust
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(conversation_json(&row))))
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await? else {
        return Ok(missing());
    };
    let title = body
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if title.is_empty() || title.chars().count() > RENAME_MAX_CHARS {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("title must be 1-{RENAME_MAX_CHARS} characters")})),
        ));
    }
    // Owner-scoped UPDATE + fetch_optional: if the row is deleted between the
    // load above and this statement (other tab / 50-cap prune), answer 404
    // instead of a 500 from `fetch_one`.
    let row: Option<ConversationRow> = sqlx::query_as(
        "UPDATE ai_conversations SET title = $2, updated_at = now() \
         WHERE id = $1 AND created_by_id = $3 \
         RETURNING id, mode, title, created_at, updated_at",
    )
    .bind(conversation_id)
    .bind(title)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(row) => Ok((StatusCode::OK, Json(conversation_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await? else {
        return Ok(missing());
    };
    let deleted = sqlx::query("DELETE FROM ai_conversations WHERE id = $1 AND created_by_id = $2")
        .bind(conversation_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

- [ ] **Step 4: Daftarkan route**

`main.rs`, setelah route list/create dari Task 3:

```rust
        // Rust-only: conversation detail + rename + hard delete (cascade
        // messages). Owner-only.
        .route(
            "/api/workspaces/:slug/ai-conversations/:conversation_id/",
            get(routes::ai_conversations::detail)
                .patch(routes::ai_conversations::patch)
                .delete(routes::ai_conversations::destroy),
        )
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: `7 passed`.

- [ ] **Step 6: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git commit -m "feat(api): add conversation detail, rename and delete"
```

---

### Task 5: Endpoint pesan + patch metadata keputusan jadwal

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_conversations_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

```rust
#[tokio::test]
async fn messages_are_listed_oldest_first_and_metadata_patch_is_allowlisted() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let other = scratch.add_actor(&pool, 20).await;

    let (_, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "agent"})),
    )
    .await
    .expect("create");
    let conversation_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    // Seed with raw SQL: the `insert_message` helper is `pub(crate)`.
    let user_message_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_messages (id, conversation_id, role, content, metadata, created_at) \
         VALUES ($1, $2, 'user', 'buat laporan', '{}'::jsonb, now() - interval '2 seconds')",
    )
    .bind(user_message_id)
    .bind(conversation_id)
    .execute(&pool)
    .await
    .expect("user message");
    let assistant_message_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_messages (id, conversation_id, role, content, content_html, metadata, created_at) \
         VALUES ($1, $2, 'assistant', 'siap', 'siap', $3::jsonb, now() - interval '1 second')",
    )
    .bind(assistant_message_id)
    .bind(conversation_id)
    .bind(json!({
        "schedule_proposal": {"name": "Laporan", "frequency": "weekly"},
        "schedule_proposal_key": Uuid::new_v4(),
        "schedule_decision": "pending",
    }))
    .execute(&pool)
    .await
    .expect("assistant message");

    let (status, Json(list)) = ai_conversations::messages(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id)),
    )
    .await
    .expect("messages");
    assert_eq!(status, StatusCode::OK);
    let messages = list["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["id"], json!(user_message_id));
    assert_eq!(messages[1]["metadata"]["schedule_decision"], json!("pending"));

    // Unknown keys are rejected.
    let (status, _) = ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, assistant_message_id)),
        Json(json!({"metadata": {"prompt": "hack"}})),
    )
    .await
    .expect("bad key");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Every other rejection branch: non-object, empty, bad decision, bad uuid.
    for bad in [
        json!({"metadata": "x"}),
        json!({"metadata": {}}),
        json!({"metadata": {"schedule_decision": "pending"}}),
        json!({"metadata": {"created_schedule_id": "not-a-uuid"}}),
    ] {
        let (status, _) = ai_conversations::patch_message(
            State(st.clone()),
            AuthUser(scratch.user_id),
            Path((scratch.slug.clone(), conversation_id, assistant_message_id)),
            Json(bad.clone()),
        )
        .await
        .expect("bad patch");
        assert_eq!(status, StatusCode::BAD_REQUEST, "payload: {bad}");
    }

    // A message from another conversation of the same owner → 404 (the
    // `conversation_id = $2` predicate must scope the update).
    let (_, Json(other_conversation)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "agent"})),
    )
    .await
    .expect("second conversation");
    let other_conversation_id =
        Uuid::parse_str(other_conversation["id"].as_str().unwrap()).unwrap();
    let (status, _) = ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((
            scratch.slug.clone(),
            other_conversation_id,
            assistant_message_id,
        )),
        Json(json!({"metadata": {"schedule_decision": "cancelled"}})),
    )
    .await
    .expect("cross-conversation patch");
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Rejections never partially applied: metadata is still pending.
    let (_, Json(still)) = ai_conversations::messages(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id)),
    )
    .await
    .expect("messages again");
    assert_eq!(
        still["messages"][1]["metadata"]["schedule_decision"],
        json!("pending")
    );

    // A valid decision merges into the existing metadata.
    let schedule_id = Uuid::new_v4();
    let (status, Json(updated)) = ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, assistant_message_id)),
        Json(json!({"metadata": {
            "schedule_decision": "created",
            "created_schedule_id": schedule_id,
        }})),
    )
    .await
    .expect("patch metadata");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["metadata"]["schedule_decision"], json!("created"));
    assert_eq!(updated["metadata"]["created_schedule_id"], json!(schedule_id));
    assert_eq!(updated["metadata"]["schedule_proposal"]["name"], json!("Laporan"));

    // Foreign user cannot read or patch.
    let (status, _) = ai_conversations::messages(
        State(st.clone()),
        AuthUser(other),
        Path((scratch.slug.clone(), conversation_id)),
    )
    .await
    .expect("foreign messages");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(other),
        Path((scratch.slug.clone(), conversation_id, assistant_message_id)),
        Json(json!({"metadata": {"schedule_decision": "cancelled"}})),
    )
    .await
    .expect("foreign patch");
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Unknown message id inside an owned conversation → 404.
    let (status, _) = ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, Uuid::new_v4())),
        Json(json!({"metadata": {"schedule_decision": "cancelled"}})),
    )
    .await
    .expect("missing message");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: FAIL — `ai_conversations::messages` / `patch_message` belum ada.

- [ ] **Step 3: Implementasi handler**

Tambahkan di `ai_conversations.rs`:

```rust
pub async fn messages(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await? else {
        return Ok(missing());
    };
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT id, conversation_id, role, content, content_html, metadata, created_at \
         FROM ai_messages WHERE conversation_id = $1 ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "messages": rows.iter().map(message_json).collect::<Vec<_>>(),
        })),
    ))
}

/// `PATCH .../messages/:message_id/` — merge an allowlisted metadata patch
/// (`schedule_decision`, `created_schedule_id`) written by the FE when the
/// user resolves a schedule proposal card.
pub async fn patch_message(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id, message_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await? else {
        return Ok(missing());
    };
    let Some(patch) = body.get("metadata").and_then(Value::as_object) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "metadata must be an object"})),
        ));
    };
    let mut clean = serde_json::Map::new();
    for (key, value) in patch {
        match key.as_str() {
            "schedule_decision" => match value.as_str() {
                Some("created") | Some("cancelled") => {
                    clean.insert(key.clone(), value.clone());
                }
                _ => {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "schedule_decision must be 'created' or 'cancelled'"})),
                    ));
                }
            },
            "created_schedule_id" => match value.as_str().and_then(|raw| Uuid::parse_str(raw).ok()) {
                Some(id) => {
                    clean.insert(key.clone(), json!(id));
                }
                None => {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "created_schedule_id must be a uuid"})),
                    ));
                }
            },
            _ => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("metadata key not allowed: {key}")})),
                ));
            }
        }
    }
    if clean.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "metadata patch is empty"})),
        ));
    }
    let row: Option<MessageRow> = sqlx::query_as(
        "UPDATE ai_messages SET metadata = metadata || $3::jsonb \
         WHERE id = $1 AND conversation_id = $2 \
         RETURNING id, conversation_id, role, content, content_html, metadata, created_at",
    )
    .bind(message_id)
    .bind(conversation_id)
    .bind(Value::Object(clean))
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(row) => Ok((StatusCode::OK, Json(message_json(&row)))),
        None => Ok(missing()),
    }
}
```

- [ ] **Step 4: Daftarkan route**

`main.rs`, setelah route detail:

```rust
        // Rust-only: stored messages of a conversation + allowlisted metadata
        // patch for schedule-proposal decisions. Owner-only.
        .route(
            "/api/workspaces/:slug/ai-conversations/:conversation_id/messages/",
            get(routes::ai_conversations::messages),
        )
        .route(
            "/api/workspaces/:slug/ai-conversations/:conversation_id/messages/:message_id/",
            patch(routes::ai_conversations::patch_message),
        )
```

Pastikan `patch` ada di import `axum::routing` yang sudah dipakai `main.rs` (sudah ada untuk route lain).

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: `8 passed`.

- [ ] **Step 6: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git commit -m "feat(api): add conversation messages and metadata patch"
```

---

### Task 6: `/ai-agent/` stateful — persistensi satu giliran + konteks 8 pesan

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tulis test integrasi yang gagal**

Tambahkan di `apps/api-rs/crates/api/tests/ai_agent_test.rs` (file ini saat ini tanpa DB; tambahkan harness DB di bawah, meniru `ai_conversations_test.rs` — salin `database_url`, `pool`, `state`, `insert_user`, `Scratch` dengan prefix slug `aia-`; `purge` menghapus `ai_conversations` + workspace + users):

```rust
/// Fake OpenAI-compatible upstream that records every request body.
async fn spawn_recording_upstream() -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::{routing::post, Json, Router};
    let bodies: std::sync::Arc<std::sync::Mutex<Vec<Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = bodies.clone();
    async fn handler(
        axum::extract::State(recorder): axum::extract::State<
            std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
        >,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        recorder.lock().unwrap().push(body);
        (
            axum::http::StatusCode::OK,
            Json(json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "agent answer"},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handler))
                .with_state(recorder),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}/v1"), bodies)
}

fn set_llm_env(base_url: &str) {
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", base_url);
    std::env::set_var("LLM_MODEL", "test-model");
}

fn clear_llm_env() {
    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");
}

async fn create_conversation(
    st: &AppState,
    slug: &str,
    user_id: Uuid,
    mode: &str,
) -> Uuid {
    let (_, Json(created)) = api::routes::ai_conversations::create(
        State(st.clone()),
        AuthUser(user_id),
        Path(slug.to_string()),
        Json(json!({"mode": mode})),
    )
    .await
    .expect("create conversation");
    Uuid::parse_str(created["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn agent_turn_persists_both_messages_and_builds_context_from_history() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // Seed 10 older messages so the 8-message window has something to drop.
    // Raw SQL because `insert_message` is `pub(crate)`.
    for index in 0..10 {
        sqlx::query(
            "INSERT INTO ai_messages (id, conversation_id, role, content, metadata, created_at) \
             VALUES ($1, $2, $3, $4, '{}'::jsonb, now() - make_interval(secs => $5))",
        )
        .bind(Uuid::new_v4())
        .bind(conversation_id)
        .bind(if index % 2 == 0 { "user" } else { "assistant" })
        .bind(format!("seed-{index}"))
        .bind(100 - index)
        .execute(&pool)
        .await
        .expect("seed");
    }

    let (base_url, bodies) = spawn_recording_upstream().await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "how many items?",
            "context": "Work item context:\nWork item: X",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["response"], json!("agent answer"));
    assert_eq!(body["conversation"]["id"], json!(conversation_id));
    assert_eq!(body["conversation"]["title"], json!("how many items?"));
    assert_eq!(body["user_message"]["role"], json!("user"));
    assert_eq!(body["user_message"]["content"], json!("how many items?"));
    assert_eq!(body["assistant_message"]["role"], json!("assistant"));
    assert_eq!(body["assistant_message"]["content"], json!("agent answer"));

    // The upstream saw the composed prompt: context + newest 8 + question.
    // Rig sends the agent preamble as messages[0], so find the user message
    // by role instead of by index.
    let sent = bodies.lock().unwrap().clone();
    let content = sent[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == json!("user"))
        .and_then(|message| message["content"].as_str())
        .unwrap();
    assert!(content.contains("Work item context:"));
    assert!(content.contains("seed-2"), "oldest two of ten are dropped");
    assert!(!content.contains("seed-0"));
    assert!(!content.contains("seed-1"));
    assert!(content.contains("User's new question: how many items?"));
    assert_eq!(
        content.matches("how many items?").count(),
        1,
        "history is loaded before the user insert, so the question appears once"
    );

    // DB: user + assistant rows persisted, conversation title filled.
    let stored: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 12);
    assert_eq!(stored[10].0, "user");
    assert_eq!(stored[10].1, "how many items?");
    assert_eq!(stored[11].0, "assistant");
    assert_eq!(stored[11].1, "agent answer");
    let title: String = sqlx::query_scalar("SELECT title FROM ai_conversations WHERE id = $1")
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "how many items?");

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn agent_requires_a_conversation_and_matching_mode() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Missing conversation_id → 400 (checked before any LLM config use).
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi"})),
    )
    .await
    .expect("missing conversation");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let classic = create_conversation(&st, &scratch.slug, scratch.user_id, "classic").await;
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi", "conversation_id": classic})),
    )
    .await
    .expect("mode mismatch");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let other = scratch.add_actor(&pool, 20).await;
    let agent = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(other),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi", "conversation_id": agent})),
    )
    .await
    .expect("foreign conversation");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn agent_failure_stores_an_error_message() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // Point at a dead upstream so the agent fails.
    set_llm_env("http://127.0.0.1:1/v1");
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi", "conversation_id": conversation_id})),
    )
    .await
    .expect("agent failure");
    clear_llm_env();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let stored: Vec<(String, Value)> = sqlx::query_as(
        "SELECT role, metadata FROM ai_messages WHERE conversation_id = $1 ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].0, "user");
    assert_eq!(stored[1].0, "assistant");
    assert_eq!(stored[1].1["is_error"], json!(true));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn agent_proposal_metadata_is_persisted_and_returned() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // Reuse the existing tool-call fake from `mod tool_roundtrip` (add the
    // `schedule_roundtrip_url()` helper there — see below).
    let base_url = tool_roundtrip::schedule_roundtrip_url().await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "/schedule daily report",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pending_action"]["kind"], json!("create_schedule"));

    let metadata = &body["assistant_message"]["metadata"];
    assert_eq!(metadata["is_error"], json!(false));
    assert_eq!(metadata["schedule_decision"], json!("pending"));
    assert_eq!(metadata["schedule_proposal"]["frequency"], json!("daily"));
    assert!(metadata["schedule_proposal_key"].as_str().is_some());

    let stored: Value = sqlx::query_scalar(
        "SELECT metadata FROM ai_messages WHERE conversation_id = $1 AND role = 'assistant'",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["schedule_decision"], json!("pending"));
    assert_eq!(stored["schedule_proposal"]["name"], json!("Daily"));

    scratch.purge(&pool).await;
}
```

Tambahkan helper berikut di dalam `mod tool_roundtrip` yang sudah ada di `ai_agent_test.rs` (jangan mengubah test lama di dalamnya):

```rust
    /// URL only — for DB-backed tests that don't need the upstream handle.
    pub(super) async fn schedule_roundtrip_url() -> String {
        spawn_schedule_roundtrip().await.0
    }
```

Catatan: test-test ini mengubah env LLM → wajib `--test-threads=1`; tambahkan komentar itu di header file.

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: FAIL — handler belum menerima `conversation_id` dan respons belum punya `conversation`/`user_message`/`assistant_message`.

- [ ] **Step 3: Implementasi handler agent**

Ubah `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`. Tambah import:

```rust
use crate::routes::ai_conversations::{
    conversation_gone, finish_turn, insert_message, load_owned_conversation, message_json,
    recent_messages, title_from, ConversationRow, MessageRow,
};
use ai::agent::{history_prompt, HistoryMessage, HISTORY_MESSAGE_LIMIT};
```

Ganti isi `workspace_ai_agent` mulai dari validasi prompt.

**Urutan validasi penting:** pindahkan blok `let cfg = resolve_llm_config(...)` + cek `cfg.api_key.is_empty() || cfg.model.is_empty()` ke **setelah** validasi `conversation_id`/percakapan/mode. Alasan: permintaan tanpa `conversation_id` atau mode salah harus 400/404 karena alasan itu, bukan karena konfigurasi LLM — test di Task 6 mengandalkan urutan ini (tanpa env LLM, mismatch mode tetap 400 dari cek mode).

```rust
    let Some(prompt) = prompt_from_body(&body) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Prompt is required"})),
        ));
    };
    let Some(conversation_id) = body
        .get("conversation_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "conversation_id is required"})),
        ));
    };
    let Some(conversation) =
        load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    if conversation.mode != "agent" {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "conversation mode does not match this endpoint"})),
        ));
    }
    // Dipindah dari atas fungsi (lihat catatan urutan validasi di atas).
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let task = task_from_body(&body);
    let context = body.get("context").and_then(Value::as_str).unwrap_or("");
    let history_rows = recent_messages(&st.pool, conversation_id, HISTORY_MESSAGE_LIMIT as i64).await?;
    let history: Vec<HistoryMessage> = history_rows
        .iter()
        .map(|row| HistoryMessage {
            role: row.role.clone(),
            content: row.content.clone(),
        })
        .collect();
    let model_prompt = history_prompt(context, &history, prompt);
    let title = title_from(prompt);
    // Insert the user message on a pooled connection, then release it before
    // the (up to 180s) LLM call so the pool is not held. A conversation that
    // vanished since the load still answers 404, not 500.
    let mut conn = st.pool.acquire().await?;
    let user_message =
        match insert_message(&mut conn, conversation_id, "user", prompt, None, &json!({})).await {
            Ok(message) => message,
            Err(error) if conversation_gone(&error) => return Ok(missing()),
            Err(error) => return Err(error.into()),
        };
    drop(conn);
    let trace = new_trace();
    let tool_server = tools::workspace_tools(st.pool.clone(), workspace_id, trace.clone());
    let agent_result = tokio::time::timeout(
        AGENT_TIMEOUT,
        run_agent(
            &cfg.base_url,
            &cfg.api_key,
            &cfg.model,
            tool_server,
            task,
            &model_prompt,
        ),
    )
    .await
    .unwrap_or_else(|_| {
        tracing::warn!("ai-agent: request timed out");
        Err(LlmError::Upstream)
    });
    match agent_result {
        Ok(text) => {
            let tool_calls: Vec<Value> = trace
                .lock()
                .map(|recorded| {
                    recorded
                        .iter()
                        .map(|call| json!({"name": call.name, "arguments": call.arguments}))
                        .collect()
                })
                .unwrap_or_default();
            let action = pending_action(&trace);
            let mut metadata = json!({ "is_error": false });
            if let Some(action) = action.as_ref() {
                metadata["schedule_proposal"] = action["proposal"].clone();
                metadata["schedule_proposal_key"] = json!(Uuid::new_v4());
                metadata["schedule_decision"] = json!("pending");
            }
            // Assistant message + prune + updated_at in ONE transaction. A
            // conversation deleted mid-turn (row gone / FK violation) → 404.
            let turn = async {
                let mut tx = st.pool.begin().await?;
                let assistant_message = insert_message(
                    &mut tx,
                    conversation_id,
                    "assistant",
                    &text,
                    Some(&crate::routes::ai::response_html(&text)),
                    &metadata,
                )
                .await?;
                let conversation = finish_turn(&mut tx, conversation_id, &title).await?;
                tx.commit().await?;
                Ok::<_, sqlx::Error>((assistant_message, conversation))
            }
            .await;
            let (assistant_message, conversation) = match turn {
                Ok(ok) => ok,
                Err(error) if conversation_gone(&error) => return Ok(missing()),
                Err(error) => return Err(error.into()),
            };
            Ok((
                StatusCode::OK,
                Json(chat_success_body(
                    &text,
                    tool_calls,
                    action,
                    &conversation,
                    &user_message,
                    &assistant_message,
                )),
            ))
        }
        Err(error) => {
            let message = match error {
                LlmError::RateLimited => format!("Rate limit exceeded for {}", host_of(&cfg.base_url)),
                LlmError::Upstream => "An internal error has occurred.".to_string(),
            };
            if let Ok(mut tx) = st.pool.begin().await {
                if let Err(error) = insert_message(
                    &mut tx,
                    conversation_id,
                    "assistant",
                    &message,
                    None,
                    &json!({ "is_error": true }),
                )
                .await
                {
                    tracing::warn!(
                        error=%error,
                        conversation_id=%conversation_id,
                        "failed to persist error turn"
                    );
                } else if let Err(error) = finish_turn(&mut tx, conversation_id, &title).await {
                    tracing::warn!(
                        error=%error,
                        conversation_id=%conversation_id,
                        "failed to finish error turn"
                    );
                }
                let _ = tx.commit().await;
            }
            match error {
                LlmError::RateLimited => Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": message})),
                )),
                LlmError::Upstream => Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": message})),
                )),
            }
        }
    }
```

Tambahkan helper di modul yang sama:

```rust
/// 200 body: the existing chat fields plus the persisted rows so the FE can
/// reconcile its optimistic bubble and refresh the conversation list.
pub(crate) fn chat_success_body(
    text: &str,
    tool_calls: Vec<Value>,
    action: Option<Value>,
    conversation: &ConversationRow,
    user_message: &MessageRow,
    assistant_message: &MessageRow,
) -> Value {
    let mut body = success_body(text, tool_calls, action);
    body["conversation"] = crate::routes::ai_conversations::conversation_json(conversation);
    body["user_message"] = message_json(user_message);
    body["assistant_message"] = message_json(assistant_message);
    body
}
```

Tambahkan `missing` ke import dari `crate::routes::project` (sekarang hanya `deny, ws_role`).

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: semua lulus (test lama `prompt_from_body_rules`, `effective_prompt_folds_task_like_django`, `success_body_*`, `record_appends_and_serializes` + 3 test baru).

- [ ] **Step 5: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api): persist agent chat turns with server-built context"
```

---

### Task 7: `/ai-assistant/` stateful (mode classic)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_conversations_test.rs` (tambah test classic) atau `apps/api-rs/crates/api/tests/ai_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `apps/api-rs/crates/api/tests/ai_conversations_test.rs` (harness sudah ada di file ini):

```rust
#[tokio::test]
async fn classic_turn_persists_and_requires_conversation_id() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Without conversation_id → 400.
    let (status, _) = api::routes::ai::workspace_ai_assistant(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "say hi", "prompt": "hi"})),
    )
    .await
    .expect("missing conversation");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Fake upstream (recording) for the happy path. These tests mutate the
    // process env, so the file must run with `--test-threads=1`.
    let (base_url, bodies) = crate::support::spawn_recording_upstream("classic answer").await;
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", &base_url);
    std::env::set_var("LLM_MODEL", "test-model");

    let (_, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    let conversation_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // Mode mismatch → 400 (classic endpoint must not write agent threads).
    let (_, Json(agent_conversation)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "agent"})),
    )
    .await
    .expect("agent conversation");
    let agent_conversation_id = Uuid::parse_str(agent_conversation["id"].as_str().unwrap()).unwrap();
    let (status, _) = api::routes::ai::workspace_ai_assistant(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "say hi",
            "prompt": "hi",
            "conversation_id": agent_conversation_id,
        })),
    )
    .await
    .expect("mode mismatch");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Foreign user → 404.
    let other = scratch.add_actor(&pool, 20).await;
    let (status, _) = api::routes::ai::workspace_ai_assistant(
        State(st.clone()),
        AuthUser(other),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "say hi",
            "prompt": "hi",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("foreign conversation");
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, Json(body)) = api::routes::ai::workspace_ai_assistant(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "say hi",
            "prompt": "hello classic",
            "context": "No active work item context.",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("classic call");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["response"], json!("classic answer"));
    assert_eq!(body["conversation"]["mode"], json!("classic"));
    assert_eq!(body["conversation"]["title"], json!("hello classic"));
    assert_eq!(body["user_message"]["content"], json!("hello classic"));
    assert_eq!(body["assistant_message"]["content"], json!("classic answer"));

    // DB rows persisted exactly once per side.
    let stored: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0], ("user".to_string(), "hello classic".to_string()));
    assert_eq!(stored[1], ("assistant".to_string(), "classic answer".to_string()));

    let sent = bodies.lock().unwrap().clone();
    let content = sent[0]["messages"][0]["content"].as_str().unwrap();
    assert!(content.contains("say hi"));
    assert!(content.contains("User's new question: hello classic"));

    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn classic_turn_prunes_messages_beyond_two_hundred() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let (base_url, _bodies) = crate::support::spawn_recording_upstream("classic answer").await;
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", &base_url);
    std::env::set_var("LLM_MODEL", "test-model");

    let (_, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    let conversation_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // 205 stored messages + the new turn (user + assistant) = 207 → prune to 200.
    for index in 0..205 {
        sqlx::query(
            "INSERT INTO ai_messages (id, conversation_id, role, content, metadata, created_at) \
             VALUES ($1, $2, 'user', $3, '{}'::jsonb, now() - make_interval(secs => $4))",
        )
        .bind(Uuid::new_v4())
        .bind(conversation_id)
        .bind(format!("seed-{index}"))
        .bind(1000 - index)
        .execute(&pool)
        .await
        .expect("seed");
    }

    let (status, _) = api::routes::ai::workspace_ai_assistant(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "say hi",
            "prompt": "the newest question",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("classic call");
    assert_eq!(status, StatusCode::OK);

    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_messages WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 200, "retention keeps the newest 200 messages");
    let oldest: Option<String> = sqlx::query_scalar(
        "SELECT content FROM ai_messages WHERE conversation_id = $1 ORDER BY created_at, id LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        oldest.as_deref(),
        Some("seed-7"),
        "205 seeds + 2 new rows pruned to 200 removes seed-0..seed-6"
    );
    let newest: Option<String> = sqlx::query_scalar(
        "SELECT content FROM ai_messages WHERE conversation_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(newest.as_deref(), Some("classic answer"));

    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");

    scratch.purge(&pool).await;
}
```

Karena helper `spawn_recording_upstream` dipakai dua file test, pindahkan ke `apps/api-rs/crates/api/tests/support/mod.rs`:

```rust
//! Shared test helpers for the API integration tests.

use serde_json::Value;

/// Fake OpenAI-compatible upstream that records every request body.
pub async fn spawn_recording_upstream() -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::{routing::post, Json, Router};
    let bodies: std::sync::Arc<std::sync::Mutex<Vec<Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = bodies.clone();
    async fn handler(
        axum::extract::State(recorder): axum::extract::State<
            std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
        >,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        recorder.lock().unwrap().push(body);
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "classic answer"},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handler))
                .with_state(recorder),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}/v1"), bodies)
}
```

Lalu di `ai_agent_test.rs` dan `ai_conversations_test.rs` tambahkan di atas:

```rust
#[path = "support/mod.rs"]
mod support;
```

**Versi final helper memakai parameter `answer: &str`** (agar satu helper melayani dua file):

```rust
pub async fn spawn_recording_upstream(
    answer: &str,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::{routing::post, Json, Router};
    let bodies: std::sync::Arc<std::sync::Mutex<Vec<Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = bodies.clone();
    let answer = answer.to_string();
    async fn handler(
        axum::extract::State((recorder, answer)): axum::extract::State<(
            std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
            String,
        )>,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        recorder.lock().unwrap().push(body);
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": answer},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handler))
                .with_state((recorder, answer)),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}/v1"), bodies)
}
```

Pemanggil: test agent → `support::spawn_recording_upstream("agent answer")`; test classic → `support::spawn_recording_upstream("classic answer")`. Hapus salinan lokal helper di `ai_agent_test.rs` (Task 6) dan ganti pemanggilannya ke `support::…`.

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: FAIL — `workspace_ai_assistant` belum menerima `conversation_id`.

- [ ] **Step 3: Implementasi handler classic**

Ubah `apps/api-rs/crates/api/src/routes/ai.rs`. Tambah import:

```rust
use crate::routes::ai_agent::chat_success_body;
use crate::routes::ai_conversations::{
    conversation_gone, finish_turn, insert_message, load_owned_conversation, recent_messages,
    title_from,
};
use crate::routes::project::missing;
use ai::agent::{history_prompt, HistoryMessage, HISTORY_MESSAGE_LIMIT};
```

Ganti isi `workspace_ai_assistant`.

**Urutan validasi penting (sama seperti agent):** pindahkan blok `let cfg = resolve_llm_config(...)` + cek config ke **setelah** validasi `conversation_id`/percakapan/mode.

```rust
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(task) = task_from_body(&body) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Task is required"})),
        ));
    };
    let Some(conversation_id) = body
        .get("conversation_id")
        .and_then(Value::as_str)
        .and_then(|raw| uuid::Uuid::parse_str(raw).ok())
    else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "conversation_id is required"})),
        ));
    };
    let Some(conversation) =
        load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    if conversation.mode != "classic" {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "conversation mode does not match this endpoint"})),
        ));
    }
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let prompt = body.get("prompt").and_then(Value::as_str).unwrap_or("");
    let context = body.get("context").and_then(Value::as_str).unwrap_or("");
    let history_rows = recent_messages(&st.pool, conversation_id, HISTORY_MESSAGE_LIMIT as i64).await?;
    let history: Vec<HistoryMessage> = history_rows
        .iter()
        .map(|row| HistoryMessage {
            role: row.role.clone(),
            content: row.content.clone(),
        })
        .collect();
    let model_prompt = history_prompt(context, &history, prompt);
    let title = title_from(prompt);
    // Insert the user message on a pooled connection, then release it before
    // the LLM call so the pool is not held. A conversation that vanished since
    // the load still answers 404, not 500.
    let mut conn = st.pool.acquire().await?;
    let user_message =
        match insert_message(&mut conn, conversation_id, "user", prompt, None, &json!({})).await {
            Ok(message) => message,
            Err(error) if conversation_gone(&error) => return Ok(missing()),
            Err(error) => return Err(error.into()),
        };
    drop(conn);
    match chat_completion(&cfg.base_url, &cfg.api_key, &cfg.model, task, &model_prompt).await {
        Ok(text) => {
            let html = response_html(&text);
            // Assistant message + prune + updated_at in ONE transaction. A
            // conversation deleted mid-turn (row gone / FK violation) → 404.
            let turn = async {
                let mut tx = st.pool.begin().await?;
                let assistant_message = insert_message(
                    &mut tx,
                    conversation_id,
                    "assistant",
                    &text,
                    Some(&html),
                    &json!({ "is_error": false }),
                )
                .await?;
                let conversation = finish_turn(&mut tx, conversation_id, &title).await?;
                tx.commit().await?;
                Ok::<_, sqlx::Error>((assistant_message, conversation))
            }
            .await;
            let (assistant_message, conversation) = match turn {
                Ok(ok) => ok,
                Err(error) if conversation_gone(&error) => return Ok(missing()),
                Err(error) => return Err(error.into()),
            };
            Ok((
                StatusCode::OK,
                Json(chat_success_body(
                    &text,
                    Vec::new(),
                    None,
                    &conversation,
                    &user_message,
                    &assistant_message,
                )),
            ))
        }
        Err(error) => {
            let message = match error {
                LlmError::RateLimited => format!("Rate limit exceeded for {}", host_of(&cfg.base_url)),
                LlmError::Upstream => "An internal error has occurred.".to_string(),
            };
            if let Ok(mut tx) = st.pool.begin().await {
                if let Err(error) = insert_message(
                    &mut tx,
                    conversation_id,
                    "assistant",
                    &message,
                    None,
                    &json!({ "is_error": true }),
                )
                .await
                {
                    tracing::warn!(
                        error=%error,
                        conversation_id=%conversation_id,
                        "failed to persist error turn"
                    );
                } else if let Err(error) = finish_turn(&mut tx, conversation_id, &title).await {
                    tracing::warn!(
                        error=%error,
                        conversation_id=%conversation_id,
                        "failed to finish error turn"
                    );
                }
                let _ = tx.commit().await;
            }
            match error {
                LlmError::RateLimited => Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": message})),
                )),
                LlmError::Upstream => Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": message})),
                )),
            }
        }
    }
```

Catatan: `uuid::Uuid` perlu ditambah sebagai dependency `api` (sudah dipakai di crate ini — cek `Cargo.toml`; kalau sudah ada, cukup `use uuid::Uuid;`).

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1 && cargo test -p api --test ai_agent_test -- --test-threads=1 && cargo test -p api --test ai_test`
Expected: semua lulus.

- [ ] **Step 5: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs apps/api-rs/crates/api/tests/support/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git add apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs apps/api-rs/crates/api/tests/support/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api): persist classic chat turns with server-built context"
```

---

---

### Task 7b: Endpoint stateless `/ai-complete/` untuk permukaan editor

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_conversations_test.rs`

**Latar:** `apps/web/core/components/issues/issue-modal/components/description-editor.tsx` ("Auto-generate description") dan `apps/web/core/components/core/modals/gpt-assistant-popover.tsx` memakai `createGptTask` satu arah tanpa konsep percakapan. Karena endpoint chat kini wajib `conversation_id`, keduanya dipindah ke endpoint stateless khusus supaya kontrak chat tetap satu jalur dan history tidak tercemar.

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `apps/api-rs/crates/api/tests/ai_conversations_test.rs`:

```rust
#[tokio::test]
async fn ai_complete_is_stateless_and_persists_nothing() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Missing task → 400.
    let (status, _) = api::routes::ai::workspace_ai_complete(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"prompt": "Pump fails"})),
    )
    .await
    .expect("missing task");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (base_url, bodies) = crate::support::spawn_recording_upstream("editor answer").await;
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", &base_url);
    std::env::set_var("LLM_MODEL", "test-model");

    let (status, Json(body)) = api::routes::ai::workspace_ai_complete(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "Generate a proper description for this work item.",
            "prompt": "Pump fails",
        })),
    )
    .await
    .expect("ai-complete");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["response"], json!("editor answer"));
    assert_eq!(body["response_html"], json!("editor answer"));

    // Old parity: task and prompt are folded, nothing else is added.
    let sent = bodies.lock().unwrap().clone();
    let content = sent[0]["messages"][0]["content"].as_str().unwrap();
    assert!(content.contains("Generate a proper description for this work item."));
    assert!(content.contains("Pump fails"));
    assert!(!content.contains("Conversation so far:"));

    // Stateless: nothing is stored.
    let conversations: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE workspace_id = $1",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conversations, 0);
    let messages: i64 =
        sqlx::query_scalar("SELECT count(*)::int8 FROM ai_messages WHERE conversation_id IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(messages, 0);

    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run (di `apps/api-rs`): `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: FAIL — `workspace_ai_complete` belum ada.

- [ ] **Step 3: Implementasi handler**

Tambahkan di `apps/api-rs/crates/api/src/routes/ai.rs` (setelah `workspace_ai_assistant`):

```rust
/// `POST /api/workspaces/:slug/ai-complete/` — one-shot stateless completion
/// for editor surfaces (no conversation, nothing persisted). Gate and error
/// shapes mirror `/ai-assistant/`.
pub async fn workspace_ai_complete(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let Some(task) = task_from_body(&body) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Task is required"})),
        ));
    };
    let prompt = body.get("prompt").and_then(Value::as_str).unwrap_or("");
    match chat_completion(&cfg.base_url, &cfg.api_key, &cfg.model, task, prompt).await {
        Ok(text) => {
            let html = response_html(&text);
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "response_html": html})),
            ))
        }
        Err(LlmError::RateLimited) => Ok((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": format!("Rate limit exceeded for {}", host_of(&cfg.base_url))})),
        )),
        Err(LlmError::Upstream) => Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "An internal error has occurred."})),
        )),
    }
}
```

- [ ] **Step 4: Daftarkan route**

`main.rs`, setelah route `ai-assistant`:

```rust
        // Rust-only: one-shot stateless completion for editor surfaces
        // (auto-generate description). No conversation, nothing persisted.
        .route(
            "/api/workspaces/:slug/ai-complete/",
            post(routes::ai::workspace_ai_complete),
        )
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p api --test ai_conversations_test -- --test-threads=1`
Expected: `11 passed` (10 + 1 baru).

- [ ] **Step 6: Format + commit**

```bash
rustfmt --edition 2021 apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git add apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/ai_conversations_test.rs
git commit -m "feat(api): add stateless ai-complete endpoint for editors"
```

---

### Task 8: FE lib — tipe percakapan + `buildAiContext`

**Files:**

- Create: `apps/web/core/lib/ai-conversations.ts`
- Create: `apps/web/core/lib/ai-conversations.test.ts`
- Modify: `apps/web/core/lib/ai-context.ts`
- Modify: `apps/web/core/lib/ai-context.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

`apps/web/core/lib/ai-conversations.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { toAiMessage } from "./ai-conversations";
import type { TAiStoredMessage } from "./ai-conversations";

const stored = (overrides: Partial<TAiStoredMessage>): TAiStoredMessage => ({
  id: "m1",
  role: "assistant",
  content: "plain",
  content_html: null,
  metadata: {},
  created_at: "2026-09-24T10:00:00Z",
  ...overrides,
});

describe("toAiMessage", () => {
  it("prefers response html for assistant messages", () => {
    const message = toAiMessage(stored({ content: "plain", content_html: "<p>rich</p>" }));
    expect(message.content).toBe("<p>rich</p>");
    expect(message.isError).toBe(false);
    expect(message.createdAt).toBe("2026-09-24T10:00:00Z");
  });

  it("falls back to plain content when html is missing", () => {
    expect(toAiMessage(stored({})).content).toBe("plain");
  });

  it("maps schedule metadata onto the message", () => {
    const message = toAiMessage(
      stored({
        metadata: {
          schedule_proposal: {
            name: "Laporan",
            prompt: "ringkas",
            frequency: "weekly",
            time: "09:00",
            timezone: "Asia/Jakarta",
          },
          schedule_proposal_key: "key-1",
          schedule_decision: "created",
          created_schedule_id: "sched-1",
          is_error: false,
        },
      })
    );
    expect(message.scheduleProposal?.name).toBe("Laporan");
    expect(message.scheduleProposalKey).toBe("key-1");
    expect(message.scheduleDecision).toBe("created");
    expect(message.createdScheduleId).toBe("sched-1");
  });

  it("maps is_error to the error flag", () => {
    const message = toAiMessage(stored({ metadata: { is_error: true } }));
    expect(message.isError).toBe(true);
  });
});
```

`apps/web/core/lib/ai-context.test.ts` — ganti blok `describe("buildAiPrompt", ...)` dengan:

```ts
describe("buildAiContext", () => {
  const context: TAiIssueContext = {
    name: "Login fails with SSO",
    descriptionHtml: "<p>User cannot log in via SSO.</p>",
    state: "In Progress",
    priority: "high",
  };

  it("includes issue context and timezone but not history", () => {
    const result = buildAiContext(context, "Asia/Jakarta");
    expect(result).toContain("Work item context:");
    expect(result).toContain("Work item: Login fails with SSO");
    expect(result).toContain("State: In Progress");
    expect(result).toContain("User timezone: Asia/Jakarta");
    expect(result).not.toContain("Conversation so far:");
  });

  it("omits context block fields that are missing", () => {
    const result = buildAiContext({ name: "X", descriptionHtml: "" });
    expect(result).not.toContain("Description:");
    expect(result).not.toContain("State:");
    expect(result).not.toContain("User timezone:");
  });

  it("falls back to general knowledge without an active issue", () => {
    expect(buildAiContext(undefined)).toBe("No active work item context. Answer from general knowledge.");
  });
});
```

Perbarui juga import di file itu: `buildAiContext` menggantikan `buildAiPrompt`.

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test -- ai-conversations ai-context`
Expected: FAIL — modul `./ai-conversations` belum ada; `buildAiContext` belum ada.

- [ ] **Step 3: Implementasi**

`apps/web/core/lib/ai-conversations.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiMessage } from "@/lib/ai-context";
import type { TAiScheduleProposal } from "@/lib/ai-schedule";

export type TAiConversationMode = "classic" | "agent";

export type TAiConversation = {
  id: string;
  title: string;
  mode: TAiConversationMode;
  created_at: string;
  updated_at: string;
};

export type TAiMessageMetadata = {
  schedule_proposal?: TAiScheduleProposal;
  schedule_proposal_key?: string;
  schedule_decision?: "pending" | "created" | "cancelled";
  created_schedule_id?: string;
  is_error?: boolean;
};

export type TAiStoredMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  content_html?: string | null;
  metadata: TAiMessageMetadata;
  created_at: string;
};

/** Keys the `PATCH .../messages/:id/` endpoint accepts (server allowlist). */
export type TAiMessageMetadataPatch = Pick<TAiMessageMetadata, "schedule_decision" | "created_schedule_id">;

/** Map a server-stored message onto the chat bubble model. */
export const toAiMessage = (stored: TAiStoredMessage): TAiMessage => ({
  id: stored.id,
  role: stored.role,
  content: stored.role === "assistant" ? (stored.content_html ?? stored.content) : stored.content,
  isError: stored.metadata?.is_error === true,
  scheduleProposal: stored.metadata?.schedule_proposal,
  scheduleProposalKey: stored.metadata?.schedule_proposal_key,
  scheduleDecision: stored.metadata?.schedule_decision,
  createdScheduleId: stored.metadata?.created_schedule_id,
  createdAt: stored.created_at,
});
```

`apps/web/core/lib/ai-context.ts` — hapus `HISTORY_MESSAGE_LIMIT`, `buildAiPrompt`; tambah `buildAiContext`:

```ts
export const buildAiContext = (context: TAiIssueContext | undefined, userTimezone?: string): string => {
  const timezoneBlock = userTimezone ? `\nUser timezone: ${userTimezone}` : "";
  return `${buildContextBlock(context)}${timezoneBlock}`;
};
```

Pada `TAiMessage`, tambah `createdAt?: string;`.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm --filter=web test -- ai-conversations ai-context`
Expected: semua lulus.

- [ ] **Step 5: Typecheck (buildAiPrompt dihapus → pastikan tidak ada pemakai lain)**

Run: `rg -n "buildAiPrompt" apps/web --glob '!node_modules'` → hanya boleh kosong setelah store diperbarui (Task 10). Untuk sekarang boleh masih ada pemakai di store; typecheck baru hijau setelah Task 10.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/lib/ai-conversations.ts apps/web/core/lib/ai-conversations.test.ts apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-context.test.ts
git commit -m "feat(web): add conversation types and context-only prompt helper"
```

---

### Task 9: FE service — `AiConversationsService` + payload chat baru

**Files:**

- Create: `apps/web/core/services/ai-conversations.service.ts`
- Modify: `apps/web/core/services/ai.service.ts`

- [ ] **Step 1: Implementasi service percakapan**

`apps/web/core/services/ai-conversations.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import { APIService } from "@/services/api.service";
import type {
  TAiConversation,
  TAiConversationMode,
  TAiMessageMetadataPatch,
  TAiStoredMessage,
} from "@/lib/ai-conversations";

export class AiConversationsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async list(workspaceSlug: string): Promise<TAiConversation[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-conversations/`)
      .then((response) => response?.data?.conversations ?? [])
      .catch((error) => {
        throw error?.response;
      });
  }

  async create(workspaceSlug: string, mode: TAiConversationMode, title?: string): Promise<TAiConversation> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-conversations/`, { mode, title })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async retrieve(workspaceSlug: string, conversationId: string): Promise<TAiConversation> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async update(workspaceSlug: string, conversationId: string, title: string): Promise<TAiConversation> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/`, { title })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async remove(workspaceSlug: string, conversationId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/`)
      .then(() => undefined)
      .catch((error) => {
        throw error?.response;
      });
  }

  async listMessages(workspaceSlug: string, conversationId: string): Promise<TAiStoredMessage[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/messages/`)
      .then((response) => response?.data?.messages ?? [])
      .catch((error) => {
        throw error?.response;
      });
  }

  async updateMessageMetadata(
    workspaceSlug: string,
    conversationId: string,
    messageId: string,
    metadata: TAiMessageMetadataPatch
  ): Promise<TAiStoredMessage> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/messages/${messageId}/`, {
      metadata,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
}
```

- [ ] **Step 2: Ubah payload + tipe respons chat**

`apps/web/core/services/ai.service.ts`:

```ts
import type { TAiConversation, TAiStoredMessage } from "@/lib/ai-conversations";
import type { TAiAgentPendingAction } from "@/lib/ai-schedule";

export type TChatPayload = {
  task: string;
  prompt: string;
  context: string;
  conversation_id: string;
};

export type TChatResponse = {
  response: string;
  response_html?: string;
  tool_calls?: { name: string; arguments: unknown }[];
  pending_action?: TAiAgentPendingAction | null;
  conversation: TAiConversation;
  user_message: TAiStoredMessage;
  assistant_message: TAiStoredMessage;
};

export type TAgentTaskResponse = TChatResponse;
```

Ganti dua method:

```ts
  async createGptTask(workspaceSlug: string, data: TChatPayload): Promise<TChatResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-assistant/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async createAgentTask(workspaceSlug: string, data: TChatPayload): Promise<TChatResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-agent/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
```

- [ ] **Step 3: Typecheck (masih gagal di store — catat error yang tersisa hanya di store)**

Run: `pnpm --filter=web check:types`
Expected: error di `ai-assistant.store.ts` (payload lama) DAN dua pemanggil `createGptTask` lama
(`gpt-assistant-popover.tsx`, `description-editor.tsx`) yang diperbaiki Task 9b; file service baru bersih.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/services/ai-conversations.service.ts apps/web/core/services/ai.service.ts
git commit -m "feat(web): add conversation service and stateful chat payload"
```

---

---

### Task 9b: FE — permukaan editor pindah ke `completeTask`

**Files:**

- Modify: `apps/web/core/services/ai.service.ts`
- Modify: `apps/web/core/components/core/modals/gpt-assistant-popover.tsx`
- Modify: `apps/web/core/components/issues/issue-modal/components/description-editor.tsx`

**Latar:** dua permukaan editor memanggil `createGptTask` (chat, kini wajib `conversation_id`). Setelah Task 7b ada `/ai-complete/`; keduanya pindah ke sana supaya tidak membuat percakapan untuk task editor satu arah.

- [ ] **Step 1: Tambah `completeTask` di `ai.service.ts`**

```ts
export type TCompleteTaskResponse = {
  response: string;
  response_html?: string;
};
```

dan method (setelah `createAgentTask`):

```ts
  async completeTask(workspaceSlug: string, data: { task: string; prompt: string }): Promise<TCompleteTaskResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-complete/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
```

- [ ] **Step 2: Ganti pemanggil di `description-editor.tsx`**

`aiService.createGptTask(workspaceSlug.toString(), { prompt: issueName, task: "Generate a proper description for this work item." })`
→ `aiService.completeTask(workspaceSlug.toString(), { ... })` (argumen sama). `res.response` / `res.response_html` tetap; `response_html` kini bertipe `string | undefined`, jadi pakai `res.response_html ?? res.response` bila TS mengeluh di `handleAiAssistance(...)`.

- [ ] **Step 3: Ganti pemanggil di `gpt-assistant-popover.tsx`**

Semua pemanggilan `aiService.createGptTask(...)` → `aiService.completeTask(...)` dengan argumen yang sama; hapus cast `any` pada respons bila tidak lagi diperlukan. Karena `response_html` kini `string | undefined`, ubah `setResponse(res.response_html)` menjadi `setResponse(res.response_html ?? res.response)`.

- [ ] **Step 4: Verifikasi**

```bash
rg -n "createGptTask" apps/web --glob '!node_modules'
```

Expected: hanya `core/store/ai-assistant.store.ts`, `core/store/ai-assistant.store.test.ts`, dan `core/services/ai.service.ts` (definisi).

```bash
pnpm --filter=web check:types
```

Expected: error hanya di `core/store/ai-assistant.store*` — plus apa pun yang berasal dari `buildAiPrompt` yang sudah dihapus (Task 10–11 memperbaikinya); TIDAK ada error di dua file editor.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/ai.service.ts apps/web/core/components/core/modals/gpt-assistant-popover.tsx apps/web/core/components/issues/issue-modal/components/description-editor.tsx
git commit -m "feat(web): route editor AI generation through stateless endpoint"
```

---

### Task 10: Store — daftar percakapan, buka/new/rename/delete, ganti mode

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Ganti helper service di `ai-assistant.store.test.ts` (blok `makeService`/`makeSchedulesService`) dan tambahkan suite baru. Contoh lengkap helper:

```ts
import { AIAssistantStore, clearPersistedAiConversations } from "./ai-assistant.store";
import type { TAiConversation, TAiStoredMessage } from "@/lib/ai-conversations";

const conversation = (
  id: string,
  mode: "classic" | "agent",
  overrides: Partial<TAiConversation> = {}
): TAiConversation => ({
  id,
  title: `conv ${id}`,
  mode,
  created_at: "2026-09-24T09:00:00Z",
  updated_at: "2026-09-24T09:00:00Z",
  ...overrides,
});

const storedMessage = (id: string, role: "user" | "assistant", content: string): TAiStoredMessage => ({
  id,
  role,
  content,
  content_html: role === "assistant" ? content : null,
  metadata: {},
  created_at: "2026-09-24T09:00:00Z",
});

const chatResponse = (conversationId: string, question: string, answer: string) => ({
  response: answer,
  response_html: answer,
  conversation: conversation(conversationId, "agent", { title: question, updated_at: "2026-09-24T10:00:00Z" }),
  user_message: storedMessage("srv-user", "user", question),
  assistant_message: storedMessage("srv-assistant", "assistant", answer),
});

const makeServices = (overrides: Partial<Record<string, any>> = {}) => ({
  ai: {
    createGptTask: vi.fn(async (_slug: string, data: any) =>
      chatResponse(data.conversation_id, data.prompt, "classic ok")
    ),
    createAgentTask: vi.fn(async (_slug: string, data: any) =>
      chatResponse(data.conversation_id, data.prompt, "agent ok")
    ),
    ...(overrides.ai ?? {}),
  },
  schedules: { create: vi.fn(async () => ({ id: "s1" })), ...(overrides.schedules ?? {}) },
  conversations: {
    list: vi.fn(async () => [conversation("c-agent", "agent"), conversation("c-classic", "classic")]),
    create: vi.fn(async (_slug: string, mode: "classic" | "agent") => conversation(`c-new-${mode}`, mode)),
    listMessages: vi.fn(async () => [
      storedMessage("srv-1", "user", "old question"),
      storedMessage("srv-2", "assistant", "old answer"),
    ]),
    update: vi.fn(async (_slug: string, id: string, title: string) => conversation(id, "agent", { title })),
    remove: vi.fn(async () => undefined),
    updateMessageMetadata: vi.fn(async () => storedMessage("srv-assistant", "assistant", "ok")),
    ...(overrides.conversations ?? {}),
  },
});

const makeStore = (services = makeServices()) =>
  new AIAssistantStore(services.ai as any, services.schedules as any, services.conversations as any);
```

Test baru:

```ts
describe("conversation history", () => {
  it("loads conversations on workspace set and opens the newest of the restored mode", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    expect(services.conversations.list).toHaveBeenCalledWith("acme");
    expect(store.conversations).toHaveLength(2);
    expect(store.activeConversationId).toBe("c-classic");
    expect(store.messages.map((m) => m.content)).toEqual(["old question", "old answer"]);
    expect(services.conversations.listMessages).toHaveBeenCalledWith("acme", "c-classic");
  });

  it("switching mode opens the newest conversation of that mode without clearing anything", async () => {
    const store = makeStore();
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    expect(store.activeConversationId).toBe("c-agent");
    expect(store.mode).toBe("agent");
  });

  it("newChat clears the active conversation without calling the API", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.newChat();
    expect(store.activeConversationId).toBeUndefined();
    expect(store.messages).toEqual([]);
    expect(services.conversations.create).not.toHaveBeenCalled();
  });

  it("rename and delete update the local list", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.renameConversation("c-classic", "Renamed");
    expect(store.conversations.find((c) => c.id === "c-classic")?.title).toBe("Renamed");
    await store.deleteConversation("c-classic");
    expect(store.conversations.find((c) => c.id === "c-classic")).toBeUndefined();
    expect(store.activeConversationId).toBeUndefined();
  });

  it("a 404 while opening a conversation resets to a new chat", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => [conversation("gone", "classic")]),
        listMessages: vi.fn(async () => {
          throw { status: 404 };
        }),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    expect(store.conversations).toEqual([]);
    expect(store.activeConversationId).toBeUndefined();
    expect(store.messages).toEqual([]);
  });

  it("clears legacy localStorage messages and keeps server history on sign-out", async () => {
    localStorage.setItem("ai_assistant_messages_acme", JSON.stringify([{ id: "legacy" }]));
    const store = makeStore();
    store.setWorkspace("acme");
    await flush();
    expect(localStorage.getItem("ai_assistant_messages_acme")).toBeNull();
    clearPersistedAiConversations();
    expect(store.conversations).toHaveLength(2);
  });
});
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test -- ai-assistant.store`
Expected: FAIL — API store baru belum ada.

- [ ] **Step 3: Implementasi store (bagian state + daftar + navigasi)**

Ubah `apps/web/core/store/ai-assistant.store.ts`:

```ts
import { AiConversationsService } from "@/services/ai-conversations.service";
import type { TAiConversation, TAiConversationMode } from "@/lib/ai-conversations";

type TAiConversationsService = Pick<
  AiConversationsService,
  "list" | "create" | "listMessages" | "update" | "remove" | "updateMessageMetadata"
>;
```

Konstanta + pembersihan:

```ts
export const AI_ASSISTANT_STORAGE_PREFIX = "ai_assistant_messages_";
export const AI_ASSISTANT_ACTIVE_PREFIX = "ai_assistant_active_conversation_";
export const clearPersistedAiConversations = () => {
  try {
    const keys: string[] = [];
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index);
      if (key?.startsWith(AI_ASSISTANT_STORAGE_PREFIX) || key?.startsWith(AI_ASSISTANT_ACTIVE_PREFIX)) {
        keys.push(key);
      }
    }
    keys.forEach((key) => localStorage.removeItem(key));
  } catch {
    // storage unavailable — best-effort
  }
};

const activeStorageKey = (workspaceSlug: string | undefined, mode: TAiAssistantMode) =>
  `${AI_ASSISTANT_ACTIVE_PREFIX}${workspaceSlug ?? "unknown"}_${mode}`;
```

State baru + konstruktor:

```ts
  conversations: TAiConversation[] = [];
  activeConversationId: string | undefined = undefined;
  conversationsLoading = false;

  private listSeq = 0;

  constructor(
    private aiService: TAiService = new AIService(),
    private schedulesService: TAiSchedulesService = new AiSchedulesService(),
    private conversationsService: TAiConversationsService = new AiConversationsService()
  ) {
```

Tambahkan ke `makeObservable`: `conversations: observable.deep`, `activeConversationId: observable.ref`, `conversationsLoading: observable.ref`, dan semua action baru (`loadConversations`, `openConversation`, `newChat`, `renameConversation`, `deleteConversation`).

Aksi inti:

```ts
  setWorkspace = (workspaceSlug: string | undefined) => {
    if (workspaceSlug !== this.workspaceSlug) {
      this.activeIssueContext = undefined;
      this.isGenerating = false;
      this.requestSeq += 1;
      this.listSeq += 1;
      this.conversations = [];
      this.activeConversationId = undefined;
      this.messages = [];
    }
    this.workspaceSlug = workspaceSlug;
    this.mode = this.restoreMode();
    if (workspaceSlug) {
      this.clearLegacyMessages();
      void this.loadConversations();
    }
  };

  loadConversations = async () => {
    const slug = this.workspaceSlug;
    if (!slug) return;
    const seq = ++this.listSeq;
    this.conversationsLoading = true;
    try {
      const conversations = await this.conversationsService.list(slug);
      if (seq !== this.listSeq) return;
      runInAction(() => {
        this.conversations = conversations;
        this.conversationsLoading = false;
      });
      await this.openLastForMode();
    } catch {
      if (seq !== this.listSeq) return;
      runInAction(() => {
        this.conversationsLoading = false;
      });
    }
  };

  setMode = (mode: TAiAssistantMode) => {
    if (mode === this.mode) return;
    this.requestSeq += 1;
    this.isGenerating = false;
    this.mode = mode;
    this.persistMode();
    void this.openLastForMode();
  };

  openConversation = async (conversationId: string) => {
    const slug = this.workspaceSlug;
    const conversation = this.conversations.find((candidate) => candidate.id === conversationId);
    if (!slug || !conversation) return;
    const seq = ++this.requestSeq;
    this.isGenerating = false;
    this.activeConversationId = conversationId;
    this.mode = conversation.mode;
    this.persistMode();
    this.persistActiveId();
    try {
      const messages = await this.conversationsService.listMessages(slug, conversationId);
      if (seq !== this.requestSeq) return;
      runInAction(() => {
        this.messages = messages.map(toAiMessage);
      });
    } catch (error: any) {
      if (seq !== this.requestSeq) return;
      if (error?.status === 404) {
        runInAction(() => {
          this.conversations = this.conversations.filter((candidate) => candidate.id !== conversationId);
        });
        this.newChat();
      }
    }
  };

  newChat = () => {
    this.requestSeq += 1;
    this.isGenerating = false;
    runInAction(() => {
      this.activeConversationId = undefined;
      this.messages = [];
    });
    this.clearActiveId();
  };

  renameConversation = async (conversationId: string, title: string) => {
    const slug = this.workspaceSlug;
    const trimmed = title.trim();
    if (!slug || !trimmed) return;
    const updated = await this.conversationsService.update(slug, conversationId, trimmed);
    runInAction(() => {
      this.conversations = this.conversations.map((candidate) =>
        candidate.id === conversationId ? updated : candidate
      );
    });
  };

  deleteConversation = async (conversationId: string) => {
    const slug = this.workspaceSlug;
    if (!slug) return;
    await this.conversationsService.remove(slug, conversationId);
    runInAction(() => {
      this.conversations = this.conversations.filter((candidate) => candidate.id !== conversationId);
    });
    if (this.activeConversationId === conversationId) this.newChat();
  };

  private openLastForMode = async () => {
    const remembered = this.restoreActiveId();
    if (remembered && this.conversations.some((candidate) => candidate.id === remembered)) {
      await this.openConversation(remembered);
      return;
    }
    const newest = this.conversations.find((candidate) => candidate.mode === this.mode);
    if (newest) {
      await this.openConversation(newest.id);
      return;
    }
    this.newChat();
  };

  private persistActiveId() {
    if (!this.workspaceSlug || !this.activeConversationId) return;
    try {
      localStorage.setItem(activeStorageKey(this.workspaceSlug, this.mode), this.activeConversationId);
    } catch {
      // storage unavailable — best-effort
    }
  }

  private restoreActiveId(): string | undefined {
    if (!this.workspaceSlug) return undefined;
    try {
      return localStorage.getItem(activeStorageKey(this.workspaceSlug, this.mode)) ?? undefined;
    } catch {
      return undefined;
    }
  }

  private clearActiveId() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.removeItem(activeStorageKey(this.workspaceSlug, this.mode));
    } catch {
      // storage unavailable — best-effort
    }
  }

  private clearLegacyMessages() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.removeItem(storageKey(this.workspaceSlug));
    } catch {
      // storage unavailable — best-effort
    }
  }
```

Hapus `restore()`/`persist()`/`clearConversation()` (diganti `newChat`); hapus import `buildAiPrompt` bila sudah tidak dipakai (dipakai sampai Task 11 — biarkan dulu agar kompilasi jalan, ganti di Task 11).

`interface IAIAssistantStore` juga diperbarui: tambah `conversations`, `activeConversationId`, `conversationsLoading`, `newChat`, `openConversation`, `renameConversation`, `deleteConversation`, `loadConversations`; hapus `clearConversation`.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm --filter=web test -- ai-assistant.store`
Expected: semua lulus.

**Daftar adaptasi test lama di `ai-assistant.store.test.ts`** (wajib, agar suite lama tidak merah):

- Hapus test "restores persisted messages when workspace is set" dan test apa pun yang memanggil `store.persist()`/`restore()` atau mengandalkan `localStorage` berisi pesan (digantikan suite "conversation history").
- Test yang memeriksa `call[1].prompt` berisi "Work item context:"/"User's new question:" → ganti menjadi `call[1].context` untuk blok konteks dan `call[1].prompt` untuk teks pertanyaan mentah.
- Semua pemakaian `store.clearConversation()` → `store.newChat()`; test "ganti mode mengosongkan chat" → ganti menjadi "ganti mode membuka percakapan terakhir mode itu" (sudah ada di suite baru).
- Test mode persistence (`ai_assistant_mode_<slug>`) tetap dipertahankan.
- Test error mapping 429/400 tetap dipertahankan, tetapi perhatikan pesan 400 kini memakai `err?.data?.error` bila ada (samakan ekspektasi).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): store conversations and switch threads per mode"
```

---

### Task 11: Store — kirim pesan stateful, optimistic replace, keputusan proposal

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

```ts
describe("stateful send flow", () => {
  it("creates a conversation on first send and replaces the optimistic bubble", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => []),
        create: vi.fn(async (_slug: string, mode: "classic" | "agent") => conversation("c-new", mode)),
        listMessages: vi.fn(async () => []),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.sendMessage("hello");
    expect(services.conversations.create).toHaveBeenCalledWith("acme", "agent");
    const payload = services.ai.createAgentTask.mock.calls[0][1];
    expect(payload.conversation_id).toBe("c-new");
    expect(payload.prompt).toBe("hello");
    expect(payload.context).toContain("No active work item context");
    expect(store.messages.map((m) => m.id)).toEqual(["srv-user", "srv-assistant"]);
    expect(store.conversations[0].id).toBe("c-new");
    expect(store.activeConversationId).toBe("c-new");
  });

  it("reuses the active conversation and updates the list title", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("second question");
    const payload = services.ai.createGptTask.mock.calls[0][1];
    expect(payload.conversation_id).toBe("c-classic");
    expect(store.conversations.find((c) => c.id === "c-classic")?.title).toBe("second question");
    expect(services.conversations.create).not.toHaveBeenCalled();
  });

  it("keeps an error bubble and re-sends on retry", async () => {
    const services = makeServices({
      ai: {
        createGptTask: vi
          .fn()
          .mockRejectedValueOnce(Object.assign(new Error("boom"), { status: 500 }))
          .mockResolvedValueOnce(chatResponse("c-classic", "retry me", "recovered")),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("retry me");
    expect(store.messages.at(-1)?.isError).toBe(true);
    await store.retryLast();
    expect(store.messages.at(-1)?.content).toBe("recovered");
    expect(store.messages.at(-1)?.isError).toBe(false);
  });

  it("confirming a proposal persists the decision metadata", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "proposal ready"),
          pending_action: {
            kind: "create_schedule",
            proposal: {
              name: "Laporan",
              prompt: "ringkas overdue",
              frequency: "weekly",
              time: "09:00",
              timezone: "Asia/Jakarta",
            },
          },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal");
    const message = store.messages.find((m) => m.scheduleProposal);
    expect(message?.scheduleProposalKey).toBeTruthy();

    await store.confirmScheduleProposal(message!.id);
    expect(services.schedules.create).toHaveBeenCalled();
    expect(message!.scheduleDecision).toBe("created");
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith(
      "acme",
      "c-agent",
      message!.id,
      expect.objectContaining({ schedule_decision: "created", created_schedule_id: "s1" })
    );
  });

  it("cancelling a proposal persists the cancelled decision", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "proposal ready"),
          pending_action: {
            kind: "create_schedule",
            proposal: {
              name: "Laporan",
              prompt: "ringkas overdue",
              frequency: "daily",
              time: "09:00",
              timezone: "UTC",
            },
          },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal");
    const message = store.messages.find((m) => m.scheduleProposal)!;
    store.resolveScheduleProposal(message.id, "cancelled");
    await flush();
    expect(message.scheduleDecision).toBe("cancelled");
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith("acme", "c-agent", message.id, {
      schedule_decision: "cancelled",
    });
  });
});
```

- [ ] **Step 2: Jalankan, pastikan gagal**

Run: `pnpm --filter=web test -- ai-assistant.store`
Expected: FAIL — payload masih `buildAiPrompt`, `request` belum stateful.

- [ ] **Step 3: Implementasi**

Ganti `sendMessage`, `request`, `confirmScheduleProposal`, `resolveScheduleProposal`:

```ts
  sendMessage = async (question: string) => {
    const trimmed = question.trim();
    if (!trimmed || this.isGenerating || !this.workspaceSlug) return;
    if (isScheduleCommand(trimmed) && this.mode !== "agent") {
      this.setMode("agent");
      await this.openLastForMode();
    }
    const slug = this.workspaceSlug;
    const conversationId = await this.ensureConversation();
    if (!conversationId) return;
    const tempId = uuidv4();
    runInAction(() => {
      this.messages.push({ id: tempId, role: "user", content: trimmed });
    });
    await this.request(trimmed, slug, conversationId, tempId);
  };

  retryLast = async () => {
    if (this.isGenerating || !this.workspaceSlug) return;
    let lastUserQuestion: string | undefined;
    for (let index = this.messages.length - 1; index >= 0; index--) {
      if (this.messages[index].role === "user") {
        lastUserQuestion = this.messages[index].content;
        break;
      }
    }
    if (!lastUserQuestion) return;
    if (!this.messages[this.messages.length - 1]?.isError) return;
    runInAction(() => {
      this.messages.pop();
    });
    const slug = this.workspaceSlug;
    const conversationId = await this.ensureConversation();
    if (!conversationId) return;
    const tempId = uuidv4();
    runInAction(() => {
      this.messages.push({ id: tempId, role: "user", content: lastUserQuestion! });
    });
    await this.request(lastUserQuestion, slug, conversationId, tempId);
  };

  confirmScheduleProposal = async (messageId: string) => {
    const slug = this.workspaceSlug;
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!slug || !message?.scheduleProposal || !message.scheduleProposalKey) return;
    if (message.scheduleDecision !== "pending") return;
    const created = await this.schedulesService.create(slug, message.scheduleProposal, message.scheduleProposalKey);
    if (!created?.id) throw new Error("Schedule creation returned no id");
    runInAction(() => {
      message.scheduleDecision = "created";
      message.createdScheduleId = created.id;
    });
    await this.persistDecision(message, {
      schedule_decision: "created",
      created_schedule_id: created.id,
    });
  };

  resolveScheduleProposal = (messageId: string, decision: "cancelled") => {
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!message || message.scheduleDecision !== "pending") return;
    runInAction(() => {
      message.scheduleDecision = decision;
    });
    void this.persistDecision(message, { schedule_decision: decision });
  };

  private ensureConversation = async (): Promise<string | undefined> => {
    const slug = this.workspaceSlug;
    if (!slug) return undefined;
    const active = this.conversations.find((candidate) => candidate.id === this.activeConversationId);
    if (active && active.mode === this.mode) return active.id;
    const created = await this.conversationsService.create(slug, this.mode);
    runInAction(() => {
      this.conversations = [created, ...this.conversations];
      this.activeConversationId = created.id;
    });
    this.persistActiveId();
    return created.id;
  };

  private persistDecision = async (
    message: TAiMessage,
    metadata: { schedule_decision: "created" | "cancelled"; created_schedule_id?: string }
  ) => {
    const slug = this.workspaceSlug;
    const conversationId = this.activeConversationId;
    if (!slug || !conversationId) return;
    try {
      await this.conversationsService.updateMessageMetadata(slug, conversationId, message.id, metadata);
    } catch {
      // best-effort: the schedule itself is already the source of truth
    }
  };

  private async request(question: string, slug: string, conversationId: string, optimisticId: string) {
    const mode = this.mode;
    const seq = ++this.requestSeq;
    this.isGenerating = true;
    try {
      const userTimezone = mode === "agent" ? Intl.DateTimeFormat().resolvedOptions().timeZone : undefined;
      const payload = {
        task: AI_ASSISTANT_TASK,
        prompt: question,
        context: buildAiContext(this.activeIssueContext, userTimezone),
        conversation_id: conversationId,
      };
      const res =
        mode === "agent"
          ? await this.aiService.createAgentTask(slug, payload)
          : await this.aiService.createGptTask(slug, payload);
      if (seq !== this.requestSeq) return;
      const userMessage = toAiMessage(res.user_message);
      const assistantMessage = toAiMessage(res.assistant_message);
      if (mode === "agent" && res.pending_action?.kind === "create_schedule") {
        assistantMessage.scheduleProposal = res.pending_action.proposal;
      }
      runInAction(() => {
        const index = this.messages.findIndex((candidate) => candidate.id === optimisticId);
        if (index >= 0) {
          this.messages.splice(index, 1, userMessage);
        } else {
          this.messages.push(userMessage);
        }
        this.messages.push(assistantMessage);
        this.conversations = [
          res.conversation,
          ...this.conversations.filter((candidate) => candidate.id !== res.conversation.id),
        ];
      });
    } catch (err: any) {
      if (seq !== this.requestSeq) return;
      const errorContent =
        err?.status === 429
          ? err?.data?.error || "Rate limit exceeded."
          : err?.status === 400
            ? err?.data?.error || "AI is not configured for this instance."
            : "An internal error has occurred. Please try again.";
      runInAction(() => {
        this.messages.push({ id: uuidv4(), role: "assistant", content: errorContent, isError: true });
      });
    } finally {
      if (seq === this.requestSeq) {
        runInAction(() => {
          this.isGenerating = false;
        });
      }
    }
  }
```

Import baru: `buildAiContext` (ganti `buildAiPrompt`), `toAiMessage`, tipe `TAiMessage`.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm --filter=web test -- ai-assistant.store`
Expected: semua lulus (suite lama yang masih relevan diadaptasi; suite "restores persisted messages" sudah diganti di Task 10).

- [ ] **Step 5: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: exit 0. Jika masih ada pemakai `clearConversation` (root.tsx), itu diperbaiki di Task 12 — sementara ini boleh gagal hanya di `root.tsx`.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): send chat turns through stored conversations"
```

---

### Task 12: UI — tombol history + panel daftar percakapan

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/conversation-history-panel.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Buat panel**

`conversation-history-panel.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { formatDistanceToNow } from "date-fns";
import { CheckIcon, CloseIcon, DeleteIcon, PencilIcon } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";

export const ConversationHistoryPanel = observer(function ConversationHistoryPanel({
  onClose,
}: {
  onClose: () => void;
}) {
  const {
    conversations,
    conversationsLoading,
    activeConversationId,
    openConversation,
    renameConversation,
    deleteConversation,
  } = useAiAssistant();
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-4 py-2.5">
        <span className="text-sm font-semibold text-primary">Chat history</span>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close history"
          className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
        >
          <CloseIcon className="size-4" />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-2">
        {conversationsLoading && conversations.length === 0 && (
          <p className="px-2 py-4 text-xs text-tertiary">Loading conversations…</p>
        )}
        {!conversationsLoading && conversations.length === 0 && (
          <p className="px-2 py-4 text-xs text-tertiary">No conversations yet.</p>
        )}
        {conversations.map((conversation) => (
          <div
            key={conversation.id}
            className={cn("group flex items-center gap-2 rounded-md px-2 py-1.5", {
              "bg-layer-1-hover": conversation.id === activeConversationId,
            })}
          >
            {renamingId === conversation.id ? (
              <>
                <input
                  value={renameValue}
                  onChange={(event) => setRenameValue(event.target.value)}
                  aria-label="Conversation title"
                  className="text-xs min-w-0 flex-1 rounded border border-subtle bg-transparent px-1.5 py-1 text-primary outline-none"
                />
                <button
                  type="button"
                  aria-label="Save title"
                  onClick={() => {
                    void renameConversation(conversation.id, renameValue);
                    setRenamingId(null);
                  }}
                  className="text-secondary hover:text-primary"
                >
                  <CheckIcon className="size-3.5" />
                </button>
                <button
                  type="button"
                  aria-label="Cancel rename"
                  onClick={() => setRenamingId(null)}
                  className="text-secondary hover:text-primary"
                >
                  <CloseIcon className="size-3.5" />
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  onClick={() => {
                    void openConversation(conversation.id);
                    onClose();
                  }}
                  className="min-w-0 flex-1 text-left"
                >
                  <span className="text-xs block truncate text-primary">
                    {conversation.title || "New conversation"}
                  </span>
                  <span className="text-[10px] block text-tertiary">
                    {conversation.mode === "agent" ? "Agent" : "Classic"} ·{" "}
                    {formatDistanceToNow(new Date(conversation.updated_at), { addSuffix: true })}
                  </span>
                </button>
                {confirmingDeleteId === conversation.id ? (
                  <>
                    <button
                      type="button"
                      aria-label="Confirm delete"
                      onClick={() => {
                        void deleteConversation(conversation.id);
                        setConfirmingDeleteId(null);
                      }}
                      className="text-[10px] text-danger-primary"
                    >
                      Delete?
                    </button>
                    <button
                      type="button"
                      aria-label="Cancel delete"
                      onClick={() => setConfirmingDeleteId(null)}
                      className="text-secondary hover:text-primary"
                    >
                      <CloseIcon className="size-3.5" />
                    </button>
                  </>
                ) : (
                  <span className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                    <button
                      type="button"
                      aria-label="Rename conversation"
                      onClick={() => {
                        setRenamingId(conversation.id);
                        setRenameValue(conversation.title);
                      }}
                      className="text-secondary hover:text-primary"
                    >
                      <PencilIcon className="size-3.5" />
                    </button>
                    <button
                      type="button"
                      aria-label="Delete conversation"
                      onClick={() => setConfirmingDeleteId(conversation.id)}
                      className="text-secondary hover:text-danger-primary"
                    >
                      <DeleteIcon className="size-3.5" />
                    </button>
                  </span>
                )}
              </>
            )}
          </div>
        ))}
      </div>
    </div>
  );
});
```

Catatan: sebelum menulis, verifikasi nama ikon yang tersedia:

```bash
rg -o "CheckIcon|CloseIcon|DeleteIcon|PencilIcon|HistoryOutline|NewChatOutline" node_modules/.pnpm/@makeplane+propel@0.3.0*/node_modules/@makeplane/propel/dist/icons/index.d.ts | sort -u
```

Jika ada nama yang tidak tersedia, pakai ikon terdekat dari daftar itu (mis. `CheckOutline`, `CloseOutline`, `DeleteOutline`, `PencilOutline`).

- [ ] **Step 2: Wire di `root.tsx`**

- Import: `HistoryOutline` dari `@makeplane/propel/icons`, dan `ConversationHistoryPanel`.
- Tambah state panel di store (pengecualian kecil dari Task 10) — di `ai-assistant.store.ts`:

```ts
historyOpen = false;

setHistoryOpen = (open: boolean) => {
  this.historyOpen = open;
};
```

tambahkan `historyOpen: observable.ref` dan `setHistoryOpen: action` ke `makeObservable`, serta `historyOpen: boolean; setHistoryOpen: (open: boolean) => void;` ke `interface IAIAssistantStore`.

- Ambil dari store di `root.tsx`: `newChat`, `historyOpen`, `setHistoryOpen`.
- Ganti tombol New chat: `onClick={clearConversation}` → `onClick={newChat}`.
- Tambah tombol history sebelum tombol New chat:

```tsx
<Tooltip label="Chat history" side="bottom">
  <button
    type="button"
    onClick={() => setHistoryOpen(!historyOpen)}
    aria-pressed={historyOpen}
    className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
  >
    <HistoryOutline className="size-4" />
  </button>
</Tooltip>
```

- Render panel menggantikan area pesan saat `historyOpen`:

```tsx
{
  historyOpen ? (
    <ConversationHistoryPanel onClose={() => setHistoryOpen(false)} />
  ) : (
    <>
      {/* context strip */}
      {/* messages */}
      {/* composer */}
    </>
  );
}
```

Susun ulang JSX sehingga context strip + messages + composer dibungkus fragment `<>…</>` dan panel dirender sebagai alternatifnya (keduanya di dalam `<div className="flex h-full w-[24rem] max-w-[85vw] flex-col">`).

- [ ] **Step 3: Typecheck + build**

Run: `pnpm --filter=web check:types && pnpm --filter=web build`
Expected: exit 0 keduanya; `rg -n "clearConversation" apps/web/core` kosong.

- [ ] **Step 4: Test FE penuh**

Run: `pnpm --filter=web test`
Expected: semua lulus (≥ 85 test; jumlah bertambah karena suite percakapan).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/conversation-history-panel.tsx apps/web/core/components/ai/assistant-sidebar/root.tsx apps/web/core/store/ai-assistant.store.ts
git commit -m "feat(web): add in-sidebar conversation history panel"
```

---

### Task 13: Sinkronisasi dokumen + smoke script

**Files:**

- Modify: `docs/superpowers/specs/2026-09-24-galileo-chat-history-design.md`
- Modify: `apps/api-rs/scripts/smoke.sh` (komentar)
- Modify: `parity-inventory.json` (entri `/ai-assistant/` + `/ai-agent/`: `conversation_id` kini wajib dan respons menambah `conversation`/`user_message`/`assistant_message`; catat endpoint baru `/ai-conversations/` dan `/ai-complete/`)

- [ ] **Step 1: Sinkronkan spec dengan implementasi**

Ubah di spec:

1. Bagian 3 poin alur: "`INSERT` pesan user" → tambahkan catatan "(di-commit sebelum panggilan LLM)" dan "`INSERT` pesan assistant + prune + `updated_at` percakapan dalam satu transaksi".
2. Bagian Testing FE: hapus baris "Komponen panel: render item + badge, dialog rename/hapus memanggil service." → ganti "Panel diuji lewat smoke manual (repo tanpa infrastruktur test komponen)."
3. Bagian 4 UI: "hapus (dialog konfirmasi)" → "hapus (konfirmasi inline dua langkah)".
4. Bagian 3 body chat: sebutkan field `context` (blok work item + timezone) dan `prompt` = teks mentah user.

- [ ] **Step 2: Update komentar smoke**

`apps/api-rs/scripts/smoke.sh` baris sekitar 617: tambahkan komentar bahwa body tanpa `conversation_id` kini 400 (`conversation_id is required`) dan itu diterima smoke (bukan 404/405):

```bash
# Route harus dilayani Rust (bukan 404/405). Status bergantung konfigurasi
# stack: 400 tanpa key ATAU tanpa conversation_id (keduanya sah), 200/500/429
# bila key ada dan conversation_id valid.
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-09-24-galileo-chat-history-design.md apps/api-rs/scripts/smoke.sh
git commit -m "docs: sync chat history spec with implementation"
```

---

### Task 14: Verifikasi penuh + deploy

**Files:** tidak ada perubahan kode.

- [ ] **Step 1: Backend suite penuh (sekuensial, timeout besar)**

Run (di `apps/api-rs`): `cargo test -p api -- --test-threads=1`
Expected: semua binary hijau (baseline 1141 + test baru), 0 failed.

- [ ] **Step 2: Crate lain**

Run: `cargo test -p ai -p worker -- --test-threads=1 && cargo test -p common`
(`-p common` butuh `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.)
Expected: hijau.

- [ ] **Step 3: Clippy + format**

Run:

```bash
cargo clippy -p api -p ai --all-targets 2>&1 | rg -i "^error" | head
pnpm check:format
pnpm check:lint
```

Expected: tanpa `^error`; format/lint exit 0.

- [ ] **Step 4: FE penuh**

Run: `pnpm --filter=web test && pnpm --filter=web check:types && pnpm --filter=web build`
Expected: hijau semua.

- [ ] **Step 5: Rebuild image API + restart stack**

Run:

```bash
docker compose build --build-arg RELEASE_LTO=false --build-arg RELEASE_CGU=16 api
docker compose up -d api worker beat-worker
sleep 12
docker compose ps api worker beat-worker
curl -s -o /dev/null -w 'health=%{http_code}\n' http://localhost:8000/health
docker exec plane-db psql -U plane -d plane -tAc "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 2;"
```

Expected: image baru; 3 container Up; `health=200`; migrasi teratas `8 | ai conversations` (recorded saat boot).

Catatan: build release penuh bisa >30 menit di mesin 8 core; flag `RELEASE_LTO=false RELEASE_CGU=16` adalah jalur dev yang dipakai stack lokal ini (lihat riwayat fitur Scheduler).

- [ ] **Step 6: Rebuild web prod + restart**

Run:

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 8
curl -s -o /dev/null -w 'index=%{http_code}\n' http://localhost:3000/
```

Expected: `index=200`.

- [ ] **Step 7: Smoke API cepat (butuh TOKEN user)**

Run (ganti `$TOKEN` dan `$WS`):

```bash
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"mode":"agent"}' "$BASE/api/workspaces/$WS/ai-conversations/"
```

Expected: 201 + conversation JSON. Lalu kirim chat dengan `conversation_id` hasil di atas dan pastikan 200 + `user_message`/`assistant_message`.

- [ ] **Step 8: Smoke browser (user)**

Checklist manual:

1. Buka sidebar Galileo → panel history kosong ("No conversations yet").
2. Kirim pesan di Classic → percakapan muncul di history dengan judul otomatis; reload halaman → pesan masih ada.
3. Ganti ke Agent → percakapan Classic tetap ada di history; kirim pesan Agent → percakapan Agent baru muncul.
4. Rename + hapus dari panel (konfirmasi inline); hapus percakapan aktif → kembali ke chat baru.
5. Buat proposal `/schedule …` → Confirm; buka percakapan lain lalu buka lagi percakapan itu → kartu berstatus "Schedule created".
6. Buka tab kedua, kirim pesan di percakapan yang sama → pesan dari kedua tab tampil setelah reload.
7. Sign-out lalu sign-in → history server tetap ada.

- [ ] **Step 9: Commit sisa (bila ada) + laporan**

```bash
git status --short
```

Expected: hanya 7 file modifikasi lama yang tidak terkait. Laporkan hasil verifikasi + minta smoke browser user.

---

## Catatan penutup untuk eksekutor

- Urutan task disusun agar setiap task bisa di-review sendiri; jangan menggabungkan commit.
- Kalau `cargo test -p api` penuh terlalu lambat, jalankan per-file test dulu selama development, dan simpan suite penuh untuk Task 14.
- Setelah semua task, tandai checkbox plan (`- [ ]` → `- [x]`) dan jalankan `pnpm exec oxfmt .` bila format docs bermasalah.
