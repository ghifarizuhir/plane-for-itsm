# AI Schedule Recipe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Jadwal AI berubah dari satu baris `prompt` menjadi resep terstruktur wajib (`description`, `how_to`, `tools`, `expected_output`) yang diisi agen, dapat diedit user di kartu konfirmasi, divalidasi server, dan ditegakkan saat run (hard allowlist tool).

**Architecture:** Satu kolom `spec jsonb` (versi di dalam spec) di `ai_schedules`; `prompt` tetap diisi hasil render kanonik saat create sehingga worker/UI/history lama bekerja. Validasi dan render hidup di crate `ai` (pure, unit-tested). Worker memuat spec saat run, memvalidasi, lalu hanya mengekspos tool yang dideklarasikan lewat `read_tools`. Legacy (`spec IS NULL`) memakai prompt mentah + tiga read tool.

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), PostgreSQL (jsonb), Next.js + MobX + vitest.

**Spec:** `docs/superpowers/specs/2026-09-27-ai-schedule-recipe-design.md`

**Prasyarat:** stack lokal jalan (Postgres + Redis, lihat `docker-compose-local.yml`); perintah Rust dijalankan dari `apps/api-rs`; perintah web dari root repo. Tes DB: `cargo test -p api --test ai_schedule_test -- --test-threads=1` (butuh `DATABASE_URL`, default `postgres://plane:plane@localhost:5432/plane`).

---

## File Structure

- `apps/api-rs/crates/ai/src/schedule.rs` — `ScheduleSpec`, `ScheduleRecipe`, `render_schedule_prompt`, `allowed_read_tools`, `ScheduleProposal::with_prompt_limit`.
- `apps/api-rs/crates/ai/src/tools.rs` — `CreateScheduleArgs` baru, `recipe_from_args`, `read_tools`, `CreateSchedule` diperbarui.
- `apps/api-rs/crates/ai/src/agent.rs` — preamble.
- `apps/api-rs/migrations/0009_ai_schedule_spec.sql` — kolom `spec`.
- `apps/api-rs/crates/api/src/routes/ai_schedule.rs` — body create baru + jalur legacy + `spec` di JSON.
- `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs` — `allowed_tools` + `read_tools`.
- `apps/web/core/lib/ai-schedule.ts` — tipe spec, `validateScheduleSpec`, `isStructuredProposal`, `scheduleDescription`.
- `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx` — form editable + fallback legacy.
- `apps/web/core/store/ai-assistant.store.ts` — `confirmScheduleProposal(messageId, proposal?)`.
- `apps/web/core/components/ai/assistant-sidebar/root.tsx` — meneruskan proposal hasil edit.
- `apps/web/core/components/ai-scheduler/schedule-item.tsx` — description + badge tools + disclosure Recipe.

---

### Task 1: `ScheduleSpec`, `ScheduleRecipe`, render prompt, subset tool (crate `ai`)

**Files:**

- Modify: `apps/api-rs/crates/ai/src/schedule.rs`
- Test: unit test di file yang sama (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Tulis test yang gagal** — tambahkan di akhir `mod tests` pada `apps/api-rs/crates/ai/src/schedule.rs`, dan tambahkan `use serde_json::json;` di dalam `mod tests` (saat ini hanya `use super::*; use chrono::{TimeZone, Utc};`):

```rust
    #[test]
    fn spec_validation_bounds_and_trim() {
        let spec = ScheduleSpec::new(
            "  Do things  ",
            &[" Step one ".to_string()],
            &["list_projects".to_string()],
            " A report ",
        )
        .unwrap();
        assert_eq!(spec.version, SPEC_VERSION);
        assert_eq!(spec.description, "Do things");
        assert_eq!(spec.how_to, vec!["Step one".to_string()]);
        assert_eq!(spec.expected_output, "A report");

        assert!(ScheduleSpec::new("", &["s".to_string()], &["list_projects".to_string()], "o").is_err());
        assert!(ScheduleSpec::new("d", &[], &["list_projects".to_string()], "o").is_err());
        let too_many: Vec<String> = (0..11).map(|index| format!("step {index}")).collect();
        assert!(ScheduleSpec::new("d", &too_many, &["list_projects".to_string()], "o").is_err());
        assert!(ScheduleSpec::new("d", &["s".to_string()], &[], "o").is_err());
        assert!(ScheduleSpec::new("d", &["s".to_string()], &["list_projects".to_string()], "").is_err());
    }

    #[test]
    fn spec_tools_are_canonical_and_deduped() {
        let spec = ScheduleSpec::new(
            "d",
            &["s".to_string()],
            &[
                "search_work_items".to_string(),
                "list_projects".to_string(),
                "list_projects".to_string(),
            ],
            "o",
        )
        .unwrap();
        assert_eq!(
            spec.tools,
            vec!["list_projects".to_string(), "search_work_items".to_string()]
        );

        let err = ScheduleSpec::new("d", &["s".to_string()], &["drop_tables".to_string()], "o").unwrap_err();
        assert!(err.contains("subset"), "unexpected error: {err}");
    }

    #[test]
    fn spec_version_is_rejected_when_unknown() {
        let mut spec = ScheduleSpec::new("d", &["s".to_string()], &["list_projects".to_string()], "o").unwrap();
        spec.version = 2;
        assert!(spec.validated().unwrap_err().contains("version"));
    }

    #[test]
    fn render_prompt_is_deterministic() {
        let spec = ScheduleSpec::new(
            "Summarize overdue",
            &["Count overdue items".to_string(), "List the top 5".to_string()],
            &["list_projects".to_string(), "search_work_items".to_string()],
            "A short markdown list",
        )
        .unwrap();
        assert_eq!(
            render_schedule_prompt("Daily overdue", &spec),
            "Task: Daily overdue\n\nDescription:\nSummarize overdue\n\nSteps:\n1. Count overdue items\n2. List the top 5\n\nExpected output:\nA short markdown list\n\nAllowed tools: list_projects, search_work_items"
        );
    }

    #[test]
    fn recipe_flattens_spec_and_preset() {
        let recipe = ScheduleRecipe::new(
            "Daily",
            "desc",
            &["step".to_string()],
            &["list_projects".to_string()],
            "out",
            "daily",
            Some("09:00"),
            None,
            None,
            Some("UTC"),
        )
        .unwrap();
        let value = serde_json::to_value(&recipe).unwrap();
        assert_eq!(value["version"], json!(1));
        assert_eq!(value["description"], json!("desc"));
        assert_eq!(value["how_to"][0], json!("step"));
        assert_eq!(value["tools"][0], json!("list_projects"));
        assert_eq!(value["expected_output"], json!("out"));
        assert_eq!(value["name"], json!("Daily"));
        assert_eq!(value["frequency"], json!("daily"));
        assert_eq!(
            value["prompt"],
            json!("Task: Daily\n\nDescription:\ndesc\n\nSteps:\n1. step\n\nExpected output:\nout\n\nAllowed tools: list_projects")
        );
    }

    #[test]
    fn recipe_prompt_limit_is_enforced() {
        let long = "x".repeat(SPEC_PROMPT_MAX + 1);
        let err = ScheduleProposal::with_prompt_limit(
            "n",
            &long,
            "daily",
            None,
            None,
            None,
            Some("UTC"),
            SPEC_PROMPT_MAX,
        )
        .unwrap_err();
        assert!(err.contains("8000"), "unexpected error: {err}");
    }

    #[test]
    fn allowed_read_tools_follows_canonical_order() {
        assert_eq!(
            allowed_read_tools(&["search_work_items".to_string(), "list_projects".to_string()]),
            vec!["list_projects", "search_work_items"]
        );
        assert!(allowed_read_tools(&[]).is_empty());
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai schedule::`
Expected: FAIL kompilasi — `cannot find function render_schedule_prompt`, `cannot find type ScheduleSpec`, `no function with_prompt_limit`, dst.

- [ ] **Step 3: Implementasi** — di `apps/api-rs/crates/ai/src/schedule.rs`:

(a) Tambahkan konstanta setelah `pub const DEFAULT_TIMEZONE: &str = "UTC";`:

```rust
pub const SPEC_VERSION: u8 = 1;
pub const SPEC_TOOLS: [&str; 3] = ["list_projects", "count_work_items", "search_work_items"];
pub const SPEC_DESCRIPTION_MAX: usize = 500;
pub const SPEC_STEPS_MAX: usize = 10;
pub const SPEC_STEP_MAX: usize = 500;
pub const SPEC_EXPECTED_OUTPUT_MAX: usize = 1000;
pub const SPEC_PROMPT_MAX: usize = 8000;
```

(b) Tambahkan setelah blok `ScheduleProposal` (setelah `impl ScheduleProposal { ... }` selesai, sebelum `fn local_at`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleSpec {
    pub version: u8,
    pub description: String,
    pub how_to: Vec<String>,
    pub tools: Vec<String>,
    pub expected_output: String,
}

fn bounded_text(field: &str, value: &str, max: usize) -> Result<String, String> {
    let trimmed = value.trim();
    let length = trimmed.chars().count();
    if length == 0 || length > max {
        return Err(format!("{field} must be 1-{max} characters"));
    }
    Ok(trimmed.to_string())
}

impl ScheduleSpec {
    pub fn new(
        description: &str,
        how_to: &[String],
        tools: &[String],
        expected_output: &str,
    ) -> Result<Self, String> {
        Self {
            version: SPEC_VERSION,
            description: description.to_string(),
            how_to: how_to.to_vec(),
            tools: tools.to_vec(),
            expected_output: expected_output.to_string(),
        }
        .validated()
    }

    /// Re-check a spec parsed from storage: version, bounds, and the tool
    /// allowlist; returns the canonical form (trimmed, deduped tools in
    /// `SPEC_TOOLS` order).
    pub fn validated(mut self) -> Result<Self, String> {
        if self.version != SPEC_VERSION {
            return Err(format!("unsupported spec version: {}", self.version));
        }
        self.description = bounded_text("description", &self.description, SPEC_DESCRIPTION_MAX)?;
        if self.how_to.is_empty() || self.how_to.len() > SPEC_STEPS_MAX {
            return Err(format!("how_to must have 1-{SPEC_STEPS_MAX} steps"));
        }
        self.how_to = self
            .how_to
            .iter()
            .map(|step| bounded_text("how_to step", step, SPEC_STEP_MAX))
            .collect::<Result<Vec<_>, _>>()?;
        if self.tools.is_empty() {
            return Err(format!(
                "tools must include at least one of: {}",
                SPEC_TOOLS.join(", ")
            ));
        }
        if let Some(unknown) = self
            .tools
            .iter()
            .map(|tool| tool.trim())
            .find(|tool| !SPEC_TOOLS.contains(tool))
        {
            return Err(format!(
                "tools must be a subset of: {} (unknown: {unknown})",
                SPEC_TOOLS.join(", ")
            ));
        }
        self.tools = SPEC_TOOLS
            .iter()
            .filter(|known| self.tools.iter().any(|tool| tool.trim() == **known))
            .map(|known| (*known).to_string())
            .collect();
        self.expected_output = bounded_text(
            "expected_output",
            &self.expected_output,
            SPEC_EXPECTED_OUTPUT_MAX,
        )?;
        Ok(self)
    }
}

/// Canonical subset of `SPEC_TOOLS` in declaration order.
pub fn allowed_read_tools(allowed: &[String]) -> Vec<&'static str> {
    SPEC_TOOLS
        .iter()
        .copied()
        .filter(|name| allowed.iter().any(|tool| tool == name))
        .collect()
}

/// Deterministic prompt rendering for a structured recipe.
pub fn render_schedule_prompt(name: &str, spec: &ScheduleSpec) -> String {
    let steps = spec
        .how_to
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {step}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Task: {}\n\nDescription:\n{}\n\nSteps:\n{}\n\nExpected output:\n{}\n\nAllowed tools: {}",
        name.trim(),
        spec.description,
        steps,
        spec.expected_output,
        spec.tools.join(", ")
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleRecipe {
    #[serde(flatten)]
    pub spec: ScheduleSpec,
    #[serde(flatten)]
    pub proposal: ScheduleProposal,
}

impl ScheduleRecipe {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        description: &str,
        how_to: &[String],
        tools: &[String],
        expected_output: &str,
        frequency: &str,
        time: Option<&str>,
        day_of_week: Option<i16>,
        day_of_month: Option<i16>,
        timezone: Option<&str>,
    ) -> Result<Self, String> {
        let spec = ScheduleSpec::new(description, how_to, tools, expected_output)?;
        let prompt = render_schedule_prompt(name, &spec);
        let proposal = ScheduleProposal::with_prompt_limit(
            name,
            &prompt,
            frequency,
            time,
            day_of_week,
            day_of_month,
            timezone,
            SPEC_PROMPT_MAX,
        )?;
        Ok(Self { spec, proposal })
    }
}
```

(c) Refactor `ScheduleProposal::new` menjadi delegasi ke `with_prompt_limit`. Ganti awal `impl ScheduleProposal {` (fungsi `new`) sehingga menjadi:

```rust
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
        Self::with_prompt_limit(
            name,
            prompt,
            frequency,
            time,
            day_of_week,
            day_of_month,
            timezone,
            2000,
        )
    }

    /// Same as [`ScheduleProposal::new`] but with a caller-chosen prompt cap:
    /// rendered schedule recipes may exceed the legacy 2000 character limit.
    #[allow(clippy::too_many_arguments)]
    pub fn with_prompt_limit(
        name: &str,
        prompt: &str,
        frequency: &str,
        time: Option<&str>,
        day_of_week: Option<i16>,
        day_of_month: Option<i16>,
        timezone: Option<&str>,
        prompt_max: usize,
    ) -> Result<Self, String> {
```

lalu di dalam body, ganti dua baris validasi prompt:

```rust
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() || prompt.chars().count() > prompt_max {
            return Err(format!("prompt must be 1-{prompt_max} characters"));
        }
```

Sisa body `new` yang lama (validasi name/frequency/time/day/timezone) tetap apa adanya.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai schedule::`
Expected: PASS semua (termasuk tes lama `rejects_bad_frequency_time_and_timezone`, `daily_uses_proposal_timezone`, dst).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/schedule.rs
git commit -m "feat(ai): add schedule spec, recipe, and canonical prompt rendering"
```

---

### Task 2: `create_schedule` berkerangka + `read_tools` (crate `ai`)

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools.rs`
- Modify: `apps/api-rs/crates/ai/tests/agent_pending_action.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs` (fixture upstream)

- [ ] **Step 1: Tulis test yang gagal** — ganti lima test lama di `mod tests` `apps/api-rs/crates/ai/src/tools.rs` (`create_schedule_proposal_normalizes_defaults`, `create_schedule_proposal_rejects_bad_args`, `create_schedule_tool_records_proposal`, `create_schedule_proposal_hourly_default_and_rejections`, `rejected_create_schedule_is_not_traced`) dengan versi berikut, dan tambahkan satu test baru:

```rust
    #[test]
    fn create_schedule_recipe_normalizes_defaults() {
        let recipe = recipe_from_args(CreateScheduleArgs {
            name: " Daily overdue ".to_string(),
            description: " Summarize overdue work ".to_string(),
            how_to: vec![" Count overdue items ".to_string()],
            tools: vec!["count_work_items".to_string()],
            expected_output: " A short list ".to_string(),
            frequency: "weekly".to_string(),
            time: None,
            day_of_week: Some(1),
            day_of_month: None,
            timezone: Some("Asia/Jakarta".to_string()),
        })
        .expect("valid args");
        assert_eq!(recipe.proposal.name, "Daily overdue");
        assert_eq!(recipe.spec.description, "Summarize overdue work");
        assert_eq!(recipe.proposal.time, "09:00");
        assert_eq!(recipe.proposal.timezone, "Asia/Jakarta");
        assert_eq!(recipe.proposal.day_of_week, Some(1));
        assert_eq!(recipe.spec.tools, vec!["count_work_items".to_string()]);
    }

    #[test]
    fn create_schedule_recipe_rejects_bad_args() {
        let err = recipe_from_args(CreateScheduleArgs {
            name: "x".to_string(),
            description: "d".to_string(),
            how_to: vec!["s".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "o".to_string(),
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
    async fn create_schedule_tool_records_recipe() {
        let trace = crate::agent::new_trace();
        let tool = CreateSchedule {
            trace: trace.clone(),
        };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateScheduleArgs {
                    name: "Daily".to_string(),
                    description: "Report".to_string(),
                    how_to: vec!["Count overdue".to_string()],
                    tools: vec!["count_work_items".to_string()],
                    expected_output: "Summary".to_string(),
                    frequency: "daily".to_string(),
                    time: Some("08:00".to_string()),
                    day_of_week: None,
                    day_of_month: None,
                    timezone: Some("UTC".to_string()),
                },
            )
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("recipe json");
        assert_eq!(parsed["frequency"], json!("daily"));
        assert_eq!(parsed["time"], json!("08:00"));
        assert_eq!(parsed["tools"][0], json!("count_work_items"));
        assert_eq!(parsed["how_to"][0], json!("Count overdue"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "create_schedule");
        assert_eq!(recorded[0].arguments["time"], json!("08:00"));
        assert_eq!(recorded[0].arguments["description"], json!("Report"));
    }

    #[test]
    fn create_schedule_recipe_hourly_default_and_rejections() {
        let hourly = recipe_from_args(CreateScheduleArgs {
            name: "Hourly".to_string(),
            description: "Check".to_string(),
            how_to: vec!["Check".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "Notes".to_string(),
            frequency: "hourly".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: None,
        })
        .expect("valid args");
        assert_eq!(hourly.proposal.time, "00:00");
        assert_eq!(hourly.proposal.timezone, "UTC");

        let bad_tz = recipe_from_args(CreateScheduleArgs {
            name: "x".to_string(),
            description: "d".to_string(),
            how_to: vec!["s".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "o".to_string(),
            frequency: "daily".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: Some("Mars/Olympus".to_string()),
        });
        assert!(bad_tz.is_err());

        let monthly_without_day = recipe_from_args(CreateScheduleArgs {
            name: "x".to_string(),
            description: "d".to_string(),
            how_to: vec!["s".to_string()],
            tools: vec!["list_projects".to_string()],
            expected_output: "o".to_string(),
            frequency: "monthly".to_string(),
            time: None,
            day_of_week: None,
            day_of_month: None,
            timezone: None,
        });
        assert!(monthly_without_day.is_err());
    }

    #[tokio::test]
    async fn rejected_create_schedule_is_not_traced() {
        let trace = crate::agent::new_trace();
        let tool = CreateSchedule {
            trace: trace.clone(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateScheduleArgs {
                    name: "x".to_string(),
                    description: "d".to_string(),
                    how_to: vec!["s".to_string()],
                    tools: vec![],
                    expected_output: "o".to_string(),
                    frequency: "daily".to_string(),
                    time: None,
                    day_of_week: None,
                    day_of_month: None,
                    timezone: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("tools"));
        assert!(trace.lock().unwrap().is_empty());
        assert!(crate::agent::pending_action(&trace).is_none());
    }

    #[tokio::test]
    async fn read_tools_accepts_a_subset() {
        let _handle = read_tools(
            lazy_pool(),
            Uuid::nil(),
            crate::agent::new_trace(),
            &["list_projects"],
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai tools::`
Expected: FAIL kompilasi — `CreateScheduleArgs` tidak punya field `description`, `cannot find function recipe_from_args`, `cannot find function read_tools`.

- [ ] **Step 3: Implementasi** — di `apps/api-rs/crates/ai/src/tools.rs`:

(a) Ganti import `use crate::schedule::ScheduleProposal;` menjadi:

```rust
use crate::schedule::ScheduleRecipe;
```

(b) Ganti seluruh struct `CreateScheduleArgs` dan fungsi `proposal_from_args` dengan:

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateScheduleArgs {
    /// Short human-readable schedule name (1-120 characters), e.g. "Daily overdue report".
    pub name: String,
    /// What the schedule is for: one or two sentences of context (1-500 characters).
    pub description: String,
    /// Ordered, concrete steps the agent must follow on every fire (1-10 steps, each 1-500 characters).
    pub how_to: Vec<String>,
    /// Tools the run may use: at least one of list_projects, count_work_items, search_work_items.
    pub tools: Vec<String>,
    /// What the result should contain, e.g. "a markdown table of overdue items with owner and due date" (1-1000 characters).
    pub expected_output: String,
    /// One of: hourly, daily, weekly, monthly.
    pub frequency: String,
    /// Time of day "HH:MM" (24h). For hourly only the minutes are used. Defaults to 09:00 (00:00 for hourly).
    pub time: Option<String>,
    /// For weekly schedules: 1 = Monday … 7 = Sunday.
    pub day_of_week: Option<i16>,
    /// For monthly schedules: day of month, 1-31. Short months clamp to the last day.
    pub day_of_month: Option<i16>,
    /// IANA timezone, e.g. "Asia/Jakarta". Defaults to UTC when omitted; unknown zones are rejected.
    pub timezone: Option<String>,
}

/// Validate raw tool args into a normalized recipe (defaults applied, prompt rendered).
pub fn recipe_from_args(args: CreateScheduleArgs) -> Result<ScheduleRecipe, ToolExecutionError> {
    ScheduleRecipe::new(
        &args.name,
        &args.description,
        &args.how_to,
        &args.tools,
        &args.expected_output,
        &args.frequency,
        args.time.as_deref(),
        args.day_of_week,
        args.day_of_month,
        args.timezone.as_deref(),
    )
    .map_err(ToolExecutionError::invalid_args)
}
```

(c) Perbarui deskripsi dan `call` tool `CreateSchedule`:

```rust
    fn description(&self) -> String {
        "Propose a recurring scheduled task for this workspace. Only call this when the user explicitly asks for a recurring or scheduled task (for example a message starting with /schedule), and only after the recipe is complete: description, ordered how_to steps, the tools it needs (at least one of list_projects, count_work_items, search_work_items), expected_output, and the frequency. The user must confirm and may edit every field in the UI before anything is saved. Never claim the schedule exists until they confirm.".to_string()
    }
```

```rust
    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let recipe = recipe_from_args(args)?;
        record(&self.trace, Self::NAME, &recipe);
        Ok(serde_json::to_string(&recipe).expect("ScheduleRecipe serializes"))
    }
```

(d) Tambahkan `read_tools` setelah `workspace_tools`:

```rust
/// Build the schedule-run tool server: only the allowed read tools, never
/// `create_schedule`. Unknown names are ignored (specs are validated before
/// this is called).
pub fn read_tools(
    pool: PgPool,
    workspace_id: Uuid,
    trace: ToolTrace,
    allowed: &[&str],
) -> rig::tool::server::ToolServerHandle {
    let mut server = rig::tool::server::ToolServer::new();
    if allowed.contains(&ListProjects::NAME) {
        server = server.tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&CountWorkItems::NAME) {
        server = server.tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&SearchWorkItems::NAME) {
        server = server.tool(SearchWorkItems {
            pool,
            workspace_id,
            trace: trace.clone(),
        });
    }
    server.run()
}
```

- [ ] **Step 4: Perbarui integration test yang memakai args lama**

`apps/api-rs/crates/ai/tests/agent_pending_action.rs` — ganti isi `CreateScheduleArgs` (baris 17–25) menjadi:

```rust
        CreateScheduleArgs {
            name: "Weekly backlog".to_string(),
            description: "Summarize the backlog every week".to_string(),
            how_to: vec!["Count backlog items".to_string()],
            tools: vec!["count_work_items".to_string()],
            expected_output: "A short summary".to_string(),
            frequency: "weekly".to_string(),
            time: Some("09:00".to_string()),
            day_of_week: Some(1),
            day_of_month: None,
            timezone: Some("Asia/Jakarta".to_string()),
        },
```

dan tambahkan assertion setelah baris 33:

```rust
    assert_eq!(action["proposal"]["tools"][0], json!("count_work_items"));
```

`apps/api-rs/crates/api/tests/ai_agent_test.rs` — di `schedule_handler` (baris 271), ganti string `arguments` menjadi:

```rust
                                    "arguments": "{\"name\":\"Daily\",\"description\":\"Report\",\"how_to\":[\"Count overdue\"],\"tools\":[\"count_work_items\"],\"expected_output\":\"A summary\",\"frequency\":\"daily\",\"time\":\"09:00\",\"timezone\":\"UTC\"}"
```

dan di test `create_schedule_roundtrip_surfaces_pending_action` tambahkan setelah baris 368:

```rust
        assert_eq!(action["proposal"]["description"], json!("Report"));
        assert_eq!(action["proposal"]["tools"][0], json!("count_work_items"));
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: PASS semua (unit + `agent_pending_action`, `agent_history_test`).

Run: `cargo test -p api --test ai_agent_test`
Expected: PASS — `create_schedule_roundtrip_surfaces_pending_action` dan `agent_proposal_metadata_is_persisted_and_returned` lulus (butuh DB untuk yang terakhir).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools.rs apps/api-rs/crates/ai/tests/agent_pending_action.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(ai): create_schedule now proposes a structured recipe with read_tools allowlist"
```

---

### Task 3: Preamble chat agent

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Ganti kalimat `/schedule` di `PREAMBLE`**

Ganti seluruh string `pub const PREAMBLE` (baris 45–55) menjadi:

```rust
pub const PREAMBLE: &str = "You are the workspace AI assistant for Plane. \
Answer factual questions about projects and work items by calling the provided \
tools; never invent project identifiers, work item identifiers, counts, or \
states. All tools are scoped to the user's current workspace and read-only, \
except create_schedule, which only proposes a schedule and never saves \
anything. If a tool returns no results, say so. Answer concisely in the \
user's language. When the user's message starts with /schedule they want a \
recurring scheduled task. A schedule is a recipe, not a one-line command: \
gather anything unclear first, then call create_schedule once with a complete \
recipe — description, ordered how_to steps, the tools it needs (at least one \
of list_projects, count_work_items, search_work_items), expected_output, and \
how often. Tell the user they can edit every field in the confirmation card. \
The schedule is only created after the user confirms the proposal card, so \
never say it is already created.";
```

- [ ] **Step 2: Verifikasi kompilasi + test crate**

Run: `cargo test -p ai`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): teach the agent to propose complete schedule recipes"
```

---

### Task 4: Migrasi kolom `spec`

**Files:**

- Create: `apps/api-rs/migrations/0009_ai_schedule_spec.sql`

- [ ] **Step 1: Tulis migrasi**

```sql
-- AI schedule recipe: structured how_to skeleton on ai_schedules.
-- Legacy rows keep spec = NULL and run their raw prompt with every read tool.

ALTER TABLE public.ai_schedules ADD COLUMN IF NOT EXISTS spec jsonb;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ai_schedules_spec_object_check') THEN
        ALTER TABLE public.ai_schedules ADD CONSTRAINT ai_schedules_spec_object_check
            CHECK (spec IS NULL OR jsonb_typeof(spec) = 'object');
    END IF;
END
$$;
```

- [ ] **Step 2: Verifikasi migrasi jalan** (butuh Postgres lokal)

Run: `psql "$DATABASE_URL" -f apps/api-rs/migrations/0009_ai_schedule_spec.sql`
Expected: `ALTER TABLE`; `DO`. Jalankan sekali lagi → `ALTER TABLE` (idempoten, tanpa error `already exists`).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/0009_ai_schedule_spec.sql
git commit -m "feat(api-rs): add ai_schedules.spec jsonb column"
```

---

### Task 5: Endpoint create menerima resep + `spec` di JSON

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_schedule.rs`
- Test: `apps/api-rs/crates/api/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal** — tambahkan helper `structured_body` setelah `create_body` (baris 44) dan tiga test baru di `apps/api-rs/crates/api/tests/ai_schedule_test.rs`:

```rust
fn structured_body(proposal_key: Uuid) -> Value {
    json!({
        "name": "Daily report",
        "description": "Summarize overdue work items for the team",
        "how_to": ["Count overdue work items", "List the top five by priority"],
        "tools": ["count_work_items", "search_work_items"],
        "expected_output": "A short markdown list with identifiers and owners",
        "frequency": "daily",
        "time": "09:00",
        "timezone": "UTC",
        "proposal_key": proposal_key,
    })
}
```

```rust
#[tokio::test]
async fn create_with_spec_stores_rendered_prompt_and_returns_spec() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let (status, Json(created)) = ai_schedule::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(structured_body(Uuid::new_v4())),
    )
    .await
    .expect("create ok");
    assert_eq!(status, StatusCode::CREATED);
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (prompt, spec): (String, Value) =
        sqlx::query_as("SELECT prompt, spec FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        prompt,
        "Task: Daily report\n\nDescription:\nSummarize overdue work items for the team\n\nSteps:\n1. Count overdue work items\n2. List the top five by priority\n\nExpected output:\nA short markdown list with identifiers and owners\n\nAllowed tools: count_work_items, search_work_items"
    );
    assert_eq!(spec["version"], json!(1));
    assert_eq!(
        spec["tools"],
        json!(["count_work_items", "search_work_items"])
    );

    let (status, Json(detail)) = ai_schedule::detail(
        State(st),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("detail ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        detail["spec"]["description"],
        json!("Summarize overdue work items for the team")
    );
    assert_eq!(
        detail["spec"]["how_to"][0],
        json!("Count overdue work items")
    );

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_rejects_incomplete_spec_and_legacy_keeps_spec_null() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(err)) = ai_schedule::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "name": "Half",
            "description": "Only description",
            "frequency": "daily",
            "time": "09:00",
            "timezone": "UTC",
            "proposal_key": Uuid::new_v4(),
        })),
    )
    .await
    .expect("create handled");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"], json!("incomplete schedule spec"));

    let (status, Json(created)) = ai_schedule::create(
        State(st),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body(Uuid::new_v4())),
    )
    .await
    .expect("legacy create ok");
    assert_eq!(status, StatusCode::CREATED);
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let spec: Option<Value> = sqlx::query_scalar("SELECT spec FROM ai_schedules WHERE id = $1")
        .bind(schedule_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(spec.is_none(), "legacy rows keep a NULL spec");

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn resume_recomputes_next_run_for_a_long_recipe() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let long_step = "x".repeat(500);
    let (status, Json(created)) = ai_schedule::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "name": "Long recipe",
            "description": "A recipe whose rendered prompt exceeds the legacy 2000 character cap",
            "how_to": [
                long_step.clone(),
                long_step.clone(),
                long_step.clone(),
                long_step.clone(),
                long_step
            ],
            "tools": ["list_projects"],
            "expected_output": "Anything",
            "frequency": "daily",
            "time": "09:00",
            "timezone": "UTC",
            "proposal_key": Uuid::new_v4(),
        })),
    )
    .await
    .expect("create ok");
    assert_eq!(status, StatusCode::CREATED);
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (status, _) = ai_schedule::patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": false})),
    )
    .await
    .expect("pause ok");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = ai_schedule::patch(
        State(st),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": true})),
    )
    .await
    .expect("resume ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], json!(true));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --test ai_schedule_test -- --test-threads=1`
Expected: FAIL kompilasi — `CreateScheduleBody` belum punya field `description`/`how_to`/`tools`/`expected_output`, `ScheduleRow` belum punya `spec`, `ai::schedule::ScheduleRecipe` belum diimpor.

- [ ] **Step 3: Implementasi** — di `apps/api-rs/crates/api/src/routes/ai_schedule.rs`:

(a) Ganti import baris 11:

```rust
use ai::schedule::{ScheduleProposal, ScheduleRecipe, SPEC_PROMPT_MAX};
```

(b) Ganti `CreateScheduleBody` (baris 37–51) menjadi:

```rust
#[derive(serde::Deserialize)]
pub struct CreateScheduleBody {
    pub name: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub how_to: Option<Vec<String>>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub expected_output: Option<String>,
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

/// Validated create payload, normalized from either the structured recipe or
/// the legacy prompt-only shape.
struct ResolvedSchedule {
    name: String,
    prompt: String,
    spec: Option<Value>,
    frequency: String,
    time: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
}

enum ScheduleInput {
    Recipe(ScheduleRecipe),
    Legacy(ScheduleProposal),
}

impl ScheduleInput {
    fn next_run_at(&self) -> chrono::DateTime<chrono::Utc> {
        let proposal = match self {
            ScheduleInput::Recipe(recipe) => &recipe.proposal,
            ScheduleInput::Legacy(proposal) => proposal,
        };
        proposal.next_occurrence(chrono::Utc::now())
    }
}

impl From<ScheduleInput> for ResolvedSchedule {
    fn from(input: ScheduleInput) -> Self {
        match input {
            ScheduleInput::Recipe(recipe) => Self {
                name: recipe.proposal.name,
                prompt: recipe.proposal.prompt,
                spec: serde_json::to_value(&recipe.spec).ok(),
                frequency: recipe.proposal.frequency,
                time: recipe.proposal.time,
                day_of_week: recipe.proposal.day_of_week,
                day_of_month: recipe.proposal.day_of_month,
                timezone: recipe.proposal.timezone,
            },
            ScheduleInput::Legacy(proposal) => Self {
                name: proposal.name,
                prompt: proposal.prompt,
                spec: None,
                frequency: proposal.frequency,
                time: proposal.time,
                day_of_week: proposal.day_of_week,
                day_of_month: proposal.day_of_month,
                timezone: proposal.timezone,
            },
        }
    }
}

/// Structured when all four spec fields are present, legacy when none are;
/// anything in between is rejected.
fn schedule_input(body: &CreateScheduleBody) -> Result<ScheduleInput, String> {
    let spec_fields = [
        body.description.is_some(),
        body.how_to.is_some(),
        body.tools.is_some(),
        body.expected_output.is_some(),
    ];
    match spec_fields.iter().filter(|present| **present).count() {
        0 => {
            let prompt = body
                .prompt
                .as_deref()
                .filter(|prompt| !prompt.trim().is_empty())
                .ok_or_else(|| "prompt is required".to_string())?;
            ScheduleProposal::new(
                &body.name,
                prompt,
                &body.frequency,
                body.time.as_deref(),
                body.day_of_week,
                body.day_of_month,
                body.timezone.as_deref(),
            )
            .map(ScheduleInput::Legacy)
        }
        4 => ScheduleRecipe::new(
            &body.name,
            body.description.as_deref().unwrap_or_default(),
            body.how_to.as_deref().unwrap_or_default(),
            body.tools.as_deref().unwrap_or_default(),
            body.expected_output.as_deref().unwrap_or_default(),
            &body.frequency,
            body.time.as_deref(),
            body.day_of_week,
            body.day_of_month,
            body.timezone.as_deref(),
        )
        .map(ScheduleInput::Recipe),
        _ => Err("incomplete schedule spec".to_string()),
    }
}
```

(c) `ScheduleRow` (baris 53–68) — tambahkan `spec`:

```rust
#[derive(sqlx::FromRow)]
struct ScheduleRow {
    id: Uuid,
    workspace_id: Uuid,
    created_by_id: Uuid,
    name: String,
    prompt: String,
    spec: Option<Value>,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
    enabled: bool,
    next_run_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
}
```

(d) `schedule_json` (baris 101–116) — tambahkan `"spec": row.spec,` setelah `"prompt": row.prompt,`.

(e) Di `list` (baris 164–169) dan `load_schedule` (baris 133–140), tambahkan `spec` ke daftar SELECT: `... s.name, s.prompt, s.spec, s.frequency, ...`.

(f) Di handler `create`, ganti blok konstruksi proposal (baris 220–233) menjadi:

```rust
    let input = match schedule_input(&body) {
        Ok(input) => input,
        Err(message) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    };
```

lalu ganti blok `let next_run_at = ...; let id = ...; let inserted = ...` (baris 264–286) menjadi:

```rust
    let next_run_at = input.next_run_at();
    let resolved: ResolvedSchedule = input.into();
    let id = Uuid::new_v4();
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, spec, frequency, \
         time_of_day, day_of_week, day_of_month, timezone, enabled, next_run_at, proposal_key, \
         created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, true, $12, $13, now(), now()) \
         ON CONFLICT (workspace_id, proposal_key) WHERE deleted_at IS NULL DO NOTHING RETURNING id",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(auth.0)
    .bind(&resolved.name)
    .bind(&resolved.prompt)
    .bind(&resolved.spec)
    .bind(&resolved.frequency)
    .bind(&resolved.time)
    .bind(resolved.day_of_week)
    .bind(resolved.day_of_month)
    .bind(&resolved.timezone)
    .bind(next_run_at)
    .bind(body.proposal_key)
    .fetch_optional(&st.pool)
    .await?;
```

(g) Di `patch`, ganti `ScheduleProposal::new(...)` (baris 364–372) menjadi `with_prompt_limit` agar schedule ber-prompt panjang bisa di-resume:

```rust
        match ScheduleProposal::with_prompt_limit(
            &row.name,
            &row.prompt,
            &row.frequency,
            Some(&row.time_of_day),
            row.day_of_week,
            row.day_of_month,
            Some(&row.timezone),
            SPEC_PROMPT_MAX,
        ) {
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --test ai_schedule_test -- --test-threads=1`
Expected: PASS semua, termasuk test lama (body legacy tetap diterima).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_schedule.rs apps/api-rs/crates/api/tests/ai_schedule_test.rs
git commit -m "feat(api): accept structured schedule recipes and expose spec"
```

---

### Task 6: Worker menegakkan allowlist dari spec

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`
- Test: `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

(a) Unit test — tambahkan di akhir `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::allowed_tools;
    use serde_json::json;

    #[test]
    fn allowed_tools_legacy_is_all_read_tools() {
        assert_eq!(
            allowed_tools(None).unwrap(),
            vec![
                "list_projects".to_string(),
                "count_work_items".to_string(),
                "search_work_items".to_string()
            ]
        );
    }

    #[test]
    fn allowed_tools_uses_validated_spec_subset() {
        let spec = json!({
            "version": 1,
            "description": "d",
            "how_to": ["step"],
            "tools": ["search_work_items", "list_projects"],
            "expected_output": "o"
        });
        assert_eq!(
            allowed_tools(Some(spec)).unwrap(),
            vec!["list_projects".to_string(), "search_work_items".to_string()]
        );
    }

    #[test]
    fn allowed_tools_rejects_invalid_spec() {
        let spec = json!({
            "version": 1,
            "description": "d",
            "how_to": [],
            "tools": [],
            "expected_output": "o"
        });
        assert!(allowed_tools(Some(spec)).is_err());
    }
}
```

(b) Integration test — tambahkan di `apps/api-rs/crates/worker/tests/ai_schedule_test.rs` (setelah test `run_marks_failed_when_llm_is_not_configured`, memakai pola seeding yang sama):

```rust
#[tokio::test]
async fn run_fails_on_invalid_spec_before_llm() {
    let pool = pool().await;
    let slug = format!("aisb-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Bad Spec', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, spec, frequency, \
         time_of_day, timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily', 'Summarize', \
         '{\"version\": 1, \"description\": \"d\", \"how_to\": [], \"tools\": [], \"expected_output\": \"o\"}'::jsonb, \
         'daily', '09:00', 'UTC', true, now(), $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize', now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    // No LLM env needed: the invalid spec must fail before config resolution.
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id }))
        .await
        .expect("run handled");

    let (status, error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM ai_schedule_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("invalid schedule spec"));

    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p worker allowed_tools`
Expected: FAIL kompilasi — `cannot find function allowed_tools`.

Run: `cargo test -p worker --test ai_schedule_test run_fails_on_invalid_spec_before_llm -- --test-threads=1`
Expected: FAIL — run sukses/`not configured`, bukan `invalid schedule spec` (worker masih memakai `workspace_tools`).

- [ ] **Step 3: Implementasi** — di `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`:

(a) Tambahkan fungsi setelah `impl DueSchedule`:

```rust
/// Tools a run may use: the validated spec subset, or every read tool for
/// legacy rows (`spec IS NULL`).
fn allowed_tools(spec: Option<Value>) -> Result<Vec<String>, String> {
    match spec {
        Some(value) => serde_json::from_value::<ai::schedule::ScheduleSpec>(value)
            .map_err(|error| error.to_string())
            .and_then(|spec| spec.validated())
            .map(|spec| spec.tools),
        None => Ok(ai::schedule::SPEC_TOOLS
            .iter()
            .map(|name| name.to_string())
            .collect()),
    }
}
```

(b) Di `run()`, setelah blok `let Some(run) = claimed else { ... };` (baris 151–154) dan sebelum `let config = ai::resolve_llm_config(pool).await;`, sisipkan:

```rust
    let spec_value: Option<Value> = sqlx::query_scalar("SELECT spec FROM ai_schedules WHERE id = $1")
        .bind(run.schedule_id)
        .fetch_optional(pool)
        .await?
        .flatten();
    let allowed = match allowed_tools(spec_value) {
        Ok(allowed) => allowed,
        Err(error) => {
            tracing::error!(run_id=%run.id, error=%error, "ai.schedule.run: invalid schedule spec");
            finish_failed(pool, run.id, "invalid schedule spec").await?;
            prune_runs(pool, run.schedule_id).await?;
            return Ok(());
        }
    };
```

(c) Ganti baris 164:

```rust
    let handle = ai::tools::workspace_tools(pool.clone(), run.workspace_id, trace.clone());
```

menjadi:

```rust
    let allowed_refs: Vec<&str> = allowed.iter().map(String::as_str).collect();
    let handle = ai::tools::read_tools(pool.clone(), run.workspace_id, trace.clone(), &allowed_refs);
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p worker allowed_tools`
Expected: PASS (3 test).

Run: `cargo test -p worker --test ai_schedule_test -- --test-threads=1`
Expected: PASS semua, termasuk `run_marks_failed_when_llm_is_not_configured` (legacy spec NULL) dan `run_success_records_response_and_prunes`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/api-rs/crates/worker/tests/ai_schedule_test.rs
git commit -m "feat(worker): enforce schedule tool allowlist from the stored recipe"
```

---

### Task 7: FE types, validator, helper deskripsi

**Files:**

- Modify: `apps/web/core/lib/ai-schedule.ts`
- Test: `apps/web/core/lib/ai-schedule.test.ts`

- [ ] **Step 1: Tulis test yang gagal** — tambahkan di `apps/web/core/lib/ai-schedule.test.ts` (tambahkan import `isStructuredProposal`, `scheduleDescription`, `validateScheduleSpec` di baris 2):

```ts
import type { TAiScheduleSpec } from "./ai-schedule";

const validSpec: TAiScheduleSpec = {
  version: 1,
  description: "Summarize overdue work",
  how_to: ["Count overdue items"],
  tools: ["count_work_items"],
  expected_output: "A short list",
};

describe("validateScheduleSpec", () => {
  it("accepts a complete spec", () => {
    expect(validateScheduleSpec({ ...validSpec, tools: [...validSpec.tools] })).toBeNull();
  });

  it("rejects missing fields with a message", () => {
    expect(validateScheduleSpec({ ...validSpec, description: "  ", tools: [...validSpec.tools] })).toBe(
      "Description is required."
    );
    expect(validateScheduleSpec({ ...validSpec, how_to: [], tools: [...validSpec.tools] })).toBe(
      "Add at least one step."
    );
    expect(validateScheduleSpec({ ...validSpec, how_to: ["ok", "  "], tools: [...validSpec.tools] })).toBe(
      "Each step must be 1-500 characters."
    );
    expect(validateScheduleSpec({ ...validSpec, tools: [] })).toBe("Select at least one tool.");
    expect(validateScheduleSpec({ ...validSpec, expected_output: "", tools: [...validSpec.tools] })).toBe(
      "Expected output is required."
    );
  });

  it("rejects over-limit and unknown values", () => {
    expect(validateScheduleSpec({ ...validSpec, description: "x".repeat(501), tools: [...validSpec.tools] })).toBe(
      "Description must be at most 500 characters."
    );
    expect(
      validateScheduleSpec({
        ...validSpec,
        how_to: Array.from({ length: 11 }, (_, index) => `step ${index}`),
        tools: [...validSpec.tools],
      })
    ).toBe("At most 10 steps are allowed.");
    expect(validateScheduleSpec({ ...validSpec, tools: ["drop_tables" as never] })).toBe("Unknown tool selected.");
  });
});

describe("isStructuredProposal", () => {
  it("detects proposals carrying every spec field", () => {
    expect(
      isStructuredProposal({ ...validSpec, name: "Daily", frequency: "daily", time: "09:00", timezone: "UTC" })
    ).toBe(true);
    expect(
      isStructuredProposal({ name: "Daily", prompt: "Report", frequency: "daily", time: "09:00", timezone: "UTC" })
    ).toBe(false);
  });
});

describe("scheduleDescription", () => {
  it("prefers the spec description and falls back to the prompt", () => {
    expect(scheduleDescription({ spec: validSpec, prompt: "legacy" })).toBe("Summarize overdue work");
    expect(scheduleDescription({ spec: null, prompt: "legacy" })).toBe("legacy");
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm --filter=web test ai-schedule`
Expected: FAIL — `validateScheduleSpec is not a function` / import tidak ditemukan.

- [ ] **Step 3: Implementasi** — di `apps/web/core/lib/ai-schedule.ts`:

(a) Ganti `TAiScheduleProposal` (baris 9–17) dan `TAiSchedule` (baris 32–42) menjadi:

```ts
export const AI_SCHEDULE_TOOLS = ["list_projects", "count_work_items", "search_work_items"] as const;
export type TAiScheduleTool = (typeof AI_SCHEDULE_TOOLS)[number];

export type TAiScheduleSpec = {
  version: number;
  description: string;
  how_to: string[];
  tools: TAiScheduleTool[];
  expected_output: string;
};

export type TAiScheduleProposal = {
  name: string;
  frequency: TAiScheduleFrequency;
  time: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone: string;
  prompt?: string;
} & Partial<TAiScheduleSpec>;

export type TStructuredScheduleProposal = TAiScheduleProposal & TAiScheduleSpec;
```

```ts
export type TAiSchedule = Omit<TAiScheduleProposal, "prompt"> & {
  id: string;
  enabled: boolean;
  next_run_at: string;
  created_by_id: string;
  created_at: string;
  prompt: string;
  spec?: TAiScheduleSpec | null;
  last_status?: TAiScheduleRun["status"] | null;
  last_finished_at?: string | null;
  last_run_at?: string | null;
  runs?: TAiScheduleRun[];
};
```

(b) Tambahkan setelah `humanizeSchedule`:

```ts
export const SCHEDULE_SPEC_LIMITS = {
  description: 500,
  steps: 10,
  step: 500,
  expectedOutput: 1000,
} as const;

/** Mirrors `ScheduleSpec::validated` on the backend; returns the first error. */
export const validateScheduleSpec = (spec: Partial<TAiScheduleSpec>): string | null => {
  const description = spec.description?.trim() ?? "";
  if (!description) return "Description is required.";
  if (description.length > SCHEDULE_SPEC_LIMITS.description)
    return `Description must be at most ${SCHEDULE_SPEC_LIMITS.description} characters.`;

  const howTo = spec.how_to ?? [];
  if (howTo.length === 0) return "Add at least one step.";
  if (howTo.length > SCHEDULE_SPEC_LIMITS.steps) return `At most ${SCHEDULE_SPEC_LIMITS.steps} steps are allowed.`;
  if (howTo.some((step) => !step.trim() || step.trim().length > SCHEDULE_SPEC_LIMITS.step))
    return `Each step must be 1-${SCHEDULE_SPEC_LIMITS.step} characters.`;

  const tools = spec.tools ?? [];
  if (tools.length === 0) return "Select at least one tool.";
  if (tools.some((tool) => !AI_SCHEDULE_TOOLS.includes(tool))) return "Unknown tool selected.";

  const expected = spec.expected_output?.trim() ?? "";
  if (!expected) return "Expected output is required.";
  if (expected.length > SCHEDULE_SPEC_LIMITS.expectedOutput)
    return `Expected output must be at most ${SCHEDULE_SPEC_LIMITS.expectedOutput} characters.`;

  return null;
};

export const isStructuredProposal = (proposal: TAiScheduleProposal): proposal is TStructuredScheduleProposal =>
  Array.isArray(proposal.how_to) &&
  Array.isArray(proposal.tools) &&
  typeof proposal.description === "string" &&
  typeof proposal.expected_output === "string";

export const scheduleDescription = (schedule: { spec?: TAiScheduleSpec | null; prompt: string }): string =>
  schedule.spec?.description?.trim() || schedule.prompt;
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm --filter=web test ai-schedule`
Expected: PASS semua (tes lama + baru).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts
git commit -m "feat(web): schedule spec types, validator, and description helper"
```

---

### Task 8: Kartu konfirmasi editable + wiring store

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx`
- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Tulis test store yang gagal** — tambahkan import tipe di `apps/web/core/store/ai-assistant.store.test.ts` (setelah baris 4):

```ts
import type { TAiScheduleProposal } from "@/lib/ai-schedule";
```

lalu tambahkan test berikut di dalam `describe` yang memuat test `"confirming a proposal creates the schedule"` (setelah test `cancelling a proposal persists the cancelled decision`, sebelum penutup `});` di baris 345):

```ts
it("confirming with an edited proposal sends the edited fields", async () => {
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
  const message = store.messages.find((m) => m.scheduleProposal)!;
  const edited: TAiScheduleProposal = {
    ...message.scheduleProposal!,
    description: "Edited description",
    how_to: ["Edited step"],
    tools: ["list_projects"],
    expected_output: "Edited output",
  };

  await store.confirmScheduleProposal(message.id, edited);
  expect(services.schedules.create).toHaveBeenCalledWith("acme", edited, message.scheduleProposalKey);
  expect(message.scheduleProposal).toEqual(edited);
  expect(message.scheduleDecision).toBe("created");
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm --filter=web test ai-assistant.store`
Expected: FAIL kompilasi — `confirmScheduleProposal` hanya menerima satu argumen.

- [ ] **Step 3: Ganti isi kartu** — timpa seluruh `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import {
  AI_SCHEDULE_TOOLS,
  humanizeSchedule,
  isStructuredProposal,
  validateScheduleSpec,
  type TAiScheduleProposal,
  type TAiScheduleSpec,
  type TAiScheduleTool,
} from "@/lib/ai-schedule";

type Props = {
  proposal: TAiScheduleProposal;
  decision?: "pending" | "created" | "cancelled";
  onConfirm: (proposal: TAiScheduleProposal) => Promise<void>;
  onCancel: () => void;
};

export function ScheduleProposalCard({ proposal, decision, onConfirm, onCancel }: Props) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<TAiScheduleProposal>(proposal);
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const rawWorkspaceSlug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm(draft);
    } catch {
      setError("Could not create the schedule. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  if (decision === "created") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule created.{" "}
        {rawWorkspaceSlug && (
          <Link href={`/${rawWorkspaceSlug}/scheduler/`} className="text-accent-primary hover:underline">
            Open Scheduler
          </Link>
        )}
      </p>
    );
  }
  if (decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule cancelled.
      </p>
    );
  }
  if (decision !== "pending") return null;

  const patch = (fields: Partial<TAiScheduleSpec>) => setDraft((current) => ({ ...current, ...fields }));

  // Proposals stored before the recipe rollout have no spec fields: keep the
  // old read-only card and let the backend take the legacy path.
  if (!isStructuredProposal(draft)) {
    return (
      <div role="group" aria-label="Schedule proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
        <p className="text-12 font-semibold break-words text-primary">{draft.name}</p>
        <p className="mt-0.5 text-12 text-secondary">{humanizeSchedule(draft)}</p>
        <p className="mt-1 line-clamp-3 text-12 text-tertiary">{draft.prompt}</p>
        {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
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
  }

  const structured = draft;
  const validationError = validateScheduleSpec(structured);

  const updateStep = (index: number, value: string) => {
    const steps = [...structured.how_to];
    steps[index] = value;
    patch({ how_to: steps });
  };
  const removeStep = (index: number) =>
    patch({ how_to: structured.how_to.filter((_, stepIndex) => stepIndex !== index) });
  const addStep = () => patch({ how_to: [...structured.how_to, ""] });
  const toggleTool = (tool: TAiScheduleTool, checked: boolean) =>
    patch({
      tools: checked ? [...structured.tools, tool] : structured.tools.filter((current) => current !== tool),
    });

  return (
    <div role="group" aria-label="Schedule proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <label className="text-12 text-secondary">
        Name
        <input
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={structured.name}
          onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
        />
      </label>
      <p className="mt-1 text-12 text-secondary">{humanizeSchedule(structured)}</p>

      <label className="mt-2 block text-12 text-secondary">
        Description
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={structured.description}
          onChange={(event) => patch({ description: event.target.value })}
        />
      </label>

      <div className="mt-2 text-12 text-secondary">
        Steps
        {structured.how_to.map((step, index) => (
          <div key={index} className="mt-1 flex items-center gap-1">
            <input
              className="w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={step}
              onChange={(event) => updateStep(index, event.target.value)}
            />
            <Button
              size="sm"
              variant="ghost"
              disabled={structured.how_to.length === 1}
              onClick={() => removeStep(index)}
            >
              Remove
            </Button>
          </div>
        ))}
        <Button size="sm" variant="ghost" onClick={addStep}>
          Add step
        </Button>
      </div>

      <div className="mt-2 text-12 text-secondary">
        Tools
        <div className="mt-1 flex flex-wrap gap-2">
          {AI_SCHEDULE_TOOLS.map((tool) => (
            <label key={tool} className="flex items-center gap-1 text-12 text-primary">
              <input
                type="checkbox"
                checked={structured.tools.includes(tool)}
                onChange={(event) => toggleTool(tool, event.target.checked)}
              />
              {tool}
            </label>
          ))}
        </div>
      </div>

      <label className="mt-2 block text-12 text-secondary">
        Expected output
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={structured.expected_output}
          onChange={(event) => patch({ expected_output: event.target.value })}
        />
      </label>

      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={Boolean(validationError)}
          onClick={() => void confirm()}
        >
          Confirm
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Store menerima proposal hasil edit** — di `apps/web/core/store/ai-assistant.store.ts`:

(a) Pastikan import tipe dari `@/lib/ai-schedule` mencakup `TAiScheduleProposal` (cek baris import di atas file; tambahkan bila belum).

(b) Ganti `confirmScheduleProposal` (baris 357–373) menjadi:

```ts
confirmScheduleProposal = async (messageId: string, proposal?: TAiScheduleProposal) => {
  const slug = this.workspaceSlug;
  const conversationId = this.activeConversationId;
  const message = this.messages.find((candidate) => candidate.id === messageId);
  if (!slug || !message?.scheduleProposal || !message.scheduleProposalKey) return;
  if (message.scheduleDecision !== "pending") return;
  const effective = proposal ?? message.scheduleProposal;
  runInAction(() => {
    message.scheduleProposal = effective;
  });
  const created = await this.schedulesService.create(slug, effective, message.scheduleProposalKey);
  if (!created?.id) throw new Error("Schedule creation returned no id");
  runInAction(() => {
    message.scheduleDecision = "created";
    message.createdScheduleId = created.id;
  });
  await this.persistDecision(conversationId, message, {
    schedule_decision: "created",
    created_schedule_id: created.id,
  });
};
```

- [ ] **Step 5: Root meneruskan proposal** — di `apps/web/core/components/ai/assistant-sidebar/root.tsx` (baris 310–316), ganti `onConfirm`:

```tsx
<ScheduleProposalCard
  proposal={message.scheduleProposal}
  decision={message.scheduleDecision}
  onConfirm={(proposal) => confirmScheduleProposal(message.id, proposal)}
  onCancel={() => resolveScheduleProposal(message.id, "cancelled")}
/>
```

- [ ] **Step 6: Verifikasi tipe + test**

Run: `pnpm --filter=web exec tsc --noEmit` (atau `pnpm check:types`)
Expected: tidak ada error terkait `ai-schedule` / `schedule-proposal-card` / `ai-assistant.store`.

Run: `pnpm --filter=web test ai-schedule ai-assistant.store`
Expected: PASS — termasuk test "confirming with an edited proposal sends the edited fields".

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx apps/web/core/store/ai-assistant.store.ts apps/web/core/components/ai/assistant-sidebar/root.tsx apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): editable schedule recipe card with legacy fallback"
```

---

### Task 9: Halaman Scheduler menampilkan audit resep

**Files:**

- Modify: `apps/web/core/components/ai-scheduler/schedule-item.tsx`
- Test: `apps/web/core/lib/ai-schedule.test.ts` (sudah mencakup `scheduleDescription` di Task 7)

- [ ] **Step 1: Perbarui item** — di `apps/web/core/components/ai-scheduler/schedule-item.tsx`:

(a) Tambahkan import helper di baris 23:

```ts
import {
  humanizeSchedule,
  scheduleDescription,
  scheduleStatusLabel,
  type TAiSchedule,
  type TAiScheduleRun,
} from "@/lib/ai-schedule";
```

(b) Tambahkan state disclosure setelah `const [toggling, setToggling] = useState(false);`:

```tsx
const [recipeOpen, setRecipeOpen] = useState(false);
```

(c) Ganti baris 117 (`<p className="text-xs line-clamp-1 text-tertiary">{schedule.prompt}</p>`) menjadi:

```tsx
<p className="text-xs line-clamp-1 text-tertiary">{scheduleDescription(schedule)}</p>;
{
  schedule.spec && (
    <div className="mt-1 flex flex-wrap gap-1">
      {schedule.spec.tools.map((tool) => (
        <Badge key={tool} variant="neutral" size="sm">
          {tool}
        </Badge>
      ))}
    </div>
  );
}
```

(d) Di blok tombol (setelah tombol History, baris 150–152), tambahkan tombol Recipe untuk schedule ber-spec:

```tsx
{
  schedule.spec && (
    <Button size="sm" variant="ghost" onClick={() => setRecipeOpen((open) => !open)}>
      {recipeOpen ? "Hide recipe" : "Recipe"}
    </Button>
  );
}
```

(e) Setelah baris `{expanded && <ScheduleRunsList runs={runs} />}` tambahkan:

```tsx
{
  recipeOpen && schedule.spec && (
    <div className="mt-2 rounded-md border border-subtle bg-layer-2 p-2">
      <p className="text-xs font-medium text-secondary">How to</p>
      <ol className="mt-1 list-decimal space-y-0.5 pl-4 text-xs text-tertiary">
        {schedule.spec.how_to.map((step, index) => (
          <li key={index}>{step}</li>
        ))}
      </ol>
      <p className="mt-2 text-xs font-medium text-secondary">Expected output</p>
      <p className="mt-0.5 text-xs text-tertiary">{schedule.spec.expected_output}</p>
    </div>
  );
}
```

- [ ] **Step 2: Verifikasi tipe + test**

Run: `pnpm check:types`
Expected: tidak ada error terkait `schedule-item`.

Run: `pnpm --filter=web test ai-schedule`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/ai-scheduler/schedule-item.tsx
git commit -m "feat(web): show recipe description, tool badges, and audit disclosure in scheduler"
```

---

### Task 10: Verifikasi akhir & rebuild

**Files:** —

- [ ] **Step 1: Gate Rust**

Run (dari `apps/api-rs`): `cargo fmt --check && cargo clippy -p ai -p api -p worker --all-targets`
Expected: bersih (atau hanya warning pre-existing).

Run (dari `apps/api-rs`): `cargo test -p ai && cargo test -p api --test ai_agent_test && cargo test -p api --test ai_schedule_test -- --test-threads=1 && cargo test -p worker --test ai_schedule_test -- --test-threads=1`
Expected: semua PASS.

- [ ] **Step 2: Gate web**

Run: `pnpm check && pnpm --filter=web test`
Expected: PASS (format/lint/types bersih).

- [ ] **Step 3: Rebuild backend + web (sesuai AGENTS.md)**

Run: `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &`
Expected: build LTO bisa 10+ menit tanpa output — poll `docker compose -f docker-compose-local.yml ps` sampai `api`, `worker`, `beat-worker` healthy/running.

Run: `curl http://localhost:8000/health`
Expected: HTTP 200.

Run: `pnpm --filter=web build && systemctl --user restart plane-web-prod.service`
Expected: build sukses; service aktif.

- [ ] **Step 4: Smoke manual di tunnel**

1. Chat agent: kirim `/schedule ringkasan work item urgent tiap Senin 09:00` → agen menanyakan yang kurang → panggil `create_schedule` → kartu muncul dengan field `description`, `steps`, `tools`, `expected_output` yang bisa diedit.
2. Ubah satu langkah + tambah tool → Confirm → jadwal dibuat; buka halaman Scheduler: deskripsi tampil, badge tool tampil, tombol Recipe menampilkan langkah + expected output.
3. `Run now` → history menampilkan hasil run; `ai_schedule_runs.prompt` berisi prompt render.
4. Schedule legacy (dibuat sebelum deploy) masih bisa `Run now` dan Pause/Resume.
5. Kartu pending lama di riwayat chat (tanpa spec) tetap tampil read-only dan bisa di-Confirm.

- [ ] **Step 5: Commit akhir (bila ada perbaikan smoke)**

Hanya file yang tersentuh task ini — jangan `git add -A` (working tree memuat perubahan unrelated):

```bash
git add apps/api-rs/crates/ai apps/api-rs/crates/api apps/api-rs/crates/worker apps/api-rs/migrations apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts apps/web/core/components/ai/assistant-sidebar apps/web/core/components/ai-scheduler apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "fix(ai): schedule recipe smoke fixes"
```

---

## Catatan pelaksanaan

- Jangan menjalankan `cargo test -p api` penuh paralel dengan suite scratch-workspace lain: ikuti `-- --test-threads=1` untuk suite DB yang disebut di AGENTS.md.
- `read_tools` sengaja tidak mengekspos `create_schedule`; `workspace_tools` tetap 4 tool untuk chat.
- `ScheduleSpec::validated` adalah satu-satunya sumber kebenaran bentuk spec; jangan duplikasi aturan di SQL.
- Jangan backfill `spec` untuk row lama.
