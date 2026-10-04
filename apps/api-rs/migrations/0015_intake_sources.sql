-- Intake webhook source: sumber inbound (Alertmanager) + identitas seri alert.
-- Diterapkan manual ke dev DB via psql (pola 0014); IF NOT EXISTS agar aman
-- bila sqlx migrate menyusul.

CREATE TABLE IF NOT EXISTS public.intake_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id uuid NOT NULL,
    name character varying(255) NOT NULL,
    token character varying(64) NOT NULL,
    is_active boolean NOT NULL DEFAULT true,
    auto_accept boolean NOT NULL DEFAULT false,
    type_id uuid,
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS intake_sources_token_uniq
    ON public.intake_sources (token) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS intake_sources_project_idx
    ON public.intake_sources (project_id) WHERE deleted_at IS NULL;

ALTER TABLE public.issues
    ADD COLUMN IF NOT EXISTS intake_source_id uuid,
    ADD COLUMN IF NOT EXISTS intake_fingerprint text,
    ADD COLUMN IF NOT EXISTS intake_occurrence_count integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS intake_last_seen_at timestamptz;

CREATE UNIQUE INDEX IF NOT EXISTS issues_intake_series_uniq
    ON public.issues (intake_source_id, intake_fingerprint)
    WHERE intake_source_id IS NOT NULL;
