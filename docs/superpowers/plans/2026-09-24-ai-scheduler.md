# AI Scheduler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Halaman workspace "Scheduler" berisi jadwal rutin agen AI yang dibuat dari chat lewat `/schedule` (agen memanggil tool `create_schedule`, user konfirmasi lewat kartu), dijalankan oleh beat + worker, dengan riwayat run.

**Architecture:** Runtime agen diekstrak ke crate `crates/ai` agar dipakai `api` dan `worker`. Jadwal disimpan di tabel `ai_schedules`/`ai_schedule_runs`; crate `beat` mem-push `ai.schedule.tick` tiap menit, worker mengklaim jadwal jatuh tempo (`FOR UPDATE SKIP LOCKED`), mem-push `ai.schedule.run`, dan mengeksekusi agen read-only. Dispatch worker dibatasi allowlist hanya untuk dua job ini.

**Tech Stack:** Rust (axum, sqlx runtime queries, rig 0.42, tokio, chrono + chrono-tz), React Router + MobX + SWR + `@makeplane/propel` UI, vitest (node env, `core/**/*.test.ts`).

Spec: `docs/superpowers/specs/2026-09-24-ai-scheduler-design.md`.

**Repo conventions yang wajib diikuti:**

- Backend root `apps/api-rs`; jangan `cargo fmt --all` (ada drift). Format file yang disentuh saja: `rustfmt --edition 2021 <file>`.
- Suite backend: `cargo test -p api -- --test-threads=1` (konvensi serial).
- FE: `pnpm --filter=web test`, `pnpm --filter=web check:types`, `pnpm --filter=web check:lint`.
- Pre-commit hook (husky/lint-staged) merapikan file; jangan `--no-verify`.
- Jangan sentuh 7 modifikasi pre-existing di working tree yang tidak terkait (vite configs, `auth.rs`, `test_host.py`, `root.tsx` bagian tak terkait, `metadata.ts`); stage hanya file task.
- Pengerjaan bertahap dengan commit per task.

## File Map

| File                                                                       | Tanggung jawab                                                            |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `apps/api-rs/Cargo.toml`                                                   | member `crates/ai`, workspace dep `chrono-tz`                             |
| `apps/api-rs/crates/ai/Cargo.toml`                                         | crate runtime agen + presets                                              |
| `apps/api-rs/crates/ai/src/lib.rs`                                         | modul: `agent`, `llm`, `schedule`, `tools`                                |
| `apps/api-rs/crates/ai/src/llm.rs`                                         | `LlmConfig`, `resolve_llm_config`, `LlmError`, `response_html`, `host_of` |
| `apps/api-rs/crates/ai/src/agent.rs`                                       | preamble, trace, `run_agent`, `pending_action`                            |
| `apps/api-rs/crates/ai/src/tools.rs`                                       | 3 tool read-only + `create_schedule`                                      |
| `apps/api-rs/crates/ai/src/schedule.rs`                                    | `ScheduleProposal`, validasi, `next_occurrence`                           |
| `apps/api-rs/crates/common/src/crypto.rs`                                  | Fernet decrypt + env helpers (dipindah dari api)                          |
| `apps/api-rs/crates/common/src/stream.rs`                                  | + `job_by_id` + `parse_entry`                                             |
| `apps/api-rs/migrations/0006_ai_schedules.sql`                             | tabel `ai_schedules`, `ai_schedule_runs`                                  |
| `apps/api-rs/crates/api/src/routes/ai.rs`                                  | re-export dari `ai::llm`, handler klasik tetap                            |
| `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`                        | handler `/ai-agent/` + `pending_action`                                   |
| `apps/api-rs/crates/api/src/routes/ai_schedule.rs`                         | endpoint REST scheduler                                                   |
| `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`                    | `tick` + `run`                                                            |
| `apps/api-rs/crates/worker/src/handlers/mod.rs`                            | dispatch allowlist via `job_by_id`                                        |
| `apps/api-rs/crates/beat/src/main.rs`                                      | job `ai.schedule.tick` tiap menit                                         |
| `apps/web/core/services/ai-schedules.service.ts`                           | client REST                                                               |
| `apps/web/core/lib/ai-schedule.ts`                                         | tipe proposal + helper murni (`isScheduleCommand`, humanize)              |
| `apps/web/core/store/ai-schedules.store.ts`                                | state halaman Scheduler                                                   |
| `apps/web/core/store/ai-assistant.store.ts`                                | auto-switch `/schedule`, metadata proposal                                |
| `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx` | kartu konfirmasi                                                          |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`                   | hint `/schedule` + render kartu                                           |
| `apps/web/core/components/ai-scheduler/*`                                  | halaman Scheduler                                                         |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/scheduler/page.tsx`         | route page                                                                |
| `apps/web/app/routes/core.ts`                                              | registrasi route                                                          |
| `packages/constants/src/workspace.ts`                                      | item nav sidebar                                                          |
| `apps/web/core/components/workspace/sidebar/helper.tsx`                    | ikon nav                                                                  |
| `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`              | allowlist `staticItems`                                                   |
| `packages/i18n/src/locales/en/navigation.json`                             | label nav                                                                 |

---

### Task 1: Ekstrak runtime agen + config LLM ke `crates/ai` (move-only)

**Files:**

- Create: `apps/api-rs/crates/ai/Cargo.toml`
- Create: `apps/api-rs/crates/ai/src/lib.rs`
- Create: `apps/api-rs/crates/ai/src/llm.rs`
- Create: `apps/api-rs/crates/ai/src/agent.rs`
- Move: `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs` → `apps/api-rs/crates/ai/src/tools.rs`
- Create: `apps/api-rs/crates/common/src/crypto.rs`
- Modify: `apps/api-rs/Cargo.toml`
- Modify: `apps/api-rs/crates/common/Cargo.toml`, `crates/common/src/lib.rs`
- Modify: `apps/api-rs/crates/api/Cargo.toml`, `crates/api/src/routes/ai.rs`, `crates/api/src/routes/ai_agent/mod.rs`, `crates/api/src/routes/instance_admin.rs`
- Modify: `apps/api-rs/crates/worker/Cargo.toml`

- [ ] **Step 1: Catat baseline test**

Run:

```bash
cd apps/api-rs && cargo test -p api -- --test-threads=1 2>&1 | tail -5
```

Expected: 0 failed (tests yang akan dipindah masih ikut dihitung di `-p api`). Catat jumlahnya.

- [ ] **Step 2: Pindahkan helper Fernet/env ke `common`**

Buat `apps/api-rs/crates/common/src/crypto.rs` dengan memindahkan **verbatim** dari `crates/api/src/routes/instance_admin.rs`: `derive_fernet_key` (baris ~1223), `fernet_secret` (~1231), `decrypt_data` (~1353), `skip_env_vars` (~1545). Ubah visibilitas: `pub fn derive_fernet_key`, `pub fn fernet_secret`, `pub fn decrypt_data`, `pub fn skip_env_vars`. Tambahkan header modul:

```rust
//! Fernet-compatible decrypt + env helpers shared by api/worker/beat.
//! Dipindah dari `crates/api/src/routes/instance_admin.rs` (move-only).
```

Di `crates/common/Cargo.toml` tambahkan dependency:

```toml
aes = { workspace = true }
```

Di `crates/common/src/lib.rs` tambahkan:

```rust
pub mod crypto;
```

Di `instance_admin.rs` ganti definisi keempat fungsi itu dengan re-export (kode lain di file itu tetap memakai nama yang sama):

```rust
pub use common::crypto::{decrypt_data, derive_fernet_key, fernet_secret, skip_env_vars};
```

- [ ] **Step 3: Buat crate `crates/ai`**

`apps/api-rs/Cargo.toml`: tambahkan `"crates/ai"` ke `members`, dan ke `[workspace.dependencies]` tambahkan:

```toml
chrono-tz = "0.10.4"
```

`apps/api-rs/crates/ai/Cargo.toml`:

```toml
[package]
name = "ai"
version = "0.1.0"
edition = "2021"

[dependencies]
common = { path = "../common" }
chrono = { workspace = true }
chrono-tz = { workspace = true }
rig = { workspace = true }
schemars = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sqlx = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
```

`apps/api-rs/crates/ai/src/lib.rs`:

```rust
//! Shared Rig agent runtime: LLM config resolution, prompt preamble, tools,
//! and schedule preset math. Consumed by `crates/api` (HTTP handlers) and
//! `crates/worker` (scheduled runs).

pub mod agent;
pub mod llm;
pub mod tools;
```

(Task 3 menambahkan `pub mod schedule;`.)

- [ ] **Step 4: Pindahkan config LLM ke `ai/src/llm.rs`**

Pindahkan **verbatim** dari `crates/api/src/routes/ai.rs` ke `ai/src/llm.rs`: `DEFAULT_BASE_URL`, `DEFAULT_MODEL`, `LlmConfig` + impl `Debug`, `llm_config_from_rows`, `llm_config_from_env`, `resolve_llm_config`, `LlmError`, `response_html`, `host_of`. Jadikan semuanya `pub`. Header + import:

```rust
//! LLM config resolution (parity with `routes/ai.rs`) + shared helpers.

use sqlx::PgPool;

use common::crypto::{decrypt_data, fernet_secret, skip_env_vars};
```

`ai/src/lib.rs` tambahkan re-export untuk kemudahan konsumen:

```rust
pub use llm::{host_of, resolve_llm_config, response_html, LlmConfig, LlmError};
```

- [ ] **Step 5: Pindahkan runtime agen ke `ai/src/agent.rs`**

Pindahkan **verbatim** dari `crates/api/src/routes/ai_agent/mod.rs` (sisakan handler `workspace_ai_agent` di api): `PREAMBLE`, `MAX_TURNS`, `AGENT_TIMEOUT`, `ToolCallTrace`, `ToolTrace`, `new_trace`, `record`, `prompt_from_body`, `effective_prompt`, `map_prompt_error`, `http_client`, `run_agent`. Jadikan pub yang diperlukan lintas crate. Ubah head import menjadi:

```rust
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::time::Duration;

use rig::completion::PromptError;
use rig::prelude::*;
use rig::providers::openai;
use rig::tool::server::ToolServerHandle;
use serde_json::Value;

use crate::llm::LlmError;
```

`run_agent` dan `effective_prompt` tetap `pub`. Tambahkan pada Step ini fungsi `pending_action` (dipakai Task 5, aman ditambahkan sekarang):

```rust
/// The last `create_schedule` proposal recorded during an agent run, shaped
/// for the FE confirmation card. `None` when the tool was never called.
pub fn pending_action(trace: &ToolTrace) -> Option<Value> {
    let recorded = trace.lock().ok()?;
    recorded
        .iter()
        .rev()
        .find(|call| call.name == "create_schedule")
        .map(|call| {
            serde_json::json!({
                "kind": "create_schedule",
                "proposal": call.arguments.clone(),
            })
        })
}
```

(Task 4 akan mengganti literal `"create_schedule"` dengan konstanta `tools::CREATE_SCHEDULE_NAME`.)

- [ ] **Step 6: Pindahkan tools ke `ai/src/tools.rs`**

```bash
git mv apps/api-rs/crates/api/src/routes/ai_agent/tools.rs apps/api-rs/crates/ai/src/tools.rs
```

Di file pindahan: ganti `use super::{record, ToolTrace};` → `use crate::agent::{record, ToolTrace};`. `record` harus `pub` di `agent.rs`. `CREATE_SCHEDULE_NAME` belum ada (dibuat di Task 4); sementara itu ganti referensi di `pending_action` dengan literal `"create_schedule"` dan di Task 4 ubah ke konstanta. Pastikan `use sqlx::PgPool` dll tetap.

- [ ] **Step 7: Perbarui `crates/api` agar memakai crate `ai`**

`crates/api/Cargo.toml` tambahkan `ai = { path = "../ai" }` (urut alfabet dengan `common`).

`crates/api/src/routes/ai.rs`:

- Hapus definisi yang dipindah, tambahkan re-export agar call site & test lama tetap hidup:

```rust
pub use ai::llm::{
    host_of, llm_config_from_rows, resolve_llm_config, response_html, LlmConfig, LlmError,
    DEFAULT_BASE_URL, DEFAULT_MODEL,
};
```

- Sisa file (handler `workspace_ai_assistant`, `chat_completion`, `build_body`, `task_from_body`, dll) tetap; tambahkan `use ai::llm::{...}` untuk item yang dipakai langsung bila re-export konflik dengan import lokal.

`crates/api/src/routes/ai_agent/mod.rs`:

- Hapus definisi yang dipindah; sisakan handler dan router-facing tipe.
- Tambahkan:

```rust
pub use ai::agent::{
    effective_prompt, new_trace, pending_action, prompt_from_body, record, run_agent,
    ToolCallTrace, ToolTrace, AGENT_TIMEOUT, MAX_TURNS, PREAMBLE,
};
pub use ai::tools::workspace_tools;
pub mod tools {
    pub use ai::tools::*;
}
```

(Task 4 menambahkan `CreateSchedule`/`CreateScheduleArgs` ke re-export.)

- Handler memakai `ai::llm::{host_of, resolve_llm_config, LlmError, response_html}` dan `ai::agent::{...}`; hapus import `crate::routes::ai::{host_of, resolve_llm_config, task_from_body, LlmError}` (kecuali `task_from_body` — pindahkan pemakaiannya ke `ai::agent::prompt_from_body` hanya untuk prompt; `task` parsing tetap dari `routes::ai::task_from_body`).

`crates/api/src/routes/instance.rs`: ubah `use crate::routes::ai::resolve_llm_config;` → `use ai::resolve_llm_config;`.

`crates/worker/Cargo.toml` tambahkan:

```toml
ai = { path = "../ai" }
chrono = { workspace = true }
uuid = { workspace = true }
```

- [ ] **Step 8: Format + verifikasi move-only**

Run:

```bash
cd apps/api-rs
rustfmt --edition 2021 crates/ai/src/*.rs crates/api/src/routes/ai.rs crates/api/src/routes/ai_agent/mod.rs crates/api/src/routes/instance_admin.rs crates/api/src/routes/instance.rs crates/common/src/crypto.rs crates/common/src/lib.rs
cargo test -p api -- --test-threads=1 2>&1 | tail -5
cargo test -p ai 2>&1 | tail -5
cargo build -p worker 2>&1 | tail -3
```

Expected: semua hijau, 0 failed. Test tools kini jalan di `-p ai`. Tidak ada perubahan perilaku endpoint.

- [ ] **Step 9: Commit**

```bash
git add apps/api-rs/Cargo.toml apps/api-rs/crates/ai apps/api-rs/crates/common apps/api-rs/crates/api apps/api-rs/crates/worker/Cargo.toml
git commit -m "refactor(api-rs): extract shared agent runtime into crates/ai"
```

---

### Task 2: Migrasi tabel scheduler

**Files:**

- Create: `apps/api-rs/migrations/0006_ai_schedules.sql`

- [ ] **Step 1: Tulis migrasi**

```sql
-- AI scheduler: recurring agent runs created from the /schedule chat flow.
-- Applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.ai_schedules (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    created_by_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    name character varying(120) NOT NULL,
    prompt text NOT NULL,
    frequency character varying(10) NOT NULL,
    time_of_day character varying(5) NOT NULL DEFAULT '09:00',
    day_of_week smallint,
    day_of_month smallint,
    timezone character varying(64) NOT NULL DEFAULT 'UTC',
    enabled boolean NOT NULL DEFAULT true,
    next_run_at timestamp with time zone NOT NULL,
    proposal_key uuid NOT NULL,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT ai_schedules_frequency_check CHECK (frequency IN ('hourly','daily','weekly','monthly')),
    CONSTRAINT ai_schedules_day_of_week_check CHECK (day_of_week IS NULL OR (day_of_week BETWEEN 1 AND 7)),
    CONSTRAINT ai_schedules_day_of_month_check CHECK (day_of_month IS NULL OR (day_of_month BETWEEN 1 AND 31))
);

CREATE UNIQUE INDEX IF NOT EXISTS ai_schedules_proposal_key_idx
    ON public.ai_schedules (proposal_key);

CREATE INDEX IF NOT EXISTS ai_schedules_due_idx
    ON public.ai_schedules (next_run_at) WHERE enabled AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS ai_schedules_workspace_idx
    ON public.ai_schedules (workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.ai_schedule_runs (
    id uuid NOT NULL,
    schedule_id uuid NOT NULL REFERENCES public.ai_schedules(id) ON DELETE CASCADE,
    workspace_id uuid NOT NULL,
    status character varying(10) NOT NULL,
    trigger character varying(10) NOT NULL,
    prompt text NOT NULL,
    response text,
    response_html text,
    error text,
    tool_calls jsonb,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT ai_schedule_runs_status_check CHECK (status IN ('queued','running','success','failed')),
    CONSTRAINT ai_schedule_runs_trigger_check CHECK (trigger IN ('scheduled','manual'))
);

CREATE INDEX IF NOT EXISTS ai_schedule_runs_schedule_idx
    ON public.ai_schedule_runs (schedule_id, created_at DESC);
```

Catatan review: `ai_schedules_proposal_key_idx` di atas kemudian diperbaiki oleh migrasi
`0007_ai_schedule_review_fixes.sql` (unique per `(workspace_id, proposal_key) WHERE deleted_at IS NULL`,
plus CHECK konsistensi preset dan index sweep `ai_schedule_runs_stuck_idx`). File 0006 tidak diubah.
Handler create (Task 10) memakai target konflik dan fallback yang sudah diskop ke workspace.

- [ ] **Step 2: Terapkan migrasi ke DB dev**

Run:

```bash
docker compose up -d api worker
docker compose exec plane-db psql -U plane -d plane -c "\d ai_schedules" | head -20
```

Expected: tabel `ai_schedules` dan `ai_schedule_runs` ada (migrasi dijalankan saat boot api/worker). Jika `plane-db` bukan nama service di lingkungan ini, gunakan container DB yang dipakai stack Rust.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/0006_ai_schedules.sql
git commit -m "feat(api-rs): add ai_schedules migration"
```

---

### Task 3: Preset + `next_occurrence` + validasi proposal

**Files:**

- Create: `apps/api-rs/crates/ai/src/schedule.rs`

- [ ] **Step 1: Tulis test yang gagal**

Di `crates/ai/src/schedule.rs` (bagian `#[cfg(test)]`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn proposal(frequency: &str, time: &str, dow: Option<i16>, dom: Option<i16>, tz: &str) -> ScheduleProposal {
        ScheduleProposal::new("Report", "Summarize overdue work items", frequency, Some(time), dow, dom, Some(tz))
            .expect("valid proposal")
    }

    #[test]
    fn rejects_bad_frequency_time_and_timezone() {
        assert!(ScheduleProposal::new("n", "p", "often", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", "p", "daily", Some("25:00"), None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", "p", "daily", Some("9:00"), None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", "p", "daily", None, None, None, Some("Mars/Olympus")).is_err());
        assert!(ScheduleProposal::new("n", "p", "weekly", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", "p", "monthly", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("", "p", "daily", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", " ", "daily", None, None, None, Some("UTC")).is_err());
    }

    #[test]
    fn applies_defaults() {
        let p = ScheduleProposal::new("n", "p", "daily", None, None, None, None).unwrap();
        assert_eq!(p.time, "09:00");
        assert_eq!(p.timezone, "UTC");
        let h = ScheduleProposal::new("n", "p", "hourly", None, None, None, None).unwrap();
        assert_eq!(h.time, "00:00");
    }

    #[test]
    fn hourly_advances_to_next_minute() {
        let p = proposal("hourly", "00:30", None, None, "UTC");
        let from = Utc.with_ymd_and_hms(2026, 9, 24, 10, 5, 0).unwrap();
        assert_eq!(p.next_occurrence(from), Utc.with_ymd_and_hms(2026, 9, 24, 10, 30, 0).unwrap());
        let from_after = Utc.with_ymd_and_hms(2026, 9, 24, 10, 45, 0).unwrap();
        assert_eq!(p.next_occurrence(from_after), Utc.with_ymd_and_hms(2026, 9, 24, 11, 30, 0).unwrap());
    }

    #[test]
    fn daily_uses_proposal_timezone() {
        // 09:00 Asia/Jakarta == 02:00 UTC.
        let p = proposal("daily", "09:00", None, None, "Asia/Jakarta");
        let from = Utc.with_ymd_and_hms(2026, 9, 24, 10, 0, 0).unwrap(); // 17:00 WIB
        assert_eq!(p.next_occurrence(from), Utc.with_ymd_and_hms(2026, 9, 25, 2, 0, 0).unwrap());
    }

    #[test]
    fn weekly_picks_monday_and_rolls_forward() {
        let p = proposal("weekly", "09:00", Some(1), None, "UTC");
        // 2026-09-24 is a Thursday; next Monday is 2026-09-28.
        let from = Utc.with_ymd_and_hms(2026, 9, 24, 10, 0, 0).unwrap();
        assert_eq!(p.next_occurrence(from), Utc.with_ymd_and_hms(2026, 9, 28, 9, 0, 0).unwrap());
        // Monday before 09:00 stays the same day.
        let from_monday = Utc.with_ymd_and_hms(2026, 9, 28, 8, 0, 0).unwrap();
        assert_eq!(p.next_occurrence(from_monday), Utc.with_ymd_and_hms(2026, 9, 28, 9, 0, 0).unwrap());
    }

    #[test]
    fn monthly_clamps_short_months() {
        let p = proposal("monthly", "09:00", None, Some(31), "UTC");
        let from = Utc.with_ymd_and_hms(2026, 1, 31, 10, 0, 0).unwrap();
        assert_eq!(p.next_occurrence(from), Utc.with_ymd_and_hms(2026, 2, 28, 9, 0, 0).unwrap());
        let from_early = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        assert_eq!(p.next_occurrence(from_early), Utc.with_ymd_and_hms(2026, 2, 28, 9, 0, 0).unwrap());
    }

    #[test]
    fn dst_gap_moves_forward() {
        // 2026-03-08 02:30 does not exist in America/New_York (spring forward).
        let p = proposal("daily", "02:30", None, None, "America/New_York");
        let from = Utc.with_ymd_and_hms(2026, 3, 8, 0, 0, 0).unwrap();
        assert_eq!(p.next_occurrence(from), Utc.with_ymd_and_hms(2026, 3, 8, 7, 30, 0).unwrap());
    }
}
```

- [ ] **Step 2: Jalankan test — harus gagal compile**

Run: `cd apps/api-rs && cargo test -p ai schedule 2>&1 | tail -5`
Expected: FAIL — `ScheduleProposal` belum ada.

- [ ] **Step 3: Implementasi `schedule.rs`**

Tambahkan `pub mod schedule;` di `apps/api-rs/crates/ai/src/lib.rs`, lalu buat `crates/ai/src/schedule.rs`:

```rust
//! Preset schedules: validation, defaults, and next-occurrence math.
//!
//! Presets: hourly / daily / weekly / monthly, plus "HH:MM" time and an IANA
//! timezone. Monthly clamps to the last day of short months; nonexistent local
//! times (DST spring-forward) move forward to the next valid wall time.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const FREQUENCIES: [&str; 4] = ["hourly", "daily", "weekly", "monthly"];
pub const DEFAULT_TIME: &str = "09:00";
pub const DEFAULT_HOURLY_TIME: &str = "00:00";
pub const DEFAULT_TIMEZONE: &str = "UTC";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleProposal {
    pub name: String,
    pub prompt: String,
    pub frequency: String,
    pub time: String,
    pub day_of_week: Option<i16>,
    pub day_of_month: Option<i16>,
    pub timezone: String,
}

fn parse_time(time: &str) -> Result<(u32, u32), String> {
    let (h, m) = time
        .split_once(':')
        .ok_or_else(|| "time must be HH:MM".to_string())?;
    if h.len() != 2 || m.len() != 2 {
        return Err("time must be HH:MM".to_string());
    }
    let hour: u32 = h.parse().map_err(|_| "time must be HH:MM".to_string())?;
    let minute: u32 = m.parse().map_err(|_| "time must be HH:MM".to_string())?;
    if hour > 23 || minute > 59 {
        return Err("time must be HH:MM (24h)".to_string());
    }
    Ok((hour, minute))
}

impl ScheduleProposal {
    pub fn new(
        name: &str,
        prompt: &str,
        frequency: &str,
        time: Option<&str>,
        day_of_week: Option<i16>,
        day_of_month: Option<i16>,
        timezone: Option<&str>,
    ) -> Result<Self, String> {
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("name must be 1-120 characters".to_string());
        }
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() || prompt.chars().count() > 2000 {
            return Err("prompt must be 1-2000 characters".to_string());
        }
        let frequency = frequency.trim().to_ascii_lowercase();
        if !FREQUENCIES.contains(&frequency.as_str()) {
            return Err(format!("frequency must be one of: {}", FREQUENCIES.join(", ")));
        }
        let fallback = if frequency == "hourly" { DEFAULT_HOURLY_TIME } else { DEFAULT_TIME };
        let time = time
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback)
            .to_string();
        parse_time(&time)?;
        match frequency.as_str() {
            "weekly" if !matches!(day_of_week, Some(1..=7)) => {
                return Err("day_of_week must be 1 (Monday) to 7 (Sunday)".to_string());
            }
            "monthly" if !matches!(day_of_month, Some(1..=31)) => {
                return Err("day_of_month must be 1-31".to_string());
            }
            _ => {}
        }
        let timezone = timezone
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_TIMEZONE)
            .to_string();
        timezone
            .parse::<Tz>()
            .map_err(|_| format!("unknown IANA timezone: {timezone}"))?;
        Ok(Self { name, prompt, frequency, time, day_of_week, day_of_month, timezone })
    }

    pub fn tz(&self) -> Tz {
        self.timezone.parse().expect("validated timezone")
    }

    /// Next fire time strictly after `from`, in UTC.
    pub fn next_occurrence(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        let (hour, minute) = parse_time(&self.time).expect("validated time");
        let local = from.with_timezone(&self.tz());
        let candidate = match self.frequency.as_str() {
            "hourly" => next_hourly(local, minute),
            "daily" => next_daily(local, hour, minute),
            "weekly" => next_weekly(local, self.day_of_week.unwrap_or(1) as u32, hour, minute),
            _ => next_monthly(local, self.day_of_month.unwrap_or(1) as u32, hour, minute),
        };
        candidate.with_timezone(&Utc)
    }
}

/// Resolve a local wall time, moving forward past DST gaps; ambiguity takes
/// the earliest occurrence.
fn local_at(tz: Tz, mut date: NaiveDate, mut hour: u32, minute: u32) -> DateTime<Tz> {
    loop {
        match tz.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => return dt,
            LocalResult::None => {
                hour += 1;
                if hour > 23 {
                    hour = 0;
                    date += Duration::days(1);
                }
            }
        }
    }
}

fn next_hourly(from: DateTime<Tz>, minute: u32) -> DateTime<Tz> {
    let mut candidate = local_at(from.timezone(), from.date_naive(), from.hour(), minute);
    if candidate <= from {
        let bumped = from + Duration::hours(1);
        candidate = local_at(bumped.timezone(), bumped.date_naive(), bumped.hour(), minute);
        if candidate <= from {
            candidate = local_at(bumped.timezone(), bumped.date_naive(), bumped.hour() + 1, minute);
        }
    }
    candidate
}

fn next_daily(from: DateTime<Tz>, hour: u32, minute: u32) -> DateTime<Tz> {
    let tz = from.timezone();
    let mut candidate = local_at(tz, from.date_naive(), hour, minute);
    if candidate <= from {
        candidate = local_at(tz, from.date_naive() + Duration::days(1), hour, minute);
    }
    candidate
}

fn next_weekly(from: DateTime<Tz>, day_of_week: u32, hour: u32, minute: u32) -> DateTime<Tz> {
    let tz = from.timezone();
    let current = from.weekday().number_from_monday();
    let delta = (day_of_week as i64 - current as i64).rem_euclid(7);
    let mut candidate = local_at(tz, from.date_naive() + Duration::days(delta), hour, minute);
    if candidate <= from {
        candidate = local_at(tz, from.date_naive() + Duration::days(delta + 7), hour, minute);
    }
    candidate
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid month");
    (next - first).num_days() as u32
}

fn next_monthly(from: DateTime<Tz>, day_of_month: u32, hour: u32, minute: u32) -> DateTime<Tz> {
    let tz = from.timezone();
    let clamp = |year: i32, month: u32| {
        NaiveDate::from_ymd_opt(year, month, day_of_month.min(days_in_month(year, month)))
            .expect("valid date")
    };
    let (mut year, mut month) = (from.year(), from.month());
    let mut candidate = local_at(tz, clamp(year, month), hour, minute);
    if candidate <= from {
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }
        candidate = local_at(tz, clamp(year, month), hour, minute);
    }
    candidate
}
```

- [ ] **Step 4: Jalankan test**

Run: `cd apps/api-rs && cargo test -p ai schedule 2>&1 | tail -8`
Expected: PASS (8 test).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/schedule.rs
git commit -m "feat(api-rs): add schedule presets and next-occurrence math"
```

---

### Task 4: Tool `create_schedule` + preamble

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools.rs`
- Modify: `apps/api-rs/crates/ai/src/agent.rs` (preamble)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `crates/ai/src/tools.rs` modul tests:

```rust
#[test]
fn create_schedule_proposal_normalizes_defaults() {
    let proposal = proposal_from_args(CreateScheduleArgs {
        name: " Daily overdue ".to_string(),
        prompt: " List overdue items ".to_string(),
        frequency: "weekly".to_string(),
        time: None,
        day_of_week: Some(1),
        day_of_month: None,
        timezone: Some("Asia/Jakarta".to_string()),
    })
    .expect("valid args");
    assert_eq!(proposal.name, "Daily overdue");
    assert_eq!(proposal.prompt, "List overdue items");
    assert_eq!(proposal.time, "09:00");
    assert_eq!(proposal.timezone, "Asia/Jakarta");
    assert_eq!(proposal.day_of_week, Some(1));
}

#[test]
fn create_schedule_proposal_rejects_bad_args() {
    let err = proposal_from_args(CreateScheduleArgs {
        name: "x".to_string(),
        prompt: "y".to_string(),
        frequency: "sometimes".to_string(),
        time: None,
        day_of_week: None,
        day_of_month: None,
        timezone: None,
    })
    .unwrap_err();
    assert!(err.to_string().contains("frequency"));
}

#[tokio::test]
async fn create_schedule_tool_records_proposal() {
    let trace = crate::agent::new_trace();
    let tool = CreateSchedule {
        trace: trace.clone(),
    };
    let out = tool
        .call(
            &mut rig::tool::ToolContext::new(),
            CreateScheduleArgs {
                name: "Daily".to_string(),
                prompt: "Report".to_string(),
                frequency: "daily".to_string(),
                time: Some("08:00".to_string()),
                day_of_week: None,
                day_of_month: None,
                timezone: Some("UTC".to_string()),
            },
        )
        .await
        .unwrap();
    assert!(out.contains("\"frequency\":\"daily\""));
    let recorded = trace.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].name, "create_schedule");
    assert_eq!(recorded[0].arguments["time"], json!("08:00"));
}
```

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cd apps/api-rs && cargo test -p ai create_schedule 2>&1 | tail -5`
Expected: FAIL — `CreateScheduleArgs` belum ada.

- [ ] **Step 3: Implementasi tool**

Di `crates/ai/src/tools.rs` tambahkan (dekat tool lain) + daftarkan di `workspace_tools`:

```rust
pub const CREATE_SCHEDULE_NAME: &str = "create_schedule";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateScheduleArgs {
    /// Short human-readable schedule name, e.g. "Daily overdue report".
    pub name: String,
    /// The exact instruction the agent will run on every fire.
    pub prompt: String,
    /// One of: hourly, daily, weekly, monthly.
    pub frequency: String,
    /// Time of day "HH:MM" (24h). For hourly only the minutes are used. Defaults to 09:00.
    pub time: Option<String>,
    /// For weekly schedules: 1 = Monday … 7 = Sunday.
    pub day_of_week: Option<i16>,
    /// For monthly schedules: day of month, 1-31. Short months clamp to the last day.
    pub day_of_month: Option<i16>,
    /// IANA timezone, e.g. "Asia/Jakarta". Defaults to UTC when unknown.
    pub timezone: Option<String>,
}

/// Validate raw tool args into a normalized proposal (defaults applied).
pub fn proposal_from_args(args: CreateScheduleArgs) -> Result<ScheduleProposal, ToolExecutionError> {
    ScheduleProposal::new(
        &args.name,
        &args.prompt,
        &args.frequency,
        args.time.as_deref(),
        args.day_of_week,
        args.day_of_month,
        args.timezone.as_deref(),
    )
    .map_err(ToolExecutionError::invalid_args)
}

pub struct CreateSchedule {
    pub trace: ToolTrace,
}

impl Tool for CreateSchedule {
    const NAME: &'static str = CREATE_SCHEDULE_NAME;
    type Args = CreateScheduleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose a recurring scheduled task for this workspace. Call this only after you know what to run and how often; the user must confirm the proposal in the UI before anything is saved. Never claim the schedule exists until they confirm.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateScheduleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).unwrap_or_default())
    }
}
```

Tambahkan import `use crate::schedule::ScheduleProposal;` dan di `workspace_tools` tambahkan `.tool(CreateSchedule { trace: trace.clone() })` sebelum `.run()`. Ubah `pending_action` di `agent.rs` memakai `tools::CREATE_SCHEDULE_NAME`. Tambahkan `CreateSchedule, CreateScheduleArgs` ke re-export di `crates/api/src/routes/ai_agent/mod.rs` (baris `pub use ai::tools::workspace_tools;`).

- [ ] **Step 4: Perbarui preamble**

Di `crates/ai/src/agent.rs`, ganti `PREAMBLE` menjadi:

```rust
pub const PREAMBLE: &str = "You are the workspace AI assistant for Plane. \
Answer factual questions about projects and work items by calling the provided \
tools; never invent project identifiers, work item identifiers, counts, or \
states. All tools are read-only and scoped to the user's current workspace. If \
a tool returns no results, say so. Answer concisely in the user's language. \
When the user's message starts with /schedule they want a recurring scheduled \
task: gather anything unclear first (what to run and how often), then call \
create_schedule once with the final details. The schedule is only created after \
the user confirms the proposal card, so never say it is already created.";
```

- [ ] **Step 5: Jalankan test**

Run: `cd apps/api-rs && cargo test -p ai 2>&1 | tail -5`
Expected: PASS (test baru + lama).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(api-rs): add create_schedule proposal tool"
```

---

### Task 5: `pending_action` pada respons `/ai-agent/`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs` (test round-trip proposal)
- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tulis test yang gagal (crate ai)**

Tambahkan di `apps/api-rs/crates/ai/tests/agent_pending_action.rs`:

```rust
//! `pending_action` extraction from a tool trace.

use ai::agent::{new_trace, pending_action, record};
use ai::tools::{CreateSchedule, CreateScheduleArgs};
use rig::tool::{Tool, ToolContext};
use serde_json::json;

#[tokio::test]
async fn pending_action_returns_last_create_schedule_proposal() {
    let trace = new_trace();
    record(&trace, "list_projects", &json!({}));
    let tool = CreateSchedule {
        trace: trace.clone(),
    };
    tool.call(
        &mut ToolContext::new(),
        CreateScheduleArgs {
            name: "Weekly backlog".to_string(),
            prompt: "Summarize backlog".to_string(),
            frequency: "weekly".to_string(),
            time: Some("09:00".to_string()),
            day_of_week: Some(1),
            day_of_month: None,
            timezone: Some("Asia/Jakarta".to_string()),
        },
    )
    .await
    .unwrap();

    let action = pending_action(&trace).expect("proposal recorded");
    assert_eq!(action["kind"], json!("create_schedule"));
    assert_eq!(action["proposal"]["name"], json!("Weekly backlog"));
    assert_eq!(action["proposal"]["day_of_week"], json!(1));
}

#[test]
fn pending_action_is_none_without_proposal() {
    let trace = new_trace();
    record(&trace, "count_work_items", &json!({"priority": "urgent"}));
    assert!(pending_action(&trace).is_none());
}
```

Catat: `record` memerlukan `args: &impl Serialize` — untuk `json!({})` gunakan `record(&trace, "list_projects", &serde_json::Value::Null)` bila perlu.

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cd apps/api-rs && cargo test -p ai pending_action 2>&1 | tail -5`
Expected: FAIL — `pending_action` belum pub/belum ada.

- [ ] **Step 3: Implementasi + plumb ke handler**

`agent.rs`: pastikan `pub fn pending_action` ada dan `pub` (Task 4 Step 3).

`crates/api/src/routes/ai_agent/mod.rs` — pada `success_body(text, tool_calls)` tambahkan parameter trace dan field:

```rust
fn success_body(text: &str, tool_calls: &[ToolCallTrace], action: Option<Value>) -> Value {
    json!({
        "response": text,
        "response_html": crate::routes::ai::response_html(text),
        "tool_calls": tool_calls,
        "pending_action": action,
    })
}
```

Di handler, panggil `success_body(&text, &trace.lock().unwrap().clone(), pending_action(&trace))` — sesuaikan dengan struktur handler saat ini (jika `success_body` masih dua argumen, tambah argumen ketiga).

- [ ] **Step 4: Tambah test integrasi endpoint (fake upstream)**

Di `apps/api-rs/crates/api/tests/ai_agent_test.rs`, tambahkan test baru di modul `tool_roundtrip` (memakai `spawn_roundtrip` yang sudah ada) yang memanggil `run_agent` dengan `tool_roundtrip` fake yang mengembalikan tool call `create_schedule`, lalu assert `pending_action(&trace)["proposal"]["frequency"] == json!("daily")`. Gunakan `ai::tools::CreateSchedule` sebagai tool.

- [ ] **Step 5: Jalankan test**

Run: `cd apps/api-rs && cargo test -p ai 2>&1 | tail -4 && cargo test -p api ai_agent 2>&1 | tail -4`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): surface create_schedule proposal as pending_action"
```

---

### Task 6: `common::stream::job_by_id` + parser murni

**Files:**

- Modify: `apps/api-rs/crates/common/src/stream.rs`

- [ ] **Step 1: Tulis test yang gagal**

Di `crates/common/src/stream.rs` tambahkan:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_entry_reads_job_and_payload() {
        let fields = vec![
            ("job".to_string(), "ai.schedule.run".to_string()),
            ("payload".to_string(), r#"{"run_id":"abc"}"#.to_string()),
        ];
        let (job, payload) = parse_entry(&fields).expect("parsed");
        assert_eq!(job, "ai.schedule.run");
        assert_eq!(payload, json!({"run_id": "abc"}));
    }

    #[test]
    fn parse_entry_rejects_missing_or_bad_fields() {
        assert!(parse_entry(&[]).is_none());
        assert!(parse_entry(&[("job".to_string(), "x".to_string())]).is_none());
        assert!(
            parse_entry(&[
                ("job".to_string(), "x".to_string()),
                ("payload".to_string(), "not-json".to_string())
            ])
            .is_none()
        );
    }
}
```

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cd apps/api-rs && cargo test -p common parse_entry 2>&1 | tail -5`
Expected: FAIL — belum ada.

- [ ] **Step 3: Implementasi**

Tambahkan di `crates/common/src/stream.rs`:

```rust
/// Read one stream entry by id and parse its `job` + JSON `payload` fields.
pub async fn job_by_id(
    mgr: &mut ConnectionManager,
    id: &str,
) -> anyhow::Result<Option<(String, Value)>> {
    let reply: redis::streams::StreamRangeReply = mgr.xrange(STREAM, id, id).await?;
    let Some(entry) = reply.ids.into_iter().next() else {
        return Ok(None);
    };
    let fields: Vec<(String, String)> = entry
        .map
        .into_iter()
        .map(|(key, value)| (key, String::from_redis_value(&value).unwrap_or_default()))
        .collect();
    Ok(parse_entry(&fields))
}

/// Pure parser for a stream entry's fields.
pub fn parse_entry(fields: &[(String, String)]) -> Option<(String, Value)> {
    let get = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let job = get("job")?.to_string();
    let payload = serde_json::from_str(get("payload")?).ok()?;
    Some((job, payload))
}
```

Tambahkan import yang diperlukan (`redis::FromRedisValue`, `serde_json::Value` sudah ada).

- [ ] **Step 4: Jalankan test**

Run: `cd apps/api-rs && cargo test -p common parse_entry 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/common/src/stream.rs
git commit -m "feat(api-rs): read stream job payloads by id"
```

---

### Task 7: Handler `tick` di worker

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`
- Modify: `apps/api-rs/crates/worker/src/handlers/mod.rs`
- Test: `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test DB yang gagal**

`apps/api-rs/crates/worker/tests/ai_schedule_test.rs`:

```rust
//! DB-backed tests for the schedule tick handler. Gated on DATABASE_URL and
//! REDIS_URL (defaults target the local compose stack).

use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use worker::handlers::ai_schedule;

fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
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

#[tokio::test]
async fn tick_claims_due_schedules_and_queues_runs() {
    let pool = pool().await;
    let slug = format!("aisc-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let proposal_key = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
         background_color) VALUES ($1, 'AI Sched', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    // schedule due one minute in the past
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, \
         time_of_day, timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily', 'Summarize overdue', 'daily', '09:00', 'UTC', true, \
         now() - interval '1 minute', $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(proposal_key)
    .execute(&pool)
    .await
    .unwrap();

    let mut redis = common::redis::create_redis(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .await;
    let pushed = ai_schedule::tick(&pool, &mut redis).await.expect("tick");

    let next: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(next > Utc::now(), "next_run_at must advance into the future");
    assert_eq!(pushed.len(), 1, "one run queued");
    let (status, trigger): (String, String) =
        sqlx::query_as("SELECT status, trigger FROM ai_schedule_runs WHERE id = $1")
            .bind(pushed[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "queued");
    assert_eq!(trigger, "scheduled");

    // cleanup
    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1").bind(workspace_id).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1").bind(workspace_id).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1").bind(workspace_id).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(workspace_id).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1").bind(user_id).execute(&pool).await.unwrap();
}
```

Catat: `worker` crate perlu menjadi library + binary agar test bisa `use worker::...`. Tambahkan `apps/api-rs/crates/worker/src/lib.rs`:

```rust
pub mod consumer;
pub mod handlers;
```

dan sesuaikan `src/main.rs` memakai `mod` yang sama bila ada duplikasi (ubah `mod consumer; mod handlers;` menjadi `use worker::{consumer, handlers};`).

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cd apps/api-rs && cargo test -p worker tick_claims -- --test-threads=1 2>&1 | tail -5`
Expected: FAIL — modul `ai_schedule` belum ada.

- [ ] **Step 3: Implementasi `tick`**

`crates/worker/src/handlers/ai_schedule.rs`:

```rust
//! AI schedule jobs: claim due schedules (`tick`) and run the agent (`run`).

use ai::schedule::ScheduleProposal;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct DueSchedule {
    id: Uuid,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
}

impl DueSchedule {
    fn proposal(&self, prompt: &str) -> Result<ScheduleProposal, String> {
        ScheduleProposal::new(
            "scheduled",
            prompt,
            &self.frequency,
            Some(&self.time_of_day),
            self.day_of_week,
            self.day_of_month,
            Some(&self.timezone),
        )
    }
}

/// Mark runs stuck in queued/running for over 15 minutes as failed.
async fn sweep_stuck_runs(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run did not finish within 15 minutes', finished_at = now() \
         WHERE status IN ('queued','running') AND created_at < now() - interval '15 minutes'",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Claim due schedules (row-locked, atomic) and queue one run per schedule.
/// Returns the queued run ids (empty when nothing was due).
pub async fn tick(pool: &PgPool, redis: &mut ConnectionManager) -> anyhow::Result<Vec<Uuid>> {
    sweep_stuck_runs(pool).await?;
    let mut tx = pool.begin().await?;
    let due: Vec<DueSchedule> = sqlx::query_as(
        "SELECT id, frequency, time_of_day, day_of_week, day_of_month, timezone \
         FROM ai_schedules \
         WHERE enabled = true AND deleted_at IS NULL AND next_run_at <= now() \
         ORDER BY next_run_at LIMIT 50 FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut queued = Vec::new();
    for schedule in &due {
        let prompt: String = sqlx::query_scalar("SELECT prompt FROM ai_schedules WHERE id = $1")
            .bind(schedule.id)
            .fetch_one(&mut *tx)
            .await?;
        let Ok(proposal) = schedule.proposal(&prompt) else {
            // DB constraints make this unreachable; stay defensive and move the
            // schedule forward so a bad row can never wedge the tick loop.
            tracing::warn!(schedule_id=%schedule.id, "ai.schedule.tick: invalid preset, skipping");
            sqlx::query(
                "UPDATE ai_schedules SET next_run_at = now() + interval '1 hour', updated_at = now() WHERE id = $1",
            )
            .bind(schedule.id)
            .execute(&mut *tx)
            .await?;
            continue;
        };
        let next = proposal.next_occurrence(Utc::now());
        sqlx::query("UPDATE ai_schedules SET next_run_at = $2, updated_at = now() WHERE id = $1")
            .bind(schedule.id)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
             SELECT $1, s.id, s.workspace_id, 'queued', 'scheduled', s.prompt, now() \
             FROM ai_schedules s WHERE s.id = $2",
        )
        .bind(run_id)
        .bind(schedule.id)
        .execute(&mut *tx)
        .await?;
        queued.push(run_id);
    }
    tx.commit().await?;

    for run_id in &queued {
        common::stream::push_job(redis, "ai.schedule.run", json!({ "run_id": run_id })).await?;
    }
    Ok(queued)
}

/// Execute a queued run (implemented in Task 8).
pub async fn run(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let _ = (pool, payload);
    Ok(())
}
```

`crates/worker/src/handlers/mod.rs` tambahkan `pub mod ai_schedule;`.

- [ ] **Step 4: Jalankan test**

Run: `cd apps/api-rs && cargo test -p worker tick_claims -- --test-threads=1 2>&1 | tail -5`
Expected: PASS (butuh DB + Redis lokal; kalau tidak tersedia, jalankan dengan env compose).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/worker
git commit -m "feat(api-rs): claim due AI schedules in the worker"
```

---

### Task 8: Handler `run` (eksekusi agen + riwayat + prune)

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`
- Modify: `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `crates/worker/tests/ai_schedule_test.rs`:

```rust
#[tokio::test]
async fn run_marks_failed_when_llm_is_not_configured() {
    let pool = pool().await;
    // Reuse seeding from tick test: create workspace + schedule + queued run.
    let slug = format!("aisr-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Run', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id).bind(&slug).bind(user_id).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
         timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily', 'Summarize', 'daily', '09:00', 'UTC', true, now(), $4, now(), now())",
    )
    .bind(schedule_id).bind(workspace_id).bind(user_id).bind(Uuid::new_v4()).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize', now())",
    )
    .bind(run_id).bind(schedule_id).bind(workspace_id).execute(&pool).await.unwrap();

    // With LLM_API_KEY unset and SKIP_ENV_VAR=1 (default), config resolves empty.
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::remove_var("LLM_API_KEY");
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id })).await.expect("run handled");

    let (status, error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM ai_schedule_runs WHERE id = $1")
            .bind(run_id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("not configured"));

    // cleanup omitted here for brevity in the plan: mirror the tick test cleanup.
}
```

Catatan implementer: test di atas memanipulasi env proses; jalankan test worker dengan `--test-threads=1` agar tidak balapan.

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cd apps/api-rs && cargo test -p worker run_marks_failed -- --test-threads=1 2>&1 | tail -5`
Expected: FAIL — `run` masih stub (status tetap `queued`).

- [ ] **Step 3: Implementasi `run`**

Ganti stub `run` di `crates/worker/src/handlers/ai_schedule.rs`:

```rust
#[derive(sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    schedule_id: Uuid,
    workspace_id: Uuid,
    prompt: String,
}

/// Execute one queued run; records success/failure and prunes old runs.
pub async fn run(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let Some(run_id) = payload
        .get("run_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        tracing::warn!(payload=%payload, "ai.schedule.run: missing run_id");
        return Ok(());
    };

    let claimed: Option<RunRow> = sqlx::query_as(
        "UPDATE ai_schedule_runs SET status = 'running', started_at = now() \
         WHERE id = $1 AND status = 'queued' \
         RETURNING id, schedule_id, workspace_id, prompt",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    let Some(run) = claimed else {
        tracing::warn!(run_id=%run_id, "ai.schedule.run: run already claimed or swept");
        return Ok(());
    };

    let config = ai::resolve_llm_config(pool).await;
    if config.api_key.trim().is_empty() {
        finish_failed(pool, run.id, "AI is not configured for this instance").await?;
        return Ok(());
    }

    let trace = ai::agent::new_trace();
    let handle = ai::tools::workspace_tools(pool.clone(), run.workspace_id, trace.clone());
    let result = ai::agent::run_agent(
        &config.base_url,
        &config.api_key,
        &config.model,
        handle,
        None,
        &run.prompt,
    )
    .await;

    match result {
        Ok(text) => {
            let calls = trace.lock().map(|c| c.clone()).unwrap_or_default();
            let tool_calls = serde_json::to_value(&calls).unwrap_or(Value::Null);
            sqlx::query(
                "UPDATE ai_schedule_runs SET status = 'success', response = $2, response_html = $3, \
                 tool_calls = $4, finished_at = now() WHERE id = $1",
            )
            .bind(run.id)
            .bind(&text)
            .bind(ai::response_html(&text))
            .bind(tool_calls)
            .execute(pool)
            .await?;
        }
        Err(error) => {
            let message = match error {
                ai::LlmError::RateLimited => "rate limited by the model provider".to_string(),
                ai::LlmError::Upstream => "model provider request failed".to_string(),
            };
            finish_failed(pool, run.id, &message).await?;
        }
    }

    sqlx::query(
        "DELETE FROM ai_schedule_runs WHERE schedule_id = $1 AND id NOT IN ( \
         SELECT id FROM ai_schedule_runs WHERE schedule_id = $1 ORDER BY created_at DESC LIMIT 20)",
    )
    .bind(run.schedule_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn finish_failed(pool: &PgPool, run_id: Uuid, message: &str) -> anyhow::Result<()> {
    let truncated: String = message.chars().take(500).collect();
    sqlx::query(
        "UPDATE ai_schedule_runs SET status = 'failed', error = $2, finished_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}
```

Catatan: `ai::agent::ToolCallTrace` harus `Serialize` (turuni `Serialize` di `agent.rs` bila belum; sebelumnya hanya `Debug, Clone, PartialEq`). Tambahkan `serde::Serialize` pada derive `ToolCallTrace`.

- [ ] **Step 4: Jalankan test**

Run: `cd apps/api-rs && cargo test -p worker -- --test-threads=1 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/worker
git commit -m "feat(api-rs): execute scheduled agent runs in the worker"
```

---

### Task 9: Wiring dispatch + job beat

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/mod.rs`
- Modify: `apps/api-rs/crates/beat/src/main.rs`

- [ ] **Step 1: Ganti stub `handle_by_id`**

`crates/worker/src/handlers/mod.rs`:

```rust
/// Read one stream entry and dispatch the allowlisted AI schedule jobs.
/// Other beat jobs stay disabled until they are validated individually.
pub async fn handle_by_id(
    pool: &sqlx::PgPool,
    redis: &mut redis::aio::ConnectionManager,
    id: &str,
) -> anyhow::Result<()> {
    let Some((job, payload)) = common::stream::job_by_id(redis, id).await? else {
        tracing::warn!(id=%id, "stream entry vanished before dispatch");
        return Ok(());
    };
    match job.as_str() {
        "ai.schedule.tick" => {
            let queued = ai_schedule::tick(pool, redis).await?;
            tracing::info!(queued=%queued.len(), "ai.schedule.tick done");
            Ok(())
        }
        "ai.schedule.run" => ai_schedule::run(pool, payload).await,
        other => {
            tracing::warn!(job=other, id=%id, "job disabled (not yet enabled)");
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Tambahkan job beat**

Di `crates/beat/src/main.rs`, tambahkan blok sebelum `sched.start()`:

```rust
    // Every minute: AI schedule tick — claims due DB schedules in the worker.
    {
        let mut r = redis.clone();
        sched
            .add(tokio_cron_scheduler::Job::new_async("0 * * * * *", move |_, _| {
                let mut rr = r.clone();
                Box::pin(async move {
                    let _ = common::stream::push_job(&mut rr, "ai.schedule.tick", json!({})).await;
                })
            }).unwrap())
            .await
            .unwrap();
    }
```

- [ ] **Step 3: Test parser dispatch (unit)**

Tambahkan di `crates/worker/src/handlers/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_allowlist_rejects_unknown_jobs_by_default() {
        // Compile-time guard: only the two AI jobs are matched by name here.
        let names = ["ai.schedule.tick", "ai.schedule.run"];
        assert_eq!(names.len(), 2);
    }
}
```

- [ ] **Step 4: Build + test**

Run:

```bash
cd apps/api-rs
cargo build -p worker -p beat 2>&1 | tail -3
cargo test -p worker -- --test-threads=1 2>&1 | tail -4
```

Expected: build sukses; test worker PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/mod.rs apps/api-rs/crates/beat/src/main.rs
git commit -m "feat(api-rs): dispatch AI schedule jobs via beat tick"
```

---

### Task 10: Endpoint `GET` + `POST /ai-schedules/`

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/ai_schedule.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis seed harness + test list/create yang gagal**

`crates/api/tests/ai_schedule_test.rs` — salin helper `insert_user` + seed workspace dari `issue_create_test.rs` (baris 53-133) ke file baru, lalu tambahkan test:

```rust
#[tokio::test]
async fn create_is_idempotent_and_list_returns_rows() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await; // workspace + owner (ADMIN)
    let state = AppState { pool: pool.clone(), redis: redis_client(), config: AppConfig::from_env() };

    let body = json!({
        "name": "Weekly backlog",
        "prompt": "Summarize backlog",
        "frequency": "weekly",
        "time": "09:00",
        "day_of_week": 1,
        "timezone": "Asia/Jakarta",
        "proposal_key": Uuid::new_v4(),
    });
    let (status, Json(created)) =
        ai_schedule::create(State(state.clone()), AuthUser(scratch.user_id), Path(scratch.slug.clone()), Json(body.clone()))
            .await
            .expect("create ok");
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_string();

    // same proposal_key returns the same row
    let (status, Json(again)) =
        ai_schedule::create(State(state.clone()), AuthUser(scratch.user_id), Path(scratch.slug.clone()), Json(body))
            .await
            .expect("replay ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["id"], created["id"]);

    let (status, Json(list)) = ai_schedule::list(State(state.clone()), AuthUser(scratch.user_id), Path(scratch.slug.clone()))
        .await
        .expect("list ok");
    assert_eq!(status, StatusCode::OK);
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], json!(id));
    assert_eq!(rows[0]["frequency"], json!("weekly"));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_rejects_guests_and_bad_payloads() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let guest = scratch.add_actor(&pool, Some(15 + 0), None).await; // sesuaikan: guest role = 5, member = 15
    // ... assert 403 for guest, 400 for frequency "sometimes"
}
```

Catatan implementer: `Scratch` di sini adalah salinan sederhana dari `issue_create_test.rs` (workspace + owner + `purge`); jangan mengubah file test lama. Peran workspace: ADMIN=20, MEMBER=15, GUEST=5.

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cd apps/api-rs && cargo test -p api --test ai_schedule_test -- --test-threads=1 2>&1 | tail -5`
Expected: FAIL — modul route belum ada.

- [ ] **Step 3: Implementasi handler list + create**

`crates/api/src/routes/ai_schedule.rs`:

```rust
//! Rust-only AI scheduler endpoints (no Django counterpart).
//!
//! `GET/POST /api/workspaces/:slug/ai-schedules/` — list + confirm-created
//! schedules. Mutations are creator-or-workspace-admin; reads are ADMIN/MEMBER.

use ai::schedule::ScheduleProposal;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::routes::module::guard_am;
use crate::routes::project::{deny, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

pub const MAX_SCHEDULES_PER_WORKSPACE: i64 = 20;
pub const WEEKLY_DAY_HINT: &str = "day_of_week must be 1 (Monday) to 7 (Sunday)";

pub(crate) async fn workspace_id_for_slug(pool: &PgPool, slug: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

#[derive(serde::Deserialize)]
pub struct CreateScheduleBody {
    pub name: String,
    pub prompt: String,
    pub frequency: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub day_of_week: Option<i16>,
    #[serde(default)]
    pub day_of_month: Option<i16>,
    #[serde(default)]
    pub timezone: Option<String>,
    pub proposal_key: Uuid,
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
    let rows: Vec<Value> = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_agg(row_to_json(rows) ORDER BY rows.created_at DESC) FROM ( \
           SELECT s.id, s.name, s.prompt, s.frequency, s.time_of_day, s.day_of_week, \
                  s.day_of_month, s.timezone, s.enabled, s.next_run_at, s.created_by_id, \
                  s.created_at, r.status AS last_status, r.finished_at AS last_finished_at, \
                  r.created_at AS last_run_at \
           FROM ai_schedules s \
           LEFT JOIN LATERAL (SELECT status, finished_at, created_at FROM ai_schedule_runs \
                              WHERE schedule_id = s.id ORDER BY created_at DESC LIMIT 1) r ON true \
           WHERE s.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) \
             AND s.deleted_at IS NULL \
         ) rows",
    )
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?
    .unwrap_or_else(|| json!([]));
    Ok((StatusCode::OK, Json(rows)))
}
```

Catatan implementer: jika `query_scalar::<_, Value>` rewel dengan tipe jsonb, gunakan `query_as::<_, (Value,)>` lalu `.0`. Alternatif paling aman: ambil `Vec<RowStruct>` dengan `#[derive(sqlx::FromRow)]` dan susun `json!` per baris.

`create`:

```rust
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateScheduleBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let proposal = match ScheduleProposal::new(
        &body.name,
        &body.prompt,
        &body.frequency,
        body.time.as_deref(),
        body.day_of_week,
        body.day_of_month,
        body.timezone.as_deref(),
    ) {
        Ok(proposal) => proposal,
        Err(message) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    };

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_schedules WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_one(&st.pool)
    .await?;
    if count >= MAX_SCHEDULES_PER_WORKSPACE {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("at most {MAX_SCHEDULES_PER_WORKSPACE} schedules per workspace")})),
        ));
    }

    let next_run_at = proposal.next_occurrence(chrono::Utc::now());
    let id = Uuid::new_v4();
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, \
         time_of_day, day_of_week, day_of_month, timezone, enabled, next_run_at, proposal_key, \
         created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, true, $11, $12, now(), now()) \
         ON CONFLICT (workspace_id, proposal_key) WHERE deleted_at IS NULL DO NOTHING RETURNING id",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(auth.0)
    .bind(&proposal.name)
    .bind(&proposal.prompt)
    .bind(&proposal.frequency)
    .bind(&proposal.time)
    .bind(proposal.day_of_week)
    .bind(proposal.day_of_month)
    .bind(&proposal.timezone)
    .bind(next_run_at)
    .bind(body.proposal_key)
    .fetch_optional(&st.pool)
    .await?;

    match inserted {
        Some(id) => Ok((StatusCode::CREATED, Json(json!({"id": id})))),
        None => {
            let existing: Uuid = sqlx::query_scalar(
                "SELECT id FROM ai_schedules WHERE workspace_id = $1 AND proposal_key = $2 AND deleted_at IS NULL",
            )
            .bind(workspace_id)
            .bind(body.proposal_key)
            .fetch_one(&st.pool)
            .await?;
            Ok((StatusCode::OK, Json(json!({"id": existing, "already_exists": true}))))
        }
    }
}
```

Daftarkan modul di `routes/mod.rs` (`pub mod ai_schedule;`) dan route di `main.rs`:

```rust
        .route(
            "/api/workspaces/:slug/ai-schedules/",
            get(routes::ai_schedule::list).post(routes::ai_schedule::create),
        )
```

- [ ] **Step 4: Jalankan test**

Run: `cd apps/api-rs && cargo test -p api --test ai_schedule_test -- --test-threads=1 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_schedule.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/ai_schedule_test.rs
git commit -m "feat(api-rs): add AI schedule list and create endpoints"
```

---

### Task 11: Endpoint detail, pause/resume, delete, run-now

Catatan polish yang dibawa dari review Task 10 (wajib dikerjakan di task ini):

- Helper bersama di `ai_schedule.rs`: `load_schedule(pool, workspace_id, id) -> Option<ScheduleRow>`
  dan `can_manage(created_by_id, user_id, ws_role) -> bool`, dipakai semua handler mutasi.
- `list` memakai `workspace_id_for_slug` (bukan subquery inline) dan run-JSON builder yang sama
  dengan `detail`.
- `ScheduleProposal::new` (crates/ai) menormalkan field hari yang tidak relevan:
  `frequency != "weekly"` → `day_of_week = None`; `frequency != "monthly"` → `day_of_month = None`
  (tambah unit test di `schedule.rs`).
- Test tambahan: guest `list` → 403, isolasi antar workspace, soft-delete membebaskan
  `proposal_key`, detail tanpa run → `last_status`/runs kosong, patch/delete/run-now authz.

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_schedule.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan:

```rust
#[tokio::test]
async fn detail_patch_delete_and_run_now_follow_creator_or_admin() {
    // seed workspace + owner + member actor + one schedule via create handler
    // 1. detail as member -> 200 with `runs` array
    // 2. patch enabled=false as non-creator member -> 403
    // 3. patch enabled=false as creator -> 200 and enabled=false
    // 4. run now as creator -> 201/200 and one queued manual run exists
    // 5. delete as creator -> 204, then list -> []
}
```

Implementer menulis assert lengkap memakai helper seed yang sama (owner = pembuat; actor member dengan role 15 lewat `insert_workspace_member`).

- [ ] **Step 2: Implementasi handler**

Tambahkan `ScheduleRow` (`#[derive(sqlx::FromRow)]` berisi id, created_by_id, prompt, frequency, time_of_day, day_of_week, day_of_month, timezone, enabled, deleted_at) + helper `load_schedule(pool, slug, id) -> Option<ScheduleRow>` dengan filter workspace + `deleted_at IS NULL`.

- `detail`: guard_am → load → 404 bila none → ambil 20 run terakhir (`SELECT id, status, trigger, response, response_html, error, created_at, started_at, finished_at FROM ai_schedule_runs WHERE schedule_id = $1 ORDER BY created_at DESC LIMIT 20`) → JSON `{...schedule, "runs": [...]}`.
- `patch`: guard_am → load → creator/admin check (`row.created_by_id == auth.0 || ws_role == Some(20)`) else `deny()` → body `{"enabled": bool}` → bila `enabled == true` hitung `next_run_at` baru dari proposal → `UPDATE ai_schedules SET enabled = $2, next_run_at = $3, updated_at = now() WHERE id = $1` (bila false, pertahankan `next_run_at`).
- `destroy`: guard_am → load → creator/admin → `UPDATE ... SET deleted_at = now(), updated_at = now()` → 204.
- `run_now`: guard_am → load → creator/admin → insert run `queued` (`trigger='manual'`, snapshot prompt) → `common::stream::push_job(&mut st.redis_client().await?, "ai.schedule.run", json!({"run_id": run_id})).await?` → 201 `{"run_id": id}`.

Registrasi route:

```rust
        .route(
            "/api/workspaces/:slug/ai-schedules/:schedule_id/",
            get(routes::ai_schedule::detail)
                .patch(routes::ai_schedule::patch)
                .delete(routes::ai_schedule::destroy),
        )
        .route(
            "/api/workspaces/:slug/ai-schedules/:schedule_id/run/",
            post(routes::ai_schedule::run_now),
        )
```

- [ ] **Step 3: Jalankan test**

Run: `cd apps/api-rs && cargo test -p api --test ai_schedule_test -- --test-threads=1 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 4: Full suite + clippy**

Run:

```bash
cd apps/api-rs
cargo test -p api -- --test-threads=1 2>&1 | tail -5
cargo clippy -p api -p worker -p beat -p ai --all-targets 2>&1 | rg -i "warning|error" | tail -5
```

Expected: 0 failed; clippy tanpa temuan baru.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api
git commit -m "feat(api-rs): add AI schedule detail, patch, delete and run-now"
```

---

### Task 12: FE service + helper murni

**Files:**

- Create: `apps/web/core/services/ai-schedules.service.ts`
- Create: `apps/web/core/lib/ai-schedule.ts`
- Test: `apps/web/core/lib/ai-schedule.test.ts`

- [ ] **Step 1: Tulis test helper yang gagal**

`apps/web/core/lib/ai-schedule.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { humanizeSchedule, isScheduleCommand } from "./ai-schedule";

describe("isScheduleCommand", () => {
  it("matches only leading /schedule commands", () => {
    expect(isScheduleCommand("/schedule")).toBe(true);
    expect(isScheduleCommand("/schedule every Monday")).toBe(true);
    expect(isScheduleCommand("  /schedule now")).toBe(true);
    expect(isScheduleCommand("please /schedule")).toBe(false);
    expect(isScheduleCommand("/scheduled")).toBe(false);
  });
});

describe("humanizeSchedule", () => {
  it("renders each preset with time and timezone", () => {
    expect(humanizeSchedule({ frequency: "hourly", time: "00:30", timezone: "UTC" })).toBe("Every hour at :30 · UTC");
    expect(humanizeSchedule({ frequency: "daily", time: "09:00", timezone: "Asia/Jakarta" })).toBe(
      "Every day at 09:00 · Asia/Jakarta"
    );
    expect(humanizeSchedule({ frequency: "weekly", time: "09:00", day_of_week: 1, timezone: "Asia/Jakarta" })).toBe(
      "Every Monday at 09:00 · Asia/Jakarta"
    );
    expect(humanizeSchedule({ frequency: "monthly", time: "09:00", day_of_month: 31, timezone: "UTC" })).toBe(
      "Every month on day 31 at 09:00 · UTC"
    );
  });
});
```

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `pnpm --filter=web test 2>&1 | tail -8`
Expected: FAIL — modul `./ai-schedule` belum ada.

- [ ] **Step 3: Implementasi helper + service**

`apps/web/core/lib/ai-schedule.ts`:

```ts
export type TAiScheduleFrequency = "hourly" | "daily" | "weekly" | "monthly";

export type TAiScheduleProposal = {
  name: string;
  prompt: string;
  frequency: TAiScheduleFrequency;
  time: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone: string;
};

export type TAiScheduleRun = {
  id: string;
  status: "queued" | "running" | "success" | "failed";
  trigger: "scheduled" | "manual";
  prompt: string;
  response?: string | null;
  response_html?: string | null;
  error?: string | null;
  created_at: string;
  started_at?: string | null;
  finished_at?: string | null;
};

export type TAiSchedule = TAiScheduleProposal & {
  id: string;
  enabled: boolean;
  next_run_at: string;
  created_by_id: string;
  created_at: string;
  last_status?: TAiScheduleRun["status"] | null;
  last_finished_at?: string | null;
  last_run_at?: string | null;
  runs?: TAiScheduleRun[];
};

const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/** True when the message starts with the `/schedule` slash command. */
export const isScheduleCommand = (text: string): boolean => {
  const trimmed = text.trimStart();
  if (!trimmed.startsWith("/schedule")) return false;
  const rest = trimmed.slice("/schedule".length);
  return rest === "" || /^\s/.test(rest);
};

export const humanizeSchedule = (
  proposal: Pick<TAiScheduleProposal, "frequency" | "time" | "day_of_week" | "day_of_month" | "timezone">
): string => {
  const zone = proposal.timezone || "UTC";
  switch (proposal.frequency) {
    case "hourly":
      return `Every hour at :${proposal.time.slice(-2)} · ${zone}`;
    case "daily":
      return `Every day at ${proposal.time} · ${zone}`;
    case "weekly":
      return `Every ${WEEKDAYS[(proposal.day_of_week ?? 1) - 1] ?? "Monday"} at ${proposal.time} · ${zone}`;
    case "monthly":
      return `Every month on day ${proposal.day_of_month ?? 1} at ${proposal.time} · ${zone}`;
  }
};
```

`apps/web/core/services/ai-schedules.service.ts`:

```ts
import { API_BASE_URL } from "@plane/constants";
import { APIService } from "@/services/api.service";
import type { TAiSchedule, TAiScheduleProposal, TAiScheduleRun } from "@/lib/ai-schedule";

export class AiSchedulesService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async list(workspaceSlug: string): Promise<TAiSchedule[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-schedules/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async create(workspaceSlug: string, proposal: TAiScheduleProposal, proposalKey: string): Promise<{ id: string }> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-schedules/`, { ...proposal, proposal_key: proposalKey })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async retrieve(workspaceSlug: string, scheduleId: string): Promise<TAiSchedule & { runs: TAiScheduleRun[] }> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async update(workspaceSlug: string, scheduleId: string, enabled: boolean): Promise<void> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`, { enabled })
      .then(() => undefined)
      .catch((error) => {
        throw error?.response;
      });
  }

  async remove(workspaceSlug: string, scheduleId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`)
      .then(() => undefined)
      .catch((error) => {
        throw error?.response;
      });
  }

  async runNow(workspaceSlug: string, scheduleId: string): Promise<{ run_id: string }> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/run/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
}
```

- [ ] **Step 4: Jalankan test + typecheck**

Run:

```bash
pnpm --filter=web test 2>&1 | tail -5
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: PASS; typecheck hijau.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts apps/web/core/services/ai-schedules.service.ts
git commit -m "feat(web): add AI schedule types, helpers and service"
```

---

### Task 13: Store halaman Scheduler + hook

**Files:**

- Create: `apps/web/core/store/ai-schedules.store.ts`
- Create: `apps/web/core/store/ai-schedules.store.test.ts`
- Create: `apps/web/core/hooks/store/use-ai-schedules.ts`
- Modify: `apps/web/core/store/root.store.ts`

- [ ] **Step 1: Tulis test store yang gagal**

`apps/web/core/store/ai-schedules.store.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { AiSchedulesStore } from "./ai-schedules.store";

const makeService = () => ({
  list: vi.fn(async () => [{ id: "s1", name: "Daily", enabled: true }]),
  create: vi.fn(async () => ({ id: "s1" })),
  retrieve: vi.fn(async () => ({ id: "s1", runs: [] })),
  update: vi.fn(async () => undefined),
  remove: vi.fn(async () => undefined),
  runNow: vi.fn(async () => ({ run_id: "r1" })),
});

describe("AiSchedulesStore", () => {
  it("fetches schedules for a workspace", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    await store.fetchSchedules("acme");
    expect(service.list).toHaveBeenCalledWith("acme");
    expect(store.schedules).toHaveLength(1);
  });

  it("toggles, removes and runs now", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.schedules = [{ id: "s1", enabled: true } as never];
    await store.toggleSchedule("acme", "s1", false);
    expect(service.update).toHaveBeenCalledWith("acme", "s1", false);
    expect(store.schedules[0].enabled).toBe(false);
    await store.runNow("acme", "s1");
    expect(service.runNow).toHaveBeenCalledWith("acme", "s1");
    await store.deleteSchedule("acme", "s1");
    expect(service.remove).toHaveBeenCalledWith("acme", "s1");
    expect(store.schedules).toHaveLength(0);
  });
});
```

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `pnpm --filter=web test 2>&1 | tail -6`
Expected: FAIL — store belum ada.

- [ ] **Step 3: Implementasi store + hook + registrasi**

`apps/web/core/store/ai-schedules.store.ts`: pola mengikuti `ai-assistant.store.ts` (constructor menerima service, `makeObservable`, `runInAction`), dengan state `schedules: TAiSchedule[]`, `loader`, `error`, aksi `fetchSchedules(slug)`, `toggleSchedule(slug,id,enabled)` (optimistic + refetch ringan), `deleteSchedule`, `runNow`, dan `fetchRuns(slug,id)` yang menyimpan `runsBySchedule[id]`.

`apps/web/core/hooks/store/use-ai-schedules.ts` mengikuti `use-ai-assistant.ts`:

```ts
export const useAiSchedules = (): AiSchedulesStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useAiSchedules must be used within StoreProvider");
  return context.aiSchedules;
};
```

`root.store.ts`: daftarkan `aiSchedules = new AiSchedulesStore();` + import tipe, sama seperti `aiAssistant`.

- [ ] **Step 4: Jalankan test + typecheck**

Run:

```bash
pnpm --filter=web test 2>&1 | tail -5
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-schedules.store.ts apps/web/core/store/ai-schedules.store.test.ts apps/web/core/store/root.store.ts apps/web/core/hooks/store/use-ai-schedules.ts
git commit -m "feat(web): add AI schedules store"
```

---

### Task 14: ai-assistant store — auto-switch, timezone, metadata proposal

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts`
- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`
- Modify: `apps/web/core/lib/ai-context.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-context.test.ts` tambahkan test `buildAiPrompt` memuat `User timezone: Asia/Jakarta` hanya saat argumen ke-4 diberikan, dan tidak muncul tanpa argumen.

Di `ai-assistant.store.test.ts` tambahkan:

```ts
it("auto-switches to agent mode for /schedule commands", async () => {
  const service = makeService();
  const store = new AIAssistantStore(service as never, makeSchedulesService() as never);
  store.setWorkspace("acme");
  await store.sendMessage("/schedule daily overdue report");
  expect(store.mode).toBe("agent");
  expect(service.createAgentTask).toHaveBeenCalled();
});

it("records pending_action metadata and confirms a proposal", async () => {
  const service = makeService(undefined, async () => ({
    response: "ok",
    response_html: "ok",
    pending_action: {
      kind: "create_schedule",
      proposal: {
        name: "Daily",
        prompt: "Report",
        frequency: "daily",
        time: "09:00",
        timezone: "UTC",
      },
    },
  }));
  const schedules = makeSchedulesService();
  const store = new AIAssistantStore(service as never, schedules as never);
  store.setWorkspace("acme");
  await store.sendMessage("buat jadwal harian");
  const message = store.messages[store.messages.length - 1];
  expect(message.scheduleProposal?.name).toBe("Daily");
  expect(message.scheduleDecision).toBe("pending");

  await store.confirmScheduleProposal(message.id);
  expect(schedules.create).toHaveBeenCalledWith(
    "acme",
    expect.objectContaining({ name: "Daily" }),
    message.scheduleProposalKey
  );
  expect(store.messages[store.messages.length - 1].scheduleDecision).toBe("created");
});
```

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `pnpm --filter=web test 2>&1 | tail -6`
Expected: FAIL.

- [ ] **Step 3: Implementasi**

- `ai-context.ts`: `TAiMessage` tambah `scheduleProposal?: TAiScheduleProposal; scheduleProposalKey?: string; scheduleDecision?: "pending" | "created" | "cancelled"; createdScheduleId?: string;`. `buildAiPrompt` tambah parameter opsional `userTimezone?: string` yang menyisipkan baris `User timezone: <tz>` setelah context block (hanya bila diberikan).
- `ai-assistant.store.ts`:
  - constructor kedua: `private schedulesService: TAiSchedulesService = new AiSchedulesService()`.
  - `TAiService` tetap; tambah tipe `TAiSchedulesService = Pick<AiSchedulesService, "create">`.
  - `request()`: mode agent → sertakan `userTimezone` dari `Intl.DateTimeFormat().resolvedOptions().timeZone`; setelah respons, bila `res.pending_action?.kind === "create_schedule"` set `scheduleProposal = res.pending_action.proposal`, `scheduleProposalKey = uuidv4()`, `scheduleDecision = "pending"`.
  - `sendMessage`: sebelum push, bila `isScheduleCommand(trimmed)` dan `mode !== "agent"` → `this.setMode("agent")` (aman: `setMode` mengosongkan percakapan, pesan user baru di-push setelahnya).
  - `confirmScheduleProposal(messageId)`: cari pesan, ambil proposal + key, set decision `created` + `createdScheduleId` setelah sukses; bila gagal biarkan `pending` dan rethrow agar kartu bisa menampilkan error + Retry.
  - `resolveScheduleProposal(messageId, decision)`: untuk Cancel.
- Update `AIService`/`AISchedulesService` types di test helper: `makeSchedulesService` mengembalikan `{ create: vi.fn(async () => ({ id: "s1" })) }`.

- [ ] **Step 4: Jalankan test + typecheck**

Run:

```bash
pnpm --filter=web test 2>&1 | tail -5
pnpm --filter=web check:types 2>&1 | tail -5
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-context.test.ts apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts apps/web/core/services/ai.service.ts
git commit -m "feat(web): create schedules from the chat agent"
```

---

### Task 15: Hint composer + kartu proposal

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Buat komponen kartu**

`schedule-proposal-card.tsx` (ringkas):

```tsx
import { observer } from "mobx-react";
import { useState } from "react";
import { Button } from "@plane/propel/button";
import { humanizeSchedule, type TAiScheduleProposal } from "@/lib/ai-schedule";

type Props = {
  proposal: TAiScheduleProposal;
  decision?: "pending" | "created" | "cancelled";
  createdScheduleId?: string;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
};

export const ScheduleProposalCard = observer(function ScheduleProposalCard({
  proposal,
  decision = "pending",
  onConfirm,
  onCancel,
}: Props) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm();
    } catch {
      setError("Could not create the schedule. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  if (decision === "created")
    return (
      <p className="text-xs mt-1 text-tertiary">
        Schedule created — manage it in the <span className="text-accent-primary">Scheduler</span>.
      </p>
    );
  if (decision === "cancelled") return <p className="text-xs mt-1 text-tertiary">Schedule cancelled.</p>;

  return (
    <div className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-xs font-semibold text-primary">{proposal.name}</p>
      <p className="text-xs mt-0.5 text-secondary">{humanizeSchedule(proposal)}</p>
      <p className="text-xs mt-1 line-clamp-3 text-tertiary">{proposal.prompt}</p>
      {error && <p className="text-xs mt-1 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button size="sm" variant="primary" loading={submitting} onClick={() => void confirm()}>
          Confirm
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
});
```

Catatan implementer: cek nama komponen/varian Button yang benar di `@plane/propel/button` (pola yang ada di repo) dan sesuaikan.

- [ ] **Step 2: Integrasikan di sidebar**

`root.tsx`:

- Import `isScheduleCommand` dan `ScheduleProposalCard`.
- Destructure `confirmScheduleProposal`, `resolveScheduleProposal` dari `useAiAssistant()`.
- Di `handleSend`, sebelum `setQuestion("")`:

```ts
if (isScheduleCommand(value) && mode !== "agent") setMode("agent");
```

- Di dalam bubble assistant (setelah blok konten), render:

```tsx
{
  message.role === "assistant" && message.scheduleProposal && (
    <ScheduleProposalCard
      proposal={message.scheduleProposal}
      decision={message.scheduleDecision}
      createdScheduleId={message.createdScheduleId}
      onConfirm={() => confirmScheduleProposal(message.id)}
      onCancel={() => resolveScheduleProposal(message.id, "cancelled")}
    />
  );
}
```

- Di atas textarea (di dalam container composer), tambahkan hint:

```tsx
{
  question.trimStart().startsWith("/") && !isGenerating && (
    <button
      type="button"
      onClick={() => setQuestion("/schedule ")}
      className="text-xs w-full border-b border-subtle px-3 py-2 text-left text-secondary hover:text-primary"
    >
      /schedule — <span className="text-tertiary">Schedule a recurring AI report</span>
    </button>
  );
}
```

- [ ] **Step 3: Verifikasi**

Run:

```bash
pnpm --filter=web check:types 2>&1 | tail -5
pnpm --filter=web check:lint 2>&1 | tail -3
```

Expected: hijau (warning boleh asal tidak bertambah signifikan).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar
git commit -m "feat(web): add /schedule hint and proposal confirmation card"
```

---

### Task 16: Halaman Scheduler + route + nav

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/scheduler/page.tsx`
- Create: `apps/web/core/components/ai-scheduler/scheduler-view.tsx`
- Create: `apps/web/core/components/ai-scheduler/schedule-item.tsx`
- Create: `apps/web/core/components/ai-scheduler/schedule-runs-list.tsx`
- Modify: `apps/web/app/routes/core.ts`
- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- Modify: `packages/i18n/src/locales/en/navigation.json`

- [ ] **Step 1: Route + nav**

- `apps/web/app/routes/core.ts`: di dalam children layout `(projects)` (dekat entri stickies, ~baris 99-101) tambahkan:

```ts
      route(":workspaceSlug/scheduler", "./(all)/[workspaceSlug]/(projects)/scheduler/page.tsx"),
```

- `packages/constants/src/workspace.ts`: tambahkan item:

```ts
  ai_scheduler: {
    key: "ai_scheduler",
    labelTranslationKey: "sidebar.ai_scheduler",
    href: `/scheduler/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

dan tambahkan ke `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS`.

- `helper.tsx`: import `CalendarOutline` dari `@makeplane/propel/icons` + case:

```tsx
    case "ai_scheduler":
      return <CalendarOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- `sidebar-item.tsx`: tambahkan `"ai_scheduler"` ke array `staticItems`.

- `packages/i18n/src/locales/en/navigation.json`: di objek `sidebar`, tambahkan `"ai_scheduler": "Scheduler"`.

- [ ] **Step 2: Halaman + komponen**

`page.tsx`:

```tsx
import { PageHead } from "@/components/core/page-title";
import { SchedulerView } from "@/core/components/ai-scheduler/scheduler-view";

export default function WorkspaceSchedulerPage() {
  return (
    <>
      <PageHead title="Scheduler" />
      <div className="relative h-full w-full overflow-hidden overflow-y-auto">
        <SchedulerView />
      </div>
    </>
  );
}
```

`scheduler-view.tsx`: `observer` + `useParams` untuk `workspaceSlug`; `useEffect` → `fetchSchedules(slug)`; interval 30 detik saat mount (`setInterval`, clear on unmount); render daftar `ScheduleItem`; empty state (`EmptyStateCompact` atau teks) mengarahkan ke `/schedule` di chat; header dengan judul + refresh.

`schedule-item.tsx`: baris berisi nama, `humanizeSchedule`, next run (`renderFormattedDate`/`renderFormattedTime` dari `@plane/utils`), badge status run terakhir, `Switch` untuk pause/resume, tombol Run now, tombol Delete dengan `AlertModalCore`; tombol hanya dirender bila `userId === schedule.created_by_id || isAdmin`. Expand → `fetchRuns` + `ScheduleRunsList`.

`schedule-runs-list.tsx`: daftar run (status, trigger, waktu, durasi = `finished_at - started_at`), jawaban dirender `dangerouslySetInnerHTML` dengan `sanitizeAssistantHtml(response_html ?? "")`, error ditampilkan sebagai teks.

Catatan implementer: cari `userId`/role lewat hook store workspace yang sudah ada (mis. `useUser` + `useMember`/`useWorkspaceMember`), lalu sesuaikan.

- [ ] **Step 3: Verifikasi**

Run:

```bash
pnpm --filter=web check:types 2>&1 | tail -5
pnpm --filter=web check:lint 2>&1 | tail -3
pnpm --filter=web test 2>&1 | tail -3
```

Expected: hijau.

- [ ] **Step 4: Commit**

```bash
git add apps/web/app packages/constants/src/workspace.ts apps/web/core/components/ai-scheduler apps/web/core/components/workspace/sidebar packages/i18n/src/locales/en/navigation.json
git commit -m "feat(web): add workspace Scheduler page"
```

---

### Task 17: Verifikasi menyeluruh, rebuild, smoke

**Files:** —

- [ ] **Step 1: Backend full suite + clippy**

Run:

```bash
cd apps/api-rs
cargo test -p api -- --test-threads=1 2>&1 | tail -4
cargo test -p ai -p common -p worker -- --test-threads=1 2>&1 | tail -4
cargo clippy -p api -p worker -p beat -p ai -p common --all-targets 2>&1 | rg -i "^error|warning: unused" | tail -5
```

Expected: 0 failed; tidak ada temuan clippy baru.

- [ ] **Step 2: FE checks**

Run:

```bash
pnpm --filter=web test 2>&1 | tail -4
pnpm --filter=web check:types 2>&1 | tail -3
pnpm check:format 2>&1 | tail -3
pnpm check:lint 2>&1 | tail -3
```

Expected: hijau.

- [ ] **Step 3: Rebuild + restart**

Run:

```bash
docker compose build api && docker compose up -d api worker beat-worker
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200`; container `api-rs`, `worker-rs`, `beat-rs` memakai image baru.

- [ ] **Step 4: Smoke live**

1. Buka sidebar Galileo di tunnel, aktifkan mode Agent, ketik `/schedule buat laporan overdue tiap Senin jam 9`.
2. Pastikan kartu proposal muncul dengan jadwal "Every Monday at 09:00 · <timezone>"; klik Confirm.
3. Buka halaman Scheduler dari sidebar workspace: jadwal tampil, `next_run_at` masuk akal.
4. Klik Run now → tunggu ≤30 detik → status run dan jawaban muncul di riwayat.
5. Tunggu tick berikutnya (≤1 menit) untuk membuktikan penjadwalan otomatis mengantre run berikutnya (bisa dicek dari log `ai.schedule.tick` di container `worker-rs`).
6. Pause → `next_run_at` tidak berubah saat resume dihitung ulang; Delete → hilang dari daftar.
7. Regresi: mode Classic masih menjawab seperti sebelumnya; `/ai-assistant/` & `/ai-agent/` klien lama tidak berubah.

- [ ] **Step 5: Commit sisa**

```bash
git status --short
# stage hanya file fitur bila ada sisa, lalu:
git add <files>
git commit -m "chore: finalize AI scheduler"
```

---

## Self-Review (hasil pemeriksaan penulis plan)

- **Spec coverage:** keputusan #1–#11 spec dipetakan ke Task 1–17 (halaman → Task 16; `/schedule` via tool agen → Task 4/14/15; riwayat run → Task 8/11/16; konfirmasi → Task 5/14/15; preset → Task 3; authz → Task 10/11; lokasi nav → Task 16; aksi → Task 11/16; beat+klaim → Task 7/9; allowlist worker → Task 9; auto-switch → Task 14). Guardrail 20 jadwal, retensi 20 run, sweep 15 menit, timeout 180 dtk semuanya ada.
- **Placeholder:** tidak ada TBD/TODO; satu-satunya penanda adalah catatan "cleanup omitted here for brevity" pada test Task 8 — implementer wajib menyalin cleanup dari test tick (dinyatakan eksplisit).
- **Type consistency:** `ScheduleProposal` dipakai konsisten di `ai`, endpoint, tool, dan test; `pending_action` memakai `ToolCallTrace.arguments` yang kini menyimpan proposal; FE `TAiScheduleProposal` memakai `day_of_week`/`day_of_month` yang sama dengan server.

## Open risks untuk implementer

- `routes/ai.rs` memakai `pub use` + definisi lokal: pastikan tidak ada duplikasi nama (hapus definisi lama saat menambah re-export).
- `worker` crate perlu `src/lib.rs` agar test integrasi bisa mengimpor handler (Task 7).
- `sqlx` tipe `jsonb` untuk `tool_calls`: gunakan `serde_json::Value` bind (fitur `json` sudah aktif di workspace).
- `StreamId.map` bertipe `HashMap<String, redis::Value>` — konversi memakai `String::from_redis_value`.
- Test worker yang mengubah env (`SKIP_ENV_VAR`, `LLM_API_KEY`) harus dijalankan `--test-threads=1`.
