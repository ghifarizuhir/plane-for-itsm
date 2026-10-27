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
