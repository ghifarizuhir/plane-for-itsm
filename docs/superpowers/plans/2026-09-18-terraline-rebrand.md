# Terraline Rebrand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebrand every user-facing trace of "Plane" to "Terraline" — logo, logo animation, product name, copy, metadata, email — while keeping the blue brand color and Inter/IBM Plex Mono/Material Symbols fonts and leaving internal identifiers untouched.

**Architecture:** In-place swap. Existing file names, component names (`PlaneLogo`, `PlaneWordmark`, `PlaneLockup`, `PlaneNewIcon`, `LogoSpinner`), and imports stay; their contents and referenced asset bytes change. A reusable codemod script (`scripts/terraline-rebrand.mjs`) performs auditable, rule-based text replacement scoped to source/content files, skipping copyright headers and internal identifiers. Raster assets are generated from one mark SVG via `sharp`.

**Tech Stack:** React 19 + React Router 7 (web/admin/space), TypeScript strict, Tailwind CSS v4 CSS-first tokens via `@plate/tailwind-config` → `@makeplane/propel`, Node ESM scripts, `sharp` for SVG→raster, JSON i18n catalogs.

**Spec:** `docs/superpowers/specs/2026-09-18-terraline-rebrand-design.md`

---

## Commit policy

The executing agent must **not** commit unless the user explicitly asks. Each task's
"Checkpoint" step says what to stage/review; run `git commit` only if the user has
authorised it.

## Reference design (single source of truth for the mark)

Mark "Strata T" — three horizontally-centered rounded bars; top bar full width (T
crossbar), lower bars progressively narrower (terraces). Mark viewBox `0 0 85 52`:
top bar `y=0 h=14`, middle `y=19 h=14`, bottom `y=38 h=14`; widths `85`, `63.75`,
`42.5`; `rx=7`; centered at `x=42.5`.

---

## Task 1: Rebrand audit + codemod tool

**Files:**
- Create: `scripts/terraline-rebrand.mjs`

- [ ] **Step 1: Write the script**

```js
#!/usr/bin/env node
// scripts/terraline-rebrand.mjs
// Auditable, rule-based rebrand helper for Plane -> Terraline.
// Usage:
//   node scripts/terraline-rebrand.mjs --check   # list remaining user-facing matches
//   node scripts/terraline-rebrand.mjs --write   # apply replacements in place
//   node scripts/terraline-rebrand.mjs --write --path packages/constants/src/payment.ts
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

const MODE = process.argv.includes("--write") ? "write" : "check";
const pathArgIndex = process.argv.indexOf("--path");
if (pathArgIndex !== -1 && process.argv[pathArgIndex + 1] === undefined) {
  console.error("error: --path requires a value");
  process.exit(1);
}
const onlyPath = pathArgIndex !== -1 ? process.argv[pathArgIndex + 1] : null;
const TARGET_GLOBS = [
  "packages/i18n/src/locales",
  "packages/constants/src",
  "packages/propel/src",
  "apps/web/app",
  "apps/web/core",
  "apps/admin/app",
  "apps/admin/components",
  "apps/admin/hooks",
  "apps/space/app",
  "apps/space/components",
  "apps/space/lib",
  "apps/api/plane",
];

// Whole-word product name, capital P + lowercase rest. Does NOT match PLANE_*,
// PlaneLockup, plane.*, @plane/*, plane.so.
const WORD_RULES = [[/\bPlane\b/g, "Terraline"]];
// User-facing domains only. Does NOT touch @plane/* or python plane.* modules.
const URL_RULES = [
  [/plane\.so/g, "terraline.space"],
  [/plane\.sh/g, "terraline.space"],
  // Long-tail brand domains/slugs found during Task 9 review (all verified
  // locale- or copy-only; no code depends on these literals).
  [/planes\.so/g, "terraline.space"],
  [/plane\.town/g, "terraline.space"],
  [/plane-github-enterprise/g, "terraline-github-enterprise"],
];
// Lines we never rewrite: license/copyright headers and SPDX tags.
const SKIP_LINE = /Plane Software, Inc\.|SPDX-|Copyright \(c\)/;
// Upstream-only resources handled by hand (Task 12), never auto-repointed.
const EXCLUDE_PATH = [
  /apps\/api\/templates\//,
  /scripts\//,
  /docs\//,
  /node_modules\//,
  /\/build\//,
  /\/dist\//,
  /\.react-router\//,
];

function files() {
  const globs = onlyPath ? [onlyPath] : TARGET_GLOBS;
  const out = execFileSync("git", ["ls-files", "--", ...globs], {
    encoding: "utf8",
  })
    .split("\n")
    .filter(Boolean)
    .filter((f) => /\.(ts|tsx|js|jsx|json|py|css|html)$/.test(f))
    .filter((f) => !EXCLUDE_PATH.some((re) => re.test(f)));
  return out;
}

function transform(text) {
  return text
    .split("\n")
    .map((line) => {
      if (SKIP_LINE.test(line)) return line;
      let next = line;
      for (const [re, to] of URL_RULES) next = next.replace(re, to);
      for (const [re, to] of WORD_RULES) next = next.replace(re, to);
      return next;
    })
    .join("\n");
}

let changed = 0;
let reported = 0;
const failures = [];
for (const file of files()) {
  try {
    const original = readFileSync(file, "utf8");
    const next = transform(original);
    if (next === original) continue;
    changed++;
    if (MODE === "write") {
      writeFileSync(file, next);
      console.log(`rewrote ${file}`);
    } else {
      const hits = original
        .split("\n")
        .map((l, i) => [l, i + 1])
        .filter(
          ([l]) =>
            !SKIP_LINE.test(l) &&
            (/\bPlane\b/.test(l) ||
              /planes?\.so/.test(l) ||
              /plane\.sh/.test(l) ||
              /plane\.town/.test(l) ||
              /plane-github-enterprise/.test(l)),
        );
      for (const [l, n] of hits) console.log(`${file}:${n}: ${l.trim()}`);
      reported += hits.length;
    }
  } catch (err) {
    failures.push(`${file}: ${err.message}`);
  }
}
if (failures.length > 0) {
  for (const f of failures) console.error(`failed ${f}`);
  process.exit(1);
}
console.log(
  MODE === "write"
    ? `\n${changed} file(s) rewritten.`
    : `\n${reported} match(es) across ${changed} file(s). Run with --write to apply.`,
);
```

- [ ] **Step 2: Dry-run the audit**

Run: `node scripts/terraline-rebrand.mjs --check`
Expected: A long list of file:line matches under the target globs, and a final
`N match(es) across M file(s)` line. No error.

- [ ] **Step 3: Verify exclusions hold**

Run: `node scripts/terraline-rebrand.mjs --check | grep -E "Plane Software|SPDX|@plane/|plane\.app" | head`
Expected: no output (copyright, SPDX, package scope, python module paths are excluded).

- [ ] **Step 4: Checkpoint**

```bash
# NOTE: repo-root scripts/ is gitignored; force-add like the existing
# scripts/e2e-create.sh precedent.
git add -f scripts/terraline-rebrand.mjs
git status --short
```

---

## Task 2: Brand icon components (propel)

**Files:**
- Modify: `packages/propel/src/icons/brand/plane-logo.tsx`
- Modify: `packages/propel/src/icons/brand/plane-wordmark.tsx`
- Modify: `packages/propel/src/icons/brand/plane-lockup.tsx`
- Modify: `packages/propel/src/icons/sub-brand/plane-icon.tsx`

- [ ] **Step 1: Replace the mark** (`packages/propel/src/icons/brand/plane-logo.tsx`)

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export function PlaneLogo({ width = "85", height = "52", className, color = "currentColor" }: ISvgIcons) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 85 52"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <rect x="0" y="0" width="85" height="14" rx="7" fill={color} />
      <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill={color} />
      <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill={color} />
    </svg>
  );
}
```

- [ ] **Step 2: Replace the wordmark** (`packages/propel/src/icons/brand/plane-wordmark.tsx`)

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export function PlaneWordmark({ width = "146", height = "44", className, color = "currentColor" }: ISvgIcons) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 146 44"
      fill={color}
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <text
        x="0"
        y="32"
        fill={color}
        fontFamily="Inter Variable, Inter, ui-sans-serif, system-ui, sans-serif"
        fontSize="34"
        fontWeight="600"
        letterSpacing="-0.02em"
        textLength="146"
        lengthAdjust="spacingAndGlyphs"
      >
        Terraline
      </text>
    </svg>
  );
}
```

- [ ] **Step 3: Replace the lockup** (`packages/propel/src/icons/brand/plane-lockup.tsx`)

Full file body (keeps `viewBox="0 0 253 53"` and default `width=253 height=53`; mark is
left-aligned at the same coordinates as the mark component, text is stretched to fit):

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export function PlaneLockup({ width = "253", height = "53", className, color = "currentColor" }: ISvgIcons) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 253 53"
      fill={color}
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <rect x="0" y="0.5" width="85" height="14" rx="7" fill={color} />
      <rect x="10.625" y="19.5" width="63.75" height="14" rx="7" fill={color} />
      <rect x="21.25" y="38.5" width="42.5" height="14" rx="7" fill={color} />
      <text
        x="96"
        y="38"
        fill={color}
        fontFamily="Inter Variable, Inter, ui-sans-serif, system-ui, sans-serif"
        fontSize="36"
        fontWeight="600"
        letterSpacing="-0.02em"
        textLength="152"
        lengthAdjust="spacingAndGlyphs"
      >
        Terraline
      </text>
    </svg>
  );
}
```

- [ ] **Step 4: Replace the sub-brand glyph** (`packages/propel/src/icons/sub-brand/plane-icon.tsx`)

Keep the `IconWrapper` + `ISvgIcons` interface; replace the two `<path>` children with:

```tsx
      <rect x="1.5" y="2" width="13" height="3" rx="1.5" fill={color} />
      <rect x="3.5" y="6.5" width="9" height="3" rx="1.5" fill={color} />
      <rect x="5.5" y="11" width="5" height="3" rx="1.5" fill={color} />
```

- [ ] **Step 5: Type-check and build the package**

Run: `pnpm --filter=@plane/propel check:types`
Expected: PASS (no errors).

Run: `pnpm --filter=@plane/propel build`
Expected: build succeeds.

- [ ] **Step 6: Checkpoint**

```bash
git add packages/propel/src/icons/brand/plane-logo.tsx \
        packages/propel/src/icons/brand/plane-wordmark.tsx \
        packages/propel/src/icons/brand/plane-lockup.tsx \
        packages/propel/src/icons/sub-brand/plane-icon.tsx
```

---

## Task 3: Admin local lockup duplicate

**Files:**
- Modify: `apps/admin/components/common/plane-lockup.tsx`
- Modify: `apps/admin/components/common/new-user-popup.tsx:14-15` (takeoff icons)
- Modify: `apps/admin/app/assets/logos/takeoff-icon-light.svg`
- Modify: `apps/admin/app/assets/logos/takeoff-icon-dark.svg`

- [ ] **Step 1: Replace the admin lockup SVG body**

In `apps/admin/components/common/plane-lockup.tsx`, keep the `PlaneLockupProps` type and
`export function PlaneLockup(...)` signature, replace the `<svg>` children (the `<g>`,
all `<path>`s, and `<defs>`) with:

```tsx
      <rect x="0" y="0.5" width="85" height="14" rx="7" fill={color} />
      <rect x="10.625" y="19.5" width="63.75" height="14" rx="7" fill={color} />
      <rect x="21.25" y="38.5" width="42.5" height="14" rx="7" fill={color} />
      <text
        x="96"
        y="38"
        fill={color}
        fontFamily="Inter Variable, Inter, ui-sans-serif, system-ui, sans-serif"
        fontSize="36"
        fontWeight="600"
        letterSpacing="-0.02em"
        textLength="152"
        lengthAdjust="spacingAndGlyphs"
      >
        Terraline
      </text>
```

- [ ] **Step 2: Replace the takeoff icons**

Write `apps/admin/app/assets/logos/takeoff-icon-light.svg`:

```svg
<svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
  <rect width="48" height="48" rx="12" fill="#3F76FF"/>
  <rect x="10" y="14" width="28" height="5" rx="2.5" fill="white"/>
  <rect x="14" y="22" width="20" height="5" rx="2.5" fill="white" fill-opacity="0.75"/>
  <rect x="18" y="30" width="12" height="5" rx="2.5" fill="white" fill-opacity="0.5"/>
</svg>
```

Write `apps/admin/app/assets/logos/takeoff-icon-dark.svg` with the same geometry but
`fill="#0A0A0A"` on the background rect and `fill="#3F76FF"` fill-opacity `1 / 0.75 / 0.5`
on the bars.

- [ ] **Step 3: Type-check admin**

Run: `pnpm --filter=admin check:types`
Expected: PASS.

- [ ] **Step 4: Checkpoint**

```bash
git add apps/admin/components/common/plane-lockup.tsx \
        apps/admin/components/common/new-user-popup.tsx \
        apps/admin/app/assets/logos/takeoff-icon-light.svg \
        apps/admin/app/assets/logos/takeoff-icon-dark.svg
```

---

## Task 4: Static SVG lockup assets

**Files:**
- Modify: `packages/propel/public/plane-lockup-light.svg`
- Modify: `apps/web/app/assets/plane-logos/white-horizontal.svg`
- Modify: `apps/space/app/assets/plane-logos/white-horizontal.svg`
- Modify: `apps/space/app/assets/plane-logo.svg`
- Modify: `apps/web/app/assets/plane-logos/black-horizontal-with-blue-logo.png` *(regenerated in Task 6)*
- Modify: `apps/web/app/assets/plane-logos/white-horizontal-with-blue-logo.png` *(regenerated in Task 6)*
- Modify: `apps/web/app/assets/plane-logos/blue-without-text.png` *(regenerated in Task 6)*
- Modify: `apps/space/app/assets/plane-logos/black-horizontal-with-blue-logo.png` *(regenerated in Task 6)*
- Modify: `apps/space/app/assets/plane-logos/white-horizontal-with-blue-logo.png` *(regenerated in Task 6)*
- Modify: `apps/space/app/assets/plane-logos/blue-without-text.png` *(regenerated in Task 6)*
- Modify: `apps/space/app/assets/plane-logos/blue-without-text-new.png` *(regenerated in Task 6)*

- [ ] **Step 1: Write the white lockup SVG**

Write `packages/propel/public/plane-lockup-light.svg` (same content to
`apps/web/app/assets/plane-logos/white-horizontal.svg` and
`apps/space/app/assets/plane-logos/white-horizontal.svg`):

```svg
<svg width="253" height="53" viewBox="0 0 253 53" fill="none" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0.5" width="85" height="14" rx="7" fill="#FFFFFF"/>
  <rect x="10.625" y="19.5" width="63.75" height="14" rx="7" fill="#FFFFFF"/>
  <rect x="21.25" y="38.5" width="42.5" height="14" rx="7" fill="#FFFFFF"/>
  <text x="96" y="38" fill="#FFFFFF" font-family="Inter, ui-sans-serif, system-ui, sans-serif" font-size="36" font-weight="600" letter-spacing="-0.02em" textLength="152" lengthAdjust="spacingAndGlyphs">Terraline</text>
</svg>
```

- [ ] **Step 2: Write the space bare mark SVG**

Write `apps/space/app/assets/plane-logo.svg`:

```svg
<svg width="85" height="52" viewBox="0 0 85 52" fill="none" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="85" height="14" rx="7" fill="#3F76FF"/>
  <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill="#3F76FF"/>
  <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill="#3F76FF"/>
</svg>
```

- [ ] **Step 3: Checkpoint**

```bash
git add packages/propel/public/plane-lockup-light.svg \
        apps/web/app/assets/plane-logos/white-horizontal.svg \
        apps/space/app/assets/plane-logos/white-horizontal.svg \
        apps/space/app/assets/plane-logo.svg
```

---

## Task 5: Animated logo spinner (replace GIFs)

**Files:**
- Modify: `packages/tailwind-config/index.css` (append keyframes)
- Modify: `apps/web/core/components/common/logo-spinner.tsx`
- Modify: `apps/admin/components/common/logo-spinner.tsx`
- Modify: `apps/space/components/common/logo-spinner.tsx`
- Delete: `apps/web/app/assets/images/logo-spinner-dark.gif`
- Delete: `apps/web/app/assets/images/logo-spinner-light.gif`
- Delete: `apps/admin/app/assets/images/logo-spinner-dark.gif`
- Delete: `apps/admin/app/assets/images/logo-spinner-light.gif`
- Delete: `apps/space/app/assets/images/logo-spinner-dark.gif`
- Delete: `apps/space/app/assets/images/logo-spinner-light.gif`

- [ ] **Step 1: Append the animation to `packages/tailwind-config/index.css`**

```css
/* ---------- Terraline animated logo ---------- */
@keyframes terraline-strata {
  0% {
    transform: scaleX(0.15);
    opacity: 0.35;
  }
  35% {
    transform: scaleX(1);
    opacity: 1;
  }
  70% {
    transform: scaleX(1);
    opacity: 1;
  }
  100% {
    transform: scaleX(0.15);
    opacity: 0.35;
  }
}

.terraline-logo-spinner rect {
  transform-box: fill-box;
  transform-origin: center;
  animation: terraline-strata 1.4s ease-in-out infinite;
}

.terraline-logo-spinner rect:nth-child(2) {
  animation-delay: 0.15s;
}

.terraline-logo-spinner rect:nth-child(3) {
  animation-delay: 0.3s;
}

@media (prefers-reduced-motion: reduce) {
  .terraline-logo-spinner rect {
    animation: none;
  }
}
```

- [ ] **Step 2: Replace the web and space `logo-spinner.tsx`**

`apps/web/core/components/common/logo-spinner.tsx` and
`apps/space/components/common/logo-spinner.tsx` both depend on `@plane/propel`, so they
import the shared mark. Replace each file's body with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { PlaneLogo } from "@plane/propel/icons";

export function LogoSpinner() {
  return (
    <div className="flex items-center justify-center">
      <PlaneLogo
        role="status"
        aria-label="Loading"
        className="terraline-logo-spinner text-primary h-6 w-auto sm:h-11"
      />
    </div>
  );
}
```

- [ ] **Step 3: Replace the admin `logo-spinner.tsx` (inline mark)**

`apps/admin` depends on `@makeplane/propel` only, not the local `@plane/propel`, so it
must inline the mark. Replace
`apps/admin/components/common/logo-spinner.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export function LogoSpinner() {
  return (
    <div className="flex items-center justify-center">
      <svg
        role="status"
        aria-label="Loading"
        viewBox="0 0 85 52"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className="terraline-logo-spinner text-primary h-6 w-auto sm:h-11"
      >
        <rect x="0" y="0" width="85" height="14" rx="7" fill="currentColor" />
        <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill="currentColor" />
        <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill="currentColor" />
      </svg>
    </div>
  );
}
```

Note: remove the now-unused `next-themes` import and the `?url` GIF imports from all
three files.

- [ ] **Step 4: Delete the GIF asset files**

Run:
```bash
git rm apps/web/app/assets/images/logo-spinner-dark.gif \
       apps/web/app/assets/images/logo-spinner-light.gif \
       apps/admin/app/assets/images/logo-spinner-dark.gif \
       apps/admin/app/assets/images/logo-spinner-light.gif \
       apps/space/app/assets/images/logo-spinner-dark.gif \
       apps/space/app/assets/images/logo-spinner-light.gif
```

- [ ] **Step 5: Type-check all three apps**

Run: `pnpm --filter=web check:types && pnpm --filter=admin check:types && pnpm --filter=space check:types`
Expected: PASS.

- [ ] **Step 6: Checkpoint**

```bash
git add packages/tailwind-config/index.css \
        apps/web/core/components/common/logo-spinner.tsx \
        apps/admin/components/common/logo-spinner.tsx \
        apps/space/components/common/logo-spinner.tsx
```

---

## Task 6: Raster asset generation (favicons, PWA, OG, gradient, illustrations)

**Files:**
- Create: `scripts/terraline-assets.mjs`
- Modify: root `package.json` (add `sharp` devDependency)
- Regenerate: favicon/icon/OG/PWA/webp/PNG assets listed below

- [ ] **Step 1: Add `sharp` as a root dev dependency**

Run: `pnpm add -D -w sharp@0.35.3`
Expected: `sharp` added to root `devDependencies`; lockfile updated.

- [ ] **Step 2: Write the generator script**

```js
#!/usr/bin/env node
// scripts/terraline-assets.mjs
import { mkdirSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BRAND = "#3F76FF";

// Point fontconfig at the in-repo Inter TTFs so librsvg can render the wordmark.
process.env.FONTCONFIG_FILE = "/tmp/opencode/terraline-fonts.conf";
mkdirSync("/tmp/opencode/terraline-fontconfig-cache", { recursive: true });
writeFileSync(
  process.env.FONTCONFIG_FILE,
  `<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n<fontconfig>\n  <dir>${join(root, "apps/web/app/assets/fonts/inter")}</dir>\n  <cachedir>/tmp/opencode/terraline-fontconfig-cache</cachedir>\n</fontconfig>\n`,
);

const { default: sharp } = await import("sharp");

const mark = (fill = BRAND, scale = 1) => {
  const w = Math.round(85 * scale);
  const h = Math.round(52 * scale);
  const r = 7 * scale;
  const x = (n) => n * scale;
  return `<svg width="${w}" height="${h}" viewBox="0 0 85 52" xmlns="http://www.w3.org/2000/svg">
    <rect x="0" y="0" width="85" height="14" rx="7" fill="${fill}"/>
    <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill="${fill}"/>
    <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill="${fill}"/>
  </svg>`;
};

const lockup = (fill, bg) => `<svg width="506" height="106" viewBox="0 0 253 53" xmlns="http://www.w3.org/2000/svg">
  ${bg ? `<rect width="253" height="53" fill="${bg}"/>` : ""}
  <rect x="0" y="0.5" width="85" height="14" rx="7" fill="${fill}"/>
  <rect x="10.625" y="19.5" width="63.75" height="14" rx="7" fill="${fill}"/>
  <rect x="21.25" y="38.5" width="42.5" height="14" rx="7" fill="${fill}"/>
  <text x="96" y="38" fill="${fill}" font-family="Inter, DejaVu Sans, sans-serif" font-size="36" font-weight="600" letter-spacing="-0.02em" textLength="152" lengthAdjust="spacingAndGlyphs">Terraline</text>
</svg>`;

async function png(svg, size, out) {
  mkdirSync(dirname(out), { recursive: true });
  await sharp(Buffer.from(svg), { density: 384 })
    .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png()
    .toFile(out);
  console.log(`wrote ${out}`);
}

async function lockupPng(fill, bg, out) {
  mkdirSync(dirname(out), { recursive: true });
  await sharp(Buffer.from(lockup(fill, bg)), { density: 384 }).png().toFile(out);
  console.log(`wrote ${out}`);
}

// Mark-only icons on brand background
const iconTile = `<svg width="512" height="512" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg">
  <rect width="512" height="512" rx="112" fill="${BRAND}"/>
  <g transform="translate(106 164) scale(3.53)">${mark("#FFFFFF").replace(/<\/?svg[^>]*>/g, "")}</g>
</svg>`;

const targets = [
  [iconTile, 16, "apps/web/app/assets/favicon/favicon-16x16.png"],
  [iconTile, 32, "apps/web/app/assets/favicon/favicon-32x32.png"],
  [iconTile, 180, "apps/web/app/assets/favicon/apple-touch-icon.png"],
  [iconTile, 180, "apps/web/app/assets/icons/icon-180x180.png"],
  [iconTile, 512, "apps/web/app/assets/icons/icon-512x512.png"],
  [iconTile, 192, "apps/web/public/favicon/android-chrome-192x192.png"],
  [iconTile, 512, "apps/web/public/favicon/android-chrome-512x512.png"],
  [iconTile, 192, "apps/web/public/icons/icon-192x192.png"],
  [iconTile, 348, "apps/web/public/icons/icon-348x348.png"],
  [iconTile, 512, "apps/web/public/icons/icon-512x512.png"],
  [iconTile, 512, "apps/web/public/plane-logos/plane-mobile-pwa.png"],
  [iconTile, 192, "apps/admin/public/favicon/android-chrome-192x192.png"],
  [iconTile, 512, "apps/admin/public/favicon/android-chrome-512x512.png"],
  [iconTile, 16, "apps/admin/app/assets/favicon/favicon-16x16.png"],
  [iconTile, 32, "apps/admin/app/assets/favicon/favicon-32x32.png"],
  [iconTile, 180, "apps/admin/app/assets/favicon/apple-touch-icon.png"],
  [iconTile, 192, "apps/space/public/favicon/android-chrome-192x192.png"],
  [iconTile, 512, "apps/space/public/favicon/android-chrome-512x512.png"],
  [iconTile, 16, "apps/space/app/assets/favicon/favicon-16x16.png"],
  [iconTile, 32, "apps/space/app/assets/favicon/favicon-32x32.png"],
  [iconTile, 180, "apps/space/app/assets/favicon/apple-touch-icon.png"],
];
for (const [svg, size, out] of targets) await png(svg, size, join(root, out));

// OG image
const og = `<svg width="1201" height="631" viewBox="0 0 1201 631" xmlns="http://www.w3.org/2000/svg">
  <rect width="1201" height="631" fill="#0A0A0A"/>
  <g transform="translate(96 250) scale(1.6)">${mark(BRAND).replace(/<\/?svg[^>]*>/g, "")}</g>
  <text x="248" y="360" fill="#FFFFFF" font-family="Inter, DejaVu Sans, sans-serif" font-size="120" font-weight="600" letter-spacing="-0.03em">Terraline</text>
  <text x="252" y="420" fill="#9CA3AF" font-family="Inter, DejaVu Sans, sans-serif" font-size="40">Modern work management</text>
</svg>`;
await sharp(Buffer.from(og), { density: 192 }).resize(1201, 631).png().toFile(join(root, "apps/web/app/assets/og-image.png"));
console.log("wrote apps/web/app/assets/og-image.png");

// Gradient auth logos (webp)
const gradient = `<svg width="512" height="512" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg">
  <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#3F76FF"/><stop offset="1" stop-color="#05C3FF"/>
  </linearGradient></defs>
  <g transform="translate(106 164) scale(3.53)">${mark("url(#g)").replace(/<\/?svg[^>]*>/g, "")}</g>
</svg>`;
for (const out of [
  "apps/web/app/assets/auth/gradient-logo.webp",
  "apps/web/app/assets/auth/gradient-bg-logo.webp",
]) {
  await sharp(Buffer.from(gradient), { density: 384 }).webp().toFile(join(root, out));
  console.log(`wrote ${out}`);
}

// Full lockup PNG variants used by app/space asset folders
await lockupPng("#0A0A0A", null, join(root, "apps/web/app/assets/plane-logos/black-horizontal-with-blue-logo.png"));
await lockupPng("#FFFFFF", null, join(root, "apps/web/app/assets/plane-logos/white-horizontal-with-blue-logo.png"));
await lockupPng("#0A0A0A", null, join(root, "apps/space/app/assets/plane-logos/black-horizontal-with-blue-logo.png"));
await lockupPng("#FFFFFF", null, join(root, "apps/space/app/assets/plane-logos/white-horizontal-with-blue-logo.png"));
for (const out of [
  "apps/web/app/assets/plane-logos/blue-without-text.png",
  "apps/space/app/assets/plane-logos/blue-without-text.png",
  "apps/space/app/assets/plane-logos/blue-without-text-new.png",
]) await png(mark(BRAND), 512, join(root, out));

// .ico (largest frame; acceptable for modern browsers)
copyFileSync(
  join(root, "apps/web/app/assets/favicon/favicon-32x32.png"),
  join(root, "apps/web/app/assets/favicon/favicon.ico"),
);
copyFileSync(
  join(root, "apps/web/app/assets/favicon/favicon-32x32.png"),
  join(root, "apps/admin/app/assets/favicon/favicon.ico"),
);
copyFileSync(
  join(root, "apps/web/app/assets/favicon/favicon-32x32.png"),
  join(root, "apps/space/app/assets/favicon/favicon.ico"),
);
console.log("favicon.ico files updated");
```

- [ ] **Step 3: Run the generator**

Run: `node scripts/terraline-assets.mjs`
Expected: Many `wrote …` lines, no error.

- [ ] **Step 4: Verify dimensions**

Run:
```bash
node -e "const s=require('sharp');(['apps/web/app/assets/og-image.png','apps/web/public/plane-logos/plane-mobile-pwa.png','apps/web/public/icons/icon-512x512.png']).forEach(async f=>console.log(f, await s(f).metadata().then(m=>m.width+'x'+m.height)))"
```
Expected: `og-image.png 1201x631`, `plane-mobile-pwa.png 512x512`, `icon-512x512.png 512x512`.

- [ ] **Step 5: Checkpoint**

```bash
# NOTE: repo-root scripts/ is gitignored; force-add like scripts/e2e-create.sh.
git add -f scripts/terraline-assets.mjs package.json pnpm-lock.yaml \
        apps/web/app/assets apps/web/public apps/admin/app/assets apps/admin/public \
        apps/space/app/assets apps/space/public
```

---

## Task 7: Constants — metadata, endpoints, payment

**Files:**
- Modify: `packages/constants/src/metadata.ts`
- Modify: `packages/constants/src/endpoints.ts`
- Modify: `packages/constants/src/payment.ts`

- [ ] **Step 1: Rewrite `metadata.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const SITE_NAME = "Terraline | Simple, extensible, open-source project management tool.";
export const SITE_TITLE = "Terraline | Simple, extensible, open-source project management tool.";
export const SITE_DESCRIPTION =
  "Open-source project management tool to manage work items, cycles, and product roadmaps easily";
export const SITE_KEYWORDS =
  "software development, plan, ship, software, accelerate, code management, release management, project management, work items tracking, agile, scrum, kanban, collaboration";
export const SITE_URL = "https://app.terraline.space/";
export const TWITTER_USER_NAME = "Terraline | Simple, extensible, open-source project management tool.";

// Terraline Sites Metadata
export const SPACE_SITE_NAME = "Terraline Publish | Make your Terraline boards and roadmaps public with just one-click. ";
export const SPACE_SITE_TITLE = "Terraline Publish | Make your Terraline boards public with one-click";
export const SPACE_SITE_DESCRIPTION =
  "Terraline Publish is a customer feedback management tool built on top of terraline.space";
export const SPACE_SITE_KEYWORDS =
  "software development, customer feedback, software, accelerate, code management, release management, project management, work items tracking, agile, scrum, kanban, collaboration";
export const SPACE_SITE_URL = "https://app.terraline.space/";
export const SPACE_TWITTER_USER_NAME = "terraline";
```

- [ ] **Step 2: Update `endpoints.ts:27-33`**

```ts
// terraline website url
export const WEBSITE_URL = process.env.VITE_WEBSITE_URL || "https://terraline.space";
// support email
export const SUPPORT_EMAIL = process.env.VITE_SUPPORT_EMAIL || "support@terraline.space";
// marketing links
export const MARKETING_PRICING_PAGE_LINK = "https://terraline.space/pricing";
export const MARKETING_CONTACT_US_PAGE_LINK = "https://terraline.space/contact";
export const MARKETING_PLANE_ONE_PAGE_LINK = "https://terraline.space/one";
```

- [ ] **Step 3: Update `payment.ts` brand strings**

Replace every `"Plane ` plan label and `plane.so` URL in
`packages/constants/src/payment.ts` with the Terraline equivalents, specifically:
`"Plane Pro"` → `"Terraline Pro"`, `"Plane Business"` → `"Terraline Business"`,
`"Plane Enterprise"` → `"Terraline Enterprise"`, `"…for Plane Cloud"` →
`"…for Terraline Cloud"` (Cloud qualifier kept — distinguishes hosted vs self-hosted), and all `https://plane.so/...` / `https://app.plane.so/...` →
`https://terraline.space/...` / `https://app.terraline.space/...`. Run the codemod to
catch the URL forms:

Run: `node scripts/terraline-rebrand.mjs --write --path packages/constants/src/payment.ts`
Then inspect: `git diff packages/constants/src/payment.ts`
Expected: all `plane.so` → `terraline.space`; no `\bPlane\b` remains in the file.

- [ ] **Step 4: Type-check constants**

Run: `pnpm --filter=@plane/constants check:types`
Expected: PASS.

- [ ] **Step 5: Checkpoint**

```bash
git add packages/constants/src
```

---

## Task 8: App metadata (roots, manifests, titles)

**Files:**
- Modify: `apps/web/app/root.tsx`
- Modify: `apps/admin/app/root.tsx`
- Modify: `apps/space/app/root.tsx`
- Modify: `apps/web/core/components/core/page-title.tsx`
- Modify: `apps/web/app/(all)/sign-up/layout.tsx`
- Modify: `apps/web/app/(all)/accounts/reset-password/layout.tsx`
- Modify: `apps/web/app/(all)/accounts/forgot-password/layout.tsx`
- Modify: `apps/web/app/(all)/accounts/set-password/layout.tsx`
- Modify: `apps/web/public/manifest.json`
- Modify: `apps/web/public/site.webmanifest.json`
- Modify: `apps/web/manifest.json`
- Modify: `apps/admin/public/site.webmanifest.json`
- Modify: `apps/space/public/site.webmanifest.json`
- Modify: `apps/space/app/issues/[anchor]/layout.tsx`

- [ ] **Step 1: Web root**

In `apps/web/app/root.tsx`:
- `:35` → `const APP_TITLE = "Terraline | Simple, extensible, open-source project management tool.";`
- `:64` → `<meta name="application-name" content="Terraline" />`
- `:93` → `{ property: "og:url", content: "https://app.terraline.space/" },`
- `:97` → `{ property: "og:image:alt", content: "Terraline - Modern work management" },`
- `:103` → remove the `twitter:site` line (`@planepowers`).
- `:108` → `{ name: "twitter:image:alt", content: "Terraline - Modern work management" },`

- [ ] **Step 2: Admin root** (`apps/admin/app/root.tsx`)

- `APP_TITLE` → `"Terraline | Simple, extensible, open-source project management tool."`
- `APP_DESCRIPTION` → replace "Plane" copy with Terraline.
- `og:url` → `https://terraline.space/`
- remove `twitter:site` (`@planepowers`).

- [ ] **Step 3: Space root** (`apps/space/app/root.tsx`)

- `APP_TITLE` → `"Terraline Publish | Make your Terraline boards public with one-click"`
- `APP_DESCRIPTION` → `"Terraline Publish is a customer feedback management tool built on top of terraline.space"`
- `og:url` → `https://sites.terraline.space/`
- remove `twitter:site`.

- [ ] **Step 4: Titles and issue metadata**

- `apps/web/core/components/core/page-title.tsx:19` default title → Terraline.
- All four route layouts (`sign-up`, `reset-password`, `forgot-password`,
  `set-password`): `"- Plane"` → `"- Terraline"`.
- `apps/space/app/issues/[anchor]/layout.tsx:22-23`: `DEFAULT_TITLE = "Terraline"`,
  `DEFAULT_DESCRIPTION = "Made with Terraline, an AI-powered work management platform with publishing capabilities."`

- [ ] **Step 5: Manifests**

Apply these edits:

`apps/web/public/site.webmanifest.json`:
```json
{
  "name": "Terraline",
  "short_name": "Terraline",
  "description": "Terraline helps you plan your work items, cycles, and product modules.",
  "start_url": ".",
  "display": "standalone",
  "background_color": "#f9fafb",
  "theme_color": "#3f76ff",
  "icons": [
    { "src": "/plane-logos/plane-mobile-pwa.png", "sizes": "192x192", "type": "image/png" },
    { "src": "/plane-logos/plane-mobile-pwa.png", "sizes": "512x512", "type": "image/png" }
  ]
}
```

`apps/web/public/manifest.json`: `"name": "Terraline"`, `"short_name": "Terraline"`.

`apps/web/manifest.json`: name → `"Terraline | Modern work management"`, short_name →
`"Terraline"`, description → `"Terraline accelerates software development for agencies and product companies."` (keep `theme_color` as-is or set `#3f76ff`).

`apps/admin/public/site.webmanifest.json`: name/short_name →
`"Terraline God Mode"`, description mentions Terraline.

`apps/space/public/site.webmanifest.json`: name/short_name → `"Terraline Space"`,
description mentions Terraline.

Also populate the BUNDLED manifest actually served by space
(`apps/space/app/assets/favicon/site.webmanifest`, imported as `?url` in
`apps/space/app/root.tsx:13,35`; the public one is not served): name/short_name →
`"Terraline Space"`, description, `theme_color` `#3f76ff`, `background_color`
`#f9fafb`, keep its icon entries and `display`.

- [ ] **Step 6: Sweep remaining app strings with the codemod**

Run: `node scripts/terraline-rebrand.mjs --write`
Then: `node scripts/terraline-rebrand.mjs --check`
Expected: remaining matches only in excluded paths (email templates, docs, repo root)
or false positives. Review the diff before continuing.

- [ ] **Step 7: Type-check apps**

Run: `pnpm --filter=web check:types && pnpm --filter=admin check:types && pnpm --filter=space check:types`
Expected: PASS.

- [ ] **Step 8: Checkpoint**

```bash
git add apps/web/app/root.tsx apps/admin/app/root.tsx apps/space/app/root.tsx \
        apps/web/core/components/core/page-title.tsx \
        apps/web/app/\(all\) apps/space/app/issues \
        apps/web/public apps/admin/public apps/space/public apps/web/manifest.json
```

---

## Task 9: i18n — all 20 locales

**Files:**
- Modify: `packages/i18n/src/locales/**/*.json`

- [ ] **Step 1: Snapshot counts**

Run:
```bash
rg -o '\bPlane\b' packages/i18n/src/locales | wc -l
```
Record the number (baseline).

- [ ] **Step 2: Apply the codemod**

Run: `node scripts/terraline-rebrand.mjs --write`
Expected: locale files rewritten.

- [ ] **Step 3: Verify values changed, keys untouched**

Run:
```bash
git diff --numstat packages/i18n/src/locales | head
rg -n '"(Plane)"' packages/i18n/src/locales | head
rg -n 'section_plane_events|open_plane_documentation|powered_by_plane_pages' packages/i18n/src/locales/en | head
```
Expected: files changed; no value equals `"Plane"`; keys (lowercase `plane`) still present
unchanged.

- [ ] **Step 4: Spot-check the branded keys**

Run: `rg -n "Terraline AI|Terraline Pro|Terraline Pages|Terraline Runner" packages/i18n/src/locales/en | head`
Expected: matches in the corresponding files.

- [ ] **Step 5: Validate JSON parses and format**

Run:
```bash
node -e "const fs=require('fs'),p=require('path');const walk=d=>fs.readdirSync(d,{withFileTypes:true}).forEach(e=>{const f=p.join(d,e.name);if(e.isDirectory())walk(f);else if(f.endsWith('.json'))JSON.parse(fs.readFileSync(f,'utf8'))});walk('packages/i18n/src/locales');console.log('all locale JSON valid')"
```
Expected: `all locale JSON valid`.

- [ ] **Step 6: Checkpoint**

```bash
git add packages/i18n/src/locales
```

---

## Task 10: Hardcoded frontend strings & external links (all apps)

**Files** (known clusters; the codemod catches the rest):
- Modify: `apps/web/core/components/workspace/billing/comparison/plans.tsx`
- Modify: `apps/web/core/components/onboarding/**`
- Modify: `apps/web/core/components/account/auth-forms/**`
- Modify: `apps/web/core/components/auth-screens/**`
- Modify: `apps/web/core/components/instance/not-ready-view.tsx`
- Modify: `apps/web/core/components/instance/maintenance-message.tsx`
- Modify: `apps/web/core/layouts/auth-layout/workspace-wrapper.tsx`
- Modify: `apps/web/core/components/common/latest-feature-block.tsx`
- Modify: `apps/web/core/components/common/activity/**`
- Modify: `apps/web/core/components/inbox/**`
- Modify: `apps/web/core/components/issues/**`
- Modify: `apps/web/core/components/license/modal/**`
- Modify: `apps/web/app/(all)/workspace-invitations/page.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/star-us-link.tsx`
- Modify: `apps/web/app/error/prod.tsx`
- Modify: `apps/web/core/components/global/product-updates/{footer,fallback}.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/help-section/root.tsx`
- Modify: `apps/web/core/components/power-k/config/help-commands.ts`
- Modify: `apps/web/core/components/estimates/root.tsx`
- Modify: `apps/web/core/components/account/terms-and-conditions.tsx`
- Modify (admin): `apps/admin/app/(all)/(dashboard)/**`, `apps/admin/components/**`, `apps/admin/hooks/**`
- Modify (space): `apps/space/components/common/powered-by.tsx`, `apps/space/components/account/**`, `apps/space/app/error.tsx`, `apps/space/lib/instance-provider.tsx`, `apps/space/components/instance/instance-failure-view.tsx`

- [ ] **Step 1: Run the codemod for copy and repointable URLs**

Run: `node scripts/terraline-rebrand.mjs --write`
Expected: `Plane` → `Terraline`, `plane.so`/`plane.sh` → `terraline.space` across the
files above. The codemod also covers long-tail `planes.so` → `terraline.space`,
`plane.town` → `terraline.space`, `plane-github-enterprise` →
`terraline-github-enterprise` (added after Task 9 review).

Then hand-review lowercase and inflected remnants the codemod cannot safely
auto-rewrite: `rg -n "\bplane\b|\bPlanes\b" apps/web apps/admin apps/space` and fix
only clear product references (e.g. "plane account" → "Terraline account"). Do NOT
touch native words in other languages, identifiers, imports, or keys.

- [ ] **Step 2: Remove upstream-only links by hand**

In each file, delete or neutralise these upstream resources (do not repoint them):

- `apps/web/app/(all)/[workspaceSlug]/(projects)/star-us-link.tsx:25` —
  `https://github.com/makeplane/plane`: remove the GitHub star link block or render it
  without a link.
- `apps/web/app/(all)/workspace-invitations/page.tsx:124` — `forum.plane.so`: remove the
  forum link (keep surrounding copy).
- `apps/web/app/error/prod.tsx:20,25,29,30` — `support@plane.so` → `support@terraline.space`;
  `status.plane.so` → `status.terraline.space`; remove `@planepowers` social reference.
- `apps/web/core/components/global/product-updates/footer.tsx:20,31,42,53,62` and
  `fallback.tsx:19-20` — remove `go.plane.so` and `forum.plane.so` links; keep
  `support@terraline.space` and `terraline.space/changelog`.
- `apps/web/core/components/workspace/sidebar/help-section/root.tsx:49,55,80` — remove
  `go.plane.so` and `forum.plane.so`; `sales@plane.so` → `sales@terraline.space`.
- `apps/web/core/components/power-k/config/help-commands.ts:40,53,66` — remove
  `forum.plane.so` and `github.com/makeplane/plane/issues`; `docs.plane.so` →
  `docs.terraline.space`.
- `apps/web/core/components/estimates/root.tsx:113` — `docs.plane.so` →
  `docs.terraline.space`.
- Verification: `rg -n "makeplane|forum\.plane|go\.plane|planepowers|plane\.sh" apps/web | grep -v "@makeplane/propel"`
  must return no results. (`@makeplane/propel` package imports and `Plane Software,
  Inc.` copyright headers are intentionally excluded — internal identifiers.)

- [ ] **Step 3: Admin/space upstream links and copy**

The codemod in Step 1 already rewrote `Plane` copy and `plane.so` URLs in admin/space.
Now verify and remove upstream-only resources:

- `apps/admin/components/instance/setup-form.tsx:416` — `developers.plane.so` →
  `developers.terraline.space` (codemod handles it; confirm).
- `apps/admin/app/(all)/(dashboard)/sidebar-help-section.tsx` — "Redirect to Plane"
  becomes "Redirect to Terraline"; remove any `makeplane`/`forum` links.
- `apps/space/components/common/powered-by.tsx` — "Powered by Terraline Publish".
- `apps/space/app/error.tsx` — replace the "That crashed Plane" copy.
- Verification:
  `rg -n "makeplane|forum\.plane|go\.plane|planepowers|plane\.sh|\bPlane\b" apps/admin apps/space | grep -v "Plane Software, Inc" | grep -v propel`
  must return no results. (Same exclusions as Step 2, plus non-shipped
  `apps/space/README.md` which is repo docs, out of scope.)

- [ ] **Step 4: Type-check all apps**

Run: `pnpm --filter=web check:types && pnpm --filter=admin check:types && pnpm --filter=space check:types`
Expected: PASS.

- [ ] **Step 5: Checkpoint**

```bash
git add apps/web apps/admin apps/space
```

---

## Task 11: Backend, email templates, seeds

**Files:**
- Modify: `apps/api/templates/emails/**` (12 templates)
- Modify: `apps/api/templates/base.html`, `apps/api/templates/admin/base_site.html`
- Modify: `apps/api/plane/settings/openapi.py`, `apps/api/plane/settings/production.py`
- Modify: `apps/api/plane/license/utils/instance_value.py`
- Modify: `apps/api/plane/license/management/commands/register_instance.py`
- Modify: `apps/api/plane/bgtasks/{forgot_password,magic_link_code,user_activation_email,user_deactivation_email,user_email_update,project_add_user_email,project_invitation,workspace_invitation}_task.py`
- Modify: `apps/api/plane/seeds/data/*.json`
- Modify: `apps/api/Dockerfile.api`, `apps/api/Dockerfile.dev`
- Modify: `apps/api/plane/static/logos/Logo.png` *(regenerated in Task 6 pattern)*
- Modify: `apps/api/plane/utils/otlp_endpoints.py`

- [ ] **Step 1: Email templates — text**

In each email HTML under `apps/api/templates/emails/`:
- Replace visible `Plane` with `Terraline` (titles, body, `alt`, buttons).
- Replace footer `Plane Software, Inc.` with `Terraline`.
- Replace any `plane.so` / `plane.sh` URL with `terraline.space`.
- Remove all `<img>` tags whose `src` hosts `media.docs.plane.so` or
  `plane-marketing.s3.../plane-assets/...`. Replace each removed header logo with:

```html
<span style="font-family: Inter, Arial, Helvetica, sans-serif; font-size: 20px; font-weight: 600; color: #0A0A0A;">Terraline</span>
```
Use `color: #FFFFFF` instead of `#0A0A0A` in the five templates whose header table
is black (`background-color: #000000`): `auth/magic_signin.html`,
`auth/forgot_password.html`, `invitations/workspace_invitation.html`,
`notifications/project_addition.html`, `exports/analytics.html`.
- Remove footer social blocks that link to `github.com/makeplane`,
  `linkedin.com/company/planepowers`, `x.com/planepowers`, `forum.plane.so`,
  `plane.sh` (delete the `<a>`/`<img>` markup, keep neutral copy where present).

Verify:
```bash
rg -n "Plane|makeplane|planepowers|forum\.plane|go\.plane|plane\.sh|media\.docs\.plane|plane-assets" apps/api/templates
```
Expected: no output.

- [ ] **Step 2: Django base templates**

- `apps/api/templates/base.html:10` → `<title>Hello Terraline!</title>`
- `apps/api/templates/admin/base_site.html:3` → `{% trans 'Terraline Admin' %}`,
  `:20` → `{% trans 'Terraline Django Admin' %}`

- [ ] **Step 3: API settings**

- `apps/api/plane/settings/openapi.py`: `"TITLE": "The Terraline REST API"`; description
  and `developers.plane.so` → `developers.terraline.space`; `"name": "Terraline"`;
  `"url": "https://terraline.space"`; `"email": "support@terraline.space"`; license URL
  → `https://github.com/terraline/terraline/blob/preview/LICENSE.txt`; server
  `https://api.terraline.space`.
- `apps/api/plane/settings/production.py:23` → `SCOUT_NAME = "Terraline"`.
- `apps/api/plane/license/utils/instance_value.py:56` →
  `"default": os.environ.get("EMAIL_FROM", "Team Terraline <team@terraline.space>")`.
- `apps/api/plane/license/management/commands/register_instance.py`:
  `instance_name="Terraline Community Edition"`; keep `edition` enum value unchanged
  (internal identifier); the GitHub releases URL → `https://api.github.com/repos/terraline/terraline/releases/latest`.

- [ ] **Step 4: Email subject lines**

In each `apps/api/plane/bgtasks/*_task.py`, replace `Plane` with `Terraline` in the
`subject = ...` strings. Verify:
```bash
rg -n "Plane" apps/api/plane/bgtasks
```
Expected: no output.

- [ ] **Step 5: Seeds**

Run: `node scripts/terraline-rebrand.mjs --write`
Then review `git diff apps/api/plane/seeds/data`. For `media.docs.plane.so/seed_assets/...`
image URLs inside seed HTML, leave the host as-is (demo imagery only) OR strip the `<img>`
tags — choose one and note it in the commit message. Do not repoint them to
`media.docs.terraline.space` (that host does not exist).

- [ ] **Step 6: OTLP telemetry default**

In `apps/api/plane/utils/otlp_endpoints.py`, change the default endpoint constant
`https://telemetry.plane.so` to `""` (empty disables default export). This intentionally
removes the upstream Plane telemetry sink. Because `grpc_endpoint_from_url("")`
falls back to a hardcoded host, ALSO add an early return in
`_collect_and_push_metrics()` in `apps/api/plane/license/bgtasks/telemetry_metrics.py`
(right after the `is_telemetry_enabled` check): skip when `OTLP_ENDPOINT` env is
unset/empty, so an empty default truly disables export while explicit configuration
keeps working.

Do NOT rebrand the proprietary license header in `apps/api/plane/utils/email.py`
("Plane Commercial License", `plane.so/legals/eula`, `LicenseRef-Plane-Commercial`):
it says DO NOT modify, the licensor is Plane Software, Inc., and the terraline.space
EULA URL does not exist. Revert that hunk if the sweep touches it; its residual
`Plane` matches are expected in verification.

- [ ] **Step 7: Docker build args / changelog URL**

- `apps/api/Dockerfile.api:7` and `apps/api/Dockerfile.dev:7` →
  `ENV INSTANCE_CHANGELOG_URL=https://sites.terraline.space/`
- `apps/admin/Dockerfile.admin:56,58` → `ARG VITE_WEBSITE_URL="https://terraline.space"`,
  `ARG VITE_SUPPORT_EMAIL="support@terraline.space"`

- [ ] **Step 8: Regenerate API static logo**

Generate `apps/api/plane/static/logos/Logo.png` as the Terraline wordmark using the same
`lockup` helper (white fill on transparent) from `scripts/terraline-assets.mjs`; add this
target to that script and re-run it.

- [ ] **Step 9: Verify no residual user-facing strings**

Run:
```bash
rg -n "\bPlane\b|planes?\.so|plane\.sh|plane\.town|plane-github-enterprise|makeplane|planepowers" apps/api --glob '!**/tests/**' | rg -v "Plane Software, Inc.|SPDX"
```
Expected: no output (or only intentional internal identifiers documented in the spec).
Also hand-review lowercase `\bplane\b` in user-facing strings (email bodies, subjects)
and fix clear product references only.

Follow-up applied during Task 13 audit: `apps/api-rs` was outside the codemod globs
but carries 4 user-facing sample-email strings in
`apps/api-rs/crates/api/src/routes/instance_admin.rs` — fixed by hand to mirror the
Django side: default `EMAIL_FROM` → `"Team Terraline <team@terraline.space>"`,
test subject/body → `"Email Notification from Terraline"` / `"…sent from Terraline
application."` (verbatim mirror of `configuration.py:117-118`), test sender →
`"Terraline <terraline@example.com>"`.

- [ ] **Step 10: Checkpoint**

```bash
git add apps/api
```

---

## Task 12: Repo-root user-facing files (README/docs optional)

**Files:**
- Modify: `README.md` (logo + name only, if in scope for the fork)
- Modify: `package.json` (description only; **not** the `name`/repo URL)

- [ ] **Step 1: Decide scope**

Repo docs are not product UI. Per spec §Out of scope they are skipped. If the user asks
for them, run the codemod is **not** safe on root README (it contains license text and
`makeplane/plane` links). Handle README by hand:
- replace the logo image with a Terraline mark,
- replace "Plane" prose with "Terraline",
- keep or remove upstream links per user preference.

- [ ] **Step 2: Checkpoint**

No change unless requested.

---

## Task 13: Final audit, checks, build, smoke

**Files:** none (verification only)

- [ ] **Step 1: Full codemod audit**

Run: `node scripts/terraline-rebrand.mjs --check`
Expected: no matches, or only matches inside excluded paths. Every remaining match must be
a documented internal identifier.

- [ ] **Step 2: Repo-wide residual audit**

Run:
```bash
rg -n "\bPlane\b" --glob '!**/node_modules/**' --glob '!**/build/**' --glob '!**/dist/**' --glob '!**/.react-router/**' | rg -v "Plane Software, Inc.|SPDX-|docs/|README|CONTRIBUTING|SECURITY|CODE_OF_CONDUCT|deployments/|COPYRIGHT|\.github/|pnpm-lock|scripts/terraline-rebrand|LICENSE"
```
Expected: only intentional internal identifiers (e.g. `packages/api`-style none) — review
each line.

- [ ] **Step 3: Lint, format, types**

Run: `pnpm check`
Expected: PASS. If format fails, run `pnpm fix:format` and re-run.

- [ ] **Step 4: Build web**

Run: `pnpm --filter=web build`
Expected: build succeeds.

- [ ] **Step 5: Prod restart (per AGENTS.md)**

Run:
```bash
systemctl --user restart plane-web-prod.service
systemctl --user status plane-web-prod.service --no-pager | head
```
Expected: service active/running. (Only if the tunnel demo must reflect the change.)

- [ ] **Step 6: Smoke test**

On the running app:
1. Login page shows the Terraline lockup, no "Plane" text.
2. Loading state shows the animated strata spinner; with OS reduced-motion enabled it is
   static.
3. Browser tab title is `Terraline | …`; PWA install name is `Terraline`.
4. Admin dashboard and Space publish page show Terraline.
5. Render one email template (e.g. via `python manage.py test_email` or the Django admin
   preview) and confirm no Plane logo/text.

- [ ] **Step 7: Final checkpoint**

```bash
git status --short
```
Report the full change set to the user.

---

## Self-review notes

- Spec coverage: logo (Task 2/3/4), animation (Task 5), raster assets (Task 6), text &
  metadata (Tasks 7/8/10), i18n (Task 9), backend/email (Task 11), external links
  (Task 10/11), out-of-scope internal identifiers preserved (header + codemod
  exclusions), verification (Task 13).
- Type/name consistency: `PlaneLogo`/`PlaneLockup`/`PlaneWordmark`/`PlaneNewIcon`/
  `LogoSpinner` names are unchanged everywhere; the mark geometry constants
  (`85/63.75/42.5`, `y=0/19/38`, `rx=7`) are identical across Tasks 2, 3, 4, 6.
- Known accepted trade-offs: SVG `<text>` wordmark depends on Inter (email uses HTML
  text); `favicon.ico` is a renamed 32×32 PNG (modern browsers accept it); unused
  Plane-branded assets (`plane-takeoff.png`, `instance-*.webp`, dead `plane-logos/*.png`
  beyond the ones regenerated) are left in place because they are not user-facing.
