# Offline release bundle

Built by `scripts/release.sh <version>` from the repo root.

## Deploy (prod, offline)

1. Copy `release/<version>/` to the server (USB).
2. `./load.sh` — verifies sha256 and `docker load`s every tar.
3. `cp .env.prod.example .env.prod`, fill all `CHANGE-ME`.
4. `./install.sh` — starts infra + app, runs migrator once.

## Notes

- `worker`/`beat-worker` reuse the `plane-api-rs` image (no separate tar).
- `VITE_*` frontend vars are baked at build time; changing the domain needs a rebuild.
- Never commit `tars/` or `.env.prod`.
