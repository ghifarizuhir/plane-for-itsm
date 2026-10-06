# Seamless Ganti IP: Prod Full Tunnel Hostname (Admin/Space Ikut) — Design

Date: 2026-10-07
Status: Draft (pending user review)
Scope: Env runtime (`apps/api/.env`, `apps/live/.env`), env build (`apps/web|admin|space/.env`), systemd user unit baru (`plane-admin-prod`, `plane-space-prod`), remote ingress cloudflared. Tanpa perubahan kode aplikasi.
Terkait: `AGENTS.md` (Web Prod vs Dev, Live Server), `docs/superpowers/plans/2026-09-24-enable-live-server.md`.

## Tujuan

Ganti IP LAN host (sekarang `192.168.1.11`) tidak lagi memerlukan perubahan env, restart, atau rebuild apa pun untuk akses produksi. Semua URL prod memakai hostname tunnel yang stabil:

- Web: `https://dashboard.terraline.space`
- API: `https://api.terraline.space`
- Live (WebSocket): `https://api.terraline.space/live` (path, bukan subdomain)
- Admin (God Mode): `https://dashboard.terraline.space/god-mode`
- Space (share publik): `https://dashboard.terraline.space/spaces`

Rationale: tunnel cloudflared outbound ke `localhost`, tidak menyentuh IP LAN. Setelah semua referensi IP dibuang dari runtime prod, IP berubah = nol aksi.

## Konteks saat ini (temuan 2026-10-07)

Ingress tunnel (remote config, dibaca dari metrics `127.0.0.1:20241/config`):

| #   | Hostname                    | Path    | Service                 |
| --- | --------------------------- | ------- | ----------------------- |
| 1   | `api.terraline.space`       | `/live` | `http://localhost:3100` |
| 2   | `dashboard.terraline.space` | —       | `http://localhost:3000` |
| 3   | `api.terraline.space`       | —       | `http://localhost:8000` |
| 4   | (catch-all)                 | —       | `http_status:404`       |

- Path rule bersifat prefix: `https://api.terraline.space/live/health/` → 200 (rule path `/live`).
- Admin/space **tidak punya ingress** dan **tidak jalan** sebagai service; link dari web masih menunjuk `192.168.1.11:3001/3002`.
- Build prod web (`apps/web/build/client`) masih membekukan `192.168.1.11:3000/3001/3002` masing-masing 21×; `VITE_API_BASE_URL` & `VITE_LIVE_BASE_URL` sudah `https://api.terraline.space`.
- `apps/admin/.env` & `apps/space/.env` semuanya masih `192.168.1.11` (termasuk `VITE_API_BASE_URL`).
- `apps/live/.env` bebas IP saat ini, tapi `CORS_ALLOWED_ORIGINS` menyimpan IP basi `172.20.10.8:3000`.
- Service aktif: `plane-web-prod` (3000), `plane-live` (3100), `plane-backend` (docker: api/worker/beat-worker); `plane-web` (dev) nonaktif.
- CSRF origin check api-rs (`apps/api-rs/crates/api/src/middleware/origin.rs:29`) membandingkan header `Origin` dengan daftar allowed **secara persis** (string match, hanya trim quote/`/`). Karena itu `WEB_URL`/`ADMIN_BASE_URL`/`APP_BASE_URL` harus **origin saja tanpa path**.

## Keputusan (brainstormed & approved)

1. **Opsi A**: semua akses prod lewat hostname tunnel; LAN-IP langsung tidak lagi didukung untuk link antar-app (user tunnel-only).
2. **Admin & spaces ikut seamless** lewat path rule di hostname `dashboard.terraline.space` (tanpa subdomain baru, tanpa perubahan CORS karena origin tetap sama).
3. Env prod diubah ke hostname tunnel; rebuild sekali untuk web/admin/space; backend cukup recreate container.
4. Dev LAN tetap manual (di luar scope).

## Perubahan env

### `apps/api/.env` (runtime, dibaca `docker-compose-local.yml`)

| Variabel               | Sekarang                                                                                                                                                                                                                        | Target                                                                                                                      |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `CORS_ALLOWED_ORIGINS` | `http://localhost:3000,http://localhost:3001,http://localhost:3002,http://localhost:3100,http://192.168.1.11:3000,http://192.168.1.11:3001,http://192.168.1.11:3002,http://192.168.1.11:3100,https://dashboard.terraline.space` | `http://localhost:3000,http://localhost:3001,http://localhost:3002,http://localhost:3100,https://dashboard.terraline.space` |
| `WEB_URL`              | `http://192.168.1.11:8000` (typo)                                                                                                                                                                                               | `https://dashboard.terraline.space`                                                                                         |
| `ADMIN_BASE_URL`       | `http://192.168.1.11:3001`                                                                                                                                                                                                      | `https://dashboard.terraline.space`                                                                                         |
| `SPACE_BASE_URL`       | `http://192.168.1.11:3002`                                                                                                                                                                                                      | `https://dashboard.terraline.space`                                                                                         |
| `APP_BASE_URL`         | `http://192.168.1.11:3000`                                                                                                                                                                                                      | `https://dashboard.terraline.space`                                                                                         |
| `LIVE_BASE_URL`        | `http://192.168.1.11:3100`                                                                                                                                                                                                      | `https://api.terraline.space`                                                                                               |
| `FRONTEND_URL`         | `http://192.168.1.11:3000`                                                                                                                                                                                                      | `https://dashboard.terraline.space`                                                                                         |

Constraint: `WEB_URL`, `ADMIN_BASE_URL`, `APP_BASE_URL` origin-only (lihat Konteks). `FRONTEND_URL` juga origin (dipakai fallback redirect OAuth).

### `apps/web/.env` (build-time baked)

| Variabel              | Sekarang                   | Target                              |
| --------------------- | -------------------------- | ----------------------------------- |
| `VITE_WEB_BASE_URL`   | `http://192.168.1.11:3000` | `https://dashboard.terraline.space` |
| `VITE_ADMIN_BASE_URL` | `http://192.168.1.11:3001` | `https://dashboard.terraline.space` |
| `VITE_SPACE_BASE_URL` | `http://192.168.1.11:3002` | `https://dashboard.terraline.space` |

`VITE_API_BASE_URL`, `VITE_LIVE_BASE_URL`, dan semua `*_BASE_PATH` tidak berubah.

### `apps/admin/.env` dan `apps/space/.env`

| Variabel              | Target                              |
| --------------------- | ----------------------------------- |
| `VITE_API_BASE_URL`   | `https://api.terraline.space`       |
| `VITE_WEB_BASE_URL`   | `https://dashboard.terraline.space` |
| `VITE_ADMIN_BASE_URL` | `https://dashboard.terraline.space` |
| `VITE_SPACE_BASE_URL` | `https://dashboard.terraline.space` |
| `VITE_LIVE_BASE_URL`  | `https://api.terraline.space`       |

`*_BASE_PATH` (`/god-mode`, `/spaces`, `/live`) tidak berubah.

### `apps/live/.env`

`CORS_ALLOWED_ORIGINS` → `https://dashboard.terraline.space,https://terraline.space,http://localhost:3000` (buang `http://172.20.10.8:3000`). Variabel lain tidak berubah.

## Ingress Cloudflare (remote config)

Sisipkan 2 rule path **di atas** rule `dashboard.terraline.space → localhost:3000`:

| #   | Hostname                    | Path        | Service                 |
| --- | --------------------------- | ----------- | ----------------------- |
| 1   | `api.terraline.space`       | `/live`     | `http://localhost:3100` |
| 2   | `dashboard.terraline.space` | `/god-mode` | `http://localhost:3001` |
| 3   | `dashboard.terraline.space` | `/spaces`   | `http://localhost:3002` |
| 4   | `dashboard.terraline.space` | —           | `http://localhost:3000` |
| 5   | `api.terraline.space`       | —           | `http://localhost:8000` |
| 6   | (catch-all)                 | —           | `http_status:404`       |

Fallback jika UI dashboard menolak hostname duplikat: PUT `cfd_tunnel/{id}/configurations` dengan payload ingress lengkap di atas.

## Service baru (systemd user)

Mengikuti pola `~/.config/systemd/user/plane-web-prod.service`:

| Unit                       | Port | ExecStart                                                                                       |
| -------------------------- | ---- | ----------------------------------------------------------------------------------------------- |
| `plane-admin-prod.service` | 3001 | `npx -y serve@13 -s /home/ghifari/plane-for-itsm/apps/admin/build/client -l tcp://0.0.0.0:3001` |
| `plane-space-prod.service` | 3002 | `pnpm --filter=space start` (react-router-serve, `PORT=3002`, cwd repo)                         |

Keduanya `Restart=always`, `WantedBy=default.target`, `PATH` nvm seperti unit existing. `pnpm --filter=space start` menjalankan `react-router-serve ./build/server/index.js` dengan base `/spaces` yang sudah dibekukan saat build.

## Urutan implementasi

1. Edit env sesuai tabel di atas.
2. Build: `pnpm --filter=web build`, `pnpm --filter=admin build`, `pnpm --filter=space build`.
3. Buat + enable dua unit systemd baru (`daemon-reload`, `enable --now`).
4. Restart `plane-web-prod.service`.
5. Recreate backend agar env baru terbaca: `docker compose -f docker-compose-local.yml up -d api worker beat-worker`.
6. Restart `plane-live.service`.
7. Tambah ingress rules via dashboard/API.
8. Verifikasi.

## Verifikasi

- `curl -s https://dashboard.terraline.space/god-mode | grep -q god-mode` → HTML admin (bukan shell web).
- `curl -s https://dashboard.terraline.space/spaces/` → HTML space.
- `curl -s -o /dev/null -w '%{http_code}' https://api.terraline.space/live/health/` → 200.
- Login tunnel → buka Pages → edit dokumen → DevTools WS `wss://api.terraline.space/live/...` status 101.
- Mutasi UI (edit work item) sukses → origin `https://dashboard.terraline.space` lolos CSRF.
- `grep -r 192.168.1.11 apps/web/build/client apps/admin/build apps/space/build` → 0 hasil.
- `grep -rn '192.168.1.11' apps/*/.env` → 0 hasil.

## Non-goals

- Dev LAN (`pnpm dev`): env yang sama dipakai dev server; kalau mau dev via LAN IP tetap edit manual seperti sekarang.
- `apps/api-rs/scripts/smoke.sh`, test Rust/Django, dan `docs/**` yang menyebut IP: non-runtime, tidak disentuh.
- Perubahan kode aplikasi (fallback `window.location.origin` di `packages/constants` tetap seperti adanya, tidak dipakai).
- Migrasi dari plain HTTP LAN; tidak ada perubahan cookie/`COOKIE_SECURE` di luar env yang sudah ada.

## Risiko & catatan operasional

- **Duplikat hostname di UI Cloudflare**: jika ditolak, pakai fallback API (lihat Ingress). Perlu account API token; tunnel token saja tidak cukup.
- **Konflik port saat dev**: `pnpm dev` memakai 3001/3002 → stop `plane-admin-prod`/`plane-space-prod` dulu, analog aturan port 3000.
- **SPA fallback admin**: `serve -s` melempar path tak dikenal ke `index.html`; pola upstream Plane memang `/god-mode/* → admin:3000`, jadi perilaku ini sudah teruji di desain upstream. Verifikasi manual tetap dilakukan.
- **Rollback**: kembalikan nilai env, rebuild tiga app, hapus dua ingress rule, stop dua unit baru. Tidak ada perubahan data.

## Definisi selesai

- Tidak ada referensi `192.168.1.11` di env runtime/build prod.
- Admin, spaces, web, api, live reachable via hostname tunnel masing-masing.
- Simulasi ganti IP (mis. mengubah IP host) tidak menuntut edit/rebuild/restart apa pun untuk akses tunnel.
