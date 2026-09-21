# ITSM Copy Kit — Wave 1 (First-run & Branding) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reposisi copy first-run & branding Terraline dari project management ke IT service management (dual-vocabulary, honest by architecture) sesuai spec `docs/superpowers/specs/2026-09-21-itsm-copy-kit-wave1-design.md`.

**Architecture:** String-only changes. Tidak ada perubahan key i18n, route, data model, atau behavior. File i18n `en` diubah value-nya; string hardcoded di `apps/web` diganti literal; metadata/positioning di package & app roots diganti. Locale lain dibiarkan stale.

**Tech Stack:** JSON i18n (`@plane/i18n`), React Router 8 (+ types), TypeScript strict, oxfmt (pre-commit lint-staged), OxLint, pnpm + Turbo.

---

## Baseline & global constraints

- **Baseline yang sudah dicek (2026-09-21):**
  - `pnpm --filter=@plane/i18n check:types` → PASS.
  - `pnpm --filter=@plane/constants check:types` → PASS; `check:lint` → 2 warnings, 0 errors.
  - `pnpm --filter=web check:lint` → 729 warnings, 0 errors (exit 0).
  - `pnpm --filter=web check:types` → **FAIL pre-existing**, 3 error di `core/components/workspace/settings/members-list.tsx:66` (`toSorted`). Jangan jadikan gate; jangan perbaiki (di luar scope).
  - `pnpm --filter=@plane/i18n check:sync` → **FAIL pre-existing** (key lama belum ada di locale lain, mis. `home.empty.create_project.cta_view_services`). Jangan jalankan sebagai gate.
- **Jangan ubah key i18n.** Hanya value. Kalau oxfmt/lint-staged mereformat file saat commit, itu wajar.
- **Locale `en` saja.** Locale lain (id, ja, ka-ge, …) tidak disentuh.
- **Apostrof dalam string baru pakai ASCII `'`** (konsisten dengan string sekitar), em dash pakai `—` (U+2014), sama seperti string lama.
- Semua perintah dijalankan dari root repo `/home/ghifari/plane-for-itsm`.

---

## File structure

| File                                                      | Aksi   | Isi perubahan                                          |
| --------------------------------------------------------- | ------ | ------------------------------------------------------ |
| `packages/i18n/src/locales/en/empty-state.json`           | Modify | 10 value (pages, cycles, modules, epics, intake, wiki) |
| `packages/i18n/src/locales/en/project.json`               | Modify | 8 value (cycle, module, issues, page empty state)      |
| `packages/i18n/src/locales/en/workspace.json`             | Modify | 9 value (dashboard, analytics, projects, pages, views) |
| `packages/i18n/src/locales/en/tour.json`                  | Modify | 6 value (workitems, module, page, intake, seed_data)   |
| `apps/web/core/components/onboarding/tour/root.tsx`       | Modify | 7 string literal tour + welcome modal                  |
| `apps/web/core/components/onboarding/steps/role/root.tsx` | Modify | 6 label persona                                        |
| `apps/web/core/components/instance/not-ready-view.tsx`    | Modify | 1 string literal                                       |
| `packages/constants/src/metadata.ts`                      | Modify | 9 konstanta metadata                                   |
| `apps/web/app/root.tsx`                                   | Modify | title, OG, keywords                                    |
| `apps/admin/app/root.tsx`                                 | Modify | title, description, keywords                           |
| `apps/space/app/root.tsx`                                 | Modify | title, description, keywords                           |
| `apps/space/app/issues/[anchor]/layout.tsx`               | Modify | OG description default                                 |
| `apps/web/manifest.json`                                  | Modify | name, description                                      |
| `apps/web/public/site.webmanifest.json`                   | Modify | description                                            |
| `apps/admin/public/site.webmanifest.json`                 | Modify | description                                            |
| `apps/space/public/site.webmanifest.json`                 | Modify | description                                            |
| `apps/space/app/assets/favicon/site.webmanifest`          | Modify | description                                            |
| `apps/web/core/components/core/page-title.tsx`            | Modify | fallback document title                                |
| `apps/web/core/components/auth-screens/footer.tsx`        | Modify | social proof line                                      |
| `package.json`                                            | Modify | repo description                                       |
| `README.md`                                               | Modify | hero tagline + intro line                              |

---

### Task 1: Empty-state i18n — `empty-state.json`

**Files:**

- Modify: `packages/i18n/src/locales/en/empty-state.json`

- [ ] **Step 1: Apply all 10 value edits**

Edit 1 — `project_empty_state.pages.title`:

```json
"title": "Document everything — from notes to PRDs",
```

→

```json
"title": "Document everything — from runbooks to postmortems",
```

Edit 2 — `project_empty_state.pages.description`:

```json
"description": "Pages let you capture and organize information in one place. Write meeting notes, project documentation, and PRDs, embed work items, and structure them with ready-to-use components.",
```

→

```json
"description": "Pages let you capture and organize operational knowledge in one place. Write runbooks, SOPs, shift handovers, and postmortems, embed work items, and structure them with ready-to-use components.",
```

Edit 3 — `project_empty_state.cycles.description`:

```json
"description": "Break work down by timeboxed chunks, work backwards from your project deadline to set dates, and make tangible progress as a team.",
```

→

```json
"description": "Break delivery work into timeboxed chunks, set dates around your deadlines, and keep your team's progress visible.",
```

Edit 4 — `project_empty_state.modules.title`:

```json
"title": "Map your project goals to Modules and track easily.",
```

→

```json
"title": "Group related work into Modules and track it easily.",
```

Edit 5 — `project_empty_state.modules.description`:

```json
"description": "Modules are made up of interconnected work items. They assist in monitoring progress through project phases, each with specific deadlines and analytics to indicate how close you are to achieving those phases.",
```

→

```json
"description": "Modules group interconnected work items — by service, platform, or phase — with their own deadlines and analytics, so you can see how close you are to done.",
```

Edit 6 — `project_empty_state.epics.title`:

```json
"title": "Turn complex projects into structured epics.",
```

→

```json
"title": "Turn large efforts into structured Epics.",
```

Edit 7 — `project_empty_state.epics.description`:

```json
"description": "An epic helps you organize big goals into smaller, trackable tasks.",
```

→

```json
"description": "An Epic helps you organize a large initiative into smaller, trackable work items.",
```

Edit 8 — `project_empty_state.intake_sidebar.description`:

```json
"description": "Submit new requests to be reviewed, prioritized, and tracked within your project's workflow.",
```

→

```json
"description": "Submit service requests to be reviewed, prioritized, and tracked in your project's workflow.",
```

Edit 9 — `workspace_empty_state.wiki.description`:

```json
"description": "Pages are thought spotting space in Terraline. Take down meeting notes, format them easily, embed work items, lay them out using a library of components, and keep them all in your project's context.",
```

→

```json
"description": "Pages are your team's knowledge base in Terraline. Write runbooks, SOPs, and postmortems, format them easily, embed work items, lay them out using a library of components, and keep them in your project's context.",
```

Edit 10 — `workspace_empty_state.analytics_no_cycle.title`:

```json
"title": "Create cycles to organise work into time-bound phases and track progress across sprints."
```

→

```json
"title": "Create cycles to organize work into time-bound phases and track delivery progress."
```

- [ ] **Step 2: Validate JSON**

Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/empty-state.json','utf8')); console.log('valid')"
```

Expected: `valid`

- [ ] **Step 3: Audit old strings are gone**

Run:

```bash
rg -n "PRDs|across sprints|project milestones to Modules|thought spotting" packages/i18n/src/locales/en/empty-state.json
```

Expected: no output (exit 1).

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/en/empty-state.json
git commit -m "feat(i18n): reposition empty-state copy to ITSM voice"
```

---

### Task 2: Empty-state i18n — `project.json`

**Files:**

- Modify: `packages/i18n/src/locales/en/project.json`

- [ ] **Step 1: Apply all 8 value edits**

Edit 1 — `project_cycle.empty_state.general.description`:

```json
"description": "Break work down by timeboxed chunks, work backwards from your project deadline to set dates, and make tangible progress as a team.",
```

→

```json
"description": "Break delivery work into timeboxed chunks, set dates around your deadlines, and keep your team's progress visible.",
```

Edit 2 — `project_cycle.empty_state.general.primary_button.comic.description`:

```json
"description": "A sprint, an iteration, and or any other term you use for weekly or fortnightly tracking of work is a cycle."
```

→

```json
"description": "A recurring window — weekly, fortnightly, or per maintenance cycle — used to track a chunk of work is a Cycle."
```

Edit 3 — `project_module.empty_state.general.title`:

```json
"title": "Map your project milestones to Modules and track aggregated work easily.",
```

→

```json
"title": "Group related work into Modules and track it easily.",
```

Edit 4 — `project_module.empty_state.general.description`:

```json
"description": "A group of work items that belong to a logical, hierarchical parent form a module. Think of them as a way to track work by project milestones. They have their own periods and deadlines as well as analytics to help you see how close or far you are from a milestone.",
```

→

```json
"description": "A Module groups work items under a logical parent — a service, a platform, or a large initiative. Modules have their own periods, deadlines, and analytics so you can see how far you are from done."
```

Edit 5 — `project_module.empty_state.general.primary_button.comic.description`:

```json
"description": "A cart module, a chassis module, and a warehouse module are all good example of this grouping."
```

→

```json
"description": "A payments service module, an identity platform module, and a data warehouse module are good examples of this grouping."
```

Edit 6 — `project_issues.empty_state.no_issues.description`:

```json
"description": "Think of work items as jobs, tasks, work, or JTBD. Which we like. A work item and its sub-work items are usually time-based actionables assigned to members of your team. Your team creates, assigns, and completes work items to move your project towards its goal.",
```

→

```json
"description": "Think of work items as tasks, tickets, or requests. A work item and its sub-work items are usually time-based actionables assigned to members of your team. Your team creates, assigns, and completes work items to move your project forward.",
```

Edit 7 — `project_issues.empty_state.no_issues.primary_button.comic.description`:

```json
"description": "Redesign the Terraline UI, Rebrand the company, or Launch the new fuel injection system are examples of work items that likely have sub-work items."
```

→

```json
"description": "Onboard a new identity provider, migrate a database, or roll out a service desk process are examples of work items that likely have sub-work items."
```

Edit 8 — `project_page.empty_state.general.description`:

```json
"description": "Pages are thoughts potting space in Terraline. Take down meeting notes, format them easily, embed work items, lay them out using a library of components, and keep them all in your project's context. To make short work of any doc, invoke Galileo, Terraline's AI, with a shortcut or the click of a button.",
```

→

```json
"description": "Pages are your team's knowledge base in Terraline. Write runbooks, SOPs, and postmortems, format them easily, embed work items, lay them out using a library of components, and keep them in your project's context. To make short work of any doc, invoke Galileo, Terraline's AI, with a shortcut or the click of a button.",
```

- [ ] **Step 2: Validate JSON**

Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/project.json','utf8')); console.log('valid')"
```

Expected: `valid`

- [ ] **Step 3: Audit old strings are gone**

Run:

```bash
rg -n "A sprint, an iteration|fuel injection|thoughts potting|JTBD|project milestones" packages/i18n/src/locales/en/project.json
```

Expected: no output (exit 1).

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/en/project.json
git commit -m "feat(i18n): reposition project empty-state copy to ITSM voice"
```

---

### Task 3: Empty-state i18n — `workspace.json`

**Files:**

- Modify: `packages/i18n/src/locales/en/workspace.json`

Catatan: comic description `"A project could be a product's roadmap, a marketing campaign, or launching a new car."` muncul **3×** (dashboard, projects general, projects no_projects) — pakai replace-all.

- [ ] **Step 1: Apply all 9 value edits**

Edit 1 — `workspace_dashboard.empty_state.general.description`:

```json
"description": "Welcome to Terraline, we are excited to have you here. Create your first project and track your work items, and this page will transform into a space that helps you progress. Admins will also see items which help their team progress.",
```

→

```json
"description": "Welcome to Terraline. Create your first project, add your services, and start tracking requests and work items — this page will turn into your operations overview.",
```

Edit 2 — comic description (3 occurrences, replace all):

```json
"description": "A project could be a product's roadmap, a marketing campaign, or launching a new car."
```

→

```json
"description": "A project could be a customer-facing API, an internal HR portal, or a payroll service."
```

Edit 3 — `workspace_analytics.empty_state.general.title`:

```json
"title": "Track progress, workloads, and allocations. Spot trends, remove blockers, and move work faster",
```

→

```json
"title": "Track throughput, workloads, and trends. Spot bottlenecks and keep work moving",
```

Edit 4 — `workspace_analytics.empty_state.general.description`:

```json
"description": "See scope versus demand, estimates, and scope creep. Get performance by team members and teams, and make sure your project runs on time.",
```

→

```json
"description": "See how work moves from open to done, where time goes, and how your team is performing — so services stay on track.",
```

Edit 5 — `workspace_analytics.empty_state.general.primary_button.comic.description`:

```json
"description": "First, timebox your issues into Cycles and, if you can, group issues that span more than a cycle into Modules. Check out both on the left nav."
```

→

```json
"description": "First, timebox your work items into Cycles and group longer efforts into Modules. Check out both on the left nav."
```

Edit 6 — `workspace_projects.empty_state.general.description`:

```json
"description": "Think of each project as the parent for goal-oriented work. Projects are where Jobs, Cycles, and Modules live and, along with your colleagues, help you achieve that goal. Create a new project or filter for archived projects.",
```

→

```json
"description": "Think of each project as the home for a service or an initiative. Projects are where services, requests, and work items live, and where your team keeps everything on track. Create a new project or filter for archived projects.",
```

Edit 7 — `workspace_pages.empty_state.general.description`:

```json
"description": "Pages are thoughts potting space in Terraline. Take down meeting notes, format them easily, embed work items, lay them out using a library of components, and keep them all in your project's context. To make short work of any doc, invoke Galileo, Terraline's AI, with a shortcut or the click of a button.",
```

→

```json
"description": "Pages are your team's knowledge base in Terraline. Write runbooks, SOPs, and postmortems, format them easily, embed work items, lay them out using a library of components, and keep them in your project's context. To make short work of any doc, invoke Galileo, Terraline's AI, with a shortcut or the click of a button.",
```

Edit 8 — `workspace_pages.empty_state.private.description`:

```json
"description": "Keep your private thoughts here. When you're ready to share, the team's just a click away.",
```

→

```json
"description": "Keep private notes here. When you're ready to share, your team is a click away.",
```

Edit 9 — `workspace_views.empty_state.all-issues.description`:

```json
"description": "First project done! Now, slice your work into trackable pieces with work items. Let's go!",
```

→

```json
"description": "First project done! Now break your work into trackable pieces with work items.",
```

- [ ] **Step 2: Validate JSON**

Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/workspace.json','utf8')); console.log('valid')"
```

Expected: `valid`

- [ ] **Step 3: Audit old strings are gone**

Run:

```bash
rg -n "marketing campaign|scope creep|thoughts potting|Jobs, Cycles" packages/i18n/src/locales/en/workspace.json
```

Expected: no output (exit 1).

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/en/workspace.json
git commit -m "feat(i18n): reposition workspace empty-state copy to ITSM voice"
```

---

### Task 4: Onboarding tour i18n — `tour.json`

**Files:**

- Modify: `packages/i18n/src/locales/en/tour.json`

Catatan: module description identik di `module.step_zero` dan `module.step_one` — pakai replace-all.

- [ ] **Step 1: Apply all 6 value edits**

Edit 1 — `product_tour.workitems.step_one.description`:

```json
"description": "Start by clicking the “+ New Work Item” button. You can create tasks, bugs, or custom type that fits your needs."
```

→

```json
"description": "Start by clicking the “+ New Work Item” button. You can create requests, incidents, tasks, or any custom type that fits your needs."
```

Edit 2 — `product_tour.module.step_zero.title`:

```json
"title": "Break your project down into Modules",
```

→

```json
"title": "Group related work into Modules",
```

Edit 3 — module description (2 occurrences, replace all):

```json
"description": "Modules are smaller, focused projects that help users group and organize work items within specific time frames."
```

→

```json
"description": "Modules group and organize work items within specific time frames — by service, platform, or initiative."
```

Edit 4 — `product_tour.page.step_zero.description`:

```json
"description": "Pages in Terraline let you capture, organize, and collaborate on project info—no external tools needed."
```

→

```json
"description": "Pages in Terraline let you capture, organize, and collaborate on operational knowledge — no external tools needed."
```

Edit 5 — `product_tour.intake.step_zero.description`:

```json
"description": "A Terraline-only feature that lets Guests create work items for bugs, requests, or tickets."
```

→

```json
"description": "A Terraline-only feature that lets Guests create work items for requests, incidents, or tickets."
```

Edit 6 — `product_tour.seed_data.description`:

```json
"description": "Projects let you manage your teams, tasks, and everything you need to get things done within your workspace."
```

→

```json
"description": "Projects are where your services, requests, and delivery work live — everything your team needs in one place."
```

- [ ] **Step 2: Validate JSON**

Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('packages/i18n/src/locales/en/tour.json','utf8')); console.log('valid')"
```

Expected: `valid`

- [ ] **Step 3: Audit old strings are gone**

Run:

```bash
rg -n "smaller, focused projects|project info|for bugs, requests" packages/i18n/src/locales/en/tour.json
```

Expected: no output (exit 1).

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales/en/tour.json
git commit -m "feat(i18n): reposition product tour copy to ITSM voice"
```

---

### Task 5: Hardcoded onboarding strings

**Files:**

- Modify: `apps/web/core/components/onboarding/tour/root.tsx`
- Modify: `apps/web/core/components/onboarding/steps/role/root.tsx`
- Modify: `apps/web/core/components/instance/not-ready-view.tsx`

- [ ] **Step 1: Edit `apps/web/core/components/onboarding/tour/root.tsx` (7 edits)**

Work-items step:

```tsx
    title: "Plan with work items",
    description:
      "The work item is the building block of the Terraline. Most concepts in Terraline are either associated with work items and their properties.",
```

→

```tsx
    title: "Start with work items",
    description: "Work items are the building block of Terraline. Most concepts are tied to work items and their properties.",
```

Cycles step:

```tsx
    description:
      "Cycles help you and your team to progress faster, similar to the sprints commonly used in agile development.",
```

→

```tsx
    description: "Cycles group work into timeboxed windows so you and your team can move delivery forward together.",
```

Modules step:

```tsx
    title: "Break into modules",
    description: "Modules break your big thing into Projects or Features, to help you organize better.",
```

→

```tsx
    title: "Group work in modules",
    description: "Modules group work items by service, platform, or initiative to keep large efforts organized.",
```

Pages step:

```tsx
    title: "Document with pages",
    description: "Use Pages to quickly jot down work items when you're in a meeting or starting a day.",
```

→

```tsx
    title: "Document with pages",
    description: "Use Pages to write runbooks, SOPs, and postmortems — your team's operational knowledge base.",
```

Welcome modal paragraph:

```tsx
                We{"'"}re glad that you decided to try out Terraline. You can now manage your projects with ease. Get
                started by creating a project.
```

→

```tsx
                We{"'"}re glad you decided to try Terraline. Run your services, handle requests, and keep delivery work
                moving. Get started by creating a project.
```

- [ ] **Step 2: Edit `apps/web/core/components/onboarding/steps/role/root.tsx` (6 label edits)**

Tetap di dalam array `ROLES` (id internal tidak berubah):

```tsx
  { id: "product-manager", label: "Product Manager", icon: CubeOutline },
  { id: "engineering-manager", label: "Engineering Manager", icon: ViewsOutline },
  { id: "designer", label: "Designer", icon: PenTool },
  { id: "developer", label: "Developer", icon: MonitorOutline },
  { id: "founder-executive", label: "Founder/Executive", icon: RocketOutline },
  { id: "operations-manager", label: "Operations Manager", icon: RefreshOutline },
```

→

```tsx
  { id: "product-manager", label: "Service Desk Manager", icon: CubeOutline },
  { id: "engineering-manager", label: "IT Operations Manager", icon: ViewsOutline },
  { id: "designer", label: "Incident Manager", icon: PenTool },
  { id: "developer", label: "IT Support Agent", icon: MonitorOutline },
  { id: "founder-executive", label: "IT Manager/Executive", icon: RocketOutline },
  { id: "operations-manager", label: "Platform Engineer", icon: RefreshOutline },
```

- [ ] **Step 3: Edit `apps/web/core/components/instance/not-ready-view.tsx` (1 edit)**

```tsx
                  Set up your instance and create your first workspace to begin managing projects and work.
```

→

```tsx
                  Set up your instance and create your first workspace to begin running services, requests, and delivery work.
```

- [ ] **Step 4: Audit old strings are gone**

Run:

```bash
rg -n "sprints commonly used|Projects or Features|Product Manager|Engineering Manager|managing projects and work" apps/web/core/components/onboarding apps/web/core/components/instance/not-ready-view.tsx
```

Expected: no output (exit 1).

- [ ] **Step 5: Lint**

Run:

```bash
pnpm --filter=web check:lint 2>&1 | tail -3
```

Expected: `Found ... warnings and 0 errors.` (warnings baseline ~729, tidak masalah).

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/onboarding/tour/root.tsx apps/web/core/components/onboarding/steps/role/root.tsx apps/web/core/components/instance/not-ready-view.tsx
git commit -m "feat(web): reposition onboarding copy to ITSM voice"
```

---

### Task 6: Site metadata — `metadata.ts`

**Files:**

- Modify: `packages/constants/src/metadata.ts:7-23`

- [ ] **Step 1: Replace the constants block**

Ganti isi file dari baris 7 sampai 23 menjadi:

```ts
export const SITE_NAME = "Terraline | Open-source IT service management platform.";
export const SITE_TITLE = "Terraline | Open-source IT service management platform.";
export const SITE_DESCRIPTION =
  "Open-source IT service management platform to run services, handle requests, and track delivery work in one place";
export const SITE_KEYWORDS =
  "IT service management, ITSM, service desk, helpdesk, service catalog, service map, service health, IT operations, incident tracking, knowledge base, work items";
export const SITE_URL = "https://app.terraline.space/";
export const TWITTER_USER_NAME = "Terraline | Open-source IT service management platform.";

// Terraline Sites Metadata
export const SPACE_SITE_NAME =
  "Terraline Publish | Share your Terraline projects and work items publicly with one click. ";
export const SPACE_SITE_TITLE = "Terraline Publish | Share your Terraline projects publicly with one click";
export const SPACE_SITE_DESCRIPTION =
  "Terraline Publish is a public publishing tool for your Terraline projects, built on top of terraline.space";
export const SPACE_SITE_KEYWORDS =
  "IT service management, ITSM, service desk, public service status, customer feedback, service catalog, IT operations, knowledge base, collaboration";
export const SPACE_SITE_URL = "https://app.terraline.space/";
export const SPACE_TWITTER_USER_NAME = "terraline";
```

- [ ] **Step 2: Type-check & lint**

Run:

```bash
pnpm --filter=@plane/constants check:types 2>&1 | tail -3; pnpm --filter=@plane/constants check:lint 2>&1 | tail -2
```

Expected: `check:types` exit 0 tanpa error; lint `Found 2 warnings and 0 errors.` (baseline).

- [ ] **Step 3: Audit old strings are gone**

Run:

```bash
rg -n "project management tool|product roadmaps|scrum|kanban" packages/constants/src/metadata.ts
```

Expected: no output (exit 1).

- [ ] **Step 4: Commit**

```bash
git add packages/constants/src/metadata.ts
git commit -m "feat(constants): ITSM positioning for site metadata"
```

---

### Task 7: App metadata, manifests, page title, auth footer

**Files:**

- Modify: `apps/web/app/root.tsx`
- Modify: `apps/admin/app/root.tsx`
- Modify: `apps/space/app/root.tsx`
- Modify: `apps/space/app/issues/[anchor]/layout.tsx`
- Modify: `apps/web/manifest.json`
- Modify: `apps/web/public/site.webmanifest.json`
- Modify: `apps/admin/public/site.webmanifest.json`
- Modify: `apps/space/public/site.webmanifest.json`
- Modify: `apps/space/app/assets/favicon/site.webmanifest`
- Modify: `apps/web/core/components/core/page-title.tsx`
- Modify: `apps/web/core/components/auth-screens/footer.tsx`

- [ ] **Step 1: Edit `apps/web/app/root.tsx`**

Line 35:

```tsx
const APP_TITLE = "Terraline | Simple, extensible, open-source project management tool.";
```

→

```tsx
const APP_TITLE = "Terraline | Open-source IT service management platform.";
```

Lines 89-92:

```tsx
  {
    property: "og:description",
    content: "Open-source project management tool to manage work items, cycles, and product roadmaps easily",
  },
```

→

```tsx
  {
    property: "og:description",
    content: "Open-source IT service management platform to run services, handle requests, and track delivery work in one place",
  },
```

Lines 97 dan 107 (dua tempat, samakan):

```tsx
  { property: "og:image:alt", content: "Terraline - Modern work management" },
```

→

```tsx
  { property: "og:image:alt", content: "Terraline - Open-source IT service management" },
```

```tsx
  { name: "twitter:image:alt", content: "Terraline - Modern work management" },
```

→

```tsx
  { name: "twitter:image:alt", content: "Terraline - Open-source IT service management" },
```

Lines 99-102:

```tsx
  {
    name: "keywords",
    content:
      "software development, plan, ship, software, accelerate, code management, release management, project management, work item tracking, agile, scrum, kanban, collaboration",
  },
```

→

```tsx
  {
    name: "keywords",
    content:
      "IT service management, ITSM, service desk, helpdesk, service catalog, service map, service health, IT operations, incident tracking, knowledge base, work items",
  },
```

- [ ] **Step 2: Edit `apps/admin/app/root.tsx`**

Lines 24-26:

```tsx
const APP_TITLE = "Terraline | Simple, extensible, open-source project management tool.";
const APP_DESCRIPTION =
  "Open-source project management tool to manage work items, sprints, and product roadmaps with peace of mind.";
```

→

```tsx
const APP_TITLE = "Terraline | Open-source IT service management platform.";
const APP_DESCRIPTION = "Open-source IT service management platform for services, requests, and delivery work.";
```

Lines 67-71:

```tsx
  {
    name: "keywords",
    content:
      "software development, customer feedback, software, accelerate, code management, release management, project management, work items tracking, agile, scrum, kanban, collaboration",
  },
```

→

```tsx
  {
    name: "keywords",
    content:
      "IT service management, ITSM, service desk, helpdesk, service catalog, service map, service health, IT operations, incident tracking, knowledge base, work items",
  },
```

- [ ] **Step 3: Edit `apps/space/app/root.tsx`**

Lines 27-28:

```tsx
const APP_TITLE = "Terraline Publish | Make your Terraline boards public with one-click";
const APP_DESCRIPTION = "Terraline Publish is a customer feedback management tool built on top of terraline.space";
```

→

```tsx
const APP_TITLE = "Terraline Publish | Share your Terraline projects publicly with one click";
const APP_DESCRIPTION =
  "Terraline Publish is a public publishing tool for your Terraline projects, built on top of terraline.space";
```

Lines 78-82:

```tsx
  {
    name: "keywords",
    content:
      "software development, customer feedback, software, accelerate, code management, release management, project management, work item tracking, agile, scrum, kanban, collaboration",
  },
```

→

```tsx
  {
    name: "keywords",
    content:
      "IT service management, ITSM, service desk, public service status, customer feedback, service catalog, IT operations, knowledge base, collaboration",
  },
```

- [ ] **Step 4: Edit `apps/space/app/issues/[anchor]/layout.tsx` (line 23)**

```tsx
const DEFAULT_DESCRIPTION = "Made with Terraline, an AI-powered work management platform with publishing capabilities.";
```

→

```tsx
const DEFAULT_DESCRIPTION =
  "Made with Terraline, an AI-powered IT service management platform with publishing capabilities.";
```

- [ ] **Step 5: Edit manifests**

`apps/web/manifest.json` lines 7 & 9:

```json
  "name": "Terraline | Modern work management",
  "short_name": "Terraline",
  "description": "Terraline accelerates software development for agencies and product companies.",
```

→

```json
  "name": "Terraline | IT service management",
  "short_name": "Terraline",
  "description": "Run your services, handle requests, and keep delivery work moving — all in Terraline.",
```

`apps/web/public/site.webmanifest.json`, `apps/space/public/site.webmanifest.json`, `apps/space/app/assets/favicon/site.webmanifest` (1 occurrence each):

```json
  "description": "Terraline helps you plan your work items, cycles, and product modules.",
```

→

```json
  "description": "Terraline helps you run services, handle requests, and track work items.",
```

`apps/admin/public/site.webmanifest.json`:

```json
  "description": "Terraline helps you plan your issues, cycles, and product modules.",
```

→

```json
  "description": "Terraline helps you run services, handle requests, and track work items.",
```

- [ ] **Step 6: Edit `apps/web/core/components/core/page-title.tsx` (line 19)**

```tsx
document.title = title ?? "Terraline | Simple, extensible, open-source project management tool.";
```

→

```tsx
document.title = title ?? "Terraline | Open-source IT service management platform.";
```

- [ ] **Step 7: Edit `apps/web/core/components/auth-screens/footer.tsx` (line 35)**

```tsx
<span className="text-13 whitespace-nowrap text-tertiary">Join 10,000+ teams building with Terraline</span>
```

→

```tsx
<span className="text-13 whitespace-nowrap text-tertiary">Join 10,000+ IT teams running on Terraline</span>
```

- [ ] **Step 8: Validate JSON manifests & audit**

Run:

```bash
for f in apps/web/manifest.json apps/web/public/site.webmanifest.json apps/admin/public/site.webmanifest.json apps/space/public/site.webmanifest.json apps/space/app/assets/favicon/site.webmanifest; do node -e "JSON.parse(require('fs').readFileSync('$f','utf8'))" || echo "INVALID $f"; done; echo "json ok"
rg -n "project management tool|product roadmaps|Modern work management|plan your (work items|issues)|teams building with" apps/web/app/root.tsx apps/admin/app/root.tsx apps/space/app/root.tsx apps/space/app/issues apps/web/manifest.json apps/web/public/site.webmanifest.json apps/admin/public/site.webmanifest.json apps/space/public/site.webmanifest.json apps/space/app/assets/favicon/site.webmanifest apps/web/core/components/core/page-title.tsx apps/web/core/components/auth-screens/footer.tsx
```

Expected: `json ok`; `rg` no output (exit 1).

- [ ] **Step 9: Lint web, admin, space**

Run:

```bash
pnpm --filter=web check:lint 2>&1 | tail -2; pnpm --filter=admin check:lint 2>&1 | tail -2; pnpm --filter=space check:lint 2>&1 | tail -2
```

Expected: masing-masing `Found ... and 0 errors.`

- [ ] **Step 10: Commit**

```bash
git add apps/web/app/root.tsx apps/admin/app/root.tsx apps/space/app/root.tsx "apps/space/app/issues/[anchor]/layout.tsx" apps/web/manifest.json apps/web/public/site.webmanifest.json apps/admin/public/site.webmanifest.json apps/space/public/site.webmanifest.json apps/space/app/assets/favicon/site.webmanifest apps/web/core/components/core/page-title.tsx apps/web/core/components/auth-screens/footer.tsx
git commit -m "feat: ITSM branding across app metadata and manifests"
```

---

### Task 8: Repo description & README hero

**Files:**

- Modify: `package.json:5`
- Modify: `README.md:8,27`

- [ ] **Step 1: Edit `package.json`**

```json
  "description": "Open-source project management that unlocks customer value",
```

→

```json
  "description": "Open-source IT service management that keeps services running",
```

- [ ] **Step 2: Edit `README.md`**

Line 8:

```html
<p align="center"><b>Modern project management for all teams</b></p>
```

→

```html
<p align="center"><b>Modern IT service management for all teams</b></p>
```

Line 27:

```markdown
Meet [Plane](https://plane.so/), an open-source project management tool to track issues, run ~sprints~ cycles, and manage product roadmaps without the chaos of managing the tool itself. 🧘‍♀️
```

→

```markdown
Meet [Plane](https://plane.so/), an open-source IT service management tool to run services, resolve requests, and track delivery work without the chaos of managing the tool itself. 🧘‍♀️
```

- [ ] **Step 3: Validate & audit**

Run:

```bash
node -e "JSON.parse(require('fs').readFileSync('package.json','utf8')); console.log('valid')"
rg -n "Modern project management|project management tool to track issues" README.md
```

Expected: `valid`; `rg` no output (exit 1).

- [ ] **Step 4: Commit**

```bash
git add package.json README.md
git commit -m "chore: reposition repo description and README hero to ITSM"
```

---

### Task 9: Final verification

**Files:** tidak ada perubahan (hanya verifikasi; commit hanya jika ada perbaikan).

- [ ] **Step 1: Full audit — banned PM/dev phrasing di file yang disentuh**

Run:

```bash
rg -n "PRDs|sprint|scrum|agile|kanban|product roadmaps|marketing campaign|fuel injection|thought spotting|thoughts potting|Modern work management|Modern project management|teams building with|project management tool" \
  packages/i18n/src/locales/en/empty-state.json \
  packages/i18n/src/locales/en/project.json \
  packages/i18n/src/locales/en/workspace.json \
  packages/i18n/src/locales/en/tour.json \
  packages/constants/src/metadata.ts \
  apps/web/app/root.tsx apps/admin/app/root.tsx apps/space/app/root.tsx \
  apps/web/manifest.json apps/web/public/site.webmanifest.json apps/admin/public/site.webmanifest.json \
  apps/space/public/site.webmanifest.json apps/space/app/assets/favicon/site.webmanifest \
  apps/web/core/components/core/page-title.tsx \
  apps/web/core/components/auth-screens/footer.tsx \
  apps/web/core/components/onboarding/tour/root.tsx \
  apps/web/core/components/onboarding/steps/role/root.tsx \
  apps/web/core/components/instance/not-ready-view.tsx \
  README.md package.json
```

Expected: no output (exit 1). Catatan: `apps/web/app/root.tsx` tidak boleh lagi memuat kata-kata ini; kalau ada sisa match, perbaiki.

- [ ] **Step 2: i18n types**

Run:

```bash
pnpm --filter=@plane/i18n check:types 2>&1 | tail -3
```

Expected: `Generated 3939 keys from 29 namespace files` dan exit 0 (baseline PASS).

- [ ] **Step 3: Lint semua paket yang disentuh**

Run:

```bash
pnpm --filter=web check:lint 2>&1 | tail -2; pnpm --filter=admin check:lint 2>&1 | tail -2; pnpm --filter=space check:lint 2>&1 | tail -2; pnpm --filter=@plane/constants check:lint 2>&1 | tail -2
```

Expected: semua `0 errors`.

- [ ] **Step 4: Audit commit history**

Run:

```bash
git log --oneline -8
```

Expected: 8 commit baru dari plan ini (Task 1-8) di atas `85b4e4ad0 docs(superpowers): ITSM copy kit wave 1 design spec`.

- [ ] **Step 5: (Opsional, hanya jika user minta verifikasi visual tunnel) Rebuild & restart prod**

Run:

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build sukses; service aktif. Lihat AGENTS.md untuk aturan port 3000.

---

## Catatan known-failures (jangan dijadikan gate)

- `pnpm --filter=web check:types` gagal karena 3 error pre-existing di `core/components/workspace/settings/members-list.tsx:66`.
- `pnpm --filter=@plane/i18n check:sync` gagal karena key lama belum ada di locale non-`en` (pre-existing).
