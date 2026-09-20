# Auth Sign-up Left Panel Copy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add compact C2 copywriting to the sign-up left panel without touching sign-in or the waves visual.

**Architecture:** New presentational component `signup-copy.tsx` (copy block + trust line) rendered conditionally in `AuthBase` for `SIGN_UP` only, with strings in `en/auth.json` following the existing `useTranslation` pattern.

**Tech Stack:** React + TypeScript, Tailwind tokens, `@plane/i18n` `useTranslation`, React Router app in `apps/web`.

---

## File map

- Create: `apps/web/core/components/auth-screens/signup-copy.tsx` — `AuthSignupCopy` (eyebrow + H1 + subcopy) and `AuthSignupTrust` (centered trust line). Static, no props, no state.
- Modify: `packages/i18n/src/locales/en/auth.json` — add `auth.sign_up.copy` keys (eyebrow, headline, subcopy, trust).
- Modify: `apps/web/core/components/auth-screens/auth-base.tsx:7-24` — value-import `EAuthModes`, conditionally render copy + trust for `SIGN_UP` only.
- No test files: `apps/web` has no unit-test runner (no `*.test.*`, no vitest/jest in `apps/web/package.json`). Verification is `tsc` + `oxlint` + `web build` + visual check.

Spec: `docs/superpowers/specs/2026-09-20-auth-signup-copy-design.md`.

---

### Task 1: i18n keys for sign-up copy

**Files:**

- Modify: `packages/i18n/src/locales/en/auth.json:283-306`
- Test: JSON validity + key presence via command line

Current snippet (`en/auth.json`, inside `"sign_up"`):

```json
    "sign_up": {
      "header": {
        "label": "Create an account to start managing work with your team.",
```

- [ ] **Step 1: Add the `copy` block to `en/auth.json`**

Edit `packages/i18n/src/locales/en/auth.json` so the `sign_up` object becomes:

```json
    "sign_up": {
      "copy": {
        "eyebrow": "Built for IT teams",
        "headline": "Take care of IT, without giving up control.",
        "subcopy": "Terraline brings requests, approvals, and audits together in one calm place — hosted your way, with your data staying yours.",
        "trust": "Self-host or cloud • Your data stays yours • Audit-ready"
      },
      "header": {
        "label": "Create an account to start managing work with your team.",
```

Keep 2-space indent and trailing commas exactly as the file uses. Do not touch any other locale file; they fall back to English.

- [ ] **Step 2: Verify JSON is valid and keys exist**

Run:

```bash
python3 -c "import json; d=json.load(open('packages/i18n/src/locales/en/auth.json')); print(d['auth']['sign_up']['copy'])"
```

Expected: prints the four-key dict with eyebrow/headline/subcopy/trust, exit 0. If it throws, the JSON is malformed — fix commas/brackets and rerun.

- [ ] **Step 3: Commit**

```bash
git add packages/i18n/src/locales/en/auth.json
git commit -m "feat(auth): add sign-up copy i18n keys"
```

---

### Task 2: Create the `signup-copy.tsx` component

**Files:**

- Create: `apps/web/core/components/auth-screens/signup-copy.tsx`
- Test: `tsc --noEmit` + `oxlint` (no unit-test runner exists in `apps/web`)

Follow the existing stateless pattern from `footer.tsx`/`header.tsx` (license header, Tailwind tokens `text-tertiary` / `text-accent-primary`, no props).

- [ ] **Step 1: Write the component file**

Create `apps/web/core/components/auth-screens/signup-copy.tsx` with exactly this content:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";

export function AuthSignupCopy() {
  const { t } = useTranslation();
  return (
    <div className="mb-6 mt-8 flex flex-col gap-2">
      <span className="text-11 font-semibold uppercase tracking-[0.12em] text-accent-primary">
        {t("auth.sign_up.copy.eyebrow")}
      </span>
      <h1 className="text-2xl font-extrabold leading-tight text-primary">{t("auth.sign_up.copy.headline")}</h1>
      <p className="text-sm leading-relaxed text-tertiary">{t("auth.sign_up.copy.subcopy")}</p>
    </div>
  );
}

export function AuthSignupTrust() {
  const { t } = useTranslation();
  return <p className="mt-6 text-center text-12 text-tertiary">{t("auth.sign_up.copy.trust")}</p>;
}
```

Notes: `mt-8` separates the copy from the sticky `AuthHeader`; `mb-6` gives the form breathing room. `text-11`/`text-12`/`text-accent-primary`/`text-tertiary` match tokens already used in `header.tsx`, `footer.tsx`, and `waves-panel.tsx`.

- [ ] **Step 2: Typecheck the new file**

Run from repo root:

```bash
pnpm --filter=web exec tsc --noEmit -p tsconfig.json 2>&1 | head -n 30
```

Expected: no output, exit 0. Any error naming `signup-copy.tsx` must be fixed before continuing.

- [ ] **Step 3: Lint the new file**

Run from repo root:

```bash
pnpm exec oxlint apps/web/core/components/auth-screens/signup-copy.tsx 2>&1 | tail -n 5
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/auth-screens/signup-copy.tsx
git commit -m "feat(auth): add sign-up copy components"
```

---

### Task 3: Render copy in `AuthBase` for SIGN_UP only

**Files:**

- Modify: `apps/web/core/components/auth-screens/auth-base.tsx:7-24`
- Test: `tsc --noEmit` + `oxlint` + visual check in Task 4

Current file content:

```tsx
import React from "react";
import { AuthRoot } from "@/components/account/auth-forms/auth-root";
import type { EAuthModes } from "@/helpers/authentication.helper";
import { AuthHeader } from "./header";
import { AuthWavesPanel } from "./shape-waves/waves-panel";

type AuthBaseProps = {
  authType: EAuthModes;
};

export function AuthBase({ authType }: AuthBaseProps) {
  return (
    <div className="relative z-10 flex h-screen w-screen overflow-hidden bg-surface-1">
      <div className="flex h-full w-full min-w-0 flex-col overflow-hidden overflow-y-auto px-6 pt-6 pb-10 sm:px-8 lg:w-[46%] lg:min-w-[30rem] xl:min-w-[34rem]">
        <AuthHeader type={authType} />
        <AuthRoot authMode={authType} />
      </div>
      <AuthWavesPanel />
    </div>
  );
}
```

Critical detail: the current import is `import type { EAuthModes }` (type-only, erased at runtime). The conditional render needs the runtime value, so the import must change to a value import.

- [ ] **Step 1: Edit imports and JSX**

New file content (only lines 7-24 change):

```tsx
import React from "react";
import { AuthRoot } from "@/components/account/auth-forms/auth-root";
import { EAuthModes } from "@/helpers/authentication.helper";
import { AuthHeader } from "./header";
import { AuthSignupCopy, AuthSignupTrust } from "./signup-copy";
import { AuthWavesPanel } from "./shape-waves/waves-panel";

type AuthBaseProps = {
  authType: EAuthModes;
};

export function AuthBase({ authType }: AuthBaseProps) {
  return (
    <div className="relative z-10 flex h-screen w-screen overflow-hidden bg-surface-1">
      <div className="flex h-full w-full min-w-0 flex-col overflow-hidden overflow-y-auto px-6 pt-6 pb-10 sm:px-8 lg:w-[46%] lg:min-w-[30rem] xl:min-w-[34rem]">
        <AuthHeader type={authType} />
        {authType === EAuthModes.SIGN_UP && <AuthSignupCopy />}
        <AuthRoot authMode={authType} />
        {authType === EAuthModes.SIGN_UP && <AuthSignupTrust />}
      </div>
      <AuthWavesPanel />
    </div>
  );
}
```

Do not reorder anything else. `SIGN_IN` renders byte-identical output to today.

- [ ] **Step 2: Typecheck**

Run from repo root:

```bash
pnpm --filter=web exec tsc --noEmit -p tsconfig.json 2>&1 | head -n 30
```

Expected: no output, exit 0.

- [ ] **Step 3: Lint**

Run from repo root:

```bash
pnpm exec oxlint apps/web/core/components/auth-screens/auth-base.tsx apps/web/core/components/auth-screens/signup-copy.tsx 2>&1 | tail -n 5
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/auth-screens/auth-base.tsx
git commit -m "feat(auth): show C2 copy on sign-up left panel"
```

---

### Task 4: Build, deploy to tunnel demo, visual verify

**Files:**

- None (verification only)

- [ ] **Step 1: Production build**

Run from repo root:

```bash
pnpm --filter=web build 2>&1 | tail -n 5
```

Expected: `✓ built in ...s` plus `SPA Mode: Generated build/client/index.html`, exit 0. A build failure blocks the next step — fix errors and rebuild.

- [ ] **Step 2: Restart prod and confirm live**

Run from repo root:

```bash
systemctl --user restart plane-web-prod.service && systemctl --user is-active plane-web-prod.service
```

Expected output: `active`. (Prod serves static `apps/web/build/client`; the dev service `plane-web.service` must stay inactive — never enable both.)

- [ ] **Step 3: Visual check**

Open `/sign-up`: eyebrow `Built for IT teams`, H1 `Take care of IT, without giving up control.`, one-sentence subcopy, form unchanged, trust line `Self-host or cloud • Your data stays yours • Audit-ready` below the form. Check desktop (≥64rem, waves panel visible on right) and one mobile width (copy stacks above form). Open `/sign-in`: confirm unchanged (no copy block, no trust line).

## Self-review

1. **Spec coverage:** copy strings + placement (§3) → Tasks 1–3; component/i18n/styling/responsive (§4) → Tasks 1–3 with matching tokens and `SIGN_UP`-only guard; verification (§6) → Task 4; out-of-scope (§5: waves panel, sign-in, form logic, logos, per-locale translations) → untouched by all tasks.
2. **Placeholder scan:** no TBD/TODO/generic steps — every edit shows exact file content, every command shows expected output.
3. **Type consistency:** `EAuthModes` value import used in both `auth-base.tsx` edit and plan text; `AuthSignupCopy`/`AuthSignupTrust` names identical in Task 2 creation and Task 3 import/render; i18n keys `auth.sign_up.copy.{eyebrow,headline,subcopy,trust}` identical in Task 1 JSON and Task 2 `t()` calls.
