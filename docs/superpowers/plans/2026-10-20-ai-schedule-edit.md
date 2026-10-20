# AI Schedule Edit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Creator/admin can edit every saved schedule field from the Scheduler UI and from Galileo, through one validated PATCH path, without losing run history.

**Architecture:** Extend the existing `PATCH /ai-schedules/:id/` from `enabled`-only to a partial update that merges fields, re-renders structured recipe prompts, and recomputes `next_run_at` only when cadence changes or a paused schedule resumes. Add read-only `list_schedules`/`get_schedule` tools plus an `update_schedule` proposal tool; the API enriches the proposal with a before snapshot and the FE renders a read-only before/after card. Also fix the worker tick's 2000-character prompt cap.

**Tech Stack:** Rust (axum, sqlx, Rig, serde/schemars, chrono), PostgreSQL, React Router + MobX, vitest.

**Spec:** `docs/superpowers/specs/2026-10-20-ai-schedule-edit-design.md`

**Prerequisites:** local Postgres/Redis stack is running. Rust commands run from `apps/api-rs`; web commands run from the repo root. DB-backed tests use `DATABASE_URL` (default `postgres://plane:plane@localhost:5432/plane`) and `-- --test-threads=1` where noted.

---

## File Structure

- `apps/api-rs/crates/ai/src/schedule.rs` — `ScheduleChanges`, `updated_recipe`, `updated_legacy`, prompt cap constant.
- `apps/api-rs/crates/ai/src/tools/schedules.rs` (new) — `list_schedules`, `get_schedule`, `update_schedule` proposal.
- `apps/api-rs/crates/ai/src/tools/mod.rs` — module exports, proposal allowlist, tool registration.
- `apps/api-rs/crates/ai/src/agent.rs` — pending schedule action and preamble.
- `apps/api-rs/crates/api/src/routes/ai_schedule.rs` — partial PATCH; shared helpers.
- `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` — update proposal metadata enrichment.
- `apps/api-rs/crates/api/src/routes/ai_conversations.rs` — decision metadata allowlist.
- `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs` — long-prompt tick fix.
- `apps/web/core/lib/ai-schedule.ts` — update types, validation, diff helper.
- `apps/web/core/services/ai-schedules.service.ts` — typed update payload.
- `apps/web/core/store/ai-schedules.store.ts` — `updateSchedule`.
- `apps/web/core/components/ai-scheduler/schedule-edit-modal.tsx` (new) — edit form.
- `apps/web/core/components/ai/assistant-sidebar/schedule-recipe-fields.tsx` (new) — shared recipe fields.
- `apps/web/core/components/ai/assistant-sidebar/schedule-update-proposal-card.tsx` (new) — before/after card.
- `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx` — use shared recipe fields.
- `apps/web/core/components/ai/assistant-sidebar/root.tsx` — render update card.
- `apps/web/core/store/ai-assistant.store.ts` — apply/cancel update proposals.
- `apps/web/core/lib/ai-context.ts`, `ai-conversations.ts` — message metadata types.
- `apps/web/core/components/docs/content/scheduler.tsx`, `galileo.tsx` — documentation.

---

### Task 1: Schedule update primitives (crate `ai`)

**Files:**

- Modify: `apps/api-rs/crates/ai/src/schedule.rs`
- Test: same file, `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests**

Append these tests inside the existing `mod tests` in `apps/api-rs/crates/ai/src/schedule.rs`:

```rust
    fn base_spec() -> ScheduleSpec {
        ScheduleSpec::new(
            "Summarize overdue items",
            &["Count overdue items".to_string()],
            &["count_work_items".to_string()],
            "A short list",
        )
        .unwrap()
    }

    #[test]
    fn update_changes_validates_bounds_and_mixing() {
        let empty = ScheduleChanges::default();
        assert!(empty.validate().unwrap_err().contains("at least one change"));

        let mixed = ScheduleChanges {
            prompt: Some("legacy".to_string()),
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        assert!(mixed.validate().unwrap_err().contains("cannot be combined"));

        let blank_name = ScheduleChanges {
            name: Some("  ".to_string()),
            ..Default::default()
        };
        assert!(blank_name.validate().unwrap_err().contains("name"));

        let bad_day = ScheduleChanges {
            day_of_week: Some(8),
            ..Default::default()
        };
        assert!(bad_day.validate().unwrap_err().contains("day_of_week"));

        let bad_tz = ScheduleChanges {
            timezone: Some("Mars/Olympus".to_string()),
            ..Default::default()
        };
        assert!(bad_tz.validate().unwrap_err().contains("timezone"));

        let valid = ScheduleChanges {
            name: Some(" Weekly report ".to_string()),
            frequency: Some("weekly".to_string()),
            day_of_week: Some(1),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn updated_recipe_merges_fields_and_renders_a_new_prompt() {
        let spec = base_spec();
        let changes = ScheduleChanges {
            name: Some("Weekly overdue report".to_string()),
            how_to: Some(vec!["Count overdue items".to_string(), "Group by owner".to_string()]),
            tools: Some(vec!["search_work_items".to_string(), "count_work_items".to_string()]),
            frequency: Some("weekly".to_string()),
            day_of_week: Some(3),
            ..Default::default()
        };
        let recipe = updated_recipe(
            "Weekly overdue report",
            &spec,
            &changes,
            "weekly",
            "10:30",
            Some(3),
            None,
            "Asia/Jakarta",
        )
        .unwrap();
        assert_eq!(recipe.proposal.frequency, "weekly");
        assert_eq!(recipe.proposal.day_of_week, Some(3));
        assert_eq!(recipe.proposal.timezone, "Asia/Jakarta");
        assert_eq!(recipe.spec.how_to.len(), 2);
        assert_eq!(recipe.spec.tools, vec!["count_work_items", "search_work_items"]);
        assert!(recipe.proposal.prompt.contains("Task: Weekly overdue report"));
        assert!(recipe.proposal.prompt.contains("2. Group by owner"));
    }

    #[test]
    fn updated_recipe_rejects_prompt_edits() {
        let changes = ScheduleChanges {
            prompt: Some("raw".to_string()),
            ..Default::default()
        };
        let error = updated_recipe(
            "Name",
            &base_spec(),
            &changes,
            "daily",
            "09:00",
            None,
            None,
            "UTC",
        )
        .unwrap_err();
        assert!(error.contains("legacy"));
    }

    #[test]
    fn updated_legacy_rejects_structured_fields() {
        let changes = ScheduleChanges {
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        let error = updated_legacy(
            "Name",
            "old prompt",
            &changes,
            "daily",
            "09:00",
            None,
            None,
            "UTC",
        )
        .unwrap_err();
        assert!(error.contains("recreate"));
    }

    #[test]
    fn updated_legacy_merges_prompt_and_cadence() {
        let changes = ScheduleChanges {
            name: Some("New name".to_string()),
            prompt: Some("new prompt".to_string()),
            frequency: Some("weekly".to_string()),
            day_of_week: Some(2),
            ..Default::default()
        };
        let proposal = updated_legacy(
            "New name",
            "old prompt",
            &changes,
            "weekly",
            "08:15",
            Some(2),
            None,
            "UTC",
        )
        .unwrap();
        assert_eq!(proposal.name, "New name");
        assert_eq!(proposal.prompt, "new prompt");
        assert_eq!(proposal.day_of_week, Some(2));
        assert_eq!(proposal.time, "08:15");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p ai update_changes -- --nocapture
```

Expected: compile error because `ScheduleChanges`, `updated_recipe`, and `updated_legacy` do not exist.

- [ ] **Step 3: Implement the primitives**

Add near the other constants in `apps/api-rs/crates/ai/src/schedule.rs`:

```rust
pub const LEGACY_PROMPT_MAX: usize = 2000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScheduleChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_to: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_week: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_month: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl ScheduleChanges {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.how_to.is_none()
            && self.tools.is_none()
            && self.expected_output.is_none()
            && self.prompt.is_none()
            && self.frequency.is_none()
            && self.time.is_none()
            && self.day_of_week.is_none()
            && self.day_of_month.is_none()
            && self.timezone.is_none()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err("at least one change is required".to_string());
        }
        if self.prompt.is_some()
            && (self.description.is_some()
                || self.how_to.is_some()
                || self.tools.is_some()
                || self.expected_output.is_some())
        {
            return Err(
                "prompt cannot be combined with structured recipe fields".to_string(),
            );
        }
        if let Some(name) = self.name.as_deref() {
            let trimmed = name.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 120 {
                return Err("name must be 1-120 characters".to_string());
            }
        }
        if let Some(description) = self.description.as_deref() {
            bounded_text("description", description, SPEC_DESCRIPTION_MAX)?;
        }
        if let Some(how_to) = self.how_to.as_ref() {
            if how_to.is_empty() || how_to.len() > SPEC_STEPS_MAX {
                return Err(format!("how_to must have 1-{SPEC_STEPS_MAX} steps"));
            }
            for step in how_to {
                bounded_text("how_to step", step, SPEC_STEP_MAX)?;
            }
        }
        if let Some(tools) = self.tools.as_ref() {
            if tools.is_empty() {
                return Err("tools must include at least one read tool".to_string());
            }
            if let Some(unknown) = tools
                .iter()
                .map(|tool| tool.trim())
                .find(|tool| !SPEC_TOOLS.contains(tool))
            {
                return Err(format!("unknown tool: {unknown}"));
            }
        }
        if let Some(prompt) = self.prompt.as_deref() {
            bounded_text("prompt", prompt, LEGACY_PROMPT_MAX)?;
        }
        if let Some(frequency) = self.frequency.as_deref() {
            let normalized = frequency.trim().to_ascii_lowercase();
            if !FREQUENCIES.contains(&normalized.as_str()) {
                return Err(format!(
                    "frequency must be one of: {}",
                    FREQUENCIES.join(", ")
                ));
            }
        }
        if let Some(time) = self.time.as_deref() {
            parse_time(time)?;
        }
        if let Some(day_of_week) = self.day_of_week {
            if !(1..=7).contains(&day_of_week) {
                return Err("day_of_week must be 1 (Monday) to 7 (Sunday)".to_string());
            }
        }
        if let Some(day_of_month) = self.day_of_month {
            if !(1..=31).contains(&day_of_month) {
                return Err("day_of_month must be 1-31".to_string());
            }
        }
        if let Some(timezone) = self.timezone.as_deref() {
            timezone
                .trim()
                .parse::<Tz>()
                .map_err(|_| format!("unknown IANA timezone: {}", timezone.trim()))?;
        }
        Ok(())
    }
}

/// Merge a structured schedule's recipe fields and render its canonical prompt.
#[allow(clippy::too_many_arguments)]
pub fn updated_recipe(
    name: &str,
    spec: &ScheduleSpec,
    changes: &ScheduleChanges,
    frequency: &str,
    time: &str,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: &str,
) -> Result<ScheduleRecipe, String> {
    if changes.prompt.is_some() {
        return Err("prompt is only editable on legacy schedules".to_string());
    }
    let description = changes.description.as_deref().unwrap_or(&spec.description);
    let how_to = changes.how_to.as_deref().unwrap_or(&spec.how_to);
    let tools = changes.tools.as_deref().unwrap_or(&spec.tools);
    let expected_output = changes
        .expected_output
        .as_deref()
        .unwrap_or(&spec.expected_output);
    ScheduleRecipe::new(
        name,
        description,
        how_to,
        tools,
        expected_output,
        frequency,
        Some(time),
        day_of_week,
        day_of_month,
        Some(timezone),
    )
}

/// Merge a legacy schedule's raw prompt and cadence.
#[allow(clippy::too_many_arguments)]
pub fn updated_legacy(
    name: &str,
    prompt: &str,
    changes: &ScheduleChanges,
    frequency: &str,
    time: &str,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: &str,
) -> Result<ScheduleProposal, String> {
    if changes.description.is_some()
        || changes.how_to.is_some()
        || changes.tools.is_some()
        || changes.expected_output.is_some()
    {
        return Err(
            "structured recipe fields can only be edited on recipe schedules; recreate the schedule to convert it"
                .to_string(),
        );
    }
    let prompt = changes.prompt.as_deref().unwrap_or(prompt);
    let name = changes.name.as_deref().unwrap_or(name);
    ScheduleProposal::new(
        name,
        prompt,
        frequency,
        Some(time),
        day_of_week,
        day_of_month,
        Some(timezone),
    )
}
```

Change `ScheduleProposal::new` to use the constant:

```rust
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
            LEGACY_PROMPT_MAX,
        )
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test -p ai update_ -- --nocapture
```

Expected: all five new update-primitive tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/schedule.rs
git commit -m "feat(ai): add schedule update validation primitives"
```

---

### Task 2: Partial PATCH endpoint

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_schedule.rs`
- Test: `apps/api-rs/crates/api/tests/ai_schedule_test.rs`

- [ ] **Step 1: Write the failing API tests**

Append these tests to `apps/api-rs/crates/api/tests/ai_schedule_test.rs`:

```rust
#[tokio::test]
async fn patch_edits_structured_fields_and_rerenders_prompt() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(structured_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (status, Json(updated)) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({
            "name": "Weekly overdue report",
            "description": "Weekly overdue summary per owner",
            "how_to": ["Count overdue items", "Group the list by owner"],
            "tools": ["count_work_items", "search_work_items"],
            "expected_output": "A markdown table grouped by owner",
            "frequency": "weekly",
            "time": "10:30",
            "day_of_week": 3,
            "timezone": "Asia/Jakarta",
        })),
    )
    .await
    .expect("patch ok");

    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], json!("Weekly overdue report"));
    assert_eq!(updated["frequency"], json!("weekly"));
    assert_eq!(updated["day_of_week"], json!(3));
    assert_eq!(updated["time"], json!("10:30"));
    assert_eq!(updated["timezone"], json!("Asia/Jakarta"));
    assert_eq!(
        updated["spec"]["description"],
        json!("Weekly overdue summary per owner")
    );
    assert_eq!(updated["spec"]["how_to"][1], json!("Group the list by owner"));
    let prompt = updated["prompt"].as_str().unwrap();
    assert!(prompt.contains("Task: Weekly overdue report"));
    assert!(prompt.contains("2. Group the list by owner"));
    assert!(prompt.contains("Allowed tools: count_work_items, search_work_items"));

    let next: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(next > chrono::Utc::now());

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_keeps_next_run_for_content_and_paused_cadence_edits() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(structured_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let original_next: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"name": "Daily report v2"})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let after_content: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_content, original_next);

    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": false})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);

    let (status, Json(paused)) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"frequency": "weekly", "time": "11:00", "day_of_week": 2})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["frequency"], json!("weekly"));
    let paused_next: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(paused_next, original_next);

    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": true})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let resumed_next: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(resumed_next > chrono::Utc::now());

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_edits_legacy_prompt_and_rejects_invalid_shapes() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (status, Json(updated)) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({
            "name": "Legacy report v2",
            "prompt": "Summarize overdue work items by owner",
            "timezone": "Asia/Jakarta",
        })),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], json!("Legacy report v2"));
    assert_eq!(updated["prompt"], json!("Summarize overdue work items by owner"));
    assert_eq!(updated["spec"], json!(null));

    // Structured fields on a legacy schedule are rejected.
    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"description": "recipe"})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Unknown key.
    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"next_run_at": "2030-01-01T00:00:00Z"})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Empty patch.
    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Weekly without a weekday.
    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"frequency": "weekly"})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_rejects_prompt_edits_on_structured_schedules() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(structured_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"prompt": "raw override"})),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_schedule_test patch_ -- --test-threads=1
```

Expected: the new tests FAIL; `enabled`-only PATCH ignores the new fields.

- [ ] **Step 3: Implement the partial PATCH**

In `apps/api-rs/crates/api/src/routes/ai_schedule.rs`:

1. Update imports:

```rust
use ai::schedule::{
    updated_legacy, updated_recipe, ScheduleChanges, ScheduleProposal, ScheduleRecipe,
    ScheduleSpec, SPEC_PROMPT_MAX,
};
```

2. Add the request shape after `CreateScheduleBody`:

```rust
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateScheduleBody {
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub how_to: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub expected_output: Option<String>,
    pub prompt: Option<String>,
    pub frequency: Option<String>,
    pub time: Option<String>,
    pub day_of_week: Option<i16>,
    pub day_of_month: Option<i16>,
    pub timezone: Option<String>,
}

impl UpdateScheduleBody {
    fn changes(&self) -> ScheduleChanges {
        ScheduleChanges {
            name: self.name.clone(),
            description: self.description.clone(),
            how_to: self.how_to.clone(),
            tools: self.tools.clone(),
            expected_output: self.expected_output.clone(),
            prompt: self.prompt.clone(),
            frequency: self.frequency.clone(),
            time: self.time.clone(),
            day_of_week: self.day_of_week,
            day_of_month: self.day_of_month,
            timezone: self.timezone.clone(),
        }
    }
}
```

3. Make shared helpers visible to `ai_agent`:

```rust
pub(crate) async fn load_schedule(
```

```rust
pub(crate) fn schedule_json(row: &ScheduleRow) -> Value {
```

```rust
pub(crate) fn can_manage(created_by_id: Uuid, user_id: Uuid, ws_role: Option<i16>) -> bool {
```

4. Replace the body of `patch` with:

```rust
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, schedule_id)): Path<(String, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_schedule(&st.pool, &slug, schedule_id).await? else {
        return Ok(missing());
    };
    if !can_manage(row.created_by_id, auth.0, role) {
        return Ok(deny());
    }
    let body: UpdateScheduleBody = match serde_json::from_value(body) {
        Ok(body) => body,
        Err(err) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid schedule payload: {err}")})),
            ));
        }
    };
    let changes = body.changes();
    if body.enabled.is_none() && changes.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "at least one field is required"})),
        ));
    }
    if let Err(message) = changes.validate() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
    }

    let enabled = body.enabled.unwrap_or(row.enabled);
    let resume = body.enabled == Some(true) && !row.enabled;
    let cadence_changed = changes.frequency.is_some()
        || changes.time.is_some()
        || changes.day_of_week.is_some()
        || changes.day_of_month.is_some()
        || changes.timezone.is_some();
    let content_changed = changes.name.is_some()
        || changes.description.is_some()
        || changes.how_to.is_some()
        || changes.tools.is_some()
        || changes.expected_output.is_some()
        || changes.prompt.is_some();

    let name = changes.name.as_deref().unwrap_or(&row.name).to_string();
    let frequency = changes
        .frequency
        .as_deref()
        .unwrap_or(&row.frequency)
        .to_string();
    let time = changes
        .time
        .as_deref()
        .unwrap_or(&row.time_of_day)
        .to_string();
    let day_of_week = changes.day_of_week.or(row.day_of_week);
    let day_of_month = changes.day_of_month.or(row.day_of_month);
    let timezone = changes
        .timezone
        .as_deref()
        .unwrap_or(&row.timezone)
        .to_string();

    let (prompt, spec, next_run_at) = if let Some(raw_spec) = row.spec.as_ref() {
        if changes.prompt.is_some() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "prompt is only editable on legacy schedules"})),
            ));
        }
        let stored = serde_json::from_value::<ScheduleSpec>(raw_spec.clone())
            .map_err(|error| error.to_string())
            .and_then(|spec| spec.validated());
        let stored = match stored {
            Ok(spec) => spec,
            Err(message) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("stored schedule spec is invalid: {message}")})),
                ));
            }
        };
        if content_changed {
            let recipe = match updated_recipe(
                &name,
                &stored,
                &changes,
                &frequency,
                &time,
                day_of_week,
                day_of_month,
                &timezone,
            ) {
                Ok(recipe) => recipe,
                Err(message) => {
                    return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
                }
            };
            let next = if enabled && (resume || cadence_changed) {
                Some(recipe.proposal.next_occurrence(chrono::Utc::now()))
            } else {
                None
            };
            (
                recipe.proposal.prompt,
                serde_json::to_value(&recipe.spec).ok(),
                next,
            )
        } else {
            let proposal = match ScheduleProposal::with_prompt_limit(
                &name,
                &row.prompt,
                &frequency,
                Some(&time),
                day_of_week,
                day_of_month,
                Some(&timezone),
                SPEC_PROMPT_MAX,
            ) {
                Ok(proposal) => proposal,
                Err(message) => {
                    return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
                }
            };
            let next = if enabled && (resume || cadence_changed) {
                Some(proposal.next_occurrence(chrono::Utc::now()))
            } else {
                None
            };
            (row.prompt.clone(), row.spec.clone(), next)
        }
    } else {
        let proposal = match updated_legacy(
            &name,
            &row.prompt,
            &changes,
            &frequency,
            &time,
            day_of_week,
            day_of_month,
            &timezone,
        ) {
            Ok(proposal) => proposal,
            Err(message) => {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
            }
        };
        let next = if enabled && (resume || cadence_changed) {
            Some(proposal.next_occurrence(chrono::Utc::now()))
        } else {
            None
        };
        (proposal.prompt, None, next)
    };

    let next_run_at = next_run_at.unwrap_or(row.next_run_at);
    sqlx::query(
        "UPDATE ai_schedules SET name = $2, prompt = $3, spec = $4, frequency = $5, \
         time_of_day = $6, day_of_week = $7, day_of_month = $8, timezone = $9, \
         enabled = $10, next_run_at = $11, updated_at = now() WHERE id = $1",
    )
    .bind(schedule_id)
    .bind(&name)
    .bind(&prompt)
    .bind(&spec)
    .bind(&frequency)
    .bind(&time)
    .bind(day_of_week)
    .bind(day_of_month)
    .bind(&timezone)
    .bind(enabled)
    .bind(next_run_at)
    .execute(&st.pool)
    .await?;

    let Some(updated) = load_schedule(&st.pool, &slug, schedule_id).await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(schedule_json(&updated))))
}
```

- [ ] **Step 4: Run the patch tests to verify they pass**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_schedule_test patch_ -- --test-threads=1
```

Expected: all patch tests PASS.

- [ ] **Step 5: Run the full schedule API suite**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_schedule_test -- --test-threads=1
```

Expected: PASS; the existing `enabled` pause/resume test still passes.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_schedule.rs apps/api-rs/crates/api/tests/ai_schedule_test.rs
git commit -m "feat(api): support full schedule edits through PATCH"
```

---

### Task 3: Worker long-prompt tick fix

**Files:**

- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`
- Test: `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`

- [ ] **Step 1: Write the failing worker test**

Append to `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`:

```rust
#[tokio::test]
async fn tick_keeps_long_structured_recipes_enabled() {
    let pool = pool().await;
    let slug = format!("aislong-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Long', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let long_step = "x".repeat(300);
    let how_to: Vec<String> = (0..10).map(|_| long_step.clone()).collect();
    let spec = serde_json::json!({
        "version": 1,
        "description": "d",
        "how_to": how_to,
        "tools": ["list_projects"],
        "expected_output": "o"
    });
    let prompt = "p".repeat(2500);
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, spec, frequency, \
         time_of_day, timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Long recipe', $4, $5, 'daily', '09:00', 'UTC', true, \
         now() - interval '1 minute', $6, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(&prompt)
    .bind(&spec)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    let mut redis = common::redis::create_redis(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .await;
    let queued = ai_schedule::tick(&pool, &mut redis).await.expect("tick");

    let enabled: bool = sqlx::query_scalar("SELECT enabled FROM ai_schedules WHERE id = $1")
        .bind(schedule_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(enabled, "long structured recipe must not be disabled");
    assert!(
        !queued.is_empty(),
        "the due long recipe must queue a run"
    );

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

- [ ] **Step 2: Run it to verify it fails**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p worker --test ai_schedule_test tick_keeps_long_structured_recipes_enabled -- --test-threads=1
```

Expected: FAIL; `enabled` is false because the tick used the 2000-character cap.

- [ ] **Step 3: Fix the tick parser**

In `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`:

```rust
use ai::schedule::{ScheduleProposal, SPEC_PROMPT_MAX};
```

```rust
impl DueSchedule {
    fn proposal(&self) -> Result<ScheduleProposal, String> {
        ScheduleProposal::with_prompt_limit(
            "scheduled",
            &self.prompt,
            &self.frequency,
            Some(&self.time_of_day),
            self.day_of_week,
            self.day_of_month,
            Some(&self.timezone),
            SPEC_PROMPT_MAX,
        )
    }
}
```

- [ ] **Step 4: Run the worker tests to verify they pass**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p worker --test ai_schedule_test -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/api-rs/crates/worker/tests/ai_schedule_test.rs
git commit -m "fix(worker): honor long structured schedule prompts on tick"
```

---

### Task 4: Galileo schedule management tools

**Files:**

- Create: `apps/api-rs/crates/ai/src/tools/schedules.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Test: `apps/api-rs/crates/ai/src/tools/schedules.rs` and `apps/api-rs/crates/api/tests/ai_schedule_test.rs`

- [ ] **Step 1: Write the failing tool tests**

Add this `#[cfg(test)] mod tests` at the bottom of the new file you are about to create (`schedules.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::new_trace;

    #[test]
    fn list_schedules_sql_is_workspace_scoped() {
        assert!(LIST_SCHEDULES_SQL.contains("workspace_id = $1"));
        assert!(LIST_SCHEDULES_SQL.contains("deleted_at IS NULL"));
        assert!(LIST_SCHEDULES_SQL.contains("role = 20"));
    }

    #[test]
    fn update_args_reject_empty_and_mixed_prompt() {
        let empty = UpdateScheduleArgs {
            schedule_id: Uuid::nil().to_string(),
            ..Default::default()
        };
        assert!(empty.into_proposal().unwrap_err().to_string().contains("at least one"));

        let mixed = UpdateScheduleArgs {
            schedule_id: Uuid::nil().to_string(),
            prompt: Some("raw".to_string()),
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        assert!(mixed.into_proposal().unwrap_err().to_string().contains("cannot be combined"));
    }

    #[tokio::test]
    async fn update_schedule_records_a_normalized_proposal() {
        let trace = new_trace();
        let tool = UpdateSchedule { trace: trace.clone() };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                UpdateScheduleArgs {
                    schedule_id: Uuid::nil().to_string(),
                    frequency: Some("WEEKLY".to_string()),
                    day_of_week: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["schedule_id"], json!(Uuid::nil().to_string()));
        assert_eq!(parsed["changes"]["frequency"], json!("weekly"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded[0].name, "update_schedule");
    }
}
```

Add this DB-backed tool test to `apps/api-rs/crates/api/tests/ai_schedule_test.rs`:

```rust
#[tokio::test]
async fn galileo_schedule_tools_list_get_and_propose_updates() {
    use rig::tool::{Tool, ToolContext};

    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(structured_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let trace = ai::agent::new_trace();
    let list = ai::tools::ListSchedules {
        pool: pool.clone(),
        workspace_id: scratch.workspace_id,
        user_id: scratch.user_id,
        trace: trace.clone(),
    };
    let listed: Value = serde_json::from_str(
        &list
            .call(
                &mut ToolContext::new(),
                ai::tools::ListSchedulesArgs {
                    query: Some("daily".to_string()),
                    enabled: None,
                    limit: None,
                },
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(listed["items"][0]["id"], json!(schedule_id));
    assert_eq!(listed["items"][0]["can_edit"], json!(true));

    let get = ai::tools::GetSchedule {
        pool: pool.clone(),
        workspace_id: scratch.workspace_id,
        user_id: scratch.user_id,
        trace: trace.clone(),
    };
    let detail: Value = serde_json::from_str(
        &get.call(
            &mut ToolContext::new(),
            ai::tools::GetScheduleArgs {
                schedule_id: schedule_id.to_string(),
            },
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["spec"]["description"], json!("Summarize overdue work items for the team"));
    assert_eq!(detail["can_edit"], json!(true));

    let member = scratch.add_actor(&pool, 15).await;
    let member_list = ai::tools::ListSchedules {
        pool: pool.clone(),
        workspace_id: scratch.workspace_id,
        user_id: member,
        trace: ai::agent::new_trace(),
    };
    let member_view: Value = serde_json::from_str(
        &member_list
            .call(
                &mut ToolContext::new(),
                ai::tools::ListSchedulesArgs {
                    query: None,
                    enabled: None,
                    limit: None,
                },
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(member_view["items"][0]["can_edit"], json!(false));

    let update = ai::tools::UpdateSchedule {
        trace: trace.clone(),
    };
    update
        .call(
            &mut ToolContext::new(),
            ai::tools::UpdateScheduleArgs {
                schedule_id: schedule_id.to_string(),
                frequency: Some("weekly".to_string()),
                day_of_week: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let action = ai::agent::pending_action(&trace).expect("update proposal");
    assert_eq!(action["kind"], json!("update_schedule"));
    assert_eq!(action["proposal"]["schedule_id"], json!(schedule_id.to_string()));
    assert_eq!(action["proposal"]["changes"]["frequency"], json!("weekly"));

    scratch.purge(&pool).await;
}
```

> NOTE (execution): `pending_action` only learns about `update_schedule` in
> Task 5, so the DB test above asserts on the raw trace instead:
>
> ```rust
> let recorded = trace.lock().unwrap().clone();
> let last = recorded.last().expect("update proposal recorded");
> assert_eq!(last.name, "update_schedule");
> assert_eq!(last.arguments["schedule_id"], json!(schedule_id.to_string()));
> assert_eq!(last.arguments["changes"]["frequency"], json!("weekly"));
> assert_eq!(last.arguments["changes"]["day_of_week"], json!(2));
> ```
>
> `pending_action` coverage for updates lives in Task 5's
> `agent_pending_action` test.

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p ai update_args -- --nocapture
```

Expected: compile error because `schedules.rs` does not exist.

Run the API test:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_schedule_test galileo_schedule_tools -- --test-threads=1
```

Expected: compile error for missing tool types.

- [ ] **Step 3: Create `schedules.rs`**

Create `apps/api-rs/crates/ai/src/tools/schedules.rs`:

```rust
//! Schedule management tools: two read tools plus an `update_schedule`
//! proposal. The proposal never touches the database; the UI confirms it.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{db_error, optional_text, schema_of};
use crate::agent::{record, ToolTrace};
use crate::schedule::{ScheduleChanges, ScheduleSpec};

pub const UPDATE_SCHEDULE_NAME: &str = "update_schedule";
const SCHEDULE_LIST_DEFAULT: i64 = 20;
const SCHEDULE_LIST_MAX: i64 = 50;

pub const LIST_SCHEDULES_SQL: &str = "SELECT s.id, s.name, s.enabled, s.spec, s.prompt, \
     s.frequency, s.time_of_day, s.day_of_week, s.day_of_month, s.timezone, s.next_run_at, \
     (s.created_by_id = $2 OR EXISTS ( \
       SELECT 1 FROM workspace_members wm \
       WHERE wm.workspace_id = s.workspace_id AND wm.member_id = $2 \
         AND wm.role = 20 AND wm.is_active = true)) AS can_edit \
     FROM ai_schedules s \
     WHERE s.workspace_id = $1 AND s.deleted_at IS NULL \
       AND ($3::text IS NULL OR s.name ILIKE '%' || $3 || '%') \
       AND ($4::bool IS NULL OR s.enabled = $4) \
     ORDER BY s.created_at DESC, s.id LIMIT $5";

#[derive(sqlx::FromRow)]
struct ScheduleListRow {
    id: Uuid,
    name: String,
    enabled: bool,
    spec: Option<Value>,
    prompt: String,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
    next_run_at: chrono::DateTime<chrono::Utc>,
    can_edit: bool,
}

fn schedule_summary(row: &ScheduleListRow) -> Value {
    let description = row.spec.as_ref().and_then(|value| {
        serde_json::from_value::<ScheduleSpec>(value.clone())
            .ok()
            .map(|spec| spec.description)
    });
    json!({
        "id": row.id,
        "name": row.name,
        "enabled": row.enabled,
        "description": description,
        "frequency": row.frequency,
        "time": row.time_of_day,
        "day_of_week": row.day_of_week,
        "day_of_month": row.day_of_month,
        "timezone": row.timezone,
        "next_run_at": row.next_run_at,
        "can_edit": row.can_edit,
    })
}

fn schedule_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(SCHEDULE_LIST_DEFAULT).clamp(1, SCHEDULE_LIST_MAX)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListSchedulesArgs {
    /// Case-insensitive substring matched against the schedule name.
    pub query: Option<String>,
    /// Filter by enabled/paused state.
    pub enabled: Option<bool>,
    /// Maximum schedules to return, 1-50 (default 20).
    pub limit: Option<i64>,
}

pub struct ListSchedules {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListSchedules {
    const NAME: &'static str = "list_schedules";
    type Args = ListSchedulesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List saved AI schedules in this workspace, newest first. Use this to \
         find the schedule the user wants to change; can_edit is false when the \
         signed-in user is neither the creator nor a workspace admin."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListSchedulesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let rows: Vec<ScheduleListRow> = sqlx::query_as(LIST_SCHEDULES_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(&query)
            .bind(args.enabled)
            .bind(schedule_limit(args.limit))
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let items: Vec<Value> = rows.iter().map(schedule_summary).collect();
        Ok(json!({"items": items, "returned": items.len()}).to_string())
    }
}

pub const GET_SCHEDULE_SQL: &str = "SELECT s.id, s.name, s.enabled, s.spec, s.prompt, \
     s.frequency, s.time_of_day, s.day_of_week, s.day_of_month, s.timezone, s.next_run_at, \
     (s.created_by_id = $3 OR EXISTS ( \
       SELECT 1 FROM workspace_members wm \
       WHERE wm.workspace_id = s.workspace_id AND wm.member_id = $3 \
         AND wm.role = 20 AND wm.is_active = true)) AS can_edit \
     FROM ai_schedules s \
     WHERE s.workspace_id = $1 AND s.id = $2 AND s.deleted_at IS NULL";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetScheduleArgs {
    /// Schedule UUID returned by list_schedules.
    pub schedule_id: String,
}

pub struct GetSchedule {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetSchedule {
    const NAME: &'static str = "get_schedule";
    type Args = GetScheduleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Load one saved AI schedule, including its full recipe and raw prompt. \
         Use this before proposing a change so the proposal is grounded in the \
         current values."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetScheduleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let schedule_id = Uuid::parse_str(args.schedule_id.trim()).map_err(|_| {
            ToolExecutionError::invalid_args("schedule_id must be a uuid")
        })?;
        let row: Option<ScheduleListRow> = sqlx::query_as(GET_SCHEDULE_SQL)
            .bind(self.workspace_id)
            .bind(schedule_id)
            .bind(self.user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Err(ToolExecutionError::invalid_args("schedule was not found"));
        };
        let mut value = schedule_summary(&row);
        value["spec"] = row.spec;
        value["prompt"] = json!(row.prompt);
        Ok(value.to_string())
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct UpdateScheduleArgs {
    /// Schedule UUID returned by list_schedules.
    pub schedule_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub how_to: Option<Vec<String>>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub expected_output: Option<String>,
    /// Raw prompt for legacy schedules only; never mix with recipe fields.
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub frequency: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub day_of_week: Option<i16>,
    #[serde(default)]
    pub day_of_month: Option<i16>,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateScheduleProposal {
    pub schedule_id: String,
    pub changes: ScheduleChanges,
}

impl UpdateScheduleArgs {
    fn into_proposal(self) -> Result<UpdateScheduleProposal, ToolExecutionError> {
        let schedule_id = self.schedule_id.trim();
        Uuid::parse_str(schedule_id)
            .map_err(|_| ToolExecutionError::invalid_args("schedule_id must be a uuid"))?;
        let mut changes = ScheduleChanges {
            name: self.name,
            description: self.description,
            how_to: self.how_to,
            tools: self.tools,
            expected_output: self.expected_output,
            prompt: self.prompt,
            frequency: self.frequency,
            time: self.time,
            day_of_week: self.day_of_week,
            day_of_month: self.day_of_month,
            timezone: self.timezone,
        };
        if let Some(frequency) = changes.frequency.as_mut() {
            *frequency = frequency.trim().to_ascii_lowercase();
        }
        changes
            .validate()
            .map_err(ToolExecutionError::invalid_args)?;
        Ok(UpdateScheduleProposal {
            schedule_id: schedule_id.to_string(),
            changes,
        })
    }
}

pub struct UpdateSchedule {
    pub trace: ToolTrace,
}

impl Tool for UpdateSchedule {
    const NAME: &'static str = UPDATE_SCHEDULE_NAME;
    type Args = UpdateScheduleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing one saved AI schedule. First call list_schedules and \
         get_schedule; if more than one schedule could match, ask the user. Send \
         the schedule_id and only the fields that change. Structured recipe \
         fields cannot be mixed with a raw prompt; the raw prompt is only for \
         legacy schedules. Pause/resume/delete/run-now are not tools. The user \
         must confirm the before/after card before anything is saved; never \
         claim the schedule was updated before that confirmation."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateScheduleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = args.into_proposal()?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateScheduleProposal serializes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::new_trace;

    #[test]
    fn list_schedules_sql_is_workspace_scoped() {
        assert!(LIST_SCHEDULES_SQL.contains("workspace_id = $1"));
        assert!(LIST_SCHEDULES_SQL.contains("deleted_at IS NULL"));
        assert!(LIST_SCHEDULES_SQL.contains("role = 20"));
    }

    #[test]
    fn update_args_reject_empty_and_mixed_prompt() {
        let empty = UpdateScheduleArgs {
            schedule_id: Uuid::nil().to_string(),
            ..Default::default()
        };
        assert!(empty
            .into_proposal()
            .unwrap_err()
            .to_string()
            .contains("at least one"));

        let mixed = UpdateScheduleArgs {
            schedule_id: Uuid::nil().to_string(),
            prompt: Some("raw".to_string()),
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        assert!(mixed
            .into_proposal()
            .unwrap_err()
            .to_string()
            .contains("cannot be combined"));
    }

    #[tokio::test]
    async fn update_schedule_records_a_normalized_proposal() {
        let trace = new_trace();
        let tool = UpdateSchedule { trace: trace.clone() };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                UpdateScheduleArgs {
                    schedule_id: Uuid::nil().to_string(),
                    frequency: Some("WEEKLY".to_string()),
                    day_of_week: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["schedule_id"], json!(Uuid::nil().to_string()));
        assert_eq!(parsed["changes"]["frequency"], json!("weekly"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded[0].name, "update_schedule");
    }
}
```

- [ ] **Step 4: Register the tools**

In `apps/api-rs/crates/ai/src/tools/mod.rs`:

```rust
pub mod schedules;
pub use schedules::*;
```

Change the proposal allowlist:

```rust
pub const PROPOSAL_TOOL_NAMES: [&str; 19] = [
    CREATE_SCHEDULE_NAME,
    UPDATE_SCHEDULE_NAME,
    CREATE_WORK_ITEM_NAME,
    UPDATE_WORK_ITEM_NAME,
    ADD_COMMENT_NAME,
    MANAGE_SERVICE_LINKS_NAME,
    MANAGE_SERVICE_DEPENDENCIES_NAME,
    MANAGE_SPRINT_ITEMS_NAME,
    MANAGE_TRACK_ITEMS_NAME,
    CREATE_SERVICE_NAME,
    UPDATE_SERVICE_NAME,
    CREATE_SPRINT_NAME,
    UPDATE_SPRINT_NAME,
    CREATE_TRACK_NAME,
    UPDATE_TRACK_NAME,
    CREATE_ARTICLE_NAME,
    UPDATE_ARTICLE_NAME,
    APPLY_TRIAGE_SUGGESTION_NAME,
    TRIAGE_INTAKE_ITEM_NAME,
];
```

Replace the existing `CreateSchedule` registration in `workspace_tools()` with this block (the three new tools come first, then `CreateSchedule`):

```rust
        .tool(ListSchedules {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetSchedule {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(UpdateSchedule {
            trace: trace.clone(),
        })
        .tool(CreateSchedule {
            trace: trace.clone(),
        })
```

Update the `workspace_tools` doc comment to say it also exposes schedule management tools.

- [ ] **Step 5: Run the tool tests to verify they pass**

Run:

```bash
cargo test -p ai schedules -- --nocapture
```

Expected: PASS.

Run the DB tool test:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_schedule_test galileo_schedule_tools -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/schedules.rs apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/api/tests/ai_schedule_test.rs
git commit -m "feat(ai): add Galileo schedule management tools"
```

---

### Task 5: Pending action, preamble, proposal metadata, decision allowlist

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs`
- Modify: `apps/api-rs/crates/ai/tests/agent_pending_action.rs`
- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`
- Test: same files plus `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Write the failing tests**

In `apps/api-rs/crates/ai/tests/agent_pending_action.rs`:

```rust
use ai::tools::{
    CreateSchedule, CreateScheduleArgs, UpdateSchedule, UpdateScheduleArgs,
};
```

Add:

```rust
#[tokio::test]
async fn pending_action_returns_the_last_schedule_action() {
    let trace = new_trace();
    record(&trace, "list_projects", &json!({}));

    let create = CreateSchedule {
        trace: trace.clone(),
    };
    create
        .call(
            &mut ToolContext::new(),
            CreateScheduleArgs {
                name: "Daily".to_string(),
                description: "Report".to_string(),
                how_to: vec!["Count".to_string()],
                tools: vec!["count_work_items".to_string()],
                expected_output: "Summary".to_string(),
                frequency: "daily".to_string(),
                time: None,
                day_of_week: None,
                day_of_month: None,
                timezone: None,
            },
        )
        .await
        .unwrap();

    let update = UpdateSchedule {
        trace: trace.clone(),
    };
    update
        .call(
            &mut ToolContext::new(),
            UpdateScheduleArgs {
                schedule_id: "00000000-0000-0000-0000-000000000001".to_string(),
                frequency: Some("weekly".to_string()),
                day_of_week: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let action = pending_action(&trace).expect("proposal recorded");
    assert_eq!(action["kind"], json!("update_schedule"));
    assert_eq!(
        action["proposal"]["schedule_id"],
        json!("00000000-0000-0000-0000-000000000001")
    );
    assert_eq!(action["proposal"]["changes"]["day_of_week"], json!(2));
}
```

In `apps/api-rs/crates/api/src/routes/ai_conversations.rs` tests:

```rust
    #[test]
    fn metadata_patch_accepts_schedule_update_decision() {
        let clean = clean_metadata_patch(&patch_map(vec![(
            "schedule_update_decision",
            json!("applied"),
        )]))
        .expect("valid patch");
        assert_eq!(clean["schedule_update_decision"], json!("applied"));

        let pending = clean_metadata_patch(&patch_map(vec![(
            "schedule_update_decision",
            json!("pending"),
        )]))
        .unwrap_err();
        assert!(pending.contains("applied"));
    }
```

In `apps/api-rs/crates/api/tests/ai_agent_test.rs`:

```rust
#[tokio::test]
async fn schedule_update_metadata_carries_before_and_permission() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;
    let member = scratch.add_actor(&pool, 15).await;

    let (_, Json(created)) = api::routes::ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "name": "Daily report",
            "prompt": "Summarize overdue work items",
            "frequency": "daily",
            "time": "09:00",
            "timezone": "UTC",
            "proposal_key": Uuid::new_v4(),
        })),
    )
    .await
    .unwrap();
    let schedule_id = created["id"].as_str().unwrap().to_string();
    let proposal = json!({
        "schedule_id": schedule_id,
        "changes": {"frequency": "weekly", "day_of_week": 2}
    });

    let creator_meta = api::routes::ai_agent::schedule_update_metadata(
        &pool,
        &scratch.slug,
        scratch.user_id,
        Some(20),
        &proposal,
    )
    .await
    .expect("creator metadata");
    assert_eq!(creator_meta["can_edit"], json!(true));
    assert_eq!(creator_meta["before"]["name"], json!("Daily report"));
    assert_eq!(creator_meta["changes"]["frequency"], json!("weekly"));

    let member_meta = api::routes::ai_agent::schedule_update_metadata(
        &pool,
        &scratch.slug,
        member,
        Some(15),
        &proposal,
    )
    .await
    .expect("member metadata");
    assert_eq!(member_meta["can_edit"], json!(false));

    let missing = api::routes::ai_agent::schedule_update_metadata(
        &pool,
        &scratch.slug,
        scratch.user_id,
        Some(20),
        &json!({"schedule_id": Uuid::new_v4().to_string(), "changes": {}}),
    )
    .await
    .expect("missing metadata");
    assert_eq!(missing["before"], json!(null));
    assert_eq!(missing["can_edit"], json!(false));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p ai pending_action_returns_the_last_schedule_action -- --nocapture
```

Expected: FAIL; `pending_action` only finds `create_schedule`.

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test schedule_update_metadata -- --test-threads=1
```

Expected: compile error because `schedule_update_metadata` does not exist.

- [ ] **Step 3: Implement pending action and preamble**

In `apps/api-rs/crates/ai/src/agent.rs`:

```rust
use crate::tools::{
    CREATE_SCHEDULE_NAME, PROPOSAL_TOOL_NAMES, UPDATE_SCHEDULE_NAME,
};

const SCHEDULE_ACTION_NAMES: [&str; 2] = [CREATE_SCHEDULE_NAME, UPDATE_SCHEDULE_NAME];
```

Replace `pending_action`:

```rust
/// The last schedule proposal (create or update) recorded during an agent run.
pub fn pending_action(trace: &ToolTrace) -> Option<Value> {
    let recorded = trace.lock().ok()?;
    recorded
        .iter()
        .rev()
        .find(|call| SCHEDULE_ACTION_NAMES.contains(&call.name.as_str()))
        .map(|call| {
            serde_json::json!({
                "kind": call.name,
                "proposal": call.arguments.clone(),
            })
        })
}
```

Update the `PREAMBLE` text:

- Change `read-only, except create_schedule and create_work_item` to `read-only, except create_schedule, update_schedule, and create_work_item`.
- After the `/schedule` paragraph, insert:

```text
When the user asks to change an existing schedule, call list_schedules first, then get_schedule for the chosen one. If more than one schedule could match, ask which one. Then call update_schedule once with its schedule_id and only the fields that change. It is a proposal: the user must confirm the before/after card, so never say the schedule was updated before that confirmation. Pause, resume, delete, and run-now are Scheduler page actions, not tools; tell the user to use the Scheduler page for those.
```

- [ ] **Step 4: Add the metadata helper and handler branch**

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`:

Add imports:

```rust
use crate::routes::ai_schedule::{can_manage, load_schedule, schedule_json};
```

Add the helper near `proposal_metadata`:

```rust
/// Enrich an `update_schedule` proposal with the target's current values and
/// whether the signed-in user can manage it. `before` is null when the schedule
/// no longer exists.
pub async fn schedule_update_metadata(
    pool: &sqlx::PgPool,
    slug: &str,
    user_id: Uuid,
    role: Option<i16>,
    proposal: &Value,
) -> Option<Value> {
    let schedule_id = proposal
        .get("schedule_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())?;
    let row = load_schedule(pool, slug, schedule_id).await.ok().flatten();
    let can_edit = row
        .as_ref()
        .is_some_and(|row| can_manage(row.created_by_id, user_id, role));
    Some(json!({
        "schedule_id": schedule_id,
        "changes": proposal.get("changes").cloned().unwrap_or_else(|| json!({})),
        "before": row.as_ref().map(schedule_json),
        "can_edit": can_edit,
    }))
}
```

Replace the metadata block in `workspace_ai_agent`:

```rust
            let mut metadata = json!({ "is_error": false });
            if let Some(action) = action.as_ref() {
                match action["kind"].as_str() {
                    Some("create_schedule") => {
                        metadata["schedule_proposal"] = action["proposal"].clone();
                        metadata["schedule_proposal_key"] = json!(Uuid::new_v4());
                        metadata["schedule_decision"] = json!("pending");
                    }
                    Some("update_schedule") => {
                        if let Some(proposal) = schedule_update_metadata(
                            &st.pool,
                            &slug,
                            auth.0,
                            role,
                            &action["proposal"],
                        )
                        .await
                        {
                            metadata["schedule_update_proposal"] = proposal;
                            metadata["schedule_update_decision"] = json!("pending");
                        }
                    }
                    _ => {}
                }
            }
```

- [ ] **Step 5: Add the metadata allowlist**

In `apps/api-rs/crates/api/src/routes/ai_conversations.rs`, inside `clean_metadata_patch`:

```rust
            "schedule_update_decision" => match value.as_str() {
                Some("applied") | Some("cancelled") => {
                    clean.insert(key.clone(), value.clone());
                }
                _ => {
                    return Err(
                        "schedule_update_decision must be 'applied' or 'cancelled'".to_string(),
                    )
                }
            },
```

Update the doc comment above `patch_message` to mention `schedule_update_decision`.

- [ ] **Step 6: Run the tests to verify they pass**

Run:

```bash
cargo test -p ai pending_action -- --nocapture
cargo test -p api --lib metadata_patch_accepts_schedule_update_decision -- --nocapture
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test schedule_update_metadata -- --test-threads=1
```

Expected: PASS.

Run the existing proposal roundtrip tests to catch regressions:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test agent_proposal_metadata_is_persisted_and_returned -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/ai/src/agent.rs apps/api-rs/crates/ai/tests/agent_pending_action.rs apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/src/routes/ai_conversations.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api): surface Galileo schedule edit proposals"
```

---

### Task 6: Frontend types, service, and scheduler store

**Files:**

- Modify: `apps/web/core/lib/ai-schedule.ts`
- Modify: `apps/web/core/services/ai-schedules.service.ts`
- Modify: `apps/web/core/store/ai-schedules.store.ts`
- Test: `apps/web/core/lib/ai-schedule.test.ts`, `apps/web/core/store/ai-schedules.store.test.ts`

- [ ] **Step 1: Write the failing tests**

In `apps/web/core/lib/ai-schedule.test.ts`, add imports:

```ts
  diffScheduleUpdate,
  validateScheduleBasics,
```

Add:

```ts
describe("validateScheduleBasics", () => {
  const base = {
    name: "Daily report",
    frequency: "daily" as const,
    time: "09:00",
    day_of_week: null,
    day_of_month: null,
    timezone: "Asia/Jakarta",
  };

  it("accepts a valid cadence", () => {
    expect(validateScheduleBasics(base)).toBeNull();
  });

  it("rejects missing or invalid values", () => {
    expect(validateScheduleBasics({ ...base, name: "  " })).toBe("Name is required.");
    expect(validateScheduleBasics({ ...base, time: "9:0" })).toBe("Time must be HH:MM (24h).");
    expect(validateScheduleBasics({ ...base, frequency: "weekly", day_of_week: null })).toBe("Choose a weekday.");
    expect(validateScheduleBasics({ ...base, frequency: "monthly", day_of_month: null })).toBe(
      "Choose a day of month (1-31)."
    );
    expect(validateScheduleBasics({ ...base, timezone: "Mars/Olympus" })).toBe("Unknown timezone: Mars/Olympus");
  });
});

describe("diffScheduleUpdate", () => {
  const before: TAiSchedule = {
    id: "s1",
    name: "Daily report",
    frequency: "daily",
    time: "09:00",
    day_of_week: null,
    day_of_month: null,
    timezone: "UTC",
    enabled: true,
    next_run_at: "2026-10-21T09:00:00Z",
    created_by_id: "u1",
    created_at: "2026-10-01T00:00:00Z",
    prompt: "Old prompt",
    spec: {
      version: 1,
      description: "Old description",
      how_to: ["Old step"],
      tools: ["count_work_items"],
      expected_output: "Old output",
    },
  };

  it("renders only changed fields", () => {
    const rows = diffScheduleUpdate({
      schedule_id: "s1",
      changes: {
        name: "Weekly report",
        frequency: "weekly",
        day_of_week: 2,
        tools: ["search_work_items"],
      },
      before,
      can_edit: true,
    });
    expect(rows.find((row) => row.label === "Name")).toEqual({
      label: "Name",
      before: "Daily report",
      after: "Weekly report",
    });
    expect(rows.find((row) => row.label === "Cadence")?.after).toBe("Every Tuesday at 09:00 · UTC");
    expect(rows.find((row) => row.label === "Tools")?.after).toBe("search_work_items");
    expect(rows.find((row) => row.label === "Description")).toBeUndefined();
  });

  it("returns no rows without a before snapshot", () => {
    expect(diffScheduleUpdate({ schedule_id: "s1", changes: { name: "x" }, before: null, can_edit: false })).toEqual(
      []
    );
  });
});
```

In `apps/web/core/store/ai-schedules.store.test.ts`, update `makeService`:

```ts
  update: vi.fn(async () => ({ id: "s1", name: "Daily", enabled: true })),
```

Change the toggle assertion:

```ts
expect(service.update).toHaveBeenCalledWith("acme", "s1", { enabled: false });
```

Add:

```ts
it("updates a schedule and merges the response while preserving run fields", async () => {
  const service = makeService();
  service.update = vi.fn(async () => ({ id: "s1", name: "Weekly", frequency: "weekly" })) as never;
  const store = new AiSchedulesStore(service as never);
  store.schedules = [
    {
      id: "s1",
      name: "Daily",
      enabled: true,
      last_status: "success",
      last_run_at: "2026-10-01T09:00:00Z",
      runs: [{ id: "r1" }],
    } as never,
  ];

  const updated = await store.updateSchedule("acme", "s1", { frequency: "weekly" });

  expect(service.update).toHaveBeenCalledWith("acme", "s1", { frequency: "weekly" });
  expect(updated.name).toBe("Weekly");
  expect(store.schedules[0]).toMatchObject({
    name: "Weekly",
    frequency: "weekly",
    last_status: "success",
    last_run_at: "2026-10-01T09:00:00Z",
  });
});

it("propagates update errors", async () => {
  const service = makeService();
  service.update = vi.fn(async () => {
    throw new Error("nope");
  }) as never;
  const store = new AiSchedulesStore(service as never);
  store.schedules = [{ id: "s1", enabled: true } as never];
  await expect(store.updateSchedule("acme", "s1", { name: "x" })).rejects.toThrow("nope");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
pnpm --filter web test -- core/lib/ai-schedule.test.ts core/store/ai-schedules.store.test.ts
```

Expected: FAIL; `validateScheduleBasics`, `diffScheduleUpdate`, and `updateSchedule` are missing.

- [ ] **Step 3: Implement the types and helpers**

In `apps/web/core/lib/ai-schedule.ts`, after `TAiScheduleProposal`:

```ts
export type TAiScheduleUpdateChanges = {
  name?: string;
  description?: string;
  how_to?: string[];
  tools?: TAiScheduleTool[];
  expected_output?: string;
  prompt?: string;
  frequency?: TAiScheduleFrequency;
  time?: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone?: string;
};

export type TAiScheduleUpdate = TAiScheduleUpdateChanges & { enabled?: boolean };

export type TAiScheduleUpdateProposal = {
  schedule_id: string;
  changes: TAiScheduleUpdateChanges;
  before: TAiSchedule | null;
  can_edit: boolean;
};

export type TAiScheduleDiffRow = {
  label: string;
  before: string;
  after: string;
};
```

Replace `TAiAgentPendingAction`:

```ts
export type TAiAgentPendingAction =
  | { kind: "create_schedule"; proposal: TAiScheduleProposal }
  | {
      kind: "update_schedule";
      proposal: { schedule_id: string; changes: TAiScheduleUpdateChanges };
    };
```

Add after `validateScheduleSpec`:

```ts
export const validateScheduleBasics = (draft: {
  name: string;
  frequency: TAiScheduleFrequency;
  time: string;
  day_of_week?: number | null;
  day_of_month?: number | null;
  timezone: string;
}): string | null => {
  const name = draft.name.trim();
  if (!name) return "Name is required.";
  if (name.length > 120) return "Name must be at most 120 characters.";
  if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(draft.time)) return "Time must be HH:MM (24h).";
  if (draft.frequency === "weekly" && !(draft.day_of_week && draft.day_of_week >= 1 && draft.day_of_week <= 7)) {
    return "Choose a weekday.";
  }
  if (draft.frequency === "monthly" && !(draft.day_of_month && draft.day_of_month >= 1 && draft.day_of_month <= 31)) {
    return "Choose a day of month (1-31).";
  }
  const timezone = draft.timezone.trim();
  if (!timezone) return "Timezone is required.";
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: timezone });
  } catch {
    return `Unknown timezone: ${timezone}`;
  }
  return null;
};
```

Add after `scheduleDescription`:

```ts
export const diffScheduleUpdate = (proposal: TAiScheduleUpdateProposal): TAiScheduleDiffRow[] => {
  const { before, changes } = proposal;
  if (!before) return [];
  const rows: TAiScheduleDiffRow[] = [];
  const push = (label: string, previous: string, next: string | undefined) => {
    if (next === undefined || previous === next) return;
    rows.push({ label, before: previous || "—", after: next || "—" });
  };

  push("Name", before.name, changes.name);

  const cadenceChanged = [
    changes.frequency,
    changes.time,
    changes.day_of_week,
    changes.day_of_month,
    changes.timezone,
  ].some((value) => value !== undefined);
  if (cadenceChanged) {
    const after = humanizeSchedule({
      frequency: changes.frequency ?? before.frequency,
      time: changes.time ?? before.time,
      day_of_week: changes.day_of_week !== undefined ? changes.day_of_week : before.day_of_week,
      day_of_month: changes.day_of_month !== undefined ? changes.day_of_month : before.day_of_month,
      timezone: changes.timezone ?? before.timezone,
    });
    push("Cadence", humanizeSchedule(before), after);
  }

  push("Description", before.spec?.description ?? "", changes.description);
  if (changes.how_to !== undefined) {
    const previous = (before.spec?.how_to ?? []).map((step, index) => `${index + 1}. ${step}`).join("\n");
    const next = changes.how_to.map((step, index) => `${index + 1}. ${step}`).join("\n");
    push("Steps", previous, next);
  }
  if (changes.tools !== undefined) {
    push("Tools", (before.spec?.tools ?? []).join(", "), changes.tools.join(", "));
  }
  push("Expected output", before.spec?.expected_output ?? "", changes.expected_output);
  push("Prompt", before.prompt, changes.prompt);
  return rows;
};
```

- [ ] **Step 4: Implement the service and store**

In `apps/web/core/services/ai-schedules.service.ts`, import `TAiScheduleUpdate` and replace `update`:

```ts
  async update(workspaceSlug: string, scheduleId: string, changes: TAiScheduleUpdate): Promise<TAiSchedule> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`, changes)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
```

In `apps/web/core/store/ai-schedules.store.ts`:

- Import `TAiScheduleUpdate`.
- Add to `IAiSchedulesStore`:

```ts
updateSchedule: (workspaceSlug: string, scheduleId: string, changes: TAiScheduleUpdate) => Promise<TAiSchedule>;
```

- Add to `makeObservable`:

```ts
      updateSchedule: action,
```

- Change `toggleSchedule`:

```ts
await this.service.update(workspaceSlug, scheduleId, { enabled });
```

- Add after `fetchRuns`:

```ts
updateSchedule = async (workspaceSlug: string, scheduleId: string, changes: TAiScheduleUpdate) => {
  const updated = await this.service.update(workspaceSlug, scheduleId, changes);
  runInAction(() => {
    this.schedules = this.schedules.map((schedule) =>
      schedule.id === scheduleId ? { ...schedule, ...updated } : schedule
    );
  });
  return updated;
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run:

```bash
pnpm --filter web test -- core/lib/ai-schedule.test.ts core/store/ai-schedules.store.test.ts
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts apps/web/core/services/ai-schedules.service.ts apps/web/core/store/ai-schedules.store.ts apps/web/core/store/ai-schedules.store.test.ts
git commit -m "feat(web): add schedule update types, service, and store"
```

---

### Task 7: Scheduler edit UI

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/schedule-recipe-fields.tsx`
- Create: `apps/web/core/components/ai-scheduler/schedule-edit-modal.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx`
- Modify: `apps/web/core/components/ai-scheduler/schedule-detail-modal.tsx`

- [ ] **Step 1: Create the shared recipe fields component**

Create `apps/web/core/components/ai/assistant-sidebar/schedule-recipe-fields.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { Button } from "@plane/propel/button";
import { v4 as uuidv4 } from "uuid";
import { AI_SCHEDULE_TOOLS, type TAiScheduleSpec, type TAiScheduleTool } from "@/lib/ai-schedule";

type Props = {
  value: Pick<TAiScheduleSpec, "description" | "how_to" | "tools" | "expected_output">;
  onChange: (patch: Partial<TAiScheduleSpec>) => void;
};

export function ScheduleRecipeFields({ value, onChange }: Props) {
  const [stepKeys, setStepKeys] = useState<string[]>(() => value.how_to.map(() => uuidv4()));

  const updateStep = (index: number, step: string) => {
    const howTo = [...value.how_to];
    howTo[index] = step;
    onChange({ how_to: howTo });
  };
  const removeStep = (index: number) => {
    onChange({ how_to: value.how_to.filter((_, stepIndex) => stepIndex !== index) });
    setStepKeys((keys) => keys.filter((_, keyIndex) => keyIndex !== index));
  };
  const addStep = () => {
    onChange({ how_to: [...value.how_to, ""] });
    setStepKeys((keys) => [...keys, uuidv4()]);
  };
  const toggleTool = (tool: TAiScheduleTool, checked: boolean) =>
    onChange({
      tools: checked ? [...value.tools, tool] : value.tools.filter((current) => current !== tool),
    });

  return (
    <>
      <label className="mt-2 block text-12 text-secondary">
        Description
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={value.description}
          onChange={(event) => onChange({ description: event.target.value })}
        />
      </label>

      <div className="mt-2 text-12 text-secondary">
        Steps
        {value.how_to.map((step, index) => (
          <div key={stepKeys[index] ?? index} className="mt-1 flex items-center gap-1">
            <input
              className="w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={step}
              onChange={(event) => updateStep(index, event.target.value)}
            />
            <Button size="sm" variant="ghost" disabled={value.how_to.length === 1} onClick={() => removeStep(index)}>
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
                checked={value.tools.includes(tool)}
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
          value={value.expected_output}
          onChange={(event) => onChange({ expected_output: event.target.value })}
        />
      </label>
    </>
  );
}
```

- [ ] **Step 2: Reuse it in the create card**

In `schedule-proposal-card.tsx`:

- Remove `uuidv4`, `AI_SCHEDULE_TOOLS`, and `TAiScheduleTool` imports plus the `stepKeys` state.
- Remove `stepEntries`, `updateStep`, `removeStep`, `addStep`, `toggleTool`.
- Import `ScheduleRecipeFields`.
- Replace the Description/Steps/Tools/Expected output blocks (lines around 133–191) with:

```tsx
<ScheduleRecipeFields value={structured} onChange={patch} />
```

Keep the name input and `humanizeSchedule` line.

- [ ] **Step 3: Create the edit modal**

Create `apps/web/core/components/ai-scheduler/schedule-edit-modal.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { ModalCore } from "@plane/ui";
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import {
  humanizeSchedule,
  validateScheduleBasics,
  validateScheduleSpec,
  type TAiSchedule,
  type TAiScheduleFrequency,
  type TAiScheduleTool,
  type TAiScheduleUpdate,
} from "@/lib/ai-schedule";
import { ScheduleRecipeFields } from "../ai/assistant-sidebar/schedule-recipe-fields";

type Props = {
  schedule: TAiSchedule;
  isOpen: boolean;
  onClose: () => void;
};

type TDraft = {
  name: string;
  frequency: TAiScheduleFrequency;
  time: string;
  timezone: string;
  day_of_week: number | null;
  day_of_month: number | null;
  description: string;
  how_to: string[];
  tools: TAiScheduleTool[];
  expected_output: string;
  prompt: string;
};

const fromSchedule = (schedule: TAiSchedule): TDraft => ({
  name: schedule.name,
  frequency: schedule.frequency,
  time: schedule.time,
  timezone: schedule.timezone,
  day_of_week: schedule.day_of_week ?? (schedule.frequency === "weekly" ? 1 : null),
  day_of_month: schedule.day_of_month ?? (schedule.frequency === "monthly" ? 1 : null),
  description: schedule.spec?.description ?? "",
  how_to: schedule.spec?.how_to ?? [],
  tools: schedule.spec?.tools ?? [],
  expected_output: schedule.spec?.expected_output ?? "",
  prompt: schedule.prompt,
});

export const ScheduleEditModal = observer(function ScheduleEditModal({ schedule, isOpen, onClose }: Props) {
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const { updateSchedule } = useAiSchedules();
  const [draft, setDraft] = useState<TDraft>(() => fromSchedule(schedule));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setDraft(fromSchedule(schedule));
    setError(null);
  }, [isOpen, schedule.id]);

  const structured = Boolean(schedule.spec);
  const patch = (fields: Partial<TDraft>) => setDraft((current) => ({ ...current, ...fields }));

  const basicsError = validateScheduleBasics(draft);
  const recipeError = structured
    ? validateScheduleSpec({
        version: 1,
        description: draft.description,
        how_to: draft.how_to,
        tools: draft.tools,
        expected_output: draft.expected_output,
      })
    : !draft.prompt.trim()
      ? "Prompt is required."
      : draft.prompt.length > 2000
        ? "Prompt must be at most 2000 characters."
        : null;
  const validationError = basicsError ?? recipeError;

  const save = async () => {
    if (!slug || validationError) return;
    setSaving(true);
    setError(null);
    const common = {
      name: draft.name.trim(),
      frequency: draft.frequency,
      time: draft.time,
      timezone: draft.timezone.trim(),
      day_of_week: draft.frequency === "weekly" ? (draft.day_of_week ?? 1) : null,
      day_of_month: draft.frequency === "monthly" ? (draft.day_of_month ?? 1) : null,
    };
    const changes: TAiScheduleUpdate = structured
      ? {
          ...common,
          description: draft.description,
          how_to: draft.how_to,
          tools: draft.tools,
          expected_output: draft.expected_output,
        }
      : { ...common, prompt: draft.prompt };
    try {
      await updateSchedule(slug, schedule.id, changes);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Schedule updated" });
      onClose();
    } catch (err: any) {
      setError(err?.data?.error ?? "Could not update the schedule.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose}>
      <div className="max-h-[80vh] space-y-2 overflow-y-auto p-5">
        <h3 className="text-base font-semibold text-primary">Edit schedule</h3>
        <p className="text-xs text-tertiary">{humanizeSchedule(draft)}</p>

        <label className="block text-12 text-secondary">
          Name
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.name}
            onChange={(event) => patch({ name: event.target.value })}
          />
        </label>

        <div className="grid grid-cols-2 gap-2">
          <label className="text-12 text-secondary">
            Frequency
            <select
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.frequency}
              onChange={(event) => patch({ frequency: event.target.value as TAiScheduleFrequency })}
            >
              <option value="hourly">Hourly</option>
              <option value="daily">Daily</option>
              <option value="weekly">Weekly</option>
              <option value="monthly">Monthly</option>
            </select>
          </label>
          <label className="text-12 text-secondary">
            Time
            <input
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.time}
              onChange={(event) => patch({ time: event.target.value })}
              placeholder="09:00"
            />
          </label>
        </div>

        {draft.frequency === "weekly" && (
          <label className="block text-12 text-secondary">
            Weekday
            <select
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.day_of_week ?? 1}
              onChange={(event) => patch({ day_of_week: Number(event.target.value) })}
            >
              {["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"].map((label, index) => (
                <option key={label} value={index + 1}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        )}

        {draft.frequency === "monthly" && (
          <label className="block text-12 text-secondary">
            Day of month
            <input
              type="number"
              min={1}
              max={31}
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.day_of_month ?? 1}
              onChange={(event) => patch({ day_of_month: Number(event.target.value) })}
            />
          </label>
        )}

        <label className="block text-12 text-secondary">
          Timezone (IANA)
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.timezone}
            onChange={(event) => patch({ timezone: event.target.value })}
            placeholder="Asia/Jakarta"
          />
        </label>

        {structured ? (
          <ScheduleRecipeFields key={schedule.id} value={draft} onChange={patch} />
        ) : (
          <label className="mt-2 block text-12 text-secondary">
            Prompt (legacy)
            <textarea
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              rows={6}
              value={draft.prompt}
              onChange={(event) => patch({ prompt: event.target.value })}
            />
          </label>
        )}

        {validationError && <p className="text-12 text-danger-primary">{validationError}</p>}
        {error && <p className="text-12 text-danger-primary">{error}</p>}
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-subtle p-4">
        <Button size="sm" variant="secondary" disabled={saving} onClick={onClose}>
          Cancel
        </Button>
        <Button
          size="sm"
          variant="primary"
          loading={saving}
          disabled={Boolean(validationError)}
          onClick={() => void save()}
        >
          Save
        </Button>
      </div>
    </ModalCore>
  );
});
```

Remove unused imports (`AI_SCHEDULE_TOOLS` if the editor does not use it directly). Keep the import list clean for OxLint.

- [ ] **Step 4: Add the Edit button**

In `schedule-detail-modal.tsx`:

- Import `ScheduleEditModal`.
- Add `const [editOpen, setEditOpen] = useState(false);`.
- Add an Edit button before Pause:

```tsx
{
  canManage && (
    <Button size="sm" variant="secondary" onClick={() => setEditOpen(true)}>
      Edit
    </Button>
  );
}
```

- Render the edit modal before the delete confirmation:

```tsx
<ScheduleEditModal schedule={schedule} isOpen={editOpen} onClose={() => setEditOpen(false)} />
```

- [ ] **Step 5: Typecheck and run existing tests**

Run:

```bash
pnpm --filter web check:types
pnpm --filter web test -- core/lib/ai-schedule.test.ts core/store/ai-schedules.store.test.ts
```

Expected: typecheck PASS; tests PASS.

- [ ] **Step 6: Manual smoke test**

With the dev server on port 3000:

1. Open Scheduler, select a structured schedule, click Edit.
2. Change name, cadence, and one step; Save.
3. Confirm the detail modal and list update, and the next run reflects the new cadence.
4. Edit a paused schedule; confirm the next run does not change until Resume.
5. Create a schedule from `/schedule`; confirm the shared recipe fields still work in the create card.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/schedule-recipe-fields.tsx apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx apps/web/core/components/ai-scheduler/schedule-edit-modal.tsx apps/web/core/components/ai-scheduler/schedule-detail-modal.tsx
git commit -m "feat(web): add schedule edit modal"
```

---

### Task 8: Galileo update proposal card and assistant store

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts`
- Modify: `apps/web/core/lib/ai-conversations.ts`
- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Create: `apps/web/core/components/ai/assistant-sidebar/schedule-update-proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`
- Test: `apps/web/core/lib/ai-conversations.test.ts`, `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Write the failing tests**

In `apps/web/core/lib/ai-conversations.test.ts`, add:

```ts
it("maps schedule update metadata onto the message", () => {
  const message = toAiMessage(
    stored({
      metadata: {
        schedule_update_proposal: {
          schedule_id: "s1",
          changes: { frequency: "weekly", day_of_week: 2 },
          before: { id: "s1", name: "Daily", frequency: "daily", time: "09:00", timezone: "UTC" },
          can_edit: true,
        },
        schedule_update_decision: "pending",
      },
    })
  );
  expect(message.scheduleUpdateProposal?.schedule_id).toBe("s1");
  expect(message.scheduleUpdateProposal?.changes.frequency).toBe("weekly");
  expect(message.scheduleUpdateProposal?.can_edit).toBe(true);
  expect(message.scheduleUpdateDecision).toBe("pending");
});
```

In `apps/web/core/store/ai-assistant.store.test.ts`, add `update` to the schedules service mock:

```ts
  schedules: {
    create: vi.fn(async () => ({ id: "s1" })),
    update: vi.fn(async () => ({ id: "s1", name: "Daily", enabled: true })),
    ...overrides.schedules,
  },
```

Add a helper after `scheduleProposal`:

```ts
const scheduleUpdateProposal = {
  schedule_id: "s1",
  changes: { frequency: "weekly", day_of_week: 2, name: "Weekly report" },
  before: {
    id: "s1",
    name: "Daily report",
    frequency: "daily",
    time: "09:00",
    timezone: "UTC",
    day_of_week: null,
    day_of_month: null,
    enabled: true,
    next_run_at: "2026-10-21T09:00:00Z",
    created_by_id: "u1",
    created_at: "2026-10-01T00:00:00Z",
    prompt: "Summarize",
    spec: null,
  },
  can_edit: true,
};

const updateChatResponse = (conversationId: string, question: string) => {
  const response = chatResponse(conversationId, question, "proposal ready");
  return {
    ...response,
    assistant_message: {
      ...response.assistant_message,
      metadata: {
        schedule_update_proposal: scheduleUpdateProposal,
        schedule_update_decision: "pending" as const,
      },
    },
  };
};
```

Add tests near the existing schedule proposal tests:

```ts
it("applies a schedule update proposal and persists the decision", async () => {
  const services = makeServices({
    ai: {
      createAgentTask: vi.fn(async (_slug: string, data: any) => updateChatResponse(data.conversation_id, data.prompt)),
    },
  });
  const store = makeStore(services);
  store.setWorkspace("acme");
  await flush();
  store.setMode("agent");
  await flush();
  await store.sendMessage("ubah jadwal jadi mingguan");

  const message = store.messages.find((candidate) => candidate.scheduleUpdateProposal)!;
  expect(message.scheduleUpdateDecision).toBe("pending");

  await store.confirmScheduleUpdate(message.id);
  expect(services.schedules.update).toHaveBeenCalledWith("acme", "s1", scheduleUpdateProposal.changes);
  expect(message.scheduleUpdateDecision).toBe("applied");
  expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith("acme", "c-agent", message.id, {
    schedule_update_decision: "applied",
  });
});

it("cancels a schedule update proposal without calling the service", async () => {
  const services = makeServices({
    ai: {
      createAgentTask: vi.fn(async (_slug: string, data: any) => updateChatResponse(data.conversation_id, data.prompt)),
    },
  });
  const store = makeStore(services);
  store.setWorkspace("acme");
  await flush();
  store.setMode("agent");
  await flush();
  await store.sendMessage("ubah jadwal");

  const message = store.messages.find((candidate) => candidate.scheduleUpdateProposal)!;
  store.resolveScheduleUpdateProposal(message.id);
  await flush();
  expect(services.schedules.update).not.toHaveBeenCalled();
  expect(message.scheduleUpdateDecision).toBe("cancelled");
  expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith("acme", "c-agent", message.id, {
    schedule_update_decision: "cancelled",
  });
});

it("keeps the update proposal pending when apply fails", async () => {
  const services = makeServices({
    ai: {
      createAgentTask: vi.fn(async (_slug: string, data: any) => updateChatResponse(data.conversation_id, data.prompt)),
    },
    schedules: {
      update: vi.fn(async () => {
        throw new Error("boom");
      }),
    },
  });
  const store = makeStore(services);
  store.setWorkspace("acme");
  await flush();
  store.setMode("agent");
  await flush();
  await store.sendMessage("ubah jadwal");

  const message = store.messages.find((candidate) => candidate.scheduleUpdateProposal)!;
  await expect(store.confirmScheduleUpdate(message.id)).rejects.toThrow("boom");
  expect(message.scheduleUpdateDecision).toBe("pending");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
pnpm --filter web test -- core/lib/ai-conversations.test.ts core/store/ai-assistant.store.test.ts
```

Expected: FAIL; fields and methods are missing.

- [ ] **Step 3: Add message types and mapping**

In `apps/web/core/lib/ai-context.ts`:

```ts
import type { TAiScheduleProposal, TAiScheduleUpdateProposal } from "@/lib/ai-schedule";
```

Add to `TAiMessage`:

```ts
  scheduleUpdateProposal?: TAiScheduleUpdateProposal;
  scheduleUpdateDecision?: "pending" | "applied" | "cancelled";
```

In `apps/web/core/lib/ai-conversations.ts`:

```ts
import type { TAiScheduleProposal, TAiScheduleUpdateProposal } from "@/lib/ai-schedule";
```

Add to `TAiMessageMetadata`:

```ts
  schedule_update_proposal?: TAiScheduleUpdateProposal;
  schedule_update_decision?: "pending" | "applied" | "cancelled";
```

Add to `TAiMessageMetadataPatch`:

```ts
  schedule_update_decision?: "applied" | "cancelled";
```

Add to `toAiMessage`:

```ts
  scheduleUpdateProposal: stored.metadata?.schedule_update_proposal,
  scheduleUpdateDecision: stored.metadata?.schedule_update_decision,
```

- [ ] **Step 4: Implement the store methods**

In `apps/web/core/store/ai-assistant.store.ts`:

- Import type `TAiScheduleUpdateProposal` if needed.
- Change `type TAiSchedulesService = Pick<AiSchedulesService, "create" | "update">;`.
- Add to `IAIAssistantStore`:

```ts
  confirmScheduleUpdate: (messageId: string) => Promise<void>;
  resolveScheduleUpdateProposal: (messageId: string) => void;
```

- Add to `makeObservable`:

```ts
      confirmScheduleUpdate: action,
      resolveScheduleUpdateProposal: action,
```

- Add after `resolveScheduleProposal`:

```ts
confirmScheduleUpdate = async (messageId: string) => {
  const slug = this.workspaceSlug;
  const conversationId = this.activeConversationId;
  const message = this.messages.find((candidate) => candidate.id === messageId);
  const proposal = message?.scheduleUpdateProposal;
  if (!slug || !message || !proposal) return;
  if (message.scheduleUpdateDecision !== "pending") return;
  if (!proposal.can_edit || !proposal.before) return;
  await this.schedulesService.update(slug, proposal.schedule_id, proposal.changes);
  runInAction(() => {
    message.scheduleUpdateDecision = "applied";
  });
  await this.persistScheduleUpdateDecision(conversationId, message, "applied");
};

resolveScheduleUpdateProposal = (messageId: string) => {
  const conversationId = this.activeConversationId;
  const message = this.messages.find((candidate) => candidate.id === messageId);
  if (!message || message.scheduleUpdateDecision !== "pending") return;
  runInAction(() => {
    message.scheduleUpdateDecision = "cancelled";
  });
  void this.persistScheduleUpdateDecision(conversationId, message, "cancelled");
};
```

- Add a private helper near `persistDecision`:

```ts
  private persistScheduleUpdateDecision = async (
    conversationId: string | undefined,
    message: TAiMessage,
    decision: "applied" | "cancelled"
  ) => {
    const slug = this.workspaceSlug;
    if (!slug || !conversationId) return;
    try {
      await this.conversationsService.updateMessageMetadata(slug, conversationId, message.id, {
        schedule_update_decision: decision,
      });
    } catch {
      // best-effort: the schedule itself is already the source of truth
    }
  };
```

- In `request()`, leave `pending_action` handling unchanged: the enriched proposal is persisted on `assistant_message.metadata` and mapped by `toAiMessage`.

- [ ] **Step 5: Create the before/after card**

Create `apps/web/core/components/ai/assistant-sidebar/schedule-update-proposal-card.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { Button } from "@plane/propel/button";
import { diffScheduleUpdate, type TAiScheduleUpdateProposal } from "@/lib/ai-schedule";

type Props = {
  proposal: TAiScheduleUpdateProposal;
  decision?: "pending" | "applied" | "cancelled";
  onConfirm: () => Promise<void>;
  onCancel: () => void;
};

export function ScheduleUpdateProposalCard({ proposal, decision, onConfirm, onCancel }: Props) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const rows = diffScheduleUpdate(proposal);
  const canApply = proposal.can_edit && Boolean(proposal.before);

  if (decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule updated.{" "}
        {slug && (
          <Link href={`/${slug}/scheduler/`} className="text-accent-primary hover:underline">
            Open Scheduler
          </Link>
        )}
      </p>
    );
  }
  if (decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Schedule update cancelled.
      </p>
    );
  }
  if (decision !== "pending") return null;

  const apply = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm();
    } catch {
      setError("Could not update the schedule. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Schedule update proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-semibold break-words text-primary">{proposal.before?.name ?? "Unknown schedule"}</p>
      {!proposal.before && <p className="mt-1 text-12 text-danger-primary">This schedule no longer exists.</p>}
      {proposal.before && !proposal.can_edit && (
        <p className="mt-1 text-12 text-danger-primary">
          Only the schedule creator or a workspace admin can edit this schedule.
        </p>
      )}
      {rows.length > 0 ? (
        <div className="mt-2 space-y-1.5">
          {rows.map((row) => (
            <div key={row.label} className="text-12">
              <p className="font-medium text-secondary">{row.label}</p>
              <p className="whitespace-pre-wrap text-tertiary line-through">{row.before}</p>
              <p className="whitespace-pre-wrap text-primary">{row.after}</p>
            </div>
          ))}
        </div>
      ) : (
        <p className="mt-1 text-12 text-tertiary">No visible changes.</p>
      )}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button size="sm" variant="primary" loading={submitting} disabled={!canApply} onClick={() => void apply()}>
          Apply
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Render the card in the assistant sidebar**

In `root.tsx`, import:

```tsx
import { ScheduleUpdateProposalCard } from "./schedule-update-proposal-card";
```

After the `ScheduleProposalCard` block:

```tsx
{
  message.scheduleUpdateProposal && (
    <ScheduleUpdateProposalCard
      proposal={message.scheduleUpdateProposal}
      decision={message.scheduleUpdateDecision}
      onConfirm={() => confirmScheduleUpdate(message.id)}
      onCancel={() => resolveScheduleUpdateProposal(message.id)}
    />
  );
}
```

Destructure the new actions from the store hook at the top of the component alongside `confirmScheduleProposal` / `resolveScheduleProposal`.

- [ ] **Step 7: Run the tests to verify they pass**

Run:

```bash
pnpm --filter web test -- core/lib/ai-conversations.test.ts core/store/ai-assistant.store.test.ts
pnpm --filter web check:types
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-conversations.ts apps/web/core/lib/ai-conversations.test.ts apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts apps/web/core/components/ai/assistant-sidebar/schedule-update-proposal-card.tsx apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): add Galileo schedule update proposal card"
```

---

### Task 9: Documentation

**Files:**

- Modify: `apps/web/core/components/docs/content/scheduler.tsx`
- Modify: `apps/web/core/components/docs/content/galileo.tsx`

- [ ] **Step 1: Update the Scheduler docs**

Replace the “Yang bisa kamu lakukan” paragraph in `scheduler.tsx` with:

```tsx
<DocsParagraph>
  Membuat jadwal dengan prompt dan frekuensi (harian, mingguan, dan seterusnya), mengedit nama, resep, dan cadence
  jadwal yang sudah tersimpan, mengaktifkan atau menonaktifkannya, serta melihat riwayat eksekusi beserta hasilnya.
</DocsParagraph>
```

Add a callout after it:

```tsx
<DocsCallout variant="info" title="Edit dari dua tempat">
  Buka detail jadwal lalu pilih <strong>Edit</strong>, atau minta Galileo mengubahnya. Perubahan dari Galileo selalu
  tampil sebagai kartu before/after yang harus kamu konfirmasi.
</DocsCallout>
```

- [ ] **Step 2: Update the Galileo docs**

In `galileo.tsx`, replace the “Yang bisa kamu lakukan” paragraph with:

```tsx
<DocsParagraph>
  Mengobrol tentang item yang sedang terbuka (minta ringkasan, draf balasan, atau analisis singkat), membuat dan
  mengubah jadwal berulang, memulai percakapan baru, dan membuka kembali riwayat percakapan sebelumnya.
</DocsParagraph>
```

Add after the existing callout:

```tsx
<DocsParagraph>
  Contoh: “Ubah laporan harian saya menjadi setiap Selasa pukul 10.00”. Galileo akan mencari jadwal yang cocok,
  menampilkan perbandingan sebelum/sesudah, dan baru menerapkannya setelah kamu menekan Apply.
</DocsParagraph>
```

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/docs/content/scheduler.tsx apps/web/core/components/docs/content/galileo.tsx
git commit -m "docs: explain schedule editing"
```

---

### Task 10: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Rust formatting and lint**

Run from `apps/api-rs`:

```bash
cargo fmt --all -- --check
cargo clippy -p ai -p api -p worker --all-targets -- -D warnings
```

Expected: PASS. If clippy flags pre-existing unrelated warnings, fix only the warnings introduced by this change.

- [ ] **Step 2: Rust unit and DB tests**

Run from `apps/api-rs`:

```bash
cargo test -p ai
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_schedule_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p worker --test ai_schedule_test -- --test-threads=1
```

Expected: PASS. If the local stack is not running, start it per `AGENTS.md` before the DB tests.

- [ ] **Step 3: Frontend checks**

Run from the repo root:

```bash
pnpm --filter web test -- core/lib/ai-schedule.test.ts core/lib/ai-conversations.test.ts core/store/ai-schedules.store.test.ts core/store/ai-assistant.store.test.ts
pnpm --filter web check:types
```

Expected: PASS.

- [ ] **Step 4: Manual end-to-end smoke**

1. Start dev (`pnpm dev`) or use the prod service per `AGENTS.md`.
2. Create a structured schedule via `/schedule`.
3. Edit it from Scheduler: change name, description, steps, tools, output, cadence, timezone; save.
4. Verify the list/detail show the new values and `next_run_at` is correct.
5. Ask Galileo: “change my <schedule name> to weekly on Tuesday at 10:00”.
6. Verify the before/after card, Apply, and that the Scheduler list updates after the 30-second poll (or a manual refresh).
7. Try editing as a non-creator member; verify the card is disabled and the modal button is absent.
8. Pause, edit cadence, verify the next run stays unchanged until Resume.
9. Create a long structured recipe (or use an existing one) and wait for a tick; verify it is not disabled.

- [ ] **Step 5: Commit any verification fixes**

If verification required changes, commit them with a message describing the fix. Do not amend earlier commits.

---

## Self-Review Notes

- **Spec coverage:** API partial update, prompt re-render, next-run rules, legacy handling, worker fix, list/get/update tools, before/after metadata, permission gate, edit modal, shared recipe fields, update card, decision persistence, docs, and tests each map to a task above.
- **Type consistency:** `ScheduleChanges` is defined in Task 1 and consumed by Tasks 2/4; `UPDATE_SCHEDULE_NAME` is defined in Task 4 and consumed by Task 5; FE `TAiScheduleUpdateChanges`/`TAiScheduleUpdate`/`TAiScheduleUpdateProposal` are defined in Task 6 and consumed by Tasks 7/8; metadata keys are identical across Rust and TS.
- **No placeholders:** every code-changing step includes concrete code or an exact replacement target.
