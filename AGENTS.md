# Agent Development Guide

## Commands

- `pnpm dev` - Start all dev servers (web:3000, admin:3001)
- `pnpm build` - Build all packages and apps
- `pnpm check` - Run all checks (format, lint, types)
- `pnpm check:lint` - OxLint across all packages
- `pnpm check:types` - TypeScript type checking
- `pnpm fix` - Auto-fix format and lint issues
- `pnpm turbo run <command> --filter=<package>` - Target specific package/app
- `pnpm --filter=@plane/ui storybook` - Start Storybook on port 6006

## Web Prod vs Dev (port 3000)

Only ONE server may occupy port 3000. Default is **prod** (`plane-web-prod.service`); dev (`plane-web.service`) is stopped + disabled.

- Prod serves static `apps/web/build/client` (built with `VITE_API_BASE_URL=https://api.terraline.space` for the tunnel demo). It never picks up code changes.
- After ANY web code change that must appear on the tunnel, rebuild + restart:
  1. `pnpm --filter=web build`
  2. `systemctl --user restart plane-web-prod.service`
- To code locally instead: `systemctl --user stop plane-web-prod.service && systemctl --user start plane-web.service` (dev needs `VITE_API_BASE_URL=http://192.168.1.11:8000` in `apps/web/.env`, otherwise login loops on SameSite=Lax cross-site cookies). Never enable both services at once.
- Do NOT expose Vite dev via Cloudflare Tunnel without a cache-bypass rule: edge-cached `node_modules/.vite/deps/*` mixes optimizer generations (`?v=` mismatch) and crashes React (`resolveDispatcher() is null`). Do NOT delete `apps/web/node_modules/.vite` to "fix" it — that resets the optimizer and makes it worse.

## Live Server (port 3100)

Server kolaborasi realtime Pages (Hocuspocus/Yjs) untuk editor dokumen.

- Service: `plane-live.service` (`systemctl --user status plane-live`), unit di `systemd/user/plane-live.service`.
- Env: `apps/live/.env` (API_BASE_URL ke api-rs, REDIS_URL ke container plane-redis, LIVE_SERVER_SECRET_KEY).
- `WEB_BASE_URL` di env live dikirim sebagai header `Origin` pada mutasi API (PATCH/POST) agar lolos origin-check api-rs; nilainya harus ada di `CORS_ALLOWED_ORIGINS` (`apps/api/.env`).
- Setelah mengubah kode `apps/live`: `pnpm --filter=live build && systemctl --user restart plane-live.service`.
- Verifikasi: `curl http://localhost:3100/live/health/` (cold start ~8 detik).
- `VITE_LIVE_BASE_URL` di-bake saat build web dan harus URL yang bisa dijangkau browser. Untuk LAN pakai `http://<LAN-IP>:3100` (IP host bisa berubah); untuk tunnel butuh ingress (mis. `live.terraline.space` → `http://localhost:3100` di dashboard Cloudflare) + rebuild dengan URL https, karena halaman https tidak boleh connect `ws://` (mixed content).

## Backend rebuild (api-rs, Docker)

Stack backend berjalan dikelola `docker-compose-local.yml` (via systemd `plane-backend.service`), BUKAN `docker-compose.yml` root — keduanya memakai project `plane-for-itsm`, jadi jangan `docker compose up` dari dua file berbeda saat stack sedang jalan: service dengan nama sama akan saling recreate. Data aman (volume project-scoped `plane-for-itsm_pgdata`/`redisdata`/`uploads`), hanya nama container yang berpindah (`api-rs` ⇄ `plane-for-itsm-api-1`).

- Rebuild setelah mengubah `apps/api-rs`:
  `docker compose -f docker-compose-local.yml up -d --build api worker beat-worker`
- Build Rust memakai LTO (`-C lto -C codegen-units=1`): tahap link bisa 10+ menit TANPA output sama sekali — bukan hang, jangan abort. Jalankan detached ke file log lalu poll, jangan pipe ke `tail`/buffer (terlihat seperti menggantung).
- Verifikasi setelah rebuild: `curl http://localhost:8000/health` → 200, lalu restart live (ia memegang koneksi Redis yang ikut ter-recreate):
  `systemctl --user restart plane-live.service && curl http://localhost:3100/live/health/`
- Jika `plane-backend.service` berstatus failed: `systemctl --user reset-failed plane-backend.service`.

## Code Style

- **Imports**: Use `workspace:*` for internal packages, `catalog:` for external deps
- **TypeScript**: Strict mode enabled, all files must be typed
- **Formatting**: oxfmt, run `pnpm fix:format`
- **Linting**: OxLint with shared `.oxlintrc.json` config
- **Naming**: camelCase for variables/functions, PascalCase for components/types
- **Error Handling**: Use try-catch with proper error types, log errors appropriately
- **State Management**: MobX stores in `packages/shared-state`, reactive patterns
- **Testing**: All features require unit tests, use existing test framework per package
- **Components**: Build in `@plane/ui` with Storybook for isolated development

## Backend tests (Docker)

The Django/pytest suite for `apps/api` runs in an isolated stack defined by `docker-compose-test.yml` at the repo root.

Prereq (once): `./setup.sh` — generates `apps/api/.env` from `.env.example`.

- Full suite: `docker compose -f docker-compose-test.yml up --build --abort-on-container-exit --exit-code-from api-tests`
- Subset: `docker compose -f docker-compose-test.yml run --rm api-tests pytest -m unit`
- Teardown: `docker compose -f docker-compose-test.yml down -v`

See `apps/api/tests/RUNNING_TESTS.md` for the full walkthrough and troubleshooting; see `apps/api/tests/TESTING_GUIDE.md` for test conventions and fixtures.
