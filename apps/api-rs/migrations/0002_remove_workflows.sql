-- Hapus skema workflow (Django 0123-0125) dan flatten constraint state.
-- Guarded: no-op bila Django 0126 sudah menjalankannya, aman di fresh DB
-- (baseline 0001 tidak memuat tabel workflow).
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_legacy_name_project_when_deleted_at_null;
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_name_project_type_when_deleted_at_null;
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_project_workflow_state_when_deleted_at_null;
ALTER TABLE states DROP CONSTRAINT IF EXISTS state_unique_default_project_type_when_deleted_at_null;
ALTER TABLE states DROP COLUMN IF EXISTS type_id;
ALTER TABLE states DROP COLUMN IF EXISTS workflow_state_id;
ALTER TABLE issue_types DROP COLUMN IF EXISTS workflow_id;
DROP TABLE IF EXISTS workflow_transitions;
DROP TABLE IF EXISTS workflow_states;
DROP TABLE IF EXISTS workflows;
CREATE UNIQUE INDEX IF NOT EXISTS state_unique_name_project_when_deleted_at_null
  ON states (project_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS state_unique_default_project_when_deleted_at_null
  ON states (project_id) WHERE deleted_at IS NULL AND "default" = true;
