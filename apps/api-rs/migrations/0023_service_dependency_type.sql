-- Typed service dependency edges (design 2026-10-20).
-- Existing rows become depends_on. Constraint is recreated so this file stays
-- idempotent when applied manually before starting the API.

ALTER TABLE public.service_dependencies
    ADD COLUMN IF NOT EXISTS dependency_type varchar(20) NOT NULL DEFAULT 'depends_on';

ALTER TABLE public.service_dependencies
    DROP CONSTRAINT IF EXISTS service_dependencies_type_check;

ALTER TABLE public.service_dependencies
    ADD CONSTRAINT service_dependencies_type_check
    CHECK (dependency_type IN ('runs_on', 'connects_to', 'replicates_to', 'depends_on', 'uses', 'other'));

DROP INDEX IF EXISTS public.service_dependencies_pair_idx;

CREATE UNIQUE INDEX IF NOT EXISTS service_dependencies_typed_pair_idx
    ON public.service_dependencies (from_service_id, to_service_id, dependency_type)
    WHERE deleted_at IS NULL;
