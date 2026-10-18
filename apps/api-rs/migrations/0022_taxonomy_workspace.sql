-- Program P1/S3: labels + estimates become single workspace-level sets.
-- Additive: new unique indexes + idempotent union backfill. Project-scoped
-- columns stay (nulled on survivors) until the S9 cleanup.

-- Labels: workspace-level uniqueness.
-- Drop the legacy table-wide unique on name for project-less labels: it
-- contradicts workspace-level labels (same name legitimately exists in many
-- workspaces; "incident" alone occurs in 60+ workspaces on this instance).
-- Zero live project-less rows exist today, so the drop is a no-op for data.
DROP INDEX IF EXISTS public.unique_name_when_project_null_and_not_deleted;
CREATE UNIQUE INDEX IF NOT EXISTS labels_ws_name_unique
    ON public.labels (workspace_id, lower(btrim(name)))
    WHERE deleted_at IS NULL;

-- (1) Remap issue_labels to the canonical label (oldest) per (workspace, name).
UPDATE public.issue_labels il SET label_id = m.canonical_id, updated_at = now()
FROM (
    SELECT id AS old_id,
           FIRST_VALUE(id) OVER w AS canonical_id
    FROM public.labels WHERE deleted_at IS NULL
    WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)
) m
WHERE il.label_id = m.old_id AND il.label_id <> m.canonical_id AND il.deleted_at IS NULL;

-- (2) Remap hierarchical parents that point at merged-away labels.
UPDATE public.labels ch SET parent_id = m.canonical_id, updated_at = now()
FROM (
    SELECT id AS old_id,
           FIRST_VALUE(id) OVER w AS canonical_id
    FROM public.labels WHERE deleted_at IS NULL
    WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)
) m
WHERE ch.parent_id = m.old_id AND ch.parent_id <> m.canonical_id AND ch.deleted_at IS NULL;

-- (3) Soft-delete non-canonical duplicates.
UPDATE public.labels l SET deleted_at = now(), updated_at = now()
WHERE l.deleted_at IS NULL AND l.id NOT IN (
    SELECT DISTINCT FIRST_VALUE(id) OVER w
    FROM public.labels WHERE deleted_at IS NULL
    WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)
);

-- (4) Survivors become workspace-level.
UPDATE public.labels SET project_id = NULL, updated_at = now()
WHERE deleted_at IS NULL AND project_id IS NOT NULL;

-- Estimates: nullable project link + workspace uniques.
ALTER TABLE public.estimates ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE public.estimate_points ALTER COLUMN project_id DROP NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS estimates_ws_name_unique
    ON public.estimates (workspace_id, lower(btrim(name)), type)
    WHERE deleted_at IS NULL;
ALTER TABLE public.workspace_settings ADD COLUMN IF NOT EXISTS estimate_id uuid;

-- (5) Merge duplicate estimates per (workspace, name, type).
-- Canonical = most issue references, else oldest. Points union by value:
-- issues pointing at moved points follow to the canonical same-value point,
-- otherwise the point row is re-homed to the canonical estimate.
-- NOTE: WITH blocks are repeated per statement (CTEs do not persist).
WITH ranked AS (
    SELECT e.id, e.workspace_id, lower(btrim(e.name)) AS norm_name, e.type,
           COUNT(i.id) AS use_count, e.created_at,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id, lower(btrim(e.name)), e.type
               ORDER BY COUNT(i.id) DESC, MIN(e.created_at), MIN(e.id::text)
           ) AS rn
    FROM public.estimates e
    LEFT JOIN public.estimate_points ep ON ep.estimate_id = e.id AND ep.deleted_at IS NULL
    LEFT JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
    WHERE e.deleted_at IS NULL
    GROUP BY e.id, e.workspace_id, e.name, e.type, e.created_at
),
canon AS (SELECT id AS canonical_id, workspace_id, norm_name, type FROM ranked WHERE rn = 1),
dupes AS (SELECT id AS dupe_id, workspace_id, norm_name, type FROM ranked WHERE rn > 1)
-- 5a: move dupe points without a same-value canonical sibling to the canonical estimate
UPDATE public.estimate_points ep SET estimate_id = c.canonical_id, updated_at = now()
FROM dupes d JOIN canon c
  ON c.workspace_id = d.workspace_id AND c.norm_name = d.norm_name AND c.type = d.type
WHERE ep.estimate_id = d.dupe_id AND ep.deleted_at IS NULL
  AND NOT EXISTS (
      SELECT 1 FROM public.estimate_points cp
      WHERE cp.estimate_id = c.canonical_id AND cp.deleted_at IS NULL AND cp.value = ep.value
  );

-- 5b: remap issues off points that are about to die (same-value canonical point wins)
WITH ranked AS (
    SELECT e.id, e.workspace_id, lower(btrim(e.name)) AS norm_name, e.type,
           COUNT(i.id) AS use_count, e.created_at,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id, lower(btrim(e.name)), e.type
               ORDER BY COUNT(i.id) DESC, MIN(e.created_at), MIN(e.id::text)
           ) AS rn
    FROM public.estimates e
    LEFT JOIN public.estimate_points ep ON ep.estimate_id = e.id AND ep.deleted_at IS NULL
    LEFT JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
    WHERE e.deleted_at IS NULL
    GROUP BY e.id, e.workspace_id, e.name, e.type, e.created_at
),
canon AS (SELECT id AS canonical_id, workspace_id, norm_name, type FROM ranked WHERE rn = 1),
dupes AS (SELECT id AS dupe_id, workspace_id, norm_name, type FROM ranked WHERE rn > 1)
UPDATE public.issues i SET estimate_point_id = cp.id, updated_at = now()
FROM public.estimate_points ep
JOIN dupes d ON d.dupe_id = ep.estimate_id
JOIN canon c ON c.workspace_id = d.workspace_id AND c.norm_name = d.norm_name AND c.type = d.type
JOIN public.estimate_points cp
  ON cp.estimate_id = c.canonical_id AND cp.deleted_at IS NULL AND cp.value = ep.value
WHERE i.estimate_point_id = ep.id AND i.deleted_at IS NULL AND ep.deleted_at IS NULL;

-- 5c: soft-delete orphaned dupe points, then dupe estimates
WITH ranked AS (
    SELECT e.id, e.workspace_id, lower(btrim(e.name)) AS norm_name, e.type,
           COUNT(i.id) AS use_count, e.created_at,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id, lower(btrim(e.name)), e.type
               ORDER BY COUNT(i.id) DESC, MIN(e.created_at), MIN(e.id::text)
           ) AS rn
    FROM public.estimates e
    LEFT JOIN public.estimate_points ep ON ep.estimate_id = e.id AND ep.deleted_at IS NULL
    LEFT JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
    WHERE e.deleted_at IS NULL
    GROUP BY e.id, e.workspace_id, e.name, e.type, e.created_at
),
dupes AS (SELECT id AS dupe_id FROM ranked WHERE rn > 1)
UPDATE public.estimate_points ep SET deleted_at = now(), updated_at = now()
WHERE ep.deleted_at IS NULL AND ep.estimate_id IN (SELECT dupe_id FROM dupes)
  AND NOT EXISTS (SELECT 1 FROM public.issues i WHERE i.estimate_point_id = ep.id AND i.deleted_at IS NULL);
WITH ranked AS (
    SELECT e.id,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id, lower(btrim(e.name)), e.type
               ORDER BY (SELECT COUNT(*) FROM public.estimate_points ep
                         JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
                         WHERE ep.estimate_id = e.id AND ep.deleted_at IS NULL) DESC,
                        e.created_at, e.id::text
           ) AS rn
    FROM public.estimates e WHERE e.deleted_at IS NULL
)
UPDATE public.estimates e SET deleted_at = now(), updated_at = now()
WHERE e.id IN (SELECT id FROM ranked WHERE rn > 1) AND e.deleted_at IS NULL;

-- (6) Exactly one last_used per workspace + active pointer on workspace_settings.
UPDATE public.estimates e SET last_used = false, updated_at = now()
WHERE e.deleted_at IS NULL AND e.last_used IS DISTINCT FROM false;
WITH canon AS (
    SELECT e.id,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id
               ORDER BY (SELECT COUNT(*) FROM public.estimate_points ep
                         JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
                         WHERE ep.estimate_id = e.id AND ep.deleted_at IS NULL) DESC,
                        e.created_at, e.id::text
           ) AS rn
    FROM public.estimates e WHERE e.deleted_at IS NULL
)
UPDATE public.estimates e SET last_used = true, updated_at = now()
FROM canon c WHERE c.id = e.id AND c.rn = 1;
UPDATE public.workspace_settings ws SET estimate_id = sub.canonical_id, updated_at = now()
FROM (
    SELECT DISTINCT ON (e.workspace_id) e.workspace_id, e.id AS canonical_id
    FROM public.estimates e WHERE e.deleted_at IS NULL AND e.last_used = true
    ORDER BY e.workspace_id
) sub WHERE sub.workspace_id = ws.workspace_id;
UPDATE public.workspace_settings SET estimate_id = NULL, updated_at = now()
WHERE estimate_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM public.estimates e
    WHERE e.id = workspace_settings.estimate_id AND e.deleted_at IS NULL
);

-- (7) Survivors become workspace-level.
UPDATE public.estimates SET project_id = NULL, updated_at = now()
WHERE deleted_at IS NULL AND project_id IS NOT NULL;
UPDATE public.estimate_points SET project_id = NULL, updated_at = now()
WHERE deleted_at IS NULL AND project_id IS NOT NULL;
