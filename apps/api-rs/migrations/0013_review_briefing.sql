-- Review briefing (AI): pre-meeting briefing stored on a review session.
-- Spec: docs/superpowers/specs/2026-10-03-review-briefing-design.md
-- Delta applied at boot by `common::db::migrate`; the IF NOT EXISTS guards
-- keep a manual apply + boot apply idempotent.

ALTER TABLE public.review_sessions
    ADD COLUMN IF NOT EXISTS briefing jsonb,
    ADD COLUMN IF NOT EXISTS briefing_generated_at timestamp with time zone,
    ADD COLUMN IF NOT EXISTS briefing_generated_by_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS briefing_model character varying(100);
