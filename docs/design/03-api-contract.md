---
status: current
last_verified: 2026-10-27
source_hash: sha256:000505effcc09eb903f65fdb22a69b5be2daa007e032691fc2752789e19b251e
source_paths:
  - apps/api-rs/crates/api/src/main.rs
  - apps/api-rs/crates/api/src/routes
---

# 03 — API Contract

Status: **Current.**

API utama adalah Rust Axum di `apps/api-rs`. Daftar method dan path lengkap
dihasilkan di [`../_generated/api-routes.md`](../_generated/api-routes.md).
Dokumen ini menjelaskan konvensi yang berlaku lintas endpoint.

## Base and conventions

- Local base URL: `http://localhost:8000`.
- JSON untuk request dan response.
- Timestamp memakai ISO 8601 UTC.
- ID adalah string/UUID; jangan mengandalkan urutan angka.
- Path workspace memakai slug: `/api/workspaces/:slug/`.
- Path application memakai project ID di bawah workspace.

## Authentication

Urutan autentikasi di `AuthUser`:

1. `Authorization: Bearer <jwt>` dengan JWT yang diverifikasi memakai
   `jwt_secret`.
2. Cookie akses `plane_at` atau `__Host-plane_at`.
3. `X-Api-Key` yang dicocokkan ke `api_tokens` aktif dan belum kedaluwarsa.

Jika tidak ada kredensial valid, response `401` dengan body
`{"error":"missing or invalid auth"}`.

Django fallback tetap memakai session cookie, CSRF, dan DRF session
authentication. Jangan mengasumsikan mekanisme itu berlaku di API utama.

## Authorization scopes

| Scope             | Akses                                                          |
| ----------------- | -------------------------------------------------------------- |
| Instance admin    | God-mode, workspace, user, auth provider, konfigurasi instance |
| Workspace admin   | Seluruh resource dalam workspace                               |
| Application admin | Konfigurasi dan ticket dalam application                       |
| Member            | Resource sesuai membership dan role                            |
| Guest             | Akses terbatas per application                                 |

Enforcement dilakukan oleh extractor `AuthUser` dan guard per handler. Jangan
mengandalkan filtering di frontend sebagai kontrol akses.

## Route families

| Family        | Contoh prefix            | Catatan                                     |
| ------------- | ------------------------ | ------------------------------------------- |
| Workspace API | `/api/workspaces/:slug/` | Ticket, service, intake, AI, settings       |
| Public API v1 | `/api/v1/`               | Work item, project, relation, activity, PQL |
| Auth          | `/api/auth/`             | Login, signup, token, session compatibility |
| Health        | `/health`                | Readiness API                               |
| Ingest        | `/api/inbound/`          | Webhook source dan metrics                  |

Daftar aktual: [`../_generated/api-routes.md`](../_generated/api-routes.md).
Jangan menyalin daftar endpoint ke dokumen lain.

## Pagination, filtering, sorting

Pagination tidak seragam:

- Sebagian endpoint memakai `page` dan `per_page`.
- Endpoint issue v2 memakai `cursor`; ukuran halaman dapat berada di dalam
  cursor.
- Endpoint version memakai cursor sendiri.

Filter dan sort mengikuti kontrak Django parity. Gunakan nama field yang sama
dengan response dan jangan mengarang parameter baru tanpa menambahkannya di
handler Rust.

## Errors and rate limits

- `400` untuk validasi, `401` untuk autentikasi, `403` untuk otorisasi, `404`
  untuk resource tidak ditemukan, `409` untuk konflik, `429` untuk rate limit.
- Body error utama berbentuk `{"error":"..."}`. Beberapa endpoint legacy
  mengembalikan pesan string untuk parity Django; client harus memeriksa status
  code dan bentuk body.
- Rate limiter token-bucket berjalan process-wide dengan kuota 600 request per
  menit; auth memiliki limiter IP terpisah. Excess langsung ditolak `429`.
- Origin check berlaku untuk mutasi; `WEB_BASE_URL`/origin yang diizinkan harus
  cocok dengan konfigurasi CORS.

## Source of truth

- Route API generated: [`../_generated/api-routes.md`](../_generated/api-routes.md)
- Arsitektur: [`01-architecture.md`](./01-architecture.md)
- Data model: [`02-data-model.md`](./02-data-model.md)

## Changelog

| Tanggal    | Perubahan                                                                   |
| ---------- | --------------------------------------------------------------------------- |
| 2026-09-03 | fork init                                                                   |
| 2026-09-05 | cutover Rust                                                                |
| 2026-10-27 | rewrite — JWT/cookie/API key, route families, cursor pagination, rate limit |
