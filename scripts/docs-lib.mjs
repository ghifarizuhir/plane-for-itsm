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
  for (const entry of fs
    .readdirSync(absDir, { withFileTypes: true })
    .toSorted((a, b) => a.name.localeCompare(b.name))) {
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

  return [...out].toSorted();
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
    for (const entry of fs
      .readdirSync(absDir, { withFileTypes: true })
      .toSorted((a, b) => a.name.localeCompare(b.name))) {
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
      /\b(CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?|ALTER\s+TABLE(?:\s+ONLY)?|DROP\s+TABLE(?:\s+IF\s+EXISTS)?)\s+(?:"?[A-Za-z0-9_]+"?\.)?"?([A-Za-z0-9_]+)"?/gi;
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

  return [...tables.values()].toSorted((a, b) => a.table.localeCompare(b.table));
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
    const items = routes.filter((route) => routeGroup(route) === key).toSorted((a, b) => a.path.localeCompare(b.path));
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
    const items = routes.filter((route) => apiGroup(route) === key).toSorted((a, b) => a.path.localeCompare(b.path));
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
    .toSorted()
    .map((name) => ({ name, sql: fs.readFileSync(path.join(migrationsDir, name), "utf8") }));
  const tables = parseSchemaMigrations(migrations);
  if (tables.length < 20) throw new Error(`expected at least 20 schema tables, got ${tables.length}`);

  return {
    "docs/_generated/routes.md": renderRoutesMarkdown(webRoutes),
    "docs/_generated/api-routes.md": renderApiRoutesMarkdown(apiRoutes),
    "docs/_generated/schema.md": renderSchemaMarkdown(tables),
  };
}
