-- AI scheduler: recurring agent runs created from the /schedule chat flow.
-- Applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.ai_schedules (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    created_by_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    name character varying(120) NOT NULL,
    prompt text NOT NULL,
    frequency character varying(10) NOT NULL,
    time_of_day character varying(5) NOT NULL DEFAULT '09:00',
    day_of_week smallint,
    day_of_month smallint,
    timezone character varying(64) NOT NULL DEFAULT 'UTC',
    enabled boolean NOT NULL DEFAULT true,
    next_run_at timestamp with time zone NOT NULL,
    proposal_key uuid NOT NULL,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT ai_schedules_frequency_check CHECK (frequency IN ('hourly','daily','weekly','monthly')),
    CONSTRAINT ai_schedules_day_of_week_check CHECK (day_of_week IS NULL OR (day_of_week BETWEEN 1 AND 7)),
    CONSTRAINT ai_schedules_day_of_month_check CHECK (day_of_month IS NULL OR (day_of_month BETWEEN 1 AND 31))
);

CREATE UNIQUE INDEX IF NOT EXISTS ai_schedules_proposal_key_idx
    ON public.ai_schedules (proposal_key);

CREATE INDEX IF NOT EXISTS ai_schedules_due_idx
    ON public.ai_schedules (next_run_at) WHERE enabled AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS ai_schedules_workspace_idx
    ON public.ai_schedules (workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.ai_schedule_runs (
    id uuid NOT NULL,
    schedule_id uuid NOT NULL REFERENCES public.ai_schedules(id) ON DELETE CASCADE,
    workspace_id uuid NOT NULL,
    status character varying(10) NOT NULL,
    trigger character varying(10) NOT NULL,
    prompt text NOT NULL,
    response text,
    response_html text,
    error text,
    tool_calls jsonb,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT ai_schedule_runs_status_check CHECK (status IN ('queued','running','success','failed')),
    CONSTRAINT ai_schedule_runs_trigger_check CHECK (trigger IN ('scheduled','manual'))
);

CREATE INDEX IF NOT EXISTS ai_schedule_runs_schedule_idx
    ON public.ai_schedule_runs (schedule_id, created_at DESC);
