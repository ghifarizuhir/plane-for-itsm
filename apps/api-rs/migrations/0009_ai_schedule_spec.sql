-- AI schedule recipe: structured how_to skeleton on ai_schedules.
-- Legacy rows keep spec = NULL and run their raw prompt with every read tool.

ALTER TABLE public.ai_schedules ADD COLUMN IF NOT EXISTS spec jsonb;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ai_schedules_spec_object_check') THEN
        ALTER TABLE public.ai_schedules ADD CONSTRAINT ai_schedules_spec_object_check
            CHECK (spec IS NULL OR jsonb_typeof(spec) = 'object');
    END IF;
END
$$;
