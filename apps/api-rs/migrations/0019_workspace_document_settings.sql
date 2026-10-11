-- Workspace-scoped DOCX export settings: letterhead header/footer config.
CREATE TABLE IF NOT EXISTS public.workspace_document_settings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL,
    header_logo_mode varchar(16) NOT NULL DEFAULT 'none',
    header_logo_asset_id uuid,
    header_text_lines jsonb NOT NULL DEFAULT '[]'::jsonb,
    footer_text_lines jsonb NOT NULL DEFAULT '[]'::jsonb,
    show_page_numbers boolean NOT NULL DEFAULT true,
    show_generated_by boolean NOT NULL DEFAULT true,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS workspace_document_settings_ws_uniq
    ON public.workspace_document_settings (workspace_id) WHERE deleted_at IS NULL;
