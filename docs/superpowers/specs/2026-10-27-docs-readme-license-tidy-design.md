# Docs Tidy: README, License, Docs Index, Contributing

**Date:** 2026-10-27
**Status:** Approved design
**Scope:** `README.md`, `NOTICE` (new), `package.json` metadata, `docs/README.md`, `CONTRIBUTING.md`

## Goal

Make the public documentation of this fork internally consistent and legally
explicit without rewriting the whole docs corpus:

- Position the product as **Terraline**, a fork of Plane for IT service
  management.
- Record fork attribution and modification notices without altering the AGPL
  license text.
- Replace contradictory documentation claims (ITSM described as backlog) with
  an honest status index.
- Make `CONTRIBUTING.md` describe this fork and its current stack instead of
  upstream Plane.

## Non-goals

- Rewriting `PRODUCT.md`, `business-capabilities.md`, `CHEAT-SHEET.md`, or the
  documents under `docs/design/` and `docs/features/`.
- Changing `LICENSE.txt` or `COPYRIGHT.txt`.
- Rebranding UI strings, package names, or source-code copyright headers.
- Updating `SECURITY.md`, issue templates, or CI workflows.

## Decisions

1. **Brand:** public docs use **Terraline**. The fork relationship and upstream
   attribution remain explicit.
2. **License:** keep AGPL-3.0-only. `LICENSE.txt` stays byte-for-byte upstream
   AGPL text. A new root `NOTICE` file carries the fork notice, copyright for
   modifications, and a summary of changes.
3. **Copyright for modifications:** `Copyright (c) 2026-present Terraline
contributors`.
4. **Language:** README and CONTRIBUTING stay in English; `docs/README.md`
   stays in Indonesian.
5. **Docs status:** the root README is the source of truth for current product
   positioning. Sub-documents are labelled as snapshots that may lag.
6. **Root package metadata:** point repository/homepage/bugs at the fork;
   standardize the license string to `AGPL-3.0-only`. Do not rename the root
   package.
7. **Per-file modification notices:** document the policy instead of mass-editing
   source headers. New files authored for Terraline carry a Terraline copyright
   header; substantially modified upstream files keep the upstream header and
   add a Terraline copyright line.
8. **Network-use notice:** README and NOTICE state that operators running a
   modified version as a network service must offer the Corresponding Source,
   per AGPL section 13.

## File changes

### `README.md`

- Replace "Plane for ITSM" with "Terraline" in the title, headings, and body.
- Keep the opening explanation that Terraline is a fork of Plane.
- Keep the ITIL4 map, capabilities, architecture, self-hosting, local
  development, repository layout, documentation, contributing, and security
  sections.
- Fix stale claims found during implementation (versions, commands, paths,
  links) only where the current repository disproves them.
- Expand the License section: AGPL-3.0-only, links to `LICENSE.txt` and
  `NOTICE`, upstream copyright retained, and the AGPL section 13 network-use
  obligation for modified versions.

### `NOTICE` (new)

Include:

- Product name, copyright for modifications, SPDX identifier.
- Statement that Terraline is a modified fork of Plane, with upstream
  copyright and repository URL.
- The modification period (`2026-present`).
- A short, factual summary of modifications (ITSM product surface, Rust API
  migration, UI and documentation changes).
- Pointer to `LICENSE.txt`, the no-warranty statement, and the AGPL section 13
  network-use obligation.

### `package.json`

- `repository`: `https://github.com/ghifarizuhir/plane-for-itsm.git`
- `homepage`: `https://github.com/ghifarizuhir/plane-for-itsm#readme`
- `bugs`: `https://github.com/ghifarizuhir/plane-for-itsm/issues`
- `license`: `AGPL-3.0-only`

No dependency or script changes.

### `docs/README.md`

- Title becomes "Terraline — Dokumentasi".
- Intro states that Terraline is a Plane fork and that ITSM is implemented,
  not backlog.
- Add a status table that labels each area as current, snapshot, or needs
  refresh, and names the root README as product source of truth.
- Update the directory tree to include `linting.md` and `superpowers/`
  (`specs`, `plans`, `decisions`, `smoke`), and reflect the real app/package
  counts.
- Adjust the reading-order and update-policy tables accordingly.
- Append a changelog row for 2026-10-27.

### `CONTRIBUTING.md`

- Rebrand to Terraline and state the fork relationship.
- Point issue, clone, and feature-request URLs to
  `github.com/ghifarizuhir/plane-for-itsm`.
- Sync prerequisites with the current stack: Docker with Compose, Node.js
  > = 22.22, pnpm 11.10 via Corepack, 12 GB RAM; data services run in Docker.
- Sync setup commands with README/AGENTS: `./setup.sh`,
  `docker compose -f docker-compose-local.yml up -d --build api worker beat-worker`,
  `pnpm dev`; list web/admin/API/live URLs.
- Keep coding guidelines, translation guidance, and the quality checklist.
- Replace upstream-cloud/docs/forum references with repository docs,
  `docs/README.md`, and `AGENTS.md`.
- Add a License section: contributions are AGPL-3.0-only, contributors retain
  copyright over their contributions, and the per-file header policy is
  documented (new Terraline files vs. modified upstream files).

## Acceptance criteria

- No public-facing occurrence of "Plane for ITSM" remains in the five changed
  files; every remaining "Plane" reference is an upstream attribution.
- README, docs index, and CONTRIBUTING agree on the product name, repository
  URL, stack, and setup commands.
- `NOTICE` exists and `LICENSE.txt` is unchanged.
- README and NOTICE mention the AGPL section 13 source-offer obligation for
  network use.
- CONTRIBUTING documents the per-file copyright-header policy for new and
  modified files, without mass-editing existing headers.
- All relative links in `README.md`, `NOTICE`, `docs/README.md`, and
  `CONTRIBUTING.md` resolve.
- `package.json` remains valid JSON and its scripts/dependencies are unchanged.

## Verification

- `node -e "JSON.parse(require('fs').readFileSync('package.json','utf8'))"`
- Script to check relative Markdown links resolve for the changed files.
- `git diff --check`
- `git diff --stat` to confirm only intended files changed.
