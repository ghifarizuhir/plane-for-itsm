# Service Graph Hover Focus Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open the service graph inspector on hover with intent delay, keep it open with click/tap pin, and animate card + node feedback using Framer Motion.

**Architecture:** A framework-free timer controller owns hover intent, close grace, pin precedence, and suppression. A small React hook adapts it to canvas state. React Flow events feed the controller; `AnimatePresence` + `motion` handle entry/exit/stagger, while the card's screen-space position remains instant during pan/zoom.

**Tech Stack:** React 19, TypeScript, React Flow (`@xyflow/react`), Framer Motion 12 (catalog), Vitest, Tailwind/Propel tokens.

---

## File Structure

- `apps/web/core/hooks/use-mobile-viewport.ts` — add hover-capable media query hook.
- `apps/web/core/components/services/graph/focus-intent.ts` (new) — timer/state controller, no React.
- `apps/web/core/components/services/graph/focus-intent.test.ts` (new) — fake-timer unit tests.
- `apps/web/core/components/services/graph/use-service-focus.ts` (new) — React adapter hook.
- `apps/web/core/components/services/graph/service-graph-canvas.tsx` — wire events + `AnimatePresence`.
- `apps/web/core/components/services/graph/service-focus-card.tsx` — motion entry/exit, stagger, pin indicator, pointer handlers.
- `apps/web/core/components/services/graph/service-node.tsx` — motion hover/tap.
- `apps/web/package.json` + `pnpm-lock.yaml` — explicit Framer Motion dependency.
- `apps/web/styles/globals.css` — remove old CSS card keyframes.

---

### Task 1: Framer Motion dependency + hover-capable pointer hook

**Files:**

- Modify: `apps/web/package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `apps/web/core/hooks/use-mobile-viewport.ts`

- [ ] **Step 1: Declare the dependency**

Add to `apps/web/package.json` dependencies, alphabetically after `export-to-csv`:

```json
"framer-motion": "catalog:",
```

Run: `pnpm install`

Expected: lockfile updates; `framer-motion` resolves to the workspace catalog version.

- [ ] **Step 2: Add the query and hook**

Append after the existing `useTouchPointer` export:

```ts
export const HOVER_POINTER_QUERY = "(hover: hover) and (pointer: fine)";

export const useHoverPointer = (): boolean => useMediaQuery(HOVER_POINTER_QUERY);
```

- [ ] **Step 3: Verify types**

Run: `pnpm --filter=web check:types`

Expected: PASS (no new errors).

- [ ] **Step 4: Commit**

```bash
git add apps/web/package.json pnpm-lock.yaml apps/web/core/hooks/use-mobile-viewport.ts
git commit -m "feat(web): add framer motion and hover pointer detection"
```

---

### Task 2: Focus intent controller tests

**Files:**

- Create: `apps/web/core/components/services/graph/focus-intent.test.ts`

- [ ] **Step 1: Write the failing test file**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
// local imports
import {
  createFocusIntentController,
  FOCUS_CLOSE_DELAY,
  FOCUS_OPEN_DELAY,
  type TServiceFocusState,
} from "./focus-intent";

const setup = () => {
  const states: TServiceFocusState[] = [];
  const controller = createFocusIntentController({
    onChange: (state) => states.push(state),
  });
  const latest = () => states[states.length - 1] ?? null;

  return { controller, latest, states };
};

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("focus intent controller", () => {
  it("opens only after the intent delay", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY - 1);
    expect(latest()).toBeNull();

    vi.advanceTimersByTime(1);
    expect(latest()).toEqual({
      hoveredServiceId: "service-a",
      pinnedServiceId: null,
      suppressed: false,
    });
  });

  it("cancels a pending open when the pointer leaves early", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY / 2);
    controller.leaveNode();
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY + FOCUS_CLOSE_DELAY);

    expect(latest()).toBeNull();
  });

  it("keeps the card open through the node-card gap and closes after the grace period", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    controller.leaveNode();

    vi.advanceTimersByTime(FOCUS_CLOSE_DELAY - 1);
    expect(latest()?.hoveredServiceId).toBe("service-a");

    controller.enterCard();
    vi.advanceTimersByTime(FOCUS_CLOSE_DELAY * 5);
    expect(latest()?.hoveredServiceId).toBe("service-a");

    controller.leaveCard();
    vi.advanceTimersByTime(FOCUS_CLOSE_DELAY);
    expect(latest()?.hoveredServiceId).toBeNull();
  });

  it("keeps a pinned card when hovering another node", () => {
    const { controller, latest } = setup();

    controller.pin("service-a");
    controller.hover("service-b");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY + FOCUS_CLOSE_DELAY);

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: "service-a",
      suppressed: false,
    });
  });

  it("moves the pin when another node is pinned", () => {
    const { controller, latest } = setup();

    controller.pin("service-a");
    controller.pin("service-b");

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: "service-b",
      suppressed: false,
    });
  });

  it("clears hover and pin timers on clear", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    controller.clear();
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY + FOCUS_CLOSE_DELAY);

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: null,
      suppressed: false,
    });
  });

  it("suppresses hover until resumed", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    controller.suppress();

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: null,
      suppressed: true,
    });

    controller.hover("service-b");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    expect(latest()?.hoveredServiceId).toBeNull();

    controller.resume();
    controller.hover("service-b");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    expect(latest()).toEqual({
      hoveredServiceId: "service-b",
      pinnedServiceId: null,
      suppressed: false,
    });
  });

  it("cancels pending timers on dispose", () => {
    const { controller, latest, states } = setup();

    controller.hover("service-a");
    controller.dispose();
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);

    expect(states).toHaveLength(0);
    expect(latest()).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm --filter=web exec vitest run core/components/services/graph/focus-intent.test.ts`

Expected: FAIL because `./focus-intent` does not exist.

---

### Task 3: Focus intent controller implementation

**Files:**

- Create: `apps/web/core/components/services/graph/focus-intent.ts`
- Test: `apps/web/core/components/services/graph/focus-intent.test.ts`

- [ ] **Step 1: Implement the controller**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const FOCUS_OPEN_DELAY = 180;
export const FOCUS_CLOSE_DELAY = 140;

export type TServiceFocusState = {
  hoveredServiceId: string | null;
  pinnedServiceId: string | null;
  suppressed: boolean;
};

type TFocusIntentControllerOptions = {
  openDelay?: number;
  closeDelay?: number;
  onChange: (state: TServiceFocusState) => void;
};

export type TFocusIntentController = {
  hover: (serviceId: string) => void;
  leaveNode: () => void;
  enterCard: () => void;
  leaveCard: () => void;
  pin: (serviceId: string) => void;
  clear: () => void;
  suppress: () => void;
  resume: () => void;
  dispose: () => void;
  getState: () => TServiceFocusState;
};

export const createFocusIntentController = ({
  openDelay = FOCUS_OPEN_DELAY,
  closeDelay = FOCUS_CLOSE_DELAY,
  onChange,
}: TFocusIntentControllerOptions): TFocusIntentController => {
  let state: TServiceFocusState = {
    hoveredServiceId: null,
    pinnedServiceId: null,
    suppressed: false,
  };
  let openTimer: ReturnType<typeof setTimeout> | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  const emit = () => onChange({ ...state });

  const clearOpenTimer = () => {
    if (openTimer !== null) {
      clearTimeout(openTimer);
      openTimer = null;
    }
  };

  const clearCloseTimer = () => {
    if (closeTimer !== null) {
      clearTimeout(closeTimer);
      closeTimer = null;
    }
  };

  const clearTimers = () => {
    clearOpenTimer();
    clearCloseTimer();
  };

  const scheduleClose = () => {
    clearCloseTimer();
    closeTimer = setTimeout(() => {
      closeTimer = null;
      if (state.pinnedServiceId) return;
      state = { ...state, hoveredServiceId: null };
      emit();
    }, closeDelay);
  };

  const hover = (serviceId: string) => {
    if (state.pinnedServiceId || state.suppressed) return;

    clearCloseTimer();

    if (state.hoveredServiceId === serviceId) return;

    clearOpenTimer();
    openTimer = setTimeout(() => {
      openTimer = null;
      if (state.pinnedServiceId || state.suppressed) return;
      state = { ...state, hoveredServiceId: serviceId };
      emit();
    }, openDelay);
  };

  const leaveNode = () => {
    if (state.pinnedServiceId) return;

    clearOpenTimer();

    if (!state.hoveredServiceId) {
      clearCloseTimer();
      return;
    }

    scheduleClose();
  };

  const enterCard = () => {
    clearCloseTimer();
  };

  const leaveCard = () => {
    if (state.pinnedServiceId) return;
    if (state.hoveredServiceId) scheduleClose();
  };

  const pin = (serviceId: string) => {
    clearTimers();
    if (state.pinnedServiceId === serviceId && state.hoveredServiceId === null) return;

    state = {
      hoveredServiceId: null,
      pinnedServiceId: serviceId,
      suppressed: state.suppressed,
    };
    emit();
  };

  const clear = () => {
    clearTimers();
    state = { ...state, hoveredServiceId: null, pinnedServiceId: null };
    emit();
  };

  const suppress = () => {
    clearTimers();
    state = { ...state, hoveredServiceId: null, suppressed: true };
    emit();
  };

  const resume = () => {
    if (!state.suppressed) return;
    state = { ...state, suppressed: false };
    emit();
  };

  const dispose = () => {
    clearTimers();
  };

  return {
    hover,
    leaveNode,
    enterCard,
    leaveCard,
    pin,
    clear,
    suppress,
    resume,
    dispose,
    getState: () => ({ ...state }),
  };
};
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `pnpm --filter=web exec vitest run core/components/services/graph/focus-intent.test.ts`

Expected: PASS (`8 tests`).

- [ ] **Step 3: Lint and format**

Run: `pnpm exec oxfmt apps/web/core/components/services/graph/focus-intent.ts apps/web/core/components/services/graph/focus-intent.test.ts && pnpm exec oxlint apps/web/core/components/services/graph/focus-intent.ts apps/web/core/components/services/graph/focus-intent.test.ts`

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/graph/focus-intent.ts apps/web/core/components/services/graph/focus-intent.test.ts
git commit -m "feat(web): add service focus intent controller"
```

---

### Task 4: React adapter hook

**Files:**

- Create: `apps/web/core/components/services/graph/use-service-focus.ts`

- [ ] **Step 1: Implement the hook**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
// local imports
import { createFocusIntentController, type TServiceFocusState } from "./focus-intent";

const INITIAL_STATE: TServiceFocusState = {
  hoveredServiceId: null,
  pinnedServiceId: null,
  suppressed: false,
};

export function useServiceFocusInteraction() {
  const [state, setState] = useState<TServiceFocusState>(INITIAL_STATE);
  const [controller] = useState(() => createFocusIntentController({ onChange: setState }));

  useEffect(
    () => () => {
      controller.dispose();
    },
    [controller]
  );

  return {
    ...state,
    effectiveServiceId: state.pinnedServiceId ?? state.hoveredServiceId,
    hover: controller.hover,
    leaveNode: controller.leaveNode,
    enterCard: controller.enterCard,
    leaveCard: controller.leaveCard,
    pin: controller.pin,
    clear: controller.clear,
    suppress: controller.suppress,
    resume: controller.resume,
  };
}
```

- [ ] **Step 2: Verify types**

Run: `pnpm --filter=web check:types`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/services/graph/use-service-focus.ts
git commit -m "feat(web): add service focus interaction hook"
```

---

### Task 5: Wire hover and pin into the canvas

**Files:**

- Modify: `apps/web/core/components/services/graph/service-graph-canvas.tsx`
- Modify: `apps/web/core/components/services/graph/service-focus-card.tsx`

- [ ] **Step 1: Add imports**

Add these imports to `service-graph-canvas.tsx`:

```tsx
import { AnimatePresence } from "framer-motion";
// hooks
import { useHoverPointer } from "@/hooks/use-mobile-viewport";
// local imports
import { useServiceFocusInteraction } from "./use-service-focus";
```

- [ ] **Step 2: Replace the selected-service state**

Replace:

```tsx
const [selectedServiceId, setSelectedServiceId] = useState<string | null>(null);
```

with:

```tsx
const {
  effectiveServiceId,
  pinnedServiceId,
  hover,
  leaveNode,
  enterCard,
  leaveCard,
  pin,
  clear: clearFocus,
  suppress,
  resume,
} = useServiceFocusInteraction();
const canHover = useHoverPointer();
```

Then replace every remaining `selectedServiceId` in derived values/effects with `effectiveServiceId`:

```tsx
const selectedService = effectiveServiceId
  ? (graphData.services.find((service) => service.id === effectiveServiceId) ?? null)
  : null;
const selectedHealth = effectiveServiceId ? graphData.health[effectiveServiceId] : undefined;
const relationships = useMemo(
  () => (effectiveServiceId ? getServiceRelationships(effectiveServiceId, graphData.dependencies) : null),
  [graphData.dependencies, effectiveServiceId]
);
```

```tsx
const selectedNode = effectiveServiceId ? (nodes.find((node) => node.id === effectiveServiceId) ?? null) : null;
```

```tsx
focused: service.id === effectiveServiceId,
```

```tsx
// eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
[layoutNodes, currentLocale, graphData.health, highlighted, relationships, effectiveServiceId];
```

```tsx
useEffect(() => {
  if (!effectiveServiceId) return;

  const handleKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") closeFocus();
  };

  window.addEventListener("keydown", handleKeyDown);
  return () => window.removeEventListener("keydown", handleKeyDown);
}, [closeFocus, effectiveServiceId]);
```

```tsx
useEffect(() => {
  if (effectiveServiceId && !graphData.services.some((service) => service.id === effectiveServiceId)) {
    clearFocus();
  }
}, [clearFocus, effectiveServiceId, graphData.services]);
```

- [ ] **Step 3: Replace click/pane/close handlers and add hover/drag handlers**

Replace the block from `handleNodeClick` through `openSelectedService` with:

```tsx
const handleNodeClick = useCallback(
  (_event: unknown, node: Node) => {
    pin(node.id);
  },
  [pin]
);

const handleNodeMouseEnter = useCallback(
  (_event: unknown, node: Node) => {
    if (canHover) hover(node.id);
  },
  [canHover, hover]
);

const handleNodeMouseLeave = useCallback(() => {
  leaveNode();
}, [leaveNode]);

const handleNodeDragStart = useCallback(() => {
  suppress();
}, [suppress]);

const handleConnectStart = useCallback(() => {
  suppress();
}, [suppress]);

const handleConnectEnd = useCallback(() => {
  resume();
}, [resume]);

const handleMoveStart = useCallback(() => {
  suppress();
}, [suppress]);

const handleMoveEnd = useCallback(() => {
  resume();
}, [resume]);

const handleNodeDoubleClick = useCallback(
  (_event: unknown, node: Node) => {
    onNodeOpen?.(node.id);
  },
  [onNodeOpen]
);

const closeFocus = useCallback(() => {
  clearFocus();
  setNodes((current) => current.map((node) => (node.selected ? { ...node, selected: false } : node)));
}, [clearFocus, setNodes]);

const openSelectedService = useCallback(() => {
  if (effectiveServiceId) onNodeOpen?.(effectiveServiceId);
}, [effectiveServiceId, onNodeOpen]);
```

Update the existing `handleNodeDragStop` so it resumes hover before persisting position:

```tsx
const handleNodeDragStop = useCallback(
  (_event: unknown, node: Node) => {
    resume();
    onNodeDragStop?.(node.id, node.position);
  },
  [onNodeDragStop, resume]
);
```

- [ ] **Step 4: Pass handlers to `ReactFlow`**

Add to the `<ReactFlow ...>` props:

```tsx
onNodeMouseEnter={canHover ? handleNodeMouseEnter : undefined}
onNodeMouseLeave={canHover ? handleNodeMouseLeave : undefined}
onNodeDragStart={readOnly ? undefined : handleNodeDragStart}
onConnectStart={readOnly ? undefined : handleConnectStart}
onConnectEnd={readOnly ? undefined : handleConnectEnd}
onMoveStart={handleMoveStart}
onMoveEnd={handleMoveEnd}
```

- [ ] **Step 5: Add pointer props to `ServiceFocusCard`**

In `service-focus-card.tsx`, extend `TServiceFocusCardProps`:

```tsx
  pinned?: boolean;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
```

Add only `onPointerEnter` and `onPointerLeave` to the function's destructured parameters in this task (`pinned` is consumed in Task 6), then add both to the outer wrapper:

```tsx
<div
  className="absolute z-20"
  style={{ ...position, width: cardWidth }}
  role="region"
  aria-label={service.name}
  onPointerEnter={onPointerEnter}
  onPointerLeave={onPointerLeave}
>
```

- [ ] **Step 6: Wrap the card in `AnimatePresence`**

Replace the card render block with:

```tsx
<AnimatePresence mode="wait">
  {selectedService && selectedNode && (
    <ServiceFocusCard
      key={selectedService.id}
      node={selectedNode}
      service={selectedService}
      relationshipCount={relationships?.edgeIds.size ?? 0}
      health={selectedHealth?.health}
      lastDeployedAt={selectedHealth?.last_deployed_at}
      pinned={pinnedServiceId === selectedService.id}
      onClose={closeFocus}
      onOpen={onNodeOpen ? openSelectedService : undefined}
      onPointerEnter={enterCard}
      onPointerLeave={leaveCard}
    />
  )}
</AnimatePresence>
```

- [ ] **Step 7: Verify types and lint**

Run: `pnpm --filter=web check:types && pnpm exec oxlint apps/web/core/components/services/graph/service-graph-canvas.tsx apps/web/core/components/services/graph/service-focus-card.tsx`

Expected: typecheck PASS; `Found 0 warnings and 0 errors.`

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/services/graph/service-graph-canvas.tsx apps/web/core/components/services/graph/service-focus-card.tsx
git commit -m "feat(web): open service focus card on hover and pin on click"
```

---

### Task 6: Animate the card

**Files:**

- Modify: `apps/web/core/components/services/graph/service-focus-card.tsx`

- [ ] **Step 1: Add motion imports and reduced-motion flag**

In `service-focus-card.tsx`, add:

```tsx
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
```

Remove the now-unused `ChevronUpOutline` import. Add `pinned` to the destructured component parameters (the prop type already declares it from Task 5). Inside the component, after `const { t } = useTranslation();`:

```tsx
const reducedMotion = useReducedMotion();
```

- [ ] **Step 2: Convert the card container to `motion.div`**

Replace the opening card `<div ...>` with:

```tsx
<motion.div
  className="service-focus-card relative flex flex-col overflow-hidden rounded-xl border border-strong bg-surface-1"
  style={
    {
      maxHeight,
      transformOrigin: `${pointerLeft}px ${placeAbove ? "bottom" : "top"}`,
      "--focus-accent": accent,
    } as CSSProperties
  }
  initial={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: placeAbove ? 8 : -8 }}
  animate={{ opacity: 1, scale: 1, y: 0 }}
  exit={
    reducedMotion
      ? { opacity: 0 }
      : { opacity: 0, scale: 0.98, y: placeAbove ? 6 : -6, transition: { duration: 0.12, ease: "easeOut" } }
  }
  transition={
    reducedMotion ? { duration: 0.12 } : { type: "spring", stiffness: 380, damping: 30, mass: 0.7 }
  }
>
```

Close it with `</motion.div>` instead of `</div>`.

- [ ] **Step 3: Stagger the card sections**

Convert these four containers:

- `<header ...>` → `<motion.header ...>`
- the meta `<div className="relative flex flex-wrap ...">` → `<motion.div ...>`
- the description wrapper `<div className="flex min-h-0 flex-1 flex-col ...">` → `<motion.div ...>`
- `<footer ...>` → `<motion.footer ...>`

For each, add the same fade/slide props (keep all existing classes). Use these delays:

```tsx
// header
initial={reducedMotion ? false : { opacity: 0, y: 4 }}
animate={{ opacity: 1, y: 0 }}
transition={{ delay: reducedMotion ? 0 : 0, duration: 0.18, ease: "easeOut" }}

// meta
initial={reducedMotion ? false : { opacity: 0, y: 4 }}
animate={{ opacity: 1, y: 0 }}
transition={{ delay: reducedMotion ? 0 : 0.03, duration: 0.18, ease: "easeOut" }}

// description
initial={reducedMotion ? false : { opacity: 0, y: 4 }}
animate={{ opacity: 1, y: 0 }}
transition={{ delay: reducedMotion ? 0 : 0.05, duration: 0.18, ease: "easeOut" }}

// footer
initial={reducedMotion ? false : { opacity: 0, y: 4 }}
animate={{ opacity: 1, y: 0 }}
transition={{ delay: reducedMotion ? 0 : 0.07, duration: 0.18, ease: "easeOut" }}
```

- [ ] **Step 4: Animate the chevron**

Replace the conditional chevron block:

```tsx
{
  expanded ? <ChevronUpOutline className="h-3 w-3 shrink-0" /> : <ChevronDownOutline className="h-3 w-3 shrink-0" />;
}
```

with:

```tsx
<motion.span
  aria-hidden
  animate={{ rotate: expanded ? 180 : 0 }}
  transition={reducedMotion ? { duration: 0 } : { type: "spring", stiffness: 420, damping: 30 }}
  className="shrink-0"
>
  <ChevronDownOutline className="h-3 w-3" />
</motion.span>
```

- [ ] **Step 5: Add the pinned indicator**

Inside the header, immediately before the close button, insert:

```tsx
<AnimatePresence initial={false}>
  {pinned && (
    <motion.span
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.9 }}
      transition={{ duration: 0.14, ease: "easeOut" }}
      className="inline-flex shrink-0 items-center gap-1 self-center font-code text-9 font-medium tracking-[0.12em] text-tertiary uppercase"
      title={t("common.pin")}
    >
      <span aria-hidden className="h-1 w-1 rounded-full bg-[var(--focus-accent)]" />
      {t("common.pin")}
    </motion.span>
  )}
</AnimatePresence>
```

- [ ] **Step 6: Verify types and lint**

Run: `pnpm --filter=web check:types && pnpm exec oxlint apps/web/core/components/services/graph/service-focus-card.tsx`

Expected: PASS.

- [ ] **Step 7: Manual motion check**

Run the dev server or rebuild prod, then verify:

- card enters with a spring from the node side;
- moving cursor from node to card crosses the 10px gap without closing;
- pinned label appears after clicking a node;
- reduced-motion (DevTools emulation) removes scale/y movement.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/services/graph/service-focus-card.tsx
git commit -m "feat(web): animate service focus card with framer motion"
```

---

### Task 7: Animate node feedback and clean up CSS

**Files:**

- Modify: `apps/web/core/components/services/graph/service-node.tsx`
- Modify: `apps/web/styles/globals.css`

- [ ] **Step 1: Add motion imports and reduced-motion flag**

In `service-node.tsx`, add:

```tsx
import { motion, useReducedMotion } from "framer-motion";
```

Inside the component after `const service = ...` guard:

```tsx
const reducedMotion = useReducedMotion();
```

- [ ] **Step 2: Convert the visual card to `motion.div`**

Replace the `service-node-card` `<div>` with:

```tsx
<motion.div
  className={cn(
    "service-node-card relative flex flex-col gap-2 overflow-hidden rounded-lg border bg-layer-1 p-3 shadow-raised-100 transition-[background-color,border-color,box-shadow] duration-150 hover:bg-layer-1-hover hover:shadow-raised-200",
    HEALTH_BORDER_STYLE[state],
    focused && "is-focused",
    isLite && "border-dashed"
  )}
  whileHover={reducedMotion ? undefined : { y: -2 }}
  whileTap={reducedMotion ? undefined : { scale: 0.995 }}
  transition={{ type: "spring", stiffness: 420, damping: 28 }}
  title={isLite ? `${service.name} — ${typeLabel}` : service.name}
>
```

Close it with `</motion.div>`.

- [ ] **Step 3: Remove the old CSS entrance animation**

In `apps/web/styles/globals.css`, delete:

```css
.service-focus-card {
  box-shadow:
    0 1px 2px 0 rgba(0, 0, 0, 0.06),
    0 20px 48px -20px rgba(0, 0, 0, 0.48),
    0 8px 20px -12px rgba(0, 0, 0, 0.32);
  animation: service-focus-card-in 160ms cubic-bezier(0.22, 1, 0.36, 1);
}

@keyframes service-focus-card-in {
  from {
    opacity: 0;
    transform: translateY(6px) scale(0.985);
  }

  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

@media (prefers-reduced-motion: reduce) {
  .service-focus-card {
    animation: none;
  }
}
```

Replace it with only the static shadow:

```css
.service-focus-card {
  box-shadow:
    0 1px 2px 0 rgba(0, 0, 0, 0.06),
    0 20px 48px -20px rgba(0, 0, 0, 0.48),
    0 8px 20px -12px rgba(0, 0, 0, 0.32);
}
```

- [ ] **Step 4: Run all graph tests**

Run: `pnpm --filter=web exec vitest run core/components/services/graph`

Expected: PASS, `31 tests` (23 existing + 8 new).

- [ ] **Step 5: Full verification**

Run:

```bash
pnpm --filter=web check:types
pnpm exec oxfmt --check apps/web/core/hooks/use-mobile-viewport.ts apps/web/core/components/services/graph/focus-intent.ts apps/web/core/components/services/graph/focus-intent.test.ts apps/web/core/components/services/graph/use-service-focus.ts apps/web/core/components/services/graph/service-graph-canvas.tsx apps/web/core/components/services/graph/service-focus-card.tsx apps/web/core/components/services/graph/service-node.tsx apps/web/styles/globals.css
pnpm turbo run build --filter=web
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000/itsm/services/
```

Expected: types PASS, format PASS, build `11 successful`, service responds `200`.

- [ ] **Step 6: Manual interaction checklist**

Verify in the browser:

- hover <180ms does not open; hover >200ms opens;
- crossing node → card does not flicker;
- pinned card stays when hovering another node;
- clicking another node moves the pin;
- close button, `Esc`, and canvas click close the card;
- drag, connect, and pan do not open the card;
- touch emulation: tap pins, tap outside closes;
- double-click still opens the full service detail.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/services/graph/service-node.tsx apps/web/styles/globals.css
git commit -m "feat(web): add node hover motion and remove legacy card animation"
```
