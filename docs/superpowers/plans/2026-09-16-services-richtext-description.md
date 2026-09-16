# Services Rich-Text Description Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the plain `TextArea` service description with the work-items `RichTextEditor` (modal form + autosaving Overview), with image upload and AI disabled and no backend changes.

**Architecture:** Reuse `DescriptionInput` (autosave + debounce, same as `IssueMainContent`) in `ServiceOverview` and controlled `RichTextEditor` (same as `IssueDescriptionEditor`) in `ServiceForm`. Derive plain `description` via a new `stripHtmlToText` helper so the existing Rust columns (`description` + `description_html`) keep working.

**Tech Stack:** React 19, `react-hook-form`, `@plane/editor` (Tiptap), MobX `useService`, existing `ServiceService` REST client.

---

## File structure

- Modify: `apps/web/core/services/service.helpers.ts` — add `stripHtmlToText` + shared `SERVICE_DESCRIPTION_DISABLED_EXTENSIONS`.
- Modify: `apps/web/core/components/services/service-form.tsx` — `TextArea` → controlled `RichTextEditor`; add `workspaceSlug` prop; submit sends `description_html` + derived `description`.
- Modify: `apps/web/core/components/services/modal.tsx` — pass `workspaceSlug` through to `ServiceForm`.
- Modify: `apps/web/core/components/services/detail/overview.tsx` — disabled `TextArea` → `DescriptionInput` autosave + `NameDescriptionUpdateStatus`.
- No backend changes. No new dependencies. No test runner exists for `apps/web` (verified: no `test` script in `apps/web/package.json`, no `*.test.*` under `apps/web`), so verification = `check:types` + `oxlint`/`oxfmt` on touched files + manual browser checklist.

Reference patterns (read before editing):

- `apps/web/core/components/issues/issue-detail/main-content.tsx:110-129` (DescriptionInput autosave)
- `apps/web/core/components/issues/issue-modal/components/description-editor.tsx:180-245` (controlled RichTextEditor in a form)
- `apps/web/core/components/editor/rich-text/description-input/root.tsx:60,108-129,232-253` (DescriptionInput props: `entityId`, `fileAssetType: EFileAssetType`, `initialValue`, `onSubmit({description_html, description_json})`, `setIsSubmitting`, `workspaceSlug`)
- `apps/web/core/components/editor/rich-text/editor.tsx:22-40` (when `editable=true`, `searchMentionCallback` + `uploadFile` + `duplicateFile` are required props)
- `apps/web/core/hooks/use-editor-flagging.ts:44-47` (richText already disables `["ai", "collaboration-cursor"]` by default)

---

### Task 1: Shared helper + disabled-extensions list

**Files:**

- Modify: `apps/web/core/services/service.helpers.ts`

- [ ] **Step 1: Add `stripHtmlToText` and `SERVICE_DESCRIPTION_DISABLED_EXTENSIONS` to `service.helpers.ts`**

Append this code to the end of `apps/web/core/services/service.helpers.ts` (after `orderServices`). Do not change existing exports.

```ts
import type { TExtensions } from "@plane/editor";

/**
 * Extensions disabled for the service description editor: image upload (no
 * `SERVICE_*` file-asset type exists yet) and AI. `TExtensions`
 * (`packages/editor/src/types/extensions.ts`) only supports
 * `"ai" | "collaboration-cursor" | "issue-embed" | "slash-commands" |
 * "enter-key" | "image"`, and only `"image"` is actually gated in
 * `CoreEditorExtensions` — table/mention stay enabled for full work-item
 * parity. `collaboration-cursor` is additionally disabled by default in
 * `useEditorFlagging` for all richText editors.
 */
export const SERVICE_DESCRIPTION_DISABLED_EXTENSIONS: TExtensions[] = ["image", "ai"];

/**
 * Derives the plain-text `description` column from editor HTML for the
 * existing Rust `services` table. Block closings become newlines, all other
 * tags are stripped, common entities decoded, blank lines dropped.
 */
export const stripHtmlToText = (html: string): string => {
  if (!html || html.trim() === "") return "";
  return html
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|h[1-6]|li|ul|ol|tr|blockquote|pre)>/gi, "\n")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .join("\n")
    .trim();
};
```

And extend the existing type import on line 7 of the same file from:

```ts
import type { IService, IServiceDependency, TServiceFilters, TServiceOrderByOptions } from "@plane/types";
```

to (add nothing there — the `@plane/editor` import is a separate new line at top-level):

```ts
import type { TExtensions } from "@plane/editor";
import type { IService, IServiceDependency, TServiceFilters, TServiceOrderByOptions } from "@plane/types";
```

- [ ] **Step 2: Runtime smoke-check `stripHtmlToText` against the real file**

Run (the helper has no runtime imports — only `import type` — so plain Node can import it):

```bash
node --experimental-strip-types -e "
import('./apps/web/core/services/service.helpers.ts').then((m) => {
  const cases = [
    ['', ''],
    ['<p></p>', ''],
    ['<p>Hello <strong>world</strong></p>', 'Hello world'],
    ['<h2>Judul</h2><ul><li>satu</li><li>dua</li></ul>', 'Judul\nsatu\ndua'],
    ['<p>a &amp; b&nbsp;&lt;tag&gt;</p>', 'a & b <tag>'],
    ['<table><tr><td>c1</td><td>c2</td></tr></table>', 'c1\nc2'],
    ['<p>&amp;lt;</p>', '&lt;'],
  ];
  let failed = 0;
  for (const [input, expected] of cases) {
    const actual = m.stripHtmlToText(input);
    if (actual !== expected) {
      failed++;
      console.error('FAIL', JSON.stringify(input), 'expected', JSON.stringify(expected), 'got', JSON.stringify(actual));
    }
  }
  if (failed > 0) process.exit(1);
  console.log('stripHtmlToText: all 7 cases pass');
});
"
```

Expected: `stripHtmlToText: all 5 cases pass`, exit 0. If any case fails, fix the helper (Step 1) and re-run this step before continuing.

- [ ] **Step 3: Typecheck the web app**

Run: `pnpm --filter=web check:types`
Expected: exit 0, no TypeScript errors. (This runs `react-router typegen && tsc --noEmit`; it takes a few minutes.)

- [ ] **Step 4: Lint + format the touched file**

Run: `pnpm --filter=web exec oxlint core/services/service.helpers.ts` (workdir: `apps/web`)
Expected: no errors or warnings.

Run: `pnpm --filter=web exec oxfmt --check core/services/service.helpers.ts` (workdir: `apps/web`)
Expected: no diff. If it reports a diff, run `pnpm --filter=web exec oxfmt core/services/service.helpers.ts` and re-check.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/service.helpers.ts
git commit -m "feat(services): add stripHtmlToText and disabled editor extensions"
```

---

### Task 2: Rich-text editor in the create/update modal form

**Files:**

- Modify: `apps/web/core/components/services/service-form.tsx`
- Modify: `apps/web/core/components/services/modal.tsx`

- [ ] **Step 1: Update `ServiceForm` props to accept `workspaceSlug`**

In `apps/web/core/components/services/service-form.tsx`, change the `Props` type from:

```ts
type Props = {
  handleFormSubmit: (values: Partial<IService>) => Promise<void>;
  handleClose: () => void;
  status: boolean;
  projectId: string;
  data?: IService;
};
```

to:

```ts
type Props = {
  handleFormSubmit: (values: Partial<IService>) => Promise<void>;
  handleClose: () => void;
  status: boolean;
  projectId: string;
  workspaceSlug: string;
  data?: IService;
};
```

And change the props destructure in `ServiceForm` from:

```ts
const { handleFormSubmit, handleClose, status, projectId, data } = props;
```

to:

```ts
const { handleFormSubmit, handleClose, status, projectId, workspaceSlug, data } = props;
```

- [ ] **Step 2: Update imports in `service-form.tsx`**

Replace the import block (lines 7-18) — remove `TextArea` from `@plane/ui` (it is only used by the description field), add editor + store + helper imports:

```ts
import { useEffect } from "react";
import { Controller, useForm, type Control } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IService } from "@plane/types";
// ui
import { CustomSelect, Input as UIKitInput } from "@plane/ui";
// components
import { MemberDropdown } from "@/components/dropdowns/member/dropdown";
import { RichTextEditor } from "@/components/editor/rich-text";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";
// plane web services
import { WorkspaceService } from "@/services/workspace.service";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS, stripHtmlToText } from "@/services/service.helpers";

// services init
const workspaceService = new WorkspaceService();
```

`UIKitInput` stays (used by the URL fields). `TextArea` must be gone — verify with `rg -n "TextArea" apps/web/core/components/services/service-form.tsx` returning no matches after the edit.

- [ ] **Step 3: Wire `description_html` through the form state**

Change the `defaultValues` const from:

```ts
const defaultValues: Partial<IService> = {
  name: "",
  description: "",
  status: "planned",
  criticality: "medium",
  type: "internal",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
};
```

to:

```ts
const defaultValues: Partial<IService> = {
  name: "",
  description_html: "",
  status: "planned",
  criticality: "medium",
  type: "internal",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
};
```

Change the `useForm` `defaultValues` from:

```ts
  } = useForm<IService>({
    defaultValues: {
      name: data?.name || "",
      description: data?.description || "",
      status: data?.status || "planned",
      criticality: data?.criticality || "medium",
      type: data?.type || "internal",
      owner_id: data?.owner_id || null,
      repository_url: data?.repository_url || "",
      documentation_url: data?.documentation_url || "",
    },
  });
```

to:

```ts
  } = useForm<IService>({
    defaultValues: {
      name: data?.name || "",
      description_html: data?.description_html || "",
      status: data?.status || "planned",
      criticality: data?.criticality || "medium",
      type: data?.type || "internal",
      owner_id: data?.owner_id || null,
      repository_url: data?.repository_url || "",
      documentation_url: data?.documentation_url || "",
    },
  });
```

- [ ] **Step 4: Replace the submit handler to send editor HTML**

Replace `handleCreateUpdateService` from:

```ts
  const handleCreateUpdateService = async (formData: Partial<IService>) => {
    const description = formData.description ?? "";
    try {
      await handleFormSubmit({
        ...formData,
        description_html: description ? `<p>${description}</p>` : "",
        repository_url: formData.repository_url || null,
        documentation_url: formData.documentation_url || null,
      });
```

to:

```ts
  const handleCreateUpdateService = async (formData: Partial<IService>) => {
    const descriptionHtml =
      formData.description_html && formData.description_html.trim() !== "" ? formData.description_html : "<p></p>";
    try {
      await handleFormSubmit({
        ...formData,
        description: stripHtmlToText(descriptionHtml),
        description_html: descriptionHtml,
        repository_url: formData.repository_url || null,
        documentation_url: formData.documentation_url || null,
      });
```

(The rest of the handler — `reset({ ...defaultValues })` and the catch — stays unchanged.)

- [ ] **Step 5: Replace the description `TextArea` with `RichTextEditor`**

Replace the whole description block from:

```tsx
<div>
  <Controller
    name="description"
    control={control}
    render={({ field: { value, onChange } }) => (
      <TextArea
        id="description"
        name="description"
        value={value}
        onChange={onChange}
        placeholder={t("service.fields.description")}
        className="min-h-24 w-full resize-none text-14"
        hasError={Boolean(errors?.description)}
      />
    )}
  />
</div>
```

with:

```tsx
<div>
  <Controller
    name="description_html"
    control={control}
    render={({ field: { onChange } }) => (
      <RichTextEditor
        editable
        key={data?.id ?? "service-create"}
        id="service-description-editor"
        initialValue={stableDescriptionHtml}
        value={stableDescriptionHtml}
        workspaceSlug={workspaceSlug}
        workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
        projectId={projectId}
        disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
        onChange={(_descriptionJson: object, descriptionHtml: string) => {
          onChange(descriptionHtml);
        }}
        placeholder={t("service.fields.description")}
        searchMentionCallback={async (payload) =>
          await workspaceService.searchEntity(workspaceSlug, {
            ...payload,
            project_id: projectId,
          })
        }
        containerClassName="min-h-24 rounded-md border border-subtle"
        uploadFile={async () => {
          throw new Error("File upload is disabled for service descriptions.");
        }}
        duplicateFile={async () => {
          throw new Error("File upload is disabled for service descriptions.");
        }}
      />
    )}
  />
</div>
```

And add the `useWorkspace` hook plus a stable server snapshot next to the existing `const { t } = useTranslation();` line:

```ts
const { t } = useTranslation();
const { getWorkspaceBySlug } = useWorkspace();
// Stable server snapshot for the editor: typing updates RHF via onChange
// but must NOT feed back into `value` (would force setContent + selection
// move on every keystroke — see packages/editor/src/hooks/use-editor.ts).
// Entity switches remount via key below, mirroring DescriptionInput.
const stableDescriptionHtml =
  data?.description_html && data.description_html.trim() !== "" ? data.description_html : "<p></p>";
```

- [ ] **Step 6: Pass `workspaceSlug` from the modal**

In `apps/web/core/components/services/modal.tsx`, change:

```tsx
<ServiceForm
  handleFormSubmit={handleFormSubmit}
  handleClose={handleClose}
  status={!!data}
  projectId={projectId}
  data={data}
/>
```

to:

```tsx
<ServiceForm
  handleFormSubmit={handleFormSubmit}
  handleClose={handleClose}
  status={!!data}
  projectId={projectId}
  workspaceSlug={workspaceSlug}
  data={data}
/>
```

- [ ] **Step 7: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: exit 0, no errors.

- [ ] **Step 8: Lint + format touched files**

Run: `pnpm --filter=web exec oxlint core/components/services/service-form.tsx core/components/services/modal.tsx` (workdir: `apps/web`)
Expected: no errors or warnings.

Run: `pnpm --filter=web exec oxfmt --check core/components/services/service-form.tsx core/components/services/modal.tsx` (workdir: `apps/web`)
Expected: no diff (run `oxfmt` without `--check` on those paths if there is one, then re-check).

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/components/services/service-form.tsx apps/web/core/components/services/modal.tsx
git commit -m "feat(services): use RichTextEditor for description in service form"
```

---

### Task 3: Autosaving rich-text description in Overview tab

**Files:**

- Modify: `apps/web/core/components/services/detail/overview.tsx`

- [ ] **Step 1: Replace imports in `overview.tsx`**

Replace the whole import block (lines 7-15) from:

```tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IService } from "@plane/types";
// ui
import { TextArea } from "@plane/ui";
// hooks
import { useService } from "@/hooks/store/use-service";
```

with:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IService, TNameDescriptionLoader } from "@plane/types";
import { EFileAssetType } from "@plane/types";
// components
import { DescriptionInput } from "@/components/editor/rich-text/description-input";
import { NameDescriptionUpdateStatus } from "@/components/issues/issue-update-status";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS, stripHtmlToText } from "@/services/service.helpers";
```

Note: `EFileAssetType.PROJECT_DESCRIPTION` is used below only to satisfy the `DescriptionInput` `fileAssetType: EFileAssetType` prop type — the `image` extension is disabled and the file handlers are dummy throwers, so the asset type is never exercised. No new `SERVICE_*` asset type is added in this iteration (explicit non-goal in the spec).

- [ ] **Step 2: Wire autosave state and store update**

Replace the component body opening from:

```tsx
export const ServiceOverview = observer(function ServiceOverview(props: Props) {
  const { serviceId } = props;
  // router
  const { projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getDependenciesByProject } = useService();
```

with:

```tsx
export const ServiceOverview = observer(function ServiceOverview(props: Props) {
  const { serviceId } = props;
  // states
  const [isSubmitting, setIsSubmitting] = useState<TNameDescriptionLoader>("saved");
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getDependenciesByProject, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString() ?? "";
  const workspaceId = getWorkspaceBySlug(slug)?.id;
```

- [ ] **Step 3: Replace the read-only description block with `DescriptionInput`**

Replace from:

```tsx
{
  service.description && (
    <div className="flex flex-col gap-1.5">
      <h4 className="text-13 font-medium text-secondary">{t("service.fields.description")}</h4>
      <TextArea
        className="ring-none !m-0 max-h-max w-full resize-none !border-0 bg-transparent !p-0 text-13 leading-5 text-secondary outline-none"
        value={service.description}
        disabled
      />
    </div>
  );
}
```

with:

```tsx
{
  slug && workspaceId && (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-4">
        <h4 className="text-13 font-medium text-secondary">{t("service.fields.description")}</h4>
        <NameDescriptionUpdateStatus isSubmitting={isSubmitting} />
      </div>
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
    </div>
  );
}
```

Keep the rest of the component (dependencies grid) unchanged. `pid` is the existing `const pid = projectId?.toString() ?? service.project_id;` line — it stays where it is.

- [ ] **Step 4: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: exit 0, no errors.

- [ ] **Step 5: Lint + format**

Run: `pnpm --filter=web exec oxlint core/components/services/detail/overview.tsx` (workdir: `apps/web`)
Expected: no errors or warnings.

Run: `pnpm --filter=web exec oxfmt --check core/components/services/detail/overview.tsx` (workdir: `apps/web`)
Expected: no diff (format in place if needed, then re-check).

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/detail/overview.tsx
git commit -m "feat(services): autosaving rich-text description in overview"
```

---

### Task 4: End-to-end verification

**Files:** none (verification only)

- [ ] **Step 1: Full web checks**

Run: `pnpm --filter=web check:lint`
Expected: exit 0 (warning budget is configured in the script; no new warnings from touched files).

Run: `pnpm --filter=web check:format`
Expected: exit 0.

- [ ] **Step 2: Build the web app**

Run: `pnpm --filter=web build`
Expected: build succeeds. (Required because per `AGENTS.md`, prod on port 3000 serves the static build — after changing web code the tunnel only picks it up after `pnpm --filter=web build` + `systemctl --user restart plane-web-prod.service`.)

- [ ] **Step 3: Manual browser checklist (dev or rebuilt prod)**

1. Open a project → Services → create service; description shows the rich-text toolbar; type `# Judul`, `**bold**`, `- item`, `code`, `> quote` → each converts to formatted blocks (markdown shortcuts via `tiptap-markdown`).
2. Save → service appears; reload page → formatting persists in Overview.
3. Edit description in Overview → "Saving..." then "Saved" appears; reload → changes persisted; network tab shows a single `PATCH/PUT services/:id` ~1.5s after last keystroke (debounce).
4. Open an old service created before this change (plain `<p>text</p>`) → renders as paragraph, editable, no loader hang.
5. Confirm no image button in the service editor toolbar (table/mention stay available — work-item parity); drag-dropping a file shows the disabled-upload error instead of uploading.
6. Check browser console: no errors during edit/save cycles.

- [ ] **Step 4: Commit any review fixes only**

If Step 3 finds issues, fix them in new commits (one per fix). Do not amend existing commits.
