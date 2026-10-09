-- War room property (Incident bridge di Work Items): lookup kolom turunan
-- `war_rooms` per issue memakai prefix `primary_issue_id`. Index existing
-- ber-prefix `project_id` (war_rooms_project_status_idx) dan partial unique
-- hanya untuk status active/monitoring (war_rooms_one_active_per_issue_idx),
-- sehingga room resolved/archived terakhir tidak ter-cover. Delta diterapkan
-- saat boot oleh `common::db::migrate`.

CREATE INDEX IF NOT EXISTS war_rooms_primary_issue_latest_idx
    ON public.war_rooms (primary_issue_id, created_at DESC)
    WHERE deleted_at IS NULL;
