# Service Management Satu Halaman — Web Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Workspace settings Service management hanya punya satu halaman `/settings/work-item-types`: list type, dan halaman detail per type yang menggabungkan field type + editor workflow (state + transisi); route workflows lama redirect; tidak ada lagi CRUD workflow standalone di UI.

**Architecture:** Sidebar kehilangan entri `workflows`; route `work-item-types/:typeId` membuka `WorkItemTypeDetail` yang memakai `WorkflowEditor` (di-refactor agar tidak fetch daftar workflow). Store/service membuang CRUD workflow (tetap menyediakan `fetchWorkflows` untuk halaman project settings). Nama workflow ditampilkan derived `${type.name} Workflow`.

**Tech Stack:** React + MobX + react-router v7 (file routes + typegen) + vitest, Tailwind/Plane UI.

**Spec:** `docs/superpowers/specs/2026-09-28-service-management-single-page-design.md`

**Prasyarat:** Plan backend (`2026-09-28-service-management-single-page-backend.md`) selesai — create type sudah auto-create workflow dan body `workflow` ditolak.

**Execution notes (baca sebelum mulai):**

- Working tree punya banyak file unrelated yang termodifikasi — **jangan** `git add -A`; commit hanya file yang disebut task.
- Pre-commit menjalankan oxlint `--fix --deny-warnings` dan oxfmt pada file web. Jangan pakai `key={index}` (`no-array-index-key`).
- `+types/*` di route adalah hasil typegen (`react-router typegen`, dijalankan `check:types`/build) — file baru mengikuti pola yang ada.
- Semua string baru lewat `packages/i18n`; Task 2 WAJIB memuat skill `translate` sebelum menyentuh `src/locales`.
- `fetchWorkflows` TETAP di store/service: dipakai `apps/web/core/components/project-work-item-types/root.tsx`.

---

### Task 1: Constants & tipe settings

**Files:**

- Modify: `packages/types/src/settings.ts:13`
- Modify: `packages/constants/src/settings/workspace.ts:61-90`
- Modify: `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`

- [ ] **Step 1: Hapus tab `workflows` dari tipe**

Di `packages/types/src/settings.ts`, ganti baris 13 menjadi:

```ts
export type TWorkspaceSettingsTabs = "general" | "members" | "export" | "webhooks" | "work_item_types";
```

- [ ] **Step 2: Hapus entri sidebar workflows**

Di `packages/constants/src/settings/workspace.ts`:

1. Hapus blok `workflows: { ... }` (baris 68-74).
2. Ubah `highlight` milik `work_item_types` agar juga aktif di halaman detail:

```ts
  work_item_types: {
    key: "work_item_types",
    i18n_label: "workspace_settings.settings.work_item_types.title",
    href: `/settings/work-item-types`,
    access: [EUserWorkspaceRoles.ADMIN],
    highlight: (pathname: string, baseUrl: string) => pathname.startsWith(`${baseUrl}/settings/work-item-types`),
  },
```

3. Ubah `GROUPED_WORKSPACE_SETTINGS` kategori Service management menjadi:

```ts
  [WORKSPACE_SETTINGS_CATEGORY.SERVICE_MANAGEMENT]: [WORKSPACE_SETTINGS["work_item_types"]],
```

- [ ] **Step 3: Hapus ikon workflows**

Di `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`, hapus `WorkflowsOutline` dari import dan hapus baris `workflows: WorkflowsOutline,`.

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/types/src/settings.ts packages/constants/src/settings/workspace.ts \
  apps/web/core/components/settings/workspace/sidebar/item-icon.tsx
git commit -m "refactor(web): drop workflows workspace settings entry"
```

### Task 2: i18n — string baru + buang string mati

**Files:**

- Modify: `packages/i18n/src/locales/en/workspace-settings.json:480-543`
- Modify: `packages/i18n/src/locales/<semua locale>/workspace-settings.json`

- [ ] **Step 1: Muat skill translate**

Baca `/home/ghifari/plane-for-itsm/.claude/skills/translate/SKILL.md` dan ikuti aturannya untuk seluruh langkah task ini (DNT glossary, register per locale, plural CLDR, dan workflow review terjemahan).

- [ ] **Step 2: Ubah `en`**

Di `packages/i18n/src/locales/en/workspace-settings.json`, pada objek `work_item_types` (baris 480-499):

1. Ganti `description` menjadi:
   `"Define the ITSM work item types available across the workspace. Each type has its own workflow."`
2. Ganti `empty_state.description` menjadi:
   `"Create Incident, Problem, Change, or Improvement types — each gets its own workflow."`
3. Hapus key `form.workflow`.
4. Tambahkan setelah key `no_workflow`:

```json
        "not_found": {
          "title": "Work item type not found",
          "description": "The work item type you are looking for does not exist or has been deleted."
        },
        "detail": {
          "epic_no_workflow": "Epic types don't have a workflow.",
          "no_workflow": "This type has no workflow yet."
        },
```

5. Pada objek `workflows` (baris 500-543), hapus key yang hanya dipakai komponen yang dihapus: `title`, `heading`, `description`, `add_workflow`, `delete_confirmation`, `empty_state`, `not_found`, `form.name`, `form.active`. **Pertahankan**: `no_states`, `states.*`, `no_transitions`, `transitions.*`, dan `form.description` (dipakai `state-form-modal.tsx`).

- [ ] **Step 3: Sinkronkan semua locale**

Terapkan perubahan yang sama ke 19 locale lain di `packages/i18n/src/locales/` (cs, de, es, fr, id, it, ja, ka-ge, ko, pl, pt-BR, ro, ru, sk, tr-TR, ua, vi-VN, zh-CN, zh-TW) mengikuti skill translate: hapus key mati, tambah key baru dengan terjemahan, pertahankan placeholder.

- [ ] **Step 4: Verifikasi sync**

Run: `pnpm --filter @plane/i18n run check:sync`

Expected: PASS (tidak ada missing/stale/collision key).

- [ ] **Step 5: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "i18n(web): strings for single service management page"
```

### Task 3: Refactor `WorkflowEditor` + komponen detail

**Files:**

- Modify: `apps/web/core/components/workflows/workflow-editor.tsx` (tulis ulang)
- Create: `apps/web/core/components/work-item-types/detail.tsx`
- Modify: `apps/web/core/components/work-item-types/index.ts`

- [ ] **Step 1: Tulis ulang `WorkflowEditor`**

Ganti seluruh isi `apps/web/core/components/workflows/workflow-editor.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// local imports
import { StateList } from "./state-list";
import { TransitionMatrix } from "./transition-matrix";
import { WorkflowLoadErrorState } from "./workflow-load-error-state";

type Props = {
  workspaceSlug: string;
  workflowId: string;
};

export const WorkflowEditor = observer(function WorkflowEditor(props: Props) {
  const { workspaceSlug, workflowId } = props;
  // states
  const [statesError, setStatesError] = useState(false);
  const [transitionsError, setTransitionsError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workflowStates, workflowTransitions, mapRefreshError, fetchWorkflowStates, fetchWorkflowTransitions } =
    useWorkflow();
  // projects whose failed map refresh already raised a toast; re-armed when the map recovers
  const warnedMapFailures = useRef(new Set<string>());
  // derived values
  const states = workflowStates[workflowId];
  const transitions = workflowTransitions[workflowId];
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");
  const mapRefreshWarningTitle = t("common.warning");
  const mapRefreshWarningMessage = t("workspace_settings.settings.work_item_types.map_refresh_failed");

  const loadStates = useCallback(() => {
    setStatesError(false);
    void fetchWorkflowStates(workspaceSlug, workflowId).catch((error: any) => {
      setStatesError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflowId, fetchWorkflowStates, fetchErrorTitle, fetchErrorMessage]);

  const loadTransitions = useCallback(() => {
    setTransitionsError(false);
    void fetchWorkflowTransitions(workspaceSlug, workflowId).catch((error: any) => {
      setTransitionsError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflowId, fetchWorkflowTransitions, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    loadStates();
    loadTransitions();
  }, [loadStates, loadTransitions]);

  // every editor mutation refreshes the affected project workflow maps; if that
  // refresh failed, board columns can be stale even though the edit landed
  useEffect(() => {
    Object.entries(mapRefreshError).forEach(([projectId, failure]) => {
      if (!failure) {
        warnedMapFailures.current.delete(projectId);
        return;
      }
      if (warnedMapFailures.current.has(projectId)) return;
      warnedMapFailures.current.add(projectId);
      setToast({
        type: TOAST_TYPE.WARNING,
        title: mapRefreshWarningTitle,
        message: mapRefreshWarningMessage,
      });
    });
  }, [mapRefreshError, mapRefreshWarningTitle, mapRefreshWarningMessage]);

  const handleRetryEditor = () => {
    loadStates();
    loadTransitions();
  };

  if (states === undefined && statesError) {
    return <WorkflowLoadErrorState onRetry={handleRetryEditor} />;
  }

  return (
    <div className="mt-6 grid grid-cols-1 gap-8 lg:grid-cols-2">
      <StateList
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        states={states}
        hasError={statesError}
        onRetry={loadStates}
      />
      <TransitionMatrix
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        states={states}
        transitions={transitions}
        hasError={statesError || transitionsError}
        onRetry={handleRetryEditor}
      />
    </div>
  );
});
```

- [ ] **Step 2: Buat komponen detail**

Buat `apps/web/core/components/work-item-types/detail.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
import { EditOutline } from "@makeplane/propel/icons";
import { Switch } from "@makeplane/propel/components/switch";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// assets
import emptyModule from "@/app/assets/empty-state/module.svg?url";
// ui
import { Spinner } from "@plane/ui";
// components
import { EmptyState } from "@/components/common/empty-state";
import { WorkflowEditor } from "@/components/workflows";
import { WorkflowLoadErrorState } from "@/components/workflows/workflow-load-error-state";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { TypeFormModal } from "./type-form-modal";

type Props = {
  workspaceSlug: string;
  typeId: string;
};

export const WorkItemTypeDetail = observer(function WorkItemTypeDetail(props: Props) {
  const { workspaceSlug, typeId } = props;
  // router
  const router = useAppRouter();
  // states
  const [hasFetchError, setHasFetchError] = useState(false);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [isActiveLoading, setIsActiveLoading] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes, fetchWorkItemTypes, updateWorkItemType } = useWorkflow();
  // derived values
  const type = workItemTypes?.find((item) => item.id === typeId);
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");

  const loadTypes = useCallback(() => {
    setHasFetchError(false);
    void fetchWorkItemTypes(workspaceSlug).catch((error: any) => {
      setHasFetchError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, fetchWorkItemTypes, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    if (workItemTypes === undefined) loadTypes();
  }, [workItemTypes, loadTypes]);

  const handleToggleActive = async (isActive: boolean) => {
    if (!type) return;
    setIsActiveLoading(true);
    try {
      await updateWorkItemType(workspaceSlug, type.id, { is_active: isActive });
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    } finally {
      setIsActiveLoading(false);
    }
  };

  if (workItemTypes === undefined && hasFetchError) {
    return <WorkflowLoadErrorState onRetry={loadTypes} />;
  }

  if (workItemTypes === undefined) {
    return (
      <div className="mt-6 flex h-40 items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (!type) {
    return (
      <div className="mt-6 flex h-80 items-center justify-center">
        <EmptyState
          image={emptyModule}
          title={t("workspace_settings.settings.work_item_types.not_found.title")}
          description={t("workspace_settings.settings.work_item_types.not_found.description")}
          primaryButton={{
            text: t("common.go_back"),
            onClick: () => router.push(`/${workspaceSlug}/settings/work-item-types`),
          }}
        />
      </div>
    );
  }

  return (
    <>
      <TypeFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        typeId={type.id}
        onClose={() => setIsFormOpen(false)}
      />
      <div className="mt-6 flex items-start justify-between gap-4 rounded-lg border border-subtle p-5">
        <div className="flex min-w-0 flex-col">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-16 font-medium text-primary">{type.name}</h3>
            {type.is_epic && (
              <span className="flex h-4 max-h-fit items-center rounded-xs bg-accent-primary/20 px-2 text-11 font-medium text-accent-primary">
                {t("common.epic")}
              </span>
            )}
          </div>
          {type.description && <p className="mt-1 text-13 text-secondary">{type.description}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-4">
          <div className="flex items-center gap-2">
            <span className="text-13 font-medium">{t("workspace_settings.settings.work_item_types.form.active")}</span>
            <Switch
              size="sm"
              checked={type.is_active}
              disabled={isActiveLoading}
              onCheckedChange={(value) => void handleToggleActive(value)}
              aria-label={t("workspace_settings.settings.work_item_types.form.active")}
            />
          </div>
          <Button variant="secondary" size="lg" onClick={() => setIsFormOpen(true)}>
            <EditOutline width={14} height={14} className="mr-1" />
            {t("common.edit")}
          </Button>
        </div>
      </div>
      {type.is_epic ? (
        <p className="mt-6 text-13 text-tertiary">
          {t("workspace_settings.settings.work_item_types.detail.epic_no_workflow")}
        </p>
      ) : type.workflow ? (
        <div className="mt-8">
          <h4 className="text-14 font-medium text-primary">{`${type.name} Workflow`}</h4>
          <WorkflowEditor workspaceSlug={workspaceSlug} workflowId={type.workflow} />
        </div>
      ) : (
        <p className="mt-6 text-13 text-tertiary">
          {t("workspace_settings.settings.work_item_types.detail.no_workflow")}
        </p>
      )}
    </>
  );
});
```

- [ ] **Step 3: Ekspor komponen**

Di `apps/web/core/components/work-item-types/index.ts`, tambahkan:

```ts
export * from "./detail";
```

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS (halaman lama masih memakai `WorkflowEditor` dengan props yang sama).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/workflows/workflow-editor.tsx \
  apps/web/core/components/work-item-types/detail.tsx apps/web/core/components/work-item-types/index.ts
git commit -m "feat(web): add work item type detail with inline workflow editor"
```

### Task 4: Route redirect + route detail + halaman detail

**Files:**

- Modify: `apps/web/app/routes/core.ts:295-306,392-432`
- Create: `apps/web/app/routes/redirects/core/workflows.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]/header.tsx`
- Delete: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/page.tsx`
- Delete: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/header.tsx`
- Delete: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/[workflowId]/page.tsx`
- Delete: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/[workflowId]/header.tsx`

- [ ] **Step 1: Tambah redirect route**

Buat `apps/web/app/routes/redirects/core/workflows.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/workflows";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const { workspaceSlug } = params;
  throw redirect(`/${workspaceSlug}/settings/work-item-types/`);
};

export default function Workflows() {
  return null;
}
```

- [ ] **Step 2: Ubah route config**

Di `apps/web/app/routes/core.ts`:

1. Hapus dua `route(...)` workflows di dalam layout workspace settings (baris 299-306).
2. Tambahkan route detail setelah route `:workspaceSlug/settings/work-item-types`:

```ts
          route(
            ":workspaceSlug/settings/work-item-types/:typeId",
            "./(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]/page.tsx"
          ),
```

3. Di blok REDIRECT ROUTES, setelah redirect api-tokens, tambahkan:

```ts
  // Workflows legacy redirect: /:workspaceSlug/settings/workflows/*
  // → /:workspaceSlug/settings/work-item-types/
  route(":workspaceSlug/settings/workflows/*", "routes/redirects/core/workflows.tsx"),
```

- [ ] **Step 3: Buat halaman detail**

Buat `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
// components
import { NotAuthorizedView } from "@/components/auth-screens/not-authorized-view";
import { PageHead } from "@/components/core/page-title";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
import { WorkItemTypeDetail } from "@/components/work-item-types";
// hooks
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import type { Route } from "./+types/page";
import { WorkItemTypeDetailWorkspaceSettingsHeader } from "./header";

function WorkItemTypeDetailSettingsPage({ params }: Route.ComponentProps) {
  // router
  const { workspaceSlug, typeId } = params;
  // plane hooks
  const { t } = useTranslation();
  // mobx store
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const canManageWorkItemTypes = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.work_item_types.title")}`
    : undefined;

  if (workspaceUserInfo && !canManageWorkItemTypes) {
    return <NotAuthorizedView section="settings" className="h-auto" />;
  }

  return (
    <SettingsContentWrapper
      header={<WorkItemTypeDetailWorkspaceSettingsHeader workspaceSlug={workspaceSlug} typeId={typeId} />}
    >
      <PageHead title={pageTitle} />
      {workspaceSlug && typeId && (
        <WorkItemTypeDetail workspaceSlug={workspaceSlug.toString()} typeId={typeId.toString()} />
      )}
    </SettingsContentWrapper>
  );
}

export default observer(WorkItemTypeDetailSettingsPage);
```

- [ ] **Step 4: Buat header detail**

Buat `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]/header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { WORKSPACE_SETTINGS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { SettingsPageHeader } from "@/components/settings/page-header";
import { WORKSPACE_SETTINGS_ICONS } from "@/components/settings/workspace/sidebar/item-icon";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  typeId: string;
};

export const WorkItemTypeDetailWorkspaceSettingsHeader = observer(function WorkItemTypeDetailWorkspaceSettingsHeader(
  props: Props
) {
  const { workspaceSlug, typeId } = props;
  // translation
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes } = useWorkflow();
  // derived values
  const settingsDetails = WORKSPACE_SETTINGS.work_item_types;
  const Icon = WORKSPACE_SETTINGS_ICONS.work_item_types;
  const type = workItemTypes?.find((item) => item.id === typeId);
  // keep the crumb informative instead of blank when types are loaded but the id does not exist
  const typeLabel = type
    ? type.name
    : workItemTypes !== undefined
      ? t("workspace_settings.settings.work_item_types.not_found.title")
      : undefined;

  return (
    <SettingsPageHeader
      leftItem={
        <div className="flex items-center gap-2">
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t(settingsDetails.i18n_label)}
                  icon={<Icon className="size-4 text-tertiary" />}
                  href={`/${workspaceSlug}/settings/work-item-types`}
                />
              }
            />
            {typeLabel && <Breadcrumbs.Item component={<BreadcrumbLink label={typeLabel} />} />}
          </Breadcrumbs>
        </div>
      }
    />
  );
});
```

- [ ] **Step 5: Hapus halaman workflows lama**

```bash
rm -r "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows"
```

- [ ] **Step 6: Typecheck**

Run: `pnpm check:types`

Expected: PASS. (Komponen `WorkflowsRoot`, `WorkflowList`, `WorkflowFormModal` masih ada tapi belum direferensikan — dihapus di Task 5.)

- [ ] **Step 7: Commit**

```bash
git add apps/web/app/routes/core.ts apps/web/app/routes/redirects/core/workflows.tsx \
  "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]" \
  "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows"
git commit -m "feat(web): route work item type detail and redirect workflows"
```

### Task 5: List + modal + hapus komponen workflows lama

**Files:**

- Modify: `apps/web/core/components/work-item-types/type-list-item.tsx`
- Modify: `apps/web/core/components/work-item-types/root.tsx`
- Modify: `apps/web/core/components/work-item-types/type-form-modal.tsx`
- Modify: `apps/web/core/components/workflows/index.ts`
- Delete: `apps/web/core/components/workflows/root.tsx`
- Delete: `apps/web/core/components/workflows/workflow-list.tsx`
- Delete: `apps/web/core/components/workflows/workflow-form-modal.tsx`

- [ ] **Step 1: Tulis ulang `TypeListItem` (link ke detail)**

Ganti seluruh isi `apps/web/core/components/work-item-types/type-list-item.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { DeleteOutline, EditOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TWorkItemType } from "@plane/types";
import { CustomMenu } from "@plane/ui";
import { cn } from "@plane/utils";

type Props = {
  workspaceSlug: string;
  type: TWorkItemType;
  onEdit: () => void;
  onDelete: () => void;
};

export const TypeListItem = observer(function TypeListItem(props: Props) {
  const { workspaceSlug, type, onEdit, onDelete } = props;
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const workflowLabel = type.workflow
    ? `${type.name} Workflow`
    : t("workspace_settings.settings.work_item_types.no_workflow");

  return (
    <div className="group flex items-center justify-between gap-4 px-4 py-3">
      <Link href={`/${workspaceSlug}/settings/work-item-types/${type.id}`} className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2">
          <p className="truncate text-13 font-medium text-primary">{type.name}</p>
          {type.is_epic && (
            <span className="flex h-4 max-h-fit items-center rounded-xs bg-accent-primary/20 px-2 text-11 font-medium text-accent-primary">
              {t("common.epic")}
            </span>
          )}
          <span
            className={cn("flex h-4 max-h-fit items-center rounded-xs px-2 text-11 font-medium", {
              "bg-success-subtle text-success-primary": type.is_active,
              "bg-layer-1 text-placeholder": !type.is_active,
            })}
          >
            {type.is_active ? t("common.active") : t("workspace_settings.settings.work_item_types.inactive")}
          </span>
        </div>
        {type.description && <p className="mt-0.5 truncate text-11 text-secondary">{type.description}</p>}
        <p className="mt-0.5 text-11 text-placeholder">{workflowLabel}</p>
      </Link>
      <CustomMenu ellipsis ariaLabel={t("aria_labels.projects_sidebar.toggle_quick_actions_menu")}>
        <CustomMenu.MenuItem onClick={onEdit}>
          <span className="flex items-center justify-start gap-2">
            <EditOutline width={14} height={14} />
            <span>{t("common.edit")}</span>
          </span>
        </CustomMenu.MenuItem>
        <CustomMenu.MenuItem onClick={onDelete}>
          <span className="flex items-center justify-start gap-2">
            <DeleteOutline width={14} height={14} />
            <span>{t("common.delete")}</span>
          </span>
        </CustomMenu.MenuItem>
      </CustomMenu>
    </div>
  );
});
```

- [ ] **Step 2: Tulis ulang `WorkItemTypesRoot`**

Ganti seluruh isi `apps/web/core/components/work-item-types/root.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkItemType } from "@plane/types";
// ui
import { Spinner } from "@plane/ui";
// components
import { SettingsHeading } from "@/components/settings/heading";
import { WorkflowLoadErrorState } from "@/components/workflows/workflow-load-error-state";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { DeleteTypeModal } from "./delete-type-modal";
import { TypeFormModal } from "./type-form-modal";
import { TypeListItem } from "./type-list-item";

type Props = {
  workspaceSlug: string;
};

export const WorkItemTypesRoot = observer(function WorkItemTypesRoot(props: Props) {
  const { workspaceSlug } = props;
  // router
  const router = useAppRouter();
  // states
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingTypeId, setEditingTypeId] = useState<string | null>(null);
  const [deletingTypeId, setDeletingTypeId] = useState<string | null>(null);
  const [hasFetchError, setHasFetchError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes, fetchWorkItemTypes } = useWorkflow();
  // derived values
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");

  const loadData = useCallback(() => {
    setHasFetchError(false);
    void fetchWorkItemTypes(workspaceSlug).catch((error: any) => {
      setHasFetchError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, fetchWorkItemTypes, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleCreated = (type: TWorkItemType) => {
    router.push(`/${workspaceSlug}/settings/work-item-types/${type.id}`);
  };

  return (
    <>
      <TypeFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        typeId={editingTypeId}
        onClose={() => {
          setIsFormOpen(false);
          setEditingTypeId(null);
        }}
        onSuccess={handleCreated}
      />
      <DeleteTypeModal
        workspaceSlug={workspaceSlug}
        isOpen={Boolean(deletingTypeId)}
        typeId={deletingTypeId}
        onClose={() => setDeletingTypeId(null)}
      />
      <SettingsHeading
        title={t("workspace_settings.settings.work_item_types.heading")}
        description={t("workspace_settings.settings.work_item_types.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditingTypeId(null);
              setIsFormOpen(true);
            }}
          >
            {t("workspace_settings.settings.work_item_types.add_type")}
          </Button>
        }
      />
      {hasFetchError && workItemTypes === undefined ? (
        <div className="mt-6">
          <WorkflowLoadErrorState onRetry={loadData} />
        </div>
      ) : workItemTypes === undefined ? (
        <div className="mt-6 flex h-40 items-center justify-center">
          <Spinner />
        </div>
      ) : workItemTypes.length === 0 ? (
        <div className="mt-6 flex flex-col items-center justify-center gap-1 rounded-lg border border-subtle py-16 text-center">
          <p className="text-13 font-medium text-primary">
            {t("workspace_settings.settings.work_item_types.empty_state.title")}
          </p>
          <p className="text-11 text-tertiary">
            {t("workspace_settings.settings.work_item_types.empty_state.description")}
          </p>
        </div>
      ) : (
        <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
          {workItemTypes.map((type) => (
            <TypeListItem
              key={type.id}
              workspaceSlug={workspaceSlug}
              type={type}
              onEdit={() => {
                setEditingTypeId(type.id);
                setIsFormOpen(true);
              }}
              onDelete={() => setDeletingTypeId(type.id)}
            />
          ))}
        </div>
      )}
    </>
  );
});
```

- [ ] **Step 3: Update `TypeFormModal`**

Di `apps/web/core/components/work-item-types/type-form-modal.tsx`:

1. Import: hapus `CustomSelect` dari import `@plane/ui` (baris 19); tambahkan `TWorkItemType` ke import type dari `@plane/types`.
2. Ganti `type Props` menjadi:

```tsx
type Props = {
  workspaceSlug: string;
  isOpen: boolean;
  typeId: string | null;
  onClose: () => void;
  onSuccess?: (type: TWorkItemType) => void;
};
```

3. Ganti `defaultValues` menjadi:

```tsx
const defaultValues: TWorkItemTypePayload = {
  name: "",
  description: "",
  is_active: true,
};
```

4. Destructure props: `const { workspaceSlug, isOpen, typeId, onClose, onSuccess } = props;`.
5. Destructure store: `const { workItemTypes, createWorkItemType, updateWorkItemType } = useWorkflow();`.
6. Di `useEffect` reset untuk edit, hapus baris `workflow: type.workflow,`.
7. Ganti `onSubmit` menjadi:

```tsx
const onSubmit = async (formData: TWorkItemTypePayload) => {
  const isEdit = Boolean(typeId);

  try {
    if (typeId) {
      await updateWorkItemType(workspaceSlug, typeId, formData);
    } else {
      const created = await createWorkItemType(workspaceSlug, formData);
      onSuccess?.(created);
    }

    setToast({
      type: TOAST_TYPE.SUCCESS,
      title: isEdit ? t("work_item_types.update.toast.success.title") : t("work_item_types.create.toast.success.title"),
      message: isEdit
        ? t("work_item_types.update.toast.success.message", { name: formData.name ?? "" })
        : t("work_item_types.create.toast.success.message"),
    });
    handleClose();
  } catch (error: any) {
    setToast({
      type: TOAST_TYPE.ERROR,
      title: isEdit ? t("work_item_types.update.toast.error.title") : t("work_item_types.create.toast.error.title"),
      message:
        error?.error ??
        (isEdit
          ? t("work_item_types.update.toast.error.message.default")
          : t("work_item_types.create.toast.error.message.default")),
    });
  }
};
```

8. Ganti blok JSX dari `<div className="flex items-center justify-between gap-2">` sampai `</div>` penutupnya (baris 156-208, berisi select workflow + switch active) dengan:

```tsx
<div className="flex items-center gap-2">
  <span className="text-13 font-medium">{t("workspace_settings.settings.work_item_types.form.active")}</span>
  <Controller
    control={control}
    name="is_active"
    render={({ field: { value, onChange } }) => (
      <Switch
        size="sm"
        checked={value ?? false}
        onCheckedChange={onChange}
        aria-label={t("workspace_settings.settings.work_item_types.form.active")}
      />
    )}
  />
</div>
```

- [ ] **Step 4: Hapus komponen workflows lama + update barrel**

```bash
rm apps/web/core/components/workflows/root.tsx \
  apps/web/core/components/workflows/workflow-list.tsx \
  apps/web/core/components/workflows/workflow-form-modal.tsx
```

Ganti isi `apps/web/core/components/workflows/index.ts` dengan:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./workflow-editor";
```

- [ ] **Step 5: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/work-item-types/type-list-item.tsx \
  apps/web/core/components/work-item-types/root.tsx \
  apps/web/core/components/work-item-types/type-form-modal.tsx \
  apps/web/core/components/workflows/index.ts \
  apps/web/core/components/workflows/root.tsx \
  apps/web/core/components/workflows/workflow-list.tsx \
  apps/web/core/components/workflows/workflow-form-modal.tsx
git commit -m "feat(web): single service management list and type form"
```

### Task 6: Payload type + service/store cleanup

**Files:**

- Modify: `packages/types/src/workflow/work-item-type.ts:25-31`
- Modify: `apps/web/core/services/workflow/workflow.service.ts:9-18,35-57`
- Modify: `apps/web/core/store/workflow.store.ts:10-19,25-69,91-119,130-155`

- [ ] **Step 1: Buang `workflow` dari payload type**

Di `packages/types/src/workflow/work-item-type.ts`, ganti `TWorkItemTypePayload` menjadi:

```ts
export type TWorkItemTypePayload = {
  name?: string;
  description?: string;
  is_active?: boolean;
  project_ids?: string[];
};
```

`TWorkItemType.workflow` (response) tetap.

- [ ] **Step 2: Buang CRUD workflow dari service**

Di `apps/web/core/services/workflow/workflow.service.ts`, hapus method `createWorkflow` (baris 35-41), `updateWorkflow` (43-49), dan `deleteWorkflow` (51-57). Hapus juga `TWorkflowPayload` dari daftar import type. `getWorkflows` tetap (dipakai project settings).

- [ ] **Step 3: Buang CRUD workflow dari store**

Di `apps/web/core/store/workflow.store.ts`:

1. Hapus tiga deklarasi di interface `IWorkflowStore` (baris 41-43).
2. Hapus tiga method di class (baris 130-155).
3. Hapus tiga entri `createWorkflow/updateWorkflow/deleteWorkflow: action,` di `makeObservable` (baris 106-108).
4. Hapus `TWorkflowPayload` dari import type.

- [ ] **Step 4: Jalankan test + typecheck**

Run:

```bash
pnpm --filter=web exec vitest run core/store/workflow.store.test.ts
pnpm check:types
```

Expected: PASS keduanya.

- [ ] **Step 5: Commit**

```bash
git add packages/types/src/workflow/work-item-type.ts \
  apps/web/core/services/workflow/workflow.service.ts apps/web/core/store/workflow.store.ts
git commit -m "refactor(web): remove standalone workflow CRUD"
```

### Task 7: Verifikasi + build + deploy lokal

**Files:** —

- [ ] **Step 1: Jalankan seluruh test web terkait**

Run:

```bash
pnpm --filter=web exec vitest run core/store/workflow.store.test.ts core/store/workflow.helpers.test.ts
pnpm --filter @plane/i18n run check:sync
pnpm check:types
pnpm check:lint
```

Expected: PASS semua.

- [ ] **Step 2: Build web**

Ikuti `AGENTS.md` (prod build memakai `VITE_API_BASE_URL=https://api.terraline.space`; jangan pakai dev server untuk verifikasi tunnel):

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

- [ ] **Step 3: Smoke manual di browser/curl**

1. `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000` → 200.
2. Buka `/<workspace>/settings/work-item-types` → hanya satu entri "Work item types" di kategori Service management; list tampil dengan label `${type.name} Workflow`.
3. Buka detail salah satu type → kartu type + editor state/transisi tampil; rename type lalu cek workflow ikut berubah (via halaman detail).
4. Buka `/<workspace>/settings/workflows` → redirect ke `/settings/work-item-types/`.
5. Pastikan tidak ada 404/error di console untuk halaman baru.

- [ ] **Step 4: Commit sisa (bila ada perbaikan dari smoke)**

```bash
git add <file-yang-diperbaiki>
git commit -m "fix(web): address single service management page smoke findings"
```

### Catatan akhir

- Backend harus lebih dulu ter-deploy (migrasi `0125` + rebuild api-rs) supaya type `Request` punya workflow dan create type tidak lagi mengirim `workflow`.
- Halaman project settings "Work item types" (`project-work-item-types/root.tsx`) tidak berubah dan tetap memakai `fetchWorkflows`; hanya label workflow di sana nanti mengikuti nama stored (`{Type} Workflow`) karena backend men-sync rename.
