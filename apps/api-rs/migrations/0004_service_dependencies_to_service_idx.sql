-- Services feature follow-up: index the incoming-edge side of the dependency DAG.
-- `service_dependencies_pair_idx (from_service_id, to_service_id)` covers the
-- `from` side via leftmost-prefix, but delete-cascade (`to_service_id = $1`)
-- and cycle-CTE traversal need the `to` side. Applied by `common::db::migrate`.

CREATE INDEX IF NOT EXISTS service_dependencies_to_service_idx
    ON public.service_dependencies (to_service_id) WHERE deleted_at IS NULL;
