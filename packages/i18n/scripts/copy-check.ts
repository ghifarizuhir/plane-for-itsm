/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// Usage:
//   tsx packages/i18n/scripts/copy-check.ts          # Report only
//   tsx packages/i18n/scripts/copy-check.ts --ci     # Exit 1 if violations found
//
// Fails when an en-locale VALUE still says project/projects (S1 rename guard).
// Interpolation tokens like {project}, {count} are ignored; ICU plural literals
// like {count, plural, one {Project} other {Projects}} are checked (and must be
// renamed in place).

import fs from "node:fs";
import path from "node:path";
// local imports
import { LOCALES_DIR, readJsonFile } from "./lib/locale-io.js";

// Key paths whose values legitimately contain "project" (real-world meanings).
const ALLOWLIST = new Set<string>(["common.json:user_roles.product_or_project_manager"]);

// Lowercase-start interpolation tokens only — ICU plural bodies stay visible.
const INTERPOLATION = /\{[a-z][a-zA-Z0-9_.]*\}/g;
const PROJECT_WORD = /\bprojects?\b/i;

function findViolations(): string[] {
  const dir = path.join(LOCALES_DIR, "en");
  const violations: string[] = [];

  for (const file of fs.readdirSync(dir).toSorted()) {
    if (!file.endsWith(".json")) continue;
    const json = readJsonFile(path.join(dir, file));

    const walk = (obj: Record<string, unknown>, prefix: string) => {
      for (const [key, value] of Object.entries(obj)) {
        const keyPath = prefix ? `${prefix}.${key}` : key;
        if (typeof value === "string") {
          if (ALLOWLIST.has(`${file}:${keyPath}`)) continue;
          if (PROJECT_WORD.test(value.replace(INTERPOLATION, "{token}"))) {
            violations.push(`${file}:${keyPath} :: ${value}`);
          }
        } else if (value && typeof value === "object") {
          walk(value as Record<string, unknown>, keyPath);
        }
      }
    };

    walk(json, "");
  }

  return violations;
}

const violations = findViolations();

if (violations.length === 0) {
  console.log("copy-check: clean — no project copy in en locale values");
  process.exit(0);
}

console.log(`copy-check: ${violations.length} en value(s) still say project:`);
for (const violation of violations) console.log(`  ${violation}`);

if (process.argv.includes("--ci")) process.exit(1);
