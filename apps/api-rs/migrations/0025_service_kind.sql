-- Component kind for services (design 2026-10-25). `kind` describes the
-- component class (database/server/...) while the existing "type" column keeps
-- describing the internal/external boundary. Existing rows default to service.
-- Idempotent so it can be applied manually before the API restarts.

ALTER TABLE public.services
    ADD COLUMN IF NOT EXISTS kind varchar(20) NOT NULL DEFAULT 'service';

ALTER TABLE public.services
    DROP CONSTRAINT IF EXISTS services_kind_check;

ALTER TABLE public.services
    ADD CONSTRAINT services_kind_check
    CHECK (kind IN ('service', 'database', 'server', 'cache', 'queue',
                    'storage', 'network', 'external_api', 'other'));
