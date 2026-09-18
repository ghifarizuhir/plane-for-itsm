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
