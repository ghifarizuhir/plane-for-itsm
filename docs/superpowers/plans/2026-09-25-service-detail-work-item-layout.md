# Service Detail Work-Item Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the service detail page from a 3-tab layout into the work-item detail pattern: scrollable main content on the left, sticky properties sidebar on the right, with inline property editing.

**Architecture:** Mirror `IssueDetailRoot` (`apps/web/core/components/issues/issue-detail/root.tsx:241-264`): two columns, left = title + description + linked work items, right = `SidebarPropertyListItem` rows (health, status, criticality, type, owner, repository, documentation) plus dependencies. Tabs are deleted; the Edit/Delete actions move to a `CustomMenu` in the breadcrumb header (like `IssueDetailQuickActions`). All existing i18n keys are reused — no locale JSON changes, so `pnpm --filter=@plane/i18n check:sync` stays green.

**Tech Stack:** React Router v7 + MobX (observer) + `@plane/ui` (CustomSelect, CustomMenu, TextArea, Input) + `@makeplane/propel/icons` + Vitest 4.

**Decisions locked with the user:**

1. Work item 2-column layout (tabs removed).
2. Properties edited inline in the sidebar.
3. Left column = description + linked work items.
4. Title inline-editable in the left column; Edit/Delete in a header "..." menu.
5. Health / last deploy / active incidents shown in the sidebar (reuse list-view components).

---

## File Structure

Create:

- `apps/web/core/components/services/detail/property-select.tsx` — `ServicePropertySelect`, inline `CustomSelect` for status/criticality/type.
- `apps/web/core/components/services/detail/url-property.tsx` — `ServiceUrlProperty`, click-to-edit URL row for repository/documentation.
- `apps/web/core/components/services/detail/sidebar.tsx` — `ServiceDetailSidebar`, the right column.
- `apps/web/core/components/services/detail/title-input.tsx` — `ServiceTitleInput`, inline title editor.
- `apps/web/core/components/services/detail/description.tsx` — `ServiceDescription` (description only, replaces `overview.tsx`).
- `apps/web/core/components/services/detail/quick-actions.tsx` — `ServiceDetailQuickActions`, header "..." menu + edit/delete modals.
- `apps/web/core/services/service.helpers.test.ts` — unit test for `normalizeServiceUrl`.

Modify:

- `apps/web/core/services/service.helpers.ts` — add `normalizeServiceUrl`.
- `apps/web/core/components/services/detail/dependencies.tsx` — compact sidebar section (no `max-w-3xl`, stacked lists).
- `apps/web/core/components/services/detail/work-items.tsx` — full-width section with heading.
- `apps/web/core/components/services/detail/root.tsx` — 2-column layout, owns `isSubmitting`.
- `apps/web/core/components/services/detail/index.ts` — update exports.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/layout.tsx` — render `ServiceDetailQuickActions` in `Header.RightItem`.

Delete:

- `apps/web/core/components/services/detail/header.tsx`
- `apps/web/core/components/services/detail/tabs.tsx`
- `apps/web/core/components/services/detail/overview.tsx` (replaced by `description.tsx`)

Reference (read-only):

- `apps/web/core/components/issues/issue-detail/root.tsx:241-264` — 2-column shell.
- `apps/web/core/components/issues/issue-detail/sidebar.tsx:82-131` — `SidebarPropertyListItem` + dropdown row styling.
- `apps/web/core/components/common/layout/sidebar/property-list-item.tsx` — row primitive.
- `apps/web/core/components/services/service-form.tsx:39-91` — option arrays + i18n prefixes.
- `apps/web/core/components/services/health/service-health-pill.tsx`, `health/service-deploy-cell.tsx` — sidebar health cells.

---

### Task 1: `normalizeServiceUrl` helper (TDD)

**Files:**

- Create: `apps/web/core/services/service.helpers.test.ts`
- Modify: `apps/web/core/services/service.helpers.ts` (append at end)

- [ ] **Step 1: Write the failing test**

```ts
// apps/web/core/services/service.helpers.test.ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import { normalizeServiceUrl } from "./service.helpers";

describe("normalizeServiceUrl", () => {
  it("returns null for empty and whitespace-only values", () => {
    expect(normalizeServiceUrl("")).toBeNull();
    expect(normalizeServiceUrl("   ")).toBeNull();
  });

  it("trims surrounding whitespace", () => {
    expect(normalizeServiceUrl("  https://github.com/plane/plane  ")).toBe("https://github.com/plane/plane");
  });

  it("keeps a normal URL unchanged", () => {
    expect(normalizeServiceUrl("https://docs.plane.so")).toBe("https://docs.plane.so");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/services/service.helpers.test.ts`

Expected: FAIL — `normalizeServiceUrl` is not exported (`TypeError: normalizeServiceUrl is not a function` or an import error).

- [ ] **Step 3: Write minimal implementation**

Append to `apps/web/core/services/service.helpers.ts`:

```ts
/**
 * Trims a service link field and collapses blank input to null so the API
 * clears the column instead of storing whitespace.
 */
export const normalizeServiceUrl = (value: string): string | null => {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/services/service.helpers.test.ts`

Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/service.helpers.ts apps/web/core/services/service.helpers.test.ts
git commit -m "feat(web): add normalizeServiceUrl helper with tests"
```

---

### Task 2: Sidebar primitives — `ServicePropertySelect` + `ServiceUrlProperty`

**Files:**

- Create: `apps/web/core/components/services/detail/property-select.tsx`
- Create: `apps/web/core/components/services/detail/url-property.tsx`

- [ ] **Step 1: Create `property-select.tsx`**

```tsx
// apps/web/core/components/services/detail/property-select.tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ChevronDownOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
// ui
import { CustomSelect } from "@plane/ui";

type Props<T extends string> = {
  value: T;
  options: readonly T[];
  i18nPrefix: string;
  onChange: (value: T) => void;
  disabled?: boolean;
};

export function ServicePropertySelect<T extends string>(props: Props<T>) {
  const { value, options, i18nPrefix, onChange, disabled = false } = props;
  // plane hooks
  const { t } = useTranslation();

  return (
    <CustomSelect
      value={value}
      onChange={(val: T) => onChange(val)}
      disabled={disabled}
      placement="bottom-start"
      customButtonClassName="group h-7.5 w-full grow px-2 text-left"
      customButton={
        <span className="flex w-full items-center justify-between gap-1">
          <span className="truncate text-body-xs-regular capitalize">{t(`${i18nPrefix}.${value}`)}</span>
          {!disabled && <ChevronDownOutline className="hidden h-3.5 w-3.5 shrink-0 group-hover:inline" />}
        </span>
      }
    >
      {options.map((option) => (
        <CustomSelect.Option key={option} value={option}>
          <span className="capitalize">{t(`${i18nPrefix}.${option}`)}</span>
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
}
```

- [ ] **Step 2: Create `url-property.tsx`**

```tsx
// apps/web/core/components/services/detail/url-property.tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
// icons
import { EditOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
// ui
import { Input } from "@plane/ui";
// helpers
import { normalizeServiceUrl } from "@/services/service.helpers";

type Props = {
  value: string | null;
  onSubmit: (value: string | null) => void;
};

export function ServiceUrlProperty(props: Props) {
  const { value, onSubmit } = props;
  // states
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(value ?? "");
  // plane hooks
  const { t } = useTranslation();

  useEffect(() => {
    setDraft(value ?? "");
  }, [value]);

  const commit = () => {
    const next = normalizeServiceUrl(draft);
    setIsEditing(false);
    if (next !== value) onSubmit(next);
  };

  const cancel = () => {
    setDraft(value ?? "");
    setIsEditing(false);
  };

  if (isEditing) {
    return (
      <Input
        autoFocus
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") cancel();
        }}
        placeholder="https://"
        className="h-7.5 w-full grow rounded-sm px-2 text-body-xs-regular"
      />
    );
  }

  if (!value) {
    return (
      <button
        type="button"
        onClick={() => setIsEditing(true)}
        className="h-7.5 rounded-sm px-2 text-body-xs-regular text-placeholder hover:bg-layer-1"
      >
        {t("add")}
      </button>
    );
  }

  return (
    <div className="group flex w-full items-center gap-1">
      <a
        href={value}
        target="_blank"
        rel="noopener noreferrer"
        className="flex h-7.5 min-w-0 grow items-center truncate rounded-sm px-2 text-body-xs-regular text-primary hover:bg-layer-1"
        title={value}
      >
        {value}
      </a>
      <button
        type="button"
        onClick={() => setIsEditing(true)}
        className="hidden shrink-0 rounded-sm p-1 text-tertiary hover:text-primary group-hover:block"
        aria-label={t("edit")}
      >
        <EditOutline className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
```

- [ ] **Step 3: Typecheck the new files**

Run: `pnpm --filter=web check:types 2>&1 | tail -n 20`

Expected: no errors mentioning `property-select.tsx` or `url-property.tsx`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/detail/property-select.tsx apps/web/core/components/services/detail/url-property.tsx
git commit -m "feat(web): add service sidebar property primitives"
```

---

### Task 3: Compact dependencies + sidebar

**Files:**

- Modify: `apps/web/core/components/services/detail/dependencies.tsx` (replace render output)
- Create: `apps/web/core/components/services/detail/sidebar.tsx`

- [ ] **Step 1: Rewrite the `ServiceDependencies` render output for the sidebar**

Replace the `return (...)` block at the end of `apps/web/core/components/services/detail/dependencies.tsx` (currently lines 107-152) with:

```tsx
return (
  <div className="mt-3 flex flex-col gap-3">
    <div className="flex items-center gap-2">
      <CustomSelect
        value={selectedId}
        onChange={(val: string) => setSelectedId(val)}
        className="h-7.5 min-w-0 grow basis-0"
        label={
          <span className="flex items-center gap-2 py-0.5 text-12">
            {selectedService ? (
              <span className="truncate">{selectedService.name}</span>
            ) : (
              <span className="text-secondary">{t("service.detail.select_service")}</span>
            )}
          </span>
        }
      >
        {candidates.map((s) => (
          <CustomSelect.Option key={s.id} value={s.id}>
            <span className="truncate">{s.name}</span>
          </CustomSelect.Option>
        ))}
      </CustomSelect>
      <Button variant="secondary" size="sm" onClick={handleAdd} disabled={!selectedId} loading={isAdding}>
        {t("add")}
      </Button>
    </div>
    <div className="flex flex-col gap-1.5">
      <p className="text-12 font-medium text-tertiary">{t("service.detail.depends_on")}</p>
      {outgoing.length === 0 ? (
        <p className="text-12 text-tertiary">{t("service.detail.no_dependencies")}</p>
      ) : (
        outgoing.map((d) => renderEdgeRow(d.id, d.to_service_id))
      )}
    </div>
    <div className="flex flex-col gap-1.5">
      <p className="text-12 font-medium text-tertiary">{t("service.detail.depended_on_by")}</p>
      {incoming.length === 0 ? (
        <p className="text-12 text-tertiary">{t("service.detail.no_dependents")}</p>
      ) : (
        incoming.map((d) => renderEdgeRow(d.id, d.from_service_id))
      )}
    </div>
  </div>
);
```

Then update `renderEdgeRow` (currently lines 86-105) to the compact row:

```tsx
const renderEdgeRow = (dependencyId: string, otherServiceId: string) => {
  const other = getServiceById(otherServiceId);
  return (
    <div
      key={dependencyId}
      className="flex items-center justify-between gap-2 rounded-sm border border-subtle px-2 py-1.5"
    >
      <span className="truncate text-12 text-primary" title={other?.name ?? otherServiceId}>
        {other?.name ?? otherServiceId}
      </span>
      <button
        type="button"
        onClick={() => handleRemove(dependencyId)}
        className="shrink-0 text-11 text-tertiary hover:text-primary"
      >
        {t("remove")}
      </button>
    </div>
  );
};
```

- [ ] **Step 2: Create `sidebar.tsx`**

```tsx
// apps/web/core/components/services/detail/sidebar.tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import {
  ActivityOutline,
  ClockOutline,
  DocumentationOutline,
  FlagOutline,
  LinkOutline,
  ModuleOutline,
  StateOutline,
  UserOutline,
} from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
// components
import { SidebarPropertyListItem } from "@/components/common/layout/sidebar/property-list-item";
import { MemberDropdown } from "@/components/dropdowns/member/dropdown";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import { ServiceDeployCell } from "../health/service-deploy-cell";
import { ServiceHealthPill } from "../health/service-health-pill";
import { ServiceDependencies } from "./dependencies";
import { ServicePropertySelect } from "./property-select";
import { ServiceUrlProperty } from "./url-property";

const SERVICE_STATUS_OPTIONS: IService["status"][] = ["active", "planned", "maintenance", "deprecated", "retired"];
const SERVICE_CRITICALITY_OPTIONS: IService["criticality"][] = ["critical", "high", "medium", "low"];
const SERVICE_TYPE_OPTIONS: IService["type"][] = ["internal", "external", "infrastructure", "third_party"];

type Props = {
  serviceId: string;
};

export const ServiceDetailSidebar = observer(function ServiceDetailSidebar(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getServiceHealth, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString() ?? "";
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = getWorkspaceBySlug(slug)?.id;
  const health = getServiceHealth(serviceId);

  const update = async (data: Partial<IService>) => {
    if (!slug || !workspaceId) return;
    try {
      await updateService(slug, workspaceId, pid, serviceId, data);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: t("entity.update.failed", { entity: t("service.title") }),
      });
    }
  };

  return (
    <div className="w-full px-6 md:h-full md:overflow-y-auto">
      <h5 className="mt-5 text-body-xs-medium">{t("common.properties")}</h5>
      <div className="mt-4 mb-2 space-y-2.5 truncate">
        <SidebarPropertyListItem icon={ActivityOutline} label={t("service.fields.health")}>
          <div className="flex flex-wrap items-center gap-2 px-2">
            <ServiceHealthPill health={health?.health} />
            {(health?.incidents.length ?? 0) > 0 && (
              <span className="text-11 font-medium text-danger-primary">{t("service.incidents.active")}</span>
            )}
          </div>
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={ClockOutline} label={t("service.fields.deploy")}>
          <div className="flex items-center px-2">
            <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
          </div>
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={StateOutline} label={t("service.fields.status")}>
          <ServicePropertySelect
            value={service.status}
            options={SERVICE_STATUS_OPTIONS}
            i18nPrefix="service.status_values"
            onChange={(val) => update({ status: val })}
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={FlagOutline} label={t("service.fields.criticality")}>
          <ServicePropertySelect
            value={service.criticality}
            options={SERVICE_CRITICALITY_OPTIONS}
            i18nPrefix="service.criticality_values"
            onChange={(val) => update({ criticality: val })}
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={ModuleOutline} label={t("service.fields.type")}>
          <ServicePropertySelect
            value={service.type}
            options={SERVICE_TYPE_OPTIONS}
            i18nPrefix="service.type_values"
            onChange={(val) => update({ type: val })}
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={UserOutline} label={t("service.fields.owner")}>
          <MemberDropdown
            value={service.owner_id}
            onChange={(val) => update({ owner_id: val })}
            projectId={pid}
            multiple={false}
            placeholder={t("service.fields.owner")}
            buttonVariant="transparent-with-text"
            className="group w-full grow"
            buttonContainerClassName="w-full text-left h-7.5"
            buttonClassName="text-body-xs-regular"
            dropdownArrow
            dropdownArrowClassName="h-3.5 w-3.5 hidden group-hover:inline"
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={LinkOutline} label={t("service.fields.repository")}>
          <ServiceUrlProperty value={service.repository_url} onSubmit={(val) => update({ repository_url: val })} />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={DocumentationOutline} label={t("service.fields.documentation")}>
          <ServiceUrlProperty
            value={service.documentation_url}
            onSubmit={(val) => update({ documentation_url: val })}
          />
        </SidebarPropertyListItem>
      </div>

      <div className="mt-5 border-t border-subtle pt-4 pb-5">
        <h5 className="text-body-xs-medium">{t("common.dependencies")}</h5>
        <ServiceDependencies serviceId={serviceId} />
      </div>
    </div>
  );
});
```

- [ ] **Step 3: Typecheck**

Run: `pnpm --filter=web check:types 2>&1 | tail -n 20`

Expected: no errors in `dependencies.tsx` or `sidebar.tsx`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/detail/dependencies.tsx apps/web/core/components/services/detail/sidebar.tsx
git commit -m "feat(web): service detail properties sidebar"
```

---

### Task 4: Inline title + description-only content

**Files:**

- Create: `apps/web/core/components/services/detail/title-input.tsx`
- Create: `apps/web/core/components/services/detail/description.tsx`
- Delete: `apps/web/core/components/services/detail/overview.tsx`

- [ ] **Step 1: Create `title-input.tsx`**

```tsx
// apps/web/core/components/services/detail/title-input.tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
import { TextArea } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
  isSubmitting: TNameDescriptionLoader;
  setIsSubmitting: (value: TNameDescriptionLoader) => void;
};

export const ServiceTitleInput = observer(function ServiceTitleInput(props: Props) {
  const { serviceId, isSubmitting, setIsSubmitting } = props;
  // states
  const [title, setTitle] = useState("");
  const [isLengthVisible, setIsLengthVisible] = useState(false);
  // refs
  const lastSaved = useRef("");
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  const slug = workspaceSlug?.toString() ?? "";
  const pid = projectId?.toString() ?? service?.project_id ?? "";
  const workspaceId = getWorkspaceBySlug(slug)?.id;

  useEffect(() => {
    if (!service) return;
    setTitle(service.name);
    lastSaved.current = service.name;
  }, [service]);

  if (!service) return null;

  const commit = async () => {
    const trimmed = title.trim();
    if (trimmed.length === 0) {
      setTitle(lastSaved.current);
      setIsSubmitting("saved");
      return;
    }
    if (trimmed === lastSaved.current) {
      setIsSubmitting("saved");
      return;
    }
    if (!slug || !workspaceId) return;
    setTitle(trimmed);
    setIsSubmitting("submitting");
    try {
      await updateService(slug, workspaceId, pid, serviceId, { name: trimmed });
      lastSaved.current = trimmed;
      setIsSubmitting("submitted");
    } catch {
      setTitle(lastSaved.current);
      setIsSubmitting("saved");
    }
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className="relative -ml-3">
        <TextArea
          id="service-title-input"
          className={cn(
            "block w-full resize-none overflow-hidden rounded-sm border-none bg-transparent px-3 py-0 text-20 font-medium ring-0 outline-none",
            { "mx-2.5 ring-1 ring-danger-strong": title.length === 0 }
          )}
          value={title}
          onChange={(e) => {
            setIsSubmitting("submitting");
            setTitle(e.target.value);
          }}
          onBlur={() => {
            setIsLengthVisible(false);
            void commit();
          }}
          maxLength={255}
          placeholder={t("service.fields.name")}
          onFocus={() => setIsLengthVisible(true)}
        />
        <div
          className={cn(
            "pointer-events-none absolute right-1 bottom-1 z-[2] rounded-sm bg-surface-1 p-0.5 text-11 text-secondary opacity-0 transition-opacity",
            { "opacity-100": isLengthVisible }
          )}
        >
          <span className={`${title.length === 0 || title.length > 255 ? "text-danger-primary" : ""}`}>
            {title.length}
          </span>
          /255
        </div>
      </div>
      {title.length === 0 && (
        <span className="text-13 font-medium text-danger-primary">{t("form.title.required")}</span>
      )}
    </div>
  );
});
```

- [ ] **Step 2: Create `description.tsx`**

```tsx
// apps/web/core/components/services/detail/description.tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
import { EFileAssetType } from "@plane/types";
// components
import { DescriptionInput } from "@/components/editor/rich-text/description-input";
import { NameDescriptionUpdateStatus } from "@/components/issues/issue-update-status";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS, stripHtmlToText } from "@/services/service.helpers";

type Props = {
  serviceId: string;
  isSubmitting: TNameDescriptionLoader;
  setIsSubmitting: (value: TNameDescriptionLoader) => void;
};

export const ServiceDescription = observer(function ServiceDescription(props: Props) {
  const { serviceId, isSubmitting, setIsSubmitting } = props;
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString() ?? "";
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = getWorkspaceBySlug(slug)?.id;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-4">
        <h4 className="text-13 font-medium text-secondary">{t("service.fields.description")}</h4>
        <NameDescriptionUpdateStatus isSubmitting={isSubmitting} />
      </div>
      {slug && workspaceId && (
        <DescriptionInput
          containerClassName="-ml-6 border-none p-0! pl-6!"
          entityId={service.id}
          fileAssetType={EFileAssetType.PROJECT_DESCRIPTION}
          initialValue={service.description_html || "<p></p>"}
          key={service.id}
          disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
          onSubmit={async (value) => {
            const descriptionHtml =
              value.description_html && value.description_html.trim() !== "" ? value.description_html : "<p></p>";
            await updateService(slug, workspaceId, pid, service.id, {
              description: stripHtmlToText(descriptionHtml),
              description_html: descriptionHtml,
            });
          }}
          projectId={pid}
          setIsSubmitting={(value) => setIsSubmitting(value)}
          workspaceSlug={slug}
          placeholder={t("service.fields.description")}
        />
      )}
    </div>
  );
});
```

- [ ] **Step 3: Delete `overview.tsx`**

```bash
git rm apps/web/core/components/services/detail/overview.tsx
```

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/detail/title-input.tsx apps/web/core/components/services/detail/description.tsx
git commit -m "feat(web): service detail inline title and description-only content"
```

---

### Task 5: Work items section

**Files:**

- Modify: `apps/web/core/components/services/detail/work-items.tsx:81-121`

- [ ] **Step 1: Replace the render output**

Replace the `return (...)` block in `apps/web/core/components/services/detail/work-items.tsx` with:

```tsx
return (
  <div className="flex flex-col gap-3">
    <div className="flex items-center justify-between gap-2">
      <h4 className="text-13 font-medium text-secondary">{t("service.tabs.work_items")}</h4>
      <Button
        variant="secondary"
        size="sm"
        onClick={() => setIsPickerOpen(true)}
        disabled={!slug || !pid || !workspaceId}
      >
        {t("service.detail.add_work_items")}
      </Button>
    </div>
    {links.length === 0 ? (
      <p className="text-13 text-tertiary">{t("service.detail.no_work_items")}</p>
    ) : (
      <div className="flex flex-col gap-1.5">
        {links.map((link) => (
          <div
            key={link.id}
            className="flex items-center justify-between gap-2 rounded-md border border-subtle px-3 py-2"
          >
            <div className="flex min-w-0 flex-col">
              <span className="text-13 font-medium text-primary">{link.issue_identifier ?? link.issue_id}</span>
              <span className="truncate text-12 text-secondary" title={link.issue_name ?? t("service.detail.untitled")}>
                {link.issue_name ?? t("service.detail.untitled")}
              </span>
            </div>
            <button
              type="button"
              onClick={() => handleUnlink(link.id)}
              className="shrink-0 text-12 text-tertiary hover:text-primary"
            >
              {t("remove")}
            </button>
          </div>
        ))}
      </div>
    )}
    <ExistingIssuesListModal
      isOpen={isPickerOpen}
      handleClose={() => setIsPickerOpen(false)}
      workspaceSlug={slug}
      projectId={pid}
      searchParams={{ workspace_search: false }}
      shouldHideIssue={(issue) => linkedIssueIds.has(issue.id)}
      handleOnSubmit={handleAddWorkItems}
    />
  </div>
);
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/core/components/services/detail/work-items.tsx
git commit -m "refactor(web): work items section fills service detail column"
```

---

### Task 6: Header quick actions

**Files:**

- Create: `apps/web/core/components/services/detail/quick-actions.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/layout.tsx:1-68`

- [ ] **Step 1: Create `quick-actions.tsx`**

```tsx
// apps/web/core/components/services/detail/quick-actions.tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// ui
import { CustomMenu } from "@plane/ui";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { DeleteServiceModal } from "../delete-service-modal";
import { CreateUpdateServiceModal } from "../modal";

type Props = {
  serviceId: string;
};

export const ServiceDetailQuickActions = observer(function ServiceDetailQuickActions(props: Props) {
  const { serviceId } = props;
  // states
  const [isEditModalOpen, setIsEditModalOpen] = useState(false);
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById } = useService();
  // derived values
  const service = getServiceById(serviceId);

  if (!service || !workspaceSlug || !projectId) return <></>;
  const slug = workspaceSlug.toString();
  const pid = projectId.toString();

  return (
    <>
      <CustomMenu
        ellipsis
        placement="bottom-end"
        closeOnSelect
        ariaLabel={t("aria_labels.projects_sidebar.toggle_quick_actions_menu")}
      >
        <CustomMenu.MenuItem onClick={() => setIsEditModalOpen(true)}>{t("edit")}</CustomMenu.MenuItem>
        <CustomMenu.MenuItem onClick={() => setIsDeleteModalOpen(true)}>{t("delete")}</CustomMenu.MenuItem>
      </CustomMenu>
      <CreateUpdateServiceModal
        isOpen={isEditModalOpen}
        onClose={() => setIsEditModalOpen(false)}
        data={service}
        workspaceSlug={slug}
        projectId={pid}
      />
      <DeleteServiceModal data={service} isOpen={isDeleteModalOpen} onClose={() => setIsDeleteModalOpen(false)} />
    </>
  );
});
```

- [ ] **Step 2: Render it in the detail route header**

In `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/layout.tsx`:

Add the import after the existing hook imports (keep the existing `useAppRouter` import):

```tsx
// components
import { ServiceDetailQuickActions } from "@/components/services";
```

Then replace the empty `Header.RightItem` inside `ServiceDetailBreadcrumbs`:

```tsx
<Header.RightItem>
  <></>
</Header.RightItem>
```

with:

```tsx
<Header.RightItem>
  <ServiceDetailQuickActions serviceId={serviceId?.toString() ?? ""} />
</Header.RightItem>
```

`serviceId` is already read inside `ServiceDetailBreadcrumbs` via `useParams()`. `ProjectServiceDetailLayout` stays unchanged.

- [ ] **Step 3: Typecheck**

Run: `pnpm --filter=web check:types 2>&1 | tail -n 20`

Expected: no errors in `quick-actions.tsx` or the detail `layout.tsx`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/detail/quick-actions.tsx "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/layout.tsx"
git commit -m "feat(web): move service edit/delete into header quick actions"
```

---

### Task 7: Two-column root + delete tabs/header

**Files:**

- Modify: `apps/web/core/components/services/detail/root.tsx` (full rewrite)
- Modify: `apps/web/core/components/services/detail/index.ts`
- Delete: `apps/web/core/components/services/detail/header.tsx`, `apps/web/core/components/services/detail/tabs.tsx`

- [ ] **Step 1: Rewrite `root.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
// assets
import emptyModule from "@/app/assets/empty-state/module.svg?url";
// components
import { EmptyState } from "@/components/common/empty-state";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
import useReloadConfirmations from "@/hooks/use-reload-confirmation";
// local imports
import { ServiceLoadErrorState } from "../service-load-error-state";
import { ServiceDescription } from "./description";
import { ServiceDetailSidebar } from "./sidebar";
import { ServiceTitleInput } from "./title-input";
import { ServiceWorkItems } from "./work-items";

type Props = {
  serviceId: string;
};

export const ServiceDetailRoot = observer(function ServiceDetailRoot(props: Props) {
  const { serviceId } = props;
  // states
  const [isSubmitting, setIsSubmitting] = useState<TNameDescriptionLoader>("saved");
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // unsaved changes guard
  const { setShowAlert } = useReloadConfirmations(isSubmitting === "submitting");
  // store hooks
  const { fetchedMap, getServiceById, errorMap, fetchServices } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const pid = projectId?.toString();
  const service = getServiceById(serviceId);
  const hasFetched = pid ? fetchedMap[pid] : undefined;

  const workspaceId = currentWorkspace?.id;
  const hasError = pid ? errorMap[pid] : false;

  useEffect(() => {
    if (isSubmitting === "submitted") {
      setShowAlert(false);
      const timer = setTimeout(() => setIsSubmitting("saved"), 2000);
      return () => clearTimeout(timer);
    }
    if (isSubmitting === "submitting") setShowAlert(true);
  }, [isSubmitting, setShowAlert]);

  const handleRetry = () => {
    if (!workspaceSlug || !workspaceId || !pid) return;
    fetchServices(workspaceSlug.toString(), workspaceId, pid);
  };

  if (!service) {
    if (hasError) return <ServiceLoadErrorState onRetry={handleRetry} />;
    if (!hasFetched) return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    return (
      <EmptyState
        image={emptyModule}
        title={t("service.detail.not_found_title")}
        description={t("service.detail.not_found_description")}
        primaryButton={{
          text: t("service.detail.view_other_services"),
          onClick: () => router.push(`/${workspaceSlug}/projects/${projectId}/services`),
        }}
      />
    );
  }

  return (
    <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
      <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
        <ServiceTitleInput serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceDescription serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceWorkItems serviceId={serviceId} />
      </div>
      <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
        <ServiceDetailSidebar serviceId={serviceId} />
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Update `index.ts`**

Replace the whole file with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./root";
export * from "./quick-actions";
export * from "./description";
export * from "./dependencies";
export * from "./work-items";
```

- [ ] **Step 3: Delete the obsolete tab/header files**

```bash
git rm apps/web/core/components/services/detail/header.tsx apps/web/core/components/services/detail/tabs.tsx
```

- [ ] **Step 4: Typecheck + lint**

Run: `pnpm --filter=web check:types 2>&1 | tail -n 30`

Expected: PASS with no references to deleted `header`/`tabs`/`overview` modules.

Run: `pnpm --filter=web check:lint 2>&1 | tail -n 20`

Expected: no new warnings.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/detail/root.tsx apps/web/core/components/services/detail/index.ts
git commit -m "refactor(web): service detail two-column work-item layout"
```

---

### Task 8: Full verification + live rebuild

**Files:**

- None (verification only)

- [ ] **Step 1: Format touched files**

Run: `pnpm --filter=web fix:format 2>&1 | tail -n 5`

Expected: formatter reports files written or no changes.

- [ ] **Step 2: Run web unit tests**

Run: `pnpm --filter=web test 2>&1 | tail -n 15`

Expected: all suites PASS, including `core/services/service.helpers.test.ts`.

- [ ] **Step 3: Run the repo checks**

Run: `pnpm check:types 2>&1 | tail -n 20 && pnpm check:lint 2>&1 | tail -n 20`

Expected: PASS.

- [ ] **Step 4: Manual acceptance checklist (dev server or prod build)**

Start the dev server if needed per `AGENTS.md` (only one server on port 3000), open a service detail page, and confirm:

- No tabs render; the page is two columns on `md+` and stacked on mobile.
- Title is editable in place; changes persist after blur and the saved indicator appears.
- Sidebar shows Health pill, Last deploy, Status, Criticality, Type, Owner, Repository, Documentation.
- Changing Status/Criticality/Type/Owner updates immediately (optimistic store update) and survives a reload.
- Repository/Documentation: "Add" opens an inline input; Enter saves, Escape cancels, blank clears to "Add".
- Dependencies add/remove works from the sidebar; Work items add/remove works from the left column.
- Header "..." menu opens Edit (modal) and Delete (confirm + redirect to the services list).

- [ ] **Step 5: Rebuild prod web for the tunnel demo (per AGENTS.md)**

Run: `pnpm --filter=web build 2>&1 | tail -n 10`

Expected: build succeeds.

Run: `systemctl --user restart plane-web-prod.service && curl -s -o /dev/null -w "%{http_code}" http://localhost:3000`

Expected: `200`.

- [ ] **Step 6: Commit any formatting changes**

```bash
git add -A apps/web/core/components/services apps/web/core/services/service.helpers.test.ts "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/layout.tsx"
git commit -m "chore(web): format service detail layout files"
```

---

## Self-Review

1. **Spec coverage:**
   - Work-item 2-column layout → Task 7.
   - Inline sidebar property editing → Tasks 2, 3 (selects, owner, URLs).
   - Left column description + work items → Tasks 4, 5.
   - Title inline + Edit/Delete in header menu → Tasks 4, 6.
   - Health/last deploy/incidents in sidebar → Task 3.
   - Dependencies moved out of a tab into the sidebar → Task 3.
   - Unit test requirement (AGENTS.md) → Task 1 TDD helper test.
   - No locale changes → all keys reused (`common.properties`, `common.dependencies`, `service.fields.*`, `service.tabs.work_items`, `service.detail.*`, `aria_labels.projects_sidebar.toggle_quick_actions_menu`); `packages/i18n/src/locales/**` untouched.

2. **Placeholder scan:** no TBD/TODO; every code step shows complete file/block content; commands include expected output.

3. **Type consistency:**
   - `normalizeServiceUrl(value: string): string | null` defined in Task 1, consumed in Task 2 `url-property.tsx`.
   - `ServicePropertySelect<T extends string>` props (`value`, `options`, `i18nPrefix`, `onChange`) match Task 3 usage with `IService["status" | "criticality" | "type"]`.
   - `ServiceUrlProperty` props (`value: string | null`, `onSubmit: (value: string | null) => void`) match `update({ repository_url })` / `update({ documentation_url })` which accept `string | null`.
   - `ServiceDetailSidebar`, `ServiceDescription`, `ServiceTitleInput`, `ServiceWorkItems`, `ServiceDetailQuickActions` are the exact export names used by `root.tsx` and the route `layout.tsx`.
   - `ServiceDependencies` keeps its existing `{ serviceId }` props.
   - `MemberDropdown` single-select branch requires `value: string | null` + `onChange: (val: string | null) => void`; `service.owner_id` is `string | null` (`packages/types/src/service/core.ts:54`).
