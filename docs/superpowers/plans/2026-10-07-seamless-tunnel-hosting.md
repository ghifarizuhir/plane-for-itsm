# Seamless Tunnel Hosting (Prod Bebas IP LAN) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Semua akses produksi (web, api, live, admin, spaces) memakai hostname tunnel stabil, sehingga ganti IP LAN tidak memerlukan edit env/rebuild/restart apa pun.

**Architecture:** cloudflared (remote-managed) meneruskan `dashboard.terraline.space` → `localhost:3000` (web), `api.terraline.space` → `localhost:8000` (api-rs), dengan dua path rule baru `dashboard.terraline.space/god-mode` → `localhost:3001` (admin) dan `/spaces` → `localhost:3002` (space). Semua env prod (runtime `apps/api/.env` + build `apps/web|admin|space/.env`) diarahkan ke hostname tunnel. Admin/space di-serve sebagai build produksi via systemd user unit baru.

**Tech Stack:** cloudflared remote tunnel, systemd user units, pnpm + Vite/React Router build, `vite preview` (admin, base `/god-mode`), `react-router-serve` (space, base `/spaces`), docker compose (`docker-compose-local.yml`).

**Spec:** `docs/superpowers/specs/2026-10-07-seamless-tunnel-hosting-design.md`

**Prasyarat:** stack docker jalan (`plane-backend.service`), `plane-web-prod.service` aktif, cloudflared aktif dengan metrics `127.0.0.1:20241`, node nvm v24.16.0 di `/home/ghifari/.nvm/versions/node/v24.16.0/bin`.

**Catatan commit:** `apps/*/.env` **gitignored** — perubahan env tidak bisa/tidak perlu di-commit; commit hanya untuk unit systemd dan `AGENTS.md`.

---

## Struktur file

| File                                    | Aksi               | Tanggung jawab                                 |
| --------------------------------------- | ------------------ | ---------------------------------------------- |
| `apps/api/.env`                         | Modify (untracked) | Origin/CORS + base URL runtime api-rs          |
| `apps/live/.env`                        | Modify (untracked) | Hapus origin basi live server                  |
| `apps/web/.env`                         | Modify (untracked) | Env build web prod                             |
| `apps/admin/.env`                       | Modify (untracked) | Env build admin prod                           |
| `apps/space/.env`                       | Modify (untracked) | Env build space prod                           |
| `systemd/user/plane-admin-prod.service` | Create (tracked)   | Serve admin build (3001)                       |
| `systemd/user/plane-space-prod.service` | Create (tracked)   | Serve space build (3002)                       |
| `AGENTS.md`                             | Modify (tracked)   | Dokumentasi service prod baru + aturan IP-free |

---

### Task 1: Runtime env backend + live

**Files:**

- Modify: `apps/api/.env` (lines 4, 48, 54, 57, 60, 63, 76)
- Modify: `apps/live/.env` (line 12)

- [ ] **Step 1: Ubah `apps/api/.env`**

Ganti tepat baris-baris ini:

```
CORS_ALLOWED_ORIGINS="http://localhost:3000,http://localhost:3001,http://localhost:3002,http://localhost:3100,https://dashboard.terraline.space"
WEB_URL="https://dashboard.terraline.space"
ADMIN_BASE_URL="https://dashboard.terraline.space"
SPACE_BASE_URL="https://dashboard.terraline.space"
APP_BASE_URL="https://dashboard.terraline.space"
LIVE_BASE_URL="https://api.terraline.space"
FRONTEND_URL="https://dashboard.terraline.space"
```

(`WEB_URL` saat ini `http://192.168.1.11:8000` — itu typo lama. `ADMIN_BASE_URL`/`APP_BASE_URL`/`WEB_URL` **wajib origin saja tanpa path**, karena CSRF origin-check membandingkan string persis.)

- [ ] **Step 2: Ubah `apps/live/.env`**

Ganti baris `CORS_ALLOWED_ORIGINS` (buang IP basi `172.20.10.8:3000`):

```
CORS_ALLOWED_ORIGINS="https://dashboard.terraline.space,https://terraline.space,http://localhost:3000"
```

- [ ] **Step 3: Recreate container backend agar env baru terbaca**

Run: `docker compose -f docker-compose-local.yml up -d api worker beat-worker`

Expected: `Container plane-for-itsm-api-1 Recreated` / `Started` (worker, beat-worker serupa), exit code 0.

- [ ] **Step 4: Verifikasi env terpasang di container**

Run: `docker compose -f docker-compose-local.yml exec -T api printenv APP_BASE_URL FRONTEND_URL LIVE_BASE_URL CORS_ALLOWED_ORIGINS`

Expected (urutan bisa beda):

```
https://dashboard.terraline.space
https://dashboard.terraline.space
https://api.terraline.space
http://localhost:3000,http://localhost:3001,http://localhost:3002,http://localhost:3100,https://dashboard.terraline.space
```

- [ ] **Step 5: Health check api**

Run: `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health`

Expected: `200` (jika belum, tunggu ~10 detik lalu ulangi).

- [ ] **Step 6: Restart live (memegang koneksi Redis yang ikut ter-recreate) + health**

Run:

```bash
systemctl --user restart plane-live.service
for i in $(seq 1 20); do code=$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3100/live/health/); [ "$code" = "200" ] && break; sleep 1; done
echo "live health: $code"
```

Expected: `live health: 200` (cold start ~8 detik).

- [ ] **Step 7: Commit**

Tidak ada commit — `apps/*/.env` gitignored.

---

### Task 2: Env build frontend + rebuild + restart web prod

**Files:**

- Modify: `apps/web/.env`
- Modify: `apps/admin/.env`
- Modify: `apps/space/.env`

- [ ] **Step 1: Tulis ulang `apps/web/.env` menjadi**

```
VITE_API_BASE_URL="https://api.terraline.space"

VITE_WEB_BASE_URL="https://dashboard.terraline.space"

VITE_ADMIN_BASE_URL="https://dashboard.terraline.space"
VITE_ADMIN_BASE_PATH="/god-mode"

VITE_SPACE_BASE_URL="https://dashboard.terraline.space"
VITE_SPACE_BASE_PATH="/spaces"

VITE_LIVE_BASE_URL="https://api.terraline.space"
VITE_LIVE_BASE_PATH="/live"
```

- [ ] **Step 2: Tulis ulang `apps/admin/.env` dan `apps/space/.env` (isinya sama) menjadi**

```
VITE_API_BASE_URL="https://api.terraline.space"

VITE_WEB_BASE_URL="https://dashboard.terraline.space"

VITE_ADMIN_BASE_URL="https://dashboard.terraline.space"
VITE_ADMIN_BASE_PATH="/god-mode"

VITE_SPACE_BASE_URL="https://dashboard.terraline.space"
VITE_SPACE_BASE_PATH="/spaces"

VITE_LIVE_BASE_URL="https://api.terraline.space"
VITE_LIVE_BASE_PATH="/live"
```

- [ ] **Step 3: Build web**

Run: `pnpm --filter=web build`

Expected: build sukses (beberapa menit), exit 0.

- [ ] **Step 4: Build admin**

Run: `pnpm --filter=admin build`

Expected: build sukses, `apps/admin/build/client/index.html` ter-update.

- [ ] **Step 5: Build space**

Run: `pnpm --filter=space build`

Expected: build sukses, `apps/space/build/server/index.js` + `apps/space/build/client` ter-update.

- [ ] **Step 6: Restart web prod**

Run: `systemctl --user restart plane-web-prod.service && sleep 2 && curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000/`

Expected: `200`.

- [ ] **Step 7: Verifikasi tidak ada IP LAN di build**

Run: `grep -rn '192\.168\.1\.11' apps/web/build/client apps/admin/build apps/space/build | wc -l`

Expected: `0`.

Run: `grep -rl 'dashboard.terraline.space' apps/web/build/client/assets | head -3`

Expected: minimal 1 file (aset web memuat hostname tunnel).

Run: `grep -rl 'api.terraline.space' apps/admin/build/client/assets | head -3`

Expected: minimal 1 file.

- [ ] **Step 8: Commit**

Tidak ada commit — env & build output gitignored.

---

### Task 3: Systemd unit prod admin & space

**Files:**

- Create: `systemd/user/plane-admin-prod.service`
- Create: `systemd/user/plane-space-prod.service`

- [ ] **Step 1: Tulis `systemd/user/plane-admin-prod.service`**

```ini
[Unit]
Description=Plane ITSM Admin Prod (3001) - vite preview (production)
After=network-online.target
Wants=plane-backend.service
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/home/ghifari/plane-for-itsm
Environment=NODE_ENV=production
Environment=PATH=/home/ghifari/.nvm/versions/node/v24.16.0/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=/home/ghifari/.nvm/versions/node/v24.16.0/bin/pnpm --filter=admin exec vite preview --port 3001 --host 0.0.0.0
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=plane-admin-prod
TimeoutStopSec=30

[Install]
WantedBy=default.target
```

- [ ] **Step 2: Tulis `systemd/user/plane-space-prod.service`**

```ini
[Unit]
Description=Plane ITSM Space Prod (3002) - react-router-serve (production)
After=network-online.target
Wants=plane-backend.service
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/home/ghifari/plane-for-itsm
Environment=NODE_ENV=production
Environment=PATH=/home/ghifari/.nvm/versions/node/v24.16.0/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=/home/ghifari/.nvm/versions/node/v24.16.0/bin/pnpm --filter=space start
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=plane-space-prod
TimeoutStopSec=30

[Install]
WantedBy=default.target
```

- [ ] **Step 3: Matikan dev admin (memakai port 3001 yang sama)**

Run: `systemctl --user disable --now plane-admin.service`

Expected: `Removed "/home/ghifari/.config/systemd/user/default.target.wants/plane-admin.service".` (atau status inactive). `plane-space.service` sudah disabled/inactive — tidak perlu aksi.

- [ ] **Step 4: Pasang unit ke config systemd dan reload**

Run: `cp systemd/user/plane-admin-prod.service systemd/user/plane-space-prod.service ~/.config/systemd/user/ && systemctl --user daemon-reload`

Expected: tanpa error.

- [ ] **Step 5: Enable + start kedua unit**

Run: `systemctl --user enable --now plane-admin-prod.service plane-space-prod.service`

Expected: `Created symlink .../default.target.wants/plane-admin-prod.service` dan `.../plane-space-prod.service`.

- [ ] **Step 6: Verifikasi service dan endpoint lokal**

Run:

```bash
systemctl --user is-active plane-admin-prod.service plane-space-prod.service
for i in $(seq 1 25); do a=$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3001/god-mode/); s=$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3002/spaces/); [ "$a" = "200" ] && [ "$s" = "200" ] && break; sleep 1; done
echo "admin=$a space=$s"
```

Expected: baris pertama `active` dua kali; `admin=200 space=200` (vite preview butuh ~10 detik saat start).

Run: `curl -s http://localhost:3001/god-mode/ | grep -c '/god-mode/assets/'`

Expected: angka ≥ 1.

Run: `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3001/god-mode/issues/list`

Expected: `200` (fallback SPA).

- [ ] **Step 7: Commit**

```bash
git add systemd/user/plane-admin-prod.service systemd/user/plane-space-prod.service
git commit -m "chore(systemd): add admin and space prod units"
```

---

### Task 4: Ingress Cloudflare (path rules admin & space)

**Files:**

- Modify: konfigurasi remote tunnel `c13ef874-ab66-43a8-a238-a235b676082b` (bukan file repo)

- [ ] **Step 1: Tambah dua published application route via dashboard**

Cloudflare Zero Trust dashboard → **Networks → Tunnels** → pilih tunnel `c13ef874-ab66-43a8-a238-a235b676082b` → **Routes** (Published application) → **Add route**:

1. Subdomain `dashboard`, Domain `terraline.space`, Path `/god-mode`, Service `http://localhost:3001` → Save
2. Subdomain `dashboard`, Domain `terraline.space`, Path `/spaces`, Service `http://localhost:3002` → Save

Pastikan di daftar/config, dua rule berpath muncul **sebelum** rule `dashboard.terraline.space` tanpa path (catch-all web).

Jika dashboard menolak hostname duplikat / tidak bisa mengatur urutan, pakai fallback API di Step 2.

- [ ] **Step 2 (fallback): PUT konfigurasi ingress via API**

Buat payload lengkap (urutan penting — path rule dulu):

Run:

```bash
cat > /tmp/opencode/tunnel-ingress.json <<'EOF'
{
  "config": {
    "ingress": [
      { "hostname": "api.terraline.space", "path": "/live", "service": "http://localhost:3100" },
      { "hostname": "dashboard.terraline.space", "path": "/god-mode", "service": "http://localhost:3001" },
      { "hostname": "dashboard.terraline.space", "path": "/spaces", "service": "http://localhost:3002" },
      { "hostname": "dashboard.terraline.space", "service": "http://localhost:3000" },
      { "hostname": "api.terraline.space", "service": "http://localhost:8000" },
      { "service": "http_status:404" }
    ]
  }
}
EOF
curl -s -X PUT "https://api.cloudflare.com/client/v4/accounts/b48e54504cb8801351b582c86007f6c8/cfd_tunnel/c13ef874-ab66-43a8-a238-a235b676082b/configurations" \
  -H "Authorization: Bearer $CF_API_TOKEN" -H "Content-Type: application/json" \
  --data @/tmp/opencode/tunnel-ingress.json | python3 -c "import json,sys; d=json.load(sys.stdin); print('success:', d['success'])"
```

Expected: `success: True` (butuh `CF_API_TOKEN` dengan izin Cloudflare Tunnel edit; jika tidak ada token, kembali ke UI dashboard).

- [ ] **Step 3: Verifikasi config ter-push ke cloudflared lokal**

Run:

```bash
sleep 10
curl -s http://127.0.0.1:20241/config | python3 -c "import json,sys; [print(i+1, r.get('hostname'), r.get('path'), r['service']) for i,r in enumerate(json.load(sys.stdin)['config']['ingress'])]"
```

Expected (urutan persis):

```
1 api.terraline.space /live http://localhost:3100
2 dashboard.terraline.space /god-mode http://localhost:3001
3 dashboard.terraline.space /spaces http://localhost:3002
4 dashboard.terraline.space None http://localhost:3000
5 api.terraline.space None http://localhost:8000
6 None None http_status:404
```

Jika urutan berbeda (path rule setelah rule tanpa path), perbaiki urutannya.

- [ ] **Step 4: Verifikasi remote**

Run:

```bash
for p in "/god-mode/" "/spaces/"; do printf "%-12s " "$p"; curl -s -o /dev/null -w '%{http_code}\n' "https://dashboard.terraline.space$p"; done
curl -s -o /dev/null -w 'live: %{http_code}\n' https://api.terraline.space/live/health/
```

Expected: `200`, `200`, `live: 200`.

Run: `curl -s https://dashboard.terraline.space/god-mode/ | grep -c '/god-mode/assets/'`

Expected: angka ≥ 1 (yang tersaji app admin, bukan shell web).

---

### Task 5: Verifikasi end-to-end + dokumentasi

**Files:**

- Modify: `AGENTS.md`

- [ ] **Step 1: Verifikasi CSRF origin lolos dari origin tunnel**

Run:

```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://api.terraline.space/api/auth/login/ \
  -H 'Origin: https://dashboard.terraline.space' -H 'Content-Type: application/json' \
  -d '{"email":"nobody@example.com","password":"wrong"}'
```

Expected: `400` atau `401` (kredensial salah), **bukan** `403`. 403 artinya origin ditolak → cek `CORS_ALLOWED_ORIGINS`/`APP_BASE_URL`/`FRONTEND_URL` di `apps/api/.env` dan recreate container.

- [ ] **Step 2: Audit tidak ada IP LAN tersisa di runtime/build prod**

Run:

```bash
grep -rn '192\.168\.1\.11' apps/web/.env apps/admin/.env apps/space/.env apps/api/.env apps/live/.env apps/web/build/client apps/admin/build apps/space/build | wc -l
```

Expected: `0`.

- [ ] **Step 3: Verifikasi manual browser (tunnel)**

1. Buka `https://dashboard.terraline.space` → login → buka project → **Pages** → buka/buat halaman dokumen → DevTools Network: koneksi WS `wss://api.terraline.space/live/collaboration?...` berstatus **101**.
2. Menu user → **God Mode**: terbuka di `https://dashboard.terraline.space/god-mode/` (tampil app admin, bukan halaman web).
3. Publish/share satu project → link `https://dashboard.terraline.space/spaces/...` terbuka.
4. Edit satu work item (mutasi) → sukses tanpa error 403.

- [ ] **Step 4: Update `AGENTS.md`**

Sisipkan section berikut setelah section `## Web Prod vs Dev (port 3000)`:

```markdown
## Admin & Space Prod (3001/3002)

- `plane-admin-prod.service` (3001) menjalankan `pnpm --filter=admin exec vite preview` atas `apps/admin/build/client` (base `/god-mode`); `plane-space-prod.service` (3002) menjalankan `pnpm --filter=space start` (react-router-serve, base `/spaces`).
- Setelah mengubah kode admin/space: `pnpm --filter=admin build` / `pnpm --filter=space build`, lalu `systemctl --user restart plane-admin-prod.service` / `plane-space-prod.service`.
- Dev `plane-admin.service` (3001) harus stop+disable saat prod jalan (konflik port); `pnpm dev` juga memakai 3001/3002.
- Ingress: `dashboard.terraline.space/god-mode` → `localhost:3001`, `/spaces` → `localhost:3002` (path rule sebelum catch-all web).
- Semua URL prod memakai hostname tunnel (`dashboard.terraline.space`, `api.terraline.space`); ganti IP LAN tidak memerlukan edit env/rebuild/restart apa pun untuk akses tunnel.
```

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md
git commit -m "docs(agents): admin/space prod units and IP-free tunnel hosting"
```

---

## Rollback

1. Hapus dua path rule ingress (`/god-mode`, `/spaces`) via dashboard/API; config kembali ke 4 rule semula.
2. `systemctl --user disable --now plane-admin-prod.service plane-space-prod.service`.
3. `systemctl --user enable --now plane-admin.service` (dev admin kembali seperti semula).
4. Kembalikan nilai env di `apps/api/.env`, `apps/live/.env`, `apps/web|admin|space/.env`; rebuild tiga app; recreate `api worker beat-worker`; restart `plane-live` + `plane-web-prod`.
5. Tidak ada perubahan data.
