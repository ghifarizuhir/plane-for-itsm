/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { AlignmentType, Document, Footer, Header, ImageRun, Packer, PageNumber, Paragraph, TextRun } from "docx";
import type { TDocumentExportTextAlign, TDocumentExportTextLine } from "@plane/types";
import type { TDocxImage, TDocumentBlock } from "./html-to-docx";

const ALIGNMENT_MAP: Record<TDocumentExportTextAlign, (typeof AlignmentType)[keyof typeof AlignmentType]> = {
  left: AlignmentType.LEFT,
  center: AlignmentType.CENTER,
  right: AlignmentType.RIGHT,
};

const LOGO_MAX_WIDTH = 160;
const LOGO_MAX_HEIGHT = 48;

const fitLogo = (image: TDocxImage) => {
  const ratio = Math.min(LOGO_MAX_WIDTH / image.width, LOGO_MAX_HEIGHT / image.height, 1);
  return { width: Math.round(image.width * ratio), height: Math.round(image.height * ratio) };
};

export type TPackDocumentInput = {
  title: string;
  creator: string;
  children: TDocumentBlock[];
  headerLines: TDocumentExportTextLine[];
  footerLines: TDocumentExportTextLine[];
  showPageNumbers: boolean;
  generatedByLine: string;
  logo: TDocxImage | null;
  numbering: unknown[];
};

export async function packDocument(input: TPackDocumentInput): Promise<Blob> {
  const headerChildren: Paragraph[] = [];
  if (input.logo) {
    headerChildren.push(
      new Paragraph({
        alignment: AlignmentType.CENTER,
        children: [new ImageRun({ type: input.logo.type, data: input.logo.data, transformation: fitLogo(input.logo) })],
        spacing: { after: 80 },
      })
    );
  }
  for (const line of input.headerLines) {
    if (!line.text.trim()) continue;
    headerChildren.push(
      new Paragraph({
        alignment: ALIGNMENT_MAP[line.align] ?? AlignmentType.LEFT,
        children: [new TextRun({ text: line.text, size: 18, color: "404040" })],
        spacing: { after: 40 },
      })
    );
  }

  const footerChildren: Paragraph[] = [];
  for (const line of input.footerLines) {
    if (!line.text.trim()) continue;
    footerChildren.push(
      new Paragraph({
        alignment: ALIGNMENT_MAP[line.align] ?? AlignmentType.LEFT,
        children: [new TextRun({ text: line.text, size: 16, color: "595959" })],
        spacing: { after: 20 },
      })
    );
  }
  if (input.showPageNumbers) {
    footerChildren.push(
      new Paragraph({
        alignment: AlignmentType.CENTER,
        children: [
          new TextRun({
            size: 16,
            color: "595959",
            children: ["Page ", PageNumber.CURRENT, " of ", PageNumber.TOTAL_PAGES],
          }),
        ],
      })
    );
  }
  if (input.generatedByLine) {
    footerChildren.push(
      new Paragraph({
        alignment: AlignmentType.CENTER,
        children: [new TextRun({ text: input.generatedByLine, size: 16, color: "808080" })],
      })
    );
  }

  const document = new Document({
    creator: input.creator,
    title: input.title,
    numbering: { config: input.numbering as never },
    sections: [
      {
        properties: { page: { margin: { top: 1440, bottom: 1440, left: 1080, right: 1080 } } },
        headers: headerChildren.length ? { default: new Header({ children: headerChildren }) } : undefined,
        footers: footerChildren.length ? { default: new Footer({ children: footerChildren }) } : undefined,
        children: input.children,
      },
    ],
  });

  return Packer.toBlob(document);
}
