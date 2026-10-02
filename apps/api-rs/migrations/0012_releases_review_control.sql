-- Release & Testing Control Boards (RCB/TCB): release bundles + generic
-- review engine (requests, sessions, agenda outcomes, participants).
-- Spec: docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md
-- Delta applied at boot by `common::db::migrate`.

CREATE TABLE IF NOT EXISTS public.releases (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    sequence_id bigint NOT NULL,
    name character varying(255) NOT NULL,
    version character varying(100),
    description_html text NOT NULL DEFAULT '',
    status character varying(20) NOT NULL DEFAULT 'draft',
    target_date date,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT releases_status_check
        CHECK (status IN ('draft', 'planned', 'in_review', 'approved', 'released', 'cancelled'))
);

CREATE UNIQUE INDEX IF NOT EXISTS releases_workspace_sequence_idx
    ON public.releases (workspace_id, sequence_id);

CREATE INDEX IF NOT EXISTS releases_workspace_status_idx
    ON public.releases (workspace_id, status) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.release_changes (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    release_id uuid NOT NULL REFERENCES public.releases(id) ON DELETE CASCADE,
    issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS release_changes_pair_idx
    ON public.release_changes (release_id, issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS release_changes_issue_idx
    ON public.release_changes (issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS release_changes_release_idx
    ON public.release_changes (release_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_requests (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    board_type character varying(3) NOT NULL,
    change_issue_id uuid REFERENCES public.issues(id) ON DELETE CASCADE,
    release_id uuid REFERENCES public.releases(id) ON DELETE CASCADE,
    project_id uuid REFERENCES public.projects(id) ON DELETE CASCADE,
    status character varying(10) NOT NULL DEFAULT 'pending',
    submission_note text NOT NULL DEFAULT '',
    submitted_by_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    submitted_at timestamp with time zone NOT NULL DEFAULT now(),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_requests_board_check CHECK (board_type IN ('tcb', 'rcb')),
    CONSTRAINT review_requests_status_check
        CHECK (status IN ('pending', 'scheduled', 'decided', 'withdrawn')),
    CONSTRAINT review_requests_subject_check CHECK (
        (board_type = 'tcb' AND change_issue_id IS NOT NULL AND project_id IS NOT NULL AND release_id IS NULL)
        OR
        (board_type = 'rcb' AND release_id IS NOT NULL AND change_issue_id IS NULL AND project_id IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS review_requests_active_tcb_subject_idx
    ON public.review_requests (change_issue_id)
    WHERE board_type = 'tcb' AND status IN ('pending', 'scheduled') AND deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS review_requests_active_rcb_subject_idx
    ON public.review_requests (release_id)
    WHERE board_type = 'rcb' AND status IN ('pending', 'scheduled') AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS review_requests_workspace_board_status_idx
    ON public.review_requests (workspace_id, board_type, status) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS review_requests_project_idx
    ON public.review_requests (project_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_sessions (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    board_type character varying(3) NOT NULL,
    project_id uuid REFERENCES public.projects(id) ON DELETE CASCADE,
    title character varying(255) NOT NULL,
    scheduled_at timestamp with time zone NOT NULL,
    status character varying(10) NOT NULL DEFAULT 'scheduled',
    minutes text NOT NULL DEFAULT '',
    location character varying(255),
    completed_at timestamp with time zone,
    cancelled_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_sessions_board_check CHECK (board_type IN ('tcb', 'rcb')),
    CONSTRAINT review_sessions_status_check
        CHECK (status IN ('scheduled', 'completed', 'cancelled')),
    CONSTRAINT review_sessions_scope_check CHECK (
        (board_type = 'tcb' AND project_id IS NOT NULL)
        OR (board_type = 'rcb' AND project_id IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS review_sessions_workspace_board_status_idx
    ON public.review_sessions (workspace_id, board_type, status) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_session_participants (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL REFERENCES public.review_sessions(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    role character varying(20) NOT NULL DEFAULT 'member',
    attendance character varying(10) NOT NULL DEFAULT 'invited',
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_session_participants_role_check
        CHECK (role IN ('chair', 'secretary', 'member')),
    CONSTRAINT review_session_participants_attendance_check
        CHECK (attendance IN ('invited', 'present', 'absent'))
);

CREATE UNIQUE INDEX IF NOT EXISTS review_session_participants_pair_idx
    ON public.review_session_participants (session_id, user_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_session_items (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL REFERENCES public.review_sessions(id) ON DELETE CASCADE,
    review_request_id uuid NOT NULL REFERENCES public.review_requests(id) ON DELETE CASCADE,
    position integer NOT NULL DEFAULT 65535,
    outcome character varying(30),
    outcome_note text NOT NULL DEFAULT '',
    decided_by_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    decided_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_session_items_outcome_check CHECK (
        outcome IS NULL
        OR outcome IN ('approved', 'rejected', 'approved_with_notes', 'deferred')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS review_session_items_pair_idx
    ON public.review_session_items (session_id, review_request_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS review_session_items_request_idx
    ON public.review_session_items (review_request_id) WHERE deleted_at IS NULL;
