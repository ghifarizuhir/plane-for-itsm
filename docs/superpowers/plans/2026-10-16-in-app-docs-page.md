# In-App Documentation Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambahkan halaman dokumentasi in-app berbahasa Indonesia di `/:workspaceSlug/docs/:topicSlug`, dengan sidebar topik, konten per fitur (member & project), dan akses dari sidebar workspace serta Help menu.

**Architecture:** Konten adalah komponen TSX per topik di `apps/web/core/components/docs/content/`; sebuah registry (`registry.ts`) menjadi sumber tunggal daftar topik (slug, judul, grup, ikon, komponen lazy), dipakai oleh sidebar, navigasi prev/next, dan validasi route. Section route baru `(docs)` mengikuti pola `(settings)`: app rail tetap dari parent layout, sidebar workspace disembunyikan, docs punya sidebar sendiri.

**Tech Stack:** React 19, React Router 8 (framework mode, `ssr: false`), MobX (tidak diperlukan), Tailwind 4 tokens, headlessui (`Disclosure`), `@makeplane/propel/icons`, vitest (node env, `core/**/*.test.ts`).

**Spec:** `docs/superpowers/specs/2026-10-16-in-app-docs-page-design.md`

---

### Task 1: Content primitives (`blocks.tsx`)

**Files:**

- Create: `apps/web/core/components/docs/blocks.tsx`

- [ ] **Step 1: Tulis file primitives**

Konten:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { InfoOutline, TickCircleOutline, WarningTriangleOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";

export function DocsSectionHeading({ children }: { children: ReactNode }) {
  return <h2 className="mt-8 mb-3 text-16 font-semibold text-primary">{children}</h2>;
}

export function DocsParagraph({ children }: { children: ReactNode }) {
  return <p className="mt-3 text-13 leading-6 text-secondary">{children}</p>;
}

type TCalloutVariant = "info" | "tip" | "warning";

const CALLOUT_STYLES: Record<TCalloutVariant, { container: string; icon: ReactNode }> = {
  info: {
    container: "border-subtle bg-layer-1",
    icon: <InfoOutline className="size-4 flex-shrink-0 text-tertiary" />,
  },
  tip: {
    container: "border-subtle bg-success-subtle",
    icon: <TickCircleOutline className="size-4 flex-shrink-0 text-success-primary" />,
  },
  warning: {
    container: "border-subtle bg-warning-subtle",
    icon: <WarningTriangleOutline className="size-4 flex-shrink-0 text-warning-primary" />,
  },
};

export function DocsCallout({
  variant = "info",
  title,
  children,
}: {
  variant?: TCalloutVariant;
  title?: string;
  children: ReactNode;
}) {
  const styles = CALLOUT_STYLES[variant];
  return (
    <div className={cn("mt-4 flex gap-3 rounded-lg border px-4 py-3", styles.container)}>
      <div className="mt-0.5">{styles.icon}</div>
      <div>
        {title && <p className="text-13 font-semibold text-primary">{title}</p>}
        <div className="text-13 leading-6 text-secondary">{children}</div>
      </div>
    </div>
  );
}

export function DocsStepList({ steps }: { steps: { title: string; description?: string }[] }) {
  return (
    <ol className="mt-4 flex flex-col gap-3">
      {steps.map((step, index) => (
        <li key={step.title} className="flex gap-3">
          <span className="mt-0.5 flex size-6 flex-shrink-0 items-center justify-center rounded-full bg-layer-1 text-11 font-semibold text-secondary">
            {index + 1}
          </span>
          <div>
            <p className="text-13 font-medium text-primary">{step.title}</p>
            {step.description && <p className="mt-0.5 text-13 leading-6 text-secondary">{step.description}</p>}
          </div>
        </li>
      ))}
    </ol>
  );
}

export function DocsFeatureGrid({ children }: { children: ReactNode }) {
  return <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">{children}</div>;
}

export function DocsFeatureCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="rounded-lg border border-subtle bg-surface-1 p-4">
      <h3 className="text-13 font-semibold text-primary">{title}</h3>
      <div className="mt-1 text-13 leading-6 text-secondary">{children}</div>
    </div>
  );
}

export function DocsTermTable({ terms }: { terms: { term: string; meaning: string }[] }) {
  return (
    <div className="mt-4 overflow-hidden rounded-lg border border-subtle">
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="bg-layer-1">
            <th className="w-1/3 border-b border-subtle px-4 py-2 text-11 font-semibold tracking-wider text-tertiary uppercase">
              Istilah di app
            </th>
            <th className="border-b border-subtle px-4 py-2 text-11 font-semibold tracking-wider text-tertiary uppercase">
              Artinya
            </th>
          </tr>
        </thead>
        <tbody>
          {terms.map((term) => (
            <tr key={term.term} className="border-b border-subtle last:border-b-0">
              <td className="px-4 py-2.5 align-top text-13 font-medium text-primary">{term.term}</td>
              <td className="px-4 py-2.5 align-top text-13 leading-6 text-secondary">{term.meaning}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function DocsTopicLink({ slug, children }: { slug: string; children: ReactNode }) {
  const { workspaceSlug } = useParams();
  return (
    <Link
      href={`/${workspaceSlug?.toString() ?? ""}/docs/${slug}`}
      className="font-medium text-accent-primary hover:underline"
    >
      {children}
    </Link>
  );
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS (tidak ada error baru).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/docs/blocks.tsx
git commit -m "feat(web): docs content primitives"
```

---

### Task 2: Konten — `overview`

**Files:**

- Create: `apps/web/core/components/docs/content/overview.tsx`

- [ ] **Step 1: Tulis file konten**

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
  DocsTermTable,
} from "../blocks";

export default function OverviewContent() {
  return (
    <>
      <DocsParagraph>
        Platform ini adalah sistem manajemen layanan IT (IT service management) untuk tim support dan operasional. Di
        dalamnya kamu mengelola insiden, permintaan layanan, dan pengetahuan tim — dari laporan yang masuk lewat service
        desk, dikerjakan sebagai ticket, sampai didokumentasikan sebagai artikel pengetahuan. Basisnya adalah Plane,
        alat manajemen proyek open-source, yang disesuaikan untuk alur kerja ITSM.
      </DocsParagraph>

      <DocsSectionHeading>Satu tempat untuk kerja harian tim</DocsSectionHeading>
      <DocsParagraph>Empat area utama yang akan sering kamu pakai:</DocsParagraph>
      <DocsFeatureGrid>
        <DocsFeatureCard title="Tickets">Pekerjaan operasional harian: insiden, permintaan, dan tugas.</DocsFeatureCard>
        <DocsFeatureCard title="Sprints & Tracks">
          Perencanaan pekerjaan dalam periode berulang atau kelompok besar.
        </DocsFeatureCard>
        <DocsFeatureCard title="Services & War Rooms">
          Katalog layanan beserta ruang koordinasi saat insiden besar.
        </DocsFeatureCard>
        <DocsFeatureCard title="Knowledge base">
          Artikel dan panduan yang bisa dibaca seluruh tim — bahkan dipublikasikan keluar.
        </DocsFeatureCard>
      </DocsFeatureGrid>

      <DocsSectionHeading>Istilah yang kamu temui</DocsSectionHeading>
      <DocsParagraph>
        Beberapa fitur memakai istilah yang berbeda dari produk project management pada umumnya. Berikut padanannya:
      </DocsParagraph>
      <DocsTermTable
        terms={[
          { term: "Ticket", meaning: "Unit kerja utama (work item): satu insiden, permintaan, atau tugas." },
          { term: "Sprint", meaning: "Periode kerja berulang (cycle) dengan tanggal mulai dan selesai." },
          { term: "Track", meaning: "Pengelompokan ticket menuju satu sasaran atau fase (module)." },
          { term: "Queue", meaning: "Daftar ticket yang tersaring dengan filter tersimpan (view)." },
          { term: "Knowledge base", meaning: "Kumpulan halaman dokumen tim (pages)." },
          { term: "Inbox", meaning: "Pemberitahuan: mention, assignment, komentar, perubahan status." },
          { term: "Reports", meaning: "Analitik workspace: tren dan distribusi pekerjaan." },
        ]}
      />

      <DocsSectionHeading>Workspace dan project</DocsSectionHeading>
      <DocsParagraph>
        Workspace adalah ruang kerja organisasimu — di sinilah member, pengaturan global, dan semua project berada.
        Project adalah unit kerja di dalam workspace, misalnya per produk, per layanan, atau per tim. Setiap ticket
        selalu berada di dalam sebuah project.
      </DocsParagraph>

      <DocsSectionHeading>Peran</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Admin",
            meaning: "Akses penuh ke seluruh workspace, termasuk pengaturan member dan konfigurasi global.",
          },
          {
            term: "Member",
            meaning: "Bisa membaca, membuat, dan mengubah pekerjaan di dalam project tempat ia bergabung.",
          },
          {
            term: "Guest",
            meaning: "Akses terbatas untuk pihak eksternal; biasanya hanya melihat project tertentu.",
          },
        ]}
      />

      <DocsCallout variant="info" title="Baru pertama kali?">
        Mulai dari bagian <b>Untuk Member</b> di sidebar kiri untuk mengenal fitur pribadimu, lalu lanjut ke{" "}
        <b>Untuk Project</b> untuk memahami isi setiap project.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/docs/content/overview.tsx
git commit -m "feat(web): docs content - overview"
```

---

### Task 3: Konten — member part 1 (home, inbox, your-work, reports, workspace-views, search)

**Files:**

- Create: `apps/web/core/components/docs/content/home.tsx`
- Create: `apps/web/core/components/docs/content/inbox.tsx`
- Create: `apps/web/core/components/docs/content/your-work.tsx`
- Create: `apps/web/core/components/docs/content/reports.tsx`
- Create: `apps/web/core/components/docs/content/workspace-views.tsx`
- Create: `apps/web/core/components/docs/content/search.tsx`

Semua file memakai header copyright yang sama seperti Task 2 dan default export dengan pola `XxxContent`.

- [ ] **Step 1: `content/home.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function HomeContent() {
  return (
    <>
      <DocsParagraph>
        Home adalah halaman pertama yang kamu lihat setelah login. Isinya ringkasan hal-hal yang paling sering kamu
        butuhkan: pekerjaan yang menunggumu dan aktivitas terbaru workspace.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat widget ringkasan — pekerjaan yang di-assign kepadamu, aktivitas terbaru rekan tim, dan statistik
        workspace. Mengubah tata letak widget sesuai kebutuhan, serta melihat siapa saja yang baru bergabung.
      </DocsParagraph>
      <DocsCallout variant="tip" title="Kebiasaan pagi yang baik">
        Buka Home, cek daftar assigned work, lalu lanjut ke <DocsTopicLink slug="inbox">Inbox</DocsTopicLink> untuk
        memastikan tidak ada mention atau komentar yang terlewat.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 2: `content/inbox.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function InboxContent() {
  return (
    <>
      <DocsParagraph>
        Inbox mengumpulkan semua pemberitahuan penting di satu tempat: mention, assignment, komentar, perubahan status,
        dan aktivitas pada item yang kamu ikuti.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuka ticket langsung dari notifikasi, menandai notifikasi sebagai sudah dibaca (satu per satu atau semua),
        dan memfilter pemberitahuan. Preferensi jenis notifikasi diatur di{" "}
        <DocsTopicLink slug="personal-settings">Profil & preferensi</DocsTopicLink>.
      </DocsParagraph>
      <DocsCallout variant="info" title="Subscription">
        Kamu otomatis mendapat notifikasi untuk ticket yang kamu buat, yang di-assign kepadamu, atau yang kamu
        komentari. Kamu juga bisa follow/unfollow ticket secara manual dari detail ticket.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 3: `content/your-work.tsx`**

```tsx
import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function YourWorkContent() {
  return (
    <>
      <DocsParagraph>
        Your work adalah halaman pribadimu: semua ticket lintas project dalam satu tampilan, tanpa perlu membuka project
        satu per satu.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat ticket yang di-assign kepadamu, yang kamu buat, dan yang kamu ikuti (subscribed). Di tab activity, kamu
        bisa melihat riwayat aktivitasmu yang terbaru.
      </DocsParagraph>
      <DocsParagraph>
        Butuh tampilan serupa untuk seluruh workspace? Gunakan{" "}
        <DocsTopicLink slug="workspace-views">Workspace Views</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 4: `content/reports.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading } from "../blocks";

export default function ReportsContent() {
  return (
    <>
      <DocsParagraph>
        Reports menampilkan analitik workspace: tren pekerjaan, sebaran status, dan gambaran beban tim — berguna saat
        rapat mingguan atau evaluasi layanan.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat metrik per project (jumlah ticket, distribusi status dan priority), memfilter berdasarkan rentang waktu,
        serta memantau tren penyelesaian pekerjaan dari waktu ke waktu.
      </DocsParagraph>
      <DocsCallout variant="info">Reports tersedia untuk admin dan member; guest tidak memilikinya.</DocsCallout>
    </>
  );
}
```

- [ ] **Step 5: `content/workspace-views.tsx`**

```tsx
import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function WorkspaceViewsContent() {
  return (
    <>
      <DocsParagraph>
        Workspace Views adalah kumpulan ticket lintas project yang tersimpan sebagai tampilan. Empat view bawaan yang
        tersedia: All issues (semua ticket yang bisa kamu akses), Assigned (milikmu), Created (kamu buat), dan
        Subscribed (kamu ikuti).
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuka view bawaan, memfilter dan mencari ticket lintas project, serta membuka ticket tetap dalam konteks
        workspace. Untuk daftar tersaring di dalam satu project, gunakan{" "}
        <DocsTopicLink slug="queues">Queues</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 6: `content/search.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function SearchContent() {
  return (
    <>
      <DocsParagraph>
        Tekan Cmd/Ctrl + K kapan saja untuk membuka command palette: satu pintu untuk mencari dan menjalankan aksi tanpa
        mouse.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Mencari ticket, project, artikel knowledge base, dan member; menjalankan aksi cepat seperti membuat ticket baru,
        membuka halaman, atau berpindah project; serta melihat riwayat pencarian terakhirmu.
      </DocsParagraph>
      <DocsCallout variant="tip">
        Hafalkan Cmd/Ctrl + K — ini jalan tercepat ke mana pun. Untuk mencari solusi insiden yang pernah terjadi, coba
        kata kunci masalahnya lalu buka hasil dari <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 7: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/docs/content/home.tsx apps/web/core/components/docs/content/inbox.tsx apps/web/core/components/docs/content/your-work.tsx apps/web/core/components/docs/content/reports.tsx apps/web/core/components/docs/content/workspace-views.tsx apps/web/core/components/docs/content/search.tsx
git commit -m "feat(web): docs content - member topics (part 1)"
```

---

### Task 4: Konten — member part 2 (drafts, stickies, scheduler, galileo, personal-settings)

**Files:**

- Create: `apps/web/core/components/docs/content/drafts.tsx`
- Create: `apps/web/core/components/docs/content/stickies.tsx`
- Create: `apps/web/core/components/docs/content/scheduler.tsx`
- Create: `apps/web/core/components/docs/content/galileo.tsx`
- Create: `apps/web/core/components/docs/content/personal-settings.tsx`

Semua file diawali header copyright yang sama seperti Task 2 dan default export dengan pola `XxxContent`.

- [ ] **Step 1: `content/drafts.tsx`**

```tsx
import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function DraftsContent() {
  return (
    <>
      <DocsParagraph>
        Drafts menyimpan ticket yang belum siap dibuat — tulisan setengah jadi tetap aman dan hanya terlihat olehmu.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Menyimpan draft ticket (judul, deskripsi, properti) tanpa langsung membuatnya, melanjutkan draft kapan saja, dan
        mempublikasikannya menjadi ticket penuh saat sudah siap. Draft bisa dimulai dari tombol New ticket lalu pilih
        Save as draft.
      </DocsParagraph>
      <DocsParagraph>
        Setelah dipublikasikan, ticket mengikuti alur normal di <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 2: `content/stickies.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading } from "../blocks";

export default function StickiesContent() {
  return (
    <>
      <DocsParagraph>
        Stickies adalah catatan tempel digital untuk hal-hal cepat yang tidak perlu menjadi ticket: daftar periksa
        manual, nomor kontak darurat, atau pengingat singkat.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Menulis catatan dengan warna pilihan, mengganti warna atau menghapusnya, memilih tampilan board atau list, serta
        mengarsipkan catatan lama.
      </DocsParagraph>
      <DocsCallout variant="info">
        Stickies bersifat pribadi — rekan satu workspace tidak melihat catatanmu.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 3: `content/scheduler.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function SchedulerContent() {
  return (
    <>
      <DocsParagraph>
        Scheduler menjalankan tugas AI secara terjadwal: laporan rutin, rangkuman mingguan, atau pengingat otomatis —
        tanpa kamu harus memicunya manual.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat jadwal dengan prompt dan frekuensi (harian, mingguan, dan seterusnya), mengaktifkan atau
        menonaktifkannya, serta melihat riwayat eksekusi beserta hasilnya.
      </DocsParagraph>
      <DocsCallout variant="tip">
        Scheduler memakai kemampuan yang sama dengan <DocsTopicLink slug="galileo">Galileo</DocsTopicLink>. Tulis prompt
        sejelas mungkin: sebutkan project dan jenis laporan yang kamu inginkan.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 4: `content/galileo.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function GalileoContent() {
  return (
    <>
      <DocsParagraph>
        Galileo adalah asisten AI di sisi kanan layar. Ia bisa menjawab pertanyaan dengan konteks pekerjaan yang sedang
        kamu lihat — ticket, service, sprint, track, atau artikel knowledge base.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Mengobrol tentang item yang sedang terbuka (minta ringkasan, draf balasan, atau analisis singkat), memulai
        percakapan baru, dan membuka kembali riwayat percakapan sebelumnya.
      </DocsParagraph>
      <DocsCallout variant="info" title="Konteks itu penting">
        Buka ticket atau service dulu, lalu bertanya — jawaban AI mengacu ke data yang sedang kamu lihat. Untuk tugas
        berulang, rangkai dengan <DocsTopicLink slug="scheduler">Scheduler</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 5: `content/personal-settings.tsx`**

```tsx
import { DocsParagraph, DocsSectionHeading, DocsTermTable, DocsTopicLink } from "../blocks";

export default function PersonalSettingsContent() {
  return (
    <>
      <DocsParagraph>
        Pengaturan pribadi dibuka dari menu profil di pojok kiri bawah (klik namamu lalu Settings). Semua yang ada di
        sini hanya memengaruhi akunmu, bukan tim.
      </DocsParagraph>
      <DocsSectionHeading>Tab yang tersedia</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "General", meaning: "Nama, foto profil, dan email." },
          { term: "Preferences", meaning: "Tema (terang/gelap), bahasa, dan preferensi tampilan." },
          { term: "Notifications", meaning: "Jenis pemberitahuan email dan in-app yang ingin kamu terima." },
          { term: "Security", meaning: "Ubah password akunmu." },
          { term: "API Tokens", meaning: "Token untuk integrasi pribadi dengan API." },
        ]}
      />
      <DocsParagraph>
        Jenis notifikasi yang kamu atur di sini menentukan isi <DocsTopicLink slug="inbox">Inbox</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 6: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/docs/content/drafts.tsx apps/web/core/components/docs/content/stickies.tsx apps/web/core/components/docs/content/scheduler.tsx apps/web/core/components/docs/content/galileo.tsx apps/web/core/components/docs/content/personal-settings.tsx
git commit -m "feat(web): docs content - member topics (part 2)"
```

---

### Task 5: Konten — project part 1 (tickets, sprints, tracks, services)

**Files:**

- Create: `apps/web/core/components/docs/content/tickets.tsx`
- Create: `apps/web/core/components/docs/content/sprints.tsx`
- Create: `apps/web/core/components/docs/content/tracks.tsx`
- Create: `apps/web/core/components/docs/content/services.tsx`

Semua file diawali header copyright yang sama seperti Task 2 dan default export dengan pola `XxxContent`.

- [ ] **Step 1: `content/tickets.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTermTable, DocsTopicLink } from "../blocks";

export default function TicketsContent() {
  return (
    <>
      <DocsParagraph>
        Ticket adalah jantung platform ini. Setiap pekerjaan — dari gangguan kecil sampai major incident — dicatat,
        di-assign, dan dilacak progresnya sebagai ticket di dalam sebuah project.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat ticket dengan cepat (tombol New ticket atau shortcut C), mengatur propertinya, berdiskusi lewat komentar
        dengan mention, melampirkan file, menautkan ticket dengan relasi, memecah pekerjaan menjadi sub-items, dan
        menandai ticket sebagai war room untuk insiden besar.
      </DocsParagraph>
      <DocsSectionHeading>Properti ticket</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "State", meaning: "Status ticket di alur kerja project (bisa dikustomisasi admin)." },
          { term: "Assignee", meaning: "Penanggung jawab ticket — bisa lebih dari satu." },
          { term: "Priority", meaning: "Tingkat urgensi: urgent, high, medium, low, none." },
          { term: "Start & Target date", meaning: "Kapan pekerjaan dimulai dan ditargetkan selesai." },
          { term: "Labels", meaning: "Penanda bebas untuk kategorisasi (mis. layanan atau jenis gangguan)." },
          { term: "Estimate", meaning: "Perkiraan ukuran pekerjaan sesuai skala project." },
          { term: "Sprints / Tracks", meaning: "Keanggotaan ticket dalam periode atau kelompok pekerjaan." },
          { term: "Parent", meaning: "Ticket induk saat pekerjaan dipecah menjadi sub-items." },
        ]}
      />
      <DocsSectionHeading>Alur hidup ticket</DocsSectionHeading>
      <DocsStepList
        steps={[
          { title: "Dibuat", description: "Ticket baru masuk dari kamu, dari draft, atau dari Intake." },
          {
            title: "Dikerjakan",
            description: "Status berjalan (mis. In Progress); assignee dan prioritas sudah jelas.",
          },
          { title: "Selesai", description: "Status masuk ke grup Done, atau Cancelled bila tidak dikerjakan." },
        ]}
      />
      <DocsCallout variant="tip">
        Gunakan relasi (duplicate, blocking, blocked by) saat menangani insiden berantai — ini membantu tim melihat
        ketergantungan antar pekerjaan. Untuk insiden besar, lanjutkan ke{" "}
        <DocsTopicLink slug="war-rooms">War Rooms</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 2: `content/sprints.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function SprintsContent() {
  return (
    <>
      <DocsParagraph>
        Sprint adalah periode kerja berulang untuk merencanakan dan menyelesaikan pekerjaan dalam rentang waktu tetap —
        misalnya satu atau dua minggu.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat sprint dengan tanggal mulai dan target selesai, menambahkan ticket dari backlog, memantau progress dan
        burndown sprint, lalu menutup sprint — ticket yang belum selesai dipindahkan ke sprint berikutnya atau kembali
        ke backlog.
      </DocsParagraph>
      <DocsCallout variant="info">
        Sprint hanya tampil bila fitur ini diaktifkan admin project di pengaturan Features. Melihat pekerjaan lintas
        sprint? Gunakan <DocsTopicLink slug="queues">Queues</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 3: `content/tracks.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading } from "../blocks";

export default function TracksContent() {
  return (
    <>
      <DocsParagraph>
        Track mengelompokkan kumpulan ticket yang menuju satu sasaran atau fase — misalnya satu perbaikan besar, atau
        satu tahap migrasi sistem.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat track dengan lead dan target date, mengelompokkan ticket ke dalamnya, dan memantau progress dari
        ringkasan ticket yang sudah selesai.
      </DocsParagraph>
      <DocsCallout variant="tip">
        Sprint menjawab "kapan", track menjawab "bagian mana dari sasaran besar". Pekerjaan bisa berada di keduanya
        sekaligus.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 4: `content/services.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTermTable, DocsTopicLink } from "../blocks";

export default function ServicesContent() {
  return (
    <>
      <DocsParagraph>
        Services adalah katalog layanan IT: apa saja yang disediakan tim, siapa pemiliknya, dan bagaimana kondisi
        kesehatannya saat ini.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat daftar service beserta status kesehatan dan pemiliknya, membuka detail service (deskripsi, dependencies,
        ticket terkait), serta menelusuri dependency graph untuk memahami hubungan antar layanan.
      </DocsParagraph>
      <DocsSectionHeading>Istilah di halaman Services</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "Health", meaning: "Status kesehatan service: sehat, terganggu, atau down." },
          {
            term: "Dependencies",
            meaning: "Layanan lain yang bergantung pada, atau menjadi tempat bergantung, service ini.",
          },
          { term: "Dependency graph", meaning: "Peta hubungan antar service untuk menilai dampak suatu gangguan." },
        ]}
      />
      <DocsCallout variant="tip">
        Saat insiden besar terjadi, cek dependency graph untuk memperkirakan siapa saja yang terdampak, lalu buka{" "}
        <DocsTopicLink slug="war-rooms">War Rooms</DocsTopicLink> untuk koordinasi.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 5: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/docs/content/tickets.tsx apps/web/core/components/docs/content/sprints.tsx apps/web/core/components/docs/content/tracks.tsx apps/web/core/components/docs/content/services.tsx
git commit -m "feat(web): docs content - project topics (part 1)"
```

---

### Task 6: Konten — project part 2 (war-rooms, queues, knowledge-base, intake)

**Files:**

- Create: `apps/web/core/components/docs/content/war-rooms.tsx`
- Create: `apps/web/core/components/docs/content/queues.tsx`
- Create: `apps/web/core/components/docs/content/knowledge-base.tsx`
- Create: `apps/web/core/components/docs/content/intake.tsx`

Semua file diawali header copyright yang sama seperti Task 2 dan default export dengan pola `XxxContent`.

- [ ] **Step 1: `content/war-rooms.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTopicLink } from "../blocks";

export default function WarRoomsContent() {
  return (
    <>
      <DocsParagraph>
        War room adalah ruang khusus untuk menangani insiden besar (major incident): semua koordinasi terjadi di satu
        tempat. War room dibuat dari sebuah ticket — ticket itu berubah menjadi ruang insiden.
      </DocsParagraph>
      <DocsSectionHeading>Alur memulai war room</DocsSectionHeading>
      <DocsStepList
        steps={[
          { title: "Buka ticket insiden", description: "Pilih ticket yang layak ditangani sebagai major incident." },
          {
            title: "Aktifkan toggle War room",
            description: "Ticket berubah menjadi war room dan otomatis masuk daftar insiden.",
          },
          {
            title: "Koordinasi di dalam ruang",
            description: "Chat realtime dengan peserta, ikuti runbook, dan pantau dampak.",
          },
        ]}
      />
      <DocsSectionHeading>Yang ada di dalam war room</DocsSectionHeading>
      <DocsParagraph>
        Blast radius (area dan service yang terdampak), chat realtime dengan peserta, runbook langkah penanganan, serta
        daftar semua war room aktif yang bisa dibuka lewat filter "war room" di halaman Tickets.
      </DocsParagraph>
      <DocsCallout variant="warning">
        War room memicu notifikasi ke peserta — gunakan untuk insiden yang benar-benar perlu penanganan terkoordinasi,
        bukan pekerjaan biasa. Cek dampak layanan di <DocsTopicLink slug="services">Services</DocsTopicLink> sebelum
        menetapkan blast radius.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 2: `content/queues.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function QueuesContent() {
  return (
    <>
      <DocsParagraph>
        Queue adalah antrean kerja tersaring: kumpulan ticket dengan filter yang kamu simpan sebagai tampilan agar bisa
        dibuka berulang kali olehmu dan tim.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat view dengan filter (status, assignee, priority, label, tanggal, dan lainnya), berganti tampilan antara
        list, kanban, calendar, dan spreadsheet, serta menyimpan dan membagikan view ke anggota project.
      </DocsParagraph>
      <DocsCallout variant="info">
        Butuh daftar serupa tetapi lintas project? Gunakan{" "}
        <DocsTopicLink slug="workspace-views">Workspace Views</DocsTopicLink>. Queue hanya menampilkan ticket dari
        project tempat ia dibuat.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 3: `content/knowledge-base.tsx`**

```tsx
import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function KnowledgeBaseContent() {
  return (
    <>
      <DocsParagraph>
        Knowledge base adalah tempat dokumentasi tim hidup: artikel solusi, panduan operasional, catatan insiden besar,
        dan apa pun yang perlu dibaca ulang.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Menulis halaman dengan editor kaya (heading, gambar, tabel, code block), membuat sub-halaman berjenjang,
        berkolaborasi realtime dengan rekan tim, mempublikasikan halaman ke web publik lewat share link, dan mengekspor
        halaman ke PDF atau DOCX.
      </DocsParagraph>
      <DocsParagraph>
        Tautkan artikel solusi ke <DocsTopicLink slug="tickets">tickets</DocsTopicLink> terkait agar penanganan
        berikutnya lebih cepat. Artikel baru biasanya lahir dari ticket yang sudah selesai — pertimbangkan menuliskannya
        setiap kali menyelesaikan insiden yang menarik.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 4: `content/intake.tsx`**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTopicLink } from "../blocks";

export default function IntakeContent() {
  return (
    <>
      <DocsParagraph>
        Intake adalah pintu masuk permintaan: laporan dari luar (email, form, webhook) masuk ke antrean triage sebelum
        menjadi ticket.
      </DocsParagraph>
      <DocsSectionHeading>Alur triage</DocsSectionHeading>
      <DocsStepList
        steps={[
          { title: "Permintaan masuk", description: "Laporan dari intake source muncul di antrean intake." },
          {
            title: "Triage",
            description: "Periksa kelayakan: ini insiden, permintaan layanan, atau bukan urusan tim?",
          },
          {
            title: "Accept atau Decline",
            description: "Accept mengubahnya menjadi ticket; Decline menutupnya dengan alasan.",
          },
        ]}
      />
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat daftar permintaan masuk beserta sumbernya, menerima atau menolak dengan alasan, dan mengatur sumber
        intake di pengaturan project (lihat <DocsTopicLink slug="project-settings">Pengaturan project</DocsTopicLink>).
      </DocsParagraph>
      <DocsCallout variant="info">
        Intake menjaga halaman Tickets tetap bersih: hanya permintaan yang tervalidasi yang menjadi{" "}
        <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
```

- [ ] **Step 5: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/docs/content/war-rooms.tsx apps/web/core/components/docs/content/queues.tsx apps/web/core/components/docs/content/knowledge-base.tsx apps/web/core/components/docs/content/intake.tsx
git commit -m "feat(web): docs content - project topics (part 2)"
```

---

### Task 7: Konten — pengaturan (project-settings)

**Files:**

- Create: `apps/web/core/components/docs/content/project-settings.tsx`

File diawali header copyright yang sama seperti Task 2.

- [ ] **Step 1: Tulis file konten**

```tsx
import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTermTable, DocsTopicLink } from "../blocks";

export default function ProjectSettingsContent() {
  return (
    <>
      <DocsParagraph>
        Setiap project punya pengaturan sendiri, diakses lewat ikon gear di halaman project. Perubahan di sini berlaku
        untuk semua anggota project — biasanya hanya admin project yang mengubahnya.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa diatur</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "General",
            meaning: "Nama project, identifier (kode ticket), deskripsi, zona waktu, dan akses jaringan.",
          },
          { term: "Members", meaning: "Undang atau kelola anggota project beserta perannya." },
          {
            term: "Features",
            meaning: "Menyalakan atau mematikan fitur: Sprints, Tracks, Queues, Knowledge base, Intake.",
          },
          { term: "States", meaning: "Alur status ticket; bisa dikustomisasi, termasuk grup statusnya." },
          { term: "Work item types", meaning: "Jenis ticket di project ini dan pengaturannya." },
          { term: "Labels", meaning: "Penanda untuk mengategorikan ticket." },
          { term: "Estimates", meaning: "Skala perkiraan ukuran pekerjaan." },
          { term: "Intake sources", meaning: "Sumber masuknya permintaan ke Intake (email, webhook, form)." },
          { term: "Automations", meaning: "Aturan otomatis, misalnya mengubah status atau menambah assignee." },
        ]}
      />
      <DocsCallout variant="warning">
        Berhati-hatilah menghapus state, label, atau work item type yang sedang dipakai ticket — dampaknya berlaku ke
        data yang ada, bukan hanya ke ticket baru.
      </DocsCallout>
      <DocsParagraph>
        Efek dari pengaturan ini terlihat di <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="intake">Intake</DocsTopicLink>, dan <DocsTopicLink slug="queues">Queues</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/docs/content/project-settings.tsx
git commit -m "feat(web): docs content - project settings"
```

---

### Task 8: Registry + unit test (TDD)

**Files:**

- Create: `apps/web/core/components/docs/registry.test.ts` (write first)
- Create: `apps/web/core/components/docs/registry.ts`

- [ ] **Step 1: Tulis test yang gagal**

`apps/web/core/components/docs/registry.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { DEFAULT_DOC_TOPIC_SLUG, DOC_TOPIC_GROUPS, DOC_TOPICS, getTopicBySlug, getTopicNeighbors } from "./registry";

describe("docs registry", () => {
  it("has unique slugs", () => {
    const slugs = DOC_TOPICS.map((topic) => topic.slug);
    expect(new Set(slugs).size).toBe(slugs.length);
  });

  it("assigns every topic to a known group", () => {
    const groupKeys = new Set(DOC_TOPIC_GROUPS.map((group) => group.key));
    DOC_TOPICS.forEach((topic) => expect(groupKeys.has(topic.group)).toBe(true));
  });

  it("keeps the default topic first in order", () => {
    expect(DOC_TOPICS[0]?.slug).toBe(DEFAULT_DOC_TOPIC_SLUG);
  });

  it("resolves known slugs and rejects unknown ones", () => {
    expect(getTopicBySlug("tickets")?.slug).toBe("tickets");
    expect(getTopicBySlug("not-a-topic")).toBeUndefined();
  });

  it("returns neighbors with correct edges", () => {
    const first = getTopicNeighbors(DOC_TOPICS[0].slug);
    expect(first.prev).toBeUndefined();
    expect(first.next?.slug).toBe(DOC_TOPICS[1].slug);

    const lastIndex = DOC_TOPICS.length - 1;
    const last = getTopicNeighbors(DOC_TOPICS[lastIndex].slug);
    expect(last.next).toBeUndefined();
    expect(last.prev?.slug).toBe(DOC_TOPICS[lastIndex - 1].slug);

    const middle = getTopicNeighbors(DOC_TOPICS[3].slug);
    expect(middle.prev?.slug).toBe(DOC_TOPICS[2].slug);
    expect(middle.next?.slug).toBe(DOC_TOPICS[4].slug);
  });

  it("returns empty neighbors for unknown slugs", () => {
    expect(getTopicNeighbors("not-a-topic")).toEqual({});
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm --filter=web exec vitest run core/components/docs/registry.test.ts`
Expected: FAIL — `Cannot find module './registry'` (atau serupa).

- [ ] **Step 3: Tulis registry**

`apps/web/core/components/docs/registry.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { lazy } from "react";
import type { ComponentType, ElementType, LazyExoticComponent } from "react";
import {
  AiStar1Outline,
  AlertOctagonOutline,
  AnalyticsOutline,
  CalendarOutline,
  CyclesOutline,
  DraftsOutline,
  FilterOutline,
  HomeOutline,
  InboxOutline,
  IntakeOutline,
  ModuleOutline,
  MultipleStickyOutline,
  OverviewOutline,
  PagesOutline,
  PreferencesOutline,
  SearchOutline,
  ServerOutline,
  SettingsOutline,
  ViewsOutline,
  WorkItemsOutline,
  YourWorkOutline,
} from "@makeplane/propel/icons";

export type TDocGroupKey = "getting-started" | "member" | "project" | "settings";

export const DOC_TOPIC_GROUPS: { key: TDocGroupKey; label: string }[] = [
  { key: "getting-started", label: "Memulai" },
  { key: "member", label: "Untuk Member" },
  { key: "project", label: "Untuk Project" },
  { key: "settings", label: "Pengaturan" },
];

export type TDocTopic = {
  slug: string;
  title: string;
  description: string;
  group: TDocGroupKey;
  icon: ElementType;
  component: LazyExoticComponent<ComponentType>;
};

export const DEFAULT_DOC_TOPIC_SLUG = "overview";

export const DOC_TOPICS: TDocTopic[] = [
  {
    slug: "overview",
    title: "Apa itu platform ini?",
    description: "Kenalan dulu: apa yang bisa dilakukan platform ini dan bagaimana istilah-istilahnya dipetakan.",
    group: "getting-started",
    icon: OverviewOutline,
    component: lazy(() => import("./content/overview")),
  },
  {
    slug: "home",
    title: "Home",
    description: "Dasbor awal: ringkasan pekerjaanmu dan aktivitas terbaru di workspace.",
    group: "member",
    icon: HomeOutline,
    component: lazy(() => import("./content/home")),
  },
  {
    slug: "inbox",
    title: "Inbox",
    description: "Semua pemberitahuan penting di satu tempat: mention, assignment, komentar, dan perubahan status.",
    group: "member",
    icon: InboxOutline,
    component: lazy(() => import("./content/inbox")),
  },
  {
    slug: "your-work",
    title: "Your work",
    description: "Halaman pribadi: semua pekerjaanmu lintas project dalam satu tampilan.",
    group: "member",
    icon: YourWorkOutline,
    component: lazy(() => import("./content/your-work")),
  },
  {
    slug: "reports",
    title: "Reports",
    description: "Analitik workspace: tren pekerjaan, distribusi status, dan gambaran beban tim.",
    group: "member",
    icon: AnalyticsOutline,
    component: lazy(() => import("./content/reports")),
  },
  {
    slug: "workspace-views",
    title: "Workspace Views",
    description: "Kumpulan ticket lintas project yang tersimpan: semua, milikmu, kamu buat, dan yang kamu ikuti.",
    group: "member",
    icon: ViewsOutline,
    component: lazy(() => import("./content/workspace-views")),
  },
  {
    slug: "search",
    title: "Pencarian & command palette",
    description: "Tekan Cmd/Ctrl + K untuk mencari apa saja dan menjalankan aksi tanpa mouse.",
    group: "member",
    icon: SearchOutline,
    component: lazy(() => import("./content/search")),
  },
  {
    slug: "drafts",
    title: "Drafts",
    description: "Tempat menyimpan ticket yang belum siap kamu buat — tulisan setengah jadi tetap tersimpan.",
    group: "member",
    icon: DraftsOutline,
    component: lazy(() => import("./content/drafts")),
  },
  {
    slug: "stickies",
    title: "Stickies",
    description: "Catatan tempel digital untuk hal-hal cepat yang tidak perlu menjadi ticket.",
    group: "member",
    icon: MultipleStickyOutline,
    component: lazy(() => import("./content/stickies")),
  },
  {
    slug: "scheduler",
    title: "Scheduler",
    description: "Jadwalkan tugas AI berulang: laporan rutin, rangkuman, atau pengingat otomatis.",
    group: "member",
    icon: CalendarOutline,
    component: lazy(() => import("./content/scheduler")),
  },
  {
    slug: "galileo",
    title: "Galileo",
    description: "Asisten AI di sisi kanan layar: tanya jawab dengan konteks pekerjaanmu.",
    group: "member",
    icon: AiStar1Outline,
    component: lazy(() => import("./content/galileo")),
  },
  {
    slug: "personal-settings",
    title: "Profil & preferensi",
    description: "Atur profilmu: nama, tema, bahasa, notifikasi, keamanan, dan API token.",
    group: "member",
    icon: PreferencesOutline,
    component: lazy(() => import("./content/personal-settings")),
  },
  {
    slug: "tickets",
    title: "Tickets",
    description: "Unit kerja utama: setiap insiden, permintaan, atau tugas direpresentasikan sebagai satu ticket.",
    group: "project",
    icon: WorkItemsOutline,
    component: lazy(() => import("./content/tickets")),
  },
  {
    slug: "sprints",
    title: "Sprints",
    description: "Periode kerja berulang untuk merencanakan dan menyelesaikan pekerjaan dalam rentang waktu tetap.",
    group: "project",
    icon: CyclesOutline,
    component: lazy(() => import("./content/sprints")),
  },
  {
    slug: "tracks",
    title: "Tracks",
    description: "Kelompok pekerjaan yang lebih besar: sekumpulan ticket yang menuju satu sasaran atau fase.",
    group: "project",
    icon: ModuleOutline,
    component: lazy(() => import("./content/tracks")),
  },
  {
    slug: "services",
    title: "Services",
    description: "Katalog layanan IT: apa yang kami sediakan, siapa pemiliknya, dan bagaimana kesehatannya.",
    group: "project",
    icon: ServerOutline,
    component: lazy(() => import("./content/services")),
  },
  {
    slug: "war-rooms",
    title: "War Rooms",
    description: "Ruang insiden untuk major incident: koordinasi cepat dalam satu tempat.",
    group: "project",
    icon: AlertOctagonOutline,
    component: lazy(() => import("./content/war-rooms")),
  },
  {
    slug: "queues",
    title: "Queues",
    description: "Antrean kerja tersaring: simpan filter favoritmu sebagai view yang bisa dibuka tim.",
    group: "project",
    icon: FilterOutline,
    component: lazy(() => import("./content/queues")),
  },
  {
    slug: "knowledge-base",
    title: "Knowledge base",
    description: "Dokumentasi tim: artikel, panduan, dan catatan solusi — semuanya bisa dicari.",
    group: "project",
    icon: PagesOutline,
    component: lazy(() => import("./content/knowledge-base")),
  },
  {
    slug: "intake",
    title: "Intake",
    description: "Pintu masuk permintaan: triage laporan baru sebelum menjadi ticket.",
    group: "project",
    icon: IntakeOutline,
    component: lazy(() => import("./content/intake")),
  },
  {
    slug: "project-settings",
    title: "Pengaturan project",
    description: "Ruang konfigurasi project: dari nama sampai automasi — khusus admin project.",
    group: "settings",
    icon: SettingsOutline,
    component: lazy(() => import("./content/project-settings")),
  },
];

export const getTopicBySlug = (slug: string): TDocTopic | undefined => DOC_TOPICS.find((topic) => topic.slug === slug);

export const getTopicNeighbors = (slug: string): { prev?: TDocTopic; next?: TDocTopic } => {
  const index = DOC_TOPICS.findIndex((topic) => topic.slug === slug);
  if (index === -1) return {};
  return { prev: DOC_TOPICS[index - 1], next: DOC_TOPICS[index + 1] };
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm --filter=web exec vitest run core/components/docs/registry.test.ts`
Expected: PASS — 6 tests.

- [ ] **Step 5: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/docs/registry.ts apps/web/core/components/docs/registry.test.ts
git commit -m "feat(web): docs topic registry with unit tests"
```

---

### Task 9: Komponen shell (sidebar, layout, topic page)

**Files:**

- Create: `apps/web/core/components/docs/docs-sidebar.tsx`
- Create: `apps/web/core/components/docs/docs-topic-page.tsx`
- Create: `apps/web/core/components/docs/docs-layout.tsx`

- [ ] **Step 1: `docs-sidebar.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
import { useParams } from "next/navigation";
import { cn } from "@plane/utils";
import { DEFAULT_DOC_TOPIC_SLUG, DOC_TOPIC_GROUPS, DOC_TOPICS } from "./registry";

type Props = {
  isMobile?: boolean;
  onNavigate?: () => void;
};

export function DocsSidebar({ isMobile = false, onNavigate }: Props) {
  const { workspaceSlug, topicSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const activeSlug = topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG;

  return (
    <nav
      className={cn(
        "h-full w-64 flex-col overflow-y-auto px-3 py-4",
        isMobile ? "flex w-full" : "hidden border-r border-subtle md:flex"
      )}
    >
      {DOC_TOPIC_GROUPS.map((group) => (
        <div key={group.key} className="mb-4 last:mb-0">
          <p className="px-2 text-11 font-semibold tracking-wider text-placeholder uppercase">{group.label}</p>
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
        </div>
      ))}
    </nav>
  );
}
```

- [ ] **Step 2: `docs-topic-page.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Suspense } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { ChevronLeftOutline, ChevronRightOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
// components
import { PageHead } from "@/components/core/page-title";
// local imports
import { DOC_TOPIC_GROUPS, getTopicNeighbors } from "./registry";
import type { TDocTopic } from "./registry";

export function DocsTopicPage({ topic }: { topic: TDocTopic }) {
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const { prev, next } = getTopicNeighbors(topic.slug);
  const group = DOC_TOPIC_GROUPS.find((item) => item.key === topic.group);
  const Content = topic.component;

  return (
    <div className="h-full w-full overflow-y-auto">
      <PageHead title={`${topic.title} · Docs`} />
      <div className="mx-auto w-full max-w-3xl px-6 py-10 md:px-10">
        {group && <p className="text-11 font-semibold tracking-wider text-tertiary uppercase">{group.label}</p>}
        <h1 className={cn("text-20 font-semibold text-primary", group && "mt-2")}>{topic.title}</h1>
        <p className="mt-2 text-13 leading-6 text-secondary">{topic.description}</p>
        <div className="mt-8">
          <Suspense fallback={<div className="h-40 w-full animate-pulse rounded-lg bg-layer-1" />}>
            <Content />
          </Suspense>
        </div>
        {(prev || next) && (
          <div className="mt-12 flex items-stretch justify-between gap-4 border-t border-subtle pt-6">
            {prev ? (
              <Link
                href={`/${slug}/docs/${prev.slug}`}
                className="flex max-w-[45%] flex-col gap-1 rounded-lg border border-subtle p-3 hover:bg-layer-transparent-hover"
              >
                <span className="flex items-center gap-1 text-11 text-tertiary">
                  <ChevronLeftOutline className="size-3 flex-shrink-0" />
                  Sebelumnya
                </span>
                <span className="text-13 font-medium text-primary">{prev.title}</span>
              </Link>
            ) : (
              <div />
            )}
            {next && (
              <Link
                href={`/${slug}/docs/${next.slug}`}
                className="ml-auto flex max-w-[45%] flex-col items-end gap-1 rounded-lg border border-subtle p-3 hover:bg-layer-transparent-hover"
              >
                <span className="flex items-center gap-1 text-11 text-tertiary">
                  Selanjutnya
                  <ChevronRightOutline className="size-3 flex-shrink-0" />
                </span>
                <span className="text-13 font-medium text-primary">{next.title}</span>
              </Link>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: `docs-layout.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import { Disclosure } from "@headlessui/react";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
// local imports
import { DocsSidebar } from "./docs-sidebar";
import { DEFAULT_DOC_TOPIC_SLUG, getTopicBySlug } from "./registry";

function DocsMobileNav() {
  const { topicSlug } = useParams();
  const activeTopic = getTopicBySlug(topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG);
  const ActiveIcon = activeTopic?.icon;

  return (
    <Disclosure as="div" className="relative z-40 border-b border-subtle bg-surface-1 md:hidden">
      {({ open, close }) => (
        <>
          <Disclosure.Button as="button" type="button" className="flex w-full items-center justify-between px-4 py-3">
            <span className="flex items-center gap-2 text-13 font-medium text-secondary">
              {ActiveIcon && <ActiveIcon className="size-4 flex-shrink-0 text-tertiary" />}
              {activeTopic?.title ?? "Docs"}
            </span>
            <ChevronDownOutline
              className={cn("size-4 flex-shrink-0 text-tertiary transition-transform", open && "rotate-180")}
            />
          </Disclosure.Button>
          {open && (
            <Disclosure.Panel
              static
              className="absolute inset-x-0 top-full max-h-[70vh] overflow-y-auto border-b border-subtle bg-surface-1 shadow-lg"
            >
              <DocsSidebar isMobile onNavigate={close} />
            </Disclosure.Panel>
          )}
        </>
      )}
    </Disclosure>
  );
}

export function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex size-full flex-col overflow-hidden bg-surface-1">
      <DocsMobileNav />
      <div className="relative flex size-full overflow-hidden">
        <DocsSidebar />
        <div className="size-full overflow-hidden">{children}</div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/docs/docs-sidebar.tsx apps/web/core/components/docs/docs-topic-page.tsx apps/web/core/components/docs/docs-layout.tsx
git commit -m "feat(web): docs shell components"
```

---

### Task 10: Routes & section layout

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(docs)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(docs)/docs/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(docs)/docs/[topicSlug]/page.tsx`
- Modify: `apps/web/app/routes/core.ts` (tambah section routes setelah blok SETTINGS SECTION, sebelum standalone routes)

- [ ] **Step 1: `(docs)/layout.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
// components
import { DocsLayout } from "@/components/docs/docs-layout";
import { ProjectsAppPowerKProvider } from "@/components/power-k/projects-app-provider";

export default function DocsSectionLayout() {
  return (
    <>
      <ProjectsAppPowerKProvider />
      <div className="relative flex size-full overflow-hidden rounded-lg border border-subtle">
        <DocsLayout>
          <Outlet />
        </DocsLayout>
      </div>
    </>
  );
}
```

- [ ] **Step 2: `docs/page.tsx` (redirect ke topik default)**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/page";
// local imports
import { DEFAULT_DOC_TOPIC_SLUG } from "@/components/docs/registry";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  throw redirect(`/${params.workspaceSlug}/docs/${DEFAULT_DOC_TOPIC_SLUG}/`);
};

export default function DocsIndexPage() {
  return null;
}
```

- [ ] **Step 3: `docs/[topicSlug]/page.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
import { redirect } from "react-router";
import type { Route } from "./+types/page";
// components
import { DocsTopicPage } from "@/components/docs/docs-topic-page";
// local imports
import { DEFAULT_DOC_TOPIC_SLUG, getTopicBySlug } from "@/components/docs/registry";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const topic = getTopicBySlug(params.topicSlug ?? "");
  if (!topic) throw redirect(`/${params.workspaceSlug}/docs/${DEFAULT_DOC_TOPIC_SLUG}/`);
};

export default function DocsTopicRoute() {
  const { topicSlug } = useParams();
  const topic = getTopicBySlug(topicSlug?.toString() ?? "");
  if (!topic) return null;
  return <DocsTopicPage topic={topic} />;
}
```

- [ ] **Step 4: Daerah routes di `core.ts`**

Sisipkan blok berikut **di dalam children layout `[workspaceSlug]/layout.tsx`**, yaitu di antara baris `      ]),` yang menutup blok `(settings)/layout.tsx` (baris 390 di `core.ts` saat plan ditulis) dan baris `    ]),` yang menutup layout `[workspaceSlug]` (baris 391, tepat di atas komentar `// STANDALONE ROUTES`). Blok baru sejajar indentasinya (6 spasi) dengan `layout("./(all)/[workspaceSlug]/(settings)/layout.tsx", [...])`:

```ts
      // ====================================================================
      // DOCS SECTION
      // ====================================================================
      layout("./(all)/[workspaceSlug]/(docs)/layout.tsx", [
        route(":workspaceSlug/docs", "./(all)/[workspaceSlug]/(docs)/docs/page.tsx"),
        route(":workspaceSlug/docs/:topicSlug", "./(all)/[workspaceSlug]/(docs)/docs/[topicSlug]/page.tsx"),
      ]),
```

- [ ] **Step 5: Typecheck (termasuk react-router typegen)**

Run: `pnpm --filter=web check:types`
Expected: PASS. Jika `./+types/page` belum ada, typegen di script akan membuatnya.

- [ ] **Step 6: Smoke test di dev server**

Run: `pnpm --filter=web dev` (catatan AGENTS.md: hentikan `plane-web-prod.service` dulu bila port 3000 terpakai; aktifkan kembali setelah selesai).
Expected: buka `http://localhost:3000/<workspace-slug>/docs/overview` → halaman render; `/<slug>/docs` redirect ke overview; `/<slug>/docs/ngawur` redirect ke overview; sideline docs tampil dan topik aktif ter-highlight.

- [ ] **Step 7: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(docs)/layout.tsx" "apps/web/app/(all)/[workspaceSlug]/(docs)/docs/page.tsx" "apps/web/app/(all)/[workspaceSlug]/(docs)/docs/[topicSlug]/page.tsx" apps/web/app/routes/core.ts
git commit -m "feat(web): docs routes and section layout"
```

---

### Task 11: Entry points (sidebar workspace, Help menu, i18n keys)

**Files:**

- Modify: `packages/constants/src/workspace.ts` (item statis `docs` + daftar links)
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx` (whitelist `staticItems`)
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx` (ikon `docs`)
- Modify: `apps/web/core/components/workspace/sidebar/help-section/root.tsx` (menu "Documentation")
- Modify: `packages/i18n/src/locales/en/navigation.json` + `packages/i18n/src/locales/id/navigation.json` (key `sidebar.docs`)

- [ ] **Step 1: Item sidebar di `packages/constants/src/workspace.ts`**

Di `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`, tambahkan setelah entri `ai_scheduler`:

```ts
  docs: {
    key: "docs",
    labelTranslationKey: "sidebar.docs",
    href: `/docs/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

Lalu update daftar:

```ts
export const WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS: IWorkspaceSidebarNavigationItem[] = [
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["home"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["ai_scheduler"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["docs"],
];
```

- [ ] **Step 2: Whitelist di `sidebar-item.tsx`**

Di array `staticItems` (fungsi `SidebarItemBase`), tambahkan `"docs"`:

```ts
const staticItems = [
  "home",
  "pi_chat",
  "projects",
  "your_work",
  "stickies",
  "drafts",
  "ai_scheduler",
  "docs",
  ...(additionalStaticItems || []),
];
```

- [ ] **Step 3: Ikon di `helper.tsx`**

Tambahkan `DocumentationOutline` ke import dari `@makeplane/propel/icons`, lalu case baru di `getSidebarNavigationItemIcon`:

```tsx
    case "docs":
      return <DocumentationOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 4: Help menu di `help-section/root.tsx`**

Tambahkan import:

```tsx
import { useParams, useRouter } from "next/navigation";
import { DocumentationOutline, HelpOutline, UserOutline } from "@makeplane/propel/icons";
```

Di dalam komponen, tambahkan:

```tsx
const { workspaceSlug } = useParams();
const router = useRouter();
```

Lalu sisipkan `CustomMenu.MenuItem` baru tepat sebelum item "Keyboard shortcuts":

```tsx
<CustomMenu.MenuItem onClick={() => router.push(`/${workspaceSlug}/docs`)}>
  <div className="flex items-center gap-x-2 rounded-sm text-11">
    <DocumentationOutline className="h-3.5 w-3.5 text-secondary" width={14} height={14} />
    <span className="text-11">{t("documentation")}</span>
  </div>
</CustomMenu.MenuItem>
```

Catatan: key i18n `documentation` sudah ada di `common.json` en ("Documentation") dan id ("Dokumentasi") — tidak perlu ditambah.

- [ ] **Step 5: i18n `sidebar.docs`**

`packages/i18n/src/locales/en/navigation.json` — di objek `sidebar`, tambahkan setelah `"stickies": "Stickies",`:

```json
    "docs": "Docs",
```

`packages/i18n/src/locales/id/navigation.json` — di objek `sidebar`, tambahkan setelah `"stickies": "Stickies",`:

```json
    "docs": "Dokumentasi",
```

- [ ] **Step 6: Typecheck packages & web**

Run: `pnpm --filter=@plane/constants check:types && pnpm --filter=@plane/i18n check:types && pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 7: Verifikasi manual**

Di dev server: item "Docs" muncul di sidebar workspace (setelah Scheduler); klik → halaman docs. Buka Help menu (ikon tanda tanya kiri bawah) → item "Documentation" → halaman docs.

- [ ] **Step 8: Commit**

```bash
git add packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/sidebar-item.tsx apps/web/core/components/workspace/sidebar/helper.tsx apps/web/core/components/workspace/sidebar/help-section/root.tsx packages/i18n/src/locales/en/navigation.json packages/i18n/src/locales/id/navigation.json
git commit -m "feat(web): docs entry points in sidebar and help menu"
```

---

### Task 12: Verifikasi menyeluruh

**Files:** tidak ada file baru; hanya verifikasi.

- [ ] **Step 1: Unit test registry**

Run: `pnpm --filter=web test`
Expected: PASS (semua test, termasuk `registry.test.ts`).

- [ ] **Step 2: Typecheck & lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: typecheck PASS; lint tanpa error baru (warning count di bawah baseline `--max-warnings=11957`).

- [ ] **Step 3: Build produksi**

Run: `pnpm --filter=web build`
Expected: build sukses.

- [ ] **Step 4: Checklist manual lengkap (dev server)**

1. `/<slug>/docs` redirect ke `/<slug>/docs/overview/`.
2. `/<slug>/docs/ngawur` redirect ke overview.
3. Deep-link tiap topik (21) render tanpa error; judul tab browser `"<Topik> · Docs"`.
4. Prev/next: di `overview` hanya "Selanjutnya"; di `project-settings` (terakhir) hanya "Sebelumnya"; di tengah keduanya.
5. Sidebar: grup sesuai urutan (Memulai, Untuk Member, Untuk Project, Pengaturan); item aktif ter-highlight; halaman tetap bisa scroll.
6. Mobile (<768px): header dropdown menampilkan topik aktif; membuka dropdown, memilih topik → menu tertutup dan halaman berpindah.
7. Light & dark theme terbaca (teks, callout, tabel).
8. Guest tidak melihat error (semua topik terbuka).

- [ ] **Step 5: Opsional — tampilkan di tunnel**

Jika perlu tampil di tunnel (per AGENTS.md):

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

- [ ] **Step 6: Commit akhir (bila ada perbaikan dari verifikasi)**

```bash
git add -A
git commit -m "fix(web): docs page verification fixes"
```
