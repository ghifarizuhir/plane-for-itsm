/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import useSWR from "swr";
// plane imports
import {
  EUserPermissions,
  EUserPermissionsLevel,
  WORKSPACE_SETTINGS_KEY,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkspaceSettingsPayload } from "@plane/types";
// services
import { WorkspaceSettingsService } from "@/services/workspace/workspace-settings.service";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useUserPermissions } from "@/hooks/store/user";

type Props = { workspaceSlug: string };

type TEditableSettings = TWorkspaceSettingsPayload & {
  cycle_view: boolean;
  module_view: boolean;
  issue_views_view: boolean;
  page_view: boolean;
  intake_view: boolean;
  is_time_tracking_enabled: boolean;
  guest_view_all_features: boolean;
  archive_in: number;
  close_in: number;
  default_assignee_id: string | null;
};

const BOOLEAN_FLAGS = [
  "cycle_view",
  "module_view",
  "issue_views_view",
  "page_view",
  "intake_view",
  "is_time_tracking_enabled",
  "guest_view_all_features",
] as const;

type TBooleanFlag = (typeof BOOLEAN_FLAGS)[number];

const FLAG_I18N: Record<TBooleanFlag, { label: string; description: string }> = {
  cycle_view: { label: "sprints_label", description: "sprints_description" },
  module_view: { label: "tracks_label", description: "tracks_description" },
  issue_views_view: { label: "views_label", description: "views_description" },
  page_view: { label: "pages_label", description: "pages_description" },
  intake_view: { label: "intake_label", description: "intake_description" },
  is_time_tracking_enabled: { label: "time_tracking_label", description: "time_tracking_description" },
  guest_view_all_features: { label: "guests_label", description: "guests_description" },
};

export const FeaturesSettingsForm = observer(function FeaturesSettingsForm({ workspaceSlug }: Props) {
  // translation
  const { t } = useTranslation();
  // store hooks
  const { allowPermissions } = useUserPermissions();
  const {
    workspace: { workspaceMemberIds, getWorkspaceMemberDetails },
  } = useMember();
  // services
  const settingsService = useMemo(() => new WorkspaceSettingsService(), []);
  // fetch settings
  const { data, isLoading, mutate } = useSWR(WORKSPACE_SETTINGS_KEY(workspaceSlug), () =>
    settingsService.fetchSettings(workspaceSlug)
  );
  // states
  const [draft, setDraft] = useState<TEditableSettings | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  // auth
  const isAdmin = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);

  useEffect(() => {
    if (data)
      setDraft({
        cycle_view: data.cycle_view,
        module_view: data.module_view,
        issue_views_view: data.issue_views_view,
        page_view: data.page_view,
        intake_view: data.intake_view,
        is_time_tracking_enabled: data.is_time_tracking_enabled,
        guest_view_all_features: data.guest_view_all_features,
        archive_in: data.archive_in,
        close_in: data.close_in,
        default_assignee_id: data.default_assignee_id,
      });
  }, [data]);

  if (isLoading || !draft) return <div className="text-13 text-tertiary">Loading…</div>;

  const showError = () => setToast({ type: TOAST_TYPE.ERROR, title: t("workspace_settings.settings.features.error") });

  const handleSave = async () => {
    if (draft.archive_in < 0 || draft.close_in < 0) {
      showError();
      return;
    }
    setIsSaving(true);
    try {
      await settingsService.updateSettings(workspaceSlug, { ...draft });
      await mutate();
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("workspace_settings.settings.features.saved") });
    } catch {
      showError();
    } finally {
      setIsSaving(false);
    }
  };

  const memberOptions = (workspaceMemberIds ?? [])
    .map((userId) => {
      const details = getWorkspaceMemberDetails(userId);
      const member = details?.member;
      if (!member) return null;
      return {
        value: member.id,
        label: member.display_name || `${member.first_name} ${member.last_name}`.trim() || member.email,
      };
    })
    .filter((option): option is { value: string; label: string } => option !== null);

  return (
    <div className="flex flex-col gap-8">
      <div className="space-y-2">
        <h4 className="text-16 font-medium text-primary">{t("workspace_settings.settings.features.title")}</h4>
        <p className="text-13 text-tertiary">{t("workspace_settings.settings.features.description")}</p>
      </div>

      <div className="space-y-4">
        {BOOLEAN_FLAGS.map((flag) => (
          <div key={flag} className="space-y-1">
            <label className="flex items-center gap-2 text-13 font-medium text-secondary">
              <input
                checked={draft[flag]}
                disabled={!isAdmin}
                type="checkbox"
                onChange={(event) => setDraft({ ...draft, [flag]: event.target.checked })}
              />
              {t(`workspace_settings.settings.features.${FLAG_I18N[flag].label}`)}
            </label>
            <p className="pl-6 text-11 text-tertiary">
              {t(`workspace_settings.settings.features.${FLAG_I18N[flag].description}`)}
            </p>
          </div>
        ))}
      </div>

      <div className="space-y-2">
        <h6 className="text-13 font-medium text-secondary">
          {t("workspace_settings.settings.features.default_assignee_label")}
        </h6>
        <p className="text-11 text-tertiary">
          {t("workspace_settings.settings.features.default_assignee_description")}
        </p>
        <select
          className="w-full max-w-sm rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-13"
          disabled={!isAdmin}
          value={draft.default_assignee_id ?? ""}
          onChange={(event) =>
            setDraft({ ...draft, default_assignee_id: event.target.value === "" ? null : event.target.value })
          }
        >
          <option value="">{t("workspace_settings.settings.features.none_label")}</option>
          {memberOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <div className="grid max-w-sm grid-cols-2 gap-4">
        {(
          [
            { key: "archive_in", label: "archive_label", description: "archive_description" },
            { key: "close_in", label: "close_label", description: "close_description" },
          ] as const
        ).map((field) => (
          <div key={field.key} className="space-y-1">
            <h6 className="text-13 font-medium text-secondary">
              {t(`workspace_settings.settings.features.${field.label}`)}
            </h6>
            <input
              className="w-full rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-13"
              disabled={!isAdmin}
              min={0}
              type="number"
              value={draft[field.key]}
              onChange={(event) => setDraft({ ...draft, [field.key]: Math.max(0, Number(event.target.value) || 0) })}
            />
            <p className="text-11 text-tertiary">{t(`workspace_settings.settings.features.${field.description}`)}</p>
          </div>
        ))}
      </div>

      {isAdmin && (
        <div>
          <Button variant="primary" size="lg" loading={isSaving} onClick={handleSave}>
            {t("workspace_settings.settings.features.save")}
          </Button>
        </div>
      )}
    </div>
  );
});
