-- Services feature (ITSM): service catalog + dependency DAG + work-item links.
-- New schema delta applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.services (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    name character varying(255) NOT NULL,
    description text NOT NULL DEFAULT '',
    description_html text NOT NULL DEFAULT '',
    status character varying(20) NOT NULL DEFAULT 'planned',
    criticality character varying(20) NOT NULL DEFAULT 'medium',
    "type" character varying(20) NOT NULL DEFAULT 'internal',
    owner_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    repository_url character varying(200),
    documentation_url character varying(200),
    position jsonb,
    sort_order double precision NOT NULL DEFAULT 65535,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS services_unique_name_project_idx
    ON public.services (project_id, lower(btrim(name))) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS services_project_idx
    ON public.services (project_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.service_dependencies (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    from_service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    to_service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS service_dependencies_pair_idx
    ON public.service_dependencies (from_service_id, to_service_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS service_dependencies_project_idx
    ON public.service_dependencies (project_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.service_issues (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS service_issues_pair_idx
    ON public.service_issues (service_id, issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS service_issues_issue_idx
    ON public.service_issues (issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS service_issues_project_idx
    ON public.service_issues (project_id) WHERE deleted_at IS NULL;
