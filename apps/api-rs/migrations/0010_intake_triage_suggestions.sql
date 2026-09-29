-- AI intake triage: Jev suggestions per intake_issue row.
-- Applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.intake_triage_suggestions (
    id uuid NOT NULL,
    intake_issue_id uuid NOT NULL REFERENCES public.intake_issues(id) ON DELETE CASCADE,
    project_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    status character varying(10) NOT NULL DEFAULT 'pending',
    model character varying(255),
    answers jsonb,
    category_type_id uuid,
    category_label character varying(255),
    category_confidence double precision,
    severity_priority character varying(10),
    severity_score double precision,
    severity_confidence double precision,
    needs_human double precision,
    applied_fields text[] NOT NULL DEFAULT '{}',
    dismissed_fields text[] NOT NULL DEFAULT '{}',
    attempts integer NOT NULL DEFAULT 0,
    last_error text,
    input_tokens integer,
    output_tokens integer,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT intake_triage_suggestions_status_check
        CHECK (status IN ('pending','ready','failed')),
    CONSTRAINT intake_triage_suggestions_severity_check
        CHECK (severity_priority IS NULL OR severity_priority IN ('none','low','medium','high','urgent'))
);

CREATE UNIQUE INDEX IF NOT EXISTS intake_triage_suggestions_issue_idx
    ON public.intake_triage_suggestions (intake_issue_id);

CREATE INDEX IF NOT EXISTS intake_triage_suggestions_retry_idx
    ON public.intake_triage_suggestions (updated_at) WHERE status = 'failed';
