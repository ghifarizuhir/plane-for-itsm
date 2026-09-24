-- Scheduler follow-ups from the 0006 review:
-- 1) proposal_key uniqueness scoped per workspace + soft-delete aware,
-- 2) preset consistency CHECKs so an unrunnable row cannot exist,
-- 3) partial index for the every-minute stuck-run sweep.

DROP INDEX IF EXISTS public.ai_schedules_proposal_key_idx;

CREATE UNIQUE INDEX IF NOT EXISTS ai_schedules_proposal_key_idx
    ON public.ai_schedules (workspace_id, proposal_key) WHERE deleted_at IS NULL;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ai_schedules_weekly_day_check') THEN
        ALTER TABLE public.ai_schedules ADD CONSTRAINT ai_schedules_weekly_day_check
            CHECK (frequency <> 'weekly' OR day_of_week IS NOT NULL);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ai_schedules_monthly_day_check') THEN
        ALTER TABLE public.ai_schedules ADD CONSTRAINT ai_schedules_monthly_day_check
            CHECK (frequency <> 'monthly' OR day_of_month IS NOT NULL);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ai_schedules_time_of_day_check') THEN
        ALTER TABLE public.ai_schedules ADD CONSTRAINT ai_schedules_time_of_day_check
            CHECK (time_of_day ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$');
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS ai_schedule_runs_stuck_idx
    ON public.ai_schedule_runs (status, created_at) WHERE status IN ('queued','running');
