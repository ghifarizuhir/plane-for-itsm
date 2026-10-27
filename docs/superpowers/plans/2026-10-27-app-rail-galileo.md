# Workspace Rail + Galileo Full Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a 60px workspace rail (Work / Procedure / Galileo / Settings) and a deep-linkable full-page Galileo chat with a grouped conversation-history sidebar.

**Architecture:** Enable the existing but dormant `AppRailRoot`, replace its single dock item with three mode links, and add a new `(galileo)` route group with its own layout and history sidebar. Galileo reuses `aiAssistantStore` and the existing AI conversation services; a thin `sendGalileoMessage` path forces classic mode without touching the right sidebar's selected mode.

**Tech Stack:** React 19, React Router 7, MobX (`mobx-react`), TypeScript, Tailwind tokens, Headless UI (`Dialog`, `Transition`, `Menu`), Vitest, `@plane/propel` + `@plane/ui`, i18next.

**Spec:** `docs/superpowers/specs/2026-10-27-app-rail-galileo-design.md`

**Working tree note:** `apps/web/core/components/workspace/application-scope/select.tsx` already has an unrelated local modification. Do not stage or commit that file in this plan.

---

## File Structure

New:

- `apps/web/core/lib/workspace-modes.ts` — pure route → workspace mode helper.
- `apps/web/core/lib/galileo-routing.ts` — pure URL/gating helpers.
- `apps/web/core/lib/galileo-history.ts` — pure conversation date grouping.
- `apps/web/core/components/navigation/workspace-mode-menu.tsx` — mobile mode menu.
- `apps/web/core/components/galileo/layout.tsx` — desktop sidebar + mobile drawer shell + access gate.
- `apps/web/core/components/galileo/history-sidebar.tsx` — history list, new chat, rename, delete.
- `apps/web/core/components/galileo/chat-page.tsx` — route sync, header, thread, error/empty states.
- `apps/web/core/components/galileo/message.tsx` — user/assistant message rendering.
- `apps/web/core/components/galileo/composer.tsx` — auto-growing composer.
- `apps/web/core/components/galileo/empty-state.tsx` — new-chat suggestions.
- `apps/web/app/(all)/[workspaceSlug]/(galileo)/layout.tsx` — route layout.
- `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/page.tsx` — new chat route.
- `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/[conversationId]/page.tsx` — conversation route.
- `apps/web/core/lib/workspace-modes.test.ts`
- `apps/web/core/lib/galileo-routing.test.ts`
- `apps/web/core/lib/galileo-history.test.ts`

Modified:

- `packages/i18n/src/locales/en/navigation.json`
- `packages/i18n/src/locales/en/assistant.json`
- `apps/web/core/lib/app-rail/types.ts`
- `apps/web/core/lib/app-rail/provider.tsx`
- `apps/web/app/(all)/[workspaceSlug]/layout.tsx`
- `apps/web/core/hooks/use-workspace-paths.ts`
- `apps/web/core/components/navigation/app-rail-hoc.tsx`
- `apps/web/core/components/navigation/app-rail-root.tsx`
- `apps/web/core/components/sidebar/sidebar-item.tsx`
- `apps/web/core/components/navigation/top-navigation-root.tsx`
- `apps/web/core/components/workspace/content-wrapper.tsx`
- `apps/web/core/store/ai-assistant.store.ts`
- `apps/web/core/store/ai-assistant.store.test.ts`
- `apps/web/app/routes/core.ts`
- `packages/i18n/src/locales/<all-locales>/{navigation,assistant}.json`

---

### Task 1: English i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/navigation.json`
- Modify: `packages/i18n/src/locales/en/assistant.json`

- [ ] **Step 1: Add navigation labels**

In `packages/i18n/src/locales/en/navigation.json`, inside `"sidebar"`, add:

```json
"work": "Work",
"procedure": "Procedure",
"galileo": "Galileo",
```

Keep the existing `"pi_chat": "Galileo"` key untouched.

- [ ] **Step 2: Add Galileo page strings**

In `packages/i18n/src/locales/en/assistant.json`, inside `"assistant"`, add:

```json
"galileo": {
  "new_chat": "New chat",
  "history": "Chat history",
  "today": "Today",
  "yesterday": "Yesterday",
  "previous_7_days": "Previous 7 days",
  "older": "Older",
  "empty_history": "No conversations yet.",
  "empty_title": "What can Galileo help with?",
  "empty_subtitle": "Ask anything, or start with one of these.",
  "suggestion_summarize": "Summarize my latest incidents",
  "suggestion_procedure": "Find the right procedure for a service request",
  "suggestion_draft": "Draft a customer update for an incident",
  "suggestion_explain": "Explain how our change process works",
  "rename": "Rename",
  "delete_confirm": "Delete this chat?",
  "conversation_actions": "Conversation actions",
  "conversation_title": "Conversation title",
  "not_found_title": "Conversation not found",
  "not_found_body": "This conversation may have been deleted or belongs to another user.",
  "start_new_chat": "Start a new chat",
  "load_failed": "Couldn't load conversations.",
  "composer_placeholder": "Ask Galileo anything…",
  "composer_hint": "Enter to send · Shift+Enter for a new line",
  "send": "Send message",
  "scroll_to_bottom": "Scroll to bottom",
  "typing": "Galileo is thinking",
  "switch_mode": "Switch workspace mode",
  "third_party_notice": "Messages are sent to a 3rd party AI service."
}
```

The final `assistant.json` must still be one valid JSON object. Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/assistant.json','utf8')); JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/navigation.json','utf8')); console.log('json ok')"
```

Expected: `json ok`

- [ ] **Step 3: Confirm sync check currently fails for the new keys**

Run:

```bash
pnpm --filter @plane/i18n run sync:check
```

Expected: the report lists missing `assistant.galileo.*` and `sidebar.work|procedure|galileo` keys for non-English locales. Do not fix translations here; Task 9 handles them.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/en/navigation.json packages/i18n/src/locales/en/assistant.json
git commit -m "i18n(web): add galileo navigation strings"
```

---

### Task 2: Workspace mode route helper

**Files:**

- Create: `apps/web/core/lib/workspace-modes.ts`
- Test: `apps/web/core/lib/workspace-modes.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/workspace-modes.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { getWorkspaceMode } from "./workspace-modes";

describe("getWorkspaceMode", () => {
  const slug = "acme";

  it("detects the workspace home", () => {
    expect(getWorkspaceMode("/acme", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/", slug)).toBe("work");
  });

  it("detects Work routes", () => {
    expect(getWorkspaceMode("/acme/projects/abc/issues", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/notifications", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/projects/abc/settings", slug)).toBe("work");
  });

  it("detects Procedure", () => {
    expect(getWorkspaceMode("/acme/docs", slug)).toBe("procedure");
    expect(getWorkspaceMode("/acme/docs/itil4-overview", slug)).toBe("procedure");
  });

  it("detects Galileo", () => {
    expect(getWorkspaceMode("/acme/galileo", slug)).toBe("galileo");
    expect(getWorkspaceMode("/acme/galileo/7f4c", slug)).toBe("galileo");
  });

  it("detects Settings", () => {
    expect(getWorkspaceMode("/acme/settings", slug)).toBe("settings");
    expect(getWorkspaceMode("/acme/settings/members", slug)).toBe("settings");
  });

  it("does not match prefix collisions", () => {
    expect(getWorkspaceMode("/acme/settings-archive", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/docs-old", slug)).toBe("work");
    expect(getWorkspaceMode("/acme/galileo-old", slug)).toBe("work");
  });

  it("returns other outside the workspace", () => {
    expect(getWorkspaceMode("/other/docs", slug)).toBe("other");
    expect(getWorkspaceMode("/", slug)).toBe("other");
  });

  it("returns other for an empty slug", () => {
    expect(getWorkspaceMode("/acme", "")).toBe("other");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
pnpm --filter=web exec vitest run core/lib/workspace-modes.test.ts
```

Expected: FAIL with `Cannot find module './workspace-modes'` or `getWorkspaceMode is not a function`.

- [ ] **Step 3: Implement the helper**

Create `apps/web/core/lib/workspace-modes.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWorkspaceMode = "work" | "procedure" | "galileo" | "settings" | "other";

const trimTrailingSlashes = (pathname: string): string => {
  const trimmed = pathname.replace(/\/+$/, "");
  return trimmed.length > 0 ? trimmed : "/";
};

export const getWorkspaceMode = (pathname: string, workspaceSlug: string): TWorkspaceMode => {
  if (!workspaceSlug) return "other";

  const base = `/${workspaceSlug}`;
  const path = trimTrailingSlashes(pathname);

  if (path === `${base}/settings` || path.startsWith(`${base}/settings/`)) return "settings";
  if (path === `${base}/docs` || path.startsWith(`${base}/docs/`)) return "procedure";
  if (path === `${base}/galileo` || path.startsWith(`${base}/galileo/`)) return "galileo";
  if (path === base || path.startsWith(`${base}/`)) return "work";
  return "other";
};
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
pnpm --filter=web exec vitest run core/lib/workspace-modes.test.ts
```

Expected: PASS, 8 tests.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/workspace-modes.ts apps/web/core/lib/workspace-modes.test.ts
git commit -m "feat(web): add workspace mode route helper"
```

---

### Task 3: Galileo routing and history helpers

**Files:**

- Create: `apps/web/core/lib/galileo-routing.ts`
- Create: `apps/web/core/lib/galileo-history.ts`
- Test: `apps/web/core/lib/galileo-routing.test.ts`
- Test: `apps/web/core/lib/galileo-history.test.ts`

- [ ] **Step 1: Write the failing routing test**

Create `apps/web/core/lib/galileo-routing.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  buildGalileoConversationHref,
  buildGalileoHref,
  canUseGalileo,
  shouldNavigateToNewConversation,
} from "./galileo-routing";

describe("galileo routing helpers", () => {
  it("builds Galileo URLs", () => {
    expect(buildGalileoHref("acme")).toBe("/acme/galileo");
    expect(buildGalileoConversationHref("acme", "conv-1")).toBe("/acme/galileo/conv-1");
  });

  it("navigates only when the route is new and a conversation now exists", () => {
    expect(shouldNavigateToNewConversation(undefined, "conv-1")).toBe(true);
    expect(shouldNavigateToNewConversation("conv-1", "conv-1")).toBe(false);
    expect(shouldNavigateToNewConversation(undefined, undefined)).toBe(false);
  });

  it("requires both LLM configuration and AI permission", () => {
    expect(canUseGalileo(true, true)).toBe(true);
    expect(canUseGalileo(false, true)).toBe(false);
    expect(canUseGalileo(true, false)).toBe(false);
  });
});
```

- [ ] **Step 2: Write the failing history test**

Create `apps/web/core/lib/galileo-history.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { TAiConversation } from "./ai-conversations";
import { groupConversationsByDate } from "./galileo-history";

const conversation = (id: string, updatedAt: Date): TAiConversation => ({
  id,
  title: id,
  mode: "classic",
  created_at: updatedAt.toISOString(),
  updated_at: updatedAt.toISOString(),
});

const at = (day: number, hour = 9): Date => new Date(2026, 9, day, hour, 0, 0);

describe("groupConversationsByDate", () => {
  it("groups conversations into today, yesterday, previous 7 days, and older", () => {
    const groups = groupConversationsByDate(
      [
        conversation("today", at(27)),
        conversation("yesterday", at(26)),
        conversation("week", at(22)),
        conversation("older", at(1)),
      ],
      at(27, 12)
    );

    expect(groups.today.map((item) => item.id)).toEqual(["today"]);
    expect(groups.yesterday.map((item) => item.id)).toEqual(["yesterday"]);
    expect(groups.previous7Days.map((item) => item.id)).toEqual(["week"]);
    expect(groups.older.map((item) => item.id)).toEqual(["older"]);
  });

  it("sorts newest first inside a group", () => {
    const groups = groupConversationsByDate(
      [conversation("older-today", at(27, 8)), conversation("newer-today", at(27, 11))],
      at(27, 12)
    );
    expect(groups.today.map((item) => item.id)).toEqual(["newer-today", "older-today"]);
  });

  it("puts invalid timestamps in older", () => {
    const groups = groupConversationsByDate(
      [{ ...conversation("broken", at(27)), updated_at: "not-a-date" }],
      at(27, 12)
    );
    expect(groups.older.map((item) => item.id)).toEqual(["broken"]);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run:

```bash
pnpm --filter=web exec vitest run core/lib/galileo-routing.test.ts core/lib/galileo-history.test.ts
```

Expected: FAIL with missing modules.

- [ ] **Step 4: Implement the routing helpers**

Create `apps/web/core/lib/galileo-routing.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const buildGalileoHref = (workspaceSlug: string): string => `/${workspaceSlug}/galileo`;

export const buildGalileoConversationHref = (workspaceSlug: string, conversationId: string): string =>
  `/${workspaceSlug}/galileo/${conversationId}`;

export const canUseGalileo = (hasLlmConfigured: boolean, canUseAi: boolean): boolean => hasLlmConfigured && canUseAi;

export const shouldNavigateToNewConversation = (
  routeConversationId: string | undefined,
  activeConversationId: string | undefined
): boolean => !routeConversationId && !!activeConversationId;
```

- [ ] **Step 5: Implement the history grouping helper**

Create `apps/web/core/lib/galileo-history.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiConversation } from "@/lib/ai-conversations";

export type TGallileoHistoryGroup = "today" | "yesterday" | "previous7Days" | "older";

export type TGallileoHistoryGroups = Record<TGallileoHistoryGroup, TAiConversation[]>;

const startOfDay = (date: Date): Date => new Date(date.getFullYear(), date.getMonth(), date.getDate());

export const groupConversationsByDate = (
  conversations: TAiConversation[],
  now: Date = new Date()
): TGallileoHistoryGroups => {
  const today = startOfDay(now);
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  const previous7Days = new Date(today);
  previous7Days.setDate(today.getDate() - 7);

  const groups: TGallileoHistoryGroups = {
    today: [],
    yesterday: [],
    previous7Days: [],
    older: [],
  };

  const sorted = [...conversations].sort((a, b) => {
    const aTime = new Date(a.updated_at).getTime();
    const bTime = new Date(b.updated_at).getTime();
    const safeA = Number.isNaN(aTime) ? Number.NEGATIVE_INFINITY : aTime;
    const safeB = Number.isNaN(bTime) ? Number.NEGATIVE_INFINITY : bTime;
    return safeB - safeA;
  });

  for (const conversation of sorted) {
    const updatedAt = new Date(conversation.updated_at);
    if (Number.isNaN(updatedAt.getTime())) {
      groups.older.push(conversation);
    } else if (updatedAt >= today) {
      groups.today.push(conversation);
    } else if (updatedAt >= yesterday) {
      groups.yesterday.push(conversation);
    } else if (updatedAt >= previous7Days) {
      groups.previous7Days.push(conversation);
    } else {
      groups.older.push(conversation);
    }
  }

  return groups;
};
```

- [ ] **Step 6: Run the tests to verify they pass**

Run:

```bash
pnpm --filter=web exec vitest run core/lib/galileo-routing.test.ts core/lib/galileo-history.test.ts
```

Expected: PASS, 6 tests.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/lib/galileo-routing.ts apps/web/core/lib/galileo-history.ts \
  apps/web/core/lib/galileo-routing.test.ts apps/web/core/lib/galileo-history.test.ts
git commit -m "feat(web): add galileo routing and history helpers"
```

---

### Task 4: Enable the rail and add Work / Procedure / Galileo

**Files:**

- Modify: `apps/web/core/lib/app-rail/types.ts`
- Modify: `apps/web/core/lib/app-rail/provider.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/layout.tsx`
- Modify: `apps/web/core/hooks/use-workspace-paths.ts`
- Modify: `apps/web/core/components/navigation/app-rail-hoc.tsx`
- Modify: `apps/web/core/components/navigation/app-rail-root.tsx`
- Modify: `apps/web/core/components/sidebar/sidebar-item.tsx`

- [ ] **Step 1: Simplify the rail visibility contract (no undock)**

Replace `apps/web/core/lib/app-rail/types.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export interface IAppRailVisibilityContext {
  isEnabled: boolean;
  shouldRenderAppRail: boolean;
}
```

Replace `apps/web/core/lib/app-rail/provider.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useMemo } from "react";
import { observer } from "mobx-react";
import { AppRailVisibilityContext } from "./context";
import type { IAppRailVisibilityContext } from "./types";

interface AppRailVisibilityProviderProps {
  children: React.ReactNode;
  isEnabled?: boolean;
}

export const AppRailVisibilityProvider = observer(function AppRailVisibilityProvider({
  children,
  isEnabled = false,
}: AppRailVisibilityProviderProps) {
  const value: IAppRailVisibilityContext = useMemo(
    () => ({
      isEnabled,
      shouldRenderAppRail: isEnabled,
    }),
    [isEnabled]
  );

  return <AppRailVisibilityContext.Provider value={value}>{children}</AppRailVisibilityContext.Provider>;
});
```

- [ ] **Step 2: Enable the rail in the workspace layout**

In `apps/web/app/(all)/[workspaceSlug]/layout.tsx`, change:

```tsx
<AppRailVisibilityProvider>
```

to:

```tsx
<AppRailVisibilityProvider isEnabled>
```

- [ ] **Step 3: Centralize path detection**

Replace `apps/web/core/hooks/use-workspace-paths.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams, usePathname } from "next/navigation";
import { getWorkspaceMode } from "@/lib/workspace-modes";

/**
 * Custom hook to detect different workspace paths.
 * `isProjectsPath` is kept as a legacy alias for Work routes.
 */
export const useWorkspacePaths = () => {
  const { workspaceSlug } = useParams();
  const pathname = usePathname();
  const slug = workspaceSlug?.toString() ?? "";
  const mode = getWorkspaceMode(pathname ?? "", slug);

  const isSettingsPath = mode === "settings";
  const isDocsPath = mode === "procedure";
  const isGalileoPath = mode === "galileo";
  const isWorkPath = mode === "work";
  const isProjectsPath = isWorkPath;
  const isWikiPath = pathname.includes(`/${slug}/wiki`);
  const isAiPath = pathname.includes(`/${slug}/pi-chat`);
  const isNotificationsPath = pathname.includes(`/${slug}/notifications`);

  return {
    isSettingsPath,
    isWikiPath,
    isAiPath,
    isProjectsPath,
    isWorkPath,
    isDocsPath,
    isGalileoPath,
    isNotificationsPath,
  };
};
```

- [ ] **Step 4: Replace the rail items**

Replace `apps/web/core/components/navigation/app-rail-hoc.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { AiStar1Outline, DocumentationOutline, WorkItemsOutline } from "@makeplane/propel/icons";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { AppSidebarItemData } from "@/components/sidebar/sidebar-item";
import { useInstance } from "@/hooks/store/use-instance";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
import { canUseGalileo } from "@/lib/galileo-routing";

type WithDockItemsProps = {
  dockItems: (AppSidebarItemData & { shouldRender: boolean })[];
};

export function withDockItems<P extends WithDockItemsProps>(WrappedComponent: React.ComponentType<P>) {
  const ComponentWithDockItems = observer(function ComponentWithDockItems(props: Omit<P, keyof WithDockItemsProps>) {
    const { workspaceSlug } = useParams();
    const slug = workspaceSlug?.toString() ?? "";
    const { isDocsPath, isGalileoPath, isWorkPath } = useWorkspacePaths();
    const { config } = useInstance();
    const { allowPermissions } = useUserPermissions();
    const { t } = useTranslation();

    const canUseAi = allowPermissions(
      [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
      EUserPermissionsLevel.WORKSPACE,
      slug
    );
    const showGalileo = canUseGalileo(!!config?.has_llm_configured, canUseAi);

    const dockItems: (AppSidebarItemData & { shouldRender: boolean })[] = [
      {
        label: t("sidebar.work"),
        icon: <WorkItemsOutline className="size-5" />,
        href: `/${slug}`,
        isActive: isWorkPath,
        shouldRender: !!slug,
      },
      {
        label: t("sidebar.procedure"),
        icon: <DocumentationOutline className="size-5" />,
        href: `/${slug}/docs`,
        isActive: isDocsPath,
        shouldRender: !!slug,
      },
      {
        label: t("sidebar.galileo"),
        icon: <AiStar1Outline className="size-5" />,
        href: `/${slug}/galileo`,
        isActive: isGalileoPath,
        shouldRender: !!slug && showGalileo,
      },
    ];

    return <WrappedComponent {...(props as P)} dockItems={dockItems} />;
  });

  return ComponentWithDockItems;
}
```

- [ ] **Step 5: Remove the undock action from the rail**

Replace `apps/web/core/components/navigation/app-rail-root.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

"use client";

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { SettingsOutline, TickOutline } from "@makeplane/propel/icons";
import { ContextMenu } from "@plane/propel/context-menu";
import { cn } from "@plane/utils";
// components
import { AppSidebarItem } from "@/components/sidebar/sidebar-item";
// hooks
import { useAppRailPreferences } from "@/hooks/use-navigation-preferences";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
// local imports
import { AppSidebarItemsRoot } from "./items-root";

export const AppRailRoot = observer(function AppRailRoot() {
  // router
  const { workspaceSlug } = useParams();
  // preferences
  const { preferences, updateDisplayMode } = useAppRailPreferences();
  const { isSettingsPath } = useWorkspacePaths();
  // derived values
  const showLabel = preferences.displayMode === "icon_with_label";
  const railWidth = showLabel ? "3.75rem" : "3rem";

  return (
    <div
      className="z-[26] h-full flex-shrink-0 bg-canvas transition-all duration-300 ease-in-out"
      style={{
        width: railWidth,
        display: "block",
      }}
    >
      <ContextMenu>
        <ContextMenu.Trigger className="h-full">
          <div className="flex h-full flex-col gap-4 px-2 py-3">
            <div
              className={cn("flex flex-col", {
                "gap-4": showLabel,
                "gap-3": !showLabel,
              })}
            >
              <AppSidebarItemsRoot showLabel={showLabel} />
              <div className="mx-2 border-t border-strong" />
              <AppSidebarItem
                item={{
                  label: "Settings",
                  icon: <SettingsOutline className="size-5" />,
                  href: `/${workspaceSlug}/settings`,
                  isActive: isSettingsPath,
                  showLabel,
                }}
              />
            </div>
          </div>
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Content positionerClassName="z-30" className="outline-none">
            <ContextMenu.Item onClick={() => updateDisplayMode("icon_only")}>
              <div className="flex w-full items-center justify-between gap-2">
                <span className="text-11">Icon only</span>
                {preferences.displayMode === "icon_only" && <TickOutline className="size-3.5" />}
              </div>
            </ContextMenu.Item>
            <ContextMenu.Item onClick={() => updateDisplayMode("icon_with_label")}>
              <div className="flex w-full items-center justify-between gap-2">
                <span className="text-11">Icon with name</span>
                {preferences.displayMode === "icon_with_label" && <TickOutline className="size-3.5" />}
              </div>
            </ContextMenu.Item>
          </ContextMenu.Content>
        </ContextMenu.Portal>
      </ContextMenu>
    </div>
  );
});
```

- [ ] **Step 6: Add `aria-current` to rail links**

In `apps/web/core/components/sidebar/sidebar-item.tsx`:

Add `isActive?: boolean;` to `AppSidebarLinkItemProps`:

```tsx
interface AppSidebarLinkItemProps {
  href?: string;
  isActive?: boolean;
  children: React.ReactNode;
  className?: string;
}
```

Update `AppSidebarLinkItem`:

```tsx
function AppSidebarLinkItem({ href, isActive, children, className }: AppSidebarLinkItemProps) {
  if (!href) return null;

  return (
    <Link href={href} aria-current={isActive ? "page" : undefined} className={cn(styles.base, className)}>
      {children}
    </Link>
  );
}
```

Update the link branch in `AppSidebarItem`:

```tsx
if (variant === "link") {
  return (
    <AppSidebarLinkItem href={href} isActive={isActive}>
      {commonItems}
    </AppSidebarLinkItem>
  );
}
```

- [ ] **Step 7: Verify types and lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/navigation core/hooks/use-workspace-paths.ts core/lib/app-rail core/components/sidebar/sidebar-item.tsx
```

Expected: both pass. If `useWorkspacePaths` reports `pathname` possibly null, use `pathname ?? ""` in the `.includes` calls.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/lib/app-rail/types.ts apps/web/core/lib/app-rail/provider.tsx \
  "apps/web/app/(all)/[workspaceSlug]/layout.tsx" apps/web/core/hooks/use-workspace-paths.ts \
  apps/web/core/components/navigation/app-rail-hoc.tsx \
  apps/web/core/components/navigation/app-rail-root.tsx \
  apps/web/core/components/sidebar/sidebar-item.tsx
git commit -m "feat(web): enable workspace rail with work, procedure, and galileo"
```

---

### Task 5: Hide the right AI sidebar on Galileo and add the mobile mode menu

**Files:**

- Modify: `apps/web/core/components/workspace/content-wrapper.tsx`
- Modify: `apps/web/core/components/navigation/top-navigation-root.tsx`
- Create: `apps/web/core/components/navigation/workspace-mode-menu.tsx`

- [ ] **Step 1: Hide the right assistant on Galileo**

In `apps/web/core/components/workspace/content-wrapper.tsx`, add the hook import:

```tsx
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
```

Inside `WorkspaceContentWrapper`, after `isMobileViewport`, add:

```tsx
const { isGalileoPath } = useWorkspacePaths();
```

Replace:

```tsx
<AiAssistantSidebar />
```

with:

```tsx
{
  !isGalileoPath && <AiAssistantSidebar />;
}
```

- [ ] **Step 2: Create the mobile mode menu**

Create `apps/web/core/components/navigation/workspace-mode-menu.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Fragment } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { Menu, Transition } from "@headlessui/react";
import { AiStar1Outline, ChevronDownOutline, DocumentationOutline, WorkItemsOutline } from "@makeplane/propel/icons";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
import { useInstance } from "@/hooks/store/use-instance";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
import { canUseGalileo } from "@/lib/galileo-routing";

export const WorkspaceModeMenu = observer(function WorkspaceModeMenu() {
  // router
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const router = useRouter();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { config } = useInstance();
  const { allowPermissions } = useUserPermissions();
  const { isDocsPath, isGalileoPath, isWorkPath } = useWorkspacePaths();
  // derived values
  const canUseAi = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.WORKSPACE,
    slug
  );
  const showGalileo = canUseGalileo(!!config?.has_llm_configured, canUseAi);

  if (!slug) return null;

  const items = [
    {
      key: "work",
      label: t("sidebar.work"),
      icon: WorkItemsOutline,
      href: `/${slug}`,
      isActive: isWorkPath,
    },
    {
      key: "procedure",
      label: t("sidebar.procedure"),
      icon: DocumentationOutline,
      href: `/${slug}/docs`,
      isActive: isDocsPath,
    },
    ...(showGalileo
      ? [
          {
            key: "galileo",
            label: t("sidebar.galileo"),
            icon: AiStar1Outline,
            href: `/${slug}/galileo`,
            isActive: isGalileoPath,
          },
        ]
      : []),
  ];

  const activeItem = items.find((item) => item.isActive) ?? items[0];
  if (!activeItem) return null;
  const ActiveIcon = activeItem.icon;

  return (
    <Menu as="div" className="relative md:hidden">
      <Menu.Button
        type="button"
        aria-label={`${t("assistant.galileo.switch_mode")}: ${activeItem.label}`}
        title={activeItem.label}
        className="flex size-8 items-center justify-center gap-0.5 rounded-md text-secondary transition-colors hover:bg-layer-1-hover focus:outline-none"
      >
        <ActiveIcon className="size-4.5" />
        <ChevronDownOutline className="size-3 text-tertiary" />
      </Menu.Button>
      <Transition
        as={Fragment}
        enter="transition ease-out duration-100"
        enterFrom="transform opacity-0 scale-95"
        enterTo="transform opacity-100 scale-100"
        leave="transition ease-in duration-75"
        leaveFrom="transform opacity-100 scale-100"
        leaveTo="transform opacity-0 scale-95"
      >
        <Menu.Items className="absolute right-0 z-30 mt-1 w-44 origin-top-right rounded-md border border-subtle bg-surface-1 p-1 shadow-raised-200 focus:outline-none">
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <Menu.Item key={item.key}>
                {({ active }) => (
                  <button
                    type="button"
                    onClick={() => router.push(item.href)}
                    className={cn("flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-12", {
                      "bg-layer-1 text-primary": item.isActive,
                      "text-secondary hover:bg-layer-1-hover": !item.isActive,
                      "bg-layer-1-hover": active && !item.isActive,
                    })}
                  >
                    <Icon className="size-4 flex-shrink-0" />
                    <span className="truncate">{item.label}</span>
                  </button>
                )}
              </Menu.Item>
            );
          })}
        </Menu.Items>
      </Transition>
    </Menu>
  );
});
```

- [ ] **Step 3: Integrate the menu and hide the AI toggle on Galileo**

In `apps/web/core/components/navigation/top-navigation-root.tsx`:

Add imports:

```tsx
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
import { WorkspaceModeMenu } from "./workspace-mode-menu";
```

Inside `TopNavigationRoot`, after `const { t } = useTranslation();`, add:

```tsx
const { isGalileoPath } = useWorkspacePaths();
```

Replace:

```tsx
{
  config?.has_llm_configured && <AiAssistantSidebarToggle />;
}
```

with:

```tsx
<WorkspaceModeMenu />;
{
  config?.has_llm_configured && !isGalileoPath && <AiAssistantSidebarToggle />;
}
```

- [ ] **Step 4: Verify types and lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/navigation/workspace-mode-menu.tsx \
  core/components/navigation/top-navigation-root.tsx core/components/workspace/content-wrapper.tsx
```

Expected: both pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/navigation/workspace-mode-menu.tsx \
  apps/web/core/components/navigation/top-navigation-root.tsx \
  apps/web/core/components/workspace/content-wrapper.tsx
git commit -m "feat(web): add mobile mode menu and hide assistant on galileo"
```

---

### Task 6: Galileo store methods

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.test.ts`
- Modify: `apps/web/core/store/ai-assistant.store.ts`

- [ ] **Step 1: Add the missing service mock**

In `apps/web/core/store/ai-assistant.store.test.ts`, inside `makeServices().conversations`, add after `listMessages`:

```ts
retrieve: vi.fn(async (_slug: string, id: string) => conversation(id, "classic")),
```

- [ ] **Step 2: Write the failing store tests**

Add this new `describe` block at the end of `apps/web/core/store/ai-assistant.store.test.ts`:

```ts
describe("galileo chat", () => {
  it("sendGalileoMessage uses the classic endpoint without changing the selected mode", async () => {
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "classic ok"),
          conversation: conversation("c-galileo", "classic", { title: data.prompt }),
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    store.newChat();

    await store.sendGalileoMessage("hello galileo");

    expect(services.ai.createGptTask).toHaveBeenCalledTimes(1);
    expect(services.ai.createAgentTask).not.toHaveBeenCalled();
    expect(services.conversations.create).toHaveBeenCalledWith("acme", "classic");
    expect(store.mode).toBe("agent");
    expect(store.messages.at(-2)?.content).toBe("hello galileo");
    expect(store.messages.at(-1)?.content).toBe("classic ok");
  });

  it("sendGalileoMessage does not switch to agent for incident commands", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.newChat();

    await store.sendGalileoMessage("/incident OPS-123");

    expect(services.ai.createAgentTask).not.toHaveBeenCalled();
    expect(services.ai.createGptTask).toHaveBeenCalledTimes(1);
    expect(store.messages.at(-2)?.content).toBe("/incident OPS-123");
  });

  it("openConversationById retrieves conversations outside the list", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => []),
        retrieve: vi.fn(async () => conversation("c-deep", "classic")),
        listMessages: vi.fn(async () => [storedMessage("m1", "user", "deep")]),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    await expect(store.openConversationById("c-deep")).resolves.toBe("opened");
    expect(services.conversations.retrieve).toHaveBeenCalledWith("acme", "c-deep");
    expect(store.activeConversationId).toBe("c-deep");
    expect(store.messages.map((message) => message.content)).toEqual(["deep"]);
  });

  it("openConversationById treats agent conversations as not found", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => [conversation("c-agent", "agent")]),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    await expect(store.openConversationById("c-agent")).resolves.toBe("not_found");
  });

  it("openConversationById maps 404 to not found and other failures to error", async () => {
    const notFound = makeServices({
      conversations: {
        list: vi.fn(async () => []),
        retrieve: vi.fn(async () => {
          throw { status: 404 };
        }),
      },
    });
    const notFoundStore = makeStore(notFound);
    notFoundStore.setWorkspace("acme");
    await flush();
    await expect(notFoundStore.openConversationById("missing")).resolves.toBe("not_found");

    const failed = makeServices({
      conversations: {
        list: vi.fn(async () => []),
        retrieve: vi.fn(async () => {
          throw { status: 500 };
        }),
      },
    });
    const failedStore = makeStore(failed);
    failedStore.setWorkspace("acme");
    await flush();
    await expect(failedStore.openConversationById("broken")).resolves.toBe("error");
  });

  it("records conversation list failures", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => {
          throw { status: 500 };
        }),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    expect(store.conversationsError).toBe(true);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run:

```bash
pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts
```

Expected: FAIL because `sendGalileoMessage`, `openConversationById`, and `conversationsError` do not exist.

- [ ] **Step 4: Extend the store interface and observables**

In `apps/web/core/store/ai-assistant.store.ts`:

Add to `IAIAssistantStore` after `conversationsLoading`:

```ts
conversationsError: boolean;
```

Change `openConversation`:

```ts
openConversation: (conversationId: string) => Promise<"opened" | "not_found" | "error">;
```

Add after `sendMessage`:

```ts
sendGalileoMessage: (question: string) => Promise<void>;
openConversationById: (conversationId: string) => Promise<"opened" | "not_found" | "error">;
```

Add the field next to `conversationsLoading`:

```ts
conversationsError = false;
```

Add to `makeObservable`:

```ts
conversationsError: observable.ref,
sendGalileoMessage: action,
openConversationById: action,
```

- [ ] **Step 5: Reset and track conversation list errors**

In `setWorkspace`, inside the `if (workspaceSlug !== this.workspaceSlug)` reset block, add:

```ts
this.conversationsError = false;
```

In `loadConversations`, set the flag before the request:

```ts
this.conversationsLoading = true;
this.conversationsError = false;
```

Replace the empty `catch` with:

```ts
} catch {
  if (seq === this.listSeq) {
    runInAction(() => {
      this.conversationsError = true;
    });
  }
}
```

- [ ] **Step 6: Make `openConversation` return a result**

Replace the existing `openConversation` method with:

```ts
openConversation = async (conversationId: string): Promise<"opened" | "not_found" | "error"> => {
  const slug = this.workspaceSlug;
  const conversation = this.conversations.find((candidate) => candidate.id === conversationId);
  if (!slug || !conversation) return "not_found";
  this.interactionSeq += 1;
  const seq = ++this.requestSeq;
  this.isGenerating = false;
  runInAction(() => {
    this.activeConversationId = conversationId;
    this.mode = conversation.mode;
    this.messages = [];
  });
  this.persistMode();
  try {
    const messages = await this.conversationsService.listMessages(slug, conversationId);
    if (seq !== this.requestSeq) return "error";
    runInAction(() => {
      this.messages = messages.map(toAiMessage);
      this.activeFlow = activeFlowFromMessages(this.messages);
    });
    this.persistActiveId();
    return "opened";
  } catch (error: any) {
    if (seq !== this.requestSeq) return "error";
    if (error?.status === 404) {
      this.touchList();
      runInAction(() => {
        this.conversations = this.conversations.filter((candidate) => candidate.id !== conversationId);
        this.activeConversationId = undefined;
        this.messages = [];
        this.activeFlow = undefined;
      });
      this.clearActiveId();
      return "not_found";
    }
    runInAction(() => {
      this.activeConversationId = undefined;
      this.messages = [];
      this.activeFlow = undefined;
    });
    return "error";
  }
};
```

- [ ] **Step 7: Add the Galileo send and deep-link open methods**

Add after `deleteConversation`:

```ts
sendGalileoMessage = async (question: string): Promise<void> => {
  const trimmed = question.trim();
  const slug = this.workspaceSlug;
  if (!trimmed || this.isGenerating || this.turnGuard || !slug) return;

  this.turnGuard = true;
  const turn = ++this.turnSeq;
  runInAction(() => {
    this.isGenerating = true;
  });

  try {
    this.interactionSeq += 1;
    const conversationId = await this.ensureConversation("classic");
    if (!conversationId) return;

    const tempId = uuidv4();
    runInAction(() => {
      this.messages.push({ id: tempId, role: "user", content: trimmed });
    });
    await this.request(trimmed, slug, conversationId, tempId, "classic");
  } finally {
    if (turn === this.turnSeq) {
      this.turnGuard = false;
      runInAction(() => {
        this.isGenerating = false;
      });
    }
  }
};

openConversationById = async (conversationId: string): Promise<"opened" | "not_found" | "error"> => {
  const slug = this.workspaceSlug;
  if (!slug) return "error";

  const known = this.conversations.find((candidate) => candidate.id === conversationId);
  if (known) {
    if (known.mode !== "classic") return "not_found";
    return this.openConversation(conversationId);
  }

  try {
    const conversation = await this.conversationsService.retrieve(slug, conversationId);
    if (slug !== this.workspaceSlug) return "error";
    if (conversation.mode !== "classic") return "not_found";
    runInAction(() => {
      this.touchList();
      this.conversations = [
        conversation,
        ...this.conversations.filter((candidate) => candidate.id !== conversation.id),
      ];
    });
  } catch (error: any) {
    if (slug !== this.workspaceSlug) return "error";
    return error?.status === 404 ? "not_found" : "error";
  }

  return this.openConversation(conversationId);
};
```

- [ ] **Step 8: Thread the mode override through the private flow**

Replace the `ensureConversation` signature and body header:

```ts
private ensureConversation = async (mode: TAiAssistantMode = this.mode): Promise<string | undefined> => {
  const slug = this.workspaceSlug;
  if (!slug) return undefined;
  const active = this.conversations.find((candidate) => candidate.id === this.activeConversationId);
  if (active && active.mode === mode) return active.id;
  if (this.ensurePromise) return this.ensurePromise;
  const interaction = this.interactionSeq;
  const pending = this.createConversation(slug, interaction, mode).finally(() => {
    if (this.ensurePromise === pending) this.ensurePromise = undefined;
  });
  this.ensurePromise = pending;
  return pending;
};
```

Replace the `createConversation` signature:

```ts
private createConversation = async (
  slug: string,
  interaction: number,
  mode: TAiAssistantMode = this.mode
): Promise<string | undefined> => {
  try {
    const created = await this.conversationsService.create(slug, mode);
```

Replace the `request` signature and first line:

```ts
private async request(
  question: string,
  slug: string,
  conversationId: string,
  optimisticId: string,
  modeOverride?: TAiAssistantMode
) {
  const mode = modeOverride ?? this.mode;
```

Leave the rest of `request` unchanged.

- [ ] **Step 9: Run the store tests to verify they pass**

Run:

```bash
pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts
```

Expected: PASS. If a TypeScript-only lint complains about the `error: any` catches, match the existing file style (`catch (error: any)` is already used).

- [ ] **Step 10: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): add galileo chat store methods"
```

---

### Task 7: Galileo layout and history sidebar

**Files:**

- Create: `apps/web/core/components/galileo/layout.tsx`
- Create: `apps/web/core/components/galileo/history-sidebar.tsx`

- [ ] **Step 1: Create the layout shell and access gate**

Create `apps/web/core/components/galileo/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { createContext, Fragment, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { Dialog, Transition } from "@headlessui/react";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useInstance } from "@/hooks/store/use-instance";
import { useUserPermissions } from "@/hooks/store/user";
import { canUseGalileo } from "@/lib/galileo-routing";
import { GalileoHistorySidebar } from "./history-sidebar";

type TGallileoMobileHistoryContext = {
  openHistory: () => void;
};

const GalileoMobileHistoryContext = createContext<TGallileoMobileHistoryContext | undefined>(undefined);

export const useGalileoMobileHistory = (): TGallileoMobileHistoryContext => {
  const context = useContext(GalileoMobileHistoryContext);
  return context ?? { openHistory: () => {} };
};

export const GalileoLayout = observer(function GalileoLayout({ children }: { children: ReactNode }) {
  // router
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const router = useRouter();
  // store hooks
  const { config, isLoading } = useInstance();
  const { allowPermissions, getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  // local state
  const [historyOpen, setHistoryOpen] = useState(false);

  const role = getWorkspaceRoleByWorkspaceSlug(slug);
  const roleLoaded = role !== undefined;
  const canUseAi = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.WORKSPACE,
    slug
  );
  const allowed = canUseGalileo(!!config?.has_llm_configured, canUseAi);

  useEffect(() => {
    if (isLoading || !roleLoaded) return;
    if (!allowed) router.replace(`/${slug}`);
  }, [allowed, isLoading, roleLoaded, router, slug]);

  const contextValue = useMemo(() => ({ openHistory: () => setHistoryOpen(true) }), []);

  if (isLoading || !roleLoaded || !allowed) return null;

  return (
    <GalileoMobileHistoryContext.Provider value={contextValue}>
      <div className="relative flex size-full overflow-hidden rounded-lg border border-subtle bg-surface-1">
        <GalileoHistorySidebar className="hidden md:flex" />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">{children}</div>
      </div>

      <Transition.Root show={historyOpen} as={Fragment}>
        <Dialog as="div" className="relative z-50 md:hidden" onClose={() => setHistoryOpen(false)}>
          <Transition.Child
            as={Fragment}
            enter="ease-out duration-200"
            enterFrom="opacity-0"
            enterTo="opacity-100"
            leave="ease-in duration-150"
            leaveFrom="opacity-100"
            leaveTo="opacity-0"
          >
            <div className="fixed inset-0 bg-backdrop" />
          </Transition.Child>
          <Transition.Child
            as={Fragment}
            enter="ease-out duration-200"
            enterFrom="-translate-x-full"
            enterTo="translate-x-0"
            leave="ease-in duration-150"
            leaveFrom="translate-x-0"
            leaveTo="-translate-x-full"
          >
            <Dialog.Panel className="fixed inset-y-0 left-0 flex w-[17.5rem] max-w-[85vw] flex-col bg-surface-1 shadow-raised-200">
              <GalileoHistorySidebar className="flex" onNavigate={() => setHistoryOpen(false)} />
            </Dialog.Panel>
          </Transition.Child>
        </Dialog>
      </Transition.Root>
    </GalileoMobileHistoryContext.Provider>
  );
});
```

- [ ] **Step 2: Create the history sidebar**

Create `apps/web/core/components/galileo/history-sidebar.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { formatDistanceToNow } from "date-fns";
import { AddOutline, CheckDoneOutline, CloseOutline, MoreHorizontalOutline } from "@makeplane/propel/icons";
import { Button } from "@plane/propel/button";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { groupConversationsByDate, type TGallileoHistoryGroup } from "@/lib/galileo-history";
import { buildGalileoConversationHref, buildGalileoHref } from "@/lib/galileo-routing";

const GROUP_ORDER: TGallileoHistoryGroup[] = ["today", "yesterday", "previous7Days", "older"];

const GROUP_LABEL_KEY: Record<TGallileoHistoryGroup, string> = {
  today: "assistant.galileo.today",
  yesterday: "assistant.galileo.yesterday",
  previous7Days: "assistant.galileo.previous_7_days",
  older: "assistant.galileo.older",
};

const formatUpdatedAt = (value: string): string => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return formatDistanceToNow(date, { addSuffix: true });
};

type Props = {
  className?: string;
  onNavigate?: () => void;
};

export const GalileoHistorySidebar = observer(function GalileoHistorySidebar({ className, onNavigate }: Props) {
  // router
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const router = useRouter();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const {
    conversations,
    conversationsError,
    conversationsLoading,
    activeConversationId,
    isGenerating,
    newChat,
    renameConversation,
    deleteConversation,
    loadConversations,
  } = useAiAssistant();
  // local state
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [renameErrorId, setRenameErrorId] = useState<string | null>(null);
  const [actionsOpenId, setActionsOpenId] = useState<string | null>(null);
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [deleteErrorId, setDeleteErrorId] = useState<string | null>(null);
  const confirmDeleteRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (confirmingDeleteId) confirmDeleteRef.current?.focus();
  }, [confirmingDeleteId]);

  const classicConversations = conversations.filter((conversation) => conversation.mode === "classic");
  const groups = groupConversationsByDate(classicConversations);
  const hasConversations = classicConversations.length > 0;

  const handleNewChat = () => {
    newChat();
    router.push(buildGalileoHref(slug));
    onNavigate?.();
  };

  const openConversation = (conversationId: string) => {
    setActionsOpenId(null);
    router.push(buildGalileoConversationHref(slug, conversationId));
    onNavigate?.();
  };

  const startRename = (conversationId: string, title: string) => {
    setRenamingId(conversationId);
    setRenameValue(title);
    setRenameErrorId(null);
    setActionsOpenId(null);
  };

  const cancelRename = () => {
    setRenamingId(null);
    setRenameErrorId(null);
  };

  const handleRename = async (conversationId: string) => {
    const saved = await renameConversation(conversationId, renameValue);
    if (saved) {
      cancelRename();
    } else {
      setRenameErrorId(conversationId);
    }
  };

  const handleDelete = async (conversationId: string) => {
    const removed = await deleteConversation(conversationId);
    if (removed) {
      setConfirmingDeleteId(null);
      setDeleteErrorId(null);
      if (conversationId === activeConversationId) router.push(buildGalileoHref(slug));
    } else {
      setDeleteErrorId(conversationId);
    }
  };

  return (
    <aside className={cn("w-[17.5rem] flex-shrink-0 flex-col border-r border-subtle bg-surface-1", className)}>
      <div className="flex flex-col gap-2 border-b border-subtle p-3">
        <h2 className="px-1 text-13 font-semibold text-primary">{t("sidebar.galileo")}</h2>
        <Button
          variant="secondary"
          size="xl"
          className="w-full justify-start"
          prependIcon={<AddOutline />}
          onClick={handleNewChat}
        >
          {t("assistant.galileo.new_chat")}
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {conversationsError && (
          <div className="mx-1 mb-2 rounded-md border border-danger-strong/40 bg-danger-subtle px-2 py-1.5 text-11 text-danger-primary">
            {t("assistant.galileo.load_failed")}{" "}
            <button type="button" className="font-medium underline" onClick={() => void loadConversations()}>
              {t("common.retry")}
            </button>
          </div>
        )}

        {conversationsLoading && !hasConversations && (
          <p className="px-2 py-3 text-12 text-tertiary">{t("common.loading")}…</p>
        )}
        {!conversationsLoading && !hasConversations && (
          <p className="px-2 py-3 text-12 text-tertiary">{t("assistant.galileo.empty_history")}</p>
        )}

        {GROUP_ORDER.map((group) => {
          const items = groups[group];
          if (items.length === 0) return null;

          return (
            <div key={group} className="mb-4 last:mb-0">
              <p className="px-2 pb-1 text-10 font-semibold tracking-wider text-placeholder uppercase">
                {t(GROUP_LABEL_KEY[group])}
              </p>
              <div className="flex flex-col gap-0.5">
                {items.map((conversation) => (
                  <div
                    key={conversation.id}
                    className={cn("group/item rounded-md", {
                      "bg-layer-1-hover": conversation.id === activeConversationId,
                    })}
                  >
                    {renamingId === conversation.id ? (
                      <div className="flex items-center gap-1 p-1">
                        <input
                          value={renameValue}
                          onChange={(event) => setRenameValue(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              event.preventDefault();
                              void handleRename(conversation.id);
                            } else if (event.key === "Escape") {
                              event.preventDefault();
                              cancelRename();
                            }
                          }}
                          // Autofocus is intentional: the user just opened inline rename.
                          // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus
                          autoFocus
                          aria-label={t("assistant.galileo.conversation_title")}
                          className="min-w-0 flex-1 rounded border border-subtle bg-transparent px-1.5 py-1 text-12 text-primary outline-none"
                        />
                        <button
                          type="button"
                          aria-label={t("common.save")}
                          onClick={() => void handleRename(conversation.id)}
                          className="text-secondary hover:text-primary"
                        >
                          <CheckDoneOutline className="size-3.5" />
                        </button>
                        <button
                          type="button"
                          aria-label={t("common.cancel")}
                          onClick={cancelRename}
                          className="text-secondary hover:text-primary"
                        >
                          <CloseOutline className="size-3.5" />
                        </button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          onClick={() => openConversation(conversation.id)}
                          className="min-w-0 flex-1 rounded-md px-2 py-1.5 text-left"
                        >
                          <span
                            className="block truncate text-12 text-primary"
                            title={conversation.title || t("assistant.galileo.new_chat")}
                          >
                            {conversation.title || t("assistant.galileo.new_chat")}
                          </span>
                          <span className="block text-10 text-tertiary">
                            {formatUpdatedAt(conversation.updated_at)}
                          </span>
                        </button>
                        <button
                          type="button"
                          disabled={isGenerating}
                          aria-label={t("assistant.galileo.conversation_actions")}
                          aria-expanded={actionsOpenId === conversation.id}
                          onClick={() => setActionsOpenId(actionsOpenId === conversation.id ? null : conversation.id)}
                          className="mr-1 flex size-6 flex-shrink-0 items-center justify-center rounded-md text-tertiary opacity-100 transition-opacity group-hover/item:opacity-100 hover:bg-layer-1-hover hover:text-primary focus:opacity-100 disabled:opacity-40 md:opacity-0 md:group-hover/item:opacity-100"
                        >
                          <MoreHorizontalOutline className="size-3.5" />
                        </button>
                      </div>
                    )}

                    {actionsOpenId === conversation.id && renamingId !== conversation.id && (
                      <div className="flex items-center gap-3 px-2 pb-1.5">
                        <button
                          type="button"
                          disabled={isGenerating}
                          onClick={() => startRename(conversation.id, conversation.title)}
                          className="text-11 text-secondary hover:text-primary disabled:opacity-40"
                        >
                          {t("assistant.galileo.rename")}
                        </button>
                        <button
                          type="button"
                          disabled={isGenerating}
                          onClick={() => {
                            setConfirmingDeleteId(conversation.id);
                            setActionsOpenId(null);
                          }}
                          className="text-11 text-danger-primary disabled:opacity-40"
                        >
                          {t("common.delete")}
                        </button>
                      </div>
                    )}

                    {confirmingDeleteId === conversation.id && (
                      <div className="flex items-center gap-2 px-2 pb-1.5">
                        <span className="text-11 text-secondary">{t("assistant.galileo.delete_confirm")}</span>
                        <button
                          type="button"
                          ref={confirmDeleteRef}
                          disabled={isGenerating}
                          onClick={() => void handleDelete(conversation.id)}
                          className="text-11 font-medium text-danger-primary disabled:opacity-40"
                        >
                          {t("common.delete")}
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmingDeleteId(null)}
                          className="text-11 text-tertiary hover:text-primary"
                        >
                          {t("common.cancel")}
                        </button>
                      </div>
                    )}

                    {renameErrorId === conversation.id && (
                      <p className="px-2 pb-1 text-10 text-danger-primary">{t("common.something_went_wrong")}</p>
                    )}
                    {deleteErrorId === conversation.id && (
                      <p className="px-2 pb-1 text-10 text-danger-primary">{t("common.something_went_wrong")}</p>
                    )}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </aside>
  );
});
```

- [ ] **Step 3: Verify types and lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/galileo
```

Expected: pass. If `formatUpdatedAt` returns a locale-dependent string in other locales, that is acceptable for v1 (date-fns uses the browser locale).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/galileo/layout.tsx \
  apps/web/core/components/galileo/history-sidebar.tsx
git commit -m "feat(web): add galileo layout and history sidebar"
```

---

### Task 8: Galileo chat page, messages, composer, and routes

**Files:**

- Modify: `apps/web/app/routes/core.ts`
- Create: `apps/web/app/(all)/[workspaceSlug]/(galileo)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/[conversationId]/page.tsx`
- Create: `apps/web/core/components/galileo/chat-page.tsx`
- Create: `apps/web/core/components/galileo/message.tsx`
- Create: `apps/web/core/components/galileo/composer.tsx`
- Create: `apps/web/core/components/galileo/empty-state.tsx`

- [ ] **Step 1: Create the message component**

Create `apps/web/core/components/galileo/message.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { AiStar1Outline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import type { TAiMessage } from "@/lib/ai-context";
import { sanitizeAssistantHtml } from "@/lib/ai-context";

type Props = {
  message: TAiMessage;
  showRetry: boolean;
  onRetry: () => void;
};

export function GalileoMessage({ message, showRetry, onRetry }: Props) {
  const { t } = useTranslation();

  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] rounded-2xl rounded-br-md bg-accent-primary px-3.5 py-2 text-13 leading-relaxed whitespace-pre-wrap text-on-color">
          {message.content}
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-w-0 gap-2.5">
      <span className="mt-0.5 flex size-6 flex-shrink-0 items-center justify-center rounded-md border border-subtle bg-layer-1">
        <AiStar1Outline className="size-3.5 text-accent-primary" />
      </span>
      <div className="min-w-0 flex-1">
        {message.isError ? (
          <div className="rounded-lg border border-danger-strong/50 bg-danger-subtle px-3 py-2 text-13 leading-relaxed whitespace-pre-wrap text-danger-primary">
            {message.content}
            {showRetry && (
              <button
                type="button"
                onClick={onRetry}
                className="mt-2 block text-12 font-medium underline transition-opacity hover:opacity-80"
              >
                {t("common.retry")}
              </button>
            )}
          </div>
        ) : (
          <div
            className="ai-prose w-full text-13 text-primary"
            dangerouslySetInnerHTML={{ __html: sanitizeAssistantHtml(message.content) }}
          />
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create the composer**

Create `apps/web/core/components/galileo/composer.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { SendOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";

type Props = {
  disabled: boolean;
  onSend: (question: string) => void;
};

export function GalileoComposer({ disabled, onSend }: Props) {
  const { t } = useTranslation();
  const [question, setQuestion] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 144)}px`;
  }, [question]);

  const handleSend = () => {
    const trimmed = question.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setQuestion("");
    textareaRef.current?.focus();
  };

  return (
    <div className="border-t border-subtle p-3">
      <div className="rounded-lg border border-subtle bg-layer-1 focus-within:border-strong">
        <textarea
          ref={textareaRef}
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              handleSend();
            }
          }}
          disabled={disabled}
          rows={1}
          aria-label={t("assistant.galileo.composer_placeholder")}
          placeholder={t("assistant.galileo.composer_placeholder")}
          className="max-h-36 w-full resize-none bg-transparent px-3.5 pt-2.5 pb-1 text-13 text-primary outline-none placeholder:text-tertiary disabled:opacity-60"
        />
        <div className="flex items-center gap-3 px-3.5 pb-2">
          <span className="hidden text-10 text-tertiary sm:inline">{t("assistant.galileo.composer_hint")}</span>
          <button
            type="button"
            onClick={handleSend}
            disabled={disabled || !question.trim()}
            aria-label={t("assistant.galileo.send")}
            className="ml-auto flex size-7 items-center justify-center rounded-full bg-accent-primary text-on-color transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            <SendOutline className="size-3.5" />
          </button>
        </div>
      </div>
      <p className="mt-2 text-center text-10 leading-snug text-placeholder">
        {t("assistant.galileo.third_party_notice")}
      </p>
    </div>
  );
}
```

- [ ] **Step 3: Create the empty state**

Create `apps/web/core/components/galileo/empty-state.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { AiStar1Outline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";

const SUGGESTIONS = [
  "assistant.galileo.suggestion_summarize",
  "assistant.galileo.suggestion_procedure",
  "assistant.galileo.suggestion_draft",
  "assistant.galileo.suggestion_explain",
] as const;

export function GalileoEmptyState({ onSelect }: { onSelect: (prompt: string) => void }) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-10 text-center">
      <span className="flex size-12 items-center justify-center rounded-xl border border-subtle bg-layer-1">
        <AiStar1Outline className="size-5 text-accent-primary" />
      </span>
      <h2 className="mt-4 text-16 font-semibold text-primary">{t("assistant.galileo.empty_title")}</h2>
      <p className="mt-1 max-w-md text-13 leading-relaxed text-tertiary">{t("assistant.galileo.empty_subtitle")}</p>
      <div className="mt-6 grid w-full max-w-md grid-cols-1 gap-2 sm:grid-cols-2">
        {SUGGESTIONS.map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => onSelect(t(key))}
            className="hover:border-strong rounded-lg border border-subtle bg-layer-1 px-3 py-2 text-left text-12 text-secondary transition-colors hover:text-primary"
          >
            {t(key)}
          </button>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Create the chat page**

Create `apps/web/core/components/galileo/chat-page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { HistoryOutline, NewChatOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { Button } from "@plane/propel/button";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { buildGalileoConversationHref, buildGalileoHref, shouldNavigateToNewConversation } from "@/lib/galileo-routing";
import { GalileoComposer } from "./composer";
import { GalileoEmptyState } from "./empty-state";
import { useGalileoMobileHistory } from "./layout";
import { GalileoMessage } from "./message";

type TLoadStatus = "idle" | "loading" | "not_found" | "error";

export const GalileoChatPage = observer(function GalileoChatPage() {
  // router
  const { workspaceSlug, conversationId: routeConversationId } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const conversationId = routeConversationId?.toString();
  const router = useRouter();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const store = useAiAssistant();
  const { messages, isGenerating, activeConversationId, conversations } = store;
  // mobile drawer
  const { openHistory } = useGalileoMobileHistory();
  // local state
  const [status, setStatus] = useState<TLoadStatus>(conversationId ? "loading" : "idle");
  const [hasSent, setHasSent] = useState(false);
  const [isPinned, setIsPinned] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  const activeConversation = conversations.find((conversation) => conversation.id === activeConversationId);
  const title = activeConversation?.title || t("assistant.galileo.new_chat");

  const loadConversation = useCallback(async () => {
    if (!conversationId) return;
    if (activeConversationId === conversationId) {
      setStatus("idle");
      return;
    }
    setStatus("loading");
    const result = await store.openConversationById(conversationId);
    if (result === "opened") setStatus("idle");
    else if (result === "not_found") setStatus("not_found");
    else setStatus("error");
  }, [activeConversationId, conversationId, store]);

  useEffect(() => {
    if (conversationId) return;
    store.newChat();
    setStatus("idle");
  }, [conversationId, store]);

  useEffect(() => {
    if (!conversationId) return;
    void loadConversation();
  }, [conversationId, loadConversation]);

  useEffect(() => {
    if (!hasSent) return;
    if (!shouldNavigateToNewConversation(conversationId, activeConversationId)) return;
    if (!activeConversationId) return;
    router.replace(buildGalileoConversationHref(slug, activeConversationId));
  }, [activeConversationId, conversationId, hasSent, router, slug]);

  useEffect(() => {
    if (!isPinned) return;
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
  }, [isGenerating, isPinned, messages]);

  const handleScroll = () => {
    const element = scrollRef.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    setIsPinned(distance < 48);
  };

  const handleSend = async (question: string) => {
    setHasSent(true);
    setIsPinned(true);
    await store.sendGalileoMessage(question);
    if (conversationId && !store.activeConversationId) {
      router.replace(buildGalileoHref(slug));
    }
  };

  const handleNewChat = () => {
    store.newChat();
    router.push(buildGalileoHref(slug));
  };

  const scrollToBottom = () => {
    setIsPinned(true);
    const element = scrollRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  };

  const renderBody = () => {
    if (status === "loading") {
      return (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-12 text-tertiary">{t("common.loading")}…</p>
        </div>
      );
    }

    if (status === "not_found") {
      return (
        <div className="flex flex-1 flex-col items-center justify-center px-6 text-center">
          <h2 className="text-16 font-semibold text-primary">{t("assistant.galileo.not_found_title")}</h2>
          <p className="mt-1 max-w-md text-13 text-tertiary">{t("assistant.galileo.not_found_body")}</p>
          <Button variant="secondary" size="xl" className="mt-4" onClick={handleNewChat}>
            {t("assistant.galileo.start_new_chat")}
          </Button>
        </div>
      );
    }

    if (status === "error") {
      return (
        <div className="flex flex-1 flex-col items-center justify-center px-6 text-center">
          <p className="text-13 text-tertiary">{t("common.something_went_wrong")}</p>
          <Button variant="secondary" size="xl" className="mt-4" onClick={() => void loadConversation()}>
            {t("common.retry")}
          </Button>
        </div>
      );
    }

    if (messages.length === 0 && !isGenerating) {
      return <GalileoEmptyState onSelect={(prompt) => void handleSend(prompt)} />;
    }

    return (
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        role="log"
        aria-label={t("sidebar.galileo")}
        aria-live="polite"
        className="flex flex-1 flex-col gap-4 overflow-y-auto px-4 py-4 md:px-6"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
          {messages.map((message, index) => (
            <GalileoMessage
              key={message.id}
              message={message}
              showRetry={
                message.isError &&
                index === messages.length - 1 &&
                !isGenerating &&
                messages.some((candidate) => candidate.role === "user")
              }
              onRetry={() => void store.retryLast()}
            />
          ))}
          {isGenerating && (
            <div className="flex items-center gap-2 text-11 text-tertiary">
              <span className="flex size-6 items-center justify-center rounded-md border border-subtle bg-layer-1">
                <span className="size-1.5 rounded-full bg-accent-primary motion-safe:animate-pulse" />
              </span>
              {t("assistant.galileo.typing")}
            </div>
          )}
          {!isPinned && (
            <button
              type="button"
              onClick={scrollToBottom}
              className="sticky bottom-0 mx-auto rounded-full border border-subtle bg-surface-1 px-3 py-1 text-11 text-secondary shadow-raised-100 hover:text-primary"
            >
              {t("assistant.galileo.scroll_to_bottom")}
            </button>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
      <header className="flex h-12 flex-shrink-0 items-center gap-2 border-b border-subtle px-3">
        <button
          type="button"
          onClick={openHistory}
          aria-label={t("assistant.galileo.history")}
          className="flex size-8 items-center justify-center rounded-md text-secondary hover:bg-layer-1-hover hover:text-primary md:hidden"
        >
          <HistoryOutline className="size-4" />
        </button>
        <h1 className="min-w-0 flex-1 truncate text-13 font-semibold text-primary">{title}</h1>
        <Tooltip label={t("assistant.galileo.new_chat")} side="bottom">
          <button
            type="button"
            onClick={handleNewChat}
            aria-label={t("assistant.galileo.new_chat")}
            className="flex size-8 items-center justify-center rounded-md text-secondary hover:bg-layer-1-hover hover:text-primary"
          >
            <NewChatOutline className="size-4" />
          </button>
        </Tooltip>
      </header>

      {renderBody()}

      <div className={cn("mx-auto w-full max-w-3xl", status !== "idle" ? "hidden" : "")}>
        <GalileoComposer
          disabled={isGenerating || status !== "idle"}
          onSend={(question) => void handleSend(question)}
        />
      </div>
    </div>
  );
});
```

- [ ] **Step 5: Register the routes and create the route files**

In `apps/web/app/routes/core.ts`, after the Docs section (the `layout("./(all)/[workspaceSlug]/(docs)/layout.tsx", ...)` block and before the closing `]),` of the workspace layout), add:

```ts
      // ====================================================================
      // GALILEO SECTION
      // ====================================================================
      layout("./(all)/[workspaceSlug]/(galileo)/layout.tsx", [
        route(":workspaceSlug/galileo", "./(all)/[workspaceSlug]/(galileo)/galileo/page.tsx"),
        route(
          ":workspaceSlug/galileo/:conversationId",
          "./(all)/[workspaceSlug]/(galileo)/galileo/[conversationId]/page.tsx"
        ),
      ]),
```

Create `apps/web/app/(all)/[workspaceSlug]/(galileo)/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
import { GalileoLayout } from "@/components/galileo/layout";

export default function GalileoSectionLayout() {
  return (
    <GalileoLayout>
      <Outlet />
    </GalileoLayout>
  );
}
```

Create `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { GalileoChatPage } from "@/components/galileo/chat-page";

export default function GalileoIndexPage() {
  return <GalileoChatPage />;
}
```

Create `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/[conversationId]/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { GalileoChatPage } from "@/components/galileo/chat-page";

export default function GalileoConversationPage() {
  return <GalileoChatPage />;
}
```

- [ ] **Step 6: Verify types and lint**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/galileo app/routes/core.ts "app/(all)/[workspaceSlug]/(galileo)"
```

Expected: pass. The composer wrapper must stay hidden while loading/not-found; if the composer jumps, adjust the wrapper classes but keep the composer outside the scroll container.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/galileo apps/web/app/routes/core.ts \
  "apps/web/app/(all)/[workspaceSlug]/(galileo)"
git commit -m "feat(web): add galileo chat page and routes"
```

---

### Task 9: Translate all locales

**Files:**

- Modify: `packages/i18n/src/locales/<locale>/navigation.json` (all 19 non-English locales)
- Modify: `packages/i18n/src/locales/<locale>/assistant.json` (all 19 non-English locales)

- [ ] **Step 1: Load the translation skill**

Invoke the `translate` skill (`/translate`) and follow it exactly. This task touches every locale file, so the skill is mandatory.

- [ ] **Step 2: Add the navigation keys to every locale**

For every locale directory in `packages/i18n/src/locales/` except `en`, add the same three keys under `"sidebar"`:

```json
"work": "<translated>",
"procedure": "<translated>",
"galileo": "Galileo"
```

`Galileo` is a brand/product name and stays Latin in every locale.

- [ ] **Step 3: Add the Galileo assistant keys to every locale**

Mirror the full `assistant.galileo` object from Task 1 in every non-English `assistant.json`. Translate the values according to the translate skill's glossary and style rules. Keep these exactly as-is:

- `Galileo` (brand)
- `Shift+Enter`, `Enter` (key names)
- Markdown/URL placeholders, if any

- [ ] **Step 4: Run the sync check**

Run:

```bash
pnpm --filter @plane/i18n run sync:check
```

Expected: `All locales are in sync with English. No issues found.`

If any locale reports missing or stale keys, fix that locale and re-run until clean.

- [ ] **Step 5: Spot-check the Indonesian and German files**

Run:

```bash
node -e "const fs=require('fs'); for (const l of ['id','de']) { const a=JSON.parse(fs.readFileSync('packages/i18n/src/locales/'+l+'/assistant.json','utf8')); const n=JSON.parse(fs.readFileSync('packages/i18n/src/locales/'+l+'/navigation.json','utf8')); console.log(l, n.sidebar.galileo, n.sidebar.work, n.sidebar.procedure, a.assistant.galileo.new_chat); }"
```

Expected: four non-empty strings per locale; `Galileo` is unchanged.

- [ ] **Step 6: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "i18n: translate galileo rail and chat strings"
```

---

### Task 10: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run all web unit tests**

Run:

```bash
pnpm --filter=web test
```

Expected: PASS, including `workspace-modes.test.ts`, `galileo-routing.test.ts`, `galileo-history.test.ts`, and `ai-assistant.store.test.ts`.

- [ ] **Step 2: Run type checking**

Run:

```bash
pnpm --filter=web check:types
```

Expected: PASS.

- [ ] **Step 3: Run lint and formatting**

Run:

```bash
pnpm --filter=web exec oxlint core/components/galileo core/components/navigation core/lib core/store/ai-assistant.store.ts
pnpm --filter=web exec oxfmt --check core/components/galileo core/components/navigation core/lib core/store/ai-assistant.store.ts
pnpm --filter @plane/i18n run sync:check
```

Expected: 0 lint warnings/errors, formatting clean, locales in sync.

- [ ] **Step 4: Build the web app through turbo**

Run:

```bash
pnpm turbo run build --filter=web
```

Expected: build succeeds. Do not use a bare `pnpm --filter=web build`; it bypasses workspace dependency builds.

- [ ] **Step 5: Restart the production web service**

Run:

```bash
systemctl --user restart plane-web-prod.service
sleep 2
systemctl --user is-active plane-web-prod.service
```

Expected: `active`.

- [ ] **Step 6: Manual verification matrix**

Verify in the browser at `http://localhost:3000`:

- Desktop rail visible on Work, Docs, Galileo, and Settings; active item matches the page.
- Work opens the workspace home; Procedure opens the docs overview.
- Rail display mode toggle (icon only / icon with name) persists after reload.
- Galileo opens a new chat; the first message creates a conversation and the URL becomes `/galileo/<id>`.
- Refresh on `/galileo/<id>` restores messages.
- History groups render Today / Yesterday / Previous 7 days / Older; rename and delete work.
- Deleting the active conversation returns to `/galileo`.
- Invalid conversation ID shows the not-found state.
- Browser Back/Forward keeps the thread and active history item in sync.
- Right AI sidebar and top-nav toggle are absent on Galileo and return elsewhere.
- Mobile width: rail hidden; mode menu in top nav works; history opens as a drawer.
- LLM disabled: Galileo is absent from the rail/menu and a direct URL redirects to the workspace home.
- Keyboard-only: rail links, history actions, composer, and retry are reachable.

- [ ] **Step 7: Commit any verification fixes**

If the manual pass or build required code changes, stage only those files and commit:

```bash
git add <changed-files>
git commit -m "fix(web): polish galileo rail and chat"
```

If no fixes were needed, skip the commit.

---

## Self-Review Notes

- Spec coverage: rail (Task 4), mobile menu (Task 5), right-sidebar hiding (Task 5), store + deep links (Task 6), routes/layout/history (Task 7), chat/composer/empty/errors (Task 8), i18n (Tasks 1 and 9), verification (Task 10).
- The undock action is removed in Task 4, matching the approved spec.
- `sendGalileoMessage` forces classic mode through the new `modeOverride` parameter; the existing `sendMessage` behavior is unchanged.
- `openConversation` now returns `"opened" | "not_found" | "error"`; existing callers ignore the return value.
- `conversationsError` is added to the shared store so the history sidebar can offer retry.
- No backend changes are required.
