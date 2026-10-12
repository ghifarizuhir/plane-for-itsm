/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import useSWR from "swr";
// plane imports
import { DOCUMENT_EXPORT_SETTINGS, EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TDocumentExportSettings, TDocumentExportTextAlign, TDocumentExportTextLine } from "@plane/types";
import { EFileAssetType } from "@plane/types";
import { cn } from "@plane/utils";
// services
import { DocumentExportSettingsService } from "@/services/document-export/document-export-settings.service";
import { FileService } from "@/services/file.service";
// hooks
import { useUserPermissions } from "@/hooks/store/user";

const MAX_LINES = 5;
const MAX_LINE_LEN = 200;

type TEditableTextLine = TDocumentExportTextLine & { uid: string };

type TEditableSettings = Omit<TDocumentExportSettings, "header_text_lines" | "footer_text_lines"> & {
  header_text_lines: TEditableTextLine[];
  footer_text_lines: TEditableTextLine[];
};

const withUids = (lines: TDocumentExportTextLine[]): TEditableTextLine[] =>
  lines.map((line) => ({ ...line, uid: `${Date.now()}-${Math.random().toString(36).slice(2)}` }));

const stripUids = (lines: TEditableTextLine[]): TDocumentExportTextLine[] =>
  lines.map(({ text, align }) => ({ text, align }));

type Props = { workspaceSlug: string };

export const DocumentExportSettingsForm = observer(function DocumentExportSettingsForm({ workspaceSlug }: Props) {
  // translation
  const { t } = useTranslation();
  // store hooks
  const { allowPermissions } = useUserPermissions();
  // services
  const settingsService = useMemo(() => new DocumentExportSettingsService(), []);
  const fileService = useMemo(() => new FileService(), []);
  // fetch settings
  const { data, isLoading, mutate } = useSWR(DOCUMENT_EXPORT_SETTINGS(workspaceSlug), () =>
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
        ...data,
        header_text_lines: withUids(data.header_text_lines),
        footer_text_lines: withUids(data.footer_text_lines),
      });
  }, [data]);

  if (isLoading || !draft) return <div className="text-13 text-tertiary">Loading…</div>;

  const updateLines = (
    key: "header_text_lines" | "footer_text_lines",
    updater: (lines: TEditableTextLine[]) => TEditableTextLine[]
  ) => setDraft((current) => (current ? { ...current, [key]: updater(current[key]) } : current));

  const showError = () =>
    setToast({ type: TOAST_TYPE.ERROR, title: t("workspace_settings.settings.document_export.error") });

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await settingsService.updateSettings(workspaceSlug, {
        header_logo_mode: draft.header_logo_mode,
        header_text_lines: stripUids(draft.header_text_lines),
        footer_text_lines: stripUids(draft.footer_text_lines),
        show_page_numbers: draft.show_page_numbers,
        show_generated_by: draft.show_generated_by,
      });
      await mutate();
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("workspace_settings.settings.document_export.saved") });
    } catch {
      showError();
    } finally {
      setIsSaving(false);
    }
  };

  const handleLogoUpload = async (file: File) => {
    try {
      await fileService.uploadWorkspaceAsset(
        workspaceSlug,
        { entity_identifier: draft.id ?? workspaceSlug, entity_type: EFileAssetType.DOC_HEADER_LOGO },
        file
      );
      await mutate();
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("workspace_settings.settings.document_export.saved") });
    } catch {
      showError();
    }
  };

  const renderLines = (key: "header_text_lines" | "footer_text_lines", label: string) => (
    <div className="space-y-2">
      <h6 className="text-13 font-medium text-secondary">{label}</h6>
      <p className="text-11 text-tertiary">{t("workspace_settings.settings.document_export.limit_hint")}</p>
      {draft[key].map((line) => (
        <div key={line.uid} className="flex items-center gap-2">
          <input
            className="w-full rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-13"
            disabled={!isAdmin}
            maxLength={MAX_LINE_LEN}
            placeholder={t("workspace_settings.settings.document_export.line_placeholder")}
            value={line.text}
            onChange={(event) =>
              updateLines(key, (lines) =>
                lines.map((item) => (item.uid === line.uid ? { ...item, text: event.target.value } : item))
              )
            }
          />
          <select
            className="rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-13"
            disabled={!isAdmin}
            value={line.align}
            onChange={(event) =>
              updateLines(key, (lines) =>
                lines.map((item) =>
                  item.uid === line.uid ? { ...item, align: event.target.value as TDocumentExportTextAlign } : item
                )
              )
            }
          >
            <option value="left">{t("workspace_settings.settings.document_export.align_left")}</option>
            <option value="center">{t("workspace_settings.settings.document_export.align_center")}</option>
            <option value="right">{t("workspace_settings.settings.document_export.align_right")}</option>
          </select>
          <button
            type="button"
            className="shrink-0 text-13 text-danger-primary disabled:opacity-50"
            disabled={!isAdmin}
            onClick={() => updateLines(key, (lines) => lines.filter((item) => item.uid !== line.uid))}
          >
            {t("workspace_settings.settings.document_export.remove_line")}
          </button>
        </div>
      ))}
      <Button
        variant="secondary"
        size="sm"
        disabled={!isAdmin || draft[key].length >= MAX_LINES}
        onClick={() =>
          updateLines(key, (lines) => [...lines, { text: "", align: "left", uid: `${Date.now()}-${lines.length}` }])
        }
      >
        {t("workspace_settings.settings.document_export.add_line")}
      </Button>
      <p className="text-11 text-tertiary">{t("workspace_settings.settings.document_export.placeholders")}</p>
    </div>
  );

  return (
    <div className="flex flex-col gap-8">
      <div className="space-y-2">
        <h4 className="text-16 font-medium text-primary">{t("workspace_settings.settings.document_export.heading")}</h4>
        <p className="text-13 text-tertiary">{t("workspace_settings.settings.document_export.description")}</p>
      </div>

      <div className="space-y-2">
        <h6 className="text-13 font-medium text-secondary">{t("workspace_settings.settings.document_export.logo")}</h6>
        <div className="flex flex-wrap items-center gap-3">
          {(["none", "workspace", "custom"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              disabled={!isAdmin}
              onClick={() => setDraft({ ...draft, header_logo_mode: mode })}
              className={cn("rounded-md border px-3 py-1.5 text-13", {
                "border-accent-strong text-accent-primary": draft.header_logo_mode === mode,
                "border-subtle text-secondary": draft.header_logo_mode !== mode,
              })}
            >
              {t(`workspace_settings.settings.document_export.logo_mode_${mode}`)}
            </button>
          ))}
          {draft.header_logo_mode === "custom" && (
            <label className="cursor-pointer text-13 text-accent-primary">
              {t("workspace_settings.settings.document_export.upload_logo")}
              <input
                accept="image/png,image/jpeg,image/webp,image/gif"
                className="hidden"
                disabled={!isAdmin}
                type="file"
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file) void handleLogoUpload(file);
                }}
              />
            </label>
          )}
        </div>
      </div>

      {renderLines("header_text_lines", t("workspace_settings.settings.document_export.header_lines"))}
      {renderLines("footer_text_lines", t("workspace_settings.settings.document_export.footer_lines"))}

      <div className="space-y-2">
        <label className="flex items-center gap-2 text-13 text-secondary">
          <input
            checked={draft.show_page_numbers}
            disabled={!isAdmin}
            type="checkbox"
            onChange={(event) => setDraft({ ...draft, show_page_numbers: event.target.checked })}
          />
          {t("workspace_settings.settings.document_export.show_page_numbers")}
        </label>
        <label className="flex items-center gap-2 text-13 text-secondary">
          <input
            checked={draft.show_generated_by}
            disabled={!isAdmin}
            type="checkbox"
            onChange={(event) => setDraft({ ...draft, show_generated_by: event.target.checked })}
          />
          {t("workspace_settings.settings.document_export.show_generated_by")}
        </label>
      </div>

      {isAdmin && (
        <div>
          <Button variant="primary" size="lg" loading={isSaving} onClick={handleSave}>
            {t("workspace_settings.settings.document_export.save")}
          </Button>
        </div>
      )}
    </div>
  );
});
