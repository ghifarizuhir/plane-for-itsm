-- External service metric buckets (design 2026-10-24-service-metrics-ingest).
-- Additive: workspace-level sources, per-service metric definitions, bucket samples.

CREATE TABLE IF NOT EXISTS public.metric_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    name character varying(255) NOT NULL,
    token character varying(64) NOT NULL,
    is_active boolean NOT NULL DEFAULT true,
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS metric_sources_token_uniq
    ON public.metric_sources (token) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS metric_sources_name_uniq
    ON public.metric_sources (workspace_id, lower(btrim(name))) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS metric_sources_workspace_idx
    ON public.metric_sources (workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.service_metrics (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    metric_key character varying(200) NOT NULL,
    display_name character varying(255) NOT NULL DEFAULT '',
    unit character varying(50) NOT NULL DEFAULT '',
    description text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS service_metrics_service_key_uniq
    ON public.service_metrics (service_id, metric_key);
CREATE INDEX IF NOT EXISTS service_metrics_workspace_idx
    ON public.service_metrics (workspace_id);

CREATE TABLE IF NOT EXISTS public.service_metric_buckets (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    metric_id uuid NOT NULL REFERENCES public.service_metrics(id) ON DELETE CASCADE,
    source_id uuid REFERENCES public.metric_sources(id) ON DELETE SET NULL,
    resolution smallint NOT NULL CHECK (resolution IN (60, 300, 600, 1800, 3600)),
    bucket_start timestamptz NOT NULL,
    count bigint CHECK (count IS NULL OR count >= 0),
    sum double precision,
    min double precision,
    max double precision,
    avg double precision,
    last_value double precision,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT service_metric_buckets_stats_present CHECK (
        count IS NOT NULL OR sum IS NOT NULL OR min IS NOT NULL OR max IS NOT NULL
        OR avg IS NOT NULL OR last_value IS NOT NULL
    ),
    CONSTRAINT service_metric_buckets_min_max CHECK (
        min IS NULL OR max IS NULL OR min <= max
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS service_metric_buckets_series_uniq
    ON public.service_metric_buckets (metric_id, resolution, bucket_start);
CREATE INDEX IF NOT EXISTS service_metric_buckets_service_time_idx
    ON public.service_metric_buckets (service_id, bucket_start DESC);
CREATE INDEX IF NOT EXISTS service_metric_buckets_retention_idx
    ON public.service_metric_buckets (resolution, bucket_start);
