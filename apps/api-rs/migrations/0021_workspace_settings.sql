-- Workspace-level feature settings (program P1/S2: single config set).
-- Additive: new table + idempotent union backfill. Project flag columns stay
-- untouched until S6/S7/S8 recompute this union at their cutovers.

CREATE TABLE IF NOT EXISTS public.workspace_settings (
    id uuid NOT NULL PRIMARY KEY,
    workspace_id uuid NOT NULL UNIQUE REFERENCES public.workspaces(id) ON DELETE CASCADE,
    cycle_view boolean NOT NULL DEFAULT true,
    module_view boolean NOT NULL DEFAULT true,
    issue_views_view boolean NOT NULL DEFAULT true,
    page_view boolean NOT NULL DEFAULT true,
    intake_view boolean NOT NULL DEFAULT false,
    is_issue_type_enabled boolean NOT NULL DEFAULT false,
    is_time_tracking_enabled boolean NOT NULL DEFAULT false,
    guest_view_all_features boolean NOT NULL DEFAULT false,
    archive_in integer NOT NULL DEFAULT 0,
    close_in integer NOT NULL DEFAULT 0,
    default_assignee_id uuid,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

-- Idempotent union backfill, one row per live workspace. Scalars use the
-- most-frequent non-null project value; ties go to the canonical application
-- (most non-deleted issues); all-null falls back to column defaults.
INSERT INTO public.workspace_settings (
    id, workspace_id, cycle_view, module_view, issue_views_view, page_view,
    intake_view, is_issue_type_enabled, is_time_tracking_enabled,
    guest_view_all_features, archive_in, close_in, default_assignee_id
)
SELECT gen_random_uuid(), w.id,
    COALESCE(BOOL_OR(p.cycle_view), true),
    COALESCE(BOOL_OR(p.module_view), true),
    COALESCE(BOOL_OR(p.issue_views_view), true),
    COALESCE(BOOL_OR(p.page_view), true),
    COALESCE(BOOL_OR(p.intake_view), false),
    COALESCE(BOOL_OR(p.is_issue_type_enabled), false),
    COALESCE(BOOL_OR(p.is_time_tracking_enabled), false),
    COALESCE(BOOL_OR(p.guest_view_all_features), false),
    COALESCE(mode_int.mode_val, 0),
    COALESCE(mode_close.mode_val, 0),
    mode_assignee.mode_val
FROM public.workspaces w
LEFT JOIN public.projects p ON p.workspace_id = w.id AND p.deleted_at IS NULL
LEFT JOIN LATERAL (
    SELECT p2.archive_in AS mode_val
    FROM public.projects p2
    WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.archive_in IS NOT NULL
    GROUP BY p2.archive_in
    ORDER BY COUNT(*) DESC,
        MAX((SELECT COUNT(*) FROM public.issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC,
        MIN(p2.created_at)
    LIMIT 1
) mode_int ON true
LEFT JOIN LATERAL (
    SELECT p2.close_in AS mode_val
    FROM public.projects p2
    WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.close_in IS NOT NULL
    GROUP BY p2.close_in
    ORDER BY COUNT(*) DESC,
        MAX((SELECT COUNT(*) FROM public.issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC,
        MIN(p2.created_at)
    LIMIT 1
) mode_close ON true
LEFT JOIN LATERAL (
    SELECT p2.default_assignee_id AS mode_val
    FROM public.projects p2
    WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.default_assignee_id IS NOT NULL
    GROUP BY p2.default_assignee_id
    ORDER BY COUNT(*) DESC,
        MAX((SELECT COUNT(*) FROM public.issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC,
        MIN(p2.created_at)
    LIMIT 1
) mode_assignee ON true
WHERE w.deleted_at IS NULL
GROUP BY w.id, mode_int.mode_val, mode_close.mode_val, mode_assignee.mode_val
ON CONFLICT (workspace_id) DO UPDATE SET
    cycle_view = EXCLUDED.cycle_view,
    module_view = EXCLUDED.module_view,
    issue_views_view = EXCLUDED.issue_views_view,
    page_view = EXCLUDED.page_view,
    intake_view = EXCLUDED.intake_view,
    is_issue_type_enabled = EXCLUDED.is_issue_type_enabled,
    is_time_tracking_enabled = EXCLUDED.is_time_tracking_enabled,
    guest_view_all_features = EXCLUDED.guest_view_all_features,
    archive_in = EXCLUDED.archive_in,
    close_in = EXCLUDED.close_in,
    default_assignee_id = EXCLUDED.default_assignee_id,
    updated_at = now();
