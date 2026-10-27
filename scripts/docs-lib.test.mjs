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
