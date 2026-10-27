---
status: current
last_verified: 2026-10-27
source_hash: sha256:d6f5d192c3a4645b91eb5687b58c4a4f6c09e3c458695d292510bcf25dd41782
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/analytics.store.ts
  - apps/api-rs/crates/api/src/routes/analytic.rs
  - apps/web/core/components/analytics
---

# Analytics (Reports)

Status: **Current.**

## Intent

Reports memberi gambaran kesehatan delivery dan operasi workspace: tren
ticket, distribusi status/priority, dan beban lintas application.

## Route

| Surface      | Route                                               |
| ------------ | --------------------------------------------------- |
| Reports      | `/:workspaceSlug/analytics/:tabId`                  |
| Legacy       | `/:workspaceSlug/analytics` → `/analytics/overview` |
| In-app guide | `/:workspaceSlug/docs/reports`                      |

## Metrics

- Ticket created vs resolved.
- Distribusi berdasarkan state, priority, assignee, label, dan application.
- Tren waktu dan perbandingan periode.
- Insight per application dengan application scope selector.

Angka mengikuti data ticket yang visible untuk user; application scope harus
konsisten dengan filter URL.

## Stores and components

- Store: `apps/web/core/store/analytics.store.ts`.
- Komponen: `apps/web/core/components/analytics`.
- API: `apps/api-rs/crates/api/src/routes/analytic.rs`.

## Permissions

| Role              | Read             | Export         |
| ----------------- | ---------------- | -------------- |
| Member            | Ya, sesuai scope | Jika diizinkan |
| Application admin | Ya               | Ya             |
| Workspace admin   | Ya               | Ya             |
| Guest             | Tidak            | Tidak          |

## Limitations

- Reports bukan live monitoring; data mengikuti query saat halaman dimuat.
- Metrik service health dan metrics eksternal tidak otomatis masuk ke reports
  kecuali dihubungkan.
- Export mengikuti batas endpoint dan permission.

## Changelog

| Tanggal    | Perubahan                                                        |
| ---------- | ---------------------------------------------------------------- |
| 2026-10-27 | rewrite — route reports, metrics, application scope, limitations |
