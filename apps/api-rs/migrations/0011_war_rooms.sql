-- War room (incident command room): rooms, links, participants, chat,
-- runbook, activity feed. Delta applied at boot by `common::db::migrate`.

CREATE TABLE IF NOT EXISTS public.war_rooms (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    sequence_id bigint NOT NULL,
    name character varying(255) NOT NULL,
    description_html text NOT NULL DEFAULT '',
    notes_html text NOT NULL DEFAULT '',
    severity character varying(10) NOT NULL DEFAULT 'sev3',
    status character varying(20) NOT NULL DEFAULT 'active',
    primary_issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    started_at timestamp with time zone NOT NULL DEFAULT now(),
    resolved_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_rooms_project_sequence_idx
    ON public.war_rooms (project_id, sequence_id);

CREATE INDEX IF NOT EXISTS war_rooms_project_status_idx
    ON public.war_rooms (project_id, status) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS war_rooms_one_active_per_issue_idx
    ON public.war_rooms (project_id, primary_issue_id)
    WHERE status IN ('active', 'monitoring') AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_services (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_room_services_pair_idx
    ON public.war_room_services (war_room_id, service_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_issues (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_room_issues_pair_idx
    ON public.war_room_issues (war_room_id, issue_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_participants (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    member_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    role character varying(20) NOT NULL DEFAULT 'responder',
    joined_at timestamp with time zone NOT NULL DEFAULT now(),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_room_participants_member_idx
    ON public.war_room_participants (war_room_id, member_id) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS war_room_participants_one_commander_idx
    ON public.war_room_participants (war_room_id)
    WHERE role = 'commander' AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_messages (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    author_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    body text NOT NULL,
    mentions jsonb NOT NULL DEFAULT '[]'::jsonb,
    edited_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS war_room_messages_room_idx
    ON public.war_room_messages (war_room_id, created_at DESC) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_runbook_items (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    title character varying(500) NOT NULL,
    sort_order double precision NOT NULL DEFAULT 65535,
    is_done boolean NOT NULL DEFAULT false,
    done_by_id uuid,
    done_at timestamp with time zone,
    template_key character varying(100),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS war_room_runbook_items_room_idx
    ON public.war_room_runbook_items (war_room_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_events (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    actor_id uuid,
    event_type character varying(50) NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS war_room_events_room_idx
    ON public.war_room_events (war_room_id, created_at DESC);
