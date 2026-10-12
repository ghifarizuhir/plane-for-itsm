/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { HeadingLevel, Paragraph, TextRun } from "docx";
import type { TDocumentExportTextLine } from "@plane/types";
import {
  DOCX_NUMBERING_CONFIG,
  htmlToDocxBlocks,
  type TDocxImage,
  type TDocumentBlock,
  type TImageFetcher,
} from "./html-to-docx";
import { packDocument } from "./shell";

export type TPageDocSubPage = {
  name: string;
  bodyHtml: string;
  level: number;
};

export type TPageDocLabels = {
  properties: string;
  subPages: string;
  generatedBy: string;
  noValue: string;
};

export type TPageDocInput = {
  title: string;
  bodyHtml: string;
  ownerName?: string;
  createdBy?: string;
  createdAt?: string;
  updatedBy?: string;
  updatedAt?: string;
  labelNames: string[];
  projectName?: string;
  versionCount?: number;
  subPages: TPageDocSubPage[];
  headerLines: TDocumentExportTextLine[];
  footerLines: TDocumentExportTextLine[];
  showPageNumbers: boolean;
  generatedByLine: string;
  creator: string;
  logo: TDocxImage | null;
  labels: TPageDocLabels;
  imageFetcher?: TImageFetcher;
};

const formatDate = (value?: string) => {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(0, 10);
};

const propertyLine = (key: string, value?: string) => {
  if (!value?.trim()) return undefined;
  return new Paragraph({
    children: [new TextRun({ text: `${key}: `, bold: true, size: 20 }), new TextRun({ text: value, size: 20 })],
    spacing: { after: 40 },
  });
};

export async function buildPageDocument(input: TPageDocInput): Promise<Blob> {
  const children: TDocumentBlock[] = [
    new Paragraph({ heading: HeadingLevel.HEADING_1, children: [new TextRun({ text: input.title })] }),
    new Paragraph({
      heading: HeadingLevel.HEADING_2,
      children: [new TextRun({ text: input.labels.properties })],
      spacing: { before: 240, after: 120 },
    }),
  ];
  for (const line of [
    propertyLine("Owner", input.ownerName),
    propertyLine("Created", [input.createdBy, formatDate(input.createdAt)].filter(Boolean).join(" · ")),
    propertyLine("Last updated", [input.updatedBy, formatDate(input.updatedAt)].filter(Boolean).join(" · ")),
    propertyLine("Labels", input.labelNames.join(", ")),
    propertyLine("Project", input.projectName),
    propertyLine("Versions", input.versionCount !== undefined ? String(input.versionCount) : undefined),
  ]) {
    if (line) children.push(line);
  }

  if (input.bodyHtml?.trim()) {
    children.push(...(await htmlToDocxBlocks(input.bodyHtml, { imageFetcher: input.imageFetcher })));
  }

  if (input.subPages.length) {
    children.push(
      new Paragraph({
        heading: HeadingLevel.HEADING_2,
        children: [new TextRun({ text: input.labels.subPages })],
        spacing: { before: 240, after: 120 },
      })
    );
    const counters: (number | undefined)[] = [];
    for (const subPage of input.subPages) {
      counters.length = subPage.level;
      counters[subPage.level - 1] = (counters[subPage.level - 1] ?? 0) + 1;
      const number = counters.map((value) => value ?? 1).join(".");
      const headingLevel = subPage.level === 1 ? HeadingLevel.HEADING_3 : HeadingLevel.HEADING_4;
      children.push(
        new Paragraph({
          heading: headingLevel,
          children: [new TextRun({ text: `${number} ${subPage.name}` })],
          spacing: { before: 200, after: 80 },
        })
      );
      if (subPage.bodyHtml?.trim()) {
        // oxlint-disable-next-line no-await-in-loop -- sections must keep tree order
        children.push(...(await htmlToDocxBlocks(subPage.bodyHtml, { imageFetcher: input.imageFetcher })));
      }
    }
  }

  return packDocument({
    title: input.title,
    creator: input.creator,
    children,
    headerLines: input.headerLines,
    footerLines: input.footerLines,
    showPageNumbers: input.showPageNumbers,
    generatedByLine: input.generatedByLine,
    logo: input.logo,
    numbering: DOCX_NUMBERING_CONFIG,
  });
}
