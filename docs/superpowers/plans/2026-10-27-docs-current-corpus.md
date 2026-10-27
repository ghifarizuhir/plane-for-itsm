# Docs Current Corpus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the stale `docs/` corpus with a current, source-linked engineering handbook and add a lightweight drift guard.

**Architecture:** Add dependency-free Node scripts that parse web routes, Rust API routes, and SQL migrations into committed inventories, then verify document frontmatter and source hashes. Rewrite the current documents around those inventories, delete superseded documents, and run the checks in CI.

**Tech Stack:** Node.js 22 ESM, `node:test`, Markdown, GitHub Actions, pnpm scripts, `oxfmt`.

## Document Frontmatter

Every current document starts with this shape. List the source paths named in the task; do not add `source_hash` because `pnpm docs:verify` writes it.

```yaml
---
status: current
last_verified: 2026-10-27
source_paths:
  - apps/web/app/routes/core.ts
---
```

Do not include the document itself or its parent directory in `source_paths`.

---

## Task 0: Baseline

**Files:**

- Read: `docs/README.md`, `docs/PRODUCT.md`, `docs/CHEAT-SHEET.md`, `docs/design/*.md`, `docs/features/*.md`, `docs/ui/*.md`
- Read: `apps/web/app/routes/core.ts`, `apps/api-rs/crates/api/src/main.rs`, `apps/api-rs/migrations/*.sql`

- [ ] **Step 1: Confirm a clean tree**

Run:

```bash
git status --short
```

Expected: no output. If there are unrelated changes, stop and ask the user before proceeding.

- [ ] **Step 2: Record the current route, API, and table counts**

Run:

```bash
rg -c 'route\(\s*"' apps/web/app/routes/core.ts
rg -c '\.route\("' apps/api-rs/crates/api/src/main.rs
find apps/api-rs/migrations -name '*.sql' | wc -l
```

Expected: three positive numbers. Use them later to spot parser under-counts.

- [ ] **Step 3: Confirm the baseline is read-only**

Do not commit in this task. If Step 1 found unrelated changes, stop and ask the user before proceeding.

---

## Task 1: Frontmatter, Source Hashing, Links, and Banned Claims

**Files:**

- Create: `scripts/docs-lib.mjs`
- Create: `scripts/docs-lib.test.mjs`

- [ ] **Step 1: Write the failing tests**

Create `scripts/docs-lib.test.mjs`:

```js
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  assertNoBannedClaims,
  checkLinks,
  extractRelativeLinks,
  hashSources,
  parseFrontmatter,
  stringifyFrontmatter,
} from "./docs-lib.mjs";

test("frontmatter round-trips known fields", () => {
  const source = [
    "---",
    "status: current",
    "last_verified: 2026-10-27",
    "source_hash: sha256:abc",
    "source_paths:",
    "  - apps/web/app/routes/core.ts",
    "---",
    "",
    "# Title",
    "",
  ].join("\n");

  const parsed = parseFrontmatter(source);
  assert.equal(parsed.data.status, "current");
  assert.equal(parsed.data.last_verified, "2026-10-27");
  assert.deepEqual(parsed.data.source_paths, ["apps/web/app/routes/core.ts"]);
  assert.equal(stringifyFrontmatter(parsed.data) + parsed.body, source);
});

test("hashSources is deterministic and includes the path", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "docs-lib-"));
  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(path.join(root, "src/a.ts"), "export const a = 1;\n");
  fs.writeFileSync(path.join(root, "src/b.ts"), "export const b = 2;\n");

  const first = hashSources(root, ["src"]);
  const second = hashSources(root, ["src"]);
  assert.match(first, /^sha256:[0-9a-f]{64}$/);
  assert.equal(first, second);

  fs.writeFileSync(path.join(root, "src/b.ts"), "export const b = 3;\n");
  assert.notEqual(first, hashSources(root, ["src"]));
});

test("link extraction ignores external links, anchors, and queries", () => {
  const markdown = [
    "[local](../README.md)",
    "[anchor](#section)",
    "[web](https://example.com)",
    "[mail](mailto:x@example.com)",
    "[query](../README.md?plain=1#top)",
  ].join("\n");

  assert.deepEqual(extractRelativeLinks(markdown), ["../README.md", "../README.md"]);
});

test("checkLinks reports missing relative targets", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "docs-links-"));
  fs.writeFileSync(path.join(root, "README.md"), "# Root\n");
  fs.writeFileSync(path.join(root, "page.md"), "[ok](./README.md)\n[bad](./missing.md)\n");

  assert.deepEqual(checkLinks(root, "page.md", fs.readFileSync(path.join(root, "page.md"), "utf8")), [
    "page.md: missing link ./missing.md",
  ]);
});

test("banned claims are reported with the document path", () => {
  assert.deepEqual(assertNoBannedClaims("Plane for ITSM still has 6 apps.", "docs/x.md"), [
    "docs/x.md: banned claim `Plane for ITSM`",
    "docs/x.md: banned claim `6 apps`",
  ]);
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```bash
node --test scripts/docs-lib.test.mjs
```

Expected: FAIL with `Cannot find module './docs-lib.mjs'`.

- [ ] **Step 3: Implement the library**

Create `scripts/docs-lib.mjs`:

```js
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export const BANNED_CLAIMS = ["Plane for ITSM", "ITSM is future", "ITSM remains future", "6 apps", "six apps"];

export function readText(relPath, root = ROOT) {
  return fs.readFileSync(path.join(root, relPath), "utf8");
}

export function parseFrontmatter(text) {
  if (!text.startsWith("---\n")) throw new Error("missing frontmatter");
  const end = text.indexOf("\n---\n", 4);
  if (end === -1) throw new Error("unterminated frontmatter");

  const raw = text.slice(4, end);
  const body = text.slice(end + 5);
  const data = {};
  let currentList = null;

  for (const line of raw.split("\n")) {
    if (!line.trim()) continue;

    const listMatch = line.match(/^\s+-\s+(.+)$/);
    if (listMatch && currentList) {
      currentList.push(listMatch[1].trim());
      continue;
    }

    const kv = line.match(/^([A-Za-z0-9_]+):\s*(.*)$/);
    if (!kv) throw new Error(`invalid frontmatter line: ${line}`);

    const [, key, value] = kv;
    if (value === "") {
      data[key] = [];
      currentList = data[key];
    } else {
      data[key] = value;
      currentList = null;
    }
  }

  return { data, body, raw };
}

export function stringifyFrontmatter(data) {
  const lines = ["---"];
  for (const key of ["status", "last_verified", "source_hash", "source_paths"]) {
    const value = data[key];
    if (Array.isArray(value)) {
      lines.push(`${key}:`);
      for (const item of value) lines.push(`  - ${item}`);
    } else if (value !== undefined) {
      lines.push(`${key}: ${value}`);
    }
  }
  lines.push("---");
  return `${lines.join("\n")}\n`;
}

function walkFiles(root, relDir, out) {
  const absDir = path.join(root, relDir);
  for (const entry of fs.readdirSync(absDir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const rel = path.join(relDir, entry.name);
    if (entry.isDirectory()) {
      walkFiles(root, rel, out);
    } else if (entry.isFile()) {
      out.push(rel);
    }
  }
}

export function expandSourcePaths(root, sourcePaths) {
  const out = new Set();

  for (const sourcePath of sourcePaths) {
    const abs = path.join(root, sourcePath);
    const stat = fs.statSync(abs);
    if (stat.isDirectory()) {
      const files = [];
      walkFiles(root, sourcePath, files);
      for (const file of files) out.add(file);
    } else if (stat.isFile()) {
      out.add(sourcePath);
    } else {
      throw new Error(`unsupported source path: ${sourcePath}`);
    }
  }

  return [...out].sort();
}

export function hashSources(root, sourcePaths) {
  const hash = crypto.createHash("sha256");
  for (const rel of expandSourcePaths(root, sourcePaths)) {
    hash.update(rel);
    hash.update("\0");
    hash.update(fs.readFileSync(path.join(root, rel)));
    hash.update("\0");
  }
  return `sha256:${hash.digest("hex")}`;
}

export function extractRelativeLinks(markdown) {
  const links = [];
  const re = /\[[^\]]*\]\(([^)]+)\)/g;
  let match;

  while ((match = re.exec(markdown))) {
    let target = match[1].trim();
    if (target.startsWith("<") && target.endsWith(">")) target = target.slice(1, -1);
    if (/^(https?:|mailto:|#)/.test(target)) continue;

    target = target.split("#")[0].split("?")[0];
    if (target) links.push(target);
  }

  return links;
}

export function checkLinks(root, docRelPath, markdown) {
  const errors = [];
  const docDir = path.dirname(path.join(root, docRelPath));

  for (const target of extractRelativeLinks(markdown)) {
    let decoded = target;
    try {
      decoded = decodeURIComponent(target);
    } catch {
      decoded = target;
    }

    const abs = decoded.startsWith("/") ? path.join(root, decoded) : path.resolve(docDir, decoded);
    if (!fs.existsSync(abs)) errors.push(`${docRelPath}: missing link ${target}`);
  }

  return errors;
}

export function assertNoBannedClaims(text, docRelPath) {
  const errors = [];
  const lower = text.toLowerCase();
  for (const claim of BANNED_CLAIMS) {
    if (lower.includes(claim.toLowerCase())) errors.push(`${docRelPath}: banned claim \`${claim}\``);
  }
  return errors;
}

export function walkCurrentDocs(root = ROOT) {
  const docsRoot = path.join(root, "docs");
  const out = [];

  function visit(absDir) {
    for (const entry of fs.readdirSync(absDir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const abs = path.join(absDir, entry.name);
      const rel = path.relative(root, abs);
      if (entry.isDirectory()) {
        if (rel === "docs/_generated" || rel === "docs/superpowers") continue;
        visit(abs);
      } else if (entry.name.endsWith(".md")) {
        out.push(rel);
      }
    }
  }

  visit(docsRoot);
  return out;
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run:

```bash
node --test scripts/docs-lib.test.mjs
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add scripts/docs-lib.mjs scripts/docs-lib.test.mjs
git commit -m "docs: add docs metadata and link helpers"
```

---

## Task 2: Route, API, and Schema Inventories

**Files:**

- Modify: `scripts/docs-lib.mjs`
- Modify: `scripts/docs-lib.test.mjs`

- [ ] **Step 1: Add failing inventory tests**

Append to `scripts/docs-lib.test.mjs`:

```js
import { buildInventories, parseApiRoutes, parseSchemaMigrations, parseWebRoutes } from "./docs-lib.mjs";

test("parseWebRoutes distinguishes canonical and redirect routes", () => {
  const source = [
    'route(":workspaceSlug/services", "./(all)/[workspaceSlug]/(projects)/services/page.tsx");',
    'route(":workspaceSlug/projects/:projectId/services", "routes/redirects/core/project-services.tsx");',
  ].join("\n");

  assert.deepEqual(parseWebRoutes(source), [
    {
      path: "/:workspaceSlug/services",
      file: "./(all)/[workspaceSlug]/(projects)/services/page.tsx",
      kind: "canonical",
    },
    {
      path: "/:workspaceSlug/projects/:projectId/services",
      file: "routes/redirects/core/project-services.tsx",
      kind: "redirect",
    },
  ]);
});

test("parseApiRoutes extracts methods and handlers", () => {
  const source = [
    '.route("/health", get(routes::health::health))',
    '.route("/api/workspaces/:slug/services/", get(routes::service::list).post(routes::service::create))',
  ].join("\n");

  assert.deepEqual(parseApiRoutes(source), [
    {
      path: "/health",
      methods: [{ method: "GET", handler: "routes::health::health" }],
      file: "apps/api-rs/crates/api/src/main.rs",
    },
    {
      path: "/api/workspaces/:slug/services/",
      methods: [
        { method: "GET", handler: "routes::service::list" },
        { method: "POST", handler: "routes::service::create" },
      ],
      file: "apps/api-rs/crates/api/src/main.rs",
    },
  ]);
});

test("parseSchemaMigrations tracks create, alter, and drop", () => {
  const tables = parseSchemaMigrations([
    { name: "0001_initial.sql", sql: "CREATE TABLE workspace (id uuid);\nALTER TABLE workspace ADD COLUMN slug text;" },
    { name: "0002_more.sql", sql: "CREATE TABLE IF NOT EXISTS service (id uuid);\nDROP TABLE old_thing;" },
  ]);

  assert.deepEqual(tables, [
    { table: "old_thing", created: null, lastChanged: "0002_more.sql", dropped: true },
    { table: "service", created: "0002_more.sql", lastChanged: "0002_more.sql", dropped: false },
    { table: "workspace", created: "0001_initial.sql", lastChanged: "0001_initial.sql", dropped: false },
  ]);
});

test("buildInventories reads the real repository", () => {
  const inventories = buildInventories(process.cwd());
  assert.match(inventories["docs/_generated/routes.md"], /\/:workspaceSlug\/services/);
  assert.match(inventories["docs/_generated/api-routes.md"], /\/api\/workspaces\/:slug\/services\//);
  assert.match(inventories["docs/_generated/schema.md"], /workspace_services/);
  assert.match(inventories["docs/_generated/schema.md"], /war_rooms/);
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run:

```bash
node --test scripts/docs-lib.test.mjs
```

Expected: FAIL because `buildInventories`, `parseApiRoutes`, `parseSchemaMigrations`, and `parseWebRoutes` are not exported.

- [ ] **Step 3: Add the parsers and renderers**

Append to `scripts/docs-lib.mjs`:

```js
function dedupeBy(items, keyFn) {
  const seen = new Set();
  return items.filter((item) => {
    const key = keyFn(item);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function parseWebRoutes(source) {
  const routes = [];
  const re = /route\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)/gs;
  let match;

  while ((match = re.exec(source))) {
    const file = match[2];
    const routePath = match[1].startsWith("/") ? match[1] : `/${match[1]}`;
    routes.push({
      path: routePath,
      file,
      kind: file.includes("routes/redirects/") ? "redirect" : "canonical",
    });
  }

  return dedupeBy(routes, (route) => `${route.path}|${route.file}|${route.kind}`);
}

export function parseApiRoutes(source, file = "apps/api-rs/crates/api/src/main.rs") {
  const routes = [];
  let index = 0;

  while ((index = source.indexOf(".route(", index)) !== -1) {
    let cursor = index + ".route(".length;
    const start = cursor;
    let depth = 1;
    let inString = false;
    let escaped = false;

    for (; cursor < source.length && depth > 0; cursor += 1) {
      const char = source[cursor];
      if (inString) {
        if (escaped) escaped = false;
        else if (char === "\\") escaped = true;
        else if (char === '"') inString = false;
      } else if (char === '"') {
        inString = true;
      } else if (char === "(") {
        depth += 1;
      } else if (char === ")") {
        depth -= 1;
      }
    }

    const args = source.slice(start, cursor - 1);
    const pathMatch = args.match(/^\s*"([^"]+)"/);
    if (pathMatch) {
      const methods = [];
      const methodRe = /\b(get|post|put|patch|delete|head|options)\(/g;
      let methodMatch;

      while ((methodMatch = methodRe.exec(args))) {
        const tail = args.slice(methodMatch.index + methodMatch[0].length);
        const handlerMatch = tail.match(/routes::[A-Za-z0-9_:]+/);
        methods.push({
          method: methodMatch[1].toUpperCase(),
          handler: handlerMatch ? handlerMatch[0] : tail.split(",")[0].trim(),
        });
      }

      routes.push({ path: pathMatch[1], methods, file });
    }

    index = cursor;
  }

  return routes;
}

export function parseSchemaMigrations(migrations) {
  const tables = new Map();

  for (const { name, sql } of migrations) {
    const re =
      /\b(CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?|ALTER\s+TABLE(?:\s+ONLY)?|DROP\s+TABLE(?:\s+IF\s+EXISTS)?)\s+"?([A-Za-z0-9_]+)"?/gi;
    let match;

    while ((match = re.exec(sql))) {
      const action = match[1].toUpperCase();
      const table = match[2];
      const entry = tables.get(table) || {
        table,
        created: null,
        lastChanged: name,
        dropped: false,
      };

      if (action.startsWith("CREATE")) entry.created = name;
      if (action.startsWith("DROP")) entry.dropped = true;
      entry.lastChanged = name;
      tables.set(table, entry);
    }
  }

  return [...tables.values()].sort((a, b) => a.table.localeCompare(b.table));
}

function generatedBanner() {
  return "<!-- Generated by pnpm docs:inventory. Do not edit by hand. -->";
}

function routeGroup(route) {
  if (route.kind === "redirect") return "legacy-redirects";
  if (route.path.includes("/galileo") || route.path.includes("/docs")) return "docs-galileo";
  if (route.path.includes("/settings")) return "settings";
  if (route.path.includes("/projects/:projectId/")) return "application";
  return "workspace";
}

function renderRoutesMarkdown(routes) {
  const groups = [
    ["workspace", "Workspace"],
    ["application", "Application"],
    ["settings", "Settings"],
    ["docs-galileo", "Docs and Galileo"],
    ["legacy-redirects", "Legacy redirects"],
  ];
  const lines = [
    generatedBanner(),
    "",
    "# Web Routes",
    "",
    `Generated from \`apps/web/app/routes/core.ts\`. ${routes.length} routes.`,
    "",
  ];

  for (const [key, label] of groups) {
    const items = routes.filter((route) => routeGroup(route) === key).sort((a, b) => a.path.localeCompare(b.path));
    if (!items.length) continue;
    lines.push(`## ${label}`, "", "| Path | Source file | Kind |", "| --- | --- | --- |");
    for (const route of items) lines.push(`| \`${route.path}\` | \`${route.file}\` | ${route.kind} |`);
    lines.push("");
  }

  return `${lines.join("\n").trimEnd()}\n`;
}

function apiGroup(apiRoute) {
  if (apiRoute.path.startsWith("/api/v1/")) return "v1";
  if (apiRoute.path.startsWith("/api/workspaces/")) return "workspaces";
  if (apiRoute.path.startsWith("/api/auth")) return "auth";
  if (apiRoute.path === "/health" || apiRoute.path.startsWith("/api/health")) return "health";
  return "other";
}

function renderApiRoutesMarkdown(routes) {
  const groups = [
    ["workspaces", "Workspace API"],
    ["v1", "Public API v1"],
    ["auth", "Auth"],
    ["health", "Health and ingest"],
    ["other", "Other"],
  ];
  const lines = [
    generatedBanner(),
    "",
    "# API Routes",
    "",
    `Generated from \`apps/api-rs/crates/api/src/main.rs\`. ${routes.length} routes.`,
    "",
  ];

  for (const [key, label] of groups) {
    const items = routes.filter((route) => apiGroup(route) === key).sort((a, b) => a.path.localeCompare(b.path));
    if (!items.length) continue;
    lines.push(`## ${label}`, "", "| Method | Path | Handler |", "| --- | --- | --- |");
    for (const route of items) {
      for (const method of route.methods) {
        lines.push(`| ${method.method} | \`${route.path}\` | \`${method.handler}\` |`);
      }
    }
    lines.push("");
  }

  return `${lines.join("\n").trimEnd()}\n`;
}

function renderSchemaMarkdown(tables) {
  const lines = [
    generatedBanner(),
    "",
    "# Schema Tables",
    "",
    `Generated from \`apps/api-rs/migrations\`. ${tables.length} tables.`,
    "",
    "| Table | Created in | Last changed | Present |",
    "| --- | --- | --- | --- |",
  ];
  for (const table of tables) {
    lines.push(
      `| \`${table.table}\` | ${table.created ? `\`${table.created}\`` : "—"} | \`${table.lastChanged}\` | ${table.dropped ? "no" : "yes"} |`
    );
  }
  return `${lines.join("\n")}\n`;
}

export function buildInventories(root = ROOT) {
  const webRoutes = parseWebRoutes(readText("apps/web/app/routes/core.ts", root));
  if (webRoutes.length < 30) throw new Error(`expected at least 30 web routes, got ${webRoutes.length}`);

  const apiRoutes = parseApiRoutes(readText("apps/api-rs/crates/api/src/main.rs", root));
  if (apiRoutes.length < 40) throw new Error(`expected at least 40 API routes, got ${apiRoutes.length}`);

  const migrationsDir = path.join(root, "apps/api-rs/migrations");
  const migrations = fs
    .readdirSync(migrationsDir)
    .filter((name) => name.endsWith(".sql"))
    .sort()
    .map((name) => ({ name, sql: fs.readFileSync(path.join(migrationsDir, name), "utf8") }));
  const tables = parseSchemaMigrations(migrations);
  if (tables.length < 20) throw new Error(`expected at least 20 schema tables, got ${tables.length}`);

  return {
    "docs/_generated/routes.md": renderRoutesMarkdown(webRoutes),
    "docs/_generated/api-routes.md": renderApiRoutesMarkdown(apiRoutes),
    "docs/_generated/schema.md": renderSchemaMarkdown(tables),
  };
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run:

```bash
node --test scripts/docs-lib.test.mjs
```

Expected: PASS, 9 tests.

- [ ] **Step 5: Commit**

```bash
git add scripts/docs-lib.mjs scripts/docs-lib.test.mjs
git commit -m "docs: add route, API, and schema inventory parsers"
```

---

## Task 3: Inventory, Check, and Verify CLIs

**Files:**

- Create: `scripts/docs-inventory.mjs`
- Create: `scripts/docs-check.mjs`
- Create: `scripts/docs-verify.mjs`
- Modify: `package.json`

- [ ] **Step 1: Add the inventory CLI**

Create `scripts/docs-inventory.mjs`:

```js
#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

import { ROOT, buildInventories } from "./docs-lib.mjs";

const inventories = buildInventories(ROOT);
for (const [relPath, contents] of Object.entries(inventories)) {
  const absPath = path.join(ROOT, relPath);
  fs.mkdirSync(path.dirname(absPath), { recursive: true });
  fs.writeFileSync(absPath, contents);
  console.log(`wrote ${relPath}`);
}
```

- [ ] **Step 2: Add the check CLI**

Create `scripts/docs-check.mjs`:

```js
#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

import {
  ROOT,
  assertNoBannedClaims,
  buildInventories,
  checkLinks,
  expandSourcePaths,
  hashSources,
  parseFrontmatter,
  walkCurrentDocs,
} from "./docs-lib.mjs";

const requested = process.argv.slice(2).map((value) => value.replace(/^\.\//, ""));
const docs = requested.length ? requested : walkCurrentDocs(ROOT);
const errors = [];

for (const docRelPath of docs) {
  const absPath = path.join(ROOT, docRelPath);
  if (!fs.existsSync(absPath)) {
    errors.push(`${docRelPath}: file does not exist`);
    continue;
  }

  const text = fs.readFileSync(absPath, "utf8");
  let parsed;
  try {
    parsed = parseFrontmatter(text);
  } catch (error) {
    errors.push(`${docRelPath}: ${error.message}`);
    continue;
  }

  const { data, body } = parsed;
  if (data.status !== "current") errors.push(`${docRelPath}: status must be current`);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(data.last_verified || "")) {
    errors.push(`${docRelPath}: last_verified must be YYYY-MM-DD`);
  }
  if (!/^sha256:[0-9a-f]{64}$/.test(data.source_hash || "")) {
    errors.push(`${docRelPath}: source_hash must be sha256:<64 hex>`);
  }
  if (!Array.isArray(data.source_paths) || data.source_paths.length === 0) {
    errors.push(`${docRelPath}: source_paths must contain at least one path`);
  } else {
    try {
      const expanded = expandSourcePaths(ROOT, data.source_paths);
      if (expanded.includes(docRelPath)) {
        errors.push(`${docRelPath}: source_paths must not include the document itself`);
      }
      const actual = hashSources(ROOT, data.source_paths);
      if (actual !== data.source_hash) {
        errors.push(`${docRelPath}: stale source_hash; run pnpm docs:verify ${docRelPath}`);
      }
    } catch (error) {
      errors.push(`${docRelPath}: ${error.message}`);
    }
  }

  errors.push(...checkLinks(ROOT, docRelPath, body));
  errors.push(...assertNoBannedClaims(text, docRelPath));
}

for (const [relPath, expected] of Object.entries(buildInventories(ROOT))) {
  const absPath = path.join(ROOT, relPath);
  if (!fs.existsSync(absPath)) {
    errors.push(`${relPath}: missing; run pnpm docs:inventory`);
  } else if (fs.readFileSync(absPath, "utf8") !== expected) {
    errors.push(`${relPath}: out of date; run pnpm docs:inventory`);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(`docs check passed (${docs.length} docs, 3 inventories)`);
```

- [ ] **Step 3: Add the verify CLI**

Create `scripts/docs-verify.mjs`:

```js
#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

import { ROOT, expandSourcePaths, hashSources, parseFrontmatter, stringifyFrontmatter } from "./docs-lib.mjs";

const docs = process.argv.slice(2).map((value) => value.replace(/^\.\//, ""));
if (docs.length === 0) {
  console.error("usage: pnpm docs:verify <doc.md> [more.md]");
  process.exit(1);
}

for (const docRelPath of docs) {
  const absPath = path.join(ROOT, docRelPath);
  const text = fs.readFileSync(absPath, "utf8");
  const { data, body } = parseFrontmatter(text);

  if (!Array.isArray(data.source_paths) || data.source_paths.length === 0) {
    throw new Error(`${docRelPath}: source_paths must contain at least one path`);
  }
  if (expandSourcePaths(ROOT, data.source_paths).includes(docRelPath)) {
    throw new Error(`${docRelPath}: source_paths must not include the document itself`);
  }

  const next = {
    ...data,
    status: data.status || "current",
    last_verified: new Date().toISOString().slice(0, 10),
    source_hash: hashSources(ROOT, data.source_paths),
  };

  fs.writeFileSync(absPath, stringifyFrontmatter(next) + body);
  console.log(`verified ${docRelPath}`);
}
```

- [ ] **Step 4: Add root scripts**

Modify the `scripts` block in `package.json`:

```json
{
  "scripts": {
    "build": "turbo run build",
    "dev": "turbo run dev --concurrency=18",
    "start": "turbo run start",
    "clean": "turbo run clean && rm -rf .turbo && rm -rf .next && rm -rf node_modules && rm -rf dist",
    "fix": "turbo run fix",
    "fix:format": "turbo run fix:format",
    "fix:lint": "turbo run fix:lint",
    "check": "turbo run check",
    "check:lint": "turbo run check:lint",
    "check:format": "turbo run check:format",
    "check:types": "turbo run check:types",
    "docs:inventory": "node scripts/docs-inventory.mjs",
    "docs:verify": "node scripts/docs-verify.mjs",
    "docs:check": "node scripts/docs-check.mjs",
    "docs:test": "node --test scripts/docs-*.test.mjs",
    "prepare": "husky",
    "doctor": "npx react-doctor@latest"
  }
}
```

- [ ] **Step 5: Smoke-test the CLIs**

Run:

```bash
node scripts/docs-inventory.mjs
node --test scripts/docs-lib.test.mjs
cat > docs/_verify-smoke.md <<'EOF'
---
status: current
last_verified: 2026-10-27
source_paths:
  - README.md
---

# Smoke
EOF
node scripts/docs-verify.mjs docs/_verify-smoke.md
rg -n '^source_hash: sha256:[0-9a-f]{64}$' docs/_verify-smoke.md
rm docs/_verify-smoke.md
```

Expected: inventory files are written; tests pass; the smoke file gains a real `source_hash`; the `rg` command finds it.

- [ ] **Step 7: Commit**

```bash
git add package.json scripts/docs-inventory.mjs scripts/docs-check.mjs scripts/docs-verify.mjs
git commit -m "docs: add inventory, check, and verify commands"
```

Do not commit `docs/_generated` yet; Task 4 creates the final inventory from the current sources.

---

## Task 4: Generate Inventories and Archive Notice

**Files:**

- Create: `docs/_generated/routes.md`
- Create: `docs/_generated/api-routes.md`
- Create: `docs/_generated/schema.md`
- Create: `docs/superpowers/README.md`

- [ ] **Step 1: Generate the inventories**

Run:

```bash
pnpm docs:inventory
```

Expected: three files under `docs/_generated/`.

- [ ] **Step 2: Inspect the generated files**

Run:

```bash
sed -n '1,40p' docs/_generated/routes.md
sed -n '1,40p' docs/_generated/api-routes.md
sed -n '1,30p' docs/_generated/schema.md
```

Expected:

- `routes.md` contains `/:workspaceSlug/services`, `/:workspaceSlug/work-items`, `/:workspaceSlug/galileo`, and `/projects/:projectId/issues`.
- `api-routes.md` contains `/api/workspaces/:slug/services/` and `/api/workspaces/:slug/war-rooms/`.
- `schema.md` contains `services`, `war_rooms`, `workspace_services`, and `service_metrics`.

If any entry is missing, fix the parser in `scripts/docs-lib.mjs` and its test instead of editing the generated file.

- [ ] **Step 3: Add the archive notice**

Create `docs/superpowers/README.md`:

```markdown
# Superpowers Archive

This directory is a historical archive of specs, plans, decision records, and
smoke-test notes. It is not part of the current documentation corpus.

- Do not use these files as a description of current behavior.
- Do not edit historical files to make them match new code.
- Current implementation docs live in `docs/design/`, `docs/features/`, and
  `docs/ui/`.
- Generated route, API, and schema inventories live in `docs/_generated/`.

Git history is the source of truth for when each historical file was written.
```

- [ ] **Step 4: Run the inventory check**

Run:

```bash
pnpm docs:inventory
git diff --exit-code docs/_generated
node --test scripts/docs-lib.test.mjs
```

Expected: no inventory diff; tests pass. `pnpm docs:check` is expected to fail until the current docs are rewritten.

- [ ] **Step 5: Commit**

```bash
git add docs/_generated docs/superpowers/README.md
git commit -m "docs: add generated inventories and archive notice"
```

---

## Task 5: Core Documents — Product and Decisions

**Files:**

- Rewrite: `docs/PRODUCT.md`
- Rewrite: `docs/CHEAT-SHEET.md`
- Add frontmatter: `docs/linting.md`
- Delete: `docs/business-capabilities.md`

- [ ] **Step 1: Draft both documents with frontmatter**

Use this frontmatter shape for both documents. Do not add `source_hash`; `pnpm docs:verify` writes it.

```yaml
---
status: current
last_verified: 2026-10-27
source_paths:
  - README.md
  - apps/web/app/routes/core.ts
---
```

Use these source paths:

- `docs/PRODUCT.md`: `README.md`, `apps/web/app/routes/core.ts`, `apps/api-rs/migrations`
- `docs/CHEAT-SHEET.md`: `package.json`, `pnpm-workspace.yaml`, `apps/web/app/routes/core.ts`, `apps/api-rs/migrations`

- [ ] **Step 2: Write `docs/PRODUCT.md`**

Required sections:

1. `# Product — Terraline`
2. `## Platform` — Terraline is a modified Plane fork for IT service management.
3. `## Users` — members, application admins, workspace admins, instance admins.
4. `## Product purpose` — one system for service operations and delivery work.
5. `## ITIL4 map` — table copied from the root `README.md`; keep the two explicit boundaries: no SLA engine/CAB board, health is a catalog indicator not live monitoring.
6. `## Current capabilities` — Tickets, Intake, Services, War Rooms, Sprints, Tracks, Queues, Knowledge base, Reports, AI, Applications.
7. `## Terminology` — Tickets/work items, Sprints/cycles, Tracks/modules, Knowledge/pages, Applications/projects.
8. `## Non-goals` — no SLA engine, no CAB approval board, no live monitoring feed, no Problem/Change/Asset modules beyond the ticket types and linked surfaces.
9. `## Operating context` — browser SPA, workspace/application scope, MobX/SWR, TipTap/Yjs, Rust API.
10. `## Changelog`.

Read `README.md` §ITIL4 practice map through §Applications and the route families in `core.ts` before writing. Do not copy line numbers.

- [ ] **Step 3: Write `docs/CHEAT-SHEET.md`**

Required sections:

1. `# Cross-Cutting Decisions — Cheat Sheet`
2. `## Stack` — pnpm + Turbo, Rust Axum + SQLx, React 19 + React Router 8 + Vite + Tailwind 4 + MobX + SWR, TipTap 2 + Yjs + Hocuspocus.
3. `## Monorepo layout` — seven apps (`web`, `admin`, `live`, `space`, `api-rs`, `api`, `proxy`) and fifteen packages; list the packages that exist in `packages/`.
4. `## Scope model` — workspace → application → ticket; workspace-level services, intake, sprints, tracks, pages, views, settings.
5. `## Backend` — `apps/api-rs` is primary; `apps/api` is the opt-in Django fallback; Redis Stream jobs.
6. `## Data` — Postgres per-table; migrations `0001`–`0025`; no `entities` JSONB; link to `design/02-data-model.md` and `_generated/schema.md`.
7. `## Auth` — `X-Api-Key` + Bearer; session/CSRF only in the Django fallback; workspace/application roles.
8. `## Routing` — link to `design/07-routing-and-navigation.md` and `_generated/routes.md`.
9. `## Testing and ops` — link to `design/05` and `design/06`.
10. `## Changelog`.

Use the exact counts: seven apps, fifteen packages. Do not write six apps.

- [ ] **Step 4: Add frontmatter to `docs/linting.md`**

Keep the existing linting content. Add:

```yaml
---
status: current
last_verified: 2026-10-27
source_paths:
  - package.json
  - .oxlintrc.json
  - .oxfmtrc.json
  - turbo.json
---
```

Then run `pnpm docs:verify docs/linting.md`.

- [ ] **Step 5: Delete the superseded capability snapshot**

Run:

```bash
git rm docs/business-capabilities.md
```

- [ ] **Step 6: Verify and check the three documents**

Run:

```bash
pnpm docs:verify docs/PRODUCT.md docs/CHEAT-SHEET.md docs/linting.md
pnpm docs:check docs/PRODUCT.md docs/CHEAT-SHEET.md docs/linting.md
```

Expected: all three verified; `docs check passed` with 3 docs and 3 inventories. If a generated inventory is stale, run `pnpm docs:inventory` and retry.

- [ ] **Step 7: Commit**

```bash
git add docs/PRODUCT.md docs/CHEAT-SHEET.md docs/linting.md docs/business-capabilities.md
git commit -m "docs: rewrite product overview and architecture cheat sheet"
```

---

## Task 6: Design Documents 01–03

Every file in this task starts with the standard frontmatter and ends with `pnpm docs:verify`.

**Files:**

- Rewrite: `docs/design/01-architecture.md`
- Rewrite: `docs/design/02-data-model.md`
- Rewrite: `docs/design/03-api-contract.md`

- [ ] **Step 1: Write `docs/design/01-architecture.md`**

Start with the standard frontmatter. Required sections:

1. `# 01 — Architecture`
2. `## Design principles` — monorepo boundaries, Postgres source of truth, strict TypeScript, thin Rust handlers, feature colocation, Docker parity.
3. `## Applications` — table of seven apps with port/role: `web`, `admin`, `live`, `space`, `api-rs`, `api`, `proxy`.
4. `## Packages` — list the fifteen packages under `packages/` and their roles.
5. `## Workspace and application model` — workspace is tenant; application is the user-facing project; tickets belong to one application; services are workspace-level.
6. `## Backend` — Rust Axum + SQLx + Redis Stream; Django fallback.
7. `## Frontend` — React Router SPA, MobX, SWR, TipTap/Yjs, app rail.
8. `## Realtime and AI` — `apps/live` + Hocuspocus; Galileo + scheduler.
9. `## Dependency rules` — apps do not import apps; packages do not import apps; workspace imports use `workspace:*`.
10. `## Changelog`.

Source paths: `package.json`, `pnpm-workspace.yaml`, `turbo.json`, `apps/web/package.json`, `apps/live/package.json`, `apps/api-rs/Cargo.toml`.

- [ ] **Step 2: Write `docs/design/02-data-model.md`**

Required sections:

1. `# 02 — Data Model`
2. `## Design principles` — Postgres per-table, workspace/application scoping, soft delete, UUID/string IDs.
3. `## Migration baseline` — explain `0001_initial.sql` plus migrations `0002`–`0025`; link to `_generated/schema.md`.
4. `## Core domains` — workspace, application/project, ticket/issue, state, label, estimate, work item type, cycle, module, page, view.
5. `## ITSM extensions` — services, service dependencies, service metrics, service kind, war rooms, intake sources, AI schedules/conversations, workspace settings, taxonomy.
6. `## Removed concepts` — workflows removed in `0016`; release review removed in `0018`.
7. `## Soft delete and retention` — `deleted_at`, archived entities, restore paths.
8. `## Changelog`.

Source paths: `apps/api-rs/migrations`, `apps/api/plane/db/models`.

- [ ] **Step 3: Write `docs/design/03-api-contract.md`**

Required sections:

1. `# 03 — API Contract`
2. `## Base and conventions` — `http://localhost:8000`; JSON; ISO timestamps; string IDs.
3. `## Authentication` — `X-Api-Key`, Bearer, legacy Django session/CSRF only for `api-legacy`.
4. `## Authorization scopes` — instance, workspace, application, member, guest.
5. `## Route families` — workspace API, application API, v1 API, auth, health/ingest; link to `_generated/api-routes.md`.
6. `## Pagination, filtering, sorting`.
7. `## Error shape` and rate limits.
8. `## Changelog`.

Source paths: `apps/api-rs/crates/api/src/main.rs`, `apps/api-rs/crates/api/src/routes`.

- [ ] **Step 4: Verify and check**

Run:

```bash
pnpm docs:verify docs/design/01-architecture.md docs/design/02-data-model.md docs/design/03-api-contract.md
pnpm docs:check docs/design/01-architecture.md docs/design/02-data-model.md docs/design/03-api-contract.md
```

Expected: all three verified; check passes.

- [ ] **Step 5: Commit**

```bash
git add docs/design/01-architecture.md docs/design/02-data-model.md docs/design/03-api-contract.md
git commit -m "docs: rewrite architecture, data model, and API contract"
```

---

## Task 7: Design Documents 04–07 and Design Index

Every file in this task starts with the standard frontmatter and ends with `pnpm docs:verify`.

**Files:**

- Rewrite: `docs/design/04-design-system.md`
- Rewrite: `docs/design/05-testing-strategy.md`
- Rewrite: `docs/design/06-ops-runbook.md`
- Create: `docs/design/07-routing-and-navigation.md`
- Rewrite: `docs/design/README.md`

- [ ] **Step 1: Write `docs/design/04-design-system.md`**

Start with the standard frontmatter. Required content:

- Tokens from `packages/tailwind-config` and `packages/ui`.
- Primitives: icon chip, eyebrow, page heading, section divider, meta row, mono chip.
- Shell: app rail modes (Work, Procedure, Galileo, Settings), workspace sidebar, mobile menu.
- Radius, shadow, typography, motion.
- Link `docs/ui/design-tokens.md`, `docs/ui/shell.md`, `docs/ui/editor.md`.

Source paths: `packages/tailwind-config`, `packages/ui/src`, `apps/web/core/components/navigation`.

- [ ] **Step 2: Write `docs/design/05-testing-strategy.md`**

Required content:

- Test layers and commands: `cargo test` workspace; serial scratch suites with `--test-threads=1`; Django Docker suite via `docker-compose-test.yml`; Vitest/RTL for web/packages; docs tests.
- Reference `AGENTS.md` for backend test setup.
- Explain that docs checks are `pnpm docs:test` and `pnpm docs:check`.
- Remove obsolete version claims unless verified from package files.

Source paths: `package.json`, `apps/api-rs/Cargo.toml`, `docker-compose-test.yml`, `AGENTS.md`, `.github/workflows`.

- [ ] **Step 3: Write `docs/design/06-ops-runbook.md`**

Required content:

- Local services: prod web service, dev web service, live service, backend Docker stack.
- Correct `docker-compose-local.yml` usage for api-rs; Django fallback is opt-in.
- Rebuild commands from `AGENTS.md`: `pnpm turbo run build --filter=web`, `systemctl --user restart`, backend rebuild, live rebuild.
- Ports: web 3000, admin 3001, API 8000, live 3100.
- Tunnel and mixed-content constraints.
- Environment files and required variables.

Source paths: `AGENTS.md`, `docker-compose-local.yml`, `docker-compose.yml`, `systemd/user/plane-live.service`, `apps/api/.env.example`.

- [ ] **Step 4: Write `docs/design/07-routing-and-navigation.md`**

Required content:

1. `## Scope model` — workspace, application, ticket.
2. `## Canonical route families` — workspace surfaces (`/services`, `/work-items`, `/intake`, `/cycles`, `/modules`, `/pages`, `/analytics`, `/scheduler`, `/notifications`, `/docs`, `/galileo`, `/settings`) and application surfaces (`/projects/:projectId/issues`, `/cycles`, `/modules`, `/pages`, `/intake`, `/war-rooms`, `/views`, `/archives`).
3. `## Product term mapping` — Tickets ↔ work items, Sprints ↔ cycles, Tracks ↔ modules, Knowledge ↔ pages, Applications ↔ projects.
4. `## App rail` — Work, Procedure, Galileo, Settings; mobile mode menu.
5. `## Legacy redirects` — list the redirects registered in `core.ts`; link to `_generated/routes.md` for the full list.
6. `## Adding a route` — register in `core.ts`, add to the app rail/sidebar when user-facing, update the feature doc, run `pnpm docs:inventory` and `pnpm docs:check`.
7. `## Changelog`.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/components/navigation`, `apps/web/app/routes/redirects`.

- [ ] **Step 5: Write `docs/design/README.md`**

Required content:

- Title `# Design Docs — Terraline`.
- Reading order table with `01`–`07`, status `current`, and dependencies.
- Note that `07-routing-and-navigation.md` owns route families and legacy redirects.
- Source paths: `apps/web/app/routes/core.ts`, `apps/api-rs/migrations`.
- Do not include hand-written docs in `source_paths`; use code or generated inventories instead.
- No references to Terra, Plane-for-ITSM, or six apps.

- [ ] **Step 6: Verify and check**

Run:

```bash
pnpm docs:verify docs/design/04-design-system.md docs/design/05-testing-strategy.md docs/design/06-ops-runbook.md docs/design/07-routing-and-navigation.md docs/design/README.md
pnpm docs:check docs/design/04-design-system.md docs/design/05-testing-strategy.md docs/design/06-ops-runbook.md docs/design/07-routing-and-navigation.md docs/design/README.md
```

Expected: all five verified; check passes.

- [ ] **Step 7: Commit**

```bash
git add docs/design/04-design-system.md docs/design/05-testing-strategy.md docs/design/06-ops-runbook.md docs/design/07-routing-and-navigation.md docs/design/README.md
git commit -m "docs: rewrite design system, testing, ops, and routing docs"
```

---

## Task 8: Feature Documents — Core ITSM Surfaces

Every file in this task starts with the standard frontmatter and ends with `pnpm docs:verify`.

**Files:**

- Rewrite: `docs/features/work-items.md`
- Create: `docs/features/services.md`
- Rewrite: `docs/features/intake.md`
- Create: `docs/features/knowledge.md`
- Create: `docs/features/sprints.md`
- Create: `docs/features/tracks.md`
- Delete: `docs/features/cmdb.md`
- Delete: `docs/features/cycles.md`
- Delete: `docs/features/modules.md`
- Delete: `docs/features/pages.md`

- [ ] **Step 1: Write `docs/features/work-items.md`**

Start with the standard frontmatter. Required sections: Intent, Routes, Current behavior, Layouts, Stores and services, API touchpoints, Permissions, War Room toggle, Service gate, Edge cases, In-app guide, Changelog.

Facts to verify in code:

- Workspace route `/:workspaceSlug/work-items`.
- Application routes `/:workspaceSlug/projects/:projectId/issues` and `/:workspaceSlug/projects/:projectId/issues/:issueId`.
- Canonical detail URL `/:workspaceSlug/browse/:workItem`.
- Ticket types and layouts described in `README.md` §Tickets.
- Stores under `apps/web/core/store/issue`.
- API files `apps/api-rs/crates/api/src/routes/issue_*.rs`.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/issue`, `apps/api-rs/crates/api/src/routes`.

- [ ] **Step 2: Write `docs/features/services.md`**

Required sections: Intent, Routes, Catalog model, Typed dependencies, Graph and list layouts, Health, Metrics ingest, Application links, API touchpoints, Permissions, Limitations, In-app guide, Changelog.

Facts to verify:

- Workspace routes `/:workspaceSlug/services` and `/:workspaceSlug/services/:serviceId`.
- Legacy project service routes redirect to the workspace catalog.
- Services are workspace-level; applications link as owner or consumer.
- Health is derived from catalog data, not live monitoring.
- Metrics sources live in workspace settings.

Source paths: `apps/api-rs/crates/api/src/routes/service.rs`, `service_health.rs`, `service_metrics.rs`, `service_access.rs`, `apps/api-rs/migrations`, `apps/web/core/store/service.store.ts`, `apps/web/core/components/services`, `apps/web/app/routes/core.ts`.

- [ ] **Step 3: Write `docs/features/intake.md`**

Required sections: Intent, Routes, Workspace versus application intake, Sources, Series deduplication, AI triage suggestions, Service gate, API touchpoints, Permissions, Edge cases, In-app guide, Changelog.

Facts to verify:

- Workspace `/intake`; application `/:workspaceSlug/projects/:projectId/intake`.
- Sources and token-authenticated webhook intake.
- Accept/decline/snooze/duplicate triage.
- `requires service` gate for Request types.

Source paths: `apps/api-rs/crates/api/src/routes/intake.rs`, `intake_source.rs`, `inbound.rs`, `apps/api-rs/migrations/0014_intake_service_gate.sql`, `0015_intake_sources.sql`, `apps/web/core/store/inbox`, `apps/web/core/components/inbox`.

- [ ] **Step 4: Write `docs/features/knowledge.md`**

Required sections: Intent, Routes, Page tree, Editor and collaboration, Versions and comments, Space publishing, In-app docs, API touchpoints, Permissions, Limitations, Changelog.

Facts to verify:

- Workspace `/pages`; application `/:workspaceSlug/projects/:projectId/pages`.
- In-app docs at `/:workspaceSlug/docs` and `/:workspaceSlug/docs/:topicSlug`.
- TipTap + Yjs + Hocuspocus via `apps/live`.
- `apps/web/core/store/pages` and `apps/api-rs/crates/api/src/routes/page.rs`.

Source paths: `apps/api-rs/crates/api/src/routes/page.rs`, `apps/live`, `apps/web/core/store/pages`, `apps/web/core/components/docs`, `apps/web/app/routes/core.ts`.

- [ ] **Step 5: Write `docs/features/sprints.md` and `tracks.md`**

For each document, include: Intent, Routes, Workspace aggregate versus application detail, Stores, API touchpoints, Permissions, Edge cases, In-app guide, Changelog.

Facts:

- Sprints use `/cycles`; Tracks use `/modules`.
- Workspace routes: `/:workspaceSlug/cycles`, `/:workspaceSlug/modules`.
- Application routes: `/:workspaceSlug/projects/:projectId/cycles`, `.../cycles/:cycleId`, `.../modules`, `.../modules/:moduleId`.
- API files: `cycle.rs`, `module.rs`.
- Stores: `cycle.store.ts`, `module.store.ts`.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/cycle.store.ts`, `apps/web/core/store/module.store.ts`, `apps/api-rs/crates/api/src/routes/cycle.rs`, `apps/api-rs/crates/api/src/routes/module.rs`.

- [ ] **Step 6: Delete superseded documents**

Run:

```bash
git rm docs/features/cmdb.md docs/features/cycles.md docs/features/modules.md docs/features/pages.md
```

- [ ] **Step 7: Verify and check**

Run:

```bash
pnpm docs:verify docs/features/work-items.md docs/features/services.md docs/features/intake.md docs/features/knowledge.md docs/features/sprints.md docs/features/tracks.md
pnpm docs:check docs/features/work-items.md docs/features/services.md docs/features/intake.md docs/features/knowledge.md docs/features/sprints.md docs/features/tracks.md
```

Expected: all six verified; check passes.

- [ ] **Step 8: Commit**

```bash
git add docs/features
git commit -m "docs: rewrite core ITSM feature documentation"
```

---

## Task 9: Feature Documents — Operations and AI

Every file in this task starts with the standard frontmatter and ends with `pnpm docs:verify`.

**Files:**

- Rewrite: `docs/features/war-rooms.md`
- Create: `docs/features/ai.md`
- Rewrite: `docs/features/analytics.md`
- Create: `docs/features/notifications.md`
- Create: `docs/features/applications.md`
- Rewrite: `docs/features/settings.md`
- Delete: `docs/features/inbox.md`

- [ ] **Step 1: Write `docs/features/war-rooms.md`**

Start with the standard frontmatter. Required sections: Intent, Routes, Lifecycle, Realtime chat, Runbook and service blast radius, Permissions, API touchpoints, Limitations, In-app guide, Changelog.

Facts:

- List route `/:workspaceSlug/projects/:projectId/issues?war_room=on`.
- Detail route `/:workspaceSlug/projects/:projectId/war-rooms/:warRoomId`.
- Legacy list route redirects to the filtered issue list.
- Realtime relay through `apps/live` over Redis.
- Store `war-room.store.ts`; API `war_room.rs`; migration `0011_war_rooms.sql`.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/war-room.store.ts`, `apps/web/core/components/war-rooms`, `apps/api-rs/crates/api/src/routes/war_room.rs`, `apps/api-rs/migrations/0011_war_rooms.sql`, `apps/live`.

- [ ] **Step 2: Write `docs/features/ai.md`**

Required sections: Intent, Galileo surfaces, Assistant versus full page, Scheduler, Intake triage suggestions, Configuration, API touchpoints, Permissions, Limitations, In-app guide, Changelog.

Facts:

- Galileo full page `/:workspaceSlug/galileo` and `/:workspaceSlug/galileo/:conversationId`.
- Right-side assistant on non-Galileo pages.
- Scheduler route `/:workspaceSlug/scheduler`.
- AI routes `ai.rs`, `ai_agent`, `ai_conversations.rs`, `ai_schedule.rs`; crate `apps/api-rs/crates/ai`.
- Stores `ai-assistant.store.ts`, `ai-schedules.store.ts`.
- LLM configuration gates the Galileo rail item.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/ai-assistant.store.ts`, `apps/web/core/store/ai-schedules.store.ts`, `apps/web/core/components/galileo`, `apps/api-rs/crates/api/src/routes`, `apps/api-rs/crates/ai`.

- [ ] **Step 3: Write `docs/features/analytics.md`**

Required sections: Intent, Route, Metrics, Application scope, Stores, API touchpoints, Permissions, Limitations, In-app guide, Changelog.

Facts:

- Route `/:workspaceSlug/analytics/:tabId`.
- `analytics.store.ts`; API `analytic.rs`.
- Workspace-level reports; application filter.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/analytics.store.ts`, `apps/api-rs/crates/api/src/routes/analytic.rs`, `apps/web/core/components/analytics`.

- [ ] **Step 4: Write `docs/features/notifications.md`**

Required sections: Intent, Route, Notification types, Read/snooze/archive behavior, Stores, API touchpoints, Permissions, In-app guide, Changelog.

Facts:

- Route `/:workspaceSlug/notifications`.
- Legacy project inbox redirects to application intake.
- `notification.rs`, `apps/web/core/store/notifications`, `apps/web/core/components/workspace-notifications`.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/notifications`, `apps/api-rs/crates/api/src/routes/notification.rs`, `apps/web/core/components/workspace-notifications`.

- [ ] **Step 5: Write `docs/features/applications.md`**

Required sections: Intent, Applications versus projects, Routes, Application scope selector, Membership and permissions, Archives, Settings, API touchpoints, In-app guide, Changelog.

Facts:

- Application routes `/:workspaceSlug/projects` and `/:workspaceSlug/projects/:projectId/*`.
- Application scope is persisted per workspace in `workspace-application-scope.store.ts`.
- `project.rs`, `project.store.ts`, `apps/web/core/components/projects`.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/store/workspace-application-scope.store.ts`, `apps/web/core/store/project`, `apps/api-rs/crates/api/src/routes/project.rs`.

- [ ] **Step 6: Write `docs/features/settings.md`**

Required sections: Workspace settings, Application settings, Profile settings, God-mode, Feature flags, Metric sources, Document export, Permissions, API touchpoints, In-app guide, Changelog.

Facts:

- Workspace settings paths from `core.ts` under `/:workspaceSlug/settings`.
- Application settings under `/:workspaceSlug/settings/projects/:projectId`.
- Profile settings under `/settings/profile/:profileTabId`.
- God-mode app `apps/admin` at `/god-mode/`.
- `workspace_settings.rs`, `instance_admin.rs`, `apps/web/core/components/settings`.

Source paths: `apps/web/app/routes/core.ts`, `apps/api-rs/crates/api/src/routes/workspace_settings.rs`, `apps/api-rs/crates/api/src/routes/instance_admin.rs`, `apps/web/core/components/settings`, `apps/web/core/components/workspace/sidebar`.

- [ ] **Step 7: Delete the old inbox document**

Run:

```bash
git rm docs/features/inbox.md
```

- [ ] **Step 8: Verify and check**

Run:

```bash
pnpm docs:verify docs/features/war-rooms.md docs/features/ai.md docs/features/analytics.md docs/features/notifications.md docs/features/applications.md docs/features/settings.md
pnpm docs:check docs/features/war-rooms.md docs/features/ai.md docs/features/analytics.md docs/features/notifications.md docs/features/applications.md docs/features/settings.md
```

Expected: all six verified; check passes.

- [ ] **Step 9: Commit**

```bash
git add docs/features
git commit -m "docs: rewrite operations, AI, and settings documentation"
```

---

## Task 10: Feature Documents — Personal Surfaces and Shared Concerns

Every file in this task starts with the standard frontmatter and ends with `pnpm docs:verify`.

**Files:**

- Rewrite: `docs/features/README.md`
- Rewrite: `docs/features/archives.md`
- Rewrite: `docs/features/drafts.md`
- Rewrite: `docs/features/stickies.md`
- Rewrite: `docs/features/views.md`
- Rewrite: `docs/features/browse.md`
- Rewrite: `docs/features/_shared/README.md`
- Rewrite: `docs/features/_shared/list.md`
- Rewrite: `docs/features/_shared/detail-page.md`
- Create: `docs/features/_shared/comments.md`
- Create: `docs/features/_shared/search.md`
- Delete: `docs/features/_shared/routing.md`
- Delete: `docs/features/_backlog.md`

- [ ] **Step 1: Write `docs/features/README.md`**

Start with the standard frontmatter. Include:

- A capability inventory table with columns: Capability, Doc, Primary routes, Status.
- Rows for every feature file that exists after this task, including `notifications.md`, `ai.md`, and `applications.md` from Task 9.
- A note that in-app docs at `/:workspaceSlug/docs` are the user guide; `docs/features` is implementation documentation.
- A note that generated route inventories are authoritative.
- No links to deleted files.

Source paths: `apps/web/app/routes/core.ts`. Do not include `docs/features/README.md` or `docs/features/` in `source_paths`.

- [ ] **Step 2: Write the personal-surface documents**

Use the existing docs as outlines, but verify every route against `core.ts`. Each document needs: Intent, Route, Current behavior, Stores/components, API touchpoints, Permissions, Edge cases, In-app guide, Changelog.

Required facts:

- Archives: application archive routes for issues, cycles, modules; workspace archived applications.
- Drafts: `/:workspaceSlug/drafts`; workspace scope.
- Stickies: `/:workspaceSlug/stickies`; personal scope.
- Views: `/:workspaceSlug/workspace-views`, `/:workspaceSlug/workspace-views/:globalViewId`, application `.../views` and `.../views/:viewId`; Queues are saved views over tickets.
- Browse: `/:workspaceSlug/browse/:workItem`; canonical identifier URL.

Source paths for all five:

- `apps/web/app/routes/core.ts`
- `apps/web/core/store`
- `apps/web/core/components`
- `apps/api-rs/crates/api/src/routes`

- [ ] **Step 3: Write `docs/features/_shared/list.md`**

Cover list layouts, filters, sorting, pagination, saved views, export, and application scope. Reference the actual table/list components, not Terra. Source paths: `apps/web/core/components/issues`, `apps/web/core/components/views`, `apps/web/core/components/cycles`, `apps/web/core/components/modules`.

- [ ] **Step 4: Write `docs/features/_shared/detail-page.md`**

Cover shared detail layout, peek/overlay behavior, activity rail, and responsive behavior. Source paths: `apps/web/core/components/issues`, `apps/web/core/layouts`, `apps/web/core/components/navigation`.

- [ ] **Step 5: Write `docs/features/_shared/comments.md`**

Cover comments, activity timeline, versions, subscriptions, reactions. Source paths: `apps/web/core/components`, `apps/api-rs/crates/api/src/routes/issue_activity_write.rs`, `history.rs`, `versions.rs`, `subscribe.rs`, `reactions.rs`.

- [ ] **Step 6: Write `docs/features/_shared/search.md`**

Cover Cmd/Ctrl+K, command palette groups, search API, and scope. Source paths: `apps/web/core/components`, `apps/web/core/store/base-command-palette.store.ts`, `apps/api-rs/crates/api/src/routes/search.rs`.

- [ ] **Step 7: Update `_shared/README.md`**

List only these files: `list.md`, `detail-page.md`, `comments.md`, `search.md`. Remove references to `routing.md`, `comments.md` as missing, and `global-search.md` if it does not exist.

Source paths: `apps/web/app/routes/core.ts`, `apps/web/core/components`, `apps/api-rs/crates/api/src/routes`. Do not include hand-written docs in `source_paths`; use code or generated inventories instead.

- [ ] **Step 8: Delete superseded shared and backlog files**

Run:

```bash
git rm docs/features/_shared/routing.md docs/features/_backlog.md
```

- [ ] **Step 9: Verify and check**

Run:

```bash
pnpm docs:verify docs/features/README.md docs/features/archives.md docs/features/drafts.md docs/features/stickies.md docs/features/views.md docs/features/browse.md docs/features/_shared/README.md docs/features/_shared/list.md docs/features/_shared/detail-page.md docs/features/_shared/comments.md docs/features/_shared/search.md
pnpm docs:check docs/features/README.md docs/features/archives.md docs/features/drafts.md docs/features/stickies.md docs/features/views.md docs/features/browse.md docs/features/_shared/README.md docs/features/_shared/list.md docs/features/_shared/detail-page.md docs/features/_shared/comments.md docs/features/_shared/search.md
```

Expected: all eleven verified; check passes.

- [ ] **Step 10: Commit**

```bash
git add docs/features
git commit -m "docs: rewrite personal surfaces and shared feature concerns"
```

---

## Task 11: UI Documents and Docs Index

`docs/README.md` is written in this task, after every linked document exists, so its status table resolves completely. Every file in this task starts with the standard frontmatter and ends with `pnpm docs:verify`.

**Files:**

- Rewrite: `docs/README.md`
- Rewrite: `docs/ui/README.md`
- Verify and rewrite as needed: `docs/ui/design-tokens.md`
- Rewrite: `docs/ui/shell.md`
- Verify and rewrite as needed: `docs/ui/editor.md`

- [ ] **Step 1: Write `docs/README.md`**

Start with the standard frontmatter. Required sections, in this order:

1. `# Terraline — Dokumentasi`
2. `## Untuk siapa dokumen ini` — engineers, contributors, AI agents; product user flows live in the in-app docs.
3. `## Sumber kebenaran` — code is authoritative; link to `_generated/routes.md`, `_generated/api-routes.md`, `_generated/schema.md`.
4. `## Status dokumen` — table with every current document and one of `current`, `generated`, `historical`.
5. `## Reading paths` — engineer/AI onboarding, feature implementation, product context, ops.
6. `## Struktur` — the real tree from the spec, including `_generated/` and `superpowers/`.
7. `## Aturan update` — run `pnpm docs:verify <doc>` after source changes; run `pnpm docs:check`; do not edit generated files.
8. `## Changelog` — add `2026-10-27 | reset corpus; current sources and generated inventories`.

Do not list `business-capabilities.md`, `_backlog.md`, `cmdb.md`, `cycles.md`, `modules.md`, `pages.md`, or `_shared/routing.md`.

Source paths: `README.md`, `apps/web/app/routes/core.ts`, `apps/api-rs/migrations`.

- [ ] **Step 2: Write `docs/ui/README.md`**

Start with the standard frontmatter. Include a table of the three UI docs with status and the source paths they cover. Link to `docs/design/04-design-system.md`.

Source paths: `packages/ui/src`, `packages/editor`, `packages/tailwind-config`. Do not include hand-written docs in `source_paths`; use package sources instead.

- [ ] **Step 3: Write `docs/ui/shell.md`**

Start with the standard frontmatter. Cover the app rail (Work, Procedure, Galileo, Settings), workspace sidebar, mobile mode menu, responsive behavior, and active states. Source paths: `apps/web/core/components/navigation`, `apps/web/core/components/workspace/sidebar`, `apps/web/core/components/sidebar`.

- [ ] **Step 4: Verify `design-tokens.md` and `editor.md`**

Read the current files and compare with:

- `packages/tailwind-config`
- `packages/ui/src`
- `packages/editor`

Add frontmatter to both files before verifying. Use `packages/tailwind-config` and `packages/ui/src` for `design-tokens.md`, and `packages/editor` for `editor.md`. Update only claims that are wrong; keep the existing structure if it is accurate.

- [ ] **Step 5: Verify and check**

Run:

```bash
pnpm docs:verify docs/README.md docs/ui/README.md docs/ui/design-tokens.md docs/ui/shell.md docs/ui/editor.md
pnpm docs:check docs/README.md docs/ui/README.md docs/ui/design-tokens.md docs/ui/shell.md docs/ui/editor.md
```

Expected: all five verified; check passes.

- [ ] **Step 6: Commit**

```bash
git add docs/README.md docs/ui
git commit -m "docs: add corpus index and update UI documentation"
```

---

## Task 12: CI Guardrail and Final Validation

**Files:**

- Create: `.github/workflows/docs-check.yml`
- Modify: `AGENTS.md`
- Verify: all `docs/` files

- [ ] **Step 1: Add the docs workflow**

Create `.github/workflows/docs-check.yml`:

```yaml
name: Docs check

on:
  workflow_dispatch:
  pull_request:
    paths:
      - "docs/**"
      - "scripts/docs-*"
      - "apps/web/app/routes/core.ts"
      - "apps/api-rs/crates/api/src/main.rs"
      - "apps/api-rs/migrations/**"
      - "package.json"
      - "AGENTS.md"
      - ".github/workflows/docs-check.yml"
  push:
    branches:
      - main
    paths:
      - "docs/**"
      - "scripts/docs-*"
      - "apps/web/app/routes/core.ts"
      - "apps/api-rs/crates/api/src/main.rs"
      - "apps/api-rs/migrations/**"
      - "package.json"
      - "AGENTS.md"
      - ".github/workflows/docs-check.yml"

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  docs:
    name: docs:check
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - name: Checkout code
        uses: actions/checkout@v6

      - name: Set up Node.js
        uses: actions/setup-node@v6
        with:
          node-version-file: .node-version

      - name: Enable Corepack and pnpm
        run: corepack enable pnpm

      - name: Run docs tests
        run: pnpm docs:test

      - name: Check docs
        run: pnpm docs:check
```

- [ ] **Step 2: Add the AGENTS rule**

Under the `## Commands` section in `AGENTS.md`, add:

```markdown
- `pnpm docs:check` - Validate docs frontmatter, links, source hashes, and generated inventories
- `pnpm docs:test` - Run docs parser tests
- `pnpm docs:verify <doc.md>` - Recompute `source_hash` after changing a documented source
```

Under `## Code Style`, add:

```markdown
- **Docs drift**: After changing routes, API routes, migrations, or documented behavior, run `pnpm docs:verify <doc>` for the affected documents and `pnpm docs:check`. Never edit `docs/_generated/*` by hand; run `pnpm docs:inventory`.
```

- [ ] **Step 3: Re-verify documents affected by the workflow and AGENTS edit**

The new workflow changes `.github/workflows`, and the AGENTS edit changes `AGENTS.md`. Both are source paths for design docs. Run:

```bash
pnpm docs:verify docs/design/05-testing-strategy.md docs/design/06-ops-runbook.md
```

Expected: both verified.

- [ ] **Step 4: Run the full validation**

Run:

```bash
pnpm docs:test
pnpm docs:inventory
git diff --exit-code docs/_generated
pnpm docs:check
pnpm exec oxfmt --check docs scripts
git diff --check
```

Expected: tests pass; inventory has no diff; docs check passes; formatting and whitespace checks pass.

- [ ] **Step 5: Check for stale file names and claims**

Run:

```bash
test ! -e docs/features/cmdb.md
test ! -e docs/features/cycles.md
test ! -e docs/features/modules.md
test ! -e docs/features/pages.md
test ! -e docs/features/inbox.md
test ! -e docs/features/_backlog.md
test ! -e docs/features/_shared/routing.md
! rg -n "Plane for ITSM|ITSM is future|ITSM remains future|6 apps|six apps" docs --glob '!docs/superpowers/**'
```

Expected: all `test` commands return 0; `rg` finds no banned claims.

- [ ] **Step 6: Verify the expected file set**

Run:

```bash
find docs -maxdepth 3 -type f | sort
```

Expected: the tree from the spec, including `docs/_generated/{routes,api-routes,schema}.md` and `docs/superpowers/README.md`.

- [ ] **Step 7: Commit**

```bash
git add .github/workflows/docs-check.yml AGENTS.md docs/design/05-testing-strategy.md docs/design/06-ops-runbook.md
git commit -m "docs: add CI drift check and contributor guidance"
```

- [ ] **Step 8: Optional follow-up commit for formatting**

If `oxfmt` changed files, commit them separately:

```bash
git add docs scripts AGENTS.md
git commit -m "docs: format current corpus"
```

---

## Final Verification Checklist

- [ ] `pnpm docs:test` passes.
- [ ] `pnpm docs:inventory` produces no diff.
- [ ] `pnpm docs:check` passes.
- [ ] `pnpm exec oxfmt --check docs scripts` passes.
- [ ] `git diff --check` passes.
- [ ] No current document contains `Plane for ITSM`, `ITSM is future`, `ITSM remains future`, `6 apps`, or `six apps`.
- [ ] Every document in `docs/features` has frontmatter and a valid source hash.
- [ ] `docs/_generated/routes.md` contains workspace and application route families.
- [ ] `docs/_generated/api-routes.md` contains service and war-room routes.
- [ ] `docs/_generated/schema.md` contains `services`, `war_rooms`, `workspace_services`, and `service_metrics`.
- [ ] The docs workflow exists and runs `pnpm docs:test` and `pnpm docs:check`.
- [ ] Manually spot-check five documents: every route and symbol reference exists in the named source file.
