-- Galileo chat history: multi-session conversations for the AI assistant.
-- Applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.ai_conversations (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    created_by_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    mode character varying(10) NOT NULL,
    title character varying(120) NOT NULL DEFAULT '',
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT ai_conversations_mode_check CHECK (mode IN ('classic','agent'))
);

CREATE INDEX IF NOT EXISTS ai_conversations_owner_idx
    ON public.ai_conversations (workspace_id, created_by_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS public.ai_messages (
    id uuid NOT NULL,
    conversation_id uuid NOT NULL REFERENCES public.ai_conversations(id) ON DELETE CASCADE,
    role character varying(10) NOT NULL,
    content text NOT NULL,
    content_html text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT ai_messages_role_check CHECK (role IN ('user','assistant'))
);

CREATE INDEX IF NOT EXISTS ai_messages_conversation_idx
    ON public.ai_messages (conversation_id, created_at, id);
