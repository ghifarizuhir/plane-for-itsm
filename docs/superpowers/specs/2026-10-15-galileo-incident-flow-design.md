# Galileo Incident Flow (`/incident`) — Guided Triage & Enrich — Design

Date: 2026-10-15
Status: Approved in brainstorming (5/5 sections, 2026-10-15); awaiting written-spec review.
Scope: `apps/api-rs` (`crates/ai`, `crates/api`) + web (`lib`, store, assistant sidebar) + `packages/i18n`. No Django changes, no new tools, no schema migrations.
Related: `2026-10-04-galileo-tools-roadmap-design.md` (tool classes, proposal metadata, non-goals), `2026-09-23-galileo-agent-dual-mode-design.md` (classic ↔ agent switch), `2026-09-24-ai-scheduler-design.md` (slash-command precedent), `2026-09-24-galileo-chat-history-design.md` (conversation metadata precedent).

## Background

ITIL 4's key operational flows are Incident management, Service request management, Problem management, Change enablement, and Knowledge management. This repo's ITSM fork already seeds matching work item types (`Incident`, `Problem`, `Change`, `Improvement` — `crates/api/src/seed.rs:296`) and has the building blocks for an incident flow:

- 40 Galileo tools (23 read + 17 proposal-only) covering work items, services, intake, sprints, tracks, KB — `crates/ai/src/tools/mod.rs:347`
- Proposal cards + decision persistence (`ai_agent/mod.rs`, `ai_conversations.rs`)
- Agent mode with auto-switch on slash commands (`/schedule`, `/task`) — `ai-assistant.store.ts`
- War rooms (major-incident command) and intake triage already exist as separate features

There is no guided experience tying these together: a triager must know which questions to ask and which tools/fields to touch. This design adds a **flow-mode framework** (backend registry) and ships its first flow: guided Incident triage & enrich from chat.

## Locked decisions (brainstorming 2026-10-15)

1. **Approach 1 — backend flow registry** (Rust). Rejected: FE-composed prompt playbooks (spoofable, untestable, not production-grade) and DB/workspace-configurable playbooks (premature scope; possible future layer).
2. **Incident first, framework generic.** Only the Incident flow ships now; Problem/Change/Request/Knowledge are future registrations via the same registry.
3. **Trigger is chat-first**: `/incident <REF>` slash command in the Galileo sidebar, auto-switching classic→agent like `/schedule`. No launch buttons on work item pages (future).
4. **Existing incident only**: the flow requires a valid Incident reference; creating incidents from raw reports is out of scope.
5. **Guided but flexible**: the agent follows the playbook order but users can ask questions, jump ahead, or skip steps; completion = every outcome confirmed or explicitly skipped. No strict state machine, no progress rail.
6. **Chat-only UI**: the agent narrates progress inline; the only flow affordances are `/incident REF`, `/incident off`, and the slash suggestions.
7. **Outcomes** (each confirmable in the UI; no writes without confirmation):
   - Classification card: priority + state transition + labels (one `update_work_item` proposal)
   - Service link (`manage_service_links`) if missing
   - Assignee (`update_work_item`)
   - First response comment (`add_comment`)
8. **Production bar for customers**: server-side validation, coded errors, i18n for FE chrome, observability, Rust + web tests.
9. **Exit/replace**: `/incident off` ends the flow; a new `/incident REF` replaces it; invalid input is caught client-side (no API call) when no ref is given.
10. **No LLM control of flow state**: activation, validation, persistence and ending are server + FE concerns only.

## Non-goals

- The other four ITIL flows (Problem, Change enablement, Service request, Knowledge) — future specs on this framework.
- Strict state machine, progress rail, resume panel, step enforcement.
- Scheduled flow runs; automated/scheduled writes (scheduler stays read-only).
- War-room steps (war-room tools remain future work; major-incident handling is the war-room feature's territory).
- Workspace-configurable/editable playbooks.
- Creating incidents from raw reports, bulk triage, auto-assignment without confirmation.
- KB grounding in Classic mode; streaming/SSE.
- New backend tools or schema changes — the existing 40 tools cover every outcome.
- Migrating all assistant-sidebar chrome to i18n (only strings this feature adds/touches are migrated).

## Design

### 1. Flow registry (`crates/ai/src/flows/`)

New module:

```rust
// flows/mod.rs
pub struct Flow {
    pub id: &'static str,           // "incident"
    pub command: &'static str,      // "/incident"
    pub expected_type: &'static str,// "Incident" (case-insensitive type-name gate)
    pub playbook: &'static str,     // prompt fragment, English
}

pub fn resolve(id: &str) -> Option<&'static Flow>;
```

`flows/incident.rs` holds the playbook const. `crates/ai/src/lib.rs` registers the module. v1 has exactly one entry; `resolve` is the only lookup authority (no dynamic registration).

### 2. Request contract (`POST /ai-agent/`)

Additive optional body fields (lax parsing, same as the rest of the body):

- `flow: Option<String>` — flow id, e.g. `"incident"`
- `flow_ref: Option<String>` — work item reference, e.g. `"OPS-123"`

If `flow` is present, `flow_ref` must be present and parseable. Absent flow fields mean a normal agent turn.

**Validation ladder** (same order every turn; failure → HTTP 400):

1. `flow` resolves in the registry → else `{error, code: "flow_unknown"}`
2. `flow_ref` parses via `tools::parse_work_item_ref` → else `{error, code: "flow_invalid"}`
3. Work item exists, not deleted, in the conversation's workspace, and passes the same access checks `get_work_item` applies (project membership/visibility) → else `{error, code: "flow_ref_gone"}`
4. Item's type name equals `expected_type` (case-insensitive) → else `{error, code: "flow_type_mismatch"}` (message names the actual type)

`code` is a new additive field on these specific 400s so the FE can react precisely; other errors are unchanged. Validation happens before the LLM call and before the user message is inserted (no partial state on rejection).

### 3. Prompt composition (`crates/ai/src/agent.rs`)

- `run_agent` gains a `flow: Option<FlowContext>` parameter (`FlowContext { id: &'static str, reference: String }`).
- Preamble = `PREAMBLE` + `"\n\n"` + `flow.playbook` with the reference interpolated (the ref is validated `[A-Za-z0-9]+-\d+`, so no injection surface).
- Re-injected every turn while the flow is active (idempotent).
- `MAX_TURNS` (6) and `AGENT_TIMEOUT` (180s) unchanged — a flow spans several user messages, each within existing budgets.
- `tracing` fields `flow_id` / `flow_ref` on the request span; log activation, validation rejections (with code), and end reasons.

### 4. Persistence — assistant-message metadata

Conversation-level metadata is not exposed by `conversation_json`; message metadata already is (`message_json`) and already supports allowlisted PATCH. So flow state rides on **assistant message metadata**:

- When a turn runs with an active flow, the server adds `"flow": "incident"` and `"flow_ref": "OPS-123"` to the assistant message metadata at insert (same place `proposals` are written — `ai_agent/mod.rs:241`).
- `clean_metadata_patch` (`ai_conversations.rs:401`) gains two allowlisted keys:
  - `flow`: `"incident"` or `null`
  - `flow_ref`: a parseable `PROJ-123` string or `null`; must be null iff `flow` is null
- FE restore rule: scan the conversation's messages newest→oldest for the newest message that has a `flow` key at all; non-null → active, null → explicitly ended.
- `/incident off` patches `{flow: null, flow_ref: null}` onto the latest message (best-effort, like decision persistence). Flow validation errors with a `flow_*` code also clear it. Transient failures (timeout/upstream) do not clear the flow.

The LLM never writes flow metadata; the server writes it at insert, the FE only clears it.

### 5. Playbook content (`flows/incident.rs`)

Required sections (exact prose finalized in implementation):

1. **Mission**: you are running the Incident triage & enrich flow for `<REF>`. Never invent facts; every change is a proposal card.
2. **Step 0 — Orient** (read-only, before speaking): `get_work_item`, `list_work_item_comments`, `list_work_item_relations`, `get_service` (if linked), `search_work_items` (similar incidents: keywords, same service), `search_articles`/`get_article` (known errors/workarounds) → open with a short situational summary.
3. **Step 1 — Clarify (conditional)**: at most two focused questions (impact/scope, workaround) when the description is insufficient; state assumptions explicitly.
4. **Step 2 — Classification proposal**: priority (ITIL impact × urgency over `urgent|high|medium|low|none`), state transition (valid states via `list_states`; propose the triaged/working state), labels (only existing `list_labels`). One `update_work_item` proposal; reasoning in prose; card fields editable.
5. **Step 3 — Service link**: skip with a note when already linked; else suggest the best service and propose `manage_service_links`; ask when ambiguous.
6. **Step 4 — Assignee**: suggest from `list_members` + resolvers of similar incidents; ask when ambiguous; propose `update_work_item`.
7. **Step 5 — First response**: `add_comment` draft — impact summary, priority/state, known error/workaround with KB reference, next steps, owner, ETA (ask if unknown).
8. **Step 6 — Review**: recap outcomes as confirmed / skipped / pending (cards awaiting UI confirmation); offer another `/incident` or free chat.
9. **Guardrails**: proposal-only (never claim applied); no duplicate proposal for the same field (a changed answer = superseding proposal); honor skip requests; unrelated questions answered briefly then resume; completion = all outcomes confirmed or explicitly skipped; respond in the user's language, concise.

### 6. Web

**New lib — `apps/web/core/lib/ai-flows.ts`**

- `isIncidentCommand(text)`, `parseIncidentCommand(text) → { kind: "start", ref } | { kind: "off" } | { kind: "invalid" } | null` (mirrors `ai-schedule.ts` patterns; ref regex `^[A-Za-z0-9]+-\d+$`)

**Store — `ai-assistant.store.ts`**

- `activeFlow: TAiFlow | undefined` observable (`{id: "incident"; ref: string}`)
- On `/incident REF`: auto-switch classic→agent, set `activeFlow`, include `flow`/`flow_ref` in the `/ai-agent/` payload (only when active; new optional fields on the payload type in `ai.service.ts`)
- On `/incident off`: clear `activeFlow`, best-effort PATCH nulls on the newest message, show a local, non-persisted "flow ended" notice — no API turn
- On invalid/missing ref: set an inline composer hint, no API call
- On conversation open: restore `activeFlow` from message metadata (rule in §4); on `flow_*` coded errors: clear local state + metadata
- `request()` payload adds the two fields; everything else unchanged

**Sidebar — `root.tsx`**

- Allow `/incident` in the unknown-slash guard; add suggestion rows:
  - `/incident REF` — "Triage an existing incident"
  - `/incident off` — "End the incident flow"
- No progress chrome (chat-only decision)

**Conversations mapping — `ai-conversations.ts`**

- Parse `flow`/`flow_ref` from message metadata into typed fields

**i18n (new `assistant` namespace)**

- Register `"assistant"` in `packages/i18n/src/constants/namespaces.ts`; add `assistant.json` to all 20 locales via the repo `translate` skill workflow; regenerate types
- Keys: `slash_incident`, `slash_incident_off`, `incident_flow_ended`, `incident_ref_required`, `incident_invalid_ref`, plus migrated literals `slash_schedule`, `slash_task` (the two existing suggestions this feature touches)
- Server error texts stay English constants, matching existing API behavior

## File map

| File                                                     | Action                                                                  |
| -------------------------------------------------------- | ----------------------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/flows/mod.rs`                 | New: registry, `Flow`, `FlowCommand`, parse, preamble merge             |
| `apps/api-rs/crates/ai/src/flows/incident.rs`            | New: playbook const (`pub const PLAYBOOK: &str`)                        |
| `apps/api-rs/crates/ai/src/lib.rs`                       | Register `flows` module                                                 |
| `apps/api-rs/crates/ai/src/agent.rs`                     | `run_agent` flow param; preamble composition                            |
| `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`      | Body fields; validation ladder; coded errors; metadata write; `tracing` |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`  | `clean_metadata_patch`: `flow`/`flow_ref` (null-clear rules)            |
| `apps/web/core/lib/ai-flows.ts`                          | New: command parsing + types                                            |
| `apps/web/core/lib/ai-flows.test.ts`                     | New: parsing tests                                                      |
| `apps/web/core/services/ai.service.ts`                   | Payload type gains optional `flow`/`flow_ref`                           |
| `apps/web/core/store/ai-assistant.store.ts`              | `activeFlow`, command handling, payload, restore, clear                 |
| `apps/web/core/store/ai-assistant.store.test.ts`         | Flow cases                                                              |
| `apps/web/core/lib/ai-conversations.ts`                  | Metadata mapping for flow fields                                        |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx` | Slash guard + suggestions (i18n)                                        |
| `packages/i18n/src/constants/namespaces.ts`              | Add `assistant` namespace                                               |
| `packages/i18n/src/locales/*/assistant.json`             | New namespace, 20 locales                                               |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`          | Flow integration cases (fixtures live here)                             |

## Testing

**Rust unit** (`crates/ai/src/flows/`):

- `resolve` known/unknown; `parse_command` variants (`/incident OPS-123`, case-folding, `off`, missing ref, junk); playbook contains the ref, all six outcome steps, completion checklist, guardrails

**Rust integration** (extends `crates/api/tests/ai_agent_test.rs`; scratch suite runs with `-- --test-threads=1`):

- Valid activation → 200; assistant message metadata carries `flow`/`flow_ref`; prompt (via `effective_prompt`) contains the playbook block with the ref
- Rejections → 400 with codes: unknown flow, bad ref, nonexistent ref, non-Incident type, cross-workspace ref
- No partial state: rejection leaves no inserted messages
- Metadata allowlist: accepts `{flow:"incident", flow_ref:"OPS-123"}`, accepts null-clear pair, rejects bad id / bad ref / mismatched null pair
- Permission scoping matches `get_work_item` (guest without project access → `flow_ref_gone`)
- Regression: existing `ai_agent_test.rs` / `ai_conversations_test.rs` pass

**Web** (vitest):

- `ai-flows.test.ts` parsing table; store tests: auto mode-switch, payload fields only when active, `/incident off` clears + patches, restore from message metadata (newest-key rule incl. explicit null), `flow_*` error clears, invalid ref hint without API call
- Sidebar test: `/incident` accepted by guard, suggestions render

**Checks**: `pnpm check` (format/lint/types) + `pnpm --filter=@plane/i18n check:sync` for locale parity.

## Observability & rollout

- `tracing` span fields `flow_id`/`flow_ref`; events for activation, rejection (with code), end reasons (`off`, `replaced`, `stale`)
- Existing tool trace / `tool_calls` response untouched; no new metrics infrastructure (repo practice)
- No feature flag: the command surfaces only when the assistant is configured (`has_llm_configured`), same as other slash commands

## Future work

- Problem / Change enablement / Service request / Knowledge flows on the same registry
- Launch points from work item detail ("Run incident flow") and intake triage
- Strict optional progress affordances and resume UI for long flows
- Workspace-configurable playbooks (layer on top of the registry)
- War-room and SLA-aware steps once those data surfaces exist
