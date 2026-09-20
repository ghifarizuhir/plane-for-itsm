# Dashboard Copy: Project → Service Management (ITSM) Voice — Design

Tanggal: 2026-09-20

## Context

Home dashboard workspace (`apps/web`, route `(projects)/…`, `WorkspaceDashboardHeader`) masih memakai copywriting nuansa project management ("Create a project", "Build, ship, and manage"). Platform ini adalah fork ITSM (Plane for ITSM) dan sudah punya fitur Services (`packages/i18n/src/locales/en/service.json`, `projects/[projectId]/services`). Tingkat changes: dashboard + terkait langsung, **locale `en` saja** — locale lain (`id`, `ja`, dll.) tidak disentuh karena UI produk adalah English.

Temuan arsitektur penting: modal create service (`apps/web/core/components/services/modal.tsx:19`) **wajib `projectId`** — services nested di dalam project. Workspace tanpa project tidak punya konteks service, sehingga urutan sehat tetap project dulu.

## Scope

- `packages/i18n/src/locales/en/home.json` — copy changes.
- `apps/web/core/components/home/widgets/empty-states/no-projects.tsx` — minor rewiring CTA.
- Tidak mengubah: recents filter "Projects" (data-accurate), `common.json:167` `create_project`, `en/power-k.json:74` "New project", `home.title`.

## Copy changes (en)

| Key                                     | Before                                                      | After                                                                         |
| --------------------------------------- | ----------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `home.empty.create_project.title`       | "Create a project"                                          | "Create your first project"                                                   |
| `home.empty.create_project.description` | "Most things start with a project in Terraline."            | "A project is home to your services, incidents, and work items in Terraline." |
| `home.empty.invite_team.description`    | "Build, ship, and manage with coworkers."                   | "Keep services healthy and resolve incidents together."                       |
| `home.recents.empty.project`            | "Your recent projects will appear here once you visit one." | "Your recent projects will show up here after you visit one."                 |

Prinsip: copy jujur oleh arsitektur — project diperkenalkan sebagai host untuk services/incidents (voice ITSM), tanpa menjanjikan CTA yang tidak sesuai fungsinya.

## Minimal rewiring (quickstart card 1)

Di `no-projects.tsx`, card `create-project`:

- Jika user sudah punya joined project (`joinedProjectIds.length > 0` — kondisi saat guide disembunyikan jarang tercapai, jadi jalur ini terutama untuk render transisi) → CTA utama card mengarah ke halaman services project pertama: `/${workspaceSlug}/projects/<firstJoinedProjectId>/services`, label "View services".
- Jika belum ada project → CTA tetap membuka create project modal, label "Get started" (prereq service).

Implementasi: tambahkan dua varian teks i18n (`home.empty.create_project.cta_view_services` = "View services") di `home.json`; pilih label dan target CTA berdasarkan `joinedProjectIds`.

## Error handling & edge cases

- Member non-admin: CTA create project tetap disabled (behavior existing dari `canCreateProject`) — tidak berubah.
- `joinedProjectIds` kosong/undefined: fallback ke jalur create project modal.
- CTA link services memakai `<Link>` existing; validasi project id dari store seperti pattern lain (guard falsy).

## Testing / verification

- `pnpm check:lint` dan `pnpm check:types`.
- Build web: `pnpm --filter=web build`, lalu restart prod service + verifikasi visual dashboard (empty state & filled state) sesuai AGENTS.md.
- Manual: quickstart guide tampil, CTA target benar pada kondisi 0 project dan ≥1 project.
