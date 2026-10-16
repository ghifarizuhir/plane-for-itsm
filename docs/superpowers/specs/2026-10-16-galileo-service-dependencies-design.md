# Galileo Service Dependency Proposals (Connect / Disconnect Services) — Design

Date: 2026-10-16
Status: Approved in brainstorming (approach A + 2 design sections, 2026-10-16); awaiting written-spec review.
Scope: one new proposal tool (backend `apps/api-rs/crates/ai` + metadata allowlist & integration test in `apps/api-rs/crates/api`) plus one new proposal card (`apps/web/core/lib`, `apps/web/core/store`, `apps/web/core/components/ai`). No migrations, no new REST endpoints, no worker/Django changes.
Related: `2026-10-04-galileo-tools-roadmap-design.md` (§4.4 generic proposal metadata, §6 mutation proposals — the `manage_service_links` precedent), `2026-10-15-galileo-incident-flow-design.md` (proposal card precedent), `2026-09-21-rust-ai-assistant-design.md`.

## Background

The service catalog already models service-to-service connections as `service_dependencies` edges (`from_service_id` depends on `to_service_id`), rendered in the service graph and service detail pages:

- Table + REST endpoints (project-scoped): list/create/delete at `.../service-dependencies/` — `apps/api-rs/crates/api/src/routes/service.rs:515-665`.
- Server-side validation on create: unknown services in the project ("Source service not found." / "Target service not found."), self edges ("A service cannot depend on itself."), duplicates ("This dependency already exists."), and cycles via recursive CTE ("This dependency would create a cycle.") — `service.rs:579-646`. Both services must belong to the **same project** (`service_exists` checks `project_id`) — `service.rs:545-557`.
- FE render + manual editing: `apps/web/core/components/services/detail/dependencies.tsx`, graph editor `apps/web/core/components/services/graph/*`, service store `addDependency` / `removeDependency` — `apps/web/core/store/service.store.ts:296`.

Galileo can already **read** services and their connections:

- `get_service` returns `dependencies` with `depends_on` / `needed_by` directions and open work items — `apps/api-rs/crates/ai/src/tools/services.rs:290-298,334-355`.
- `list_services` lists the catalog — same file, `:108-155`.

But there is **no mutation tool** for service-to-service edges. `manage_service_links` only links/unlinks work items to services (`apps/api-rs/crates/ai/src/tools/mutations.rs:240-320`). The generic proposal pipeline (roadmap spec §4.4) already supports new proposal kinds without migrations: validator → trace → `pending_actions` → assistant-message metadata `proposals` → FE card → Confirm calls an existing REST endpoint → decision persisted in `proposal_decisions`.

Motivation: the user maintains a new "prepaid system" project whose service descriptions document how services connect. They want to update the service graph by explicit chat commands (e.g. `connect prepaid-iso -> core-banking`) with the usual proposal-card confirmation instead of the UI graph editor.

## Locked decisions (brainstorming 2026-10-16)

1. **"Connection" = service dependency edge** (`service_dependencies`, `from` depends_on `to`). Not repository/documentation URLs. Confirmed by the user.
2. **Explicit commands only.** No inference from service descriptions; the descriptions remain a reference the user (and the read tools) can consult, not an input to generate proposals.
3. **One edge per proposal.** A card carries exactly one `(service, depends_on)` pair; no bulk in this iteration.
4. **Approach A** — new proposal tool `manage_service_dependencies` mirroring `manage_service_links`. Rejected: B (extend `manage_service_links` with a target mode — conflates work-item links with service edges) and C (UI-only manual edits — does not fulfill "use Galileo").
5. **Direction semantics**: "connect A -> B" means **A depends on B** (edge `from_service_id = A`, `to_service_id = B`). Cards render "A depends on B".
6. **Explicit `action: link | unlink`.** Unlink removes the existing `(from, to)` edge; the card resolves and passes the dependency id.
7. **Names are resolved in the FE card**, matching every existing card; the backend validates shape only and never touches the DB (proposal tools are trace-only).
8. **Same-project only** — enforced by the existing REST endpoint; no cross-project support.
9. **No result id in decisions** — applied decisions carry no `result` (same as `manage_service_links`).
10. **No migration, no new endpoint, no worker/Django changes.** The tool is proposal-only: registered in `workspace_tools()` only, never in `read_tools()`, so scheduled runs stay structurally read-only.

## Non-goals

- Bulk edges ("connect A to B, C, D" in one card) or workspace-wide import.
- Auto-inference of connections from service descriptions.
- Cross-project dependencies.
- Creating or deleting services in this flow (existing `create_service` / `update_service` cover the former; delete stays UI-only).
- Editing service descriptions or URLs (existing `update_service`).
- Suggesting alternative edges when a cycle is blocked.
- Scheduled-run mutations (proposal tools are not schedule-eligible by construction).
- Any change to existing read tools or to `manage_service_links`.

## Design

### 1. Tool — `apps/api-rs/crates/ai/src/tools/mutations.rs`

New proposal tool, modeled on `manage_service_links` (`mutations.rs:240-320`):

```rust
pub const MANAGE_SERVICE_DEPENDENCIES_NAME: &str = "manage_service_dependencies";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageServiceDependenciesArgs {
    /// The dependent service (name or id). Required.
    pub service: String,
    /// The service it depends on (name or id). Required.
    pub depends_on: String,
    /// Project identifier or name; use it when a service name is not unique.
    pub project: Option<String>,
    /// One of: link, unlink.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageServiceDependenciesProposal {
    pub service: String,
    pub depends_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub action: String,
}
```

Validator `manage_service_dependencies_proposal_from_args`:

- `service` required, trimmed, ≤ `CONTAINER_REF_MAX` (255).
- `depends_on` required, trimmed, ≤ `CONTAINER_REF_MAX` (255).
- `service` and `depends_on` must differ (trimmed, case-insensitive) → `invalid_args("service and depends_on must be different services")` (cheap early guard; the authoritative self-edge check is server-side and in the card).
- `action` via `enum_arg(&["link", "unlink"], "action")`.
- `project` via `optional_container_project` (≤ `CONTAINER_PROJECT_MAX`).

`Tool` impl: description mirrors `manage_service_links` phrasing — only call when the user asks to connect/disconnect two services; both names (or ids) required, never guess; both services must be in the same project; the user must confirm the card in the UI before anything is saved; never claim the connection was changed until they confirm. `call()` validates, records the proposal to the trace, returns the serialized JSON.

`mutation_kind_for_tool` gains: `MANAGE_SERVICE_DEPENDENCIES_NAME => Some("manage_service_dependencies")` (`mutations.rs:528-547`).

### 2. Registry & prompt — `tools/mod.rs`, `agent.rs`

- `PROPOSAL_TOOL_NAMES` (`tools/mod.rs:42-60`): 17 → 18 entries; add the new name.
- `workspace_tools()`: register `ManageServiceDependencies { trace }` (trace-only; no pool).
- `read_tools()`: **no change** — proposal tools stay out, so scheduled runs cannot mutate.
- `PREAMBLE` (`crates/ai/src/agent.rs`, near the `manage_service_links` guidance at `:77`): add a sentence for the new tool — use it when the user asks to connect/disconnect two services; the direction is "<service> depends on <depends_on>"; require explicit names; call `get_service` first when unsure about names or current edges; never claim a connection was added or removed before the user confirms.

### 3. Metadata allowlist — `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- Add `"manage_service_dependencies"` to the empty-allowlist group in `allowed_result_keys` (`:526-545`); it is absent from `required_result_key` (no `result` required) — identical to `manage_service_links`.

### 4. FE types & validation — `apps/web/core/lib/ai-proposals.ts`

- `PROPOSAL_KINDS` (`:10-26`): add `"manage_service_dependencies"` (order: after `manage_service_links`).
- Types:

```ts
export type TAiManageServiceDependenciesProposal = {
  service: string;
  depends_on: string;
  project?: string | null;
  action: "link" | "unlink";
};
```

- `TAiProposal` union: new variant `{ key; kind: "manage_service_dependencies"; proposal: TAiManageServiceDependenciesProposal }`.
- `TAiProposalConfirmPayload` union: `{ kind: "manage_service_dependencies"; projectId; action; fromServiceId; toServiceId; dependencyId?: string }` (`dependencyId` required when `action === "unlink"`).
- `validateManageServiceDependenciesProposal` mirrors the backend: both refs required, different (case-insensitive), action enum.
- New pure helper (unit-testable, mirrors the server CTE at `service.rs:610-624`):

```ts
export const createsDependencyCycle = (
  edges: { from_service_id: string; to_service_id: string }[],
  fromId: string,
  toId: string
): boolean; // true when `fromId === toId` or `fromId` is reachable from `toId`
```

### 5. Card — `apps/web/core/components/ai/assistant-sidebar/proposals/dependency-proposal-card.tsx` (new)

Modeled on `ServiceLinksCard` (`link-items-proposal-card.tsx:61-212`); observer component fed by `proposal`, `decision`, `onConfirm`, `onCancel`.

- **Title/body**: "Connect services" (`link`) / "Disconnect services" (`unlink`); renders `{service} → {depends_on}` with the caption "`{service}` depends on `{depends_on}`".
- **Project resolution**: `matchProject(projects, proposal.project)` when `project` is set; otherwise the first workspace project (in `workspaceProjectIds` order) in which `service` resolves after `fetchServices`; otherwise render a project `<select>` (ContainerItemsCard pattern, `link-items-proposal-card.tsx:361-377`). Only one project is ever active — both services must resolve there.
- **Resolution**: `fetchServices(slug, workspaceId, projectId)` once per active project; service list built from `getProjectServiceIds(projectId)` + `getServiceById`; both refs resolved with `matchByNameOrId` (exact id, else case-insensitive exact name — no fuzzy matching).
- **Edge lookup**: `getDependenciesByProject(projectId).find(d => d.from_service_id === fromId && d.to_service_id === toId)` (`service.store.ts:181-183`).
- **Single-row status** (Confirm disabled unless "actionable"):
  - either ref unresolved → "not found in this project".
  - same resolved id → "a service cannot depend on itself".
  - `link` + edge exists → "already connected — nothing to do".
  - `link` + `createsDependencyCycle(...)` → "would create a dependency cycle".
  - `unlink` + no edge → "not connected — nothing to do".
  - else actionable.
- **Confirm**: `onConfirm({ kind: "manage_service_dependencies", projectId, action, fromServiceId, toServiceId, dependencyId? })`; on success re-fetch `fetchServices` (so the graph/detail pages reflect the new edge immediately) — the card already holds the service-store hook; on failure show the error inline and keep the decision pending (retryable), matching existing cards.
- **Decision states**: applied → "Connection added." / "Connection removed." plus an "Open service" link to `/{workspaceSlug}/projects/{projectId}/services/{fromServiceId}` (route registered at `apps/web/app/routes/core.ts:180-183`); cancelled → "Connection update cancelled."

### 6. Routing & dispatch

- `proposals/proposal-card.tsx`: route `entry.kind === "manage_service_dependencies"` → `DependencyProposalCard`.
- `store/ai-assistant.store.ts` `confirmProposal` dispatch (after the `manage_service_links` branch at `:602`):
  - `link` → `await this.servicesService.createDependency(slug, "", payload.projectId, payload.fromServiceId, payload.toServiceId)`.
  - `unlink` → require `payload.dependencyId` (throw when missing, like the unlink guard at `:612-613`), then `await this.servicesService.deleteDependency(slug, "", payload.projectId, payload.dependencyId)`.
  - No `result`; decision `applied` with `{ kind, decision }` only (existing generic flow, `:639-645`).
- Cancel path is the existing generic `cancelProposal` — no change.

### 7. Errors & edge cases

- **Ambiguous names across projects** → project selector; both refs must resolve in the same selected project.
- **Race (edge added elsewhere)** → server 400 "This dependency already exists." → inline card error, decision stays pending. The FE pre-check minimizes this.
- **Cycle race** → card blocks via `createsDependencyCycle`; the server CTE double-checks.
- **Service deleted before confirm** → server "Source/Target service not found." → inline error, retryable.
- **Different casing** → `matchByNameOrId` handles case-insensitive exact names only; otherwise "not found" (no guessing), mirroring `pick_service` semantics on the backend read side.
- **Model misuse** (same ref twice) → backend `invalid_args` before any proposal is recorded.
- **Classic mode** → proposal tools only exist in agent mode; no change.
- **Scheduled runs** → structurally read-only (tool not in `read_tools()`); the worker's `allowed_tools` expectations are unchanged except registry test updates.

## Files touched

Backend:

- `apps/api-rs/crates/ai/src/tools/mutations.rs` — tool consts, args/proposal structs, validator, `Tool` impl, `mutation_kind_for_tool` arm, unit tests.
- `apps/api-rs/crates/ai/src/tools/mod.rs` — `PROPOSAL_TOOL_NAMES` (18), `workspace_tools()` registration, registry unit tests.
- `apps/api-rs/crates/ai/src/agent.rs` — PREAMBLE guidance only; `proposals` / `pending_actions` metadata derives from `mutation_kind_for_tool`, so no other backend plumbing is needed.
- `apps/api-rs/crates/api/src/routes/ai_conversations.rs` — `allowed_result_keys` entry.
- `apps/api-rs/crates/api/tests/ai_agent_test.rs` — roundtrip integration test.

Frontend:

- `apps/web/core/lib/ai-proposals.ts` — kind, types, validator, cycle helper.
- `apps/web/core/lib/ai-proposals.test.ts` — validator + cycle-helper tests.
- `apps/web/core/components/ai/assistant-sidebar/proposals/dependency-proposal-card.tsx` — new card.
- `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx` — routing.
- `apps/web/core/store/ai-assistant.store.ts` — confirm dispatch branch.
- `apps/web/core/store/ai-assistant.store.test.ts` — dispatch tests.

## Testing & verification

Rust unit (`cd apps/api-rs && cargo test -p ai`):

- Validator: blank `service`, blank `depends_on`, equal refs (case-insensitive), unknown action, over-length refs; valid proposal normalizes (trim) and serializes with `service` / `depends_on` / `action` and optional `project`.
- `parameters()` exposes `service`, `depends_on`, `project`, `action`; description non-empty; `mutation_kind_for_tool("manage_service_dependencies")` maps to the kind.
- Registry: `workspace_tools()` includes the tool; `PROPOSAL_TOOL_NAMES` includes it; `read_tools()` never does.

Rust integration (`crates/api/tests/ai_agent_test.rs`, DB-backed — run per `apps/api-rs` rules):

- Roundtrip mirrors `manage_service_links_proposal_roundtrip` (`:1368`): fake upstream tool call → assistant metadata contains `proposals[0].kind == "manage_service_dependencies"` + matching `pending_actions` entry; PATCH of `proposal_decisions` with `{"decision": "applied"}` is accepted.

FE vitest (`pnpm --filter=web test`):

- `validateManageServiceDependenciesProposal`: required refs, equal refs, bad action, valid case.
- `createsDependencyCycle`: direct back-edge, transitive cycle, no cycle, self-edge; empty edge list.
- Store dispatch: link calls `createDependency(projectId, from, to)`; unlink calls `deleteDependency(projectId, dependencyId)`; unlink without `dependencyId` throws and leaves the decision pending; applied decision persisted without `result`.
- Existing proposal suites keep passing.

Checks: `pnpm check` (format, lint, types) and cargo fmt/clippy as configured.

Manual smoke (prepaid project, agent mode):

1. "connect prepaid-iso -> core-banking" → card "prepaid-iso depends on core-banking" → Confirm → edge appears in service detail/graph and in `get_service` output.
2. "disconnect prepaid-iso -> core-banking" → card → Confirm → edge gone.
3. Already-connected, self-edge, and cycle attempts show the disabled reasons; no request is sent.
4. Wrong names → "not found in this project", Confirm disabled.
5. Scheduled run still exposes no proposal tools.
