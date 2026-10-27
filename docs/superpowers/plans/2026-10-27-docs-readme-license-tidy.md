# Docs Tidy: README, License, Docs Index, Contributing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make README, license notices, docs index, and CONTRIBUTING consistent with the Terraline fork of Plane.

**Architecture:** Documentation-only changes. Brand public docs as Terraline, keep AGPL text untouched, add a root NOTICE for fork attribution, and rewrite the docs index and contributing guide against the current stack. No application code, dependencies, or scripts change.

**Tech Stack:** Markdown, JSON, AGPL-3.0-only.

**Spec:** `docs/superpowers/specs/2026-10-27-docs-readme-license-tidy-design.md`

---

## Commit policy

Do not commit unless the user explicitly asks. The checkpoints below stage nothing
and only verify the working tree.

---

### Task 1: Rebrand README and expand the license section

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Replace the brand in the three product-name locations**

Replace:

```markdown
# Plane for ITSM
```

with:

```markdown
# Terraline
```

Replace both remaining `Plane for ITSM` occurrences in `## Why Plane for ITSM`
and the architecture intro with `Terraline`:

```markdown
## Why Terraline
```

```markdown
Terraline is a pnpm + Turbo monorepo with a Rust API at its center.
```

Update the first sentence of `## Why Terraline` to:

```markdown
Terraline is a fork of [Plane](https://plane.so) that moves the product from project delivery into **IT service management**. The workspace is the tenant. Applications group the work. Services form a shared catalog. Tickets carry incidents, requests, problems, changes, and improvements through ITIL4 practices.
```

- [ ] **Step 2: Replace the License section**

Replace the entire existing section (from `## License` to end of file) with:

```markdown
## License

Terraline is licensed under the [GNU Affero General Public License v3.0 only](LICENSE.txt) (`AGPL-3.0-only`). It is a modified fork of [Plane](https://github.com/makeplane/plane), which is also licensed under AGPL-3.0.

Upstream copyright notices and license terms remain in effect. Fork attribution, the copyright for modifications, and a summary of changes are recorded in [`NOTICE`](NOTICE).

If you run a modified version of Terraline as a network service, AGPL section 13 requires you to offer users of that service the Corresponding Source of the version you run.
```

- [ ] **Step 3: Verify no stale brand text or broken relative links remain**

Run:

```bash
rg -n "Plane for ITSM" README.md
```

Expected: no output.

Run:

```bash
python3 - <<'PY'
import pathlib, re
p = pathlib.Path("README.md")
for i, line in enumerate(p.read_text().splitlines(), 1):
    for match in re.finditer(r"\]\(([^)#]+)(?:#[^)]*)?\)", line):
        target = match.group(1)
        if target.startswith(("http", "mailto:", "#")):
            continue
        path = pathlib.Path(target.split("?")[0])
        print(("OK " if path.exists() else "MISSING "), i, target)
PY
```

Expected: every line starts with `OK`.

---

### Task 2: Add the fork NOTICE

**Files:**

- Create: `NOTICE`

- [ ] **Step 1: Create `NOTICE` with the following content**

```text
Terraline
Copyright (c) 2026-present Terraline contributors
SPDX-License-Identifier: AGPL-3.0-only

Terraline is a modified fork of Plane:
  https://github.com/makeplane/plane
  Copyright (c) 2023-present Plane Software, Inc. and contributors
  Licensed under the GNU Affero General Public License v3.0 only.

Modifications have been made from 2026 to the present. They include the ITSM
product surface (incidents, problems, changes, service catalog, intake, war
rooms), the Rust API migration, UI changes, and documentation updates.

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU Affero General Public License as published by the Free
Software Foundation, version 3 of the License.

This program is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

The full license text is in LICENSE.txt. Upstream copyright notices and license
terms remain in effect.

If you run a modified version of Terraline as a network service, AGPL section 13
requires you to offer users of that service the Corresponding Source of the
version you run.
```

- [ ] **Step 2: Verify the license text was not modified**

Run:

```bash
git diff --exit-code -- LICENSE.txt COPYRIGHT.txt
```

Expected: no output, exit code 0.

---

### Task 3: Point package metadata at the fork

**Files:**

- Modify: `package.json` (top-level metadata only)

- [ ] **Step 1: Update the metadata fields**

Replace:

```json
  "license": "AGPL-3.0",
  "repository": "https://github.com/makeplane/plane.git",
```

with:

```json
  "license": "AGPL-3.0-only",
  "repository": "https://github.com/ghifarizuhir/plane-for-itsm.git",
  "homepage": "https://github.com/ghifarizuhir/plane-for-itsm#readme",
  "bugs": "https://github.com/ghifarizuhir/plane-for-itsm/issues",
```

Do not change `name`, `version`, scripts, or dependencies.

- [ ] **Step 2: Verify JSON validity and unchanged dependency sections**

Run:

```bash
node -e "const p=require('./package.json'); console.log(p.license); console.log(p.repository);"
```

Expected:

```text
AGPL-3.0-only
https://github.com/ghifarizuhir/plane-for-itsm.git
```

Run:

```bash
git diff -- package.json
```

Expected: only `license`, `repository`, `homepage`, and `bugs` changed.

---

### Task 4: Rewrite the docs index honestly

**Files:**

- Modify: `docs/README.md`

- [ ] **Step 1: Replace the title and intro (lines 1–13)**

Replace the section through the `---` after the intro with:

```markdown
# Terraline — Dokumentasi

Terraline adalah **fork Plane** untuk **IT service management (ITSM)**. Workspace menjadi tenant, application mengelompokkan pekerjaan, service membentuk katalog bersama, dan ticket membawa incident, request, problem, change, serta improvement melalui praktik ITIL4.

Dokumen di folder ini menjelaskan implementasi yang ada di repo, bukan rencana jangka panjang. **[`../README.md`](../README.md) adalah sumber kebenaran untuk positioning produk dan fitur yang sudah berjalan.** Beberapa dokumen di `design/` dan `features/` masih berupa snapshot dari fase cutover Rust (September 2026) dan belum diperbarui untuk seluruh permukaan ITSM; statusnya ditandai di bawah.

## Status dokumen

| Dokumen                    | Status      | Catatan                                                          |
| -------------------------- | ----------- | ---------------------------------------------------------------- |
| `../README.md`             | ✅ Current  | Positioning produk, capability, arsitektur, self-hosting         |
| `PRODUCT.md`               | ⚠️ Snapshot | Masih menyebut ITSM sebagai future/backlog; perlu refresh        |
| `business-capabilities.md` | ⚠️ Snapshot | Potret kode saat cutover; perlu verifikasi ulang                 |
| `CHEAT-SHEET.md`           | ⚠️ Snapshot | Ringkasan keputusan; cek versi stack                             |
| `design/`                  | ⚠️ Snapshot | Arsitektur, data model, API, design system, testing, ops         |
| `features/`                | ⚠️ Sebagian | `war-rooms.md` dan `cmdb.md` sudah ada; fitur ITSM lain menyusul |
| `ui/`                      | ✅ Spec     | Token, shell, editor                                             |
| `superpowers/`             | 📚 Arsip    | Spec, plan, decision, dan smoke historis                         |

---
```

- [ ] **Step 2: Replace the structure block (lines 18–46)**

Replace the fenced tree with:

```text
docs/
├── README.md                   ← ini
├── PRODUCT.md                  ← platform, users, positioning, constraints
├── business-capabilities.md    ← snapshot kode (apps/, packages/)
├── CHEAT-SHEET.md              ← rangkuman keputusan (stack, layout, auth, URL, testing)
├── linting.md                  ← aturan lint/format
│
├── design/                     ← engineering cross-cutting (kontrak hidup)
│   ├── README.md               ← reading order & status
│   ├── 01-architecture.md      # monorepo pnpm+turbo, 7 apps, 15 packages
│   ├── 02-data-model.md        # Postgres per-table + ITSM extensions
│   ├── 03-api-contract.md      # Rust Axum + X-Api-Key/Bearer
│   ├── 04-design-system.md     # @plane/ui + tailwind-config + editor
│   ├── 05-testing-strategy.md  # cargo test + pytest baseline + vitest
│   └── 06-ops-runbook.md       # docker-compose + deployments + env
│
├── features/                   ← product per halaman + shared
│   ├── README.md               ← inventory + template
│   ├── _backlog.md             ← ide yang diparkir
│   ├── _shared/                ← concern cross-cutting
│   └── <page>.md               ← 1 file per halaman
│
├── ui/                         ← spesifikasi komponen
│   ├── README.md
│   ├── design-tokens.md
│   ├── shell.md
│   └── editor.md
│
└── superpowers/                ← arsip desain & implementasi
    ├── decisions/
    ├── plans/
    ├── smoke/
    └── specs/
```

- [ ] **Step 3: Replace the reading-order table**

Replace the table under `## Kapan baca yang mana` with:

```markdown
| Kamu adalah…                    | Mulai dari                                                                   |
| ------------------------------- | ---------------------------------------------------------------------------- |
| **New dev / onboarding**        | [`../README.md`](../README.md) → status dokumen di atas → `design/README.md` |
| **Sedang implementasi halaman** | `features/<page>.md` → `design/03-api-contract.md` untuk endpoint            |
| **Architect / reviewer**        | `design/*` penuh; cross-check `business-capabilities.md`                     |
| **Designer / UX**               | `design/04-design-system.md` + `ui/*` + `features/<page>.md`                 |
| **DevOps / infra**              | `design/06-ops-runbook.md` + [`../AGENTS.md`](../AGENTS.md)                  |
| **QA / test**                   | `design/05-testing-strategy.md` + `AGENTS.md` §Backend tests                 |
```

- [ ] **Step 4: Update the update policy and changelog**

Replace the update-policy table body with:

```markdown
| Path           | Update kapan?                                 | Siapa boleh ubah?                      |
| -------------- | --------------------------------------------- | -------------------------------------- |
| `../README.md` | Positioning produk atau capability berubah    | Maintainer                             |
| `design/`      | Setiap keputusan arsitektur berubah           | Dengan review — kontrak hidup teknikal |
| `features/`    | Setiap perubahan spec produk                  | Dengan review — kontrak hidup produk   |
| `ui/`          | Setiap perubahan komponen shell/editor        | Dengan review                          |
| `superpowers/` | Arsip historis; tambah file baru, jangan edit | —                                      |
```

Append this row to the changelog table:

```markdown
| 2026-10-27 | rapikan index: status snapshot, struktur aktual, dan sumber kebenaran README |
```

- [ ] **Step 5: Add the source-of-truth principle**

Append a sixth item to `## Prinsip dokumentasi`:

```markdown
6. **README root menang saat konflik.** Kalau dokumen di folder ini bertentangan dengan [`../README.md`](../README.md), README adalah sumber kebenaran dan dokumen tersebut perlu diperbarui.
```

- [ ] **Step 6: Verify the index**

Run:

```bash
rg -n "ITSM.*future|future.*ITSM|6 apps, 15 packages" docs/README.md
```

Expected: no output.

Run:

```bash
python3 - <<'PY'
import pathlib, re
p = pathlib.Path("docs/README.md")
for i, line in enumerate(p.read_text().splitlines(), 1):
    for match in re.finditer(r"\]\(([^)#]+)(?:#[^)]*)?\)", line):
        target = match.group(1)
        if target.startswith(("http", "mailto:", "#")):
            continue
        path = (p.parent / target.split("?")[0]).resolve()
        print(("OK " if path.exists() else "MISSING "), i, target)
PY
```

Expected: every line starts with `OK`.

---

### Task 5: Sync CONTRIBUTING with the fork and current stack

**Files:**

- Modify: `CONTRIBUTING.md`

- [ ] **Step 1: Replace the title and issue-intake section**

Replace `# Contributing to Plane` through the paragraph ending with
`You can open a new issue with this [issue form](https://github.com/makeplane/plane/issues/new).`
with:

```markdown
# Contributing to Terraline

Thank you for showing an interest in contributing to Terraline! All kinds of contributions are valuable to the project. In this guide, we will cover how you can quickly onboard and make your first contribution.

Terraline is a modified fork of [Plane](https://github.com/makeplane/plane). Upstream code remains subject to its original copyright and AGPL-3.0 license.

## Submitting an issue

Before submitting a new issue, please search the [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues) tab. Maybe an issue or discussion already exists and might inform you of workarounds. Otherwise, you can give new information.

While we want to fix all the [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues), before fixing a bug we need to be able to reproduce and confirm it. Please provide us with a minimal reproduction scenario using a repository or [Gist](https://gist.github.com/). Having a live, reproducible scenario gives us the information without asking questions back & forth with additional questions like:

- 3rd-party libraries being used and their versions
- a use-case that fails

Without said minimal reproduction, we won't be able to investigate all [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues), and the issue might not be resolved.

You can open a new issue with this [issue form](https://github.com/ghifarizuhir/plane-for-itsm/issues/new).
```

- [ ] **Step 2: Replace requirements and setup instructions**

Replace the `### Requirements` list and the numbered setup through
`That's it! You're all set to begin coding...` with:

````markdown
### Requirements

- Docker Engine with Docker Compose
- Node.js >= 22.22
- pnpm 11.10 (enabled through Corepack)
- **Memory**: Minimum **12 GB RAM** recommended
  > ⚠️ Running the project on a system with only 8 GB RAM may lead to setup failures or memory crashes (especially during Docker container build/start or dependency install). Use cloud environments like GitHub Codespaces or upgrade local RAM if possible.

Postgres, Valkey/Redis, and MinIO run inside Docker; you do not need local installations.

### Setup the project

The project is a pnpm + Turbo monorepo. The primary API is Rust (`apps/api-rs`); the Django app (`apps/api`) remains as an opt-in fallback.

1. Clone the repo

```bash
git clone https://github.com/ghifarizuhir/plane-for-itsm.git [folder-name]
cd [folder-name]
chmod +x setup.sh
```
````

2. Run setup.sh

```bash
./setup.sh
```

3. Start the backend containers

```bash
docker compose -f docker-compose-local.yml up -d --build api worker beat-worker
```

4. Start web apps:

```bash
pnpm dev
```

5. Open your browser to http://localhost:3001/god-mode/ and register yourself as instance admin
6. Open up your browser to http://localhost:3000 then log in using the same credentials from the previous step

Other local endpoints: API at `http://localhost:8000`, live collaboration at `http://localhost:3100`. The web app runs at `http://localhost:3000` and admin at `http://localhost:3001`.

That's it! You're all set to begin coding. Detailed commands and service-specific workflows are in [AGENTS.md](AGENTS.md).

````

- [ ] **Step 3: Update the feature-request and contribution links**

Replace the `## Missing a Feature?` paragraph's upstream links with:

```markdown
If a feature is missing, you can directly _request_ a new one [here](https://github.com/ghifarizuhir/plane-for-itsm/issues/new?labels=feature). You can also choose "🚀 Feature" when raising a [New Issue](https://github.com/ghifarizuhir/plane-for-itsm/issues/new/choose) on this repository.
If you would like to _implement_ it, an issue with your proposal must be submitted first, to be sure that we can use it. Please consider the guidelines given below.
````

Replace the `## Ways to contribute` list with:

```markdown
- Try Terraline and the self-hosting platform and give feedback
- Add new integrations
- Add or update translations
- Help with open [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues) or [create your own](https://github.com/ghifarizuhir/plane-for-itsm/issues/new/choose)
- Share your thoughts and suggestions
- Help create tutorials and blog posts
- Request a feature by submitting a proposal
- Report a bug
- **Improve documentation** — fix incomplete or missing docs in [`docs/`](docs/README.md), bad wording, examples, or explanations.
```

- [ ] **Step 4: Replace the closing help section and add the license section**

Replace `## Need help? Questions and suggestions` and its paragraph with:

````markdown
## License

Terraline is licensed under the GNU Affero General Public License v3.0 only (`AGPL-3.0-only`). By contributing, you agree that your contribution is licensed under the same terms. You retain copyright over your contribution. See [LICENSE.txt](LICENSE.txt) and [NOTICE](NOTICE).

New source files authored for Terraline should carry:

```text
Copyright (c) 2026-present Terraline contributors
SPDX-License-Identifier: AGPL-3.0-only
See the LICENSE file for details.
```
````

When you substantially modify an upstream Plane file, keep the existing upstream
copyright header and add a `Copyright (c) 2026-present Terraline contributors`
line. Do not remove upstream notices. `COPYRIGHT.txt` remains the upstream
header template used by the copyright-check tooling.

## Need help? Questions and suggestions

Questions, suggestions, and thoughts are most welcome. Open an issue in this repository or read [AGENTS.md](AGENTS.md) for the development workflow.

````

- [ ] **Step 5: Verify no upstream-only links remain**

Run:

```bash
rg -n "makeplane/plane/(issues|pulls)|docs\.plane\.so|forum\.plane\.so|Plane Cloud" CONTRIBUTING.md
````

Expected: no output.

Run:

```bash
rg -n "Plane for ITSM|Contributing to Plane" CONTRIBUTING.md
```

Expected: no output.

---

### Task 6: Final cross-file verification

**Files:**

- Verify only: `README.md`, `NOTICE`, `package.json`, `docs/README.md`, `CONTRIBUTING.md`, `docs/superpowers/specs/2026-10-27-docs-readme-license-tidy-design.md`

- [ ] **Step 1: Check for stale brand text across the changed files**

Run:

```bash
rg -n "Plane for ITSM" README.md NOTICE package.json docs/README.md CONTRIBUTING.md
```

Expected: no output.

Run:

```bash
rg -n "section 13|Terraline contributors" README.md NOTICE CONTRIBUTING.md
```

Expected: at least one match for section 13 in README and NOTICE, and the
header-policy text in CONTRIBUTING.

- [ ] **Step 2: Check all relative links in all changed Markdown files**

Run:

```bash
python3 - <<'PY'
import pathlib, re
files = [
    pathlib.Path("README.md"),
    pathlib.Path("NOTICE"),
    pathlib.Path("docs/README.md"),
    pathlib.Path("CONTRIBUTING.md"),
]
failed = False
for p in files:
    for i, line in enumerate(p.read_text().splitlines(), 1):
        for match in re.finditer(r"\]\(([^)#]+)(?:#[^)]*)?\)", line):
            target = match.group(1)
            if target.startswith(("http", "mailto:", "#")):
                continue
            resolved = (p.parent / target.split("?")[0]).resolve()
            ok = resolved.exists()
            failed = failed or not ok
            print(("OK " if ok else "MISSING "), p, i, target)
raise SystemExit(1 if failed else 0)
PY
```

Expected: every line starts with `OK`, exit code 0.

- [ ] **Step 3: Validate JSON and whitespace**

Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('package.json','utf8')); console.log('package.json OK')"
git diff --check
```

Expected: `package.json OK` and no whitespace errors.

- [ ] **Step 4: Confirm the intended file set**

Run:

```bash
git status --short
git diff --stat
```

Expected changed files:

```text
 CONTRIBUTING.md
 README.md
 docs/README.md
 package.json
 NOTICE                     (new)
 docs/superpowers/specs/2026-10-27-docs-readme-license-tidy-design.md (new)
```

Expected: no application code, lockfile, or dependency changes.
