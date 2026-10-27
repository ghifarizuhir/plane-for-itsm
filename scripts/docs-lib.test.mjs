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
