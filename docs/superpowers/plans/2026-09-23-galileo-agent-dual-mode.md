# Galileo Dual-Mode (Classic ↔ Agent) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the Galileo sidebar switch between the existing classic `/ai-assistant/` chat and the new Rig agent `/ai-agent/` via a header toggle, with the classic path as the safe default and manual fallback.

**Architecture:** Backend changes are additive-only: `/ai-agent/` accepts an optional `task` (folded into the prompt exactly like Django's `task + "\n" + prompt`) and returns `response_html` derived through a shared helper extracted from `/ai-assistant/`. The FE gets a `mode` state persisted per workspace (`classic` default); the store routes each send to the matching service method and renders `response_html ?? response`; switching mode clears the conversation. UI is a segmented control next to the "Galileo" title.

**Tech Stack:** Rust (axum 0.7, rig 0.42), TypeScript/React Router (MobX store, propel `Tooltip`), Vitest (node environment, `core/**/*.test.ts`).

**Spec:** `docs/superpowers/specs/2026-09-23-galileo-agent-dual-mode-design.md`

---

## File map

| Action | File                                                     | Responsibility                                                               |
| ------ | -------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Modify | `apps/api-rs/crates/api/src/routes/ai.rs`                | Extract shared `response_html` helper; use it in `/ai-assistant/`; unit test |
| Modify | `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`      | `effective_prompt`, `run_agent` task param, `success_body`, unit tests       |
| Modify | `apps/api-rs/crates/api/tests/ai_agent_test.rs`          | Update call sites; task-fold integration test against the fake upstream      |
| Modify | `apps/web/core/services/ai.service.ts`                   | `createAgentTask` + response type                                            |
| Modify | `apps/web/core/store/ai-assistant.store.ts`              | `mode` state/persistence, `setMode`, per-mode request branch                 |
| Modify | `apps/web/core/store/ai-assistant.store.test.ts`         | Mode + agent-branch store tests                                              |
| Modify | `apps/web/package.json`                                  | Add `sanitize-html` + types via catalog                                      |
| Modify | `apps/web/core/lib/ai-context.ts`                        | `sanitizeAssistantHtml` allowlist helper                                     |
| Modify | `apps/web/core/lib/ai-context.test.ts`                   | Sanitizer unit tests                                                         |
| Modify | `apps/web/core/components/ai/assistant-sidebar/root.tsx` | Sanitize render sink + header segmented control                              |

No `parity-inventory.json` change, no migrations, no changes to `/ai-assistant/` behavior (only an internal refactor), no changes to editor/popover GPT flows.

---

### Task 1: Shared `response_html` helper in `routes/ai.rs`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai.rs`
- Test: same file (`mod tests` at the bottom)

- [ ] **Step 1: Write the failing unit test**

In `apps/api-rs/crates/api/src/routes/ai.rs`, inside `mod tests`, add this test directly after `extract_content_handles_missing_and_empty`:

```rust
    #[test]
    fn response_html_maps_newlines_only() {
        assert_eq!(response_html("a\nb"), "a<br/>b");
        assert_eq!(response_html("plain"), "plain");
        assert_eq!(response_html(""), "");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib response_html_maps_newlines_only 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'response_html' in this scope`.

- [ ] **Step 3: Implement the helper and use it in the handler**

In the same file, add the helper directly after `extract_content`:

```rust
/// Django parity: `/ai-assistant/` returns the raw text plus a copy with
/// newlines mapped to `<br/>`. Shared with the agent route so both chat modes
/// render identically.
pub(crate) fn response_html(text: &str) -> String {
    text.replace('\n', "<br/>")
}
```

Then replace the `Ok(text)` arm of `workspace_ai_assistant`:

Old:

```rust
        Ok(text) => {
            let response_html = text.replace('\n', "<br/>");
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "response_html": response_html})),
            ))
        }
```

New:

```rust
        Ok(text) => {
            let html = response_html(&text);
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "response_html": html})),
            ))
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --lib response_html_maps_newlines_only 2>&1 | tail -20`
Expected: PASS (1 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai.rs
git commit -m "refactor(api-rs): extract shared response_html helper"
```

---

### Task 2: Fold optional `task` into the agent prompt

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Test: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Write the failing unit test**

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`, inside `mod tests`, add after `prompt_from_body_rules`:

```rust
    #[test]
    fn effective_prompt_folds_task_like_django() {
        assert_eq!(effective_prompt(Some("do it"), "text"), "do it\ntext");
        assert_eq!(effective_prompt(None, "text"), "text");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib effective_prompt_folds_task_like_django 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'effective_prompt' in this scope`.

- [ ] **Step 3: Implement `effective_prompt`, change `run_agent`, wire the handler**

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`:

Add the helper directly after `prompt_from_body`:

```rust
/// Effective user message: Django parity concatenates `task + "\n" + prompt`
/// (`routes/ai.rs::build_body`); an absent task leaves the prompt as-is.
pub fn effective_prompt(task: Option<&str>, prompt: &str) -> String {
    match task {
        Some(task) => format!("{task}\n{prompt}"),
        None => prompt.to_string(),
    }
}
```

Change the import line:

Old:

```rust
use crate::routes::ai::{host_of, resolve_llm_config, LlmError};
```

New:

```rust
use crate::routes::ai::{host_of, resolve_llm_config, task_from_body, LlmError};
```

Change `run_agent`'s signature (add the `task` parameter before `prompt`):

Old:

```rust
pub async fn run_agent(
    base_url: &str,
    api_key: &str,
    model: &str,
    tool_server: ToolServerHandle,
    prompt: &str,
) -> Result<String, LlmError> {
```

New:

```rust
pub async fn run_agent(
    base_url: &str,
    api_key: &str,
    model: &str,
    tool_server: ToolServerHandle,
    task: Option<&str>,
    prompt: &str,
) -> Result<String, LlmError> {
```

Change the final call inside `run_agent`:

Old:

```rust
    agent.prompt(prompt).await.map_err(map_prompt_error)
```

New:

```rust
    agent
        .prompt(effective_prompt(task, prompt))
        .await
        .map_err(map_prompt_error)
```

In the handler `workspace_ai_agent`, after the `let Some(prompt) = prompt_from_body(&body) else { ... };` block, add:

```rust
    let task = task_from_body(&body);
```

Then change the `run_agent` call:

Old:

```rust
        run_agent(&cfg.base_url, &cfg.api_key, &cfg.model, tool_server, prompt),
```

New:

```rust
        run_agent(
            &cfg.base_url,
            &cfg.api_key,
            &cfg.model,
            tool_server,
            task,
            prompt,
        ),
```

- [ ] **Step 4: Update existing integration call sites and add the fake-upstream test**

In `apps/api-rs/crates/api/tests/ai_agent_test.rs`:

Change the root imports:

Old:

```rust
use api::routes::ai_agent::run_agent;
use axum::{http::StatusCode, routing::post, Json, Router};
use rig::tool::server::ToolServer;
use serde_json::{json, Value};
```

New:

```rust
use std::sync::{Arc, Mutex};

use api::routes::ai_agent::run_agent;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use rig::tool::server::ToolServer;
use serde_json::{json, Value};
```

Add a capturing upstream + the fold test after `spawn_fixed`:

```rust
#[derive(Default)]
struct Capturing {
    bodies: Mutex<Vec<Value>>,
}

async fn spawn_capturing() -> (String, Arc<Capturing>) {
    async fn handler(
        State(state): State<Arc<Capturing>>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.bodies.lock().unwrap().push(body);
        (
            StatusCode::OK,
            Json(json!({
                "id": "chatcmpl-2",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "final answer"},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let state: Arc<Capturing> = Arc::new(Capturing::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .with_state(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/v1"), state)
}

#[tokio::test]
async fn task_folds_into_upstream_user_message() {
    let (base, upstream) = spawn_capturing().await;
    let out = run_agent(
        &base,
        "key",
        "model",
        ToolServer::new().run(),
        Some("be terse"),
        "hi",
    )
    .await;
    assert_eq!(out, Ok("final answer".to_string()));
    let bodies = upstream.bodies.lock().unwrap();
    let messages = bodies[0]["messages"].as_array().unwrap();
    let user = messages
        .iter()
        .find(|m| m["role"] == json!("user"))
        .expect("upstream call must carry a user message");
    assert_eq!(user["content"], json!("be terse\nhi"));
}
```

Update the five existing `run_agent` call sites to pass `None` before the prompt:

```rust
// in prompt_without_tools_returns_content
let out = run_agent(&base, "key", "gpt-4o-mini", ToolServer::new().run(), None, "hi").await;
```

```rust
// in tool_call_roundtrip_records_trace_and_returns_final_text
let out = run_agent(
    &base,
    "key",
    "model",
    ToolServer::new().tool(tool).run(),
    None,
    "say hi",
)
.await;
```

```rust
// in upstream_429_maps_to_rate_limited
let out = run_agent(&base, "key", "model", ToolServer::new().run(), None, "hi").await;
```

```rust
// in upstream_500_maps_to_upstream
let out = run_agent(&base, "key", "model", ToolServer::new().run(), None, "hi").await;
```

```rust
// in malformed_json_maps_to_upstream
let out = run_agent(&base, "key", "model", ToolServer::new().run(), None, "hi").await;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p api --lib effective_prompt_folds_task_like_django 2>&1 | tail -20`
Expected: PASS (1 passed).

Run: `cargo test -p api --test ai_agent_test 2>&1 | tail -20`
Expected: PASS (6 passed: 1 no-tool, 1 roundtrip, 1 fold, 429, 500, malformed).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): fold optional task into ai-agent prompt"
```

---

### Task 3: `response_html` in the agent 200 body

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`

- [ ] **Step 1: Write the failing unit test**

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`, inside `mod tests`, add after `effective_prompt_folds_task_like_django`:

```rust
    #[test]
    fn success_body_maps_newlines_and_keeps_tool_calls() {
        let body = success_body("line1\nline2", vec![json!({"name": "list_projects"})]);
        assert_eq!(body["response"], json!("line1\nline2"));
        assert_eq!(body["response_html"], json!("line1<br/>line2"));
        assert_eq!(body["tool_calls"][0]["name"], json!("list_projects"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib success_body_maps_newlines_and_keeps_tool_calls 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'success_body' in this scope`.

- [ ] **Step 3: Implement `success_body` and use it in the handler**

In the same file, add after `run_agent`:

```rust
/// 200 response body: raw text, newline-mapped HTML for the chat bubble, and
/// the recorded tool calls.
pub fn success_body(text: &str, tool_calls: Vec<Value>) -> Value {
    json!({
        "response": text,
        "response_html": crate::routes::ai::response_html(text),
        "tool_calls": tool_calls,
    })
}
```

Replace the `Ok(text)` arm of `workspace_ai_agent`:

Old:

```rust
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
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "tool_calls": tool_calls})),
            ))
        }
```

New:

```rust
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
            Ok((StatusCode::OK, Json(success_body(&text, tool_calls))))
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --lib success_body_maps_newlines_and_keeps_tool_calls 2>&1 | tail -20`
Expected: PASS (1 passed).

Run: `cargo test -p api --lib routes::ai_agent 2>&1 | tail -20`
Expected: PASS (4 unit tests: `prompt_from_body_rules`, `record_appends_and_serializes`, `effective_prompt_folds_task_like_django`, `success_body_maps_newlines_and_keeps_tool_calls`).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs
git commit -m "feat(api-rs): return response_html from ai-agent"
```

---

### Task 4: FE service `createAgentTask`

**Files:**

- Modify: `apps/web/core/services/ai.service.ts`

- [ ] **Step 1: Add the method and response type**

In `apps/web/core/services/ai.service.ts`, add the type above the class:

```ts
export type TAgentTaskResponse = {
  response: string;
  response_html?: string;
  tool_calls?: { name: string; arguments: unknown }[];
};
```

Add the method directly after `createGptTask`:

```ts
  async createAgentTask(
    workspaceSlug: string,
    data: { prompt: string; task: string }
  ): Promise<TAgentTaskResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-agent/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
```

- [ ] **Step 2: Verify types**

Run: `pnpm --filter=web check:types 2>&1 | tail -20`
Expected: no new errors from `ai.service.ts` (the store still compiles; it does not use the method yet).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/ai.service.ts
git commit -m "feat(web): add ai-agent service method"
```

---

### Task 5: Store mode state + per-mode request branch

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Write the failing tests**

In `apps/web/core/store/ai-assistant.store.test.ts`, replace the service helper:

Old:

```ts
type TCreateGptTask = (workspaceSlug: string, data: { prompt: string; task: string }) => Promise<any>;

const makeService = (impl: TCreateGptTask = async () => ({ response: "ok", response_html: "ok" })) => ({
  createGptTask: vi.fn(impl) as unknown as TCreateGptTask,
});
```

New:

```ts
type TCreateGptTask = (workspaceSlug: string, data: { prompt: string; task: string }) => Promise<any>;
type TCreateAgentTask = (workspaceSlug: string, data: { prompt: string; task: string }) => Promise<any>;

const makeService = (
  gptImpl: TCreateGptTask = async () => ({ response: "ok", response_html: "ok" }),
  agentImpl: TCreateAgentTask = async () => ({ response: "agent ok", response_html: "agent ok html", tool_calls: [] })
) => ({
  createGptTask: vi.fn(gptImpl) as unknown as TCreateGptTask,
  createAgentTask: vi.fn(agentImpl) as unknown as TCreateAgentTask,
});
```

Add these tests at the end of the `describe("AIAssistantStore", ...)` block, after `serializes sendMessage while generating`:

```ts
it("defaults to classic mode and persists the mode per workspace", () => {
  const store = new AIAssistantStore(makeService());
  store.setWorkspace("acme");
  expect(store.mode).toBe("classic");

  store.setMode("agent");
  expect(store.mode).toBe("agent");

  const rehydrated = new AIAssistantStore(makeService());
  rehydrated.setWorkspace("acme");
  expect(rehydrated.mode).toBe("agent");

  const other = new AIAssistantStore(makeService());
  other.setWorkspace("other-ws");
  expect(other.mode).toBe("classic");
});

it("setMode clears the conversation and is a no-op for the active mode", async () => {
  const store = new AIAssistantStore(makeService());
  store.setWorkspace("acme");
  await store.sendMessage("hi");
  expect(store.messages).toHaveLength(2);

  store.setMode("agent");
  expect(store.mode).toBe("agent");
  expect(store.messages).toHaveLength(0);

  await store.sendMessage("again");
  expect(store.messages).toHaveLength(2);
  store.setMode("agent");
  expect(store.messages).toHaveLength(2);
});

it("agent mode calls createAgentTask and prefers response_html", async () => {
  const service = makeService(undefined, async () => ({
    response: "plain answer",
    response_html: "agent <b>answer</b>",
    tool_calls: [{ name: "count_work_items", arguments: { priority: "urgent" } }],
  }));
  const store = new AIAssistantStore(service);
  store.setWorkspace("acme");
  store.setActiveIssueContext(CONTEXT);
  store.setMode("agent");

  await store.sendMessage("how many urgent?");

  expect(store.messages).toHaveLength(2);
  expect(store.messages[1].content).toBe("agent <b>answer</b>");
  expect(store.messages[1].isError).toBe(false);

  const call = (service.createAgentTask as any).mock.calls[0];
  expect(call[0]).toBe("acme");
  expect(call[1].task).toContain("ITSM work-item assistant");
  expect(call[1].prompt).toContain("Work item context:");
  expect(call[1].prompt).toContain("User's new question: how many urgent?");
  expect((service.createGptTask as any).mock.calls).toHaveLength(0);
});

it("agent mode falls back to response when response_html is missing", async () => {
  const service = makeService(undefined, async () => ({ response: "plain text" }));
  const store = new AIAssistantStore(service);
  store.setWorkspace("acme");
  store.setMode("agent");

  await store.sendMessage("hi");

  expect(store.messages[1].content).toBe("plain text");
  expect(store.messages[1].isError).toBe(false);
});

it("agent mode maps errors to error bubbles and retries in the same mode", async () => {
  const service = makeService(undefined, async () => {
    throw Object.assign(new Error("fail"), {
      status: 429,
      data: { error: "Rate limit exceeded for openrouter.ai" },
    });
  });
  const store = new AIAssistantStore(service);
  store.setWorkspace("acme");
  store.setMode("agent");

  await store.sendMessage("hi");

  expect(store.messages).toHaveLength(2);
  expect(store.messages[1].isError).toBe(true);
  expect(store.messages[1].content).toBe("Rate limit exceeded for openrouter.ai");

  const passing = makeService(undefined, async () => ({ response: "fixed", response_html: "fixed html" }));
  (store as unknown as { aiService: unknown }).aiService = passing;
  await store.retryLast();

  expect(store.messages).toHaveLength(2);
  expect(store.messages[1].content).toBe("fixed html");
  expect((passing.createAgentTask as any).mock.calls).toHaveLength(1);
  expect((passing.createGptTask as any).mock.calls).toHaveLength(0);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm --filter=web test 2>&1 | tail -30`
Expected: the five new tests FAIL (`setMode is not a function` / `mode` undefined / `createAgentTask is not a function`); the pre-existing tests still pass.

- [ ] **Step 3: Implement the store changes**

In `apps/web/core/store/ai-assistant.store.ts`:

Change the service type:

Old:

```ts
type TAiService = Pick<AIService, "createGptTask">;
```

New:

```ts
export type TAiAssistantMode = "classic" | "agent";

type TAiService = Pick<AIService, "createGptTask" | "createAgentTask">;
```

Extend the interface:

Old:

```ts
export interface IAIAssistantStore {
  messages: TAiMessage[];
  isGenerating: boolean;
  activeIssueContext: TAiIssueContext | undefined;
  hasActiveIssue: boolean;
  setWorkspace: (workspaceSlug: string | undefined) => void;
  setActiveIssueContext: (context: TAiIssueContext | undefined) => void;
  sendMessage: (question: string) => Promise<void>;
  retryLast: () => Promise<void>;
  clearConversation: () => void;
}
```

New:

```ts
export interface IAIAssistantStore {
  messages: TAiMessage[];
  isGenerating: boolean;
  mode: TAiAssistantMode;
  activeIssueContext: TAiIssueContext | undefined;
  hasActiveIssue: boolean;
  setWorkspace: (workspaceSlug: string | undefined) => void;
  setMode: (mode: TAiAssistantMode) => void;
  setActiveIssueContext: (context: TAiIssueContext | undefined) => void;
  sendMessage: (question: string) => Promise<void>;
  retryLast: () => Promise<void>;
  clearConversation: () => void;
}
```

Add the mode storage helpers next to `storageKey`:

```ts
export const AI_ASSISTANT_MODE_PREFIX = "ai_assistant_mode_";
const modeStorageKey = (workspaceSlug: string | undefined) =>
  `${AI_ASSISTANT_MODE_PREFIX}${workspaceSlug ?? "unknown"}`;
```

Add the class field after `isGenerating`:

```ts
mode: TAiAssistantMode = "classic";
```

Register it in `makeObservable` (after `isGenerating`):

```ts
      mode: observable.ref,
```

and in the action list (after `setWorkspace`):

```ts
      setMode: action,
```

Update `setWorkspace` to restore the mode:

Old:

```ts
    this.workspaceSlug = workspaceSlug;
    this.messages = this.restore();
  };
```

New:

```ts
    this.workspaceSlug = workspaceSlug;
    this.messages = this.restore();
    this.mode = this.restoreMode();
  };
```

Add `setMode` directly after `setWorkspace`:

```ts
setMode = (mode: TAiAssistantMode) => {
  if (mode === this.mode) return;
  this.isGenerating = false;
  this.mode = mode;
  this.persistMode();
  this.clearConversation();
};
```

Add the private restore/persist methods directly after `persist`:

```ts
  private restoreMode(): TAiAssistantMode {
    if (!this.workspaceSlug) return "classic";
    try {
      return localStorage.getItem(modeStorageKey(this.workspaceSlug)) === "agent" ? "agent" : "classic";
    } catch {
      return "classic";
    }
  }

  private persistMode() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.setItem(modeStorageKey(this.workspaceSlug), this.mode);
    } catch {
      // storage unavailable — persistence is best-effort
    }
  }
```

Replace `request` to branch on the mode (capture the mode at send time and drop stale responses, mirroring the workspace guard):

Old:

```ts
  private async request(question: string, slug: string) {
    this.isGenerating = true;
    try {
      const res = await this.aiService.createGptTask(slug, {
        task: AI_ASSISTANT_TASK,
        prompt: buildAiPrompt(this.activeIssueContext, this.messages.slice(0, -1), question),
      });
      if (this.workspaceSlug !== slug) return;
      const assistantMessage: TAiMessage = {
        id: uuidv4(),
        role: "assistant",
        content: res.response_html ?? "",
        isError: false,
      };
      runInAction(() => {
        this.messages.push(assistantMessage);
        this.persist();
      });
    } catch (err: any) {
      if (this.workspaceSlug !== slug) return;
      const errorContent =
        err?.status === 429
          ? err?.data?.error || "Rate limit exceeded."
          : err?.status === 400
            ? "AI is not configured for this instance."
            : "An internal error has occurred. Please try again.";
      runInAction(() => {
        this.messages.push({ id: uuidv4(), role: "assistant", content: errorContent, isError: true });
        this.persist();
      });
    } finally {
      if (this.workspaceSlug === slug) {
        runInAction(() => {
          this.isGenerating = false;
        });
      }
    }
  }
```

New:

```ts
  private async request(question: string, slug: string) {
    const mode = this.mode;
    this.isGenerating = true;
    try {
      const payload = {
        task: AI_ASSISTANT_TASK,
        prompt: buildAiPrompt(this.activeIssueContext, this.messages.slice(0, -1), question),
      };
      const res =
        mode === "agent"
          ? await this.aiService.createAgentTask(slug, payload)
          : await this.aiService.createGptTask(slug, payload);
      if (this.workspaceSlug !== slug || this.mode !== mode) return;
      const assistantMessage: TAiMessage = {
        id: uuidv4(),
        role: "assistant",
        content: mode === "agent" ? (res.response_html ?? res.response ?? "") : (res.response_html ?? ""),
        isError: false,
      };
      runInAction(() => {
        this.messages.push(assistantMessage);
        this.persist();
      });
    } catch (err: any) {
      if (this.workspaceSlug !== slug || this.mode !== mode) return;
      const errorContent =
        err?.status === 429
          ? err?.data?.error || "Rate limit exceeded."
          : err?.status === 400
            ? "AI is not configured for this instance."
            : "An internal error has occurred. Please try again.";
      runInAction(() => {
        this.messages.push({ id: uuidv4(), role: "assistant", content: errorContent, isError: true });
        this.persist();
      });
    } finally {
      if (this.workspaceSlug === slug && this.mode === mode) {
        runInAction(() => {
          this.isGenerating = false;
        });
      }
    }
  }
```

Note: `clearPersistedAiConversations` is intentionally left untouched — mode is a UI preference, not conversation data, and the spec does not clear it on sign-out (YAGNI).

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=web test 2>&1 | tail -30`
Expected: all tests pass, including the 12 pre-existing store tests and the 5 new ones.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): persist assistant mode and route sends per mode"
```

---

### Task 6: Sanitize assistant HTML at the render sink

**Files:**

- Modify: `apps/web/package.json`
- Modify: `apps/web/core/lib/ai-context.ts`
- Test: `apps/web/core/lib/ai-context.test.ts`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

Security amendment (approved 2026-09-23): model output can echo DB-sourced project/work-item names and is injected via `dangerouslySetInnerHTML`. Both chat modes now render allowlist-sanitized HTML; the backend `response_html` helper stays parity-exact.

- [ ] **Step 1: Add the sanitizer dependency**

In `apps/web/package.json`, add to `dependencies` (keep the existing ordering style):

```json
    "sanitize-html": "catalog:",
```

and to `devDependencies`:

```json
    "@types/sanitize-html": "catalog:",
```

Then run `pnpm install` from the repo root. Expected: install completes and `sanitize-html` resolves for `apps/web` (it is already in the workspace lockfile via `@plane/utils`).

- [ ] **Step 2: Write the failing tests**

In `apps/web/core/lib/ai-context.test.ts`, extend the import from `./ai-context` to include `sanitizeAssistantHtml`, then add:

```ts
describe("sanitizeAssistantHtml", () => {
  it("keeps formatting tags", () => {
    expect(sanitizeAssistantHtml("<b>bold</b><br/>line")).toBe("<b>bold</b><br />line");
    expect(sanitizeAssistantHtml("<ul><li>one</li></ul>")).toBe("<ul><li>one</li></ul>");
  });

  it("strips scripts, event handlers, and unsafe URLs", () => {
    expect(sanitizeAssistantHtml("<script>alert(1)</script>")).toBe("");
    expect(sanitizeAssistantHtml('<img src=x onerror="alert(1)">')).toBe("");
    expect(sanitizeAssistantHtml('<a href="javascript:alert(1)">x</a>')).toBe("<a>x</a>");
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `pnpm --filter=web test 2>&1 | tail -30`
Expected: FAIL — `sanitizeAssistantHtml` is not exported / not a function.

- [ ] **Step 4: Implement the helper**

In `apps/web/core/lib/ai-context.ts`, add the import at the top:

```ts
import sanitizeHtml from "sanitize-html";
```

and after `stripHtml`:

```ts
const ASSISTANT_ALLOWED_TAGS = [
  "b",
  "strong",
  "i",
  "em",
  "u",
  "s",
  "br",
  "p",
  "ul",
  "ol",
  "li",
  "code",
  "pre",
  "blockquote",
  "h1",
  "h2",
  "h3",
  "h4",
  "hr",
  "a",
];

/** Allowlist-sanitize model output before it is injected as HTML. */
export const sanitizeAssistantHtml = (html: string): string =>
  sanitizeHtml(html, {
    allowedTags: ASSISTANT_ALLOWED_TAGS,
    allowedAttributes: { a: ["href", "target", "rel"] },
  });
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `pnpm --filter=web test 2>&1 | tail -30`
Expected: PASS — all existing tests plus the 2 new ones. If `sanitize-html` normalizes self-closing tags differently than the expected strings, adjust only the cosmetic assertions (`<br />` spelling) and keep the security assertions (`""` for script/img, no `javascript:` in the anchor) exactly as written; report any adjustment.

- [ ] **Step 6: Apply at the render sink**

In `apps/web/core/components/ai/assistant-sidebar/root.tsx`, change the type-only import:

Old:

```tsx
import type { TAiIssueContext } from "@/lib/ai-context";
```

New:

```tsx
import { sanitizeAssistantHtml, type TAiIssueContext } from "@/lib/ai-context";
```

Then change the assistant message render:

Old:

```tsx
<div dangerouslySetInnerHTML={{ __html: message.content }} />
```

New:

```tsx
<div dangerouslySetInnerHTML={{ __html: sanitizeAssistantHtml(message.content) }} />
```

- [ ] **Step 7: Verify types and commit**

Run: `pnpm --filter=web check:types 2>&1 | tail -20`
Expected: no errors.

```bash
git add apps/web/package.json apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-context.test.ts apps/web/core/components/ai/assistant-sidebar/root.tsx pnpm-lock.yaml
git commit -m "fix(web): sanitize assistant HTML before rendering"
```

---

### Task 7: Header segmented control

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Add the mode options and hook fields**

In `apps/web/core/components/ai/assistant-sidebar/root.tsx`, add after `SUGGESTIONS`:

```tsx
const MODES = [
  { value: "classic", label: "Classic", hint: "Single model call grounded in the work item on screen" },
  { value: "agent", label: "Agent", hint: "Looks up projects and work items in this workspace" },
] as const;
```

In the `useAiAssistant()` destructure, add `mode` and `setMode`:

Old:

```tsx
const {
  messages,
  isGenerating,
  activeIssueContext,
  hasActiveIssue,
  setWorkspace,
  setActiveIssueContext,
  sendMessage,
  retryLast,
  clearConversation,
} = useAiAssistant();
```

New:

```tsx
const {
  messages,
  isGenerating,
  mode,
  activeIssueContext,
  hasActiveIssue,
  setWorkspace,
  setMode,
  setActiveIssueContext,
  sendMessage,
  retryLast,
  clearConversation,
} = useAiAssistant();
```

- [ ] **Step 2: Render the segmented control in the header**

Replace the header's left group:

Old:

```tsx
<div className="flex items-center gap-2.5">
  <span className={cn("size-2 rounded-full", isGenerating ? "ai-status-orb bg-accent-primary" : "bg-accent-primary")} />
  <span className="text-sm font-semibold text-primary">Galileo</span>
  {isGenerating && <span className="font-mono text-[10px] uppercase tracking-widest text-tertiary">thinking</span>}
</div>
```

New:

```tsx
<div className="flex items-center gap-2.5">
  <span className={cn("size-2 rounded-full", isGenerating ? "ai-status-orb bg-accent-primary" : "bg-accent-primary")} />
  <span className="text-sm font-semibold text-primary">Galileo</span>
  <div
    role="group"
    aria-label="Assistant mode"
    className="flex items-center rounded-md border border-subtle bg-layer-1 p-0.5"
  >
    {MODES.map(({ value, label, hint }) => (
      <Tooltip key={value} label={hint} side="bottom">
        <button
          type="button"
          aria-pressed={mode === value}
          disabled={isGenerating}
          onClick={() => setMode(value)}
          className={cn(
            "rounded-[5px] px-2 py-0.5 text-[11px] font-medium transition-colors disabled:opacity-50",
            mode === value ? "bg-accent-primary text-on-color" : "text-secondary hover:text-primary"
          )}
        >
          {label}
        </button>
      </Tooltip>
    ))}
  </div>
  {isGenerating && <span className="font-mono text-[10px] uppercase tracking-widest text-tertiary">thinking</span>}
</div>
```

No other UI changes: empty state, suggestions, context strip, and message rendering stay as they are; `tool_calls` are deliberately not rendered.

- [ ] **Step 3: Verify types, format, lint**

Run: `pnpm --filter=web check:types 2>&1 | tail -20`
Expected: no errors.

Run: `pnpm --filter=web check:format 2>&1 | tail -10`
Expected: no diffs. If it reports diffs, run `pnpm --filter=web fix:format` and re-check.

Run: `pnpm --filter=web check:lint 2>&1 | tail -20`
Expected: passes (`--max-warnings=11957` is the pre-existing baseline).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): add classic/agent mode toggle to Galileo header"
```

---

### Task 8: Full verification and live smoke

**Files:** none (verification only; commit only if fixes are needed)

- [ ] **Step 1: Backend formatting and lint on touched files**

The repo has pre-existing rustfmt drift; do not run `cargo fmt --all`. Format only the agent files, and keep the `ai.rs` edit hand-formatted to match its surroundings:

```bash
rustfmt --edition 2021 \
  apps/api-rs/crates/api/src/routes/ai_agent/mod.rs \
  apps/api-rs/crates/api/tests/ai_agent_test.rs

cargo clippy -p api --all-targets 2>&1 | tail -20
```

Expected: no clippy findings in `ai_agent` / `ai.rs` files (pre-existing warnings elsewhere are out of scope).

- [ ] **Step 2: Full backend suite (serial)**

Run: `cargo test -p api -- --test-threads=1 2>&1 | tail -20`
Expected: all tests pass (1141 pre-existing + the new ones; the serial flag avoids the pre-existing `issue_create_test` purge flake).

- [ ] **Step 3: Full web checks**

```bash
pnpm --filter=web test 2>&1 | tail -20
pnpm --filter=web check:types 2>&1 | tail -20
pnpm --filter=web check:format 2>&1 | tail -10
pnpm --filter=web check:lint 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 4: Rebuild and restart the deployed services**

Per `AGENTS.md`, prod serves the static web build, so a rebuild is required for the toggle to appear on the tunnel. Only one server may hold port 3000 (prod is the default).

```bash
# from repo root
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
docker compose build api && docker compose up -d api
```

- [ ] **Step 5: Live smoke — API contract**

Preconditions: `LLM_API_KEY` + `LLM_MODEL` configured (admin AI form or env), `LLM_BASE_URL=https://openrouter.ai/api/v1` for this deployment, and a model that supports tool calling. Use a workspace slug where the token user is ADMIN or MEMBER.

```bash
SLUG=<workspace-slug>
TOKEN=<session token>

# task folds into the prompt; response_html present
curl -sS -X POST "http://localhost:8000/api/workspaces/$SLUG/ai-agent/" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"task":"Be terse.","prompt":"Berapa banyak work item urgent di workspace ini?"}' \
  | jq '{response, response_html, tool_calls: (.tool_calls | length)}'

# prompt-only still works (backward compatible)
curl -sS -X POST "http://localhost:8000/api/workspaces/$SLUG/ai-agent/" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"prompt":"Sebutkan project di workspace ini."}' | jq 'has("response_html")'

# /ai-assistant/ unchanged
curl -sS -X POST "http://localhost:8000/api/workspaces/$SLUG/ai-assistant/" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"task":"Be terse.","prompt":"hello"}' | jq 'has("response_html")'
```

Expected: all three return 200; the first shows a non-empty `tool_calls` for a data question and a non-empty `response_html`; the second and third report `true`.

- [ ] **Step 6: Live smoke — UI (browser on the tunnel)**

1. Open a work item page with the Galileo sidebar open. The header shows `Classic | Agent` with `Classic` selected.
2. Ask a work-item question in Classic (e.g. "Summarize this work item") — answer renders as before.
3. Click `Agent`: the conversation clears. Ask a data question (e.g. "Berapa work item urgent di workspace ini?") — the answer is data-backed; no tool trace is shown.
4. Click `Classic`: the conversation clears again and classic answers still work.
5. Error path: in Agent mode with an invalid `LLM_API_KEY`, send a message — an error bubble appears with a Retry button; Retry re-issues the agent request in Agent mode. Restore the key afterwards.
6. Reload the page: the last selected mode is restored for the workspace.

- [ ] **Step 7: Commit any fixes**

```bash
git add <only the files you fixed>
git commit -m "chore(web,api-rs): dual-mode smoke follow-ups"
```

---

## Rollback

- FE: revert the web commits; the sidebar returns to classic-only behavior (the mode key in localStorage is inert without the store code).
- API: revert the three api-rs commits; `/ai-assistant/` behavior is unchanged by design, and `/ai-agent/` returns to the pre-task/response_html contract.
- No DB state is created by any of these changes.

---

## Security amendment log

- 2026-09-23 (after Task 3 review): assistant HTML (both modes) is sanitized with a `sanitize-html` allowlist at the sidebar render sink. Reason: agent answers can echo DB-sourced names; the render sink used `dangerouslySetInnerHTML`. The backend helper remains parity-exact; sanitization lives in the FE (Task 6).
