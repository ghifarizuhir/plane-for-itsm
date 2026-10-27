---
status: current
last_verified: 2026-10-27
source_hash: sha256:772e0a109d60beb73e118e160e0c59a86adf498544c2761e19656330ac5d5f80
source_paths:
  - AGENTS.md
  - docker-compose-local.yml
  - docker-compose.yml
  - systemd/user/plane-live.service
  - apps/api/.env.example
---

# 06 — Ops Runbook

Status: **Current.**

## Local services

| Service      | Port | Catatan                                                     |
| ------------ | ---- | ----------------------------------------------------------- |
| Web prod     | 3000 | Static build di `apps/web/build/client`; memakai API tunnel |
| Web dev      | 3000 | Vite dev; hanya satu dari prod/dev yang boleh aktif         |
| Admin        | 3001 | God-mode                                                    |
| API          | 8000 | `apps/api-rs` di Docker Compose lokal                       |
| Live         | 3100 | Hocuspocus/Yjs                                              |
| PostgreSQL   | 5432 | Container `plane-db`                                        |
| Redis/Valkey | 6379 | Container `plane-redis`                                     |

Jangan menjalankan web prod dan web dev bersamaan di port 3000.

## Backend stack

Stack backend dikelola `docker-compose-local.yml` melalui systemd
`plane-backend.service`, bukan `docker-compose.yml` root. Keduanya memakai
project Compose yang sama; jangan menjalankan dua file berbeda saat stack aktif.

```bash
# Rebuild api, worker, dan beat setelah perubahan apps/api-rs
docker compose -f docker-compose-local.yml up -d --build api worker beat-worker

# Verifikasi
curl http://localhost:8000/health
```

Build Rust memakai LTO dan dapat memakan 10+ menit tanpa output. Jalankan
detached ke file log dan poll; jangan menganggap proses menggantung.

## Web prod vs dev

Default adalah web prod (`plane-web-prod.service`). Setelah perubahan kode web:

```bash
pnpm turbo run build --filter=web
systemctl --user restart plane-web-prod.service
```

Jangan memakai `pnpm --filter=web build` untuk prod karena dapat membundel
`packages/*/dist` yang stale. Untuk kerja lokal:

```bash
systemctl --user stop plane-web-prod.service
systemctl --user start plane-web.service
```

Dev membutuhkan `VITE_API_BASE_URL=http://<host>:8000` di `apps/web/.env` agar
cookie SameSite tidak memicu login loop.

## Live server

```bash
pnpm turbo run build --filter=live
systemctl --user restart plane-live.service
curl http://localhost:3100/live/health/
```

`WEB_BASE_URL` di env live dikirim sebagai `Origin` pada mutasi API dan harus
ada di `CORS_ALLOWED_ORIGINS` (`apps/api/.env`). `VITE_LIVE_BASE_URL` di-bake
saat build web; untuk LAN gunakan URL yang dapat dijangkau browser, untuk tunnel
gunakan `https` agar tidak kena mixed content.

## Tunnel

- Jangan mengekspos Vite dev lewat tunnel tanpa cache-bypass; edge cache dapat
  mencampur optimizer generation dan memicu error React.
- Jangan menghapus `apps/web/node_modules/.vite` untuk "memperbaiki" cache;
  itu mereset optimizer dan biasanya memperburuk keadaan.
- Tunnel membutuhkan ingress terpisah untuk web, API, dan live.

## Environment

- `apps/api/.env` dibaca oleh api-rs dan fallback Django.
- `apps/live/.env` berisi `API_BASE_URL`, `REDIS_URL`, `LIVE_SERVER_SECRET_KEY`,
  dan `WEB_BASE_URL`.
- `CORS_ALLOWED_ORIGINS` harus memuat origin web dan live yang dipakai.
- Secret produksi tidak boleh memakai placeholder dari `.env.example`.

## Recovery

1. Cek `systemctl --user status plane-backend plane-live plane-web-prod`.
2. Cek `docker compose -f docker-compose-local.yml ps`.
3. Cek `/health` API dan `/live/health/`.
4. Jika `plane-backend.service` failed, jalankan
   `systemctl --user reset-failed plane-backend.service` lalu start ulang.
5. Setelah container backend di-recreate, restart `plane-live.service` karena
   koneksi Redis ikut terputus.

## Changelog

| Tanggal    | Perubahan                                                |
| ---------- | -------------------------------------------------------- |
| 2026-09-03 | fork init                                                |
| 2026-09-05 | cutover Rust                                             |
| 2026-10-27 | rewrite — api-rs local stack, prod/dev web, live, tunnel |
