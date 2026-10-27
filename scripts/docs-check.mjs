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
