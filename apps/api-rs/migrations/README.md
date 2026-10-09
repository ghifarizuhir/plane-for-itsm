# Migrations (Plan 3.1)

`0001_initial.sql` is a **squashed idempotent baseline** of all 123 Django
migrations (`apps/api/plane/db/migrations/*.py`), extracted via:

```bash
docker exec plane-db pg_dump -U plane -d plane --schema-only --no-owner --no-privileges
```

then transformed:

- `CREATE TABLE/INDEX` → `... IF NOT EXISTS`
- `ALTER TABLE ... ADD CONSTRAINT` → wrapped in `DO $$ ... EXCEPTION WHEN duplicate_object`
- `ADD GENERATED ... AS IDENTITY` → same guard

Safe to run on a fresh DB **and** on the already-migrated live `plane-db`.
Applied at boot via `common::db::migrate`. New schema changes go in
`NNNN_*.sql` where `NNNN` is one higher than the current maximum version —
NEVER reuse an existing version number or edit an applied migration in place
(2026-10-09: reusing `0002` wedged api + worker startup on the migration
advisory lock). Guard: `crates/common/tests/migration_versions_test.rs` fails
the build on duplicate versions.
