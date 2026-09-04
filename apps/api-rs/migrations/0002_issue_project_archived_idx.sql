-- Plan 3.2 kajian: composite index for the hot issue-list filter
-- (project_id + archived_at IS NULL). IF NOT EXISTS keeps the
-- baseline idempotency property (see migrations/README.md).
CREATE INDEX IF NOT EXISTS issues_project_archived_idx
    ON issues (project_id, archived_at);
