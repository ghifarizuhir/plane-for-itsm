-- Workspace-level services (design 2026-10-17): project links + workspace-scoped
-- name uniqueness. Phase 1 is additive: legacy project_id columns stay (made
-- nullable, dual-written by the API) until the cleanup release.

CREATE TABLE IF NOT EXISTS public.service_project_links (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    link_type character varying(20) NOT NULL CHECK (link_type IN ('owner', 'consumer')),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS service_project_links_service_project_idx
    ON public.service_project_links (service_id, project_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS service_project_links_one_owner_idx
    ON public.service_project_links (service_id) WHERE link_type = 'owner' AND deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS service_project_links_project_idx
    ON public.service_project_links (project_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS service_project_links_service_idx
    ON public.service_project_links (service_id) WHERE deleted_at IS NULL;

-- Backfill: every existing service's project becomes its owner link.
INSERT INTO public.service_project_links
    (id, workspace_id, service_id, project_id, link_type, created_at, updated_at)
SELECT gen_random_uuid(), s.workspace_id, s.id, s.project_id, 'owner', now(), now()
FROM public.services s
WHERE s.deleted_at IS NULL
ON CONFLICT DO NOTHING;

-- Dedupe pass 1: workspace duplicate names -> "Name (PROJECT-IDENTIFIER)".
WITH ranked AS (
    SELECT s.id, s.name, p.identifier,
           row_number() OVER (
             PARTITION BY s.workspace_id, lower(btrim(s.name))
             ORDER BY s.created_at, s.id
           ) AS rn
    FROM public.services s
    JOIN public.projects p ON p.id = s.project_id
    WHERE s.deleted_at IS NULL
)
UPDATE public.services s
SET name = r.name || ' (' || r.identifier || ')', updated_at = now()
FROM ranked r
WHERE s.id = r.id AND r.rn > 1;

-- Dedupe pass 2: residual collisions -> append a short id fragment.
WITH ranked AS (
    SELECT s.id, s.name, p.identifier,
           row_number() OVER (
             PARTITION BY s.workspace_id, lower(btrim(s.name))
             ORDER BY s.created_at, s.id
           ) AS rn
    FROM public.services s
    JOIN public.projects p ON p.id = s.project_id
    WHERE s.deleted_at IS NULL
)
UPDATE public.services s
SET name = r.name || ' (' || r.identifier || '-' || left(s.id::text, 8) || ')', updated_at = now()
FROM ranked r
WHERE s.id = r.id AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS services_unique_name_workspace_idx
    ON public.services (workspace_id, lower(btrim(name))) WHERE deleted_at IS NULL;

ALTER TABLE public.services ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE public.service_dependencies ALTER COLUMN project_id DROP NOT NULL;
