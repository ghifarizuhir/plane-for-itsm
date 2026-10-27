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
