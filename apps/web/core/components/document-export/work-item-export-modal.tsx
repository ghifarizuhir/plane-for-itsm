/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TIssue, TIssueActivity, TIssueRelationTypes } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { buildWorkItemDocument } from "@/components/document-export/build-work-item-doc";
import { documentFileName, initiateDownload } from "@/components/document-export/download";
import { createDocumentImageFetcher } from "@/components/document-export/image-fetcher";
import { resolveDocumentLogo } from "@/components/document-export/logo";
import { milestoneRows } from "@/components/document-export/milestones";
import { resolvePlaceholders } from "@/components/document-export/resolve-placeholders";
// hooks
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useProjectState } from "@/hooks/store/use-project-state";
import { useUser } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
// services
import { DocumentExportSettingsService } from "@/services/document-export/document-export-settings.service";
import { IssueActivityService } from "@/services/issue/issue_activity.service";
import { IssueRelationService } from "@/services/issue/issue_relation.service";

type Props = {
  issue: TIssue;
  isOpen: boolean;
  onClose: () => void;
};

const RELATION_LABEL_KEYS: Record<TIssueRelationTypes, string> = {
  blocking: "document_export.doc.relation_blocking",
  blocked_by: "document_export.doc.relation_blocked_by",
  relates_to: "document_export.doc.relation_relates_to",
  duplicate: "document_export.doc.relation_duplicate",
};

const DEFAULT_SETTINGS = {
  header_logo_mode: "none" as const,
  header_logo_url: null,
  header_logo_asset_id: null,
  header_text_lines: [] as { text: string; align: "left" | "center" | "right" }[],
  footer_text_lines: [] as { text: string; align: "left" | "center" | "right" }[],
  show_page_numbers: true,
  show_generated_by: true,
};

export const WorkItemExportModal = observer(function WorkItemExportModal({ issue, isOpen, onClose }: Props) {
  // translation
  const { t } = useTranslation();
  // router
  const { workspaceSlug } = useParams();
  // store hooks
  const { currentWorkspace } = useWorkspace();
  const { getStateById } = useProjectState();
  const { getProjectIdentifierById, getProjectById } = useProject();
  const { memberMap } = useMember();
  const { getLabelById } = useLabel();
  const { workItemTypes } = useWorkItemType();
  const { data: currentUser } = useUser();
  // states
  const [includeHistory, setIncludeHistory] = useState(true);
  const [isExporting, setIsExporting] = useState(false);
  // services
  const settingsService = useMemo(() => new DocumentExportSettingsService(), []);
  const activityService = useMemo(() => new IssueActivityService(), []);
  const relationService = useMemo(() => new IssueRelationService(), []);

  const handleExport = async () => {
    // Prefer the route param, but fall back to the workspace store: the
    // modal can be opened from peek overview on routes without workspaceSlug
    // (e.g. global search), where useParams() has no workspaceSlug.
    const slug = workspaceSlug?.toString() ?? currentWorkspace?.slug;
    if (!slug) {
      setToast({ type: TOAST_TYPE.ERROR, title: t("document_export.modal.error") });
      return;
    }
    setIsExporting(true);
    try {
      const [settings, activities, relations] = await Promise.all([
        settingsService.fetchSettings(slug).catch(() => null),
        activityService.getIssueActivities(slug, issue.project_id ?? "", issue.id).catch(() => []),
        issue.project_id
          ? relationService.listIssueRelations(slug, issue.project_id, issue.id).catch(() => null)
          : Promise.resolve(null),
      ]);

      const resolvedSettings = settings ?? { ...DEFAULT_SETTINGS };
      const identifier = `${getProjectIdentifierById(issue.project_id)}-${issue.sequence_id}`;
      const exportedByName = currentUser?.display_name ?? "";
      const today = new Date().toISOString().slice(0, 10);
      const placeholderValues = {
        workspace_name: currentWorkspace?.name ?? "",
        document_title: issue.name,
        document_id: identifier,
        export_date: today,
        exported_by: exportedByName,
      };
      const headerLines = resolvedSettings.header_text_lines.map((line) => ({
        ...line,
        text: resolvePlaceholders(line.text, placeholderValues),
      }));
      const footerLines = resolvedSettings.footer_text_lines.map((line) => ({
        ...line,
        text: resolvePlaceholders(line.text, placeholderValues),
      }));
      const milestones = milestoneRows(
        activities as TIssueActivity[],
        {
          stateName: (id) => getStateById(id)?.name,
          memberName: (id) => memberMap?.[id]?.display_name,
        },
        {
          created: t("document_export.doc.event_created"),
          state: t("document_export.doc.event_state"),
          priority: t("document_export.doc.event_priority"),
          assignees: t("document_export.doc.event_assignees"),
          target_date: t("document_export.doc.event_target_date"),
          start_date: t("document_export.doc.event_start_date"),
          archived_at: t("document_export.doc.event_archived_at"),
          noValue: t("document_export.doc.no_value"),
        }
      );
      const linkedItems = Object.entries(relations ?? {}).flatMap(([relationType, related]) =>
        (related as TIssue[]).map((item) => ({
          identifier: `${getProjectIdentifierById(item.project_id)}-${item.sequence_id}`,
          title: item.name,
          relationLabel: t(RELATION_LABEL_KEYS[relationType as TIssueRelationTypes]),
          stateName: getStateById(item.state_id)?.name,
        }))
      );
      const documentImageFetcher = createDocumentImageFetcher({
        workspaceSlug: slug,
        projectId: issue.project_id ?? undefined,
      });
      const logoImageFetcher = createDocumentImageFetcher({ workspaceSlug: slug });
      const logo = await resolveDocumentLogo(resolvedSettings, currentWorkspace, logoImageFetcher);

      const blob = await buildWorkItemDocument({
        title: `${identifier} — ${issue.name}`,
        projectName: getProjectById(issue.project_id)?.name ?? "",
        stateName: getStateById(issue.state_id)?.name,
        typeName: workItemTypes?.find((type) => type.id === issue.type_id)?.name,
        priority: issue.priority,
        assigneeNames: (issue.assignee_ids ?? []).map((id) => memberMap?.[id]?.display_name ?? id),
        labelNames: (issue.label_ids ?? []).map((id) => getLabelById(id)?.name ?? id),
        startDate: issue.start_date,
        targetDate: issue.target_date,
        createdBy: memberMap?.[issue.created_by ?? ""]?.display_name,
        createdAt: issue.created_at,
        updatedBy: memberMap?.[issue.updated_by ?? ""]?.display_name,
        updatedAt: issue.updated_at,
        descriptionHtml: issue.description_html ?? "",
        imageFetcher: documentImageFetcher,
        imageUnavailableText: t("document_export.doc.image_unavailable"),
        linkedItems,
        milestones,
        includeHistory,
        headerLines,
        footerLines,
        showPageNumbers: resolvedSettings.show_page_numbers,
        generatedByLine: resolvedSettings.show_generated_by
          ? t("document_export.doc.generated_by", { user: exportedByName, date: today })
          : "",
        creator: exportedByName,
        logo,
        labels: {
          properties: t("document_export.doc.properties"),
          description: t("document_export.doc.description"),
          linked: t("document_export.doc.linked_work_items"),
          history: t("document_export.doc.history"),
          generatedBy: t("document_export.doc.generated_by"),
          noValue: t("document_export.doc.no_value"),
          columnWhen: t("document_export.doc.column_when"),
          columnActor: t("document_export.doc.column_actor"),
          columnEvent: t("document_export.doc.column_event"),
          columnChange: t("document_export.doc.column_change"),
        },
        workspaceName: currentWorkspace?.name ?? "",
      });

      initiateDownload(blob, documentFileName(identifier, issue.name));
      setToast({ type: TOAST_TYPE.SUCCESS, title: t("document_export.modal.success") });
      onClose();
    } catch (error) {
      console.error("Error exporting work item document:", error);
      setToast({ type: TOAST_TYPE.ERROR, title: t("document_export.modal.error") });
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.CENTER} width={EModalWidth.SM}>
      <div data-prevent-outside-click>
        <div className="space-y-5 p-5">
          <h3 className="text-18 font-medium text-secondary">{t("document_export.modal.title")}</h3>
          <p className="text-13 text-tertiary">{t("document_export.modal.description")}</p>
          <label className="flex items-center gap-2 text-13 text-secondary">
            <input
              type="checkbox"
              checked={includeHistory}
              onChange={(event) => setIncludeHistory(event.target.checked)}
            />
            {t("document_export.modal.include_history")}
          </label>
        </div>
        <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isExporting} onClick={handleExport}>
            {isExporting ? t("document_export.modal.exporting") : t("document_export.modal.export")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
