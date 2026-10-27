# Workspace Rail + Galileo Full Page — Design

- Date: 2026-10-27
- Status: Approved for implementation planning
- Area: `apps/web`

## 1. Problem

The workspace currently exposes one navigation surface: the Work sidebar. Documentation lives on a separate route with its own sidebar, and Galileo exists only as a right-side assistant panel. There is no shared switcher between these surfaces, and the existing App Rail (`AppRailRoot`) is implemented but disabled.

We want an outer rail on the far left with three product modes — **Work**, **Procedure**, and **Galileo** — where the inner sidebar changes with the selected mode. Galileo becomes a full chat page with its own conversation-history sidebar, similar to ChatGPT/Claude/Gemini.

## 2. Goals

- Enable a global workspace rail with Work, Procedure, Galileo, and Settings.
- Make Galileo a first-class, deep-linkable full-page chat experience.
- Reuse the existing AI conversations store, services, and backend endpoints.
- Keep the existing right-side Galileo assistant for non-Galileo pages.
- Work on desktop and mobile.

## 3. Non-goals (v1)

- Agent mode in the full page.
- File attachments, image input, streaming responses, or a stop-generation button.
- AI schedules/proposals UI in the full page.
- Conversation search, pinning, sharing, folders, or export.
- Backend/API changes.
- Replacing the right-side assistant globally.
- Bottom navigation bar on mobile.

## 4. Decisions

| Topic                      | Decision                                                                                           |
| -------------------------- | -------------------------------------------------------------------------------------------------- |
| Rail scope                 | Always visible on all workspace pages (desktop); no undock action.                                 |
| Rail style                 | Icon + label, 60px, using the existing rail preference (`icon_with_label` is already the default). |
| Work target                | Workspace home `/:workspaceSlug` (same destination as the old "Applications" rail item).           |
| Procedure target           | Static Docs at `/:workspaceSlug/docs` (defaults to the overview topic).                            |
| Galileo v1 scope           | Core chat only: new chat, history, open, rename, delete, send, markdown.                           |
| Right assistant on Galileo | Hidden, including its top-nav toggle.                                                              |
| LLM not configured         | Galileo rail item is hidden; direct URL redirects to workspace home.                               |
| Initial Galileo state      | New chat; conversations are deep-linkable at `/galileo/:conversationId`.                           |
| Settings                   | Stays at the bottom of the rail after a divider.                                                   |
| Mobile mode switch         | Menu in the top navigation; rail remains hidden.                                                   |
| Implementation approach    | Reuse `aiAssistantStore` + services, build a new page UI.                                          |

## 5. Information architecture

### Routes

| Surface                | Path                                                         |
| ---------------------- | ------------------------------------------------------------ |
| Work                   | `/:workspaceSlug` and all existing Work routes               |
| Procedure              | `/:workspaceSlug/docs` and `/:workspaceSlug/docs/:topicSlug` |
| Galileo (new chat)     | `/:workspaceSlug/galileo`                                    |
| Galileo (conversation) | `/:workspaceSlug/galileo/:conversationId`                    |
| Settings               | `/:workspaceSlug/settings` and nested workspace settings     |

New Galileo routes are registered in `apps/web/app/routes/core.ts` inside the workspace layout, outside the Projects and Docs layouts. The Galileo layout wraps both Galileo routes so the history sidebar does not remount when switching conversations.

### Rail items

Top group, in order:

1. **Work** — `WorkItemsOutline` icon, label `sidebar.work`, href `/:workspaceSlug`.
2. **Procedure** — `DocumentationOutline` icon, label `sidebar.procedure`, href `/:workspaceSlug/docs`.
3. **Galileo** — `AiStar1Outline` icon, label `sidebar.pi_chat` ("Galileo"), href `/:workspaceSlug/galileo`.

Divider.

4. **Settings** — existing `SettingsOutline` item in `AppRailRoot`, unchanged.

The old "Applications" dock item is removed. The rail context menu keeps the display-mode choice (icon only / icon + name). The existing "Undock App Rail" action is removed because the rail is now the primary mode switcher and hiding it would leave no way to switch modes; old collapsed values in localStorage are ignored so the rail is always visible when `isEnabled` is true.

### Active states

Centralize mode detection in a pure helper, `apps/web/core/lib/workspace-modes.ts`:

```ts
type WorkspaceMode = "work" | "procedure" | "galileo" | "settings" | "other";
getWorkspaceMode(pathname: string, workspaceSlug: string): WorkspaceMode;
```

Matching rules:

- `settings`: path is `/:slug/settings` or starts with `/:slug/settings/`.
- `procedure`: path is `/:slug/docs` or starts with `/:slug/docs/`.
- `galileo`: path is `/:slug/galileo` or starts with `/:slug/galileo/`.
- `work`: path is exactly `/:slug` or starts with `/:slug/`, excluding the three modes above.
- `other`: anything else.

A mode is active only for an exact segment match; prefix collisions such as `/slug/settings-archive` must not match. Match the segment boundary, not just `startsWith`.

`useWorkspacePaths` gains `isDocsPath` and `isGalileoPath`; `isProjectsPath` is corrected to exclude docs and Galileo (it currently includes docs).

### Visibility gating

Galileo is shown only when:

- `config.has_llm_configured` is true, and
- The current user has workspace role ADMIN or MEMBER (the AI endpoints reject GUEST via `guard_am`).

If the condition is false, the rail item and mobile menu item are not rendered. Direct navigation to `/galileo` or `/galileo/:id` redirects to `/:workspaceSlug` with `replace`.

## 6. Rail behavior

- `AppRailVisibilityProvider` in `apps/web/app/(all)/[workspaceSlug]/layout.tsx` is enabled by passing `isEnabled`.
- `WorkspaceContentWrapper` already hides the rail on mobile and adjusts content padding; keep that behavior.
- `AppRailRoot` keeps the 60px width, item spacing, context menu, and Settings entry.
- Rail visibility is `isEnabled` only. Remove the undock action and do not let a stored `APP_RAIL_*` collapsed value hide the rail; clear or ignore that legacy value.
- `app-rail-hoc.tsx` is rewritten to produce the three mode items with active flags from `getWorkspaceMode`.
- Rail links set `aria-current="page"` when active and keep labels visible. When the user switches to icon-only mode, tooltips remain available.
- The rail is hidden on fullscreen routes because the workspace layout already bypasses `WorkspaceContentWrapper` for them.

## 7. Mobile

- The rail stays hidden below the existing mobile breakpoint.
- `TopNavigationRoot` gets a compact mode menu for small screens, placed in the right-side action group. It lists Work, Procedure, and Galileo (if enabled), shows the active mode, and navigates on selection.
- On Galileo mobile:
  - The history sidebar becomes a left drawer opened by a history button in the chat header.
  - The drawer uses Headless UI `Dialog`/`Transition` (already used elsewhere in the app), closes on backdrop click, Escape, or after selecting a conversation.
  - The chat main area is full width.
- The right AI assistant toggle remains visible on non-Galileo pages.

## 8. Galileo page UX

### Desktop layout

The page fills the rounded workspace content panel, matching the Docs layout treatment (border + rounded corners):

```text
┌──────┬──────────────────────┬────────────────────────────────────┐
│ Rail │ History sidebar 280  │ Chat                                │
│ 60   │ New chat             │ Header: title      [history][new]  │
│      │ Today                │ Scrollable thread (max ~760px)      │
│      │  Conversation A      │                                    │
│      │ Yesterday            │                                    │
│      │  Conversation B      │ Composer (sticky bottom)            │
└──────┴──────────────────────┴────────────────────────────────────┘
```

### History sidebar

- Header: "Galileo" + full-width secondary "New chat" button with a plus icon.
- Conversations are filtered to `mode === "classic"` for v1 and sorted by `updated_at` descending (the API already returns this order).
- Grouped by date, computed client-side from `updated_at`: **Today**, **Yesterday**, **Previous 7 days**, **Older**.
- Each item:
  - Truncated title; fallback "New conversation" when the backend title is empty.
  - Relative update time.
  - Active highlight when it matches the URL conversation ID.
  - Hover/focus `...` menu with **Rename** and **Delete**.
- Rename: inline input in the item; Enter saves, Escape cancels; errors show inline without losing the input.
- Delete: inline confirmation ("Delete?" + cancel) with the confirm button focused; deleting the active conversation navigates to `/galileo`.
- States: loading skeleton with no items; empty state "Belum ada percakapan"; load failure banner with retry.

### Chat header

- Conversation title (truncated) for an existing conversation; "New chat" for the empty route.
- Desktop: New chat icon button on the right.
- Mobile: history button and New chat icon button.

### Empty state (new chat)

- Centered Galileo mark, heading, and one-line subtitle.
- Four clickable suggestion prompts (static, i18n keys). Clicking one sends it immediately.
- Composer remains focused and ready.

### Message thread

- Maximum content width ~760px, centered.
- User messages: right-aligned, accent-filled rounded bubble, `whitespace-pre-wrap`.
- Assistant messages: left-aligned, small Galileo avatar/mark, sanitized HTML prose (reuse the existing `sanitizeAssistantHtml` + `ai-prose` pattern from the right sidebar).
- Code blocks remain readable and scroll horizontally.
- Error messages: danger-subtle panel with a retry action for the last failed turn.
- Generating state: typing indicator; composer disabled.
- Scrolling: the thread auto-scrolls while pinned to the bottom. If the user scrolls up, auto-scroll pauses and a "Scroll to bottom" button appears.
- Accessibility: `role="log"`, `aria-live="polite"`, messages labelled by author.

### Composer

- Sticky at the bottom of the chat column, above the panel edge.
- Auto-growing textarea, 1–6 rows, then scroll.
- Enter sends; Shift+Enter inserts a newline.
- Send button disabled when empty, generating, or workspace slug missing.
- Hint text below the textarea: "Enter untuk kirim • Shift+Enter untuk baris baru".
- No stop button in v1 because the backend request is non-streaming and has no cancel endpoint.

### Deep links and state

- `/galileo` always starts a new chat:
  - On mount, call `newChat()` and clear the remembered active conversation.
  - The conversation is created only when the first message is sent.
  - After creation, replace the URL with `/galileo/:conversationId` so refresh restores it.
- `/galileo/:conversationId`:
  - Wait for the conversation list to load, then open the conversation.
  - If the ID is not in the list, fetch it directly via `retrieve` so deep links work beyond the 50-item list limit.
  - Invalid, foreign, deleted, or non-classic conversations show a "Percakapan tidak ditemukan" state with a "Mulai chat baru" button.
- Browser back/forward is route-driven; effects are idempotent and must not navigate in a loop.
- New chat from the sidebar navigates to `/galileo` (replace or push is acceptable; use push so Back returns to the previous conversation).

### Errors

- Conversation list failure: inline retry.
- Message load failure: inline retry; the conversation remains selected.
- Send failure: store already appends an error message; expose retry for the last failed turn.
- 404 during send: the store starts a new chat and shows the existing "conversation was deleted" message; navigate to `/galileo`.
- No LLM/guest: redirect (see gating).

## 9. Technical design

### Files

New:

- `apps/web/app/(all)/[workspaceSlug]/(galileo)/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/page.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(galileo)/galileo/[conversationId]/page.tsx`
- `apps/web/core/components/galileo/layout.tsx`
- `apps/web/core/components/galileo/history-sidebar.tsx`
- `apps/web/core/components/galileo/chat.tsx`
- `apps/web/core/components/galileo/composer.tsx`
- `apps/web/core/components/galileo/message.tsx`
- `apps/web/core/components/galileo/empty-state.tsx`
- `apps/web/core/components/galileo/mobile-history-drawer.tsx`
- `apps/web/core/lib/workspace-modes.ts`
- `apps/web/core/lib/galileo-history.ts` (pure grouping helper)
- `apps/web/core/lib/galileo-routing.ts` (pure URL-sync and gating helpers)
- Tests: `apps/web/core/lib/workspace-modes.test.ts`, `apps/web/core/lib/galileo-history.test.ts`, `apps/web/core/lib/galileo-routing.test.ts`

Modified:

- `apps/web/app/routes/core.ts` — register the Galileo layout/routes.
- `apps/web/app/(all)/[workspaceSlug]/layout.tsx` — enable the rail.
- `apps/web/core/components/navigation/app-rail-hoc.tsx` — new items and active logic.
- `apps/web/core/components/navigation/top-navigation-root.tsx` — mobile mode menu; hide AI toggle on Galileo.
- `apps/web/core/components/workspace/content-wrapper.tsx` — hide the right AI sidebar on Galileo.
- `apps/web/core/hooks/use-workspace-paths.ts` — docs/Galileo flags; fix `isProjectsPath`.
- `apps/web/core/store/ai-assistant.store.ts` — Galileo send + deep-link open helpers.
- `packages/i18n/src/locales/en/navigation.json` — `sidebar.work`, `sidebar.procedure`.
- `packages/i18n/src/locales/en/assistant.json` — Galileo page strings.
- Other locale files per the repository translation policy.

### Store changes

Keep the existing behavior untouched for the right sidebar.

Add a public method:

```ts
sendGalileoMessage(question: string): Promise<void>;
```

Behavior:

- Trim and ignore empty input; guard `isGenerating`, `turnGuard`, and missing workspace slug.
- Force classic mode for this send without mutating the globally selected mode of the right sidebar.
- Skip incident-command and schedule-command parsing.
- Reuse the same optimistic user message, `ensureConversation`, and `request` flow.
- On success, `activeConversationId` is set and the page navigates to the conversation URL.

To support this, thread an optional mode override through the private `ensureConversation`, `createConversation`, and `request` methods, defaulting to `this.mode`. Existing callers are unchanged.

Add a public method:

```ts
openConversationById(conversationId: string): Promise<"opened" | "not_found">;
```

Behavior:

- If the conversation is in `conversations`, reuse `openConversation`.
- Otherwise call `conversationsService.retrieve(slug, id)`, insert the result into `conversations`, and load messages.
- Return `not_found` for 404 or a non-classic conversation (Galileo v1 only supports classic).
- Does not auto-create anything.

Existing actions used as-is: `loadConversations`, `newChat`, `renameConversation`, `deleteConversation`, `retryLast`.

Shared-state note: the right sidebar and Galileo page share one store. Opening a conversation or starting a new chat on Galileo also changes the store's active conversation. This is accepted for v1; the right sidebar remains functional because it reads the same state.

### Right sidebar hiding

`WorkspaceContentWrapper` computes `isGalileoPath` from the pathname and skips `<AiAssistantSidebar />`. `TopNavigationRoot` hides `<AiAssistantSidebarToggle />` on the same path. This is route-based; no theme-store changes are needed.

### Route/state synchronization

- The layout renders the sidebar and `<Outlet />`; the page owns URL synchronization.
- The route parameter is the single source of truth.
- With an ID: load the list if needed, call `openConversationById`, track local `loading`/`notFound` state. Skip if `activeConversationId === id`.
- Without an ID: call `newChat()` once on mount. Watch `activeConversationId`; when it becomes defined after the first send, `navigate(..., { replace: true })` to the conversation URL.
- Guard against loops by comparing the target ID with the current route param before navigating.
- The store's `interactionSeq` guards already prevent stale `loadConversations` calls from re-opening old conversations.

### Route registration sketch

```ts
layout("./(all)/[workspaceSlug]/(galileo)/layout.tsx", [
  route(":workspaceSlug/galileo", "./(all)/[workspaceSlug]/(galileo)/galileo/page.tsx"),
  route(
    ":workspaceSlug/galileo/:conversationId",
    "./(all)/[workspaceSlug]/(galileo)/galileo/[conversationId]/page.tsx"
  ),
]);
```

### i18n

Reuse `sidebar.pi_chat` for "Galileo". Add:

- `sidebar.work`: "Work"
- `sidebar.procedure`: "Procedure"
- `assistant.galileo.*`: new chat, history, empty state, suggestions, rename, delete, not found, retry, error, composer hint, loading.

All supported locales must receive the keys following `packages/i18n` translation policy. English remains the fallback.

### Accessibility

- Rail and mobile menu links expose the active state with `aria-current`.
- Icon-only rail mode keeps accessible names and tooltips.
- History item menu is keyboard reachable; rename input has a label; delete confirmation moves focus to the confirm button.
- Chat log uses `aria-live="polite"`; the composer textarea has a label; send button has an accessible name.
- Respect reduced-motion preferences; reuse the existing `ai-rise` animation and motion conventions.
- Focus returns to the composer after sending and to the history trigger after closing the mobile drawer.

## 10. Edge cases

- **No LLM or guest**: no rail/mobile-menu item; direct URL redirects to workspace home.
- **Invalid/deleted/foreign conversation ID**: not-found state with a new-chat CTA; do not silently create a conversation.
- **Agent-mode conversation URL**: treated as not found in Galileo v1.
- **Conversation older than the 50-item list**: still opens via direct retrieval.
- **Delete active conversation**: navigate to `/galileo` and start a new chat.
- **Send fails**: error message appears in the thread; retry available; conversation remains selected.
- **First send fails after conversation creation**: the empty conversation remains in history; this is acceptable for v1.
- **Refresh mid-generation**: the request is lost; the persisted user message and any stored assistant response are reloaded from the server.
- **Workspace switch while on Galileo**: the store resets via `setWorkspace`; the page reloads the list for the new workspace; if Galileo is disabled there, redirect to workspace home.
- **Rail display preference**: icon-only vs icon+label persists per workspace; rail visibility itself is not persisted and is always on for enabled routes.
- **Mobile drawer open then route change**: drawer closes on navigation.
- **Browser Back from a conversation to `/galileo`**: new-chat state is restored; no stale messages remain.

## 11. Testing

### Unit tests (Vitest, node environment)

- `workspace-modes.test.ts`: exact segment matching for home, docs, Galileo, settings, nested routes, trailing slashes, prefix collisions, and unknown paths.
- `galileo-history.test.ts`: date grouping boundaries for Today/Yesterday/Previous 7 days/Older, invalid timestamps, and stable ordering.
- `galileo-routing.test.ts`: URL synchronization decisions (new chat, existing ID, created ID, already-current ID) and gating (LLM/role).

No React rendering tests are required for v1 because the current web test setup is node-only. Use pure helpers for the logic that needs coverage.

### Manual verification matrix

- Rail appears on Work, Docs, Galileo, and Settings on desktop; active item is correct on each.
- Rail is hidden on mobile; the top-nav mode menu navigates to all three modes.
- Icon+label and icon-only modes both work; labels/tooltips are accessible.
- Work opens the workspace home; Procedure opens the docs overview.
- Galileo opens a new chat; first message creates a conversation and updates the URL.
- Refresh on `/galileo/:id` restores messages; invalid ID shows not found.
- History opens, renames, deletes, and navigates; deleting the active conversation starts a new chat.
- Browser Back/Forward keeps the sidebar and thread in sync.
- Right AI sidebar and toggle are absent on Galileo and return on other pages.
- LLM disabled and guest users cannot reach Galileo.
- Send error and retry; long thread scroll behavior; reduced motion.
- Keyboard-only pass: rail, mobile menu, history menu, rename, composer.

### Verification commands

```bash
pnpm --filter=web test
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/galileo core/lib/workspace-modes.ts core/lib/galileo-history.ts core/lib/galileo-routing.ts
pnpm turbo run build --filter=web
systemctl --user restart plane-web-prod.service
```

## 12. Acceptance criteria

- A 60px icon+label rail is visible on all desktop workspace pages with Work, Procedure, Galileo, and Settings.
- The inner sidebar changes correctly for Work, Procedure, and Galileo.
- Galileo is a full-page chat with a grouped history sidebar, deep links, rename/delete, and reliable send/retry.
- The right-side AI assistant is hidden only on Galileo.
- Mobile users can switch modes from the top navigation and open history in a drawer.
- No backend changes are required.
- Unit tests and the manual matrix pass.
