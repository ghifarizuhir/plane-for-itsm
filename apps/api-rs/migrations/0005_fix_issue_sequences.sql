-- Repair per-project issue sequences and enforce uniqueness.
--
-- Legacy create paths (`issue_write::create` backing `/issues/` and
-- `/work-items/`, plus `intake::create_issue`) read
-- `MAX(issue_sequences.sequence)` without ever writing the counter row, so
-- consecutive creates reused the same number and `issue_sequences` went
-- missing rows entirely. This migration renumbers duplicate active issues,
-- backfills missing counter rows, and adds the unique index that makes the
-- bug impossible to reintroduce. Idempotent: a clean database is untouched.

-- 1. Renumber active issues that reuse a (project_id, sequence_id), keeping
--    the earliest row per group. New numbers start above the project counter
--    floor (GREATEST of counter rows and every issue sequence).
WITH floor AS (
    SELECT p.id AS project_id,
           GREATEST(
               COALESCE((SELECT MAX(sq.sequence) FROM issue_sequences sq WHERE sq.project_id = p.id), 0),
               COALESCE((SELECT MAX(i.sequence_id) FROM issues i WHERE i.project_id = p.id), 0)
           ) AS counter
    FROM projects p
),
dups AS (
    SELECT i.id,
           i.project_id,
           ROW_NUMBER() OVER (PARTITION BY i.project_id ORDER BY i.created_at, i.id) AS rn
    FROM issues i
    WHERE i.deleted_at IS NULL
      AND EXISTS (
          SELECT 1 FROM issues j
          WHERE j.project_id = i.project_id
            AND j.sequence_id = i.sequence_id
            AND j.deleted_at IS NULL
            AND (j.created_at, j.id) < (i.created_at, i.id)
      )
)
UPDATE issues i
SET sequence_id = floor.counter + dups.rn
FROM dups
JOIN floor ON floor.project_id = dups.project_id
WHERE i.id = dups.id;

-- 2. Keep existing counter rows in step with their (renumbered) issue.
UPDATE issue_sequences sq
SET sequence = i.sequence_id
FROM issues i
WHERE sq.issue_id = i.id
  AND i.deleted_at IS NULL
  AND sq.sequence <> i.sequence_id;

-- 3. Backfill the counter row every active issue should have had.
INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, created_by_id, deleted, created_at, updated_at)
SELECT gen_random_uuid(), i.sequence_id, i.id, i.project_id, i.workspace_id, i.created_by_id, false, now(), now()
FROM issues i
WHERE i.deleted_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM issue_sequences sq WHERE sq.issue_id = i.id);

-- 4. Guard: no two active issues may share a sequence in a project.
--    (`draft_issues` live in their own table; soft-deleted rows stay exempt.)
CREATE UNIQUE INDEX IF NOT EXISTS issue_unique_project_sequence_active
    ON issues (project_id, sequence_id)
    WHERE deleted_at IS NULL;
