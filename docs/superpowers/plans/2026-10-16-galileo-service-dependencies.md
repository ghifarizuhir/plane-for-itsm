# Galileo Service Dependency Proposals (Connect / Disconnect Services) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah proposal tool `manage_service_dependencies` (satu edge per proposal) plus kartu `DependencyProposalCard` yang me-resolve dua nama service di FE, memvalidasi self/cycle/duplikat, dan mengeksekusi `createDependency` / `deleteDependency` lewat endpoint REST existing saat Confirm.

**Architecture:** Tool hidup di `tools/mutations.rs` (trace-only, tanpa akses DB) dan hanya memvalidasi bentuk. Metadata generik (`proposals` / `proposal_decisions`, `mutation_kind_for_tool`, `pending_actions`) sudah ada dan otomatis memuat kind baru begitu arm mapping ditambah. FE memakai service store yang sudah dimuat (`fetchServices`, `getProjectServiceIds`, `getServiceById`, `getDependenciesByProject`) untuk resolusi nama → id, deteksi no-op, dan pre-check cycle (`createsDependencyCycle`, mirror CTE server).

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), React + MobX + vitest.

**Spec:** `docs/superpowers/specs/2026-10-16-galileo-service-dependencies-design.md`

---

## File Structure

| File                                                                                   | Aksi | Tanggung jawab                                                                                             |
| -------------------------------------------------------------------------------------- | ---- | ---------------------------------------------------------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/tools/mutations.rs`                                         | Ubah | Tool `manage_service_dependencies` (args/proposal/validator/impl), arm `mutation_kind_for_tool`, test unit |
| `apps/api-rs/crates/ai/src/tools/mod.rs`                                               | Ubah | `PROPOSAL_TOOL_NAMES` 18 entri, registrasi di `workspace_tools()`, test metadata                           |
| `apps/api-rs/crates/ai/src/agent.rs`                                                   | Ubah | `PREAMBLE` + test `pending_actions` kind baru                                                              |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                                | Ubah | Allowlist `allowed_result_keys` + test decision tanpa result                                               |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`                                        | Ubah | Roundtrip: pending_actions + metadata `proposals` + PATCH decision                                         |
| `apps/web/core/lib/ai-proposals.ts`                                                    | Ubah | Kind baru, tipe proposal + confirm payload, validator, `createsDependencyCycle`                            |
| `apps/web/core/lib/ai-proposals.test.ts`                                               | Ubah | Test validator + helper cycle                                                                              |
| `apps/web/core/components/ai/assistant-sidebar/proposals/dependency-proposal-card.tsx` | Baru | Kartu proposal dependency                                                                                  |
| `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`            | Ubah | Routing kind baru                                                                                          |
| `apps/web/core/store/ai-assistant.store.ts`                                            | Ubah | Pick `createDependency`/`deleteDependency` + dispatch arm                                                  |
| `apps/web/core/store/ai-assistant.store.test.ts`                                       | Ubah | Mock service baru + test dispatch                                                                          |

---

### Task 1: Tool `manage_service_dependencies` + validator

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs` (setelah test `service_links_normalize_and_validate`):

```rust
    #[test]
    fn service_dependencies_normalize_and_validate() {
        let proposal =
            manage_service_dependencies_proposal_from_args(ManageServiceDependenciesArgs {
                service: " prepaid-iso ".into(),
                depends_on: " core-banking ".into(),
                project: Some(" PRE ".into()),
                action: " LINK ".into(),
            })
            .unwrap();
        assert_eq!(proposal.service, "prepaid-iso");
        assert_eq!(proposal.depends_on, "core-banking");
        assert_eq!(proposal.project.as_deref(), Some("PRE"));
        assert_eq!(proposal.action, "link");

        let blank_service =
            manage_service_dependencies_proposal_from_args(ManageServiceDependenciesArgs {
                service: "  ".into(),
                depends_on: "core-banking".into(),
                project: None,
                action: "link".into(),
            })
            .unwrap_err();
        assert!(blank_service.to_string().contains("service is required"));

        let blank_depends_on =
            manage_service_dependencies_proposal_from_args(ManageServiceDependenciesArgs {
                service: "prepaid-iso".into(),
                depends_on: " ".into(),
                project: None,
                action: "link".into(),
            })
            .unwrap_err();
        assert!(blank_depends_on.to_string().contains("depends_on is required"));

        let same_service =
            manage_service_dependencies_proposal_from_args(ManageServiceDependenciesArgs {
                service: "Core-Banking".into(),
                depends_on: "core-banking".into(),
                project: None,
                action: "link".into(),
            })
            .unwrap_err();
        assert!(same_service.to_string().contains("different services"));

        let bad_action =
            manage_service_dependencies_proposal_from_args(ManageServiceDependenciesArgs {
                service: "prepaid-iso".into(),
                depends_on: "core-banking".into(),
                project: None,
                action: "attach".into(),
            })
            .unwrap_err();
        assert!(bad_action.to_string().contains("link, unlink"));
    }

    #[test]
    fn service_dependencies_proposal_serializes_keys() {
        let proposal =
            manage_service_dependencies_proposal_from_args(ManageServiceDependenciesArgs {
                service: "prepaid-iso".into(),
                depends_on: "core-banking".into(),
                project: None,
                action: "unlink".into(),
            })
            .unwrap();
        let value = serde_json::to_value(&proposal).unwrap();
        assert_eq!(value["service"], serde_json::json!("prepaid-iso"));
        assert_eq!(value["depends_on"], serde_json::json!("core-banking"));
        assert_eq!(value["action"], serde_json::json!("unlink"));
        assert!(value.get("project").is_none());
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p ai service_dependencies`
Expected: gagal kompilasi (`ManageServiceDependenciesArgs` belum ada).

- [ ] **Step 3: Implementasi tool**

Tambahkan di `mutations.rs` tepat setelah blok `impl Tool for ManageServiceLinks` (sebelum `pub const MANAGE_SPRINT_ITEMS_NAME`):

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

/// Validate raw tool args into a normalized service dependency proposal.
pub fn manage_service_dependencies_proposal_from_args(
    args: ManageServiceDependenciesArgs,
) -> Result<ManageServiceDependenciesProposal, ToolExecutionError> {
    let service = required_text(&args.service, "service", CONTAINER_REF_MAX)?;
    let depends_on = required_text(&args.depends_on, "depends_on", CONTAINER_REF_MAX)?;
    if service.eq_ignore_ascii_case(&depends_on) {
        return Err(ToolExecutionError::invalid_args(
            "service and depends_on must be different services",
        ));
    }
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["link", "unlink"], "action")?;
    Ok(ManageServiceDependenciesProposal {
        service,
        depends_on,
        project,
        action,
    })
}

pub struct ManageServiceDependencies {
    pub trace: ToolTrace,
}

impl Tool for ManageServiceDependencies {
    const NAME: &'static str = MANAGE_SERVICE_DEPENDENCIES_NAME;
    type Args = ManageServiceDependenciesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose connecting or disconnecting two services in the service \
         graph: service depends on depends_on. Only call this when the user \
         asks to connect or disconnect two services. Both service names (or \
         ids) are required; never guess them, and both must be in the same \
         project. The user must confirm the card in the UI before anything is \
         saved. Never claim the connection was changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageServiceDependenciesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_service_dependencies_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageServiceDependenciesProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run (dari `apps/api-rs`): `cargo test -p ai service_dependencies`
Expected: PASS (2 test).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add manage_service_dependencies proposal tool"
```

---

### Task 2: Kind mapping, registry, dan `pending_actions`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs` (setelah test `mutation_kind_mapping_covers_link_tools`):

```rust
    #[test]
    fn mutation_kind_mapping_covers_service_dependencies() {
        assert_eq!(
            mutation_kind_for_tool(MANAGE_SERVICE_DEPENDENCIES_NAME),
            Some("manage_service_dependencies")
        );
    }
```

Tambahkan di `mod tests` `agent.rs` (setelah test `pending_actions_covers_link_mutations`):

```rust
    #[test]
    fn pending_actions_covers_service_dependency_mutations() {
        let trace = new_trace();
        record(
            &trace,
            crate::tools::MANAGE_SERVICE_DEPENDENCIES_NAME,
            &json!({"service": "prepaid-iso", "depends_on": "core-banking", "action": "link"}),
        );
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["kind"], json!("manage_service_dependencies"));
    }
```

Tambahkan di `mod tests` `tools/mod.rs`, di dalam test `link_tools_expose_metadata` (setelah assertion `track`):

```rust
        let dependencies = ManageServiceDependencies {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(
            ManageServiceDependencies::NAME,
            "manage_service_dependencies"
        );
        assert!(dependencies.parameters()["properties"]["service"].is_object());
        assert!(dependencies.parameters()["properties"]["depends_on"].is_object());
        assert!(dependencies.parameters()["properties"]["action"].is_object());
        assert!(!dependencies.description().is_empty());
        assert!(PROPOSAL_TOOL_NAMES.contains(&MANAGE_SERVICE_DEPENDENCIES_NAME));
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p ai mutation_kind_mapping_covers_service_dependencies`
Expected: FAIL (mapping mengembalikan `None`).

- [ ] **Step 3: Implementasi mapping + registry**

Di `mutations.rs`, tambahkan arm di `mutation_kind_for_tool` setelah arm `MANAGE_SERVICE_LINKS_NAME`:

```rust
        MANAGE_SERVICE_DEPENDENCIES_NAME => Some("manage_service_dependencies"),
```

Di `tools/mod.rs`, ubah `PROPOSAL_TOOL_NAMES` menjadi 18 entri dan tambahkan konstanta baru setelah `MANAGE_SERVICE_LINKS_NAME`:

```rust
pub const PROPOSAL_TOOL_NAMES: [&str; 18] = [
    CREATE_SCHEDULE_NAME,
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

Di `workspace_tools()`, tambahkan registrasi setelah `.tool(ManageServiceLinks { ... })`:

```rust
        .tool(ManageServiceDependencies {
            trace: trace.clone(),
        })
```

`read_tools()` tidak diubah — proposal tool tidak pernah schedule-eligible.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run (dari `apps/api-rs`): `cargo test -p ai`
Expected: seluruh test `ai` lulus (termasuk `proposal_tools_are_not_schedule_eligible`).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): register service dependencies tool and pending action"
```

---

### Task 3: `PREAMBLE` guidance

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Update PREAMBLE**

Di `PREAMBLE` (`agent.rs`), sisipkan setelah kalimat `manage_service_links` (setelah `work item identifier and service names or ids. `):

```text
To connect or disconnect two services in the service graph, call \
manage_service_dependencies with the service that depends and the service it \
depends on (service depends on depends_on); both must be in the same project. \
```

Hasil akhir bagian tersebut (untuk verifikasi visual):

```rust
that confirmation. To link or unlink services on a work item, call manage_service_links with the \
work item identifier and service names or ids. To connect or disconnect two \
services in the service graph, call manage_service_dependencies with the \
service that depends and the service it depends on (service depends on \
depends_on); both must be in the same project. To add or remove work items in \
a sprint or track, call manage_sprint_items or manage_track_items with the \
```

- [ ] **Step 2: Jalankan test regresi**

Run (dari `apps/api-rs`): `cargo test -p ai`
Expected: seluruh test lulus.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): teach preamble about service dependencies"
```

---

### Task 4: Allowlist `proposal_decisions` (tanpa result)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_conversations.rs` (setelah test `metadata_patch_accepts_link_decisions_without_result`):

```rust
    #[test]
    fn metadata_patch_accepts_service_dependency_decisions() {
        let key = Uuid::new_v4().to_string();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({
                (key.clone()): {"kind": "manage_service_dependencies", "decision": "applied"},
            }),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][key.as_str()]["decision"],
            json!("applied")
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p api --lib metadata_patch_accepts_service_dependency_decisions`
Expected: FAIL (`kind not allowed: manage_service_dependencies`).

- [ ] **Step 3: Implementasi allowlist**

Di `allowed_result_keys`, tambahkan kind baru ke grup allowlist kosong:

```rust
        | "manage_service_links"
        | "manage_service_dependencies"
        | "manage_sprint_items"
```

`required_result_key` tidak diubah (kind ini tidak butuh result id).

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run (dari `apps/api-rs`): `cargo test -p api --lib metadata_patch`
Expected: seluruh test `metadata_patch` lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): allow service dependency proposal decisions"
```

---

### Task 5: Roundtrip integrasi agent

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tulis test**

Tambahkan setelah test `manage_service_links_proposal_roundtrip`:

```rust
#[tokio::test]
async fn manage_service_dependencies_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "manage_service_dependencies",
        r#"{"service":"prepaid-iso","depends_on":"core-banking","action":"link"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "connect prepaid-iso to core-banking",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);

    // pending_actions carries the proposal for the current turn.
    let actions = body["pending_actions"].as_array().expect("actions array");
    let action = actions
        .iter()
        .find(|action| action["kind"] == json!("manage_service_dependencies"))
        .expect("service dependency action");
    assert_eq!(action["proposal"]["service"], json!("prepaid-iso"));
    assert_eq!(action["proposal"]["depends_on"], json!("core-banking"));

    let proposals = body["assistant_message"]["metadata"]["proposals"]
        .as_array()
        .expect("proposals array");
    assert_eq!(proposals[0]["kind"], json!("manage_service_dependencies"));
    assert_eq!(proposals[0]["proposal"]["service"], json!("prepaid-iso"));
    assert_eq!(proposals[0]["proposal"]["depends_on"], json!("core-banking"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "manage_service_dependencies", "decision": "applied"}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["decision"],
        json!("applied")
    );

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan test**

Run (dari `apps/api-rs`, butuh DB dev): `cargo test -p api --test ai_agent_test manage_service_dependencies_proposal_roundtrip -- --test-threads=1`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): roundtrip service dependency proposals"
```

---

### Task 6: FE — tipe, validator, helper cycle

**Files:**

- Modify: `apps/web/core/lib/ai-proposals.ts`
- Modify: `apps/web/core/lib/ai-proposals.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-proposals.test.ts`, tambahkan import `createsDependencyCycle` dan `validateManageServiceDependenciesProposal` ke daftar import dari `./ai-proposals` (urut alfabetis mengikuti daftar existing).

Tambahkan di dalam `describe("link proposal validators", ...)` (setelah test `validates service link proposals`):

```ts
it("validates service dependency proposals", () => {
  expect(
    validateManageServiceDependenciesProposal({
      service: "prepaid-iso",
      depends_on: "core-banking",
      action: "link",
    })
  ).toBeNull();
  expect(validateManageServiceDependenciesProposal({ depends_on: "core-banking", action: "link" })).toBe(
    "Service is required."
  );
  expect(validateManageServiceDependenciesProposal({ service: "prepaid-iso", action: "link" })).toBe(
    "Depends on is required."
  );
  expect(
    validateManageServiceDependenciesProposal({
      service: "Core-Banking",
      depends_on: "core-banking",
      action: "link",
    })
  ).toBe("Service and depends on must be different services.");
  expect(
    validateManageServiceDependenciesProposal({
      service: "prepaid-iso",
      depends_on: "core-banking",
      action: "attach" as never,
    })
  ).toBe("Action must be link or unlink.");
});
```

Tambahkan describe baru setelah `describe("matchByNameOrId", ...)`:

```ts
describe("createsDependencyCycle", () => {
  const edges = [
    { from_service_id: "a", to_service_id: "b" },
    { from_service_id: "b", to_service_id: "c" },
  ];

  it("flags direct and transitive cycles", () => {
    expect(createsDependencyCycle(edges, "b", "a")).toBe(true);
    expect(createsDependencyCycle(edges, "c", "a")).toBe(true);
    expect(createsDependencyCycle(edges, "c", "b")).toBe(true);
  });

  it("flags self-edges and allows independent edges", () => {
    expect(createsDependencyCycle(edges, "a", "a")).toBe(true);
    expect(createsDependencyCycle(edges, "a", "c")).toBe(false);
    expect(createsDependencyCycle(edges, "d", "a")).toBe(false);
    expect(createsDependencyCycle([], "a", "b")).toBe(false);
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/web`): `pnpm test core/lib/ai-proposals.test.ts`
Expected: gagal kompilasi/import (fungsi belum ada).

- [ ] **Step 3: Implementasi tipe + validator + helper**

Di `ai-proposals.ts`:

1. Tambahkan `"manage_service_dependencies"` ke `PROPOSAL_KINDS` tepat setelah `"manage_service_links"`.
2. Tambahkan tipe tepat setelah `TAiManageServiceLinksProposal`:

```ts
export type TAiManageServiceDependenciesProposal = {
  service: string;
  depends_on: string;
  project?: string | null;
  action: "link" | "unlink";
};
```

3. Tambahkan varian di union `TAiProposal` tepat setelah varian `manage_service_links`:

```ts
  | { key: string; kind: "manage_service_dependencies"; proposal: TAiManageServiceDependenciesProposal }
```

4. Tambahkan arm di union `TAiProposalConfirmPayload` tepat setelah arm `manage_service_links`:

```ts
  | {
      kind: "manage_service_dependencies";
      projectId: string;
      action: "link" | "unlink";
      fromServiceId: string;
      toServiceId: string;
      dependencyId?: string;
    }
```

5. Tambahkan validator tepat setelah `validateManageServiceLinksProposal`:

```ts
/** Mirrors `manage_service_dependencies_proposal_from_args` on the backend. */
export const validateManageServiceDependenciesProposal = (
  proposal: Partial<TAiManageServiceDependenciesProposal>
): string | null => {
  const service = proposal.service?.trim() ?? "";
  const dependsOn = proposal.depends_on?.trim() ?? "";
  if (!service) return "Service is required.";
  if (!dependsOn) return "Depends on is required.";
  if (service.toLowerCase() === dependsOn.toLowerCase()) return "Service and depends on must be different services.";
  if (proposal.action !== "link" && proposal.action !== "unlink") return "Action must be link or unlink.";
  return null;
};
```

6. Tambahkan helper cycle tepat setelah validator di atas (mirror CTE server `service.rs:610-624`):

```ts
/** True when adding `fromId -> toId` creates a cycle: `fromId` is reachable
 * from `toId` following the existing edges (or the edge is a self-edge). */
export const createsDependencyCycle = (
  edges: { from_service_id: string; to_service_id: string }[],
  fromId: string,
  toId: string
): boolean => {
  if (fromId === toId) return true;
  const seen = new Set<string>();
  const stack = [toId];
  while (stack.length > 0) {
    const node = stack.pop() as string;
    if (seen.has(node)) continue;
    seen.add(node);
    for (const edge of edges) {
      if (edge.from_service_id !== node) continue;
      if (edge.to_service_id === fromId) return true;
      stack.push(edge.to_service_id);
    }
  }
  return false;
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run (dari `apps/web`): `pnpm test core/lib/ai-proposals.test.ts`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposals.test.ts
git commit -m "feat(web): service dependency proposal types and cycle helper"
```

---

### Task 7: FE — kartu `DependencyProposalCard`

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/dependency-proposal-card.tsx`

- [ ] **Step 1: Buat komponen kartu**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { matchProject } from "@/lib/ai-work-items";
import {
  createsDependencyCycle,
  matchByNameOrId,
  validateManageServiceDependenciesProposal,
  type TAiManageServiceDependenciesProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TNamedRef,
} from "@/lib/ai-proposals";

type Props = {
  proposal: TAiManageServiceDependenciesProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const DependencyProposalCard = observer(function DependencyProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { currentWorkspace } = useWorkspace();
  const workspaceId = currentWorkspace?.id;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchServices, getProjectServiceIds, getServiceById, getDependenciesByProject } = useService();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));
  const projectsKey = projects.map((project) => project.id).join("|");
  const matchedProjectId = proposal.project ? (matchProject(projects, proposal.project)?.id ?? null) : null;
  const activeProjectId = selectedProjectId ?? matchedProjectId;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || !activeProjectId) return;
    void fetchServices(workspaceSlug, workspaceId, activeProjectId);
  }, [workspaceSlug, workspaceId, activeProjectId, fetchServices]);

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || activeProjectId) return;
    let cancelled = false;
    void (async () => {
      setScanning(true);
      try {
        for (const projectId of projectsKey.split("|").filter(Boolean)) {
          if (cancelled) return;
          await fetchServices(workspaceSlug, workspaceId, projectId);
          const ids = getProjectServiceIds(projectId) ?? [];
          const services: TNamedRef[] = ids.flatMap((id) => {
            const service = getServiceById(id);
            return service ? [{ id: service.id, name: service.name }] : [];
          });
          if (matchByNameOrId(services, proposal.service)) {
            if (!cancelled) setSelectedProjectId(projectId);
            return;
          }
        }
      } finally {
        if (!cancelled) setScanning(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [
    workspaceSlug,
    workspaceId,
    activeProjectId,
    projectsKey,
    fetchServices,
    getProjectServiceIds,
    getServiceById,
    proposal.service,
  ]);

  const serviceIds = activeProjectId ? (getProjectServiceIds(activeProjectId) ?? []) : [];
  const services: TNamedRef[] = serviceIds.flatMap((id) => {
    const service = getServiceById(id);
    return service ? [{ id: service.id, name: service.name }] : [];
  });
  const servicesLoaded = activeProjectId ? getProjectServiceIds(activeProjectId) !== null : false;
  const fromService = matchByNameOrId(services, proposal.service);
  const toService = matchByNameOrId(services, proposal.depends_on);
  const dependencies = activeProjectId ? getDependenciesByProject(activeProjectId) : [];
  const existing =
    fromService && toService
      ? dependencies.find(
          (dependency) => dependency.from_service_id === fromService.id && dependency.to_service_id === toService.id
        )
      : undefined;
  const validationError = validateManageServiceDependenciesProposal(proposal);

  let blockedReason: string | null = null;
  if (servicesLoaded) {
    if (!fromService || !toService) blockedReason = "Service not found in this project.";
    else if (fromService.id === toService.id) blockedReason = "A service cannot depend on itself.";
    else if (proposal.action === "link" && existing) blockedReason = "Already connected — nothing to do.";
    else if (proposal.action === "link" && createsDependencyCycle(dependencies, fromService.id, toService.id))
      blockedReason = "This connection would create a dependency cycle.";
    else if (proposal.action === "unlink" && !existing) blockedReason = "Not connected — nothing to do.";
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {proposal.action === "link" ? "Connection added." : "Connection removed."}{" "}
        {workspaceSlug && activeProjectId && fromService && (
          <Link
            href={`/${workspaceSlug}/projects/${activeProjectId}/services/${fromService.id}`}
            className="text-accent-primary hover:underline"
          >
            Open service
          </Link>
        )}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Connection update cancelled.
      </p>
    );
  }

  const confirm = async () => {
    if (!activeProjectId || !fromService || !toService || blockedReason || validationError) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "manage_service_dependencies",
        projectId: activeProjectId,
        action: proposal.action,
        fromServiceId: fromService.id,
        toServiceId: toService.id,
        ...(proposal.action === "unlink" && existing ? { dependencyId: existing.id } : {}),
      });
      if (workspaceSlug && workspaceId) void fetchServices(workspaceSlug, workspaceId, activeProjectId);
    } catch {
      setError("Could not update the connection. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Service dependency proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {proposal.action === "link" ? "Connect services" : "Disconnect services"}
      </p>
      <p className="mt-1 text-12 text-secondary">
        {proposal.service} → {proposal.depends_on}{" "}
        <span className="text-tertiary">
          ({proposal.service} depends on {proposal.depends_on})
        </span>
      </p>
      {!activeProjectId && scanning && <p className="mt-1 text-12 text-tertiary">Looking up the services…</p>}
      {!activeProjectId && !scanning && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => setSelectedProjectId(event.target.value || null)}
          >
            <option value="">Select a project</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.identifier} — {project.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {activeProjectId && !servicesLoaded && <p className="mt-1 text-12 text-tertiary">Loading services…</p>}
      {blockedReason && <p className="mt-1 text-12 text-tertiary">{blockedReason}</p>}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={
            !activeProjectId ||
            !servicesLoaded ||
            !fromService ||
            !toService ||
            Boolean(blockedReason) ||
            Boolean(validationError)
          }
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
});
```

- [ ] **Step 2: Verifikasi tipe**

Run (dari `apps/web`): `pnpm check:types`
Expected: lulus tanpa error baru.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/proposals/dependency-proposal-card.tsx
git commit -m "feat(web): dependency proposal card"
```

---

### Task 8: FE — routing kartu + dispatch store

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`
- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test store yang gagal**

Di `ai-assistant.store.test.ts`:

1. Tambahkan mock service baru di `makeServices()` (blok `services:`):

```ts
    createDependency: vi.fn(async () => ({ id: "d1" })),
    deleteDependency: vi.fn(async () => undefined),
```

2. Tambahkan metadata const setelah `serviceLinksMetadata`:

```ts
const serviceDependencyLinkMetadata = {
  proposals: [
    {
      key: "k3c",
      kind: "manage_service_dependencies",
      proposal: { service: "prepaid-iso", depends_on: "core-banking", action: "link" },
    },
  ],
};

const serviceDependencyUnlinkMetadata = {
  proposals: [
    {
      key: "k3d",
      kind: "manage_service_dependencies",
      proposal: { service: "prepaid-iso", depends_on: "core-banking", action: "unlink" },
    },
  ],
};
```

3. Tambahkan test di dalam `describe("link proposals", ...)` (setelah test `unlinks a service with its link id`):

```ts
it("connects two services", async () => {
  const { store, services } = await openAgent(makeServices(), serviceDependencyLinkMetadata);
  await store.confirmProposal("srv-assistant", "k3c", {
    kind: "manage_service_dependencies",
    projectId: "p1",
    action: "link",
    fromServiceId: "s1",
    toServiceId: "s2",
  });
  expect(services.services.createDependency).toHaveBeenCalledWith("acme", "", "p1", "s1", "s2");
  expect(store.messages[0].proposalDecisions?.k3c).toEqual({
    kind: "manage_service_dependencies",
    decision: "applied",
  });
});

it("disconnects two services with the dependency id", async () => {
  const { store, services } = await openAgent(makeServices(), serviceDependencyUnlinkMetadata);
  await store.confirmProposal("srv-assistant", "k3d", {
    kind: "manage_service_dependencies",
    projectId: "p1",
    action: "unlink",
    fromServiceId: "s1",
    toServiceId: "s2",
    dependencyId: "d1",
  });
  expect(services.services.deleteDependency).toHaveBeenCalledWith("acme", "", "p1", "d1");
  expect(store.messages[0].proposalDecisions?.k3d).toEqual({
    kind: "manage_service_dependencies",
    decision: "applied",
  });
});

it("rejects an unlink without the dependency id and stays pending", async () => {
  const { store, services } = await openAgent(makeServices(), serviceDependencyUnlinkMetadata);
  await expect(
    store.confirmProposal("srv-assistant", "k3d", {
      kind: "manage_service_dependencies",
      projectId: "p1",
      action: "unlink",
      fromServiceId: "s1",
      toServiceId: "s2",
    })
  ).rejects.toThrow("dependency id");
  expect(services.services.deleteDependency).not.toHaveBeenCalled();
  expect(store.messages[0].proposalDecisions?.k3d).toBeUndefined();
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/web`): `pnpm test core/store/ai-assistant.store.test.ts`
Expected: FAIL (`createDependency` belum ada di tipe; test connect gagal).

- [ ] **Step 3: Implementasi routing + dispatch**

Di `proposal-card.tsx`, tambahkan import dan branch:

```tsx
import { DependencyProposalCard } from "./dependency-proposal-card";
```

```tsx
if (entry.kind === "manage_service_dependencies")
  return (
    <DependencyProposalCard proposal={entry.proposal} decision={decision} onConfirm={onConfirm} onCancel={onCancel} />
  );
```

Di `ai-assistant.store.ts`:

1. Perluas `TServicesService`:

```ts
type TServicesService = Pick<
  ServiceService,
  "linkWorkItem" | "unlinkWorkItem" | "createService" | "updateService" | "createDependency" | "deleteDependency"
>;
```

2. Tambahkan dispatch arm tepat setelah blok `else if (payload.kind === "manage_service_links") { ... }`:

```ts
    } else if (payload.kind === "manage_service_dependencies") {
      if (payload.action === "link") {
        await this.servicesService.createDependency(
          slug,
          "",
          payload.projectId,
          payload.fromServiceId,
          payload.toServiceId
        );
      } else {
        if (!payload.dependencyId) throw new Error("Unlink target is missing its dependency id");
        await this.servicesService.deleteDependency(slug, "", payload.projectId, payload.dependencyId);
      }
    } else if (payload.kind === "manage_sprint_items") {
```

(Teks `} else if (payload.kind === "manage_sprint_items") {` yang lama digantikan oleh blok di atas; pastikan hanya satu yang tersisa.)

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run (dari `apps/web`): `pnpm test core/store/ai-assistant.store.test.ts`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): dispatch service dependency proposals"
```

---

### Task 9: Verifikasi akhir

- [ ] **Step 1: Seluruh test Rust**

Run (dari `apps/api-rs`): `cargo test -p ai`
Expected: seluruh test lulus.

Run (dari `apps/api-rs`, butuh DB dev): `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus.

- [ ] **Step 2: Build backend**

Run (dari `apps/api-rs`): `cargo build -p api -p worker`
Expected: `Finished`.

- [ ] **Step 3: Seluruh test FE + tipe + lint**

Run (dari `apps/web`): `pnpm test`
Expected: seluruh test vitest lulus.

Run (dari `apps/web`): `pnpm check:types`
Expected: lulus.

Run (dari `apps/web`): `pnpm check:lint`
Expected: lulus (jumlah warning tidak bertambah).

- [ ] **Step 4: Format**

Run (dari root): `pnpm fix`
Commit bila formatter mengubah file:

```bash
git add -A
git commit -m "chore: format galileo service dependencies"
```

- [ ] **Step 5: Smoke manual (butuh stack hidup)**

Ikuti AGENTS.md:

1. Rebuild backend api-rs (detached):
   `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &`
   Poll log sampai selesai, lalu `curl http://localhost:8000/health` → 200, lalu `systemctl --user restart plane-live.service`.
2. Rebuild web: `pnpm --filter=web build` lalu `systemctl --user restart plane-web-prod.service`.
3. Di Agent mode, project prepaid:
   - "connect prepaid-iso -> core-banking" → kartu "prepaid-iso depends on core-banking" → Confirm → edge muncul di detail/graph service dan di output `get_service`.
   - "disconnect prepaid-iso -> core-banking" → kartu → Confirm → edge hilang.
   - Percobaan already-connected, self-edge, dan cycle → alasan disabled muncul, tidak ada request terkirim.
   - Nama salah → "Service not found in this project", Confirm disabled.
4. Scheduled run tetap tidak mengekspos proposal tool.

---

## Self-Review Checklist (sudah dijalankan)

- **Spec coverage:** §1 tool + validator → Task 1; §2 registry + prompt → Task 2–3; §3 allowlist metadata → Task 4; §4 tipe/validator/cycle helper → Task 6; §5 kartu → Task 7; §6 routing + dispatch → Task 8; §Testing (unit Rust, integrasi, vitest) → Task 1–8 + verifikasi Task 9; smoke manual → Task 9 Step 5.
- **Placeholder scan:** tidak ada TODO/TBD; semua langkah berisi kode lengkap dan perintah eksak.
- **Type consistency:** `ManageServiceDependenciesArgs`/`ManageServiceDependenciesProposal` (Rust) ↔ `TAiManageServiceDependenciesProposal` (FE) konsisten (`service`, `depends_on`, `project`, `action`); confirm payload memakai `fromServiceId`/`toServiceId`/`dependencyId` di kartu, store, dan test; `PROPOSAL_TOOL_NAMES` 18 entri; `createsDependencyCycle` dipakai kartu dan diuji terpisah; unlink tanpa `dependencyId` melempar error yang sama di store dan diuji.
