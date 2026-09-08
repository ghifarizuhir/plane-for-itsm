#!/usr/bin/env python3
"""Audit parity-inventory.json: report what needs fixing or adding.

Reads:
  apps/api-rs/crates/api/parity-inventory.json   (source of truth)
  apps/api/plane/app/urls/*.py                  (Django contract, coverage only)

Prints:
  [1] status per domain + every non-`implemented` entry  -> fix candidates
  [2] entries with empty `fe_evidence`                   -> evidence-mining candidates
  [3] per-file Django-vs-inventory coverage               -> expansion candidates
      (use --coverage to list every missing path)

Usage:
  parity-audit.py [--domain NAME] [--coverage]

Exit code: 0 = no action items in scope; 1 = fixes/additions outstanding.
Stdlib only.
"""

import argparse
import json
import re
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
INVENTORY_PATH = SCRIPT_DIR.parent / "crates" / "api" / "parity-inventory.json"
URLS_DIR = SCRIPT_DIR.parent.parent.parent / "apps" / "api" / "plane" / "app" / "urls"

# Canonical Django form -> inventory form, for routes Axum cannot express.
# (Batch F T10: `:a-:b` in one segment; the handler enforces the constraint.)
ALIASES = {
    "/api/workspaces/:slug/work-items/:project_identifier-:issue_identifier/":
        "/api/workspaces/:slug/work-items/:ident/",
}

ACTION_STATUSES = ("missing", "shape_mismatch", "constraint_mismatch")


def canon(django_path):
    """Django `workspaces/<str:slug>/...` -> canonical `/api/workspaces/:slug/...`."""
    return "/api/" + re.sub(r"<(?:str|uuid|int):(\w+)>", r":\1", django_path)


def load_inventory():
    try:
        return json.loads(INVENTORY_PATH.read_text())
    except FileNotFoundError:
        sys.exit("error: inventory not found: %s" % INVENTORY_PATH)
    except json.JSONDecodeError as exc:
        sys.exit("error: invalid JSON in %s: %s" % (INVENTORY_PATH, exc))


def django_index():
    """{file_stem: [(line_no, canonical_path)]} for every urls/*.py."""
    if not URLS_DIR.is_dir():
        sys.exit("error: Django urls dir not found: %s" % URLS_DIR)
    out = {}
    pat = re.compile(r"path\(\s*(?:[rRuU]?\"([^\"]+)\"|[rRuU]?'([^']+)')")
    for path in sorted(URLS_DIR.glob("*.py")):
        if path.name == "__init__.py":
            continue
        src = path.read_text()
        out[path.stem] = [
            (src[:m.start()].count("\n") + 1, canon(m.group(1) or m.group(2)))
            for m in pat.finditer(src)
        ]
    return out


def all_endpoints(inv):
    for domain, data in inv.get("domains", {}).items():
        for ep in data.get("endpoints", []):
            yield domain, ep


def report_status(inv, domains):
    lines = ["== [1] status per domain (fix candidates) =="]
    actions = 0
    for domain in domains:
        eps = inv["domains"][domain]["endpoints"]
        counts = {}
        for ep in eps:
            counts[ep["rust_status"]] = counts.get(ep["rust_status"], 0) + 1
        mix = ", ".join("%s=%d" % kv for kv in sorted(counts.items()))
        lines.append("%s: %d entries (%s)" % (domain, len(eps), mix))
    lines.append("")
    for domain in domains:
        bad = [ep for ep in inv["domains"][domain]["endpoints"]
               if ep["rust_status"] in ACTION_STATUSES and not ep.get("out_scope")]
        for ep in bad:
            actions += 1
            lines.append("[%s] %s" % (ep["rust_status"], ep["path"]))
            lines.append("  methods=%s handler=%s src=%s" %
                         (",".join(ep["methods"]), ep.get("rust_handler", "-"),
                          ep.get("django_source", "-")))
            if ep.get("notes"):
                lines.append("  notes: %s" % ep["notes"])
    if actions == 0:
        lines.append("no fix candidates: everything in scope is implemented.")
    return lines, actions


def report_evidence(inv, domains):
    lines = ["", "== [2] empty fe_evidence (evidence-mining candidates) =="]
    actions = 0
    for domain in domains:
        bare = [ep for ep in inv["domains"][domain]["endpoints"]
                if not ep.get("fe_evidence") and not ep.get("out_scope")]
        for ep in bare:
            actions += 1
            lines.append("%s: %s  [%s]" % (domain, ep["path"], ",".join(ep["methods"])))
    if actions == 0:
        lines.append("every in-scope entry has FE evidence.")
    return lines, actions


def report_coverage(inv, index, only_stems, verbose):
    lines = ["", "== [3] Django-vs-inventory coverage (expansion candidates) =="]
    have = set(ep["path"] for _, ep in all_endpoints(inv))
    total_missing = 0
    for stem in sorted(index):
        if only_stems is not None and stem not in only_stems:
            continue
        paths = index[stem]
        missing = [p for _, p in paths if ALIASES.get(p, p) not in have]
        total_missing += len(missing)
        lines.append("%s.py: %d/%d inventoried" % (stem, len(paths) - len(missing), len(paths)))
        if verbose:
            for path in missing:
                lines.append("  MISSING %s" % path)
    if total_missing == 0:
        lines.append("full coverage in scope.")
    return lines, total_missing


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Audit parity-inventory.json: list fixes and additions outstanding.")
    parser.add_argument("--domain", help="restrict to one inventory domain (e.g. issue)")
    parser.add_argument("--coverage", action="store_true",
                        help="list every missing Django path, not just counts")
    args = parser.parse_args(argv)

    inv = load_inventory()
    domains = sorted(inv.get("domains", {}))
    if args.domain:
        if args.domain not in domains:
            sys.exit("error: unknown domain %r (have: %s)" % (args.domain, ", ".join(domains)))
        domains = [args.domain]

    out, actions = [], 0
    section, count = report_status(inv, domains)
    out += section
    actions += count
    section, count = report_evidence(inv, domains)
    out += section
    actions += count
    section, count = report_coverage(inv, django_index(),
                                     set(domains) if args.domain else None,
                                     args.coverage)
    out += section
    actions += count

    print("\n".join(out))
    print("\naction items in scope: %d" % actions)
    return 1 if actions else 0


if __name__ == "__main__":
    raise SystemExit(main())
