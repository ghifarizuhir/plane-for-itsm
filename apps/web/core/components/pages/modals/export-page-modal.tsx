/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import type { PageProps } from "@react-pdf/renderer";
import { pdf } from "@react-pdf/renderer";
import { Controller, useForm } from "react-hook-form";
import { useParams } from "react-router";
// plane i18n
import { useTranslation } from "@plane/i18n";
// plane editor
import type { EditorRefApi } from "@plane/editor";
// plane ui
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { CustomSelect, EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { buildPageDocument } from "@/components/document-export/build-page-doc";
import { documentFileName, initiateDownload } from "@/components/document-export/download";
import { createDocumentImageFetcher } from "@/components/document-export/image-fetcher";
import { resolveDocumentLogo } from "@/components/document-export/logo";
import { resolvePlaceholders } from "@/components/document-export/resolve-placeholders";
import { PDFDocument } from "@/components/editor/pdf";
// hooks
import { useParseEditorContent } from "@/hooks/use-parse-editor-content";
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useUser } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
// services
import { DocumentExportSettingsService } from "@/services/document-export/document-export-settings.service";
import { ProjectPageService } from "@/services/page/project-page.service";
import { ProjectPageVersionService } from "@/services/page/project-page-version.service";

type Props = {
  editorRef: EditorRefApi | null;
  isOpen: boolean;
  onClose: () => void;
  pageTitle: string;
};

type TExportFormats = "pdf" | "markdown" | "docx";
type TPageFormats = Exclude<PageProps["size"], undefined>;
type TContentVariety = "everything" | "no-assets";

type TFormValues = {
  export_format: TExportFormats;
  page_format: TPageFormats;
  content_variety: TContentVariety;
};

const EXPORT_FORMATS: {
  key: TExportFormats;
  label: string;
}[] = [
  {
    key: "pdf",
    label: "PDF",
  },
  {
    key: "markdown",
    label: "Markdown",
  },
  {
    key: "docx",
    label: "DOCX (Formal document)",
  },
];

const PAGE_FORMATS: {
  key: TPageFormats;
  label: string;
}[] = [
  {
    key: "A4",
    label: "A4",
  },
  {
    key: "A3",
    label: "A3",
  },
  {
    key: "A2",
    label: "A2",
  },
  {
    key: "LETTER",
    label: "Letter",
  },
  {
    key: "LEGAL",
    label: "Legal",
  },
  {
    key: "TABLOID",
    label: "Tabloid",
  },
];

const CONTENT_VARIETY: {
  key: TContentVariety;
  label: string;
}[] = [
  {
    key: "everything",
    label: "Everything",
  },
  {
    key: "no-assets",
    label: "No images",
  },
];

const defaultValues: TFormValues = {
  export_format: "pdf",
  page_format: "A4",
  content_variety: "everything",
};

export function ExportPageModal(props: Props) {
  const { editorRef, isOpen, onClose, pageTitle } = props;
  // translation
  const { t } = useTranslation();
  // states
  const [isExporting, setIsExporting] = useState(false);
  // params
  const { workspaceSlug, projectId, pageId } = useParams();
  // store hooks
  const { currentWorkspace } = useWorkspace();
  const { getProjectById } = useProject();
  const { memberMap } = useMember();
  const { getLabelById } = useLabel();
  const { data: currentUser } = useUser();
  // form info
  const { control, reset, watch } = useForm<TFormValues>({
    defaultValues,
  });
  // parse editor content
  const { replaceCustomComponentsFromHTMLContent, replaceCustomComponentsFromMarkdownContent } = useParseEditorContent({
    projectId,
    workspaceSlug: workspaceSlug ?? "",
  });
  // derived values
  const selectedExportFormat = watch("export_format");
  const selectedPageFormat = watch("page_format");
  const selectedContentVariety = watch("content_variety");
  const isPDFSelected = selectedExportFormat === "pdf";
  const fileName = pageTitle
    ?.toLowerCase()
    ?.replace(/[^a-z0-9-_]/g, "-")
    .replace(/-+/g, "-");
  // handle modal close
  const handleClose = () => {
    onClose();
    setTimeout(() => {
      reset();
    }, 300);
  };

  // handle export as a PDF
  const handleExportAsPDF = async () => {
    try {
      const pageContent = `<h1 class="page-title">${pageTitle}</h1>${editorRef?.getDocument().html ?? "<p></p>"}`;
      const parsedPageContent = await replaceCustomComponentsFromHTMLContent({
        htmlContent: pageContent,
        noAssets: selectedContentVariety === "no-assets",
      });

      const blob = await pdf(<PDFDocument content={parsedPageContent} pageFormat={selectedPageFormat} />).toBlob();
      initiateDownload(blob, `${fileName}-${selectedPageFormat.toString().toLowerCase()}.pdf`);
    } catch (error) {
      throw new Error("Error in exporting as a PDF", { cause: error });
    }
  };
  // handle export as markdown
  const handleExportAsMarkdown = async () => {
    try {
      const markdownContent = editorRef?.getMarkDown() ?? "";
      const parsedMarkdownContent = replaceCustomComponentsFromMarkdownContent({
        markdownContent,
        noAssets: selectedContentVariety === "no-assets",
      });

      const blob = new Blob([parsedMarkdownContent], { type: "text/markdown" });
      initiateDownload(blob, `${fileName}.md`);
    } catch (error) {
      throw new Error("Error in exporting as markdown", { cause: error });
    }
  };
  // handle export as a formal DOCX document
  const handleExportAsDocx = async () => {
    if (!workspaceSlug || !projectId) return;
    const slug = workspaceSlug.toString();
    const projectIdString = projectId.toString();
    const pageIdString = pageId?.toString();
    try {
      const settingsService = new DocumentExportSettingsService();
      const pageService = new ProjectPageService();
      const versionService = new ProjectPageVersionService();
      const settings = await settingsService.fetchSettings(slug).catch(() => null);
      const resolvedSettings = settings ?? {
        header_logo_mode: "none" as const,
        header_logo_url: null,
        header_logo_asset_id: null,
        header_text_lines: [],
        footer_text_lines: [],
        show_page_numbers: true,
        show_generated_by: true,
      };
      const exportedByName = currentUser?.display_name ?? "";
      const today = new Date().toISOString().slice(0, 10);
      const placeholderValues = {
        workspace_name: currentWorkspace?.name ?? "",
        document_title: pageTitle,
        document_id: pageIdString ? pageIdString.slice(0, 8) : "",
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

      const detail = pageIdString
        ? await pageService.fetchById(slug, projectIdString, pageIdString, false).catch(() => null)
        : null;
      const subPages: { name: string; bodyHtml: string; level: number }[] = [];
      const collectChildren = async (parentId: string, level: number): Promise<void> => {
        if (level > 10 || subPages.length >= 50) return;
        const children = await pageService.fetchAll(slug, projectIdString, parentId).catch(() => []);
        for (const child of children) {
          if (subPages.length >= 50 || !child.id) break;
          // oxlint-disable-next-line no-await-in-loop -- sections must keep tree order
          const childDetail = await pageService.fetchById(slug, projectIdString, child.id, false).catch(() => null);
          subPages.push({ name: child.name ?? "", bodyHtml: childDetail?.description_html ?? "", level });
          // oxlint-disable-next-line no-await-in-loop -- sections must keep tree order
          await collectChildren(child.id, level + 1);
        }
      };
      if (pageIdString) await collectChildren(pageIdString, 1);

      const versions = pageIdString
        ? await versionService.fetchAllVersions(slug, projectIdString, pageIdString).catch(() => [])
        : [];
      const logo = await resolveDocumentLogo(
        resolvedSettings,
        currentWorkspace,
        createDocumentImageFetcher({ workspaceSlug: slug })
      );

      const blob = await buildPageDocument({
        title: pageTitle,
        bodyHtml: `<h1 class="page-title">${pageTitle}</h1>${editorRef?.getDocument().html ?? "<p></p>"}`,
        ownerName: memberMap?.[detail?.owned_by ?? ""]?.display_name,
        createdBy: memberMap?.[detail?.created_by ?? ""]?.display_name,
        createdAt: detail?.created_at ? new Date(detail.created_at).toISOString() : undefined,
        updatedBy: memberMap?.[detail?.updated_by ?? ""]?.display_name,
        updatedAt: detail?.updated_at ? new Date(detail.updated_at).toISOString() : undefined,
        labelNames: (detail?.label_ids ?? []).map((id) => getLabelById(id)?.name ?? id),
        projectName: getProjectById(projectIdString)?.name,
        versionCount: versions.length || undefined,
        subPages,
        imageFetcher: createDocumentImageFetcher({ workspaceSlug: slug, projectId: projectIdString }),
        imageUnavailableText: t("document_export.doc.image_unavailable"),
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
          subPages: t("document_export.doc.sub_pages"),
          generatedBy: t("document_export.doc.generated_by"),
          noValue: t("document_export.doc.no_value"),
        },
      });
      initiateDownload(blob, documentFileName("article", pageTitle));
    } catch (error) {
      throw new Error("Error in exporting as a DOCX", { cause: error });
    }
  };
  // handle export
  const handleExport = async () => {
    setIsExporting(true);
    try {
      if (selectedExportFormat === "pdf") {
        await handleExportAsPDF();
      }
      if (selectedExportFormat === "markdown") {
        await handleExportAsMarkdown();
      }
      if (selectedExportFormat === "docx") {
        await handleExportAsDocx();
      }
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: "Success!",
        message: "Page exported successfully.",
      });
      handleClose();
    } catch (error) {
      console.error("Error in exporting page:", error);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: "Page could not be exported. Please try again later.",
      });
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={handleClose} position={EModalPosition.CENTER} width={EModalWidth.SM}>
      <div data-prevent-outside-click>
        <div className="space-y-5 p-5">
          <h3 className="text-18 font-medium text-secondary">Export page</h3>
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
              <h6 className="flex-shrink-0 text-13 text-secondary">Export format</h6>
              <Controller
                control={control}
                name="export_format"
                render={({ field: { onChange, value } }) => (
                  <CustomSelect
                    label={EXPORT_FORMATS.find((format) => format.key === value)?.label}
                    buttonClassName="border-none"
                    value={value}
                    onChange={(val: TExportFormats) => onChange(val)}
                    className="flex-shrink-0"
                    placement="bottom-end"
                  >
                    {EXPORT_FORMATS.map((format) => (
                      <CustomSelect.Option key={format.key} value={format.key}>
                        {format.label}
                      </CustomSelect.Option>
                    ))}
                  </CustomSelect>
                )}
              />
            </div>
            <div className="flex items-center justify-between gap-2">
              <h6 className="flex-shrink-0 text-13 text-secondary">Include content</h6>
              <Controller
                control={control}
                name="content_variety"
                render={({ field: { onChange, value } }) => (
                  <CustomSelect
                    label={CONTENT_VARIETY.find((variety) => variety.key === value)?.label}
                    buttonClassName="border-none"
                    value={value}
                    onChange={(val: TContentVariety) => onChange(val)}
                    className="flex-shrink-0"
                    placement="bottom-end"
                  >
                    {CONTENT_VARIETY.map((variety) => (
                      <CustomSelect.Option key={variety.key} value={variety.key}>
                        {variety.label}
                      </CustomSelect.Option>
                    ))}
                  </CustomSelect>
                )}
              />
            </div>
            {isPDFSelected && (
              <div className="flex items-center justify-between gap-2">
                <h6 className="flex-shrink-0 text-13 text-secondary">Page format</h6>
                <Controller
                  control={control}
                  name="page_format"
                  render={({ field: { onChange, value } }) => (
                    <CustomSelect
                      label={PAGE_FORMATS.find((format) => format.key === value)?.label}
                      buttonClassName="border-none"
                      value={value}
                      onChange={(val: TPageFormats) => onChange(val)}
                      className="flex-shrink-0"
                      placement="bottom-end"
                    >
                      {PAGE_FORMATS.map((format) => (
                        <CustomSelect.Option key={format.key.toString()} value={format.key}>
                          {format.label}
                        </CustomSelect.Option>
                      ))}
                    </CustomSelect>
                  )}
                />
              </div>
            )}
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
          <Button variant="secondary" size="lg" onClick={handleClose}>
            Cancel
          </Button>
          <Button variant="primary" size="lg" loading={isExporting} onClick={handleExport}>
            {isExporting ? "Exporting" : "Export"}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
}
