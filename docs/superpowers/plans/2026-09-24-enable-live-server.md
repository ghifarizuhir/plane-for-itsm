# Enable Live Server (apps/live) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menjalankan server kolaborasi realtime `apps/live` (Hocuspocus/Yjs) di port 3100 sebagai systemd user service yang terhubung ke api-rs + Redis, sehingga editor Pages di web prod punya kolaborasi realtime via LAN, dengan opsi akses via Cloudflare Tunnel.

**Architecture:** `apps/live` adalah service Node mandiri (build tsdown → `dist/start.mjs`) yang membaca `apps/live/.env`. Ia memanggil api-rs di `localhost:8000` untuk autentikasi user (`/api/users/me/`) dan baca/tulis dokumen halaman (`/api/workspaces/:slug/projects/:pid/pages/:page_id/` + `/description/`), serta memakai Redis container `plane-redis` di `localhost:6379` untuk state kolaborasi. Dijalankan sebagai `plane-live.service` (systemd user), di-enable individual seperti service plane lain. Web prod (port 3000) sudah dibuild dengan `VITE_LIVE_BASE_URL=http://192.168.1.11:3100`, jadi verifikasi LAN tidak perlu rebuild.

**Tech Stack:** Node 24 + pnpm (corepack) + tsdown, Express + Hocuspocus + ioredis, systemd user unit, api-rs (Rust, docker-compose-local), Redis (valkey container), web prod (`serve`, port 3000).

**Fakta yang sudah diverifikasi (2026-09-24):**

- api-rs `GET http://localhost:8000/health` → 200; `docker exec plane-redis redis-cli ping` → `PONG`; port 3100 bebas.
- `pnpm --filter=live check:types` lulus (tsc bersih).
- Build web prod saat ini sudah memuat `192.168.1.11:3100` (21 kemunculan di `apps/web/build/client/assets/*.js`).
- Endpoint api-rs yang dibutuhkan live sudah ada: page detail + description GET/PATCH (`apps/api-rs/crates/api/src/main.rs:1005-1022`), `/api/users/me/` (`apps/api-rs/crates/api/src/main.rs:1471`).
- Health live: `GET /live/health/` (`apps/live/src/controllers/health.controller.ts`).
- WS editor: `/live/collaboration` (`apps/web/core/components/pages/editor/editor-body.tsx:202`).
- `apps/live/.env` sudah berisi `PORT=3100`, `API_BASE_URL=http://localhost:8000`, `REDIS_URL=redis://localhost:6379/`, `LIVE_BASE_PATH=/live`, `LIVE_SERVER_SECRET_KEY=secret-key`.
- `apps/live/.env` dan `apps/live/dist` keduanya gitignored.

**Repo conventions yang wajib diikuti:**

- Unit systemd sumbernya di `systemd/user/` (repo), lalu disalin ke `~/.config/systemd/user/`.
- Service yang berjalan di-enable individual (symlink di `default.target.wants/`); `plane.target` ada tetapi `disabled`, jadi `enable plane-live.service` adalah langkah yang menentukan.
- Jangan sentuh 7 modifikasi pre-existing di working tree (`apps/admin/vite.config.ts`, `apps/api-rs/crates/api/src/routes/auth.rs`, `apps/api/plane/tests/unit/utils/test_host.py`, `apps/space/vite.config.ts`, `apps/web/app/root.tsx`, `apps/web/vite.config.ts`, `packages/constants/src/metadata.ts`); stage hanya file task.
- Commit per task; jangan `--no-verify`.

## File Map

| File                                        | Aksi             | Tanggung jawab                                |
| ------------------------------------------- | ---------------- | --------------------------------------------- |
| `systemd/user/plane-live.service`           | Create           | unit service live (port 3100)                 |
| `systemd/user/plane.target`                 | Modify           | tambah `plane-live.service` di Wants/After    |
| `~/.config/systemd/user/plane-live.service` | Create (copy)    | unit aktif di host                            |
| `~/.config/systemd/user/plane.target`       | Modify (copy)    | target aktif di host                          |
| `apps/live/.env`                            | Modify (ignored) | tambah `CORS_ALLOWED_ORIGINS` + `APP_VERSION` |
| `AGENTS.md`                                 | Modify           | dokumentasi operasional live                  |

Tidak ada perubahan kode aplikasi.

---

### Task 1: Preflight (read-only)

**Files:** none

- [ ] **Step 1: Pastikan container backend, db, dan redis jalan**

Run:

```bash
docker ps --format '{{.Names}}\t{{.Status}}' | grep -E 'api-rs|plane-redis|plane-db'
```

Expected: `api-rs ... Up`, `plane-redis ... Up (healthy)`, `plane-db ... Up (healthy)`.

- [ ] **Step 2: api-rs sehat**

Run:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200`

- [ ] **Step 3: Redis sehat**

Run:

```bash
docker exec plane-redis redis-cli ping
```

Expected: `PONG`

- [ ] **Step 4: Port 3100 bebas**

Run:

```bash
ss -ltnp | grep 3100 || echo FREE
```

Expected: `FREE`

- [ ] **Step 5: Env live lengkap**

Run:

```bash
grep -E '^(API_BASE_URL|REDIS_URL|LIVE_SERVER_SECRET_KEY|LIVE_BASE_PATH|PORT)=' apps/live/.env
```

Expected: kelima baris muncul (`API_BASE_URL="http://localhost:8000"`, `REDIS_URL="redis://localhost:6379/"`, `LIVE_SERVER_SECRET_KEY="secret-key"`, `LIVE_BASE_PATH="/live"`, `PORT=3100`).

- [ ] **Step 6: Web prod sudah menunjuk live LAN**

Run:

```bash
grep -rho '192.168.1.11:3100' apps/web/build/client/assets/*.js | wc -l
```

Expected: `21` (yang penting > 0).

Tidak ada perubahan file; tidak ada commit.

---

### Task 2: Build apps/live

**Files:**

- Build artifact: `apps/live/dist/start.mjs` (gitignored)

- [ ] **Step 1: Build**

Run:

```bash
pnpm --filter=live build
```

Expected: exit 0; `tsc --noEmit` tanpa error; tsdown menulis `dist/start.mjs` tanpa warning fatal.

- [ ] **Step 2: Verifikasi artifact**

Run:

```bash
ls -la apps/live/dist/start.mjs
```

Expected: file ada dengan mtime hari ini (bukan Sep 11).

Tidak ada commit (`dist` di-gitignore).

---

### Task 3: Smoke test foreground

**Files:** none (hanya proses sementara)

- [ ] **Step 1: Jalankan live di background sementara**

Menjalankan `node` langsung (persis perintah yang dipakai `pnpm --filter=live start`, tanpa wrapper pnpm agar sinyal SIGTERM sampai ke proses node):

Run:

```bash
cd /home/ghifari/plane-for-itsm/apps/live
node --env-file=.env . > /tmp/live-smoke.log 2>&1 &
echo $! > /tmp/live-smoke.pid
sleep 4
```

Expected: proses hidup (PID tersimpan).

- [ ] **Step 2: Cek health**

Run:

```bash
curl -s http://localhost:3100/live/health/
```

Expected: `{"status":"OK","timestamp":"...","version":"1.0.0"}`

- [ ] **Step 3: Cek log inisialisasi**

Run:

```bash
grep -E 'Redis setup completed|Hocuspocus setup completed|Express server has started at port 3100' /tmp/live-smoke.log
```

Expected: ketiga baris ada.

- [ ] **Step 4: Matikan dan cek shutdown graceful**

Run:

```bash
kill $(cat /tmp/live-smoke.pid); sleep 2
grep -iE 'graceful|SIGTERM' /tmp/live-smoke.log
```

Expected: minimal satu baris shutdown graceful (`SERVER: Express server closed gracefully.`).

- [ ] **Step 5: Pastikan port kembali bebas**

Run:

```bash
ss -ltnp | grep 3100 || echo FREE
```

Expected: `FREE`

---

### Task 4: Buat systemd unit

**Files:**

- Create: `systemd/user/plane-live.service`
- Modify: `systemd/user/plane.target`
- Create (host): `~/.config/systemd/user/plane-live.service`
- Modify (host): `~/.config/systemd/user/plane.target`

- [ ] **Step 1: Buat `systemd/user/plane-live.service`**

Isi lengkap file:

```ini
[Unit]
Description=Plane ITSM Live (3100) - pnpm --filter=live start
After=plane-backend.service network-online.target
Wants=plane-backend.service
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/home/ghifari/plane-for-itsm
Environment=NODE_ENV=production
Environment=PATH=/home/ghifari/.nvm/versions/node/v24.16.0/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=/home/ghifari/.nvm/versions/node/v24.16.0/bin/pnpm --filter=live start
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=plane-live
TimeoutStopSec=30

[Install]
WantedBy=default.target
```

- [ ] **Step 2: Update `systemd/user/plane.target`**

Isi lengkap file setelah perubahan:

```ini
[Unit]
Description=Plane ITSM Full Stack
Wants=plane-backend.service plane-web.service plane-admin.service plane-live.service
After=plane-backend.service plane-web.service plane-admin.service plane-live.service

[Install]
WantedBy=default.target
```

- [ ] **Step 3: Commit**

Run:

```bash
git add systemd/user/plane-live.service systemd/user/plane.target
git commit -m "chore(systemd): add plane-live service unit"
```

- [ ] **Step 4: Salin ke host dan reload**

Run:

```bash
cp systemd/user/plane-live.service systemd/user/plane.target ~/.config/systemd/user/
systemctl --user daemon-reload
```

Expected: tanpa error.

- [ ] **Step 5: Start dan enable**

Run:

```bash
systemctl --user start plane-live.service
systemctl --user enable plane-live.service
systemctl --user is-active plane-live.service
```

Expected: `active`

---

### Task 5: Verifikasi service

**Files:** none

- [ ] **Step 1: Status dan log inisialisasi**

Run:

```bash
systemctl --user status plane-live.service --no-pager | head -12
journalctl --user -u plane-live -n 30 --no-pager | grep -E 'Redis setup completed|Hocuspocus setup completed|Express server has started at port 3100'
```

Expected: status `active (running)`; ketiga baris log ada.

- [ ] **Step 2: Health dari LAN**

Run:

```bash
curl -s http://192.168.1.11:3100/live/health/
```

Expected: `{"status":"OK",...}` (jika firewall host memblokir, `http://localhost:3100/live/health/` juga sah).

- [ ] **Step 3: Restart resilience**

Run:

```bash
systemctl --user restart plane-live.service
sleep 3
systemctl --user is-active plane-live.service
```

Expected: `active`

- [ ] **Step 4: Symlink enable ada**

Run:

```bash
ls -l ~/.config/systemd/user/default.target.wants/plane-live.service
```

Expected: symlink ke `plane-live.service` ada.

---

### Task 6: E2E kolaborasi (LAN)

**Files:** none

- [ ] **Step 1: Buka halaman Pages**

Buka `http://192.168.1.11:3000`, login, pilih project → **Pages** → buka (atau buat) satu halaman dokumen.

- [ ] **Step 2: Cek koneksi WebSocket**

DevTools → Network → filter `WS` → refresh halaman.
Expected: request `ws://192.168.1.11:3100/live/collaboration?...` dengan status **101 Switching Protocols**.

- [ ] **Step 3: Uji kolaborasi dua user**

Buka browser/incognito kedua dengan user berbeda, buka halaman yang sama, ketik di keduanya.
Expected: teks tersinkron dalam ~1 detik; indikator user lain muncul di editor.

- [ ] **Step 4: Log bersih**

Run:

```bash
journalctl --user -u plane-live -n 50 --no-pager | grep -iE 'error|unauthorized' || echo CLEAN
```

Expected: `CLEAN` (error sporadis boleh dicatat, tapi tidak boleh ada `AUTH_MISSING_CREDENTIALS` berulang).

- [ ] **Step 5: Uji fallback tanpa live**

Run:

```bash
systemctl --user stop plane-live.service
```

Lalu edit halaman di browser dan tunggu autosave.
Expected: DevTools Network menunjukkan `PATCH .../pages/<page_id>/description/` → 200 (fallback `apps/web/core/hooks/use-page-fallback.ts`), editor tidak kehilangan perubahan. Lalu:

```bash
systemctl --user start plane-live.service
```

Expected: `active`.

---

### Task 7 (Opsional): Akses via Cloudflare Tunnel

Tunnel dikelola remote (token di `~/.cloudflared/tunnel-token`), jadi ingress ditambah lewat dashboard Zero Trust, bukan file lokal.

**Files:**

- Modify: `apps/live/.env` (gitignored)

- [ ] **Step 1 (aksi user): Tambah public hostname di dashboard Cloudflare**

Cloudflare Zero Trust → Networks → Tunnels → pilih tunnel terraline → Public Hostname → Add:

- Subdomain: `live`, Domain: `terraline.space`
- Service: `HTTP` → `localhost:3100`
  Simpan. (Alternatif: path rule `terraline.space/live` → `http://localhost:3100`.)

- [ ] **Step 2: Tambah CORS + versi di `apps/live/.env`**

Tambahkan baris berikut ke `apps/live/.env`:

```
CORS_ALLOWED_ORIGINS="https://terraline.space,http://192.168.1.11:3000,http://localhost:3000"
APP_VERSION="1.0.0"
```

Lalu:

```bash
systemctl --user restart plane-live.service
```

- [ ] **Step 3: Verifikasi health dari publik**

Run:

```bash
curl -s https://live.terraline.space/live/health/
```

Expected: `{"status":"OK",...}`

- [ ] **Step 4: Rebuild web prod dengan URL live publik**

Run:

```bash
cd /home/ghifari/plane-for-itsm
VITE_API_BASE_URL=https://api.terraline.space VITE_LIVE_BASE_URL=https://live.terraline.space pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build sukses; service `active`.
Catatan: env inline menang atas `.env` (dotenv tidak menimpa env yang sudah ada).

- [ ] **Step 5: Verifikasi WSS dari tunnel**

Buka `https://terraline.space`, buka halaman Pages, DevTools → Network → WS.
Expected: `wss://live.terraline.space/live/collaboration?...` status **101**.

---

### Task 8: Dokumentasi AGENTS.md

**Files:**

- Modify: `AGENTS.md`

- [ ] **Step 1: Tambah section "Live Server (port 3100)"**

Sisipkan setelah bagian "Web Prod vs Dev (port 3000)":

```markdown
## Live Server (port 3100)

Server kolaborasi realtime Pages (Hocuspocus/Yjs) untuk editor dokumen.

- Service: `plane-live.service` (`systemctl --user status plane-live`).
- Env: `apps/live/.env` (API_BASE_URL ke api-rs, REDIS_URL ke container plane-redis).
- Setelah mengubah kode `apps/live`: `pnpm --filter=live build && systemctl --user restart plane-live.service`.
- Verifikasi: `curl http://localhost:3100/live/health/`.
- Web prod hanya bisa connect `ws://` saat diakses dari LAN (build memakai `VITE_LIVE_BASE_URL=http://192.168.1.11:3100`); via tunnel butuh ingress `live.terraline.space` + rebuild dengan URL https.
```

- [ ] **Step 2: Commit**

Run:

```bash
git add AGENTS.md
git commit -m "docs: document live server operations"
```

---

## Known Limitations

1. `GET .../pages/:id/mentions/` tidak ada di api-rs (dan juga tidak ada di Django versi ini); hanya memengaruhi metadata @mention saat PDF export (`apps/live/src/services/pdf-export/pdf-export.service.ts:117`). Kolaborasi editor tidak terpengaruh.
2. PDF export komunitas tidak memanggil endpoint HTTP live dari browser (hanya WebSocket), jadi `CORS_ALLOWED_ORIGINS` bukan blocker inti.
3. Tanpa live, Pages tetap bisa dipakai: editor jatuh ke fallback save per-user (`apps/web/core/hooks/use-page-fallback.ts`).
4. Build web prod saat ini (LAN) menunjuk `http://192.168.1.11:3100`; akses via tunnel sebelum Task 7 akan gagal karena mixed content (`wss://` ke port tanpa TLS).

## Rollback

```bash
systemctl --user stop plane-live.service
systemctl --user disable plane-live.service
rm ~/.config/systemd/user/plane-live.service
systemctl --user daemon-reload
```

Unit di repo tetap ada; hapus file + revert commit `chore(systemd): add plane-live service unit` bila ingin membersihkan sepenuhnya.
