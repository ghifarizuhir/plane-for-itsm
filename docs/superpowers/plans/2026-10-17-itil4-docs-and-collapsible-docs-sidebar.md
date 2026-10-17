# ITIL4 Docs Group & Collapsible Docs Sidebar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambahkan grup dokumentasi "ITIL4" (6 topik berbahasa Indonesia yang menjelaskan penerapan ITIL4 dengan fitur platform yang saling terkait) dan buat grup sidebar docs bisa dibuka-tutup dengan perilaku "hanya grup aktif yang terbuka + keputusan manual diingat".

**Architecture:** Semua perubahan di `apps/web/core/components/docs/`. Registry (`registry.ts`) tetap menjadi sumber tunggal topik; konten tetap komponen TSX lazy. Sidebar memakai `Collapsible` dari `@plane/ui` dalam mode terkontrol; logika state murni dipisahkan ke `docs-sidebar-state.ts` (unit-tested) dan hook tipis `use-docs-sidebar-groups.ts` yang menyimpan override di `localStorage`.

**Tech Stack:** React 19, React Router 8 (framework mode, `ssr: false`), Tailwind CSS v4 (token `@plane/*`), Headless UI lewat `@plane/ui` `Collapsible`, Vitest (environment node, `apps/web/vitest.config.ts` include `core/**/*.test.ts`), ikon `@makeplane/propel/icons`.

**Spec:** `docs/superpowers/specs/2026-10-17-itil4-docs-and-collapsible-docs-sidebar-design.md`

**Commands (dari root repo):**

- Unit test web: `pnpm --filter=web test` (filter file: `pnpm --filter=web test -- docs-sidebar-state`)
- Typecheck: `pnpm --filter=web check:types`
- Lint: `pnpm --filter=web check:lint`
- Build: `pnpm --filter=web build`
- Commit menjalankan lint-staged + oxfmt otomatis (husky).

---

## File Structure

| File                                                                 | Aksi   | Tanggung jawab                                                                       |
| -------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------ |
| `apps/web/core/components/docs/docs-sidebar-state.ts`                | Create | Fungsi murni state grup sidebar (parse/serialize/resolve/toggle) + key localStorage  |
| `apps/web/core/components/docs/docs-sidebar-state.test.ts`           | Create | Unit test modul state                                                                |
| `apps/web/core/components/docs/use-docs-sidebar-groups.ts`           | Create | Hook: baca/tulis localStorage, hitung grup aktif, ekspos `isGroupOpen`/`toggleGroup` |
| `apps/web/core/components/docs/docs-sidebar.tsx`                     | Modify | Render grup via `Collapsible` + chevron                                              |
| `apps/web/core/components/docs/blocks.tsx`                           | Modify | Tambah primitif `DocsPracticeMatrix` + tipe `TPracticeUsage`                         |
| `apps/web/core/components/docs/content/itil4-matrix-data.ts`         | Create | Data matrix praktik × fitur (dipakai halaman overview & flow)                        |
| `apps/web/core/components/docs/content/itil4-overview.tsx`           | Create | Topik "ITIL4 di platform ini"                                                        |
| `apps/web/core/components/docs/content/itil4-incident.tsx`           | Create | Topik "Incident management"                                                          |
| `apps/web/core/components/docs/content/itil4-problem.tsx`            | Create | Topik "Problem management"                                                           |
| `apps/web/core/components/docs/content/itil4-service-request.tsx`    | Create | Topik "Service request"                                                              |
| `apps/web/core/components/docs/content/itil4-change-improvement.tsx` | Create | Topik "Change & improvement"                                                         |
| `apps/web/core/components/docs/content/itil4-flow.tsx`               | Create | Topik "Alur organisasi end-to-end"                                                   |
| `apps/web/core/components/docs/registry.ts`                          | Modify | Grup `itil4` + 6 topik baru (bertahap per task)                                      |

---

### Task 1: State helper grup sidebar (TDD)

**Files:**

- Create: `apps/web/core/components/docs/docs-sidebar-state.ts`
- Test: `apps/web/core/components/docs/docs-sidebar-state.test.ts`

- [ ] **Step 1: Write the failing test**

Buat `apps/web/core/components/docs/docs-sidebar-state.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  DOCS_SIDEBAR_GROUPS_KEY,
  parseGroupOverrides,
  resolveGroupOpen,
  serializeGroupOverrides,
  toggleGroupOverrides,
} from "./docs-sidebar-state";

describe("docs sidebar group state", () => {
  it("parses valid overrides", () => {
    expect(parseGroupOverrides('{"member":true,"project":false}')).toEqual({ member: true, project: false });
  });

  it("ignores unknown keys and non-boolean values", () => {
    expect(parseGroupOverrides('{"member":true,"project":"yes","unknown":true}')).toEqual({ member: true });
  });

  it("falls back to empty overrides for invalid input", () => {
    expect(parseGroupOverrides(null)).toEqual({});
    expect(parseGroupOverrides("")).toEqual({});
    expect(parseGroupOverrides("{not json")).toEqual({});
    expect(parseGroupOverrides("[1,2]")).toEqual({});
    expect(parseGroupOverrides('"text"')).toEqual({});
  });

  it("opens only the active group by default", () => {
    expect(resolveGroupOpen("project", "project", {})).toBe(true);
    expect(resolveGroupOpen("member", "project", {})).toBe(false);
  });

  it("lets manual overrides win over the active group", () => {
    expect(resolveGroupOpen("project", "project", { project: false })).toBe(false);
    expect(resolveGroupOpen("member", "project", { member: true })).toBe(true);
  });

  it("sets a single override immutably", () => {
    const before = { member: true };
    const after = toggleGroupOverrides(before, "project", true);
    expect(after).toEqual({ member: true, project: true });
    expect(before).toEqual({ member: true });
    expect(toggleGroupOverrides(after, "member", false)).toEqual({ member: false, project: true });
  });

  it("roundtrips through serialization", () => {
    const overrides = { project: false, member: true };
    expect(parseGroupOverrides(serializeGroupOverrides(overrides))).toEqual(overrides);
  });

  it("exposes a stable storage key", () => {
    expect(DOCS_SIDEBAR_GROUPS_KEY).toBe("plane-docs-sidebar-groups");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test -- docs-sidebar-state`
Expected: FAIL — gagal resolve import `./docs-sidebar-state` (module belum ada).

- [ ] **Step 3: Write minimal implementation**

Buat `apps/web/core/components/docs/docs-sidebar-state.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DOC_TOPIC_GROUPS, type TDocGroupKey } from "./registry";

export const DOCS_SIDEBAR_GROUPS_KEY = "plane-docs-sidebar-groups";

export type TGroupOverrides = Partial<Record<TDocGroupKey, boolean>>;

export function parseGroupOverrides(raw: string | null): TGroupOverrides {
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    const overrides: TGroupOverrides = {};
    for (const group of DOC_TOPIC_GROUPS) {
      const value = (parsed as Record<string, unknown>)[group.key];
      if (typeof value === "boolean") overrides[group.key] = value;
    }
    return overrides;
  } catch {
    return {};
  }
}

export function serializeGroupOverrides(overrides: TGroupOverrides): string {
  return JSON.stringify(overrides);
}

export function resolveGroupOpen(group: TDocGroupKey, activeGroup: TDocGroupKey, overrides: TGroupOverrides): boolean {
  return overrides[group] ?? group === activeGroup;
}

export function toggleGroupOverrides(overrides: TGroupOverrides, group: TDocGroupKey, open: boolean): TGroupOverrides {
  return { ...overrides, [group]: open };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test -- docs-sidebar-state`
Expected: PASS (8 test).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/docs/docs-sidebar-state.ts apps/web/core/components/docs/docs-sidebar-state.test.ts
git commit -m "feat(web): docs sidebar group state helper"
```

---

### Task 2: Sidebar docs collapsible

**Files:**

- Create: `apps/web/core/components/docs/use-docs-sidebar-groups.ts`
- Modify: `apps/web/core/components/docs/docs-sidebar.tsx` (seluruh isi file)

- [ ] **Step 1: Write the hook**

Buat `apps/web/core/components/docs/use-docs-sidebar-groups.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useMemo, useState } from "react";
import { useParams } from "next/navigation";
import {
  DOCS_SIDEBAR_GROUPS_KEY,
  parseGroupOverrides,
  resolveGroupOpen,
  serializeGroupOverrides,
  toggleGroupOverrides,
  type TGroupOverrides,
} from "./docs-sidebar-state";
import { DEFAULT_DOC_TOPIC_SLUG, getTopicBySlug, type TDocGroupKey } from "./registry";

const persistOverrides = (overrides: TGroupOverrides) => {
  try {
    window.localStorage.setItem(DOCS_SIDEBAR_GROUPS_KEY, serializeGroupOverrides(overrides));
  } catch {
    return;
  }
};

const readOverrides = (): TGroupOverrides => {
  if (typeof window === "undefined") return {};
  try {
    return parseGroupOverrides(window.localStorage.getItem(DOCS_SIDEBAR_GROUPS_KEY));
  } catch {
    return {};
  }
};

export function useDocsSidebarGroups() {
  const { topicSlug } = useParams();
  const activeSlug = topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG;
  const activeGroup = useMemo<TDocGroupKey>(() => getTopicBySlug(activeSlug)?.group ?? "getting-started", [activeSlug]);

  const [overrides, setOverrides] = useState<TGroupOverrides>(readOverrides);

  const isGroupOpen = useCallback(
    (group: TDocGroupKey) => resolveGroupOpen(group, activeGroup, overrides),
    [activeGroup, overrides]
  );

  const toggleGroup = useCallback(
    (group: TDocGroupKey) => {
      setOverrides((previous) => {
        const next = toggleGroupOverrides(previous, group, !resolveGroupOpen(group, activeGroup, previous));
        persistOverrides(next);
        return next;
      });
    },
    [activeGroup]
  );

  return { isGroupOpen, toggleGroup };
}
```

- [ ] **Step 2: Rewrite `docs-sidebar.tsx`**

Ganti seluruh isi `apps/web/core/components/docs/docs-sidebar.tsx` dengan:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { Collapsible } from "@plane/ui";
import { cn } from "@plane/utils";
import { DEFAULT_DOC_TOPIC_SLUG, DOC_TOPIC_GROUPS, DOC_TOPICS } from "./registry";
import { useDocsSidebarGroups } from "./use-docs-sidebar-groups";

type Props = {
  isMobile?: boolean;
  onNavigate?: () => void;
};

export function DocsSidebar({ isMobile = false, onNavigate }: Props) {
  const { workspaceSlug, topicSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const activeSlug = topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG;
  const { isGroupOpen, toggleGroup } = useDocsSidebarGroups();

  return (
    <nav
      className={cn(
        "h-full w-64 flex-col overflow-y-auto px-3 py-4",
        isMobile ? "flex w-full" : "hidden border-r border-subtle md:flex"
      )}
    >
      {DOC_TOPIC_GROUPS.map((group) => {
        const isOpen = isGroupOpen(group.key);
        return (
          <Collapsible
            key={group.key}
            className="mb-4 last:mb-0"
            buttonClassName="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 hover:bg-layer-transparent-hover"
            isOpen={isOpen}
            onToggle={() => toggleGroup(group.key)}
            title={
              <>
                <span className="tracking-wider text-11 font-semibold text-placeholder uppercase">{group.label}</span>
                <ChevronDownOutline
                  className={cn(
                    "size-3.5 flex-shrink-0 text-placeholder transition-transform",
                    !isOpen && "-rotate-90"
                  )}
                />
              </>
            }
          >
            <div className="mt-1 flex flex-col gap-0.5">
              {DOC_TOPICS.filter((topic) => topic.group === group.key).map((topic) => {
                const Icon = topic.icon;
                const isActive = topic.slug === activeSlug;
                return (
                  <Link
                    key={topic.slug}
                    href={`/${slug}/docs/${topic.slug}`}
                    onClick={onNavigate}
                    className={cn(
                      "flex items-center gap-2 rounded-md px-2 py-1.5",
                      isActive ? "bg-layer-1 text-primary" : "text-secondary hover:bg-layer-transparent-hover"
                    )}
                  >
                    <Icon className="size-4 flex-shrink-0" />
                    <span className="text-13 font-medium whitespace-nowrap">{topic.title}</span>
                  </Link>
                );
              })}
            </div>
          </Collapsible>
        );
      })}
    </nav>
  );
}
```

- [ ] **Step 3: Verify typecheck + tests**

Run: `pnpm --filter=web check:types`
Expected: exit 0, tanpa error.

Run: `pnpm --filter=web test`
Expected: PASS (termasuk seluruh test docs existing).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/docs/use-docs-sidebar-groups.ts apps/web/core/components/docs/docs-sidebar.tsx
git commit -m "feat(web): collapsible docs sidebar groups"
```

---

### Task 3: Primitif `DocsPracticeMatrix`

**Files:**

- Modify: `apps/web/core/components/docs/blocks.tsx` (sisipkan setelah komponen `DocsTermTable`, sebelum `DocsTopicLink`)

- [ ] **Step 1: Tambahkan tipe & komponen**

Sisipkan kode berikut ke `apps/web/core/components/docs/blocks.tsx` di antara `DocsTermTable` dan `DocsTopicLink`:

```tsx
export type TPracticeUsage = "primary" | "secondary" | "none";

const PRACTICE_USAGE_STYLES: Record<TPracticeUsage, { symbol: string; className: string; label: string }> = {
  primary: { symbol: "●", className: "text-accent-primary", label: "sering dipakai" },
  secondary: { symbol: "○", className: "text-tertiary", label: "kadang dipakai" },
  none: { symbol: "–", className: "text-placeholder", label: "tidak dipakai" },
};

export function DocsPracticeMatrix({
  columns,
  rows,
  legend = "● sering dipakai · ○ kadang dipakai · – tidak dipakai",
}: {
  columns: string[];
  rows: { practice: string; cells: TPracticeUsage[] }[];
  legend?: string;
}) {
  return (
    <div className="mt-4">
      <div className="overflow-x-auto rounded-lg border border-subtle">
        <table className="w-full border-collapse text-left">
          <thead>
            <tr className="bg-layer-1">
              <th className="tracking-wider border-b border-subtle px-4 py-2 text-11 font-semibold text-tertiary uppercase">
                Praktik
              </th>
              {columns.map((column) => (
                <th
                  key={column}
                  className="tracking-wider border-b border-subtle px-4 py-2 text-center text-11 font-semibold text-tertiary uppercase"
                >
                  {column}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.practice} className="border-b border-subtle last:border-b-0">
                <th scope="row" className="px-4 py-2.5 text-left align-top text-13 font-medium text-primary">
                  {row.practice}
                </th>
                {columns.map((column, index) => {
                  const usage = PRACTICE_USAGE_STYLES[row.cells[index] ?? "none"];
                  return (
                    <td key={column} className="px-4 py-2.5 text-center align-top">
                      <span aria-hidden className={cn("text-13", usage.className)}>
                        {usage.symbol}
                      </span>
                      <span className="sr-only">{usage.label}</span>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-2 text-11 text-tertiary">{legend}</p>
    </div>
  );
}
```

- [ ] **Step 2: Verify typecheck**

Run: `pnpm --filter=web check:types`
Expected: exit 0, tanpa error.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/docs/blocks.tsx
git commit -m "feat(web): docs practice matrix block"
```

---

### Task 4: Grup `itil4` + topik `itil4-overview` + data matrix

**Files:**

- Create: `apps/web/core/components/docs/content/itil4-matrix-data.ts`
- Create: `apps/web/core/components/docs/content/itil4-overview.tsx`
- Modify: `apps/web/core/components/docs/registry.ts`

- [ ] **Step 1: Buat data matrix**

Buat `apps/web/core/components/docs/content/itil4-matrix-data.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TPracticeUsage } from "../blocks";

export const ITIL4_MATRIX_COLUMNS = ["Intake", "Queue", "Service", "Sprint", "Track", "KB", "War Room", "Reports"];

export const ITIL4_MATRIX_ROWS: { practice: string; cells: TPracticeUsage[] }[] = [
  {
    practice: "Incident",
    cells: ["primary", "primary", "primary", "secondary", "none", "primary", "primary", "secondary"],
  },
  {
    practice: "Problem",
    cells: ["secondary", "primary", "primary", "secondary", "primary", "primary", "none", "secondary"],
  },
  {
    practice: "Service request",
    cells: ["primary", "primary", "primary", "primary", "none", "secondary", "none", "secondary"],
  },
  {
    practice: "Change",
    cells: ["secondary", "primary", "primary", "primary", "primary", "secondary", "none", "secondary"],
  },
  {
    practice: "Improvement",
    cells: ["secondary", "primary", "secondary", "primary", "primary", "secondary", "none", "primary"],
  },
];
```

- [ ] **Step 2: Buat halaman `itil4-overview`**

Buat `apps/web/core/components/docs/content/itil4-overview.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsParagraph,
  DocsPracticeMatrix,
  DocsSectionHeading,
  DocsTermTable,
  DocsTopicLink,
} from "../blocks";
import { ITIL4_MATRIX_COLUMNS, ITIL4_MATRIX_ROWS } from "./itil4-matrix-data";

export default function Itil4OverviewContent() {
  return (
    <>
      <DocsParagraph>
        ITIL4 adalah kerangka kerja pengelolaan layanan TI yang menekankan alur nilai: dari permintaan atau gangguan,
        sampai layanan pulih dan pengetahuan bertambah. Platform ini menerjemahkan praktik-praktik ITIL4 menjadi fitur
        yang bisa langsung dipakai tim support dan operasional.
      </DocsParagraph>

      <DocsSectionHeading>Peta praktik ke fitur</DocsSectionHeading>
      <DocsParagraph>
        Lima praktik utama ITIL4 dan fitur platform yang mendukungnya. Tanda ● berarti sering dipakai, ○ kadang dipakai,
        dan – tidak dipakai.
      </DocsParagraph>
      <DocsPracticeMatrix columns={ITIL4_MATRIX_COLUMNS} rows={ITIL4_MATRIX_ROWS} />

      <DocsSectionHeading>Istilah ITIL4 ↔ istilah di app</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "Incident", meaning: "Gangguan layanan yang perlu dipulihkan — jenis ticket Incident di project." },
          { term: "Problem", meaning: "Akar masalah di balik insiden berulang — jenis ticket Problem." },
          {
            term: "Known error",
            meaning: "Problem yang akarnya sudah diketahui — artikel Knowledge base yang ditautkan ke ticket.",
          },
          {
            term: "Service request",
            meaning: "Permintaan standar dari pengguna — alur Intake dengan jenis ticket Request.",
          },
          {
            term: "Change",
            meaning: "Perubahan terencana pada layanan — jenis ticket Change, direncanakan lewat Sprint dan Track.",
          },
          {
            term: "Continual improvement",
            meaning: "Perbaikan berkelanjutan — jenis ticket Improvement, dikelola lewat Track.",
          },
          { term: "Queue", meaning: "Antrean kerja tersaring: daftar ticket dengan filter tersimpan (view)." },
        ]}
      />

      <DocsSectionHeading>Mulai dari mana</DocsSectionHeading>
      <DocsParagraph>
        Setiap praktik punya halaman sendiri dengan alur langkah, peran, dan fitur yang dipakai. Mulailah dari{" "}
        <DocsTopicLink slug="itil4-incident">Incident management</DocsTopicLink> karena alur itulah yang paling sering
        dijalankan, lalu pelajari <DocsTopicLink slug="itil4-problem">Problem management</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-service-request">Service request</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-change-improvement">Change &amp; improvement</DocsTopicLink>, dan terakhir{" "}
        <DocsTopicLink slug="itil4-flow">alur organisasi end-to-end</DocsTopicLink>.
      </DocsParagraph>

      <DocsCallout variant="tip" title="Mulai kecil">
        Tidak perlu mengaktifkan semua praktik sekaligus. Mulai dari satu project, satu layanan, dan satu queue — tambah
        praktik lain saat tim sudah nyaman.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 3: Daftarkan grup & topik di registry**

Di `apps/web/core/components/docs/registry.ts`:

1. Tambahkan `ConnectionsOutline` ke daftar impor ikon dari `@makeplane/propel/icons` (letakkan sesuai urutan alfabetis, setelah `CalendarOutline`).
2. Ubah union key grup:

```ts
export type TDocGroupKey = "getting-started" | "member" | "project" | "itil4" | "settings";
```

3. Sisipkan grup baru setelah entri `project`, sebelum `settings`:

```ts
export const DOC_TOPIC_GROUPS: { key: TDocGroupKey; label: string }[] = [
  { key: "getting-started", label: "Memulai" },
  { key: "member", label: "Untuk Member" },
  { key: "project", label: "Untuk Project" },
  { key: "itil4", label: "ITIL4" },
  { key: "settings", label: "Pengaturan" },
];
```

4. Sisipkan topik baru ke `DOC_TOPICS` **setelah entri `intake`** dan **sebelum `project-settings`**:

```ts
  {
    slug: "itil4-overview",
    title: "ITIL4 di platform ini",
    description: "Peta praktik ITIL4 ke fitur platform dan bagaimana semuanya saling terhubung.",
    group: "itil4",
    icon: ConnectionsOutline,
    component: lazy(() => import("./content/itil4-overview")),
  },
```

- [ ] **Step 4: Verify**

Run: `pnpm --filter=web test`
Expected: PASS — `registry.test.ts` memvalidasi slug unik, grup valid (termasuk `itil4`), dan tepi prev/next.

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/docs/content/itil4-matrix-data.ts apps/web/core/components/docs/content/itil4-overview.tsx apps/web/core/components/docs/registry.ts
git commit -m "feat(web): itil4 docs group with overview page"
```

---

### Task 5: Topik `itil4-incident` & `itil4-problem`

**Files:**

- Create: `apps/web/core/components/docs/content/itil4-incident.tsx`
- Create: `apps/web/core/components/docs/content/itil4-problem.tsx`
- Modify: `apps/web/core/components/docs/registry.ts`

- [ ] **Step 1: Buat halaman `itil4-incident`**

Buat `apps/web/core/components/docs/content/itil4-incident.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsFeatureCard,
  DocsFeatureGrid,
  DocsParagraph,
  DocsSectionHeading,
  DocsStepList,
  DocsTermTable,
  DocsTopicLink,
} from "../blocks";

export default function Itil4IncidentContent() {
  return (
    <>
      <DocsParagraph>
        Incident management bertujuan memulihkan layanan secepat mungkin dan mencatat pengetahuan dari setiap gangguan.
        Targetnya bukan mencari siapa yang salah, melainkan meminimalkan dampak dan mencegah terulang.
      </DocsParagraph>

      <DocsSectionHeading>Di platform ini</DocsSectionHeading>
      <DocsFeatureGrid>
        <DocsFeatureCard title="Intake">
          Pintu masuk laporan: form, email, webhook, dan alert otomatis dari monitoring.
        </DocsFeatureCard>
        <DocsFeatureCard title="Tickets & Services">
          Jenis ticket Incident, prioritas, dan tautan ke layanan terdampak.
        </DocsFeatureCard>
        <DocsFeatureCard title="War Rooms">
          Ruang koordinasi saat major incident: peran, runbook, chat, dan timeline.
        </DocsFeatureCard>
        <DocsFeatureCard title="Queues & Sprints">
          Antrean per layanan untuk triage, dan sprint bila perbaikannya perlu direncanakan.
        </DocsFeatureCard>
      </DocsFeatureGrid>

      <DocsSectionHeading>Alur insiden</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Laporan masuk lewat Intake",
            description:
              "Pengguna melapor lewat form atau email, atau alert monitoring masuk otomatis lewat Intake source. Alert dengan pola sama digabung (occurrence count bertambah) alih-alih membuat ticket baru.",
          },
          {
            title: "Triage: klasifikasi, prioritas, layanan",
            description:
              "Tentukan jenis ticket Incident, prioritas (urgent sampai low), dan tautkan Service yang terdampak. Untuk jenis Incident, tautan layanan wajib ada saat accept.",
          },
          {
            title: "Kerjakan dari queue",
            description:
              "Ticket masuk queue tim (misalnya per layanan). Gunakan relasi duplicate atau blocking untuk menandai kaitan antar-insiden.",
          },
          {
            title: "Major incident: nyalakan War Room",
            description:
              "Untuk gangguan berdampak besar, aktifkan War Room pada ticket: severity diturunkan dari prioritas, runbook otomatis disiapkan, dan peran commander/comms/scribe/responder diisi.",
          },
          {
            title: "Resolve dan dokumentasikan",
            description:
              "Catat langkah pemulihan di komentar, buat atau tautkan artikel Knowledge base, lalu tandai ticket selesai.",
          },
          {
            title: "Review dan tindak lanjut",
            description: "Kalau akar masalah belum diketahui, buat ticket Problem dan hubungkan ke insiden terkait.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Helpdesk L1",
            meaning: "Memantau queue triage, mengklasifikasi laporan, dan menyelesaikan insiden sederhana.",
          },
          { term: "Helpdesk L2 / SME", meaning: "Menangani insiden yang butuh keahlian layanan tertentu." },
          {
            term: "Incident commander",
            meaning: "Memimpin War Room saat major incident; perannya diatur di dalam War Room.",
          },
          {
            term: "Problem manager",
            meaning: "Menindaklanjuti insiden berulang sebagai problem (lihat Problem management).",
          },
        ]}
      />

      <DocsCallout variant="warning" title="Kesehatan service belum monitoring nyata">
        Indikator di halaman Services saat ini bersifat katalog (belum terhubung monitoring real-time). Jangan
        mengandalkannya sebagai satu-satunya alarm — sambungkan alert monitoring lewat Intake source.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="intake">Intake</DocsTopicLink>, <DocsTopicLink slug="war-rooms">War Rooms</DocsTopicLink>,{" "}
        <DocsTopicLink slug="queues">Queues</DocsTopicLink>, <DocsTopicLink slug="services">Services</DocsTopicLink>,
        dan <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 2: Buat halaman `itil4-problem`**

Buat `apps/web/core/components/docs/content/itil4-problem.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsFeatureCard,
  DocsFeatureGrid,
  DocsParagraph,
  DocsSectionHeading,
  DocsStepList,
  DocsTermTable,
  DocsTopicLink,
} from "../blocks";

export default function Itil4ProblemContent() {
  return (
    <>
      <DocsParagraph>
        Problem management mencari akar masalah di balik insiden yang berulang atau berdampak besar. Tujuannya bukan
        memadamkan kebakaran, melainkan menghilangkan sumbernya — lalu mengubah temuannya menjadi pengetahuan tim.
      </DocsParagraph>

      <DocsSectionHeading>Di platform ini</DocsSectionHeading>
      <DocsFeatureGrid>
        <DocsFeatureCard title="Tickets">
          Jenis ticket Problem, dengan relasi ke setiap insiden yang terkait.
        </DocsFeatureCard>
        <DocsFeatureCard title="Tracks">
          Investigasi akar masalah sebagai track tersendiri dengan langkah-langkahnya.
        </DocsFeatureCard>
        <DocsFeatureCard title="Knowledge base">
          Artikel known error: gejala, akar masalah, dan workaround.
        </DocsFeatureCard>
        <DocsFeatureCard title="Queues">
          Antrean masalah yang sedang diselidiki dan yang menunggu perbaikan.
        </DocsFeatureCard>
      </DocsFeatureGrid>

      <DocsSectionHeading>Alur problem</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Deteksi pola",
            description:
              "Tandai insiden yang saling terkait lewat relasi duplicate atau relates to, atau simpan queue khusus untuk insiden dengan gejala sama.",
          },
          {
            title: "Buat ticket Problem",
            description:
              "Dokumentasikan gejala yang berulang, dampak, dan insiden yang sudah terjadi. Hubungkan ticket Problem ke insiden-insiden sumbernya.",
          },
          {
            title: "Investigasi akar masalah",
            description:
              'Kelola investigasi sebagai Track (misalnya "Investigasi gangguan login") dan pecah langkah penyelidikan menjadi sub-items yang bisa ditugaskan.',
          },
          {
            title: "Catat known error",
            description:
              "Begitu akar masalah diketahui, tulis artikel di Knowledge base dan tautkan ke ticket Problem agar tim punya satu rujukan.",
          },
          {
            title: "Rencanakan perbaikan permanen",
            description:
              "Buat ticket Improvement — atau Change, jika mengubah layanan — dan jadwalkan pengerjaannya lewat Sprint.",
          },
          {
            title: "Tutup problem",
            description:
              "Tandai problem selesai setelah perbaikan terpasang dan tidak ada insiden serupa yang terulang.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Problem manager",
            meaning:
              "Member yang ditunjuk memiliki queue problem, memimpin investigasi, dan memutuskan kapan problem ditutup.",
          },
          { term: "SME / tim layanan", meaning: "Membantu analisis akar masalah di area keahliannya." },
          { term: "Ritme", meaning: "Review queue problem setiap minggu; tinjau problem terbuka di awal sprint." },
        ]}
      />

      <DocsCallout variant="info" title="Tidak semua insiden perlu RCA">
        Prioritaskan problem untuk insiden yang berulang, berdampak besar, atau tidak punya workaround. Insiden sekali
        lewat cukup diselesaikan sebagai incident.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="tracks">Tracks</DocsTopicLink>,{" "}
        <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>,{" "}
        <DocsTopicLink slug="services">Services</DocsTopicLink>, dan{" "}
        <DocsTopicLink slug="itil4-change-improvement">Change &amp; improvement</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 3: Daftarkan kedua topik**

Di `apps/web/core/components/docs/registry.ts`:

1. Tambahkan `RelationsOutline` dan `WarningCircleOutline` ke impor ikon (urutan alfabetis).
2. Sisipkan ke `DOC_TOPICS` setelah entri `itil4-overview`, sebelum `project-settings`:

```ts
  {
    slug: "itil4-incident",
    title: "Incident management",
    description: "Alur insiden dari laporan masuk sampai pemulihan, termasuk major incident.",
    group: "itil4",
    icon: WarningCircleOutline,
    component: lazy(() => import("./content/itil4-incident")),
  },
  {
    slug: "itil4-problem",
    title: "Problem management",
    description: "Menemukan akar masalah dari insiden berulang dan mengubahnya jadi pengetahuan.",
    group: "itil4",
    icon: RelationsOutline,
    component: lazy(() => import("./content/itil4-problem")),
  },
```

- [ ] **Step 4: Verify**

Run: `pnpm --filter=web test`
Expected: PASS.

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/docs/content/itil4-incident.tsx apps/web/core/components/docs/content/itil4-problem.tsx apps/web/core/components/docs/registry.ts
git commit -m "feat(web): itil4 incident and problem docs"
```

---

### Task 6: Topik `itil4-service-request` & `itil4-change-improvement`

**Files:**

- Create: `apps/web/core/components/docs/content/itil4-service-request.tsx`
- Create: `apps/web/core/components/docs/content/itil4-change-improvement.tsx`
- Modify: `apps/web/core/components/docs/registry.ts`

- [ ] **Step 1: Buat halaman `itil4-service-request`**

Buat `apps/web/core/components/docs/content/itil4-service-request.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTermTable, DocsTopicLink } from "../blocks";

export default function Itil4ServiceRequestContent() {
  return (
    <>
      <DocsParagraph>
        Service request adalah permintaan standar dari pengguna — akses, perangkat, informasi, atau layanan lain yang
        alurnya sudah diketahui. Praktik ini membuat permintaan seperti itu konsisten, cepat, dan tercatat.
      </DocsParagraph>

      <DocsSectionHeading>Siapkan jenis ticket Request</DocsSectionHeading>
      <DocsParagraph>
        Jenis ticket bawaan tidak memuat "Request" — buat sekali di pengaturan workspace, lalu aktifkan di project yang
        membutuhkannya.
      </DocsParagraph>
      <DocsStepList
        steps={[
          {
            title: "Buat jenis ticket",
            description:
              'Buka pengaturan workspace → Work item types, tambah jenis baru bernama "Request", dan aktifkan switch "requires service" agar setiap permintaan wajib menunjuk layanan.',
          },
          {
            title: "Aktifkan di project",
            description: 'Buka pengaturan project → Work item types, lalu enable jenis "Request" yang baru dibuat.',
          },
          {
            title: "Sambungkan pintu masuk",
            description:
              "Aktifkan Intake di project dan tambahkan channel: form untuk pengguna, email, atau webhook/intake source.",
          },
        ]}
      />

      <DocsSectionHeading>Alur permintaan</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Permintaan masuk",
            description: "Permintaan datang lewat Intake dan menunggu di antrean triage.",
          },
          {
            title: "Triase dan accept",
            description:
              "Pilih jenis Request dan tautkan Service yang relevan — accept memindahkan ticket ke state awal project.",
          },
          {
            title: "Fulfillment",
            description:
              "Kerjakan dari queue per layanan. Untuk permintaan berulang (onboarding, pengadaan), simpan checklist sebagai sub-items.",
          },
          {
            title: "Selesai dan dokumentasikan",
            description:
              "Tandai ticket selesai setelah pengguna terlayani; jika pertanyaannya berulang, tulis artikel di Knowledge base.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Helpdesk L1",
            meaning: "Menjaga pintu masuk, memverifikasi permintaan, dan menuntaskan permintaan standar.",
          },
          { term: "Service owner", meaning: "Pemilik layanan yang menerima eskalasi permintaan khusus." },
          {
            term: "Ritme",
            meaning: "Permintaan terjadwal (misalnya onboarding batch) direncanakan dalam Sprint.",
          },
        ]}
      />

      <DocsCallout variant="warning" title="Jangan aktifkan requires service sembarangan">
        Switch "requires service" memaksa setiap ticket jenis itu menunjuk layanan. Pasang hanya untuk jenis yang memang
        terikat layanan, bukan untuk tugas internal biasa.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="intake">Intake</DocsTopicLink>,{" "}
        <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>, <DocsTopicLink slug="services">Services</DocsTopicLink>,{" "}
        <DocsTopicLink slug="queues">Queues</DocsTopicLink>, <DocsTopicLink slug="sprints">Sprints</DocsTopicLink>, dan{" "}
        <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 2: Buat halaman `itil4-change-improvement`**

Buat `apps/web/core/components/docs/content/itil4-change-improvement.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTermTable, DocsTopicLink } from "../blocks";

export default function Itil4ChangeImprovementContent() {
  return (
    <>
      <DocsParagraph>
        Dua praktik yang saling melengkapi: change enablement memastikan perubahan pada layanan direncanakan dan
        dikomunikasikan, sedangkan continual improvement memastikan organisasi belajar dan memperbaiki diri.
      </DocsParagraph>

      <DocsSectionHeading>Change enablement</DocsSectionHeading>
      <DocsParagraph>
        Setiap perubahan pada layanan — upgrade, perubahan konfigurasi, penambahan akses — dicatat sebagai ticket Change
        supaya ada jejak, rencana, dan titik koordinasi.
      </DocsParagraph>
      <DocsStepList
        steps={[
          {
            title: "Buat ticket Change",
            description: "Tulis apa yang berubah, mengapa, dan layanan mana yang terdampak (tautkan Service).",
          },
          {
            title: "Nilai risiko",
            description:
              "Pakai prioritas dan deskripsi: rencana langkah, cara rollback, dan jendela waktu yang diinginkan.",
          },
          {
            title: "Rencanakan timing",
            description: "Kelompokkan perubahan terkait dalam Track, lalu jadwalkan eksekusinya lewat Sprint.",
          },
          {
            title: "Komunikasikan dan eksekusi",
            description: "Koordinasi lewat komentar ticket; kalau perubahan memicu insiden, hubungkan lewat relasi.",
          },
          {
            title: "Catat hasil",
            description: "Setelah selesai, catat hasilnya di ticket — termasuk perbedaan dari rencana semula.",
          },
        ]}
      />

      <DocsSectionHeading>Continual improvement</DocsSectionHeading>
      <DocsParagraph>
        Temuan dari review insiden, problem, dan retrospektif sprint tidak berhenti sebagai catatan — jadikan ticket
        Improvement yang dikerjakan seperti pekerjaan lain.
      </DocsParagraph>
      <DocsStepList
        steps={[
          {
            title: "Kumpulkan temuan",
            description: "Sumbernya bisa review problem, checklist pasca-insiden, atau retrospektif Sprint.",
          },
          {
            title: "Buat ticket Improvement",
            description: "Rumuskan perbaikan sekonkret mungkin: kondisi sekarang, target, dan ukurannya.",
          },
          {
            title: "Kelola dalam Track",
            description: "Kelompokkan improvement terkait sebagai satu inisiatif agar progresnya terlihat.",
          },
          {
            title: "Kerjakan dalam Sprint",
            description: "Sisihkan porsi kapasitas tiap sprint untuk improvement — jangan menunggu waktu luang.",
          },
          {
            title: "Ukur dan lanjutkan",
            description: "Tinjau tren di Reports; tetapkan improvement berikutnya dari hasil pengukuran.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Change owner",
            meaning: "Orang yang mengajukan dan bertanggung jawab atas eksekusi sebuah perubahan.",
          },
          {
            term: "Improvement lead",
            meaning: "Member yang menjaga backlog improvement tetap hidup dan terukur.",
          },
          {
            term: "Ritme",
            meaning: "Tinjau backlog improvement di awal sprint; review perubahan yang selesai di akhir sprint.",
          },
        ]}
      />

      <DocsCallout variant="info" title="Belum ada board persetujuan (CAB)">
        Platform ini tidak punya alur approval formal untuk perubahan. Sepakati bersama tim siapa yang boleh menyetujui
        apa, lalu catat persetujuannya di komentar ticket.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="sprints">Sprints</DocsTopicLink>, <DocsTopicLink slug="tracks">Tracks</DocsTopicLink>,{" "}
        <DocsTopicLink slug="services">Services</DocsTopicLink>, dan{" "}
        <DocsTopicLink slug="reports">Reports</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 3: Daftarkan kedua topik**

Di `apps/web/core/components/docs/registry.ts`:

1. Tambahkan `MessageSupportOutline` dan `UpgradeOutline` ke impor ikon (urutan alfabetis).
2. Sisipkan ke `DOC_TOPICS` setelah entri `itil4-problem`, sebelum `project-settings`:

```ts
  {
    slug: "itil4-service-request",
    title: "Service request",
    description: "Melayani permintaan standar lewat Intake dan katalog layanan.",
    group: "itil4",
    icon: MessageSupportOutline,
    component: lazy(() => import("./content/itil4-service-request")),
  },
  {
    slug: "itil4-change-improvement",
    title: "Change & improvement",
    description: "Mengelola perubahan secara terencana dan perbaikan berkelanjutan.",
    group: "itil4",
    icon: UpgradeOutline,
    component: lazy(() => import("./content/itil4-change-improvement")),
  },
```

- [ ] **Step 4: Verify**

Run: `pnpm --filter=web test`
Expected: PASS.

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/docs/content/itil4-service-request.tsx apps/web/core/components/docs/content/itil4-change-improvement.tsx apps/web/core/components/docs/registry.ts
git commit -m "feat(web): itil4 service request and change improvement docs"
```

---

### Task 7: Topik `itil4-flow` (alur organisasi end-to-end)

**Files:**

- Create: `apps/web/core/components/docs/content/itil4-flow.tsx`
- Modify: `apps/web/core/components/docs/registry.ts`

- [ ] **Step 1: Buat halaman `itil4-flow`**

Buat `apps/web/core/components/docs/content/itil4-flow.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsParagraph,
  DocsPracticeMatrix,
  DocsSectionHeading,
  DocsStepList,
  DocsTopicLink,
} from "../blocks";
import { ITIL4_MATRIX_COLUMNS, ITIL4_MATRIX_ROWS } from "./itil4-matrix-data";

export default function Itil4FlowContent() {
  return (
    <>
      <DocsParagraph>
        Halaman ini merangkum contoh penerapan dari nol untuk satu organisasi — misalnya perusahaan dengan tiga layanan
        internal: email, aplikasi keuangan, dan jaringan kantor.
      </DocsParagraph>

      <DocsSectionHeading>Langkah penerapan</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Tentukan struktur",
            description:
              "Satu workspace untuk organisasi; satu project per layanan besar, atau satu project operasional dengan Service catalog untuk rinciannya.",
          },
          {
            title: "Bangun service catalog",
            description:
              "Daftarkan setiap layanan dengan owner dan criticality; catat dependency antar-layanan supaya dampak gangguan terlihat.",
          },
          {
            title: "Siapkan jenis ticket",
            description:
              "Aktifkan Incident, Problem, Change, dan Improvement di project; buat jenis Request dengan requires service untuk permintaan standar.",
          },
          {
            title: "Buka pintu masuk",
            description: "Aktifkan Intake; sediakan form untuk pengguna dan intake source untuk alert monitoring.",
          },
          {
            title: "Susun queue standar",
            description:
              "Buat queue: Triage, Incident aktif, Problem, Request per layanan, dan Change mendatang — jadikan public agar dipakai seluruh tim.",
          },
          {
            title: "Tetapkan ritme",
            description:
              "Sprint dua minggu untuk pekerjaan terjadwal dan improvement; Track untuk investigasi problem dan inisiatif besar; War Room saat major incident.",
          },
          {
            title: "Tutup loop pengetahuan",
            description:
              "Setelah insiden atau problem selesai, perbarui Knowledge base dan tautkan artikelnya ke ticket.",
          },
          {
            title: "Perbaiki berulang",
            description: "Jadikan temuan review sebagai ticket Improvement; pantau tren di Reports setiap bulan.",
          },
        ]}
      />

      <DocsSectionHeading>Peta praktik ke fitur (ringkasan)</DocsSectionHeading>
      <DocsPracticeMatrix columns={ITIL4_MATRIX_COLUMNS} rows={ITIL4_MATRIX_ROWS} />

      <DocsCallout variant="tip" title="Mulai kecil, ulangi">
        Satu layanan, satu queue, satu sprint sudah cukup untuk mulai. Tambah praktik dan otomasi setelah alur dasarnya
        jalan — dokumentasi dan kebiasaan lebih penting daripada konfigurasi lengkap di hari pertama.
      </DocsCallout>

      <DocsParagraph>
        Baca ulang detail tiap praktik: <DocsTopicLink slug="itil4-overview">ITIL4 di platform ini</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-incident">Incident</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-problem">Problem</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-service-request">Service request</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-change-improvement">Change &amp; improvement</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 2: Daftarkan topik terakhir**

Di `apps/web/core/components/docs/registry.ts`:

1. Tambahkan `WorkflowsOutline` ke impor ikon (urutan alfabetis, sebelum `WorkItemsOutline`).
2. Sisipkan ke `DOC_TOPICS` setelah entri `itil4-change-improvement`, sebelum `project-settings`:

```ts
  {
    slug: "itil4-flow",
    title: "Alur organisasi end-to-end",
    description: "Contoh penerapan dari nol: struktur, ritual, dan siklus kerja organisasi.",
    group: "itil4",
    icon: WorkflowsOutline,
    component: lazy(() => import("./content/itil4-flow")),
  },
```

- [ ] **Step 3: Verify**

Run: `pnpm --filter=web test`
Expected: PASS.

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/docs/content/itil4-flow.tsx apps/web/core/components/docs/registry.ts
git commit -m "feat(web): itil4 organization flow docs"
```

---

### Task 8: Verifikasi menyeluruh

**Files:** tidak ada file baru; hanya verifikasi (dan perbaikan bila ada temuan).

- [ ] **Step 1: Unit test**

Run: `pnpm --filter=web test`
Expected: seluruh test PASS, termasuk `docs-sidebar-state.test.ts` dan `registry.test.ts`.

- [ ] **Step 2: Typecheck & lint**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

Run: `pnpm --filter=web check:lint`
Expected: exit 0 (tanpa error; warning mengikuti batas existing).

- [ ] **Step 3: Build**

Run: `pnpm --filter=web build`
Expected: build sukses.

- [ ] **Step 4: Verifikasi manual di browser**

Ikuti AGENTS.md untuk menampilkan web lokal (hentikan `plane-web-prod.service` lalu jalankan server dev, atau rebuild + restart prod) — jangan pernah menyalakan dua server di port 3000 bersamaan.

Checklist:

1. `/<workspace>/docs/overview` — sidebar menampilkan 5 grup (Memulai, Untuk Member, Untuk Project, ITIL4, Pengaturan); hanya "Memulai" terbuka.
2. Deep-link `/<workspace>/docs/itil4-incident` — grup ITIL4 otomatis terbuka, item Incident tersorot; grup lain tertutup.
3. Klik header grup untuk buka/tutup; reload halaman → pilihan manual tetap berlaku.
4. Tutup grup ITIL4 secara manual, pindah ke topik ITIL4 lain → grup tetap tertutup (keputusan manual menang).
5. Mobile (lebar < `md`): drawer nav membuka; grup bisa dibuka-tutup; memilih topik menutup drawer.
6. Light & dark mode: header grup, chevron, dan matrix terbaca jelas.
7. Prev/next: dari `intake` → `itil4-overview`; dari `itil4-flow` → `project-settings`; dari `project-settings` prev = `itil4-flow`.
8. Halaman ITIL4: matrix tampil dengan legenda; semua `DocsTopicLink` menuju topik yang benar; konten terbaca di layar sempit (matrix bisa di-scroll horizontal).

- [ ] **Step 5: Tanyakan deploy**

Deploy hanya bila user meminta: rebuild web prod + `systemctl --user restart plane-web-prod.service` (per AGENTS.md). Tidak ada perubahan backend.
