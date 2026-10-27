---
status: current
last_verified: 2026-10-27
source_hash: sha256:2be52121762e1f597bfe6298e5627428c1d84da2f06226c1db8432ed0e83697a
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/ai-assistant.store.ts
  - apps/web/core/store/ai-schedules.store.ts
  - apps/web/core/components/galileo
  - apps/api-rs/crates/api/src/routes
  - apps/api-rs/crates/ai
---

# AI — Galileo and Scheduler

Status: **Current.**

## Intent

Galileo adalah asisten kontekstual untuk pekerjaan di workspace. Scheduler
menjalankan recipe AI berulang. Keduanya memakai konfigurasi LLM yang sama dan
tetap menyimpan manusia sebagai pengambil keputusan.

## Galileo surfaces

| Surface              | Route                                                                |
| -------------------- | -------------------------------------------------------------------- |
| Full page            | `/:workspaceSlug/galileo`, `/:workspaceSlug/galileo/:conversationId` |
| Right-side assistant | Panel di halaman non-Galileo                                         |

- Full page memiliki history sidebar dan deep link per conversation.
- Right assistant dipakai untuk konteks halaman aktif.
- Rail Galileo disembunyikan jika LLM belum dikonfigurasi; URL langsung
  diarahkan ke workspace home.

## Scheduler

- Route `/:workspaceSlug/scheduler`.
- Recipe berjalan berkala: laporan harian, rangkuman mingguan, pengingat.
- Run history menyimpan status dan hasil.
- Schedule dapat diedit dan dijalankan manual.

## Intake triage suggestions

Saran triage intake memakai AI untuk mengusulkan klasifikasi. Saran tidak
diterapkan otomatis; user harus accept/decline. Ini gate human-in-the-loop.

## Configuration

- LLM dikonfigurasi di instance/workspace settings.
- Tanpa konfigurasi, fitur AI tidak muncul sebagai surface aktif.
- Jangan menyimpan API key di repo atau client bundle.

## API touchpoints

- `ai.rs`, `ai_agent`, `ai_conversations.rs`, `ai_schedule.rs`.
- Crate `apps/api-rs/crates/ai`.
- Store: `ai-assistant.store.ts`, `ai-schedules.store.ts`.
- Tabel: `ai_schedules`, `ai_schedule_runs`, `ai_conversations`, `ai_messages`.

Inventory: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Use assistant | Manage schedules | Configure LLM               |
| ----------------- | ------------- | ---------------- | --------------------------- |
| Member            | Ya            | Schedule sendiri | Tidak                       |
| Application admin | Ya            | Ya               | Tidak                       |
| Workspace admin   | Ya            | Ya               | Ya, jika diizinkan instance |
| Instance admin    | Ya            | Ya               | Ya                          |

## Limitations

- AI bukan sumber kebenaran; jawaban harus diverifikasi.
- Full page belum mendukung attachment, image input, streaming, atau stop
  generation.
- Conversation search/pinning/sharing belum tersedia.
- Kualitas saran bergantung pada konfigurasi model dan konteks yang dikirim.

## In-app guide

`/:workspaceSlug/docs/galileo` dan `/:workspaceSlug/docs/scheduler`.

## Changelog

| Tanggal    | Perubahan                                                         |
| ---------- | ----------------------------------------------------------------- |
| 2026-10-27 | new — Galileo full page, assistant, scheduler, triage suggestions |
