/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// One-off S1 rename sweep: Project -> Application in EN locale VALUES only.
// Keys are never touched. Interpolation tokens ({project}, {count}) are
// preserved; ICU plural literals are renamed in place.
//
// Usage:
//   tsx packages/i18n/scripts/rename-project-copy.ts           # dry run
//   tsx packages/i18n/scripts/rename-project-copy.ts --apply   # write files

import fs from "node:fs";
import path from "node:path";
// local imports
import { LOCALES_DIR, readJsonFile } from "./lib/locale-io.js";

const ALLOWLIST = new Set<string>(["common.json:user_roles.product_or_project_manager"]);
const INTERPOLATION = /\{[a-z][a-zA-Z0-9_.]*\}/g;
const REPLACEMENTS: [RegExp, string][] = [
  [/\bProjects\b/g, "Applications"],
  [/\bProject\b/g, "Application"],
  [/\bprojects\b/g, "applications"],
  [/\bproject\b/g, "application"],
];

function renameValue(value: string): string {
  const tokens: string[] = [];
  const masked = value.replace(INTERPOLATION, (match) => {
    tokens.push(match);
    return `__S1_TOKEN_${tokens.length - 1}__`;
  });
  let renamed = masked;
  for (const [pattern, replacement] of REPLACEMENTS) renamed = renamed.replace(pattern, replacement);
  return renamed.replace(/__S1_TOKEN_(\d+)__/g, (_, index: string) => tokens[Number(index)]);
}

const apply = process.argv.includes("--apply");
const dir = path.join(LOCALES_DIR, "en");
let changedValues = 0;

for (const file of fs.readdirSync(dir).toSorted()) {
  if (!file.endsWith(".json")) continue;
  const filePath = path.join(dir, file);
  const json = readJsonFile(filePath);
  let fileChanged = false;

  const walk = (obj: Record<string, unknown>, prefix: string) => {
    for (const [key, value] of Object.entries(obj)) {
      const keyPath = prefix ? `${prefix}.${key}` : key;
      if (typeof value === "string") {
        if (ALLOWLIST.has(`${file}:${keyPath}`)) continue;
        const renamed = renameValue(value);
        if (renamed !== value) {
          changedValues += 1;
          fileChanged = true;
          if (apply) obj[key] = renamed;
          else console.log(`${file}:${keyPath}\n  - ${value}\n  + ${renamed}`);
        }
      } else if (value && typeof value === "object") {
        walk(value as Record<string, unknown>, keyPath);
      }
    }
  };

  walk(json, "");
  if (apply && fileChanged) fs.writeFileSync(filePath, `${JSON.stringify(json, null, 2)}\n`);
}

console.log(`${apply ? "Applied" : "Would change"} ${changedValues} value(s) across en/*.json`);
