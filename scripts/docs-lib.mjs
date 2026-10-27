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
  for (const entry of fs.readdirSync(absDir, { withFileTypes: true }).toSorted((a, b) => a.name.localeCompare(b.name))) {
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
    for (const entry of fs.readdirSync(absDir, { withFileTypes: true }).toSorted((a, b) => a.name.localeCompare(b.name))) {
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
