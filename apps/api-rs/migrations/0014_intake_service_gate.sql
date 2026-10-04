-- Intake ITSM: per-type service requirement + Jev service suggestion columns.
-- Applied at boot by `common::db::migrate` (sqlx migrate); IF NOT EXISTS so
-- Django 0128 (`AddField`) may land first without conflict.

ALTER TABLE public.issue_types
    ADD COLUMN IF NOT EXISTS requires_service boolean NOT NULL DEFAULT false;

ALTER TABLE public.intake_triage_suggestions
    ADD COLUMN IF NOT EXISTS service_id uuid,
    ADD COLUMN IF NOT EXISTS service_label character varying(255),
    ADD COLUMN IF NOT EXISTS service_confidence double precision;
